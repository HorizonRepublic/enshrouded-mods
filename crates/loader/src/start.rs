//! Startup: identify the game binary and resolve what the mods need.

use keen_reflect::{Fingerprint, Image, Layout, Reflection};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::{info, warn};

const CLIENT_HOOK_DELAY: std::time::Duration = std::time::Duration::from_secs(15);

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

    let entry = |name: &str| {
        build
            .systems
            .iter()
            .find(|s| s.0 == name)
            .map(|s| image.base as usize + s.1 as usize)
    };
    let config = exe.parent().map(load_config).unwrap_or_default();
    let seam = image.base as usize + build.seam_check as usize;
    let max_altars = image.base as usize + build.max_altars as usize;
    let install_seam = move || {
        match crate::seam::install(seam) {
            Ok(()) => info!("building across seams on"),
            Err(e) => warn!("building across seams off: {e}"),
        }
        let table = config.altars_per_flame_level.clone();
        match crate::altar_count::install(max_altars, config) {
            Ok(()) => info!("altar count on: {table:?} by Flame level"),
            Err(e) => warn!("altar count off: {e}"),
        }
    };
    if build.role == crate::builds::Role::Client {
        // The client starts under Steam's DRM wrapper, which may check its
        // own code first; patch only once the game is running. Placement
        // checks matter only in game, long after this.
        std::thread::spawn(move || {
            std::thread::sleep(CLIENT_HOOK_DELAY);
            let _ = std::panic::catch_unwind(install_seam);
        });
    } else {
        install_seam();
    }

    if build.role == crate::builds::Role::Server
        && let Some(dir) = exe.parent()
    {
        crate::advertise::start(dir.to_path_buf());
    }

    if build.role == crate::builds::Role::Server {
        let (Some(upgrade), Some(add_zones)) = (
            entry("upgrade_building_range"),
            entry("add_buildzones_server"),
        ) else {
            warn!("altar zones off: the build table lacks their systems");
            return;
        };
        match crate::altars::install(build, image.base as usize, upgrade, add_zones) {
            Ok(()) => info!("altar zones on"),
            Err(e) => warn!("altar zones off: {e}"),
        }
    }
}

/// Reads `ensh-mods.toml` beside the executable, writing the defaults when
/// there is none.
fn load_config(dir: &std::path::Path) -> crate::config::Config {
    use crate::config::{Config, FILE_NAME, default_file};
    let path = dir.join(FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let (config, problems) = Config::parse(&text);
            for problem in problems {
                warn!("{FILE_NAME}: {problem}");
            }
            config
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::write(&path, default_file()) {
                Ok(()) => info!("wrote the default {FILE_NAME}"),
                Err(e) => warn!("cannot write {}: {e}; using the defaults", path.display()),
            }
            Config::default()
        }
        Err(e) => {
            warn!("cannot read {}: {e}; using the defaults", path.display());
            Config::default()
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
