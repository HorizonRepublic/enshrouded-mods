//! Survivors on the server: a read-only probe, and an experiment towards a
//! copy of a survivor per altar.
//!
//! The base system keeps one map from a survivor's id to its placement:
//! base index, entity and happiness. Every lookup in it goes through one
//! function, and its callers decide what a survivor may do from the answer.
//! The detour on that function:
//!
//! - logs the map, at most every ten seconds and only when it changed, on
//!   the game's own thread where the map cannot change under it;
//! - with `[survivors] experiment_duplicates`, changes the answer for two
//!   callers only, told apart by the return address. The spawn path is told
//!   no copy exists, so a summoning staff spawns a new copy instead of
//!   moving the old one; `unique_npc_spawns`, which destroys every entity
//!   whose survivor maps to another entity, is told each entity is the one
//!   on record. Every other caller gets the vanilla answer.
//!
//! The experiment keeps every copy alive and lets them share one record, so
//! beds and happiness follow whichever copy registered last. Telling copies
//! apart by altar comes after it shows the game tolerates them.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::builds::Build;
use crate::info;

type Lookup = unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32;
type System = unsafe extern "system" fn(ctx: *mut u64);

static ORIGINAL_LOOKUP: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UNIQUE: AtomicUsize = AtomicUsize::new(0);
static DUPLICATES: AtomicBool = AtomicBool::new(false);
/// Return addresses of the callers the experiment answers differently.
static SPAWN_RETURN: AtomicUsize = AtomicUsize::new(0);
static UNIQUE_RETURNS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

thread_local! {
    /// The query context of the `unique_npc_spawns` run on this thread.
    static UNIQUE_CONTEXT: Cell<*mut u64> = const { Cell::new(std::ptr::null_mut()) };
}

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

pub(crate) fn install(
    build: &Build,
    base: usize,
    unique: usize,
    duplicates: bool,
) -> Result<(), String> {
    SPAWN_RETURN.store(base + build.npc_spawn_return as usize, Ordering::Release);
    for (slot, rva) in UNIQUE_RETURNS.iter().zip(build.npc_unique_returns) {
        slot.store(base + rva as usize, Ordering::Release);
    }
    DUPLICATES.store(duplicates, Ordering::Release);

    let lookup = base + build.npc_lookup as usize;
    let original = unsafe { crate::hook::install(lookup, lookup_entry as Lookup as usize) }?;
    ORIGINAL_LOOKUP.store(original, Ordering::Release);
    if duplicates {
        let original = unsafe { crate::hook::install(unique, unique_detour as System as usize) }?;
        ORIGINAL_UNIQUE.store(original, Ordering::Release);
    }
    Ok(())
}

/// Hands the lookup's own return address to `lookup` as a third argument
/// and lets it return straight to the caller.
#[unsafe(naked)]
unsafe extern "system" fn lookup_entry(_bases: *mut u8, _npc: *const u32) -> u32 {
    core::arch::naked_asm!("mov r8, [rsp]", "jmp {inner}", inner = sym lookup);
}

unsafe extern "system" fn lookup(bases: *mut u8, npc: *const u32, caller: usize) -> u32 {
    let original: Lookup = unsafe { std::mem::transmute(ORIGINAL_LOOKUP.load(Ordering::Acquire)) };
    let found = unsafe { original(bases, npc) };
    let _ = std::panic::catch_unwind(|| unsafe { maybe_log(bases) });
    if !DUPLICATES.load(Ordering::Acquire) {
        return found;
    }
    if caller == SPAWN_RETURN.load(Ordering::Acquire) {
        return 0;
    }
    if found != 0
        && UNIQUE_RETURNS
            .iter()
            .any(|r| r.load(Ordering::Acquire) == caller)
    {
        let ctx = UNIQUE_CONTEXT.with(Cell::get);
        if !ctx.is_null() {
            let me = unsafe { crate::ecs::entity(ctx) };
            if me != 0 {
                return me;
            }
        }
    }
    found
}

unsafe extern "system" fn unique_detour(ctx: *mut u64) {
    let original: System = unsafe { std::mem::transmute(ORIGINAL_UNIQUE.load(Ordering::Acquire)) };
    UNIQUE_CONTEXT.with(|c| c.set(ctx));
    unsafe { original(ctx) };
    UNIQUE_CONTEXT.with(|c| c.set(std::ptr::null_mut()));
}

unsafe fn maybe_log(bases: *mut u8) {
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
