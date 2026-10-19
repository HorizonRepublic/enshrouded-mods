//! The server tells the server browser which mod pack it runs.
//!
//! Once the dedicated server has logged on to Steam, the loader sets its
//! Steam name to the configured one plus `[ensh-mods <version>]`, and a
//! Steam rule `ensh-mods = <version>`. Players see the version before they
//! join, with or without the mods; a modded client can read it from the
//! server query. The game sets its name once at startup, so the loader sets
//! it again now and then.

/// Steam keeps at most 63 bytes of a server name.
const NAME_LIMIT: usize = 63;

pub const RULE: &str = "ensh-mods";

/// `name` with the pack version appended, shortened to fit Steam's limit.
pub fn server_name(name: &str, version: &str) -> String {
    let suffix = format!(" [ensh-mods {version}]");
    let base = name.trim();
    let base = base
        .strip_suffix(suffix.trim_start())
        .map(str::trim_end)
        .unwrap_or(base);
    let room = NAME_LIMIT.saturating_sub(suffix.len());
    let mut cut = base.len().min(room);
    while !base.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{suffix}", base[..cut].trim_end())
}

/// The `"name"` of an `enshrouded_server.json`.
pub fn configured_name(json: &str) -> Option<String> {
    let start = json.find("\"name\"")? + "\"name\"".len();
    let rest = json[start..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let mut name = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(name),
            '\\' => match chars.next()? {
                'n' => name.push('\n'),
                't' => name.push('\t'),
                other => name.push(other),
            },
            c => name.push(c),
        }
    }
    None
}

#[cfg(windows)]
pub(crate) use runtime::start;

#[cfg(windows)]
mod runtime {
    use std::ffi::{CString, c_void};
    use std::time::Duration;

    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    use crate::{info, warn};

    type Interface = unsafe extern "C" fn() -> *mut c_void;
    type LoggedOn = unsafe extern "C" fn(server: *mut c_void) -> bool;
    type SetName = unsafe extern "C" fn(server: *mut c_void, name: *const i8);
    type SetKeyValue = unsafe extern "C" fn(server: *mut c_void, key: *const i8, value: *const i8);

    const POLL: Duration = Duration::from_secs(2);
    const GIVE_UP: Duration = Duration::from_secs(300);
    const REASSERT: Duration = Duration::from_secs(60);

    /// Starts the thread that advertises the version.
    pub(crate) fn start(dir: std::path::PathBuf) {
        let _ = std::thread::Builder::new()
            .name("ensh-mods-advertise".into())
            .spawn(move || {
                if std::panic::catch_unwind(|| run(&dir)).is_err() {
                    warn!("version advertisement stopped: it panicked");
                }
            });
    }

    fn run(dir: &std::path::Path) {
        let Some(api) = Api::load() else {
            warn!("version advertisement off: steam_api64.dll lacks the game server API");
            return;
        };
        let configured = std::fs::read_to_string(dir.join("enshrouded_server.json"))
            .ok()
            .and_then(|json| super::configured_name(&json))
            .unwrap_or_else(|| "Enshrouded Server".to_owned());
        let version = env!("CARGO_PKG_VERSION");
        let name = super::server_name(&configured, version);
        let (Ok(name), Ok(key), Ok(value)) = (
            CString::new(name.clone()),
            CString::new(super::RULE),
            CString::new(version),
        ) else {
            warn!("version advertisement off: the server name holds a NUL");
            return;
        };

        let started = std::time::Instant::now();
        let server = loop {
            let server = unsafe { (api.interface)() };
            if !server.is_null() && unsafe { (api.logged_on)(server) } {
                break server;
            }
            if started.elapsed() > GIVE_UP {
                warn!("version advertisement off: the server never logged on to Steam");
                return;
            }
            std::thread::sleep(POLL);
        };
        info!("advertising \"{}\"", name.to_string_lossy());
        loop {
            unsafe {
                (api.set_name)(server, name.as_ptr());
                (api.set_key_value)(server, key.as_ptr(), value.as_ptr());
            }
            std::thread::sleep(REASSERT);
        }
    }

    struct Api {
        interface: Interface,
        logged_on: LoggedOn,
        set_name: SetName,
        set_key_value: SetKeyValue,
    }

    impl Api {
        fn load() -> Option<Api> {
            let module_name: Vec<u16> = "steam_api64.dll\0".encode_utf16().collect();
            let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
            if module.is_null() {
                return None;
            }
            let get = |name: &str| -> Option<usize> {
                let name = CString::new(name).ok()?;
                unsafe { GetProcAddress(module, name.as_ptr().cast()) }.map(|f| f as usize)
            };
            let interface = get("SteamAPI_SteamGameServer_v015")?;
            let logged_on = get("SteamAPI_ISteamGameServer_BLoggedOn")?;
            let set_name = get("SteamAPI_ISteamGameServer_SetServerName")?;
            let set_key_value = get("SteamAPI_ISteamGameServer_SetKeyValue")?;
            // Each export has the flat API's signature for its name.
            unsafe {
                Some(Api {
                    interface: std::mem::transmute::<usize, Interface>(interface),
                    logged_on: std::mem::transmute::<usize, LoggedOn>(logged_on),
                    set_name: std::mem::transmute::<usize, SetName>(set_name),
                    set_key_value: std::mem::transmute::<usize, SetKeyValue>(set_key_value),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_goes_after_the_name() {
        assert_eq!(server_name("Homelab", "0.1.1"), "Homelab [ensh-mods 0.1.1]");
    }

    #[test]
    fn a_long_name_is_shortened_to_fit() {
        let name = server_name(&"x".repeat(80), "0.1.1");
        assert_eq!(name.len(), NAME_LIMIT);
        assert!(name.ends_with(" [ensh-mods 0.1.1]"));
    }

    #[test]
    fn a_name_is_never_cut_inside_a_character() {
        let name = server_name(&"ї".repeat(40), "0.1.1");
        assert!(name.len() <= NAME_LIMIT);
        assert!(name.ends_with(" [ensh-mods 0.1.1]"));
    }

    #[test]
    fn the_suffix_is_not_added_twice() {
        assert_eq!(
            server_name("Homelab [ensh-mods 0.1.1]", "0.1.1"),
            "Homelab [ensh-mods 0.1.1]"
        );
    }

    #[test]
    fn the_name_is_read_from_the_server_config() {
        let json = "{\n\t\"name\": \"Our \\\"Base\\\"\",\n\t\"saveDirectory\": \"./savegame\"\n}";
        assert_eq!(configured_name(json).as_deref(), Some("Our \"Base\""));
        assert_eq!(configured_name("{}"), None);
    }
}
