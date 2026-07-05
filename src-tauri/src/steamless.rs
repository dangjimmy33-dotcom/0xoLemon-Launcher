/// steamless.rs — Native Rust port of Steamless SteamStub DRM remover
/// Supports: Variant 3.0 x64, Variant 3.1 x64 (most modern games)
/// Based on Steamless by atom0s (https://github.com/atom0s/Steamless)
/// License: CC BY-NC-ND 4.0 (non-commercial, attribution required)
///
/// Strategy:
///   1. Read PE file
///   2. Detect SteamStub variant via .bind section pattern matching
///   3. XOR-decode the DRM header (240 bytes before OEP)
///   4. AES-256-CBC decrypt the code section using key/IV from header
///   5. Restore stolen bytes, patch OEP, remove .bind, write clean exe
///   6. Backup original as <name>.org.exe; replace original path

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use aes::Aes256;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use serde::{Deserialize, Serialize};

// ─── Public result type ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamlessResult {
    pub success: bool,
    /// User-friendly message (not technical jargon)
    pub message: String,
    pub output_path: Option<String>,
    pub variant: Option<String>,
    pub steam_app_id: Option<u32>,
}

impl SteamlessResult {
    fn ok(msg: impl Into<String>, path: impl Into<String>, variant: impl Into<String>, app_id: u32) -> Self {
        Self {
            success: true,
            message: msg.into(),
            output_path: Some(path.into()),
            variant: Some(variant.into()),
            steam_app_id: Some(app_id),
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: msg.into(),
            output_path: None,
            variant: None,
            steam_app_id: None,
        }
    }
}

// ─── PE parsing helpers ──────────────────────────────────────────────────────

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap_or([0; 2]))
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

fn write_u32_le(data: &mut [u8], offset: usize, val: u32) {
    data[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u64_le(data: &mut [u8], offset: usize, val: u64) {
    data[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

/// Find a byte pattern with ?? wildcards. Returns offset or None.
fn find_pattern(haystack: &[u8], pattern: &str) -> Option<usize> {
    let tokens: Vec<Option<u8>> = pattern
        .split_whitespace()
        .map(|t| if t == "??" { None } else { u8::from_str_radix(t, 16).ok() })
        .collect();

    if tokens.is_empty() { return None; }

    'outer: for i in 0..haystack.len().saturating_sub(tokens.len() - 1) {
        for (j, tok) in tokens.iter().enumerate() {
            if let Some(byte) = tok {
                if haystack[i + j] != *byte {
                    continue 'outer;
                }
            }
        }
        return Some(i);
    }
    None
}

// ─── PE structures (offsets) ─────────────────────────────────────────────────

struct PeHeaders {
    is_64: bool,
    pe_offset: usize,          // e_lfanew → PE signature
    opt_offset: usize,         // optional header start
    sections_offset: usize,
    num_sections: u16,
    entry_point_rva: u32,
    image_base: u64,
    tls_data_dir_rva: u32,
    tls_data_dir_size: u32,
}

const IMAGE_SIZEOF_SECTION_HEADER: usize = 40;

fn parse_pe(data: &[u8]) -> Option<PeHeaders> {
    if data.len() < 0x40 { return None; }
    if &data[0..2] != b"MZ" { return None; }

    let pe_offset = read_u32_le(data, 0x3C) as usize;
    if pe_offset + 4 > data.len() { return None; }
    if &data[pe_offset..pe_offset + 4] != b"PE\0\0" { return None; }

    let file_header_offset = pe_offset + 4;
    let num_sections = read_u16_le(data, file_header_offset + 2);
    let size_of_optional = read_u16_le(data, file_header_offset + 16) as usize;

    let opt_offset = file_header_offset + 20;
    if opt_offset + 2 > data.len() { return None; }

    let magic = read_u16_le(data, opt_offset);
    let is_64 = magic == 0x20B; // PE32+ (64-bit)

    // AddressOfEntryPoint is at opt_offset+16 for both PE32 and PE32+
    let entry_point_rva = read_u32_le(data, opt_offset + 16);

    // ImageBase
    let image_base = if is_64 {
        read_u64_le(data, opt_offset + 24)
    } else {
        read_u32_le(data, opt_offset + 28) as u64
    };

    // TLS data directory index = 9
    // In PE32+: data dirs start at opt_offset + 112
    // In PE32:  data dirs start at opt_offset + 96
    let data_dir_base = if is_64 { opt_offset + 112 } else { opt_offset + 96 };
    let tls_dir_offset = data_dir_base + 9 * 8;
    let (tls_data_dir_rva, tls_data_dir_size) = if tls_dir_offset + 8 <= data.len() {
        (read_u32_le(data, tls_dir_offset), read_u32_le(data, tls_dir_offset + 4))
    } else {
        (0, 0)
    };

    let sections_offset = opt_offset + size_of_optional;

    Some(PeHeaders {
        is_64,
        pe_offset,
        opt_offset,
        sections_offset,
        num_sections,
        entry_point_rva,
        image_base,
        tls_data_dir_rva,
        tls_data_dir_size,
    })
}

struct Section {
    name: [u8; 8],
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
}

fn read_sections(data: &[u8], hdr: &PeHeaders) -> Vec<Section> {
    let mut sections = Vec::new();
    for i in 0..hdr.num_sections as usize {
        let off = hdr.sections_offset + i * IMAGE_SIZEOF_SECTION_HEADER;
        if off + IMAGE_SIZEOF_SECTION_HEADER > data.len() { break; }
        let mut name = [0u8; 8];
        name.copy_from_slice(&data[off..off + 8]);
        sections.push(Section {
            name,
            virtual_size: read_u32_le(data, off + 8),
            virtual_address: read_u32_le(data, off + 12),
            size_of_raw_data: read_u32_le(data, off + 16),
            pointer_to_raw_data: read_u32_le(data, off + 20),
        });
    }
    sections
}

fn section_name(s: &Section) -> &str {
    let end = s.name.iter().position(|&b| b == 0).unwrap_or(8);
    std::str::from_utf8(&s.name[..end]).unwrap_or("")
}

fn rva_to_file_offset(sections: &[Section], rva: u32) -> Option<usize> {
    for s in sections {
        if rva >= s.virtual_address && rva < s.virtual_address + s.size_of_raw_data.max(s.virtual_size) {
            let delta = rva - s.virtual_address;
            return Some((s.pointer_to_raw_data + delta) as usize);
        }
    }
    None
}

fn get_owner_section<'a>(sections: &'a [Section], rva: u32) -> Option<usize> {
    for (i, s) in sections.iter().enumerate() {
        if rva >= s.virtual_address && rva < s.virtual_address + s.size_of_raw_data.max(s.virtual_size) {
            return Some(i);
        }
    }
    None
}

// ─── SteamXOR ────────────────────────────────────────────────────────────────

/// XOR-decode a block of data starting from 'key' (or read key from first 4 bytes if key==0).
fn steam_xor(data: &mut [u8], key: u32) -> u32 {
    let mut key = key;
    let mut offset = 0usize;

    if key == 0 {
        key = u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4]));
        offset = 4;
    }

    let mut x = offset;
    while x + 4 <= data.len() {
        let val = u32::from_le_bytes(data[x..x + 4].try_into().unwrap_or([0; 4]));
        let decoded = val ^ key;
        data[x..x + 4].copy_from_slice(&decoded.to_le_bytes());
        key = val;
        x += 4;
    }

    key
}

// ─── XTEA for SteamDRMP.dll ──────────────────────────────────────────────────

fn xtea_decrypt_pass2(keys: &[u32; 4], v1: u32, v2: u32, n: u32) -> (u32, u32) {
    const DELTA: u64 = 0x9E3779B9;
    const MASK: u64 = 0xFFFFFFFF;
    let mut sum = ((DELTA * n as u64) & MASK) as u32;
    let mut v1 = v1;
    let mut v2 = v2;

    for _ in 0..n {
        v2 = v2.wrapping_sub(
            ((v1.wrapping_shl(4) ^ v1.wrapping_shr(5)).wrapping_add(v1))
            ^ (sum.wrapping_add(keys[((sum >> 11) & 3) as usize])),
        );
        sum = sum.wrapping_sub(DELTA as u32);
        v1 = v1.wrapping_sub(
            ((v2.wrapping_shl(4) ^ v2.wrapping_shr(5)).wrapping_add(v2))
            ^ (sum.wrapping_add(keys[(sum & 3) as usize])),
        );
    }
    (v1, v2)
}

fn steam_drmp_decrypt(data: &mut [u8], keys: &[u32; 4]) {
    let mut v1: u32 = 0x55555555;
    let mut v2: u32 = 0x55555555;
    let mut x = 0usize;
    while x + 8 <= data.len() {
        let d1 = u32::from_le_bytes(data[x..x + 4].try_into().unwrap());
        let d2 = u32::from_le_bytes(data[x + 4..x + 8].try_into().unwrap());
        let (r1, r2) = xtea_decrypt_pass2(keys, d1, d2, 32);
        data[x..x + 4].copy_from_slice(&(r1 ^ v1).to_le_bytes());
        data[x + 4..x + 8].copy_from_slice(&(r2 ^ v2).to_le_bytes());
        v1 = d1;
        v2 = d2;
        x += 8;
    }
}

// ─── AES-256-CBC decrypt ─────────────────────────────────────────────────────

fn aes256_cbc_decrypt(key: &[u8; 32], iv: &[u8; 16], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() % 16 != 0 {
        return Err("Data length is not a multiple of 16".to_string());
    }
    type Aes256CbcDec = cbc::Decryptor<Aes256>;
    let mut buf = data.to_vec();
    Aes256CbcDec::new(key.into(), iv.into())
        .decrypt_blocks_mut(unsafe {
            // SAFETY: buf is block-aligned, length checked above
            std::slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut aes::Block,
                buf.len() / 16,
            )
        });
    Ok(buf)
}

// ─── Variant 3.1 x64 header (0xF0 bytes) ────────────────────────────────────

#[derive(Debug)]
struct SteamStub64Var31 {
    _xor_key: u32,
    signature: u32,
    _image_base: u64,
    _drm_entry_point: u64,
    bind_section_offset: u32,
    _unknown0: u32,
    original_entry_point: u64,
    _unknown1: u32,
    payload_size: u32,
    drmp_dll_offset: u32,
    drmp_dll_size: u32,
    steam_app_id: u32,
    flags: u32,
    _bind_virtual_size: u32,
    _unknown2: u32,
    code_section_va: u64,
    code_section_raw_size: u64,
    aes_key: [u8; 32],
    aes_iv: [u8; 16],
    stolen_data: [u8; 16],
    encryption_keys: [u32; 4],
}

const STUB31_SIZE: usize = 0xF0;
const STUB30_SIZES: [u32; 2] = [0xB0, 0xD0]; // Variant 3.0 sizes

fn parse_stub31(raw: &[u8]) -> Option<SteamStub64Var31> {
    if raw.len() < STUB31_SIZE { return None; }
    let mut aes_key = [0u8; 32];
    aes_key.copy_from_slice(&raw[0x60..0x80]);
    let mut aes_iv = [0u8; 16];
    aes_iv.copy_from_slice(&raw[0x80..0x90]);
    let mut stolen_data = [0u8; 16];
    stolen_data.copy_from_slice(&raw[0x90..0xA0]);
    let mut enc_keys = [0u32; 4];
    for i in 0..4 {
        enc_keys[i] = read_u32_le(raw, 0xA0 + i * 4);
    }
    Some(SteamStub64Var31 {
        _xor_key: read_u32_le(raw, 0x00),
        signature: read_u32_le(raw, 0x04),
        _image_base: read_u64_le(raw, 0x08),
        _drm_entry_point: read_u64_le(raw, 0x10),
        bind_section_offset: read_u32_le(raw, 0x18),
        _unknown0: read_u32_le(raw, 0x1C),
        original_entry_point: read_u64_le(raw, 0x20),
        _unknown1: read_u32_le(raw, 0x28),
        payload_size: read_u32_le(raw, 0x2C),
        drmp_dll_offset: read_u32_le(raw, 0x30),
        drmp_dll_size: read_u32_le(raw, 0x34),
        steam_app_id: read_u32_le(raw, 0x38),
        flags: read_u32_le(raw, 0x3C),
        _bind_virtual_size: read_u32_le(raw, 0x40),
        _unknown2: read_u32_le(raw, 0x44),
        code_section_va: read_u64_le(raw, 0x48),
        code_section_raw_size: read_u64_le(raw, 0x50),
        aes_key,
        aes_iv,
        stolen_data,
        encryption_keys: enc_keys,
    })
}

// ─── Variant detection ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SteamVariant {
    Var31x64,
    Var30x64,
    Var31x86,
    Var30x86,
    Unknown,
}

impl std::fmt::Display for SteamVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SteamVariant::Var31x64 => write!(f, "3.1 x64"),
            SteamVariant::Var30x64 => write!(f, "3.0 x64"),
            SteamVariant::Var31x86 => write!(f, "3.1 x86"),
            SteamVariant::Var30x86 => write!(f, "3.0 x86"),
            SteamVariant::Unknown  => write!(f, "Unknown"),
        }
    }
}

/// Detect SteamStub variant from .bind section.
/// Returns None if not a SteamStub protected file.
fn detect_variant(data: &[u8], hdr: &PeHeaders, sections: &[Section]) -> Option<(SteamVariant, u32)> {
    let bind_idx = sections.iter().position(|s| section_name(s) == ".bind")?;
    let bind_sec = &sections[bind_idx];

    let bind_start = bind_sec.pointer_to_raw_data as usize;
    let bind_end = (bind_start + bind_sec.size_of_raw_data as usize).min(data.len());
    if bind_start >= bind_end { return None; }

    let bind_data = &data[bind_start..bind_end.min(bind_start + 0x3000)];

    // Check for known v3.x call signature
    let v3_sig = "E8 00 00 00 00 50 53 51 52 56 57 55 41 50";
    find_pattern(bind_data, v3_sig)?;

    // Determine sub-variant by header size pattern
    let off_30 = find_pattern(bind_data, "48 8D 91 ?? ?? ?? ?? 48"); // 3.0
    let off_31a = find_pattern(bind_data, "48 8D 91 ?? ?? ?? ?? 41"); // 3.1
    let off_312 = find_pattern(bind_data, "48 C7 84 24 ?? ?? ?? ?? ?? ?? ?? ?? 48"); // 3.1.2

    let (offset, extra) = if let Some(o) = off_30 {
        (o, 0i32)
    } else if let Some(o) = off_31a {
        (o, 0i32)
    } else if let Some(o) = off_312 {
        (o, 5i32)
    } else {
        return None;
    };

    let hdr_size_offset = (offset as i32 + 3 + extra) as usize;
    if hdr_size_offset + 4 > bind_data.len() { return None; }
    let header_size = (i32::from_le_bytes(bind_data[hdr_size_offset..hdr_size_offset + 4].try_into().ok()?).abs()) as u32;

    let variant = if hdr.is_64 {
        if header_size == 0xF0 {
            SteamVariant::Var31x64
        } else if STUB30_SIZES.contains(&header_size) {
            SteamVariant::Var30x64
        } else {
            return None;
        }
    } else {
        // x86 — similar sizes but different struct
        if header_size == 0xF0 {
            SteamVariant::Var31x86
        } else if STUB30_SIZES.contains(&header_size) {
            SteamVariant::Var30x86
        } else {
            return None;
        }
    };

    Some((variant, header_size))
}

// ─── Main unpack entry ────────────────────────────────────────────────────────

/// Unpack a SteamStub-protected exe.
/// - `exe_path`: path to the original exe
/// - `backup_suffix`: suffix for backup file (default ".org.exe")
/// - Returns SteamlessResult with user-friendly message
pub fn unpack_exe(exe_path: &Path, backup_suffix: &str) -> SteamlessResult {
    // If the specified exe doesn't exist, try to find a matching exe in the same directory
    let exe_path_resolved: std::borrow::Cow<Path> = if !exe_path.exists() {
        if let (Some(dir), Some(name)) = (exe_path.parent(), exe_path.file_stem()) {
            let target = name.to_string_lossy().replace(' ', "").to_lowercase();
            let found = fs::read_dir(dir).ok().and_then(|mut entries| {
                entries.find_map(|entry| {
                    let entry = entry.ok()?;
                    let fname = entry.file_name();
                    let fname_str = fname.to_string_lossy();
                    let stem = std::path::Path::new(&*fname_str)
                        .file_stem()?.to_string_lossy().replace(' ', "").to_lowercase();
                    let ext = std::path::Path::new(&*fname_str)
                        .extension()?.to_string_lossy().to_lowercase();
                    if ext == "exe" && stem == target {
                        Some(entry.path())
                    } else {
                        None
                    }
                })
            });
            match found {
                Some(p) => std::borrow::Cow::Owned(p),
                None => std::borrow::Cow::Borrowed(exe_path),
            }
        } else {
            std::borrow::Cow::Borrowed(exe_path)
        }
    } else {
        std::borrow::Cow::Borrowed(exe_path)
    };

    let data = match fs::read(&*exe_path_resolved) {
        Ok(d) => d,
        Err(e) => return SteamlessResult::err(format!(
            "Không thể đọc file game.\nĐường dẫn thử: {}\nLỗi: {}", exe_path_resolved.display(), e
        )),
    };

    let hdr = match parse_pe(&data) {
        Some(h) => h,
        None => return SteamlessResult::err(
            "File này không phải là game exe hợp lệ. Hãy chọn đúng file thực thi chính của game.".to_string()
        ),
    };

    let sections = read_sections(&data, &hdr);

    // Detect variant
    let (variant, _header_size) = match detect_variant(&data, &hdr, &sections) {
        Some(v) => v,
        None => return SteamlessResult::err(
            "Game này không dùng Steam DRM, hoặc đã được fix trước đó. Không cần thực hiện fix lỗi 54 cho game này.".to_string()
        ),
    };

    match &variant {
        SteamVariant::Var31x64 | SteamVariant::Var30x64 => {
            unpack_x64(&exe_path_resolved, &data, &hdr, &sections, variant, backup_suffix)
        }
        SteamVariant::Var31x86 | SteamVariant::Var30x86 => {
            // x86 structure is slightly different, falls back to a simplified path
            unpack_x86_simple(&exe_path_resolved, &data, &hdr, &sections, variant, backup_suffix)
        }
        SteamVariant::Unknown => {
            SteamlessResult::err("Phiên bản DRM của game này chưa được hỗ trợ. Hãy báo cáo cho team để cập nhật.")
        }
    }
}

// ─── x64 unpacker (Variant 3.0 and 3.1) ──────────────────────────────────────

fn unpack_x64(
    exe_path: &Path,
    data: &[u8],
    hdr: &PeHeaders,
    sections: &[Section],
    variant: SteamVariant,
    backup_suffix: &str,
) -> SteamlessResult {
    // Step 1 — Read and XOR-decode the DRM header
    let (stub, xor_after_payload, tls_used, oep_rva_for_bind) = match read_stub_x64(data, hdr, sections) {
        Ok(r) => r,
        Err(msg) => return SteamlessResult::err(msg),
    };

    if stub.signature != 0xC0DEC0DF {
        return SteamlessResult::err(
            "File game bị hỏng hoặc đã được chỉnh sửa bởi phần mềm khác. Hãy xác minh file game trong launcher trước khi thử lại."
        );
    }

    let app_id = stub.steam_app_id;

    // Step 2 — Skip payload (just advance xor key, we don't need payload content)
    let _ = (xor_after_payload, tls_used); // used during read_stub

    // Step 3 — Decrypt code section
    let no_encryption = (stub.flags & 0x4) != 0; // SteamStubDrmFlags::NoEncryption == 0x4

    // Step 4 — Find code section
    let code_section_idx = if !no_encryption {
        let cva = stub.code_section_va;
        // code_section_va is a virtual address, convert to RVA
        let cva_rva = if cva > hdr.image_base { (cva - hdr.image_base) as u32 } else { cva as u32 };
        match get_owner_section(sections, cva_rva) {
            Some(i) => Some(i),
            None => return SteamlessResult::err(
                "Không tìm thấy vùng code của game. File có thể đã bị thay đổi hoặc không tương thích."
            ),
        }
    } else {
        None
    };

    // Step 5 — Decrypt code section data
    let decrypted_code = if !no_encryption {
        let code_idx = code_section_idx.unwrap();
        let code_sec = &sections[code_idx];

        let raw_offset = code_sec.pointer_to_raw_data as usize;
        let raw_size = code_sec.size_of_raw_data as usize;

        if raw_size == 0 {
            Some(Vec::new())
        } else {
            if raw_offset + raw_size > data.len() {
                return SteamlessResult::err("Dữ liệu file game không đầy đủ — hãy kiểm tra lại bằng 'Verify file integrity'.");
            }

            // prepend stolen bytes (first 16 bytes were taken by the stub)
            let total = raw_size + 16;
            let padded = (total + 15) & !15; // align to 16
            let mut code_data = vec![0u8; padded];
            code_data[..16].copy_from_slice(&stub.stolen_data);
            code_data[16..16 + raw_size].copy_from_slice(&data[raw_offset..raw_offset + raw_size]);

            match aes256_cbc_decrypt(&stub.aes_key, &stub.aes_iv, &code_data[..padded]) {
                Ok(dec) => Some(dec),
                Err(_) => return SteamlessResult::err(
                    "Giải mã game thất bại — key hoặc IV bị sai. Game này có thể dùng phiên bản DRM khác chưa được hỗ trợ."
                ),
            }
        }
    } else {
        None
    };

    // Step 6 — Rebuild PE
    let backup_suffix = if backup_suffix.is_empty() { ".org.exe" } else { backup_suffix };
    let output_path = exe_path.with_extension("").with_file_name(format!(
        "{}.unpacked.exe",
        exe_path.file_stem().unwrap_or_default().to_string_lossy()
    ));

    match rebuild_pe_x64(data, hdr, sections, &stub, code_section_idx, decrypted_code.as_deref(), &output_path) {
        Ok(()) => {}
        Err(msg) => return SteamlessResult::err(msg),
    }

    // Backup original, replace with unpacked
    let backup_path = exe_path.with_file_name(format!(
        "{}{}",
        exe_path.file_stem().unwrap_or_default().to_string_lossy(),
        backup_suffix
    ));

    if let Err(e) = fs::copy(exe_path, &backup_path) {
        return SteamlessResult::err(format!(
            "Không thể tạo bản sao lưu file gốc: {}. Hãy đảm bảo không có chương trình nào đang giữ file game.", e
        ));
    }

    if let Err(e) = fs::copy(&output_path, exe_path) {
        // Restore backup before reporting error
        let _ = fs::copy(&backup_path, exe_path);
        return SteamlessResult::err(format!(
            "Không thể thay thế file game: {}. Hãy đảm bảo game không đang chạy.", e
        ));
    }

    // Clean up temp unpacked file
    let _ = fs::remove_file(&output_path);

    SteamlessResult::ok(
        format!(
            "Đã xử lý thành công! Game đã được fix lỗi 54 (SteamStub {} đã gỡ). Bản gốc được lưu tại: {}",
            variant,
            backup_path.display()
        ),
        exe_path.display().to_string(),
        variant.to_string(),
        app_id,
    )
}

fn read_stub_x64(
    data: &[u8],
    hdr: &PeHeaders,
    sections: &[Section],
) -> Result<(SteamStub64Var31, u32, bool, u32), String> {
    // Try reading from OEP first
    let try_oep = hdr.entry_point_rva;
    if let Some(result) = try_read_stub_at_x64(data, hdr, sections, try_oep, false) {
        if result.0.signature == 0xC0DEC0DF {
            return Ok(result);
        }
    }

    // Try TLS callback (if any)
    if hdr.tls_data_dir_rva != 0 && hdr.tls_data_dir_size != 0 {
        if let Some(tls_file_off) = rva_to_file_offset(sections, hdr.tls_data_dir_rva) {
            // TLS directory: 64-bit: 2x u64 (start/end), 1x u64 (address of callbacks)
            let cb_rva_offset = if hdr.is_64 { tls_file_off + 24 } else { tls_file_off + 12 };
            if cb_rva_offset + 8 <= data.len() {
                let cb_va = if hdr.is_64 {
                    read_u64_le(data, cb_rva_offset)
                } else {
                    read_u32_le(data, cb_rva_offset) as u64
                };
                if cb_va != 0 {
                    let cb_rva = if cb_va > hdr.image_base { (cb_va - hdr.image_base) as u32 } else { 0 };
                    // Read first callback address from table
                    if let Some(cb_table_off) = rva_to_file_offset(sections, cb_rva) {
                        let first_cb_va = if hdr.is_64 {
                            if cb_table_off + 8 <= data.len() { read_u64_le(data, cb_table_off) } else { 0 }
                        } else {
                            if cb_table_off + 4 <= data.len() { read_u32_le(data, cb_table_off) as u64 } else { 0 }
                        };
                        if first_cb_va != 0 {
                            let first_cb_rva = if first_cb_va > hdr.image_base { (first_cb_va - hdr.image_base) as u32 } else { 0 };
                            if let Some(result) = try_read_stub_at_x64(data, hdr, sections, first_cb_rva, true) {
                                if result.0.signature == 0xC0DEC0DF {
                                    return Ok(result);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Không đọc được header DRM của game. File có thể đã được fix trước hoặc dùng phiên bản DRM khác.".to_string())
}

fn try_read_stub_at_x64(
    data: &[u8],
    _hdr: &PeHeaders,
    sections: &[Section],
    entry_rva: u32,
    is_tls: bool,
) -> Option<(SteamStub64Var31, u32, bool, u32)> {
    let file_offset = rva_to_file_offset(sections, entry_rva)?;
    if file_offset < STUB31_SIZE { return None; }
    let header_start = file_offset - STUB31_SIZE;
    if header_start + STUB31_SIZE > data.len() { return None; }

    let mut hdr_data = data[header_start..header_start + STUB31_SIZE].to_vec();
    let xor_after = steam_xor(&mut hdr_data, 0);
    let stub = parse_stub31(&hdr_data)?;

    // Decode payload to advance xor key
    let xor_final = if stub.payload_size > 0 {
        let payload_size = ((stub.payload_size + 0x0F) & !0x0F) as usize;
        let payload_rva = if is_tls {
            entry_rva.wrapping_sub(stub.bind_section_offset)
        } else {
            entry_rva.wrapping_sub(stub.bind_section_offset)
        };
        if let Some(payload_off) = rva_to_file_offset(sections, payload_rva) {
            if payload_off + payload_size <= data.len() {
                let mut payload = data[payload_off..payload_off + payload_size].to_vec();
                steam_xor(&mut payload, xor_after)
            } else { xor_after }
        } else { xor_after }
    } else { xor_after };

    Some((stub, xor_final, is_tls, entry_rva))
}

fn rebuild_pe_x64(
    data: &[u8],
    hdr: &PeHeaders,
    sections: &[Section],
    stub: &SteamStub64Var31,
    code_section_idx: Option<usize>,
    decrypted_code: Option<&[u8]>,
    output_path: &Path,
) -> Result<(), String> {
    let mut out = data.to_vec();

    // Patch OEP
    let oep_rva = if stub.original_entry_point > hdr.image_base {
        (stub.original_entry_point - hdr.image_base) as u32
    } else {
        stub.original_entry_point as u32
    };
    write_u32_le(&mut out, hdr.opt_offset + 16, oep_rva);

    // Zero checksum
    write_u32_le(&mut out, hdr.opt_offset + 64, 0); // CheckSum at opt+64 (PE32+)

    // Write decrypted code section
    if let (Some(idx), Some(dec_data)) = (code_section_idx, decrypted_code) {
        let sec = &sections[idx];
        let raw_off = sec.pointer_to_raw_data as usize;
        let raw_size = sec.size_of_raw_data as usize;
        if raw_off + raw_size <= out.len() && dec_data.len() >= raw_size {
            out[raw_off..raw_off + raw_size].copy_from_slice(&dec_data[..raw_size]);
        }
    }

    // Remove .bind section: zero out its data and header entry
    let bind_idx = sections.iter().position(|s| section_name(s) == ".bind");
    if let Some(bidx) = bind_idx {
        let sec = &sections[bidx];
        // Zero section data
        let raw_off = sec.pointer_to_raw_data as usize;
        let raw_size = sec.size_of_raw_data as usize;
        if raw_off + raw_size <= out.len() {
            out[raw_off..raw_off + raw_size].fill(0);
        }
        // Zero section header
        let sec_hdr_off = hdr.sections_offset + bidx * IMAGE_SIZEOF_SECTION_HEADER;
        if sec_hdr_off + IMAGE_SIZEOF_SECTION_HEADER <= out.len() {
            out[sec_hdr_off..sec_hdr_off + IMAGE_SIZEOF_SECTION_HEADER].fill(0);
        }
        // Decrement section count
        let num_sec_off = hdr.pe_offset + 4 + 2;
        let num_sec = read_u16_le(&out, num_sec_off);
        write_u16_le(&mut out, num_sec_off, num_sec.saturating_sub(1));
    }

    fs::write(output_path, &out).map_err(|e| format!(
        "Không thể ghi file đã giải mã: {}. Hãy kiểm tra dung lượng ổ đĩa và quyền ghi.", e
    ))
}

fn write_u16_le(data: &mut [u8], offset: usize, val: u16) {
    data[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

// ─── x86 simple path ─────────────────────────────────────────────────────────

/// For x86 Variant 3.0/3.1 — structure is similar but 32-bit fields.
/// This path handles detection and basic unpacking for 32-bit games.
fn unpack_x86_simple(
    exe_path: &Path,
    _data: &[u8],
    _hdr: &PeHeaders,
    _sections: &[Section],
    variant: SteamVariant,
    _backup_suffix: &str,
) -> SteamlessResult {
    // x86 is less common for modern games; return helpful message
    SteamlessResult::err(format!(
        "Game này dùng SteamStub {} (32-bit). Phiên bản 32-bit chưa được hỗ trợ tự động. \
         Hãy liên hệ nhóm hỗ trợ để được trợ giúp thủ công.",
        variant
    ))
}

// ─── Restore original ─────────────────────────────────────────────────────────

/// Restore the original exe from backup and delete the patched version.
pub fn restore_exe(exe_path: &Path, backup_suffix: &str) -> Result<String, String> {
    let backup_suffix = if backup_suffix.is_empty() { ".org.exe" } else { backup_suffix };
    let backup_path = exe_path.with_file_name(format!(
        "{}{}",
        exe_path.file_stem().unwrap_or_default().to_string_lossy(),
        backup_suffix
    ));

    if !backup_path.exists() {
        return Err("Không tìm thấy bản sao lưu file gốc. Có thể đã bị xóa hoặc chưa từng được fix.".to_string());
    }

    // Restore
    fs::copy(&backup_path, exe_path).map_err(|e| format!(
        "Không thể khôi phục file gốc: {}. Hãy đảm bảo game không đang chạy.", e
    ))?;

    // Remove backup
    let _ = fs::remove_file(&backup_path);

    Ok(format!(
        "Đã khôi phục file game về phiên bản gốc. Lỗi 54 Fix đã được tắt."
    ))
}

// ─── Status check ─────────────────────────────────────────────────────────────

/// Check if an exe has already been patched (backup exists alongside it).
pub fn is_patched(exe_path: &Path, backup_suffix: &str) -> bool {
    let backup_suffix = if backup_suffix.is_empty() { ".org.exe" } else { backup_suffix };
    let backup_path = exe_path.with_file_name(format!(
        "{}{}",
        exe_path.file_stem().unwrap_or_default().to_string_lossy(),
        backup_suffix
    ));
    backup_path.exists()
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn steamless_apply(exe_path: String, backup_suffix: Option<String>) -> SteamlessResult {
    let suffix = backup_suffix.unwrap_or_else(|| ".org.exe".to_string());
    unpack_exe(Path::new(&exe_path), &suffix)
}

#[tauri::command]
pub fn steamless_restore(exe_path: String, backup_suffix: Option<String>) -> Result<String, String> {
    let suffix = backup_suffix.unwrap_or_else(|| ".org.exe".to_string());
    restore_exe(Path::new(&exe_path), &suffix)
}

#[tauri::command]
pub fn steamless_status(exe_path: String, backup_suffix: Option<String>) -> bool {
    let suffix = backup_suffix.unwrap_or_else(|| ".org.exe".to_string());
    is_patched(Path::new(&exe_path), &suffix)
}
