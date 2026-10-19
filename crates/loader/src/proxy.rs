//! Forwards every POWRPROF.dll export to the system copy.
//!
//! Each export is a stub that jumps through `FORWARDS`. `install` fills the
//! table before the game can call anything; an export the system copy lacks
//! (an older Windows, or Wine's builtin) returns `ERROR_CALL_NOT_IMPLEMENTED`.

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

include!(concat!(env!("OUT_DIR"), "/stubs.rs"));

pub(crate) static mut FORWARDS: [usize; EXPORTS.len()] = [0; EXPORTS.len()];

extern "system" fn not_implemented() -> u32 {
    windows_sys::Win32::Foundation::ERROR_CALL_NOT_IMPLEMENTED
}

/// Loads the system POWRPROF.dll and points every stub at it. Returns how
/// many exports it lacks, or `None` when it cannot be loaded at all.
///
/// # Safety
/// Call once, from `DllMain`, before any stub can run.
pub(crate) unsafe fn install() -> Option<usize> {
    let fallback = not_implemented as extern "system" fn() -> u32 as usize;
    for i in 0..EXPORTS.len() {
        unsafe { (&raw mut FORWARDS).cast::<usize>().add(i).write(fallback) };
    }

    let mut path = [0u16; 512];
    let len = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    let suffix: Vec<u16> = "\\powrprof.dll\0".encode_utf16().collect();
    if len == 0 || len + suffix.len() > path.len() {
        return None;
    }
    path[len..len + suffix.len()].copy_from_slice(&suffix);
    let module = unsafe { LoadLibraryW(path.as_ptr()) };
    if module.is_null() {
        return None;
    }

    let mut missing = 0;
    for (i, name) in EXPORTS.iter().enumerate() {
        let c_name = format!("{name}\0");
        match unsafe { GetProcAddress(module, c_name.as_ptr()) } {
            Some(function) => unsafe {
                (&raw mut FORWARDS)
                    .cast::<usize>()
                    .add(i)
                    .write(function as usize)
            },
            None => missing += 1,
        }
    }
    Some(missing)
}
