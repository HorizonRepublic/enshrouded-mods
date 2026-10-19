//! Survivors on the server: a read-only probe of the survivor map, and a
//! copy of each survivor per altar.
//!
//! The base system keeps one map from a survivor's id to one entity, and
//! its callers decide what a survivor does from the answer. With
//! `[survivors] per_altar`, the detour on that lookup answers some callers
//! with the copy that belongs to the altar they work in (see `copies`):
//!
//! - `unique_npc_spawns` files each copy in its slot; a second copy in a
//!   slot, or a copy outside every zone, gets another entity as the answer,
//!   so vanilla destroys it;
//! - the spawn path is told the slot's copy, which vanilla then moves, or no
//!   copy, so it spawns one; the move command reaches every copy of a
//!   survivor, so the move system skips all but the target for a moment;
//! - `npc_bed` gets the copy of the bed's altar, so each copy keeps its bed;
//!   `npc_bed_init` and the cursor check get no copy while copies exist,
//!   which leaves the binding to `npc_bed` and skips a distance check.
//!
//! Callers are told apart by the lookup's return address, which a naked
//! stub hands over with the caller's stack, where the callers keep the
//! components of the entity they work on. Every other caller, and every
//! caller with the setting off, gets the vanilla answer.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::builds::Build;
use crate::copies::{Copies, Verdict};
use crate::{info, warn};

type Lookup = unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32;
type Move = unsafe extern "system" fn(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize;

static ORIGINAL_LOOKUP: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_CURSOR_LOOKUP: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_MOVE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE: AtomicUsize = AtomicUsize::new(0);

type Update =
    unsafe extern "system" fn(bases: *mut u8, npc: *const u32, state: usize, flag: usize) -> usize;

thread_local! {
    /// Set when the uniqueness pass keeps a copy that is not the record: the
    /// record update that follows on this thread must not take the copy's
    /// base and happiness.
    static SKIP_UPDATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
static ON: AtomicBool = AtomicBool::new(false);

/// Where the spawn path's lookup returns to. The stub compares against it,
/// so it lives on its own.
static SPAWN_RETURN: AtomicUsize = AtomicUsize::new(0);

struct Returns {
    assign: [usize; 2],
    roaming: usize,
    unique: [usize; 2],
    bed: usize,
    bed_init: usize,
    cursor: [usize; 2],
}

static RETURNS: OnceLock<Returns> = OnceLock::new();

type ComponentRead =
    unsafe extern "system" fn(ctx: *mut u64, kind: *const u16, entity: u32) -> *const u8;
static COMPONENT_READ: AtomicUsize = AtomicUsize::new(0);
/// The type index of `CurrentTransform`, learned from `npc_bed`, which has
/// both the bed and its transform; `u16::MAX` until then.
static TRANSFORM_TYPE: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(u16::MAX);
const TYPE_SEARCH: u16 = 4096;

struct Shared {
    copies: Copies,
    /// When the uniqueness pass last saw each copy.
    seen: BTreeMap<u32, Instant>,
    refreshed: Option<Instant>,
    /// The copy a summoning staff is moving, and since when.
    moving: Option<(u32, Instant)>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

const REFRESH: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);
const MOVE_WINDOW: Duration = Duration::from_secs(2);

/// Caller frames, from the caller's stack pointer at the call.
const UNIQUE_CONTEXT: usize = 0x38;
const UNIQUE_SAVED_STATE: usize = 0x40;
const UNIQUE_TRANSFORM: usize = 0x48;
const BED_CONTEXT: usize = 0x40;
const BED_TRANSFORM: usize = 0x48;
const ASSIGN_CONTEXT: usize = 0x30;
/// `player_assign_npc` keeps the player's targets at 0x40; the bed is the
/// selected target at 0x85c.
const ASSIGN_TARGETS: usize = 0x40;
const ASSIGN_BED: usize = 0x85c;
const ROAMING_CONTEXT: usize = 0x40;
/// The spawn request: the target transform, position first, at 0x18.
const SPAWN_TARGET: usize = 0x18;

pub(crate) fn install(build: &Build, base: usize, per_altar: bool) -> Result<(), String> {
    let at = |rva: u64| base + rva as usize;
    SPAWN_RETURN.store(at(build.npc_spawn_return), Ordering::Release);
    COMPONENT_READ.store(at(build.component_read), Ordering::Release);
    let _ = RETURNS.set(Returns {
        assign: build.npc_assign_returns.map(at),
        roaming: at(build.npc_roaming_return),
        unique: build.npc_unique_returns.map(at),
        bed: at(build.npc_bed_return),
        bed_init: at(build.npc_bed_init_return),
        cursor: build.npc_cursor_returns.map(at),
    });
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Shared {
            copies: Copies::default(),
            seen: BTreeMap::new(),
            refreshed: None,
            moving: None,
        });
    }

    let original =
        unsafe { crate::hook::install(at(build.npc_lookup), lookup_entry as Lookup as usize) }?;
    ORIGINAL_LOOKUP.store(original, Ordering::Release);
    if per_altar {
        let cursor = at(build.npc_lookup_cursor);
        let original = unsafe { crate::hook::install(cursor, cursor_entry as Lookup as usize) }?;
        ORIGINAL_CURSOR_LOOKUP.store(original, Ordering::Release);
        let original =
            unsafe { crate::hook::install(at(build.npc_move), move_detour as Move as usize) }?;
        ORIGINAL_MOVE.store(original, Ordering::Release);
        let original = unsafe {
            crate::hook::install(at(build.npc_update), update_detour as Update as usize)
        }?;
        ORIGINAL_UPDATE.store(original, Ordering::Release);
        ON.store(true, Ordering::Release);
    }
    Ok(())
}

/// Calls `lookup` with the return address and the caller's stack pointer,
/// or for the spawn path the spawn request in R14, and lets it return
/// straight to the caller.
#[unsafe(naked)]
unsafe extern "system" fn lookup_entry(_bases: *mut u8, _npc: *const u32) -> u32 {
    core::arch::naked_asm!(
        "mov r8, [rsp]",
        "lea r9, [rsp + 8]",
        "cmp r8, qword ptr [rip + {spawn}]",
        "jne 2f",
        "mov r9, r14",
        "2:",
        "jmp {inner}",
        spawn = sym SPAWN_RETURN,
        inner = sym lookup,
    );
}

#[unsafe(naked)]
unsafe extern "system" fn cursor_entry(_bases: *mut u8, _npc: *const u32) -> u32 {
    core::arch::naked_asm!("mov r8, [rsp]", "lea r9, [rsp + 8]", "jmp {inner}", inner = sym cursor_lookup);
}

unsafe fn original(slot: &AtomicUsize) -> Lookup {
    unsafe { std::mem::transmute(slot.load(Ordering::Acquire)) }
}

unsafe extern "system" fn lookup(
    bases: *mut u8,
    npc: *const u32,
    caller: usize,
    frame: usize,
) -> u32 {
    let found = unsafe { original(&ORIGINAL_LOOKUP)(bases, npc) };
    let _ = std::panic::catch_unwind(|| unsafe { probe::maybe_log(bases) });
    answer(found, npc, caller, frame)
}

unsafe extern "system" fn cursor_lookup(
    bases: *mut u8,
    npc: *const u32,
    caller: usize,
    frame: usize,
) -> u32 {
    let found = unsafe { original(&ORIGINAL_CURSOR_LOOKUP)(bases, npc) };
    answer(found, npc, caller, frame)
}

fn answer(found: u32, npc: *const u32, caller: usize, frame: usize) -> u32 {
    if !ON.load(Ordering::Acquire) || npc.is_null() {
        return found;
    }
    match std::panic::catch_unwind(|| unsafe { resolve(found, *npc, caller, frame) }) {
        Ok(answer) => answer,
        Err(_) => {
            if ON.swap(false, Ordering::AcqRel) {
                warn!("survivors per altar off, vanilla from now on: it panicked");
            }
            found
        }
    }
}

unsafe fn resolve(found: u32, npc: u32, caller: usize, frame: usize) -> u32 {
    let Some(returns) = RETURNS.get() else {
        return found;
    };
    let spawn = caller == SPAWN_RETURN.load(Ordering::Acquire);
    let unique = returns.unique.contains(&caller);
    let bed = caller == returns.bed;
    let assign = returns.assign.contains(&caller);
    let roaming = caller == returns.roaming;
    let quiet = caller == returns.bed_init || returns.cursor.contains(&caller);
    if !(spawn || unique || bed || assign || roaming || quiet) {
        return found;
    }

    let Ok(mut slot) = SHARED.lock() else {
        return found;
    };
    let Some(shared) = slot.as_mut() else {
        return found;
    };
    refresh(shared);
    let copies = &mut shared.copies;

    if unique {
        let ctx = unsafe { read::<*mut u64>(frame + UNIQUE_CONTEXT) };
        let me = unsafe { crate::ecs::entity(ctx) };
        if me == 0 {
            return found;
        }
        let home = unsafe {
            home(
                read(frame + UNIQUE_SAVED_STATE),
                read(frame + UNIQUE_TRANSFORM),
            )
        };
        shared.seen.insert(me, Instant::now());
        // Right after a load the zones are not known yet: keep every copy
        // until they are, or each would look like one outside the zones.
        if !copies.knows_altars() {
            SKIP_UPDATE.with(|skip| skip.set(found != 0 && me != found));
            return if found == 0 { 0 } else { me };
        }
        let original = if found != 0 { found } else { me };
        return match copies.observe(npc, me, home, original) {
            Verdict::Keep if found == 0 => 0,
            Verdict::Keep => {
                SKIP_UPDATE.with(|skip| skip.set(me != found));
                me
            }
            Verdict::Remove => {
                let holder = copies.at(npc, home, original);
                info!("survivor {npc:#010x}: copy {me} removed, {holder} keeps the slot");
                if holder != me { holder } else { original }
            }
        };
    }
    if spawn {
        let target = unsafe { cells_fixed(read(frame + SPAWN_TARGET)) };
        return match copies.for_spawn(npc, target, found) {
            Some(copy) => {
                shared.moving = Some((copy, Instant::now()));
                copy
            }
            None => {
                info!("survivor {npc:#010x}: a new copy at {target:?}");
                0
            }
        };
    }
    if bed {
        unsafe { calibrate(read(frame + BED_CONTEXT), read(frame + BED_TRANSFORM)) };
    }
    if copies.count(npc) <= 1 {
        return found;
    }
    if bed {
        let position = unsafe { cells_fixed(read(read::<usize>(frame + BED_TRANSFORM))) };
        return copies.at(npc, position, found);
    }
    if assign || roaming {
        // The bed being assigned, or the player setting the range.
        let (ctx, entity) = unsafe {
            if assign {
                let targets: usize = read(frame + ASSIGN_TARGETS);
                (
                    read::<*mut u64>(frame + ASSIGN_CONTEXT),
                    read::<u32>(targets + ASSIGN_BED),
                )
            } else {
                let ctx = read::<*mut u64>(frame + ROAMING_CONTEXT);
                (ctx, crate::ecs::entity(ctx))
            }
        };
        return match unsafe { position_of(ctx, entity) } {
            Some(position) => copies.at(npc, position, found),
            None => found,
        };
    }
    0
}

/// Learns the type index of `CurrentTransform` from a bed whose transform
/// the caller already holds.
unsafe fn calibrate(bed_ctx: *mut u64, transform: *const u8) {
    if TRANSFORM_TYPE.load(Ordering::Acquire) != u16::MAX
        || bed_ctx.is_null()
        || transform.is_null()
    {
        return;
    }
    let read_component: ComponentRead =
        unsafe { std::mem::transmute(COMPONENT_READ.load(Ordering::Acquire)) };
    let bed = unsafe { crate::ecs::entity(bed_ctx) };
    if bed == 0 {
        return;
    }
    for kind in 0..TYPE_SEARCH {
        if unsafe { read_component(bed_ctx, &kind, bed) } == transform {
            TRANSFORM_TYPE.store(kind, Ordering::Release);
            info!("CurrentTransform is component type {kind}");
            return;
        }
    }
}

/// Where an entity stands, in whole cells, once the transform type is known.
unsafe fn position_of(ctx: *mut u64, entity: u32) -> Option<[i64; 3]> {
    let kind = TRANSFORM_TYPE.load(Ordering::Acquire);
    if kind == u16::MAX || ctx.is_null() || entity == 0 {
        return None;
    }
    let read_component: ComponentRead =
        unsafe { std::mem::transmute(COMPONENT_READ.load(Ordering::Acquire)) };
    let transform = unsafe { read_component(ctx, &kind, entity) };
    if transform.is_null() {
        return None;
    }
    Some(cells_fixed(unsafe {
        transform.cast::<[i64; 3]>().read_unaligned()
    }))
}

/// Altars move only when someone builds; read their zones now and then, and
/// let go of copies the uniqueness pass no longer sees.
fn refresh(shared: &mut Shared) {
    if shared.refreshed.is_some_and(|at| at.elapsed() < REFRESH) {
        return;
    }
    shared.refreshed = Some(Instant::now());
    shared.copies.set_altars(crate::altars::zones());
    let gone: Vec<u32> = shared
        .seen
        .iter()
        .filter(|(_, at)| at.elapsed() > FORGET_AFTER)
        .map(|(&e, _)| e)
        .collect();
    for entity in gone {
        shared.seen.remove(&entity);
        shared.copies.forget(entity);
    }
}

/// A copy's home: its saved spawn position in metres, or where it stands.
unsafe fn home(saved: *const u8, transform: *const u8) -> [i64; 3] {
    if !saved.is_null() {
        let spawn: [f32; 3] = unsafe { saved.cast::<[f32; 3]>().read_unaligned() };
        if spawn != [0.0; 3] {
            return spawn.map(|m| (m as f64 * 2.0).floor() as i64);
        }
    }
    if transform.is_null() {
        return [i64::MIN; 3];
    }
    unsafe { cells_fixed(transform.cast::<[i64; 3]>().read_unaligned()) }
}

/// 32.32 fixed-point metres to whole 0.5 m cells.
fn cells_fixed(position: [i64; 3]) -> [i64; 3] {
    position.map(|v| v >> 31)
}

unsafe fn read<T: Copy>(at: usize) -> T {
    unsafe { (at as *const T).read_unaligned() }
}

/// Keeps the survivor record on the original: skips the update right after
/// the uniqueness pass kept a copy.
unsafe extern "system" fn update_detour(
    bases: *mut u8,
    npc: *const u32,
    state: usize,
    flag: usize,
) -> usize {
    if SKIP_UPDATE.with(|skip| skip.replace(false)) {
        return 0;
    }
    let original: Update = unsafe { std::mem::transmute(ORIGINAL_UPDATE.load(Ordering::Acquire)) };
    unsafe { original(bases, npc, state, flag) }
}

/// Lets only the copy a summoning staff moves take the move command.
unsafe extern "system" fn move_detour(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize {
    let original: Move = unsafe { std::mem::transmute(ORIGINAL_MOVE.load(Ordering::Acquire)) };
    if ON.load(Ordering::Acquire) && !ctx.is_null() {
        let target = SHARED
            .lock()
            .ok()
            .and_then(|s| s.as_ref()?.moving)
            .filter(|(_, at)| at.elapsed() < MOVE_WINDOW)
            .map(|(copy, _)| copy);
        if let Some(target) = target {
            let me = unsafe { crate::ecs::entity(*ctx) };
            if me != 0 && me != target {
                return 0;
            }
        }
    }
    unsafe { original(ctx, b, c, d) }
}

mod probe {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use crate::info;

    static LAST: Mutex<Option<(Instant, Vec<Placed>)>> = Mutex::new(None);
    const EVERY: Duration = Duration::from_secs(10);

    /// The survivor map inside the base system, an open-addressing hash map.
    const MAP: usize = 0x1560;
    const MAP_OCCUPIED: usize = 0x8;
    const MAP_CAPACITY: usize = 0x10;
    const MAP_KEYS: usize = 0x28;
    const MAP_VALUES: usize = 0x40;
    const MAP_COUNT: usize = 0x54;
    const VALUE_SIZE: usize = 0xc;
    const SANE_CAPACITY: usize = 1 << 12;

    #[derive(Debug, PartialEq)]
    struct Placed {
        npc: u32,
        base: u8,
        entity: u32,
        happiness: u32,
    }

    /// Logs the map, at most every ten seconds and only when it changed.
    pub(super) unsafe fn maybe_log(bases: *mut u8) {
        let Ok(mut last) = LAST.try_lock() else {
            return;
        };
        if last.as_ref().is_some_and(|(at, _)| at.elapsed() < EVERY) {
            return;
        }
        let Some(placed) = (unsafe { read(bases) }) else {
            return;
        };
        if last.as_ref().is_none_or(|(_, seen)| *seen != placed) {
            info!("survivor map: {} entries", placed.len());
            for p in &placed {
                info!(
                    "  survivor {:#010x}: base {}, entity {}, happiness {}",
                    p.npc, p.base, p.entity, p.happiness
                );
            }
        }
        *last = Some((Instant::now(), placed));
    }

    unsafe fn read(bases: *mut u8) -> Option<Vec<Placed>> {
        unsafe {
            let map = bases.add(MAP);
            let capacity = map.add(MAP_CAPACITY).cast::<usize>().read();
            let count = map.add(MAP_COUNT).cast::<u32>().read() as usize;
            let occupied = map.add(MAP_OCCUPIED).cast::<*const u64>().read();
            let keys = map.add(MAP_KEYS).cast::<*const u32>().read();
            let values = map.add(MAP_VALUES).cast::<*const u8>().read();
            if capacity == 0 || capacity > SANE_CAPACITY || count > capacity {
                return None;
            }
            if occupied.is_null() || keys.is_null() || values.is_null() {
                return None;
            }
            let mut placed = Vec::with_capacity(count);
            for slot in 0..capacity {
                if (occupied.add(slot / 64).read() >> (slot % 64)) & 1 == 0 {
                    continue;
                }
                let value = values.add(slot * VALUE_SIZE);
                placed.push(Placed {
                    npc: keys.add(slot).read(),
                    base: value.read(),
                    entity: value.add(4).cast::<u32>().read_unaligned(),
                    happiness: value.add(8).cast::<u32>().read_unaligned(),
                });
            }
            placed.sort_by_key(|p| p.npc);
            Some(placed)
        }
    }
}
