use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};

// We reuse the PeSection from cloud_redirect::pe to parse sections
use crate::cloud_redirect::pe::PeSection;

#[derive(Debug)]
struct TomlEntry {
    key: String,
    name: String,
    rva: String,
    sig: String,
}

fn parse_sig(sig_str: &str) -> (Vec<u8>, Vec<bool>) {
    let mut pattern = Vec::new();
    let mut mask = Vec::new();
    for p in sig_str.trim().split_whitespace() {
        if p == "??" || p == "?" {
            pattern.push(0);
            mask.push(false);
        } else if let Ok(val) = u8::from_str_radix(p, 16) {
            pattern.push(val);
            mask.push(true);
        }
    }
    (pattern, mask)
}

fn scan_pattern(data: &[u8], start: usize, end: usize, pattern: &[u8], mask: &[bool]) -> Option<usize> {
    let p_len = pattern.len();
    if start + p_len > end || start + p_len > data.len() {
        return None;
    }
    let limit = end.min(data.len()) - p_len;
    for i in start..=limit {
        let mut matched = true;
        for j in 0..p_len {
            if mask[j] && data[i + j] != pattern[j] {
                matched = false;
                break;
            }
        }
        if matched {
            return Some(i);
        }
    }
    None
}

fn parse_toml_entries(content: &str) -> Vec<TomlEntry> {
    let mut entries = Vec::new();
    let mut current: Option<TomlEntry> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(TomlEntry {
                key: trimmed[1..trimmed.len() - 1].to_string(),
                name: String::new(),
                rva: String::new(),
                sig: String::new(),
            });
        } else if let Some(ref mut curr) = current {
            if trimmed.starts_with("name =") || trimmed.starts_with("name=") {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                if parts.len() == 2 {
                    curr.name = parts[1].trim().trim_matches('"').to_string();
                }
            } else if trimmed.starts_with("rva =") || trimmed.starts_with("rva=") {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                if parts.len() == 2 {
                    curr.rva = parts[1].trim().trim_matches('"').to_string();
                }
            } else if trimmed.starts_with("sig =") || trimmed.starts_with("sig=") {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                if parts.len() == 2 {
                    curr.sig = parts[1].trim().trim_matches('"').to_string();
                }
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    entries
}

pub fn generate_pattern_toml(template_content: &str, target_dll_path: &Path, pattern_dir: &Path, sub_dir: &str) -> Result<(), String> {
    let dll_buffer = fs::read(target_dll_path).map_err(|e| format!("Failed to read DLL: {}", e))?;
    let mut hasher = Sha256::new();
    hasher.update(&dll_buffer);
    let dll_sha = format!("{:x}", hasher.finalize());

    let file_name = format!("{}.toml", dll_sha);
    let root_path = pattern_dir.join(&file_name);
    let sub_path = pattern_dir.join(sub_dir).join(&file_name);

    if root_path.exists() {
        return Ok(());
    }

    let sections = PeSection::parse(&dll_buffer);
    let text_section = PeSection::find(&sections, ".text")
        .cloned()
        .or_else(|| sections.first().cloned())
        .unwrap_or(PeSection {
            name: String::new(),
            virtual_size: 0,
            virtual_address: 0,
            raw_size: 0,
            raw_offset: 0,
            characteristics: 0,
        });

    let template_entries = parse_toml_entries(template_content);
    let mut results = Vec::new();
    let mut matched_count = 0;

    let start = text_section.raw_offset as usize;
    let end = (text_section.raw_offset + text_section.raw_size) as usize;

    for entry in &template_entries {
        if entry.sig.is_empty() {
            continue;
        }
        let (pattern, mask) = parse_sig(&entry.sig);
        let mut file_offset = scan_pattern(&dll_buffer, start, end, &pattern, &mask);

        if file_offset.is_none() && pattern.len() > 20 {
            let short_len = (pattern.len() - 8).min((pattern.len() as f32 * 0.75) as usize).max(16);
            let short_pattern = &pattern[0..short_len];
            let short_mask = &mask[0..short_len];
            file_offset = scan_pattern(&dll_buffer, start, end, short_pattern, short_mask);
        }

        if let Some(offset) = file_offset {
            let rva = offset as u32 - text_section.raw_offset + text_section.virtual_address;
            let rva_hex = format!("0x{:X}", rva);
            results.push(TomlEntry {
                key: entry.key.clone(),
                name: entry.name.clone(),
                rva: rva_hex,
                sig: entry.sig.clone(),
            });
            matched_count += 1;
        }
    }

    let mut out_toml = String::new();
    for r in results {
        out_toml.push_str(&format!("[{}]\nname = \"{}\"\nrva = \"{}\"\nsig = \"{}\"\n\n", r.key, r.name, r.rva, r.sig));
    }

    let _ = fs::write(&root_path, &out_toml);
    let _ = fs::write(&sub_path, &out_toml);

    Ok(())
}

pub fn auto_adapt_steam_patterns() -> Result<(), String> {
    let steam_path = crate::steam_integration::find_steam_root().unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam"));
    let pattern_dir = steam_path.join("_0xolemoncore").join("pattern");
    
    let _ = fs::create_dir_all(pattern_dir.join("steamclient"));
    let _ = fs::create_dir_all(pattern_dir.join("steamui"));
    let _ = fs::create_dir_all(pattern_dir.join("steamclientipc"));

    // Embed templates
    let client_template = include_str!("../resources/emu/pattern_templates/steamclient.toml");
    let ui_template = include_str!("../resources/emu/pattern_templates/steamui.toml");
    let ipc_template = include_str!("../resources/emu/pattern_templates/steamclientipc.toml");

    let client_dll = steam_path.join("steamclient64.dll");
    if client_dll.exists() {
        let _ = generate_pattern_toml(client_template, &client_dll, &pattern_dir, "steamclient");
        if let Ok(bytes) = fs::read(&client_dll) {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let sha = format!("{:x}", hasher.finalize());
            let ipc_path = pattern_dir.join("steamclientipc").join(format!("{sha}.toml"));
            if !ipc_path.exists() {
                let _ = fs::write(&ipc_path, ipc_template);
            }
        }
    }

    let ui_dll = steam_path.join("steamui.dll");
    if ui_dll.exists() {
        let _ = generate_pattern_toml(ui_template, &ui_dll, &pattern_dir, "steamui");
    }

    Ok(())
}
