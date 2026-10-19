//! Reading the engine's ECS query state.

use crate::builds::engine;

/// The 1-based row the query context is on; 0 before the first row.
///
/// # Safety
/// `ctx` must be a live query context.
pub(crate) unsafe fn row_counter(ctx: *mut u64) -> *mut u32 {
    unsafe { (ctx as *mut u8).add(engine::CONTEXT_ROW).cast::<u32>() }
}

/// The entity of the query's current row, 0 before the first row.
///
/// # Safety
/// `ctx` must be a live query context.
pub(crate) unsafe fn entity(ctx: *mut u64) -> u32 {
    unsafe {
        let row = row_counter(ctx).read();
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
