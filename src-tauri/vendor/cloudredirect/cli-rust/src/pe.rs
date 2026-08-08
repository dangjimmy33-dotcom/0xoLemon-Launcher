// PE section parsing and RVA/file-offset conversion.

/// IMAGE_SCN_MEM_EXECUTE
const SCN_MEM_EXECUTE: u32 = 0x2000_0000;

#[derive(Clone, Debug)]
pub struct PeSection {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
    pub characteristics: u32,
}

impl PeSection {
    pub fn is_executable(&self) -> bool {
        self.characteristics & SCN_MEM_EXECUTE != 0
    }

    /// Parse the section table out of a PE image. Returns an empty vec on any
    /// malformed/short input (mirrors the C# which returns Array.Empty).
    pub fn parse(pe: &[u8]) -> Vec<PeSection> {
        if pe.len() < 64 {
            return Vec::new();
        }
        let pe_off = read_i32(pe, 0x3C);
        if pe_off < 0 || pe_off as usize + 24 > pe.len() {
            return Vec::new();
        }
        let pe_off = pe_off as usize;
        if pe[pe_off] != b'P' || pe[pe_off + 1] != b'E' {
            return Vec::new();
        }

        let num_sections = read_u16(pe, pe_off + 6) as usize;
        if num_sections > 96 {
            return Vec::new();
        }
        let opt_size = read_u16(pe, pe_off + 20) as usize;
        let first_section = pe_off + 24 + opt_size;
        if first_section > pe.len() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let off = first_section + i * 40;
            if off + 40 > pe.len() {
                break;
            }
            // Section name: up to 8 bytes, NUL-terminated, ASCII.
            let mut name_end = 0usize;
            for j in 0..8 {
                if pe[off + j] == 0 {
                    break;
                }
                name_end = j + 1;
            }
            let name = String::from_utf8_lossy(&pe[off..off + name_end]).into_owned();

            result.push(PeSection {
                name,
                virtual_size: read_u32(pe, off + 8),
                virtual_address: read_u32(pe, off + 12),
                raw_size: read_u32(pe, off + 16),
                raw_offset: read_u32(pe, off + 20),
                characteristics: read_u32(pe, off + 36),
            });
        }
        result
    }

    pub fn find<'a>(sections: &'a [PeSection], name: &str) -> Option<&'a PeSection> {
        sections.iter().find(|s| s.name == name)
    }

    /// File offset -> RVA, returns -1 if outside any section.
    pub fn file_offset_to_rva(sections: &[PeSection], file_offset: i64) -> i64 {
        if file_offset < 0 {
            return -1;
        }
        let fo = file_offset as u32;
        for s in sections {
            if fo >= s.raw_offset && fo - s.raw_offset < s.raw_size {
                return (s.virtual_address + (fo - s.raw_offset)) as i64;
            }
        }
        -1
    }

    /// RVA -> file offset, returns -1 if in BSS or outside any section.
    pub fn rva_to_file_offset(sections: &[PeSection], rva: i64) -> i64 {
        if rva < 0 {
            return -1;
        }
        let r = rva as u32;
        for s in sections {
            let size = s.virtual_size.max(s.raw_size);
            if r >= s.virtual_address && r - s.virtual_address < size {
                let offset_in_section = r - s.virtual_address;
                if offset_in_section >= s.raw_size {
                    return -1; // BSS / zero-fill, no file backing
                }
                return (s.raw_offset + offset_in_section) as i64;
            }
        }
        -1
    }

    pub fn find_by_file_offset(sections: &[PeSection], file_offset: i64) -> Option<&PeSection> {
        if file_offset < 0 {
            return None;
        }
        let fo = file_offset as u32;
        sections
            .iter()
            .find(|s| fo >= s.raw_offset && fo < s.raw_offset + s.raw_size)
    }

    /// Find the section containing an RVA (using VirtualAddress + VirtualSize).
    /// Unlike rva_to_file_offset, succeeds for BSS regions with no file backing.
    pub fn find_by_rva(sections: &[PeSection], rva: i64) -> Option<&PeSection> {
        if rva < 0 {
            return None;
        }
        let r = rva as u32;
        sections.iter().find(|s| {
            let size = s.virtual_size.max(s.raw_size);
            r >= s.virtual_address && r - s.virtual_address < size
        })
    }
}

#[inline]
fn read_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn read_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
fn read_i32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parse the real steamclient64.dll (if present) and sanity-check the section
    // table: must contain .text, the section table must be ordered, and RVA<->file
    // round-trips inside .text must be consistent.
    #[test]
    fn parses_steamclient() {
        let candidates = [
            r"C:\Games\Steam\steamclient64.dll",
            r"C:\Program Files (x86)\Steam\steamclient64.dll",
        ];
        let path = candidates.iter().find(|p| std::path::Path::new(p).is_file());
        let Some(path) = path else {
            eprintln!("steamclient64.dll not found; skipping parity test");
            return;
        };
        let data = std::fs::read(path).expect("read dll");
        let sections = PeSection::parse(&data);
        assert!(!sections.is_empty(), "no sections parsed");

        let text = PeSection::find(&sections, ".text").expect(".text section present");
        assert!(text.is_executable(), ".text must be executable");
        assert!(text.raw_size > 0);

        // round-trip a file offset in the middle of .text
        let fo = (text.raw_offset + text.raw_size / 2) as i64;
        let rva = PeSection::file_offset_to_rva(&sections, fo);
        assert!(rva > 0);
        let back = PeSection::rva_to_file_offset(&sections, rva);
        assert_eq!(back, fo, "RVA<->file offset round-trip mismatch");

        eprintln!("parsed {} sections from {}", sections.len(), path);
        for s in &sections {
            eprintln!(
                "  {:8} VA={:#010x} VSize={:#x} Raw={:#x} RawSize={:#x} exec={}",
                s.name, s.virtual_address, s.virtual_size, s.raw_offset, s.raw_size, s.is_executable()
            );
        }
    }
}
