// Patcher: STFixer flow adapted for Tauri integration.
// Removed: embedded DLL deploy (replaced by downloader.rs online download).

use crate::cloud_redirect::crypto::{self, AES_KEY};
use crate::cloud_redirect::file_util;
use crate::cloud_redirect::pe::PeSection;
use crate::cloud_redirect::signatures::{self, PatchEntry};
use crate::cloud_redirect::steam_detector;

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const HIJACK_CANDIDATES: [&str; 2] = ["xinput1_4.dll", "dwmapi.dll"];

// Core DLL download sources (primary catbox + HTTPS fallback; SHA-256 verified).
const XINPUT_URL: &str = "https://files.catbox.moe/heom44.dll";
const DWMAPI_URL: &str = "https://files.catbox.moe/32p6f9.dll";
const XINPUT_FALLBACK_URL: &str = "https://update.aaasn.com/update";
const DWMAPI_FALLBACK_URL: &str = "https://update.aaasn.com/dwmapi";
const XINPUT_HASH: &str = "ddb1f0909c7092f06890674f90b5d4f1198724b05b4bf1e656b4063897340243";
const DWMAPI_HASH: &str = "1ce49ed63af004ad37a4d2921a5659a17001c4c0026d6245fcc0d543e9c265d0";

// SteamTools.exe DeployCoreToSteamDir prologue patch.
const STEXE_PATCH_OFFSET: usize = 0x282F0;
const STEXE_ORIGINAL: [u8; 2] = [0x40, 0x55];
const STEXE_PATCHED: [u8; 2] = [0xC3, 0x90];

pub struct PatchOutcome {
    pub succeeded: bool,
    pub error: Option<String>,
}

impl PatchOutcome {
    pub fn ok() -> Self { PatchOutcome { succeeded: true, error: None } }
    pub fn fail(msg: impl Into<String>) -> Self { PatchOutcome { succeeded: false, error: Some(msg.into()) } }
}

pub struct Patcher {
    steam_path: PathBuf,
    pub log_lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Patcher {
    pub fn new(steam_path: PathBuf) -> Self {
        Patcher { steam_path, log_lines: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())) }
    }

    fn log(&self, msg: &str) {
        println!("{}", msg);
        if let Ok(mut lines) = self.log_lines.lock() {
            lines.push(msg.to_string());
        }
    }

    fn find_core_dll(&self) -> Option<&'static str> {
        for name in HIJACK_CANDIDATES {
            let path = self.steam_path.join(name);
            if !path.is_file() { continue; }
            if let Ok(buf) = std::fs::read(&path) {
                if signatures::scan_for_bytes(&buf, 0, buf.len() as i64, &AES_KEY) >= 0 {
                    return Some(name);
                }
            }
        }
        None
    }

    pub fn has_core_dll(&self) -> bool { self.find_core_dll().is_some() }

    pub fn repair_core_dlls(&self) -> PatchOutcome {
        let targets = [
            ("xinput1_4.dll", XINPUT_URL, XINPUT_FALLBACK_URL, XINPUT_HASH),
            ("dwmapi.dll", DWMAPI_URL, DWMAPI_FALLBACK_URL, DWMAPI_HASH),
        ];
        for (name, url, fallback, hash) in targets {
            let dest = self.steam_path.join(name);
            if dest.is_file() {
                if let Ok(existing) = std::fs::read(&dest) {
                    if sha256_hex(&existing) == hash {
                        self.log(&format!("  {}: already present, hash OK", name));
                        continue;
                    }
                    self.log(&format!("  {}: hash mismatch, re-downloading..", name));
                }
            }
            self.log(&format!("Downloading {}..", name));
            let mut data: Option<Vec<u8>> = None;
            let mut from_fallback = false;

            match http_get(url) {
                Ok(dl) if !dl.is_empty() && sha256_hex(&dl) == hash => data = Some(dl),
                Ok(dl) => self.log(&format!("  Primary returned bad data (len={})", dl.len())),
                Err(e) => self.log(&format!("  Primary failed: {}", e)),
            }
            if data.is_none() {
                self.log("  Trying fallback..");
                match http_get(fallback) {
                    Ok(dl) if !dl.is_empty() && sha256_hex(&dl) == hash => { data = Some(dl); from_fallback = true; }
                    Ok(dl) => self.log(&format!("  Fallback returned bad data (len={})", dl.len())),
                    Err(e) => self.log(&format!("  Fallback failed: {}", e)),
                }
            }
            let Some(data) = data else {
                return PatchOutcome::fail(format!("Could not download {}: no valid source", name));
            };
            if let Err(e) = file_util::atomic_write_all_bytes(&dest, &data) {
                return PatchOutcome::fail(format!("Could not write {}: {}", name, e));
            }
            self.log(&format!("  {}: {} bytes{}", name, data.len(), if from_fallback { " (fallback)" } else { "" }));
        }
        self.log("DLL repair complete.");
        PatchOutcome::ok()
    }

    pub fn apply_offline_setup(&self) -> PatchOutcome {
        let version = match steam_detector::get_steam_version(&self.steam_path) {
            Some(v) => v,
            None => return PatchOutcome::fail("Steam version could not be determined. Cannot safely patch."),
        };
        if !steam_detector::is_supported_steam_version(version) {
            return PatchOutcome::fail(format!(
                "Steam version mismatch: installed {}. Update CloudRedirect or downgrade Steam.",
                version
            ));
        }
        self.log(&format!("  Steam version: {} (OK)", version));

        let hijack = match self.find_core_dll() {
            Some(n) => n,
            None => return PatchOutcome::fail("SteamTools Core DLL not found. Is SteamTools installed?"),
        };
        let dll_path = self.steam_path.join(hijack);
        let dll_data = match std::fs::read(&dll_path) {
            Ok(d) => d,
            Err(_) => return PatchOutcome::fail(format!("{} is in use - close Steam first", hijack)),
        };

        self.log(&format!("Patching {}..", hijack));
        let resolved_core = match self.resolve_core_patch_offsets(&dll_data) {
            Some(r) => r,
            None => return PatchOutcome::fail(format!("Could not identify patch locations in {}", hijack)),
        };
        let (patched_dll, dll_applied, dll_skipped, dll_errors) = apply_patches(&dll_data, &resolved_core);
        if !dll_errors.is_empty() {
            for e in &dll_errors { self.log(e); }
            return PatchOutcome::fail(format!("Byte mismatch in {} - wrong version?", hijack));
        }

        let mut log_fn = |m: &str| self.log(m);
        let cache_path = match crypto::find_cache_path(&self.steam_path, &mut log_fn) {
            Some(p) => p,
            None => return PatchOutcome::fail("Payload cache not found. Launch Steam once to generate it, then retry."),
        };

        self.log("Patching payload (offline setup)..");
        let (payload, iv) = match self.read_and_decrypt_payload(&cache_path) {
            Ok(v) => v,
            Err(e) => return PatchOutcome::fail(e),
        };

        let resolved_setup = match self.resolve_setup_patch_offsets(&payload) {
            Some(r) if !r.is_empty() => r,
            _ => return PatchOutcome::fail("Could not identify activation patch locations in payload - unsupported version?"),
        };
        let (patched_payload, pl_applied, pl_skipped, pl_errors) = apply_patches(&payload, &resolved_setup);
        if !pl_errors.is_empty() {
            for e in &pl_errors { self.log(e); }
            return PatchOutcome::fail("Byte mismatch in payload - wrong version?".to_string());
        }

        self.backup_both(&cache_path, &dll_path);

        if pl_applied > 0 {
            if let Err(e) = self.re_encrypt_and_write(&cache_path, &patched_payload, &iv) {
                return PatchOutcome::fail(format!("Could not write payload cache: {}", e));
            }
            self.log(&format!("  {} patch(es) applied to payload{}", pl_applied,
                if pl_skipped > 0 { format!(", {} already done", pl_skipped) } else { String::new() }));
        } else {
            self.log("  Payload: already patched");
        }

        if dll_applied > 0 {
            if let Err(e) = file_util::atomic_write_all_bytes(&dll_path, &patched_dll) {
                return PatchOutcome::fail(format!("Could not write {}: {}", hijack, e));
            }
            self.log(&format!("  {} patch(es) applied to {}{}", dll_applied, hijack,
                if dll_skipped > 0 { format!(", {} already done", dll_skipped) } else { String::new() }));
        } else {
            self.log(&format!("  {}: already patched", hijack));
        }

        self.log("Done.");
        PatchOutcome::ok()
    }

    pub fn patch_steamtools_exe(&self) -> i32 {
        let exe = match find_steamtools_exe() {
            Some(e) => e,
            None => { self.log("  SteamTools.exe not found -- skipping"); return 0; }
        };
        kill_steamtools(|m| self.log(m));
        let mut data = match std::fs::read(&exe) {
            Ok(d) => d,
            Err(e) => { self.log(&format!("  SteamTools.exe: {}", e)); return -1; }
        };
        if data.len() < STEXE_PATCH_OFFSET + 2 {
            self.log("  SteamTools.exe too small - unrecognized version");
            return -1;
        }
        if data[STEXE_PATCH_OFFSET] == STEXE_PATCHED[0] && data[STEXE_PATCH_OFFSET + 1] == STEXE_PATCHED[1] {
            self.log("  SteamTools.exe: already patched");
            return 1;
        }
        if data[STEXE_PATCH_OFFSET] != STEXE_ORIGINAL[0] || data[STEXE_PATCH_OFFSET + 1] != STEXE_ORIGINAL[1] {
            self.log(&format!("  SteamTools.exe: unexpected bytes at patch site - unrecognized version"));
            return -1;
        }
        self.backup(&exe);
        data[STEXE_PATCH_OFFSET] = STEXE_PATCHED[0];
        data[STEXE_PATCH_OFFSET + 1] = STEXE_PATCHED[1];
        if let Err(e) = file_util::atomic_write_all_bytes(&exe, &data) {
            self.log(&format!("  SteamTools.exe: {}", e));
            return -1;
        }
        self.log("  SteamTools.exe: patched (DLL deploy disabled)");
        1
    }

    fn resolve_core_patch_offsets(&self, dll: &[u8]) -> Option<Vec<PatchEntry>> {
        let sections = PeSection::parse(dll);
        let rdata = match PeSection::find(&sections, ".rdata") {
            Some(s) => s,
            None => { self.log("  Core.dll: no .rdata section found"); return None; }
        };
        let mut key_offset = signatures::scan_for_bytes(dll, rdata.raw_offset as i64, (rdata.raw_offset + rdata.raw_size) as i64, &AES_KEY);
        if key_offset < 0 { key_offset = signatures::scan_for_bytes(dll, 0, dll.len() as i64, &AES_KEY); }
        if key_offset < 0 { self.log("  Core.dll: AES key not found"); return None; }
        self.log(&format!("  AES key found at 0x{:X}", key_offset));
        let text = match PeSection::find(&sections, ".text") {
            Some(s) => s,
            None => { self.log("  Core.dll: no .text section found"); return None; }
        };
        let t_start = text.raw_offset as i64;
        let t_end = (t_start + text.raw_size as i64).min(dll.len() as i64);
        let mut log = |m: &str| self.log(m);
        let defs = signatures::core_patch_defs();
        signatures::resolve_pattern_group(dll, &defs, t_start, t_end, 0, 0, &mut log)
    }

    fn resolve_payload_sections(&self, payload: &[u8]) -> Option<(i64, i64, i64, i64)> {
        let sections = PeSection::parse(payload);
        let text = PeSection::find(&sections, ".text")?;
        const KNOWN: [&str; 8] = [".text", ".rdata", ".data", ".pdata", ".fptable", ".rsrc", ".reloc", ".idata"];
        let obf = sections.iter().find(|s| !KNOWN.contains(&s.name.as_str()));
        let t_start = text.raw_offset as i64;
        let t_end = (t_start + text.raw_size as i64).min(payload.len() as i64);
        let (g_start, g_end) = match obf {
            Some(o) => { let s = o.raw_offset as i64; (s, (s + o.raw_size as i64).min(payload.len() as i64)) }
            None => (t_start, t_end),
        };
        Some((t_start, t_end, g_start, g_end))
    }

    fn resolve_setup_patch_offsets(&self, payload: &[u8]) -> Option<Vec<PatchEntry>> {
        let (t_start, t_end, g_start, g_end) = self.resolve_payload_sections(payload)?;
        let mut log = |m: &str| self.log(m);
        let new_defs = signatures::payload_setup_defs();
        if let Some(r) = signatures::resolve_pattern_group(payload, &new_defs, t_start, t_end, g_start, g_end, &mut log) {
            return Some(r);
        }
        let old_defs = signatures::payload_setup_defs_v1();
        signatures::resolve_pattern_group(payload, &old_defs, t_start, t_end, g_start, g_end, &mut log)
    }

    fn read_and_decrypt_payload(&self, cache_path: &Path) -> Result<(Vec<u8>, [u8; 16]), String> {
        let raw = std::fs::read(cache_path).map_err(|_| "Payload cache is in use - close Steam first".to_string())?;
        if raw.len() < 32 { return Err("Cache file too small".to_string()); }
        let iv: [u8; 16] = raw[0..16].try_into().unwrap();
        let ct = &raw[16..];
        self.log("  Decrypting..");
        let dec = crypto::aes_cbc_decrypt(ct, &AES_KEY, &iv).ok_or_else(|| "Decryption failed".to_string())?;
        if dec.len() < 4 { return Err("Decrypted payload too small".to_string()); }
        let mut z = flate2::read::ZlibDecoder::new(&dec[4..]);
        let mut payload = Vec::new();
        z.read_to_end(&mut payload).map_err(|e| format!("Decompression failed: {}", e))?;
        self.log(&format!("  Payload: {} bytes", payload.len()));
        Ok((payload, iv))
    }

    fn re_encrypt_and_write(&self, cache_path: &Path, patched_payload: &[u8], iv: &[u8; 16]) -> std::io::Result<()> {
        self.log("  Re-encrypting..");
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(patched_payload)?;
        let compressed = enc.finish()?;
        let mut blob = Vec::with_capacity(4 + compressed.len());
        blob.extend_from_slice(&(patched_payload.len() as u32).to_le_bytes());
        blob.extend_from_slice(&compressed);
        let new_ct = crypto::aes_cbc_encrypt(&blob, &AES_KEY, iv);
        let mut output = Vec::with_capacity(16 + new_ct.len());
        output.extend_from_slice(iv);
        output.extend_from_slice(&new_ct);
        file_util::atomic_write_all_bytes(cache_path, &output)
    }

    fn backup(&self, path: &Path) {
        let orig = file_util::with_extension_suffix(path, ".orig");
        if !orig.exists() {
            if file_util::atomic_copy(path, &orig).is_ok() { self.log(&format!("  Original saved to {}", orig.display())); }
        }
        let bak = file_util::with_extension_suffix(path, ".bak");
        if file_util::atomic_copy(path, &bak).is_ok() { self.log(&format!("  Backed up to {}", bak.display())); }
    }

    fn backup_both(&self, first: &Path, second: &Path) { self.backup(first); self.backup(second); }
}

fn bytes_match(data: &[u8], data_offset: i64, pattern: &[u8]) -> bool {
    if data_offset < 0 || data_offset as usize + pattern.len() > data.len() { return false; }
    &data[data_offset as usize..data_offset as usize + pattern.len()] == pattern
}

fn hex_dump(data: &[u8], offset: i64) -> String {
    if offset < 0 || offset as usize >= data.len() { return "(out of bounds)".to_string(); }
    let avail = (data.len() - offset as usize).min(16);
    data[offset as usize..offset as usize + avail].iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("-")
}

fn apply_patches(data: &[u8], patches: &[PatchEntry]) -> (Vec<u8>, usize, usize, Vec<String>) {
    let mut buf = data.to_vec();
    let mut applied = 0; let mut skipped = 0; let mut errors = Vec::new();
    for p in patches {
        if bytes_match(&buf, p.offset, &p.replacement) { skipped += 1; }
        else if bytes_match(&buf, p.offset, &p.original) {
            let off = p.offset as usize;
            buf[off..off + p.replacement.len()].copy_from_slice(&p.replacement);
            applied += 1;
        } else {
            errors.push(format!("  Mismatch at 0x{:X}: expected {}, got {}",
                p.offset,
                p.original.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("-"),
                hex_dump(&buf, p.offset)
            ));
        }
    }
    (buf, applied, skipped, errors)
}

fn find_steamtools_exe() -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(r"Software\Valve\Steamtools").ok()?;
    let raw: String = key.get_value("SteamPath").ok()?;
    let path = PathBuf::from(raw.replace('/', "\\")).join("SteamTools.exe");
    if path.is_file() { Some(path) } else { None }
}

fn kill_steamtools(mut log: impl FnMut(&str)) {
    use std::os::windows::process::CommandExt;
    let mut tasklist = std::process::Command::new("tasklist");
    tasklist.creation_flags(0x08000000);
    let out = tasklist.args(["/FI", "IMAGENAME eq SteamTools.exe", "/NH"]).output();
    let running = out.map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("steamtools.exe")).unwrap_or(false);
    if running {
        log("  Killing SteamTools.exe...");
        let mut kill = std::process::Command::new("taskkill");
        kill.creation_flags(0x08000000);
        let _ = kill.args(["/F", "/IM", "SteamTools.exe"]).output();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("CloudRedirect/2.2.2")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}
