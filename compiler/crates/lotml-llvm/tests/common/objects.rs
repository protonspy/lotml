//! What a built executable or library imports and exports, read from its bytes by a PE and an ELF
//! reader with no dependency (plans/target-parity-assurance.md 3.2): every read is bounded and
//! every offset a file gives is added and multiplied checked, so a file cut short or lying is an
//! error, never a panic or a read past its end.

#![allow(dead_code)]

/// The libraries a file loads (PE imports, ELF `DT_NEEDED`) and the functions it exports (the PE
/// export directory, the ELF dynamic symbols defined, global and of default visibility).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Linkage {
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

/// `bytes`, read as a PE or an ELF file.
pub fn linkage(bytes: &[u8]) -> Result<Linkage, String> {
    match bytes.get(..4) {
        Some([0x7f, b'E', b'L', b'F']) => elf(bytes),
        Some([b'M', b'Z', ..]) => pe(bytes),
        Some(_) => Err("neither a PE nor an ELF file".into()),
        None => Err("too short to be an executable".into()),
    }
}

/// Little-endian reads that fail rather than run past the end.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn slice(&self, at: u64, len: u64) -> Result<&[u8], String> {
        let start = usize::try_from(at).map_err(|_| "an offset too large".to_string())?;
        let len = usize::try_from(len).map_err(|_| "a size too large".to_string())?;
        let end = start.checked_add(len).ok_or("an offset that overflows")?;
        self.0
            .get(start..end)
            .ok_or_else(|| format!("cut short: {len} bytes at {start} past its {} bytes", self.0.len()))
    }

    fn u8(&self, at: u64) -> Result<u8, String> {
        Ok(self.slice(at, 1)?[0])
    }

    fn u16(&self, at: u64) -> Result<u16, String> {
        let b = self.slice(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&self, at: u64) -> Result<u32, String> {
        let b = self.slice(at, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&self, at: u64) -> Result<u64, String> {
        let b = self.slice(at, 8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// The NUL-terminated string at `at`, at most 4096 bytes.
    fn string(&self, at: u64) -> Result<String, String> {
        self.string_within(at, 4096)
    }

    /// The NUL-terminated string at `at`, ending within `room` bytes.
    fn string_within(&self, at: u64, room: u64) -> Result<String, String> {
        let start = usize::try_from(at).map_err(|_| "an offset too large".to_string())?;
        let rest = self.0.get(start..).ok_or_else(|| format!("a name at {start}, past the end"))?;
        let room = usize::try_from(room.min(4096)).unwrap_or(4096);
        let end = rest.iter().take(room).position(|b| *b == 0).ok_or("a name with no end")?;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

/// Entries read from a table are capped, so a corrupt count cannot ask for billions of reads.
const MOST: u64 = 1 << 16;

fn pe(data: &[u8]) -> Result<Linkage, String> {
    let b = Bytes(data);
    let header = u64::from(b.u32(0x3c)?);
    if b.slice(header, 4)? != b"PE\0\0" {
        return Err("no PE signature".into());
    }
    let coff = header + 4;
    let sections = u64::from(b.u16(coff + 2)?);
    let optional_size = u64::from(b.u16(coff + 16)?);
    let optional = coff + 20;
    let directories = match b.u16(optional)? {
        0x10b => optional + 96,
        0x20b => optional + 112,
        magic => return Err(format!("an optional header of magic {magic:#x}")),
    };
    let table = optional + optional_size;
    let mut spans = Vec::new();
    for k in 0..sections.min(MOST) {
        let at = table + 40 * k;
        let virtual_size = u64::from(b.u32(at + 8)?);
        let address = u64::from(b.u32(at + 12)?);
        let raw_size = u64::from(b.u32(at + 16)?);
        let raw = u64::from(b.u32(at + 20)?);
        spans.push((address, virtual_size.max(raw_size), raw));
        if raw_size != 0 {
            b.slice(raw, raw_size)?;
        }
    }
    let offset = |rva: u64| -> Result<u64, String> {
        spans
            .iter()
            .find(|(address, size, _)| rva >= *address && rva < address + size)
            .map(|(address, _, raw)| rva - address + raw)
            .ok_or_else(|| format!("an address {rva:#x} in no section"))
    };
    let mut linkage = Linkage::default();
    let exports = u64::from(b.u32(directories)?);
    if exports != 0 {
        let directory = offset(exports)?;
        let count = u64::from(b.u32(directory + 24)?);
        let names = offset(u64::from(b.u32(directory + 32)?))?;
        for k in 0..count.min(MOST) {
            linkage.exports.push(b.string(offset(u64::from(b.u32(names + 4 * k)?))?)?);
        }
    }
    let directory_count = u64::from(b.u32(directories - 4)?);
    // The import directory (1), and the delay-load one (13): descriptors of 20 and of 32 bytes,
    // the library's name at 12 and at 4, until one names none.
    for (index, size, name_at) in [(1, 20, 12), (13, 32, 4)] {
        if index >= directory_count {
            continue;
        }
        let table = u64::from(b.u32(directories + 8 * index)?);
        if table == 0 {
            continue;
        }
        let mut at = offset(table)?;
        for _ in 0..MOST {
            let name = u64::from(b.u32(at + name_at)?);
            if name == 0 {
                break;
            }
            linkage.imports.push(b.string(offset(name)?)?);
            at += size;
        }
    }
    Ok(linkage)
}

/// `a + b`, or an error where it would overflow: an offset a file gives is never trusted.
fn plus(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "an offset that overflows".to_string())
}

/// `a * b`, or an error where it would overflow.
fn times(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "a size that overflows".to_string())
}

fn elf(data: &[u8]) -> Result<Linkage, String> {
    let b = Bytes(data);
    let wide = match b.u8(4)? {
        1 => false,
        2 => true,
        class => return Err(format!("an ELF class {class}")),
    };
    if b.u8(5)? != 1 {
        return Err("a big-endian ELF file".into());
    }
    let word = |at: u64| -> Result<u64, String> { if wide { b.u64(at) } else { b.u32(at).map(u64::from) } };
    let (sections, size, count) = if wide {
        (b.u64(0x28)?, u64::from(b.u16(0x3a)?), u64::from(b.u16(0x3c)?))
    } else {
        (u64::from(b.u32(0x20)?), u64::from(b.u16(0x2e)?), u64::from(b.u16(0x30)?))
    };
    // The whole table of section headers lies inside the file, or nothing of it is read.
    b.slice(sections, times(size, count)?)?;
    struct Section {
        kind: u32,
        offset: u64,
        size: u64,
        link: u64,
        entry: u64,
    }
    let mut table = Vec::new();
    for k in 0..count.min(MOST) {
        let at = plus(sections, times(size, k)?)?;
        let field = |offset: u64| plus(at, offset);
        let (offset, length, link, entry) = if wide {
            (b.u64(field(0x18)?)?, b.u64(field(0x20)?)?, u64::from(b.u32(field(0x28)?)?), b.u64(field(0x38)?)?)
        } else {
            (word(field(0x10)?)?, word(field(0x14)?)?, u64::from(b.u32(field(0x18)?)?), word(field(0x24)?)?)
        };
        let kind = b.u32(field(4)?)?;
        // SHT_NOBITS (8) and SHT_NULL (0) have no bytes in the file.
        if !matches!(kind, 0 | 8) {
            b.slice(offset, length)?;
        }
        table.push(Section { kind, offset, size: length, link, entry });
    }
    let strings = |s: &Section| -> Result<&Section, String> {
        let link = usize::try_from(s.link).map_err(|_| "a link too large".to_string())?;
        let linked = table.get(link).ok_or_else(|| format!("a link to section {link}, which is not there"))?;
        b.slice(linked.offset, linked.size)?;
        Ok(linked)
    };
    let name = |s: &Section, at: u64| -> Result<String, String> {
        if at >= s.size {
            return Err(format!("a name at {at} past its table of {} bytes", s.size));
        }
        b.string_within(plus(s.offset, at)?, s.size - at)
    };
    let mut linkage = Linkage::default();
    for s in &table {
        match s.kind {
            // SHT_DYNAMIC: tag and value pairs, DT_NEEDED (1) naming a library.
            6 => {
                let entry: u64 = if wide { 16 } else { 8 };
                b.slice(s.offset, s.size)?;
                for k in 0..(s.size / entry).min(MOST) {
                    let at = plus(s.offset, entry * k)?;
                    let tag = word(at)?;
                    if tag == 0 {
                        break;
                    }
                    if tag == 1 {
                        let value = word(plus(at, entry / 2)?)?;
                        linkage.imports.push(name(strings(s)?, value)?);
                    }
                }
            }
            // SHT_DYNSYM: a function defined here, global or weak, of default visibility.
            11 => {
                let least: u64 = if wide { 24 } else { 16 };
                let entry = s.entry.max(least);
                b.slice(s.offset, s.size)?;
                for k in 0..(s.size / entry).min(MOST) {
                    let at = plus(s.offset, entry * k)?;
                    let (info, other, index) = if wide {
                        (b.u8(at + 4)?, b.u8(at + 5)?, b.u16(at + 6)?)
                    } else {
                        (b.u8(at + 12)?, b.u8(at + 13)?, b.u16(at + 14)?)
                    };
                    let (bind, kind) = (info >> 4, info & 0xf);
                    if index != 0 && matches!(bind, 1 | 2) && kind == 2 && other & 3 == 0 {
                        linkage.exports.push(name(strings(s)?, u64::from(b.u32(at)?))?);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(linkage)
}
