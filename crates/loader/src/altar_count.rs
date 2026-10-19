//! How many Flame Altars each Flame level allows.
//!
//! The engine asks one function, `max_altars(balancing, flame_level)`,
//! which reads `altarsPerFlameLevel` and caps it at 16. The cursor check
//! that refuses an altar past the limit and the client's interface both ask
//! it, so the detour answers from the config instead of the table.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::config::Config;
use crate::warn;

type MaxAltars = unsafe extern "system" fn(balancing: u64, flame_level: u32) -> u8;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static OFF: AtomicBool = AtomicBool::new(false);
static TABLE: OnceLock<Config> = OnceLock::new();

pub(crate) fn install(target: usize, config: Config) -> Result<(), String> {
    let _ = TABLE.set(config);
    let original = unsafe { crate::hook::install(target, detour as MaxAltars as usize) }?;
    ORIGINAL.store(original, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn detour(balancing: u64, flame_level: u32) -> u8 {
    if !OFF.load(Ordering::Acquire)
        && let Ok(Some(n)) =
            std::panic::catch_unwind(|| TABLE.get().map(|c| c.altars(flame_level as usize)))
    {
        return n;
    }
    if !OFF.swap(true, Ordering::AcqRel) {
        warn!("altar count off, vanilla from now on: no table");
    }
    let original: MaxAltars = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    unsafe { original(balancing, flame_level) }
}
