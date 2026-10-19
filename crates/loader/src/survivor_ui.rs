//! The client's survivor status with a copy per altar.
//!
//! The client keeps one UI record per survivor, and `ui_npc_status` writes
//! each copy's status bits into it in turn, so the copy written last decides
//! whether the panel says "no bed". After vanilla's pass, this one walks the
//! same entities and leaves in each record the bits every copy has, the
//! worst status: a copy without a bed shows as one. It only reads the game's
//! data and writes that one UI byte; anything unexpected keeps vanilla's.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::builds::Build;
use crate::warn;

type System = unsafe extern "system" fn(ctx: *mut u64);
type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;
type Template = unsafe extern "system" fn(ctx: *mut u64) -> *const [u8; 16];

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static INIT: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static TEMPLATE: AtomicUsize = AtomicUsize::new(0);
static ON: AtomicBool = AtomicBool::new(false);

/// `ui_npc_status`'s iterator: a header, `ClientActiveNpcState`,
/// `NetworkComfort`, `RenderTransform`, UI offsets, `UiPlayData`.
const ITERATOR_WORDS: usize = 6;
const ITERATOR_STATE: usize = 1;
const ITERATOR_PLAY_DATA: usize = 5;
/// `ClientActiveNpcState`: the status bits at 0x14.
const STATE_STATUS: usize = 0x14;
/// The survivor UI records in `UiPlayData`: an open-addressing map from
/// survivor id to a 0x928-byte record, the status byte at 0x880.
const MAP_BITS: usize = 0x463e98;
const MAP_CAPACITY: usize = 0x463ea0;
const MAP_KEYS: usize = 0x463eb8;
const MAP_VALUES: usize = 0x463ed0;
const MAP_COUNT: usize = 0x463ee4;
const RECORD_SIZE: usize = 0x928;
const RECORD_STATUS: usize = 0x880;

pub(crate) fn install(build: &Build, base: usize) -> Result<(), String> {
    if build.npc_ui_status == 0 || build.npc_template == 0 {
        return Err("no addresses for this build".into());
    }
    let at = |rva: u64| base + rva as usize;
    INIT.store(at(build.iterator_init), Ordering::Release);
    NEXT.store(at(build.iterator_next), Ordering::Release);
    TEMPLATE.store(at(build.npc_template), Ordering::Release);
    let original =
        unsafe { crate::hook::install(at(build.npc_ui_status), detour as System as usize) }?;
    ORIGINAL.store(original, Ordering::Release);
    ON.store(true, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn detour(ctx: *mut u64) {
    let original: System = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    unsafe { original(ctx) };
    if !ON.load(Ordering::Acquire) || ctx.is_null() {
        return;
    }
    if std::panic::catch_unwind(|| unsafe { worst_status(ctx) }).is_err()
        && ON.swap(false, Ordering::AcqRel)
    {
        warn!("survivor status per altar off, vanilla from now on: it panicked");
    }
}

unsafe fn worst_status(ctx: *mut u64) {
    let init: IteratorInit = unsafe { std::mem::transmute(INIT.load(Ordering::Acquire)) };
    let next: IteratorNext = unsafe { std::mem::transmute(NEXT.load(Ordering::Acquire)) };
    let template: Template = unsafe { std::mem::transmute(TEMPLATE.load(Ordering::Acquire)) };

    // Survivor id -> (bits every copy has, copies seen).
    let mut status: BTreeMap<u32, (u8, u32)> = BTreeMap::new();
    let mut play_data = 0usize;
    let size = (ITERATOR_WORDS * 8) as u64;
    let mut it = [0u64; ITERATOR_WORDS];
    unsafe {
        init(ctx, it.as_mut_ptr(), size);
        while next(ctx, it.as_mut_ptr(), size) & 0xff != 0 {
            let state = it[ITERATOR_STATE] as usize;
            let guid = template(ctx);
            if state == 0 || guid.is_null() {
                continue;
            }
            play_data = it[ITERATOR_PLAY_DATA] as usize;
            let bits = read::<u8>(state + STATE_STATUS);
            let entry = status.entry(survivor_id(&*guid)).or_insert((u8::MAX, 0));
            entry.0 &= bits;
            entry.1 += 1;
        }
    }
    if play_data == 0 {
        return;
    }
    for (id, (bits, copies)) in status {
        if copies < 2 {
            continue;
        }
        if let Some(record) = unsafe { record(play_data, id) } {
            unsafe { ((record + RECORD_STATUS) as *mut u8).write(bits) };
        }
    }
}

/// The UI record of survivor `id`, found as vanilla finds it.
unsafe fn record(play_data: usize, id: u32) -> Option<usize> {
    unsafe {
        let capacity = read::<u64>(play_data + MAP_CAPACITY) as usize;
        if read::<u32>(play_data + MAP_COUNT) == 0 || capacity == 0 {
            return None;
        }
        let bits = read::<usize>(play_data + MAP_BITS);
        let keys = read::<usize>(play_data + MAP_KEYS);
        let values = read::<usize>(play_data + MAP_VALUES);
        if bits == 0 || keys == 0 || values == 0 {
            return None;
        }
        let used = |i: usize| read::<u64>(bits + (i >> 6) * 8) >> (i & 63) & 1 != 0;
        let mut i = (capacity - 1) & id as usize;
        for _ in 0..capacity {
            if !used(i) {
                return None;
            }
            if read::<u32>(keys + i * 4) == id {
                return Some(values + i * RECORD_SIZE);
            }
            i = if i + 1 < capacity { i + 1 } else { 0 };
        }
        None
    }
}

/// A survivor's id: FNV-1a over its template's GUID, 0 for an empty GUID.
fn survivor_id(guid: &[u8; 16]) -> u32 {
    if guid.iter().all(|&b| b == 0) {
        return 0;
    }
    guid.iter().fold(0x811c_9dc5u32, |h, &b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

unsafe fn read<T: Copy>(at: usize) -> T {
    unsafe { (at as *const T).read_unaligned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_guid_has_id_zero() {
        assert_eq!(survivor_id(&[0; 16]), 0);
    }

    #[test]
    fn the_id_is_fnv1a_over_the_guid() {
        // FNV-1a of sixteen 0x01 bytes, computed byte by byte.
        let mut h = 0x811c_9dc5u32;
        for _ in 0..16 {
            h = (h ^ 1).wrapping_mul(0x0100_0193);
        }
        assert_eq!(survivor_id(&[1; 16]), h);
    }
}
