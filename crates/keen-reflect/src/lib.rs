//! Reflection data and ECS system records of Keen's Holistic engine, read
//! from an Enshrouded server or client image.
//!
//! The engine keeps, in `.rdata`, a descriptor for every reflected type and
//! a record for every ECS system. Both are found by the names they carry, so
//! a lookup survives a game update that moves code and data around. The
//! layouts below follow the analysis in Ember (zachthedev/enshrouded-ember,
//! MIT), checked against Enshrouded 1.0.
//!
//! A string in these tables is a slice: a pointer into `.rdata` and a
//! length, with the bytes NUL-terminated right at the length.

pub mod pe;

use std::collections::HashMap;

pub use pe::{Fingerprint, Image, ImageError, Layout};

/// A reflected type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeInfo {
    /// Address of the descriptor.
    pub va: u64,
    /// Full name, such as `keen::ecs::TriggerShape`.
    pub name: String,
    pub size: u32,
    pub align: u16,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub offset: u64,
    /// Full name of the field's type, when it is reflected.
    pub type_name: Option<String>,
}

/// An ECS system record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct System {
    pub name: String,
    /// The system's update function; 0 when this binary does not run it
    /// (a client-only system in the server, for instance).
    pub entry: u64,
    /// Short names of the components the system triggers on, reads and
    /// writes.
    pub triggers: Vec<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LookupError {
    Missing(String),
    /// More than one candidate: a lookup never guesses.
    Ambiguous(String),
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupError::Missing(name) => write!(f, "{name} not found"),
            LookupError::Ambiguous(name) => write!(f, "{name} is ambiguous"),
        }
    }
}

const DESCRIPTOR_SHORT_NAME: u64 = 0x40;
const DESCRIPTOR_FULL_NAME: u64 = 0x60;
const DESCRIPTOR_SIZE_ALIGN: u64 = 0x80;
const DESCRIPTOR_FIELD_COUNT: u64 = 0x88;
const DESCRIPTOR_HASH: u64 = 0x90;
const DESCRIPTOR_FIELDS: u64 = 0x98;
const FIELD_RECORD: u64 = 48;
const FIELD_TYPE: u64 = 16;
const FIELD_OFFSET: u64 = 24;
const SYSTEM_ENTRY: u64 = 0x10;
/// Component lists in a system record: triggers, reads, writes.
const SYSTEM_LISTS: [u64; 3] = [0x18, 0x38, 0x58];
const MAX_NAME: u64 = 250;
const MAX_FIELDS: u32 = 1024;
const MAX_LIST: u64 = 64;

/// FNV-1a, 32 bits. A descriptor's hash carries it of the full name in its
/// low half.
pub fn fnv1a32(text: &str) -> u32 {
    text.bytes()
        .fold(0x811c_9dc5, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

pub struct Reflection {
    types: HashMap<String, Vec<TypeInfo>>,
    systems: HashMap<String, Vec<System>>,
}

impl Reflection {
    /// Scans the image's `.rdata`.
    pub fn scan(image: &Image) -> Reflection {
        let Some(rdata) = image.section(".rdata") else {
            return Reflection {
                types: HashMap::new(),
                systems: HashMap::new(),
            };
        };
        let rdata = image.range(rdata);
        let text = image
            .section(".text")
            .map(|s| image.range(s))
            .unwrap_or(0..0);
        let reader = Reader {
            image,
            rdata: rdata.clone(),
        };

        let mut types: HashMap<String, Vec<TypeInfo>> = HashMap::new();
        let mut systems: HashMap<String, Vec<System>> = HashMap::new();
        let mut va = rdata.start;
        while va + 16 <= rdata.end {
            if let Some(name) = reader.slice(va) {
                if let Some(info) = reader.descriptor(va - DESCRIPTOR_SHORT_NAME) {
                    types.entry(info.name.clone()).or_default().push(info);
                } else if is_system_name(&name)
                    && let Some(system) = reader.system(va, name, &text)
                {
                    systems.entry(system.name.clone()).or_default().push(system);
                }
            }
            va += 8;
        }
        Reflection { types, systems }
    }

    pub fn type_info(&self, name: &str) -> Result<&TypeInfo, LookupError> {
        unique(self.types.get(name), name)
    }

    pub fn system(&self, name: &str) -> Result<&System, LookupError> {
        unique(self.systems.get(name), name)
    }

    /// The offset of `field` in type `type_name`.
    pub fn field_offset(&self, type_name: &str, field: &str) -> Result<u64, LookupError> {
        let info = self.type_info(type_name)?;
        let mut found = info.fields.iter().filter(|f| f.name == field);
        let path = format!("{type_name}.{field}");
        match (found.next(), found.next()) {
            (Some(f), None) => Ok(f.offset),
            (None, _) => Err(LookupError::Missing(path)),
            (Some(_), Some(_)) => Err(LookupError::Ambiguous(path)),
        }
    }

    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    pub fn system_count(&self) -> usize {
        self.systems.len()
    }
}

fn unique<'a, T>(found: Option<&'a Vec<T>>, name: &str) -> Result<&'a T, LookupError> {
    match found.map(Vec::as_slice) {
        Some([one]) => Ok(one),
        Some([_, _, ..]) => Err(LookupError::Ambiguous(name.to_owned())),
        _ => Err(LookupError::Missing(name.to_owned())),
    }
}

fn is_system_name(name: &str) -> bool {
    name.len() >= 4
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

struct Reader<'i, 'a> {
    image: &'i Image<'a>,
    rdata: std::ops::Range<u64>,
}

impl Reader<'_, '_> {
    /// The string slice at `va`, if one is there.
    fn slice(&self, va: u64) -> Option<String> {
        let ptr = self.image.u64(va)?;
        let len = self.image.u64(va + 8)?;
        if !self.rdata.contains(&ptr) || len == 0 || len >= MAX_NAME {
            return None;
        }
        let bytes = self.image.read(ptr, len + 1)?;
        let (text, end) = bytes.split_at(len as usize);
        if end != [0] || text.contains(&0) {
            return None;
        }
        Some(text.iter().map(|&b| b as char).collect())
    }

    fn descriptor(&self, va: u64) -> Option<TypeInfo> {
        self.slice(va + DESCRIPTOR_SHORT_NAME)?;
        let name = self.slice(va + DESCRIPTOR_FULL_NAME)?;
        if self.image.u32(va + DESCRIPTOR_HASH)? != fnv1a32(&name) {
            return None;
        }
        let size = self.image.u32(va + DESCRIPTOR_SIZE_ALIGN)?;
        let align = self.image.u16(va + DESCRIPTOR_SIZE_ALIGN + 4)?;
        let count = self.image.u32(va + DESCRIPTOR_FIELD_COUNT)?;
        let mut fields = Vec::new();
        if count > 0 && count <= MAX_FIELDS {
            let table = self.image.u64(va + DESCRIPTOR_FIELDS)?;
            for i in 0..count as u64 {
                let record = table + i * FIELD_RECORD;
                let Some(field_name) = self.slice(record) else {
                    break;
                };
                let type_ptr = self.image.u64(record + FIELD_TYPE).unwrap_or(0);
                let type_name = type_ptr
                    .checked_sub(DESCRIPTOR_SHORT_NAME)
                    .and_then(|d| self.slice(d + DESCRIPTOR_FULL_NAME));
                fields.push(Field {
                    name: field_name,
                    offset: self.image.u64(record + FIELD_OFFSET)?,
                    type_name,
                });
            }
        }
        Some(TypeInfo {
            va,
            name,
            size,
            align,
            fields,
        })
    }

    fn system(&self, va: u64, name: String, text: &std::ops::Range<u64>) -> Option<System> {
        let entry = self.image.u64(va + SYSTEM_ENTRY)?;
        if entry != 0 && !text.contains(&entry) {
            return None;
        }
        let [triggers, reads, writes] = SYSTEM_LISTS.map(|at| self.list(va + at));
        if triggers.is_empty() && reads.is_empty() && writes.is_empty() {
            return None;
        }
        Some(System {
            name,
            entry,
            triggers,
            reads,
            writes,
        })
    }

    fn list(&self, va: u64) -> Vec<String> {
        let (Some(ptr), Some(count)) = (self.image.u64(va), self.image.u64(va + 8)) else {
            return Vec::new();
        };
        if ptr == 0 || count == 0 || count > MAX_LIST || !self.rdata.contains(&ptr) {
            return Vec::new();
        }
        (0..count).map_while(|i| self.slice(ptr + i * 16)).collect()
    }
}
