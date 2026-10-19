//! Startup: identify the game binary and resolve what the mods need.

use keen_reflect::{Fingerprint, Image, Layout, Reflection};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::{info, warn};

pub(crate) fn start(forwarded: Option<usize>) {
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(dir) = exe.parent() {
        crate::log::open(dir);
    }
    info!(
        "ensh-mods {} in {}",
        env!("CARGO_PKG_VERSION"),
        exe.display()
    );
    match forwarded {
        Some(0) => info!("POWRPROF.dll forwarded"),
        Some(missing) => {
            warn!("POWRPROF.dll forwarded; the system copy lacks {missing} exports")
        }
        None => warn!("system POWRPROF.dll not loaded; its exports return an error"),
    }

    let Some(image) = main_image() else {
        warn!("cannot read the game image; mods disabled");
        return;
    };
    info!(
        "image at {:#x}, build {}",
        image.base,
        describe(image.fingerprint)
    );
    let reflection = Reflection::scan(&image);
    info!(
        "{} types, {} systems",
        reflection.type_count(),
        reflection.system_count()
    );

    let Some(build) = crate::builds::find(image.fingerprint) else {
        warn!("unknown game build; every mod stays off");
        return;
    };
    info!("known build: {}", build.name);
    for &(name, rva) in build.systems {
        match reflection.system(name) {
            Ok(s) if s.entry == image.base + rva => {}
            Ok(s) => {
                warn!(
                    "{name} is at {:#x}, the build table says {rva:#x}; every mod stays off",
                    s.entry - image.base
                );
                return;
            }
            Err(e) => {
                warn!("{e}; every mod stays off");
                return;
            }
        }
    }

    if build.role == crate::builds::Role::Server {
        let entry = image.base as usize
            + build
                .systems
                .iter()
                .find(|s| s.0 == "add_buildzones_server")
                .map(|s| s.1)
                .unwrap_or(0) as usize;
        match crate::altars::install(build, image.base as usize, entry) {
            Ok(()) => info!("altar observer installed"),
            Err(e) => warn!("altar observer off: {e}"),
        }
    }
}

fn describe(fingerprint: Option<Fingerprint>) -> String {
    match fingerprint {
        Some(f) => {
            let guid: String = f.guid.iter().map(|b| format!("{b:02x}")).collect();
            format!("{guid}/{}", f.age)
        }
        None => "unknown".to_owned(),
    }
}

/// The executable's own image, as mapped.
fn main_image() -> Option<Image<'static>> {
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    if base == 0 {
        return None;
    }
    // SizeOfImage sits at offset 56 of the optional header.
    let size = unsafe {
        let pe = *((base + 0x3c) as *const u32) as usize;
        *((base + pe + 24 + 56) as *const u32) as usize
    };
    let bytes = unsafe { std::slice::from_raw_parts(base as *const u8, size) };
    Image::parse(bytes, Layout::Mapped, Some(base as u64)).ok()
}
