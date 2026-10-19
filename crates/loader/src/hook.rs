//! Inline hooks through MinHook.

use std::ffi::c_void;

use minhook::MinHook;

/// Detours `target` to `detour` and returns the trampoline that calls the
/// original. The hook is created and enabled, or nothing changes.
///
/// # Safety
/// `target` must be the start of a function whose signature `detour` and
/// the returned trampoline share.
pub(crate) unsafe fn install(target: usize, detour: usize) -> Result<usize, String> {
    let target = target as *mut c_void;
    let original = unsafe { MinHook::create_hook(target, detour as *mut c_void) }
        .map_err(|e| format!("create hook at {target:p}: {e:?}"))?;
    if let Err(e) = unsafe { MinHook::enable_hook(target) } {
        let _ = unsafe { MinHook::remove_hook(target) };
        return Err(format!("enable hook at {target:p}: {e:?}"));
    }
    Ok(original as usize)
}
