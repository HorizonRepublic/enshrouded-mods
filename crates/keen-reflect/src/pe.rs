//! Just enough of the PE format to address an image by virtual address,
//! whether it is a file on disk or a module mapped into the process.

/// How the image's bytes are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A file: sections sit at their raw file offsets.
    File,
    /// A loaded module: byte `i` is at virtual address `base + i`.
    Mapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// Relative to the image base.
    pub rva: u64,
    pub virtual_size: u64,
    pub raw_offset: u64,
    pub raw_size: u64,
}

/// The build fingerprint: the CodeView debug record's GUID (in file order)
/// and age. The version string is not unique per build; this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    pub guid: [u8; 16],
    pub age: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    NotPe,
    Truncated,
    NotX64,
}

pub struct Image<'a> {
    bytes: &'a [u8],
    layout: Layout,
    /// Where the image is, or for a file, where it would be at its
    /// preferred base. Pointers inside the image are relative to this.
    pub base: u64,
    pub sections: Vec<Section>,
    pub fingerprint: Option<Fingerprint>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

impl<'a> Image<'a> {
    /// Parses `bytes`. For `Layout::Mapped`, `base` is where the module is
    /// loaded; for `Layout::File` pass `None` to use the preferred base.
    pub fn parse(bytes: &'a [u8], layout: Layout, base: Option<u64>) -> Result<Self, ImageError> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err(ImageError::NotPe);
        }
        let pe = u32_at(bytes, 0x3c).ok_or(ImageError::Truncated)? as usize;
        if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(ImageError::NotPe);
        }
        let section_count = u16_at(bytes, pe + 6).ok_or(ImageError::Truncated)? as usize;
        let optional_size = u16_at(bytes, pe + 20).ok_or(ImageError::Truncated)? as usize;
        let optional = pe + 24;
        if u16_at(bytes, optional) != Some(0x20b) {
            return Err(ImageError::NotX64);
        }
        let preferred = u64_at(bytes, optional + 24).ok_or(ImageError::Truncated)?;

        let mut sections = Vec::with_capacity(section_count);
        for i in 0..section_count {
            let at = optional + optional_size + i * 40;
            let raw = bytes.get(at..at + 40).ok_or(ImageError::Truncated)?;
            let name_len = raw[..8].iter().position(|&c| c == 0).unwrap_or(8);
            sections.push(Section {
                name: String::from_utf8_lossy(&raw[..name_len]).into_owned(),
                virtual_size: u32_at(raw, 8).unwrap() as u64,
                rva: u32_at(raw, 12).unwrap() as u64,
                raw_size: u32_at(raw, 16).unwrap() as u64,
                raw_offset: u32_at(raw, 20).unwrap() as u64,
            });
        }

        let mut image = Image {
            bytes,
            layout,
            base: base.unwrap_or(preferred),
            sections,
            fingerprint: None,
        };
        // Data directory 6 is the debug directory.
        let debug_rva = u32_at(bytes, optional + 112 + 6 * 8).unwrap_or(0) as u64;
        let debug_size = u32_at(bytes, optional + 112 + 6 * 8 + 4).unwrap_or(0) as u64;
        image.fingerprint = image.codeview(debug_rva, debug_size);
        Ok(image)
    }

    fn codeview(&self, rva: u64, size: u64) -> Option<Fingerprint> {
        for entry in 0..size / 28 {
            let record = self.read(self.base + rva + entry * 28, 28)?;
            if u32_at(record, 12)? != 2 {
                continue; // not CodeView
            }
            let data_rva = u32_at(record, 20)? as u64;
            let cv = self.read(self.base + data_rva, 24)?;
            if &cv[..4] != b"RSDS" {
                continue;
            }
            return Some(Fingerprint {
                guid: cv[4..20].try_into().ok()?,
                age: u32_at(cv, 20)?,
            });
        }
        None
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The virtual address range a section's readable bytes cover.
    pub fn range(&self, section: &Section) -> std::ops::Range<u64> {
        let len = match self.layout {
            Layout::File => section.raw_size,
            Layout::Mapped => section.virtual_size,
        };
        self.base + section.rva..self.base + section.rva + len
    }

    /// `len` bytes at virtual address `va`, if they lie inside one section.
    pub fn read(&self, va: u64, len: u64) -> Option<&'a [u8]> {
        let rva = va.checked_sub(self.base)?;
        let offset = match self.layout {
            Layout::Mapped => {
                // Headers and sections alike are mapped at their RVA.
                rva
            }
            Layout::File => {
                let s = self
                    .sections
                    .iter()
                    .find(|s| s.rva <= rva && rva < s.rva + s.raw_size)?;
                if rva + len > s.rva + s.raw_size {
                    return None;
                }
                s.raw_offset + (rva - s.rva)
            }
        };
        let start = usize::try_from(offset).ok()?;
        self.bytes
            .get(start..start.checked_add(usize::try_from(len).ok()?)?)
    }

    pub fn u64(&self, va: u64) -> Option<u64> {
        u64_at(self.read(va, 8)?, 0)
    }

    pub fn u32(&self, va: u64) -> Option<u32> {
        u32_at(self.read(va, 4)?, 0)
    }

    pub fn u16(&self, va: u64) -> Option<u16> {
        u16_at(self.read(va, 2)?, 0)
    }
}
