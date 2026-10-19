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
pub enum Role {
    Server,
    Client,
}

/// Addresses relative to the image base.
#[derive(Debug)]
pub struct Build {
    pub name: &'static str,
    pub role: Role,
    pub fingerprint: Fingerprint,
    /// `(system name, entry)`; each must match reflection.
    pub systems: &'static [(&'static str, u64)],
    /// The ECS query iterator: `init(ctx, iterator, size)` and
    /// `next(ctx, iterator, size) -> bool`.
    pub iterator_init: u64,
    pub iterator_next: u64,
    /// `find(out, attribute, registry)`: `out` gets an error byte and the
    /// attribute's definition.
    pub attribute_find: u64,
    /// `size(out, balancing, level)`: `out` gets the level's full zone size
    /// as three floats and a valid byte at 0xc.
    pub zone_size: u64,
    /// `mark(tracker, stamp_a, stamp_b, entity, type, flags)`: marks a
    /// written component changed, which replication follows.
    pub mark_changed: u64,
    /// `seam(zones, corners, margin) -> bool`: both corners of an object in
    /// one zone.
    pub seam_check: u64,
    /// `max_altars(balancing, flame_level) -> u8`: altars allowed at a Flame
    /// level, capped at 16.
    pub max_altars: u64,
    /// `server_change(provider, request)`: the client is asked to join the
    /// server whose `ip:port` starts `request`; 0 on the server.
    pub server_change: u64,
    /// `lookup(bases, &npc) -> entity`: the base system's survivor map; 0 on
    /// the client.
    pub npc_lookup: u64,
    /// Where the spawn path's survivor lookup returns to.
    pub npc_spawn_return: u64,
    /// Where `unique_npc_spawns`'s two survivor lookups return to.
    pub npc_unique_returns: [u64; 2],
    /// The second copy of the survivor lookup, used by the cursor check.
    pub npc_lookup_cursor: u64,
    /// Where `npc_bed` and `npc_bed_init` lookups return to.
    pub npc_bed_return: u64,
    pub npc_bed_init_return: u64,
    /// Where the cursor check's two lookups return to.
    pub npc_cursor_returns: [u64; 2],
    /// `move(behavior_ctx, ...)`: an NPC takes a move command; every copy
    /// of a survivor runs it for one command.
    pub npc_move: u64,
    /// `component(ctx, &type, entity) -> pointer`: any component of any
    /// entity, read only.
    pub component_read: u64,
    /// Where `player_assign_npc`'s two lookups return to: the survivor that
    /// gets the bed, then the bed's previous occupant.
    pub npc_assign_returns: [u64; 2],
    /// Where `npc_active_input`'s lookup, which sets the roaming range,
    /// returns to.
    pub npc_roaming_return: u64,
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

pub const BUILDS: &[Build] = &[
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
        attribute_find: 0x984940,
        zone_size: 0x927560,
        mark_changed: 0x666a90,
        seam_check: 0x93e210,
        max_altars: 0x931860,
        server_change: 0,
        npc_lookup: 0x202e80,
        npc_spawn_return: 0x405cb,
        npc_unique_returns: [0x1f4929, 0x1f499d],
        npc_lookup_cursor: 0x202e30,
        npc_bed_return: 0x1f29ff,
        npc_bed_init_return: 0x1f2d76,
        npc_cursor_returns: [0x99fe8, 0x99ff3],
        npc_move: 0x1eca40,
        component_read: 0x346c0,
        npc_assign_returns: [0xbf541, 0xbf5e4],
        npc_roaming_return: 0x1f24f8,
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
        attribute_find: 0x121bea0,
        zone_size: 0xe72e50,
        mark_changed: 0x9c5df0,
        seam_check: 0xe899f0,
        max_altars: 0xe7d070,
        server_change: 0x107d200,
        npc_lookup: 0,
        npc_spawn_return: 0,
        npc_unique_returns: [0, 0],
        npc_lookup_cursor: 0,
        npc_bed_return: 0,
        npc_bed_init_return: 0,
        npc_cursor_returns: [0, 0],
        npc_move: 0,
        component_read: 0,
        npc_assign_returns: [0, 0],
        npc_roaming_return: 0,
    },
];

pub fn find(fingerprint: Option<Fingerprint>) -> Option<&'static Build> {
    let fingerprint = fingerprint?;
    BUILDS.iter().find(|b| b.fingerprint == fingerprint)
}

/// The value of an engine field at a fixed offset; offsets of engine
/// internals that reflection does not describe live here, next to the
/// build table that vouches for them.
pub mod engine {
    /// A query context: `*ctx` is the query descriptor, the low dword of
    /// `ctx[1]` the 1-based current row.
    pub const CONTEXT_ROW: usize = 8;
    /// Descriptor: pointer to the per-row entity records (16 bytes each,
    /// entity id in the first dword).
    pub const DESCRIPTOR_ENTITY_RECORDS: usize = 0x828;
    pub const ENTITY_RECORD_SIZE: usize = 0x10;

    /// The attribute that holds an altar's level.
    pub const ALTAR_LEVEL_ATTRIBUTE: u32 = 0x724b_f568;
    /// `ecs::Level`: set id at 0, data offset (u16) at 4, flags at 6 (bit 0:
    /// has data).
    pub const LEVEL_DATA_OFFSET: usize = 4;
    pub const LEVEL_FLAGS: usize = 6;
    /// An attribute definition: set id at 0, slot (u16) at 0xc.
    pub const ATTRIBUTE_SLOT: usize = 0xc;

    /// The zone list the placement checks take: entries at 0x10, count at
    /// 0x18.
    pub const ZONE_LIST_ENTRIES: usize = 0x10;
    pub const ZONE_LIST_COUNT: usize = 0x18;
    /// A zone entry: min and max as three i64 each, then the base GUID,
    /// shape at 0x44 (0: box) and an ignore flag at 0x46.
    pub const ZONE_ENTRY_SIZE: usize = 0x48;
    pub const ZONE_ENTRY_SHAPE: usize = 0x44;
    pub const ZONE_ENTRY_IGNORED: usize = 0x46;
    /// The object's corners: two points of three i64.
    pub const CORNER_SIZE: usize = 0x18;
}
