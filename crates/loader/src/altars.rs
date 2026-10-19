//! Flame altar zones on the server.
//!
//! Two hooks and a solver:
//!
//! - After `add_buildzones_server` runs, its query is walked again to learn
//!   every altar's position and orientation.
//! - `upgrade_building_range` is replaced by the same loop vanilla runs, on
//!   the same engine helpers, except that the range it writes comes from the
//!   cluster solution when there is one. It also learns every altar's level.
//! - When positions or levels change, a background thread solves the
//!   clusters; the game thread never waits for it and keeps writing the last
//!   solution, or vanilla sizes, meanwhile.
//!
//! Anything unexpected turns the feature off and hands the system back to
//! vanilla for good; it never takes the server down.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use zone_geometry::{Altar, clusters};

use crate::builds::{Build, engine};
use crate::ecs::{entity, row_counter};
use crate::zones::{self, Placed};
use crate::{info, warn};

type System = unsafe extern "system" fn(ctx: *mut u64);
type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;
type AttributeFind = unsafe extern "system" fn(out: *mut u64, attribute: u32, registry: u64) -> u64;
type ZoneSize = unsafe extern "system" fn(out: *mut f32, balancing: u64, level: u64) -> u64;
type MarkChanged = unsafe extern "system" fn(
    tracker: u64,
    stamp_a: u64,
    stamp_b: u64,
    entity: u64,
    kind: u16,
    flags: u8,
);

/// Engine entry points, as absolute addresses.
struct Engine {
    init: AtomicUsize,
    next: AtomicUsize,
    attribute_find: AtomicUsize,
    zone_size: AtomicUsize,
    mark_changed: AtomicUsize,
    original_upgrade: AtomicUsize,
    original_add_zones: AtomicUsize,
}

static ENGINE: Engine = Engine {
    init: AtomicUsize::new(0),
    next: AtomicUsize::new(0),
    attribute_find: AtomicUsize::new(0),
    zone_size: AtomicUsize::new(0),
    mark_changed: AtomicUsize::new(0),
    original_upgrade: AtomicUsize::new(0),
    original_add_zones: AtomicUsize::new(0),
};

/// Set once anything goes wrong; from then on both hooks run vanilla.
static OFF: AtomicBool = AtomicBool::new(false);

/// How long an altar stays known after a pass last saw it. The engine runs
/// a system once per batch of entities, several times a tick, so no single
/// pass sees every altar; one that no pass has seen for this long is gone.
const FORGET_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// What the passes have seen, merged across batches.
struct Seen<T> {
    entries: BTreeMap<u32, (T, std::time::Instant)>,
}

impl<T: PartialEq> Seen<T> {
    const fn new() -> Self {
        Seen {
            entries: BTreeMap::new(),
        }
    }

    /// Records one batch and forgets what nobody saw lately; true when the
    /// known values changed.
    fn merge(&mut self, batch: BTreeMap<u32, T>) -> bool {
        let now = std::time::Instant::now();
        let mut changed = false;
        for (id, value) in batch {
            match self.entries.get_mut(&id) {
                Some((known, seen)) => {
                    changed |= *known != value;
                    *known = value;
                    *seen = now;
                }
                None => {
                    changed = true;
                    self.entries.insert(id, (value, now));
                }
            }
        }
        let before = self.entries.len();
        self.entries
            .retain(|_, (_, seen)| now.duration_since(*seen) < FORGET_AFTER);
        changed || self.entries.len() != before
    }

    fn get(&self, id: &u32) -> Option<&T> {
        self.entries.get(id).map(|(value, _)| value)
    }

    fn iter(&self) -> impl Iterator<Item = (&u32, &T)> {
        self.entries.iter().map(|(id, (value, _))| (id, value))
    }
}

impl<T: PartialEq> Default for Seen<T> {
    fn default() -> Self {
        Seen::new()
    }
}

/// What the hooks have seen and what the solver produced.
#[derive(Default)]
struct State {
    placed: Seen<Placed>,
    /// Full zone sizes for levels 1 to the current one.
    levels: Seen<Vec<[f32; 3]>>,
    /// The input the current targets were solved for.
    solved: Option<Input>,
    solving: bool,
    /// The range to write, per altar, with the level it was solved at.
    targets: BTreeMap<u32, (usize, [f32; 3])>,
}

type Input = BTreeMap<u32, (Placed, Vec<[f32; 3]>)>;

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// add_buildzones_server's iterator: context, BaseIdComponent,
/// CurrentTransform, TriggerShape, BuildZoneSystem.
const ZONES_ITERATOR_WORDS: usize = 5;
const ZONES_TRANSFORM: usize = 2;
const ZONES_TRIGGER_SHAPE: usize = 3;

/// The ranges the zones were last built from, for the log only.
static BUILT: Mutex<Seen<[f32; 3]>> = Mutex::new(Seen::new());
/// upgrade_building_range's iterator: context, Level, TriggerShape, two
/// stamps, entity record, type and flags, tracker, BalancingRegistry,
/// AttributeRegistry.
const UPGRADE_ITERATOR_WORDS: usize = 10;
const UPGRADE_LEVEL: usize = 1;
const UPGRADE_TRIGGER_SHAPE: usize = 2;
const UPGRADE_STAMP_A: usize = 3;
const UPGRADE_STAMP_B: usize = 4;
const UPGRADE_ENTITY: usize = 5;
const UPGRADE_KIND: usize = 6;
const UPGRADE_TRACKER: usize = 7;
const UPGRADE_BALANCING: usize = 8;
const UPGRADE_ATTRIBUTES: usize = 9;
/// TriggerShape: half extents at 4.
const SHAPE_RANGE: usize = 4;
/// Vanilla writes half the level's size.
const HALF: f32 = 0.5;

pub(crate) fn install(
    build: &Build,
    base: usize,
    upgrade: usize,
    add_zones: usize,
) -> Result<(), String> {
    let at = |rva: u64| base + rva as usize;
    ENGINE
        .init
        .store(at(build.iterator_init), Ordering::Release);
    ENGINE
        .next
        .store(at(build.iterator_next), Ordering::Release);
    ENGINE
        .attribute_find
        .store(at(build.attribute_find), Ordering::Release);
    ENGINE
        .zone_size
        .store(at(build.zone_size), Ordering::Release);
    ENGINE
        .mark_changed
        .store(at(build.mark_changed), Ordering::Release);
    *STATE.lock().map_err(|_| "state poisoned")? = Some(State::default());

    let original = unsafe { crate::hook::install(add_zones, add_zones_detour as System as usize) }?;
    ENGINE.original_add_zones.store(original, Ordering::Release);
    let original = unsafe { crate::hook::install(upgrade, upgrade_detour as System as usize) }?;
    ENGINE.original_upgrade.store(original, Ordering::Release);
    Ok(())
}

fn turn_off(why: &str) {
    if !OFF.swap(true, Ordering::AcqRel) {
        warn!("altar zones off, vanilla from now on: {why}");
    }
}

unsafe fn function<T: Copy>(slot: &AtomicUsize) -> T {
    let address = slot.load(Ordering::Acquire);
    unsafe { std::mem::transmute_copy(&address) }
}

unsafe extern "system" fn add_zones_detour(ctx: *mut u64) {
    let original: System = unsafe { function(&ENGINE.original_add_zones) };
    unsafe { original(ctx) };
    if OFF.load(Ordering::Acquire) {
        return;
    }
    if std::panic::catch_unwind(|| unsafe { observe(ctx) }).is_err() {
        turn_off("the position pass panicked");
    }
}

unsafe extern "system" fn upgrade_detour(ctx: *mut u64) {
    if !OFF.load(Ordering::Acquire) {
        match std::panic::catch_unwind(|| unsafe { upgrade(ctx) }) {
            Ok(()) => return,
            Err(_) => turn_off("the range pass panicked"),
        }
        // Hand the tick to vanilla from the first row.
        unsafe { row_counter(ctx).write(0) };
    }
    let original: System = unsafe { function(&ENGINE.original_upgrade) };
    unsafe { original(ctx) };
}

unsafe fn read<T: Copy>(at: *const u8) -> T {
    unsafe { at.cast::<T>().read_unaligned() }
}

/// Learns positions and orientations, after vanilla built the zones.
unsafe fn observe(ctx: *mut u64) {
    let init: IteratorInit = unsafe { function(&ENGINE.init) };
    let next: IteratorNext = unsafe { function(&ENGINE.next) };
    let size = (ZONES_ITERATOR_WORDS * 8) as u64;
    let mut iterator = [0u64; ZONES_ITERATOR_WORDS];
    let mut placed = BTreeMap::new();
    let mut built = BTreeMap::new();
    unsafe {
        // Vanilla left the row counter at the end; walk the query again from
        // the start and leave the counter as it was.
        let row = row_counter(ctx);
        let saved = row.read();
        row.write(0);
        init(ctx, iterator.as_mut_ptr(), size);
        while next(ctx, iterator.as_mut_ptr(), size) & 0xff != 0 {
            let transform = iterator[ZONES_TRANSFORM] as *const u8;
            let id = entity(ctx);
            let shape = iterator[ZONES_TRIGGER_SHAPE] as *const u8;
            built.insert(id, read::<[f32; 3]>(shape.add(SHAPE_RANGE)));
            placed.insert(
                id,
                Placed {
                    position: read(transform),
                    orientation: read(transform.add(0x18)),
                },
            );
        }
        row.write(saved);
    }

    if let Ok(mut known) = BUILT.lock()
        && known.merge(built)
    {
        let all: BTreeMap<_, _> = known.iter().collect();
        info!("zones built from ranges {all:?}");
    }

    let Ok(mut slot) = STATE.lock() else { return };
    let Some(state) = slot.as_mut() else { return };
    if state.placed.merge(placed) {
        maybe_solve(state);
    }
}

/// Vanilla's upgrade_building_range, writing solved ranges where there are
/// any.
unsafe fn upgrade(ctx: *mut u64) {
    let init: IteratorInit = unsafe { function(&ENGINE.init) };
    let next: IteratorNext = unsafe { function(&ENGINE.next) };
    let find: AttributeFind = unsafe { function(&ENGINE.attribute_find) };
    let zone_size: ZoneSize = unsafe { function(&ENGINE.zone_size) };
    let mark: MarkChanged = unsafe { function(&ENGINE.mark_changed) };

    let targets = match STATE.lock() {
        Ok(slot) => slot.as_ref().map(|s| s.targets.clone()).unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    };
    let mut levels = BTreeMap::new();

    let size = (UPGRADE_ITERATOR_WORDS * 8) as u64;
    let mut it = [0u64; UPGRADE_ITERATOR_WORDS];
    unsafe {
        init(ctx, it.as_mut_ptr(), size);
        while next(ctx, it.as_mut_ptr(), size) & 0xff != 0 {
            let level_component = it[UPGRADE_LEVEL] as *const u8;
            if level_component.is_null()
                || read::<u8>(level_component.add(engine::LEVEL_FLAGS)) & 1 == 0
            {
                continue;
            }
            let mut found = [0u64; 2];
            find(
                found.as_mut_ptr(),
                engine::ALTAR_LEVEL_ATTRIBUTE,
                it[UPGRADE_ATTRIBUTES],
            );
            let definition = found[1] as *const u8;
            if found[0] & 0xff != 0
                || definition.is_null()
                || read::<u32>(definition) != read::<u32>(level_component)
            {
                continue;
            }
            let slot = read::<u16>(definition.add(engine::ATTRIBUTE_SLOT)) as usize;
            let data = read::<u16>(level_component.add(engine::LEVEL_DATA_OFFSET)) as usize;
            let level = read::<u8>(level_component.add(data + slot * 4)) as usize;

            // Sizes for every level up to this one, and vanilla's for this one
            // (level 0 has a zone of size 0).
            let size_of = |l: usize| {
                let mut out = [0f32; 4];
                zone_size(out.as_mut_ptr(), it[UPGRADE_BALANCING], l as u64);
                let valid = read::<u8>(out.as_ptr().cast::<u8>().add(0xc)) != 0;
                valid.then_some([out[0], out[1], out[2]])
            };
            let Some(current) = size_of(level) else {
                continue;
            };
            let sizes: Vec<[f32; 3]> = (1..=level).filter_map(size_of).collect();
            if sizes.len() != level {
                continue;
            }
            let id = entity(ctx);
            let target = match targets.get(&id) {
                Some(&(solved_level, range)) if solved_level == level => range,
                _ => current.map(|v| v * HALF),
            };
            levels.insert(id, sizes);

            let shape = it[UPGRADE_TRIGGER_SHAPE] as *mut u8;
            let range = shape.add(SHAPE_RANGE).cast::<[f32; 3]>();
            if range.read_unaligned() != target {
                if it[UPGRADE_TRACKER] != 0 {
                    let kind = it[UPGRADE_KIND];
                    mark(
                        it[UPGRADE_TRACKER],
                        it[UPGRADE_STAMP_A],
                        it[UPGRADE_STAMP_B],
                        it[UPGRADE_ENTITY],
                        kind as u16,
                        (kind >> 16) as u8,
                    );
                }
                range.write_unaligned(target);
            }
        }
    }

    let Ok(mut slot) = STATE.lock() else { return };
    let Some(state) = slot.as_mut() else { return };
    if state.levels.merge(levels) {
        maybe_solve(state);
    }
}

/// Starts the solver when the input changed and it is not already running.
fn maybe_solve(state: &mut State) {
    if state.solving {
        return;
    }
    let input: Input = state
        .placed
        .iter()
        .filter_map(|(&id, &placed)| Some((id, (placed, state.levels.get(&id)?.clone()))))
        .collect();
    if state.solved.as_ref() == Some(&input) {
        return;
    }
    let levels: Vec<(u32, usize)> = state.levels.iter().map(|(&id, s)| (id, s.len())).collect();
    let placed: Vec<(u32, [f64; 3])> = state
        .placed
        .iter()
        .map(|(&id, p)| (id, p.position.map(|v| v as f64 / 4294967296.0)))
        .collect();
    info!("altars placed {placed:.2?}, levelled {levels:?}");
    state.solving = true;
    std::thread::spawn(move || {
        let solved = std::panic::catch_unwind(|| solve(&input));
        let Ok(mut slot) = STATE.lock() else { return };
        let Some(state) = slot.as_mut() else { return };
        state.solving = false;
        match solved {
            Ok(targets) => {
                state.targets = targets;
                state.solved = Some(input);
                // The input may have moved on while this ran.
                maybe_solve(state);
            }
            Err(_) => turn_off("the solver panicked"),
        }
    });
}

/// The range each altar should have.
fn solve(input: &Input) -> BTreeMap<u32, (usize, [f32; 3])> {
    let started = std::time::Instant::now();
    let mut altars = Vec::new();
    let mut shapes = BTreeMap::new();
    for (&id, (placed, sizes)) in input {
        let (Some(turn), Some(centre)) = (zones::turn(placed.orientation), zones::centre(placed))
        else {
            warn!("altar {id} is tilted; it keeps its vanilla zone");
            continue;
        };
        if sizes.is_empty() {
            continue;
        }
        let levels = sizes
            .iter()
            .map(|s| zones::cells(centre, s.map(|v| v * HALF), turn))
            .collect();
        altars.push(Altar {
            key: id as u64,
            levels,
        });
        shapes.insert(
            id,
            (sizes.len(), sizes[sizes.len() - 1].map(|v| v * HALF), turn),
        );
    }

    let placements = match clusters(&altars) {
        Ok(p) => p,
        Err(e) => {
            warn!("cluster input rejected: {e:?}; altars keep vanilla zones");
            return BTreeMap::new();
        }
    };
    let mut targets = BTreeMap::new();
    for p in &placements {
        let id = p.key as u32;
        let Some(&(level, range, turn)) = shapes.get(&id) else {
            continue;
        };
        let height = p.zone.max[1] - p.zone.min[1];
        let target = zones::grown_range(range, turn, p.growth, height);
        info!(
            "altar {id} level {level}: cluster {}, growth {} cells, range {range:?} -> {target:?}",
            p.cluster, p.growth
        );
        targets.insert(id, (level, target));
    }
    info!(
        "solved {} altars in {:?}",
        placements.len(),
        started.elapsed()
    );
    targets
}
