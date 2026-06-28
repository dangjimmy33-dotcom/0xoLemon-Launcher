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

    pub fn parse(pe: &[u8]) -> Vec<PeSection> {
        if pe.len() < 64 { return Vec::new(); }
        let pe_off = read_i32(pe, 0x3C);
        if pe_off < 0 || pe_off as usize + 24 > pe.len() { return Vec::new(); }
        let pe_off = pe_off as usize;
        if pe[pe_off] != b'P' || pe[pe_off + 1] != b'E' { return Vec::new(); }

        let num_sections = read_u16(pe, pe_off + 6) as usize;
        if num_sections > 96 { return Vec::new(); }
        let opt_size = read_u16(pe, pe_off + 20) as usize;
        let first_section = pe_off + 24 + opt_size;
        if first_section > pe.len() { return Vec::new(); }

        let mut result = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let off = first_section + i * 40;
            if off + 40 > pe.len() { break; }
            let mut name_end = 0usize;
            for j in 0..8 {
                if pe[off + j] == 0 { break; }
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

    pub fn file_offset_to_rva(sections: &[PeSection], file_offset: i64) -> i64 {
        if file_offset < 0 { return -1; }
        let fo = file_offset as u32;
        for s in sections {
            if fo >= s.raw_offset && fo - s.raw_offset < s.raw_size {
                return (s.virtual_address + (fo - s.raw_offset)) as i64;
            }
        }
        -1
    }

    pub fn rva_to_file_offset(sections: &[PeSection], rva: i64) -> i64 {
        if rva < 0 { return -1; }
        let r = rva as u32;
        for s in sections {
            let size = s.virtual_size.max(s.raw_size);
            if r >= s.virtual_address && r - s.virtual_address < size {
                let offset_in_section = r - s.virtual_address;
                if offset_in_section >= s.raw_size { return -1; }
                return (s.raw_offset + offset_in_section) as i64;
            }
        }
        -1
    }

    pub fn find_by_file_offset(sections: &[PeSection], file_offset: i64) -> Option<&PeSection> {
        if file_offset < 0 { return None; }
        let fo = file_offset as u32;
        sections.iter().find(|s| fo >= s.raw_offset && fo < s.raw_offset + s.raw_size)
    }

    pub fn find_by_rva(sections: &[PeSection], rva: i64) -> Option<&PeSection> {
        if rva < 0 { return None; }
        let r = rva as u32;
        sections.iter().find(|s| {
            let size = s.virtual_size.max(s.raw_size);
            r >= s.virtual_address && r - s.virtual_address < size
        })
    }
}

#[inline]
fn read_u16(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
#[inline]
fn read_u32(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }
#[inline]
fn read_i32(b: &[u8], o: usize) -> i32 { i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }
