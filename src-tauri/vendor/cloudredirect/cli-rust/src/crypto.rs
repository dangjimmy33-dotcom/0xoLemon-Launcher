// SteamTools-compatible crypto and payload-cache fingerprinting.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use std::io::Read;
use std::path::{Path, PathBuf};

/// AES-256 key SteamTools uses to encrypt/decrypt the payload cache.
pub const AES_KEY: [u8; 32] = [
    0x31, 0x4C, 0x20, 0x86, 0x15, 0x05, 0x74, 0xE1, 0x5C, 0xF1, 0x1D, 0x1B, 0xC1, 0x71, 0x25, 0x1A,
    0x47, 0x08, 0x6C, 0x00, 0x26, 0x93, 0x55, 0xCD, 0x51, 0xC9, 0x3A, 0x42, 0x3C, 0x14, 0x02, 0x94,
];

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

/// AES-256-CBC decrypt with PKCS7 padding. Returns None on bad padding/length.
pub fn aes_cbc_decrypt(ct: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Option<Vec<u8>> {
    let dec = Aes256CbcDec::new(key.into(), iv.into());
    let mut buf = ct.to_vec();
    dec.decrypt_padded_mut::<Pkcs7>(&mut buf)
        .ok()
        .map(|s| s.to_vec())
}

/// AES-256-CBC encrypt with PKCS7 padding.
pub fn aes_cbc_encrypt(pt: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Vec<u8> {
    let enc = Aes256CbcEnc::new(key.into(), iv.into());
    let mut buf = vec![0u8; pt.len() + 16];
    let n = enc
        .encrypt_padded_b2b_mut::<Pkcs7>(pt, &mut buf)
        .expect("encrypt buffer large enough")
        .len();
    buf.truncate(n);
    buf
}

/// Compute the SteamTools payload-cache fingerprint.
pub fn compute_fingerprint() -> String {
    // CPUID leaf 0 -> vendor string (EBX, EDX, ECX order).
    let l0 = core::arch::x86_64::__cpuid(0);
    let mut vendor_bytes = [0u8; 12];
    vendor_bytes[0..4].copy_from_slice(&l0.ebx.to_le_bytes());
    vendor_bytes[4..8].copy_from_slice(&l0.edx.to_le_bytes());
    vendor_bytes[8..12].copy_from_slice(&l0.ecx.to_le_bytes());
    let vendor = String::from_utf8_lossy(&vendor_bytes).into_owned();

    // CPUID leaf 1 -> base family/model nibbles (not extended).
    let l1 = core::arch::x86_64::__cpuid(1);
    let family = (l1.eax >> 8) & 0xF;
    let model = (l1.eax >> 4) & 0xF;
    let nproc = (num_cpus() as u32) & 0xFF;

    let tag = format!("V{}_F{:X}_M{:X}_C{:X}", vendor, family, model, nproc);
    let tag = tag.as_bytes();

    // XOR with "version" (same as Core.dll).
    let xor_key = b"version";
    let xored: Vec<u8> = tag
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ xor_key[i % 7])
        .collect();

    // MD5 -> lowercase hex ASCII bytes.
    use md5::{Digest, Md5};
    let digest = Md5::digest(&xored);
    let md5_hex = hex_lower(&digest);
    let md5_hex_bytes = md5_hex.as_bytes();

    // Non-standard CRC-64: poly 0x85E1C3D753D46D27, XOR-before-shift.
    // This is SteamTools-specific; do NOT "fix" the bit ordering.
    let mut crc: u64 = 0xFFFF_FFFF_FFFF_FFFF;
    for &b in md5_hex_bytes {
        crc ^= b as u64;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc ^= 0x85E1_C3D7_53D4_6D27;
            }
            crc >>= 1;
        }
    }
    format!("{:016X}", crc ^ 0xFFFF_FFFF_FFFF_FFFF)
}

/// Locate the encrypted payload cache file under appcache\httpcache\3b.
pub fn find_cache_path(steam_path: &Path, log: &mut dyn FnMut(&str)) -> Option<PathBuf> {
    let cache_dir = steam_path
        .join("appcache")
        .join("httpcache")
        .join("3b");
    if !cache_dir.is_dir() {
        return None;
    }

    // Try the computed fingerprint slot first.
    let fp = compute_fingerprint();
    let fp_path = cache_dir.join(&fp);
    if fp_path.is_file() {
        if validate_payload_cache(&fp_path) {
            log(&format!("Cache: {}", fp_path.display()));
            return Some(fp_path);
        }
        log(&format!(
            "Cache at {} failed validation, scanning..",
            fp_path.display()
        ));
    } else {
        log(&format!(
            "Fingerprint {} computed but no cache file there",
            fp
        ));
    }

    // Fall back to scanning for a 16-char-named file of plausible size.
    let entries = std::fs::read_dir(&cache_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if name.len() == 16 && len > 500_000 && len < 5_000_000 {
            if !validate_payload_cache(&path) {
                log(&format!(
                    "Cache candidate {} failed validation, skipping",
                    name
                ));
                continue;
            }
            log(&format!("Cache (found by scan): {}", path.display()));
            return Some(path);
        }
    }
    None
}

pub fn get_expected_cache_path(steam_path: &Path) -> PathBuf {
    steam_path
        .join("appcache")
        .join("httpcache")
        .join("3b")
        .join(compute_fingerprint())
}

/// Validate a candidate cache file: AES-CBC decrypt -> skip 4-byte prefix ->
/// zlib inflate -> must start with an MZ header.
pub fn validate_payload_cache(path: &Path) -> bool {
    let raw = match std::fs::read(path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if raw.len() < 32 {
        return false;
    }
    let iv: [u8; 16] = match raw[0..16].try_into() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let ct = &raw[16..];
    let plain = match aes_cbc_decrypt(ct, &AES_KEY, &iv) {
        Some(p) => p,
        None => return false,
    };
    if plain.len() < 6 {
        return false;
    }
    // Payload format: 4-byte prefix then zlib data.
    let mut z = flate2::read::ZlibDecoder::new(&plain[4..]);
    let mut header = [0u8; 2];
    match z.read_exact(&mut header) {
        Ok(()) => header[0] == 0x4D && header[1] == 0x5A, // 'M','Z'
        Err(_) => false,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_roundtrip() {
        let iv = [7u8; 16];
        let pt = b"hello steamtools payload check";
        let ct = aes_cbc_encrypt(pt, &AES_KEY, &iv);
        let back = aes_cbc_decrypt(&ct, &AES_KEY, &iv).unwrap();
        assert_eq!(&back, pt);
    }

    #[test]
    fn fingerprint_is_16_hex() {
        let fp = compute_fingerprint();
        assert_eq!(fp.len(), 16);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        eprintln!("fingerprint = {}", fp);
    }

    // The computed fingerprint must locate and validate the real payload cache.
    #[test]
    fn finds_live_payload_cache() {
        let candidates = [r"C:\Games\Steam", r"C:\Program Files (x86)\Steam"];
        let steam = candidates
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.is_dir());
        let Some(steam) = steam else {
            eprintln!("Steam not found; skipping cache parity test");
            return;
        };
        let fp = compute_fingerprint();
        let expected = get_expected_cache_path(&steam);
        eprintln!("fingerprint={} expected={}", fp, expected.display());
        eprintln!("expected exists on disk: {}", expected.is_file());

        let mut log = |m: &str| eprintln!("  [find] {}", m);
        match find_cache_path(&steam, &mut log) {
            Some(p) => eprintln!("FOUND + VALIDATED cache: {}", p.display()),
            None => eprintln!("no valid cache found (payload may not be cached yet)"),
        }
    }
}
