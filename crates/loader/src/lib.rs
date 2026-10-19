//! The mods' loader: a proxy POWRPROF.dll placed beside the Enshrouded
//! server or client executable.
//!
//! On load it forwards POWRPROF.dll to the system copy, then starts the mods.
//! Nothing here may take the game down: every mod step runs behind
//! `catch_unwind`, and a step that fails disables its feature and logs why.

#[cfg(windows)]
mod altar_count;
#[cfg(windows)]
mod altars;
#[cfg(windows)]
mod builds;
pub mod config;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
pub mod log;
#[cfg(windows)]
mod proxy;
pub mod seam;
#[cfg(windows)]
mod start;
pub mod zones;

#[cfg(windows)]
#[unsafe(no_mangle)]
extern "system" fn DllMain(
    _module: *mut core::ffi::c_void,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
    if reason == DLL_PROCESS_ATTACH {
        // The forwards come first and never depend on the mods: the game
        // calls through them whatever happens below.
        let forwarded = unsafe { proxy::install() };
        let _ = std::panic::catch_unwind(|| start::start(forwarded));
    }
    1
}
