//! Flame altar zones.
//!
//! This first step only observes: after `add_buildzones_server` runs, it
//! walks the same query (every altar: `BaseIdComponent`, `CurrentTransform`,
//! `TriggerShape`) and logs each altar whenever the set changes. What it
//! logs decides how the writer handles rotation and offsets.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::builds::{Build, engine};
use crate::{info, warn};

type System = unsafe extern "system" fn(ctx: *mut u64);
type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static INIT: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static LAST: Mutex<Vec<Altar>> = Mutex::new(Vec::new());

/// The add_buildzones_server iterator: context, then one pointer per
/// component and resource.
const ITERATOR_WORDS: usize = 5;
const ITERATOR_BASE_ID: usize = 1;
const ITERATOR_TRANSFORM: usize = 2;
const ITERATOR_TRIGGER_SHAPE: usize = 3;

#[derive(Clone, Debug, PartialEq)]
struct Altar {
    entity: u32,
    base_id: [u8; 16],
    /// 32.32 fixed point.
    position: [i64; 3],
    /// The eight floats after the position: orientation, then the rest.
    transform_tail: [f32; 8],
    shape: u32,
    range: [f32; 3],
    /// `ecs::Offset`: local offset, world offset, orientation offset.
    offset: [f32; 10],
}

pub(crate) fn install(build: &Build, base: usize, entry: usize) -> Result<(), String> {
    INIT.store(base + build.iterator_init as usize, Ordering::Release);
    NEXT.store(base + build.iterator_next as usize, Ordering::Release);
    let original = unsafe { crate::hook::install(entry, detour as System as usize) }?;
    ORIGINAL.store(original, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn detour(ctx: *mut u64) {
    let original: System = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    unsafe { original(ctx) };
    if std::panic::catch_unwind(|| unsafe { observe(ctx) }).is_err() {
        warn!("altar observer panicked; ignoring this tick");
    }
}

unsafe fn observe(ctx: *mut u64) {
    let init: IteratorInit = unsafe { std::mem::transmute(INIT.load(Ordering::Acquire)) };
    let next: IteratorNext = unsafe { std::mem::transmute(NEXT.load(Ordering::Acquire)) };
    let size = (ITERATOR_WORDS * 8) as u64;
    let mut iterator = [0u64; ITERATOR_WORDS];
    let mut altars = Vec::new();
    // The original left the row counter at the end of the query; walk it
    // again from the start and leave the counter as it was.
    let row = unsafe { (ctx as *mut u8).add(engine::CONTEXT_ROW).cast::<u32>() };
    let saved = unsafe { row.read() };
    unsafe {
        row.write(0);
        init(ctx, iterator.as_mut_ptr(), size);
        while next(ctx, iterator.as_mut_ptr(), size) & 0xff != 0 {
            let transform = iterator[ITERATOR_TRANSFORM] as *const u8;
            let shape = iterator[ITERATOR_TRIGGER_SHAPE] as *const u8;
            altars.push(Altar {
                entity: entity(ctx),
                base_id: read(iterator[ITERATOR_BASE_ID] as *const u8),
                position: read(transform),
                transform_tail: read(transform.add(0x18)),
                shape: read(shape),
                range: read(shape.add(4)),
                offset: read(shape.add(0x10)),
            });
        }
        row.write(saved);
    }
    altars.sort_by_key(|a| a.entity);

    let Ok(mut last) = LAST.lock() else { return };
    if *last == altars {
        return;
    }
    info!("{} altars:", altars.len());
    for a in &altars {
        let metres = a.position.map(|p| p as f64 / 4294967296.0);
        let id: String = a.base_id.iter().map(|b| format!("{b:02x}")).collect();
        info!(
            "  entity {} base {id} at {metres:.2?} tail {:?} shape {} range {:?} offset {:?}",
            a.entity, a.transform_tail, a.shape, a.range, a.offset
        );
    }
    *last = altars;
}

/// The entity of the query's current row.
unsafe fn entity(ctx: *mut u64) -> u32 {
    unsafe {
        let row = *(ctx as *const u8).add(engine::CONTEXT_ROW).cast::<u32>();
        if row == 0 {
            return 0;
        }
        let descriptor = *ctx as *const u8;
        let records = *descriptor
            .add(engine::DESCRIPTOR_ENTITY_RECORDS)
            .cast::<*const u8>();
        *records
            .add((row as usize - 1) * engine::ENTITY_RECORD_SIZE)
            .cast::<u32>()
    }
}

unsafe fn read<T: Copy>(at: *const u8) -> T {
    unsafe { at.cast::<T>().read_unaligned() }
}
