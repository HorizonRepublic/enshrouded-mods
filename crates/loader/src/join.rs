//! The client checks the server's pack version when it joins.
//!
//! The game hands every server change to one function with the target's
//! `ip:port`. The detour lets the join go ahead and asks the server, over
//! the Steam server query, for its `ensh-mods` rule (see `advertise`). When
//! the server runs another version, or none while this client runs the
//! mods, a message box says which release to install.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

const RELEASES: &str = "https://github.com/HorizonRepublic/enshrouded-mods/releases";
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// What the server runs, as far as the query tells.
#[derive(Debug, PartialEq, Eq)]
pub enum Server {
    Pack(String),
    Vanilla,
}

/// The message for the player, or `None` when the versions match.
pub fn verdict(ours: &str, server: &Server) -> Option<String> {
    match server {
        Server::Pack(theirs) if theirs == ours => None,
        Server::Pack(theirs) => Some(format!(
            "This server runs ensh-mods {theirs}; you have {ours}.\n\n\
             Install the same release from\n{RELEASES}/tag/v{theirs}\n\
             and restart the game. Until then, building may not match what the server allows."
        )),
        Server::Vanilla => Some(format!(
            "This server runs no ensh-mods; you have {ours}.\n\n\
             You can play, but building across altar seams and the altar allowance\n\
             follow the server, not your mods."
        )),
    }
}

/// The `ip:port` at the start of a server change request.
pub fn address(request: &[u8]) -> Option<SocketAddr> {
    let end = request.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&request[..end])
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The value of `key` in an A2S_RULES reply.
pub fn rule(reply: &[u8], key: &str) -> Option<String> {
    let body = reply.strip_prefix(&[0xff, 0xff, 0xff, 0xff, b'E'])?;
    let count = u16::from_le_bytes(body.get(..2)?.try_into().ok()?) as usize;
    let mut fields = body[2..].split(|&b| b == 0);
    for _ in 0..count {
        let (name, value) = (fields.next()?, fields.next()?);
        if name == key.as_bytes() {
            return String::from_utf8(value.to_vec()).ok();
        }
    }
    None
}

/// Asks `server` for its rules over the Steam server query.
pub fn query_rules(server: SocketAddr) -> std::io::Result<Vec<u8>> {
    let local: SocketAddr = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()
    .unwrap();
    let socket = UdpSocket::bind(local)?;
    socket.set_read_timeout(Some(QUERY_TIMEOUT))?;
    socket.connect(server)?;
    let mut request = vec![0xff, 0xff, 0xff, 0xff, b'V', 0xff, 0xff, 0xff, 0xff];
    let mut reply = [0u8; 1400];
    for _ in 0..2 {
        socket.send(&request)?;
        let n = socket.recv(&mut reply)?;
        // A challenge comes back first; send it with the next request.
        if n == 9 && reply[4] == b'A' {
            request.truncate(5);
            request.extend_from_slice(&reply[5..9]);
            continue;
        }
        return Ok(reply[..n].to_vec());
    }
    Err(std::io::Error::other(
        "the server kept answering with challenges",
    ))
}

#[cfg(windows)]
pub(crate) use hook::install;

#[cfg(windows)]
mod hook {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };

    use super::Server;
    use crate::{info, warn};

    type ServerChange = unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;

    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
    /// The request holds the address in its first 0x40 bytes.
    const ADDRESS_BYTES: usize = 0x40;

    pub(crate) fn install(target: usize) -> Result<(), String> {
        let original = unsafe { crate::hook::install(target, detour as ServerChange as usize) }?;
        ORIGINAL.store(original, Ordering::Release);
        Ok(())
    }

    unsafe extern "system" fn detour(provider: *mut u8, request: *const u8) -> u64 {
        if !request.is_null() {
            let bytes = unsafe { std::slice::from_raw_parts(request, ADDRESS_BYTES) }.to_vec();
            let _ = std::thread::Builder::new()
                .name("ensh-mods-join".into())
                .spawn(move || {
                    let _ = std::panic::catch_unwind(|| check(&bytes));
                });
        }
        let original: ServerChange =
            unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
        unsafe { original(provider, request) }
    }

    fn check(request: &[u8]) {
        let Some(address) = super::address(request) else {
            info!("join check skipped: no address in the server change request");
            return;
        };
        let server = match super::query_rules(address) {
            Ok(reply) => match super::rule(&reply, crate::advertise::RULE) {
                Some(version) => Server::Pack(version),
                None => Server::Vanilla,
            },
            Err(e) => {
                warn!("join check: no answer from {address}: {e}");
                return;
            }
        };
        let ours = env!("CARGO_PKG_VERSION");
        info!("join check: {address} runs {server:?}, this client {ours}");
        if let Some(message) = super::verdict(ours, &server) {
            let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
            let title: Vec<u16> = "ensh-mods".encode_utf16().chain([0]).collect();
            let style = MB_OK | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND;
            unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), style) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_versions_say_nothing() {
        assert_eq!(verdict("0.1.1", &Server::Pack("0.1.1".into())), None);
    }

    #[test]
    fn a_different_version_names_the_release_to_install() {
        let message = verdict("0.1.1", &Server::Pack("0.2.0".into())).unwrap();
        assert!(message.contains("runs ensh-mods 0.2.0; you have 0.1.1"));
        assert!(message.contains("/releases/tag/v0.2.0"));
    }

    #[test]
    fn a_vanilla_server_is_mentioned() {
        assert!(
            verdict("0.1.1", &Server::Vanilla)
                .unwrap()
                .contains("runs no ensh-mods")
        );
    }

    #[test]
    fn the_address_is_read_up_to_the_first_nul() {
        let mut request = [0u8; 0x40];
        request[..15].copy_from_slice(b"127.0.0.1:15647");
        assert_eq!(address(&request), Some("127.0.0.1:15647".parse().unwrap()));
        assert_eq!(address(b"not an address\0"), None);
    }

    #[test]
    fn a_rule_is_found_in_a_rules_reply() {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 2, 0];
        for field in ["other", "x", "ensh-mods", "0.1.1"] {
            reply.extend_from_slice(field.as_bytes());
            reply.push(0);
        }
        assert_eq!(rule(&reply, "ensh-mods").as_deref(), Some("0.1.1"));
        assert_eq!(rule(&reply, "missing"), None);
        assert_eq!(rule(b"garbage", "ensh-mods"), None);
    }
}

/// Against a running server: `ENSH_TEST_SERVER=127.0.0.1:15647`.
#[cfg(test)]
mod live {
    #[test]
    fn a_running_server_reports_its_rule() {
        let Ok(address) = std::env::var("ENSH_TEST_SERVER") else {
            return;
        };
        let reply = super::query_rules(address.parse().unwrap()).unwrap();
        assert!(
            super::rule(&reply, crate::advertise::RULE).is_some(),
            "{reply:?}"
        );
    }
}
