//! A read-only look at how the server binds survivors to entities.
//!
//! The base system keeps one map from a survivor's id to its placement:
//! base index, entity and happiness. Every lookup in it passes through one
//! function, so its detour sees the base system and, every few seconds,
//! logs the whole map when it changed. It runs on the game's own thread,
//! where the map cannot change under it, and never writes anything.
//!
//! This is the first experiment towards survivors per altar; it changes
//! nothing in game.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::info;

type Lookup = unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
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

pub(crate) fn install(target: usize) -> Result<(), String> {
    let original = unsafe { crate::hook::install(target, detour as Lookup as usize) }?;
    ORIGINAL.store(original, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn detour(bases: *mut u8, npc: *const u32) -> u32 {
    let original: Lookup = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    let result = unsafe { original(bases, npc) };
    let _ = std::panic::catch_unwind(|| unsafe { maybe_log(bases) });
    result
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
    let changed = last.as_ref().is_none_or(|(_, seen)| *seen != placed);
    if changed {
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
