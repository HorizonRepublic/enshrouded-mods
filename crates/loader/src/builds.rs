//! The game builds the mods know.
//!
//! Systems are found by name through reflection, but a hook also calls
//! engine helpers and reads engine structures that reflection does not
//! describe. Those are pinned per build here, and a build missing from this
//! table runs vanilla: the loader logs why and installs no hook. Every
//! system address in a row is checked against reflection at startup, so a
//! row can never be applied to the wrong binary.

use keen_reflect::Fingerprint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Server,
    Client,
}

/// Addresses relative to the image base.
#[derive(Debug)]
pub(crate) struct Build {
    pub name: &'static str,
    pub role: Role,
    pub fingerprint: Fingerprint,
    /// `(system name, entry)`; each must match reflection.
    pub systems: &'static [(&'static str, u64)],
    /// The ECS query iterator: `init(ctx, iterator, size)` and
    /// `next(ctx, iterator, size) -> bool`.
    pub iterator_init: u64,
    pub iterator_next: u64,
}

const fn guid(hex: &[u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (digit(hex[2 * i]) << 4) | digit(hex[2 * i + 1]);
        i += 1;
    }
    out
}

const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("not a lowercase hex digit"),
    }
}

pub(crate) const BUILDS: &[Build] = &[
    Build {
        name: "1.0 server (1269579)",
        role: Role::Server,
        fingerprint: Fingerprint {
            guid: guid(b"1c129660cd6efc4f9f0b21652d5ff2bf"),
            age: 387,
        },
        systems: &[
            ("upgrade_building_range", 0x90a60),
            ("add_buildzones_server", 0xe90e0),
        ],
        iterator_init: 0x65b8c0,
        iterator_next: 0x656690,
    },
    Build {
        name: "1.0 client (1269579)",
        role: Role::Client,
        fingerprint: Fingerprint {
            guid: guid(b"69f0e9573b32c24a8838d3fff550824c"),
            age: 410,
        },
        systems: &[
            ("upgrade_building_range", 0x27f030),
            ("add_buildzones_server", 0x328df0),
            ("add_buildzones_client", 0x3288e0),
        ],
        iterator_init: 0x9bab60,
        iterator_next: 0x9b5a50,
    },
];

pub(crate) fn find(fingerprint: Option<Fingerprint>) -> Option<&'static Build> {
    let fingerprint = fingerprint?;
    BUILDS.iter().find(|b| b.fingerprint == fingerprint)
}

/// The value of an engine field at a fixed offset; offsets of engine
/// internals that reflection does not describe live here, next to the
/// build table that vouches for them.
pub(crate) mod engine {
    /// A query context: `*ctx` is the query descriptor, the low dword of
    /// `ctx[1]` the 1-based current row.
    pub const CONTEXT_ROW: usize = 8;
    /// Descriptor: pointer to the per-row entity records (16 bytes each,
    /// entity id in the first dword).
    pub const DESCRIPTOR_ENTITY_RECORDS: usize = 0x828;
    pub const ENTITY_RECORD_SIZE: usize = 0x10;
}
