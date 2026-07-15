// Removed log use
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::collections::HashSet;
use once_cell::sync::Lazy;

static DOWNLOADING_APPS: Lazy<Mutex<HashSet<u32>>> = Lazy::new(|| Mutex::new(HashSet::new()));

use tauri::{command, Manager};
use winreg::enums::*;
use winreg::RegKey;
use reqwest::blocking::Client;
use zip::ZipArchive;
use serde::{Deserialize, Serialize};
use regex::Regex;
use chrono::{DateTime, Utc, NaiveDateTime};
use base64::{Engine as _, engine::general_purpose};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LuaVersionInfo {
    pub hf_date: Option<String>,
    pub hubcap_date: Option<String>,
    pub needs_update: bool,
    pub reason: String,
}

#[derive(Deserialize)]
struct HubcapStatus {
    manifest_file_exists: Option<bool>,
    file_size: Option<u64>,
    file_modified: Option<String>,
    file_age_days: Option<f64>,
    needs_update: Option<bool>,
    game_name: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateCheckResult {
    pub needs_update: bool,
    pub reason: String,
    pub is_missing: bool,
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct RepoConfig {
    repoId: String,
    token: String,
}

#[derive(Deserialize)]
struct ReposConfig {
    repositories: Vec<RepoConfig>,
    #[serde(default)]
    hubcap_keys: Vec<String>,
}

fn get_hf_token() -> Option<String> {
    let json_str = include_str!("../huggingface-repos.json");
    let config: ReposConfig = serde_json::from_str(json_str).ok()?;
    for repo in config.repositories {
        if repo.repoId == "Immaking/Luas" {
            return Some(repo.token);
        }
    }
    None
}

pub fn get_steam_path() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let steam_key = hkcu.open_subkey("Software\\Valve\\Steam").ok()?;
    let steam_path: String = steam_key.get_value("SteamPath").ok()?;
    Some(PathBuf::from(steam_path))
}

#[command]
pub fn check_steam_status(appid: u32) -> Result<bool, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let lua_path = steam_path.join("config").join("stplug-in").join(format!("{}.lua", appid));
    Ok(lua_path.exists())
}

#[command]
pub fn remove_from_steam(appid: u32) -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    
    // 1. Remove .lua file and update .sync_state
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    let lua_path = stplug_in_dir.join(format!("{}.lua", appid));
    if lua_path.exists() {
        if let Err(e) = fs::remove_file(&lua_path) {
            return Err(format!("Lỗi xóa file lua: {}", e));
        }
        println!("Removed lua for {}", appid);
    }
    
    let sync_state_path = stplug_in_dir.join(".sync_state");
    if sync_state_path.exists() {
        if let Ok(content) = fs::read_to_string(&sync_state_path) {
            let target_line = format!("{}.lua", appid);
            let new_content: Vec<&str> = content.lines().filter(|&line| line.trim() != target_line).collect();
            let mut final_content = new_content.join("\n");
            if !final_content.ends_with('\n') && !final_content.is_empty() {
                final_content.push('\n');
            }
            let _ = fs::write(&sync_state_path, final_content);
            println!("Removed {} from .sync_state", target_line);
        }
    }
    
    // 2 & 3. Remove .manifest files in depotcache AND appmanifest in steamapps
    let steamapps_dir = steam_path.join("steamapps");
    let appmanifest = steamapps_dir.join(format!("appmanifest_{}.acf", appid));
    let depotcache_dir = steam_path.join("depotcache");
    
    if appmanifest.exists() {
        // Read appmanifest to extract exact manifest IDs before deleting it
        if let Ok(content) = fs::read_to_string(&appmanifest) {
            let mut current_depot = String::new();
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("\"") && line.ends_with("\"") && !line.contains("manifest") && !line.contains("size") {
                    current_depot = line.trim_matches('"').to_string();
                } else if line.contains("\"manifest\"") && !current_depot.is_empty() {
                    let parts: Vec<&str> = line.split('"').collect();
                    if parts.len() >= 4 {
                        let manifest_id = parts[3];
                        let manifest_name = format!("{}_{}.manifest", current_depot, manifest_id);
                        let manifest_path = depotcache_dir.join(&manifest_name);
                        if manifest_path.exists() {
                            if let Err(e) = fs::remove_file(&manifest_path) {
                                return Err(format!("Lỗi xóa file manifest ({}): {}", manifest_name, e));
                            }
                            println!("Removed manifest {:?}", manifest_path);
                        }
                    }
                    current_depot.clear();
                }
            }
        }

        // Now remove the .acf file
        if let Err(e) = fs::remove_file(&appmanifest) {
            return Err(format!("Lỗi xóa file appmanifest: {}", e));
        }
        println!("Removed appmanifest {:?}", appmanifest);
    } else {
        // Fallback: if .acf doesn't exist but we still want to try deleting manifests by prefix (just in case)
        if let Ok(entries) = fs::read_dir(&depotcache_dir) {
            let prefix = format!("{}_", appid);
            let exact = format!("{}.manifest", appid);
            for entry in entries.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with(&prefix) || file_name == exact {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
    
    Ok(())
}

#[command]
pub fn list_installed_luas() -> Result<Vec<String>, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    if !stplug_in_dir.exists() {
        return Ok(vec![]);
    }
    let mut result = vec![];
    if let Ok(entries) = fs::read_dir(&stplug_in_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".lua") {
                let appid = file_name.trim_end_matches(".lua").to_string();
                result.push(appid);
            }
        }
    }
    result.sort();
    Ok(result)
}

#[command]
pub fn force_restart_steam(post_restart_action: Option<String>) -> Result<(), String> {
    println!("Restarting steam...");
    let _ = Command::new("taskkill")
        .args(&["/F", "/IM", "steam.exe"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();
    
    // Đợi 2.5 giây để Steam cũ chết hẳn (tránh bị lỗi single-instance mutex làm steam mới exit ngay lập tức)
    std::thread::sleep(std::time::Duration::from_millis(2500));

    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let steam_exe = steam_path.join("steam.exe");
    
    let mut cmd = Command::new(steam_exe);
    cmd.current_dir(&steam_path) // Bắt buộc phải set current_dir để Steam khởi động đúng thư mục
       .creation_flags(0x08000000); // CREATE_NO_WINDOW
       
    if let Some(action) = post_restart_action {
        cmd.arg(action);
    }
    
    cmd.spawn().map_err(|e| e.to_string())?;
        
    Ok(())
}




const ACN_BASE_URL: &str = "https://acn-m7nc.onrender.com";

#[derive(Deserialize)]
struct CommunityKeyResponse {
    ok: bool,
    key: String,
}

fn fetch_community_api_key(client: &Client) -> Result<String, String> {
    let url = format!("{}/api/hubcap/key", ACN_BASE_URL);
    let resp = client.get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .header("Cache-Control", "no-cache")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        if let Ok(data) = resp.json::<CommunityKeyResponse>() {
            if data.ok && !data.key.is_empty() {
                return Ok(data.key.trim().to_string());
            }
        }
    }
    Err("Failed to fetch community API key".into())
}

fn hubcap_api_call(client: &Client, endpoint: &str) -> Result<reqwest::blocking::Response, String> {
    let url = format!("https://hubcapmanifest.com{}", endpoint);
    
    for _ in 0..5 {
        let key = fetch_community_api_key(client)?;
        let resp = client.get(&url)
            .header("Authorization", format!("Bearer {}", key))
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            return Ok(resp);
        } else if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 || resp.status().as_u16() == 429 {
            println!("Hubcap key expired/rate-limited, rotating via backend...");
            continue;
        } else {
            return Err(format!("Hubcap API error: {}", resp.status()));
        }
    }
    Err("All Hubcap API keys failed (401/403/429)".into())
}

fn parse_hf_date(text: &str) -> Option<DateTime<Utc>> {
    let re_bot = Regex::new(r"--\s*Bot Last Updated:\s*(.+)").unwrap();
    let re_created = Regex::new(r"--\s*Created:\s*(.+)").unwrap();
    
    let date_str = if let Some(caps) = re_bot.captures(text) {
        caps.get(1).map(|m| m.as_str().trim())
    } else if let Some(caps) = re_created.captures(text) {
        caps.get(1).map(|m| m.as_str().trim())
    } else {
        None
    };
    
    if let Some(ds) = date_str {
        if let Ok(dt) = NaiveDateTime::parse_from_str(ds, "%Y-%m-%d %H:%M:%S") {
            return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
        }
        if let Ok(dt) = NaiveDateTime::parse_from_str(ds, "%Y-%m-%d %H:%M") {
            return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
        }
    }
    None
}

#[command]
pub fn check_steam_update(appid: u32) -> Result<UpdateCheckResult, String> {
    let client = Client::new();
    let mut hf_date = None;
    let mut hubcap_date = None;
    
    // Check HF
    let url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua/{}.lua", appid);
    if let Ok(resp) = client.get(&url).header("Range", "bytes=0-1024").send() {
        if resp.status().is_success() {
            if let Ok(text) = resp.text() {
                hf_date = parse_hf_date(&text);
            }
        }
    }
    
    // Check Hubcap using rotation
    let status_endpoint = format!("/api/v1/status/{}", appid);
    if let Ok(resp) = hubcap_api_call(&client, &status_endpoint) {
        if let Ok(status) = resp.json::<HubcapStatus>() {
            if let Some(fm) = status.file_modified {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&fm) {
                    hubcap_date = Some(dt.with_timezone(&Utc));
                }
            }
        }
    }
    
    let mut needs_update = false;
    let mut reason = String::new();
    let mut is_missing = false;
    
    if hf_date.is_none() {
        needs_update = true;
        is_missing = true;
        reason = "Không tìm thấy file trên HF hoặc không đọc được ngày giờ.".to_string();
    } else if let Some(hc) = hubcap_date {
        let hf = hf_date.unwrap();
        if hc > hf {
            needs_update = true;
            reason = format!("Bản trên Hubcap mới hơn HF (Hubcap: {} > HF: {})", hc.format("%Y-%m-%d %H:%M"), hf.format("%Y-%m-%d %H:%M"));
        } else {
            reason = "Bản trên HF đã là mới nhất.".to_string();
        }
    } else {
        reason = "Không lấy được thông tin từ Hubcap (API lỗi), dùng bản hiện có.".to_string();
    }
    
    Ok(UpdateCheckResult { needs_update, reason, is_missing })
}

#[command]
pub fn add_to_steam(appid: u32, force_update: bool) -> Result<(), String> {
    {
        let mut apps = DOWNLOADING_APPS.lock().unwrap();
        if !apps.insert(appid) {
            return Err("Already downloading".to_string());
        }
    }

    let result = add_to_steam_internal(appid, force_update);

    {
        let mut apps = DOWNLOADING_APPS.lock().unwrap();
        apps.remove(&appid);
    }

    result
}

fn add_to_steam_internal(appid: u32, force_update: bool) -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let client = Client::builder().timeout(std::time::Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
    
    let mut zip_bytes = Vec::new();
    let token = get_hf_token().unwrap_or_default();
    
    if force_update {
        // Step 1: Download manifest ZIP from Hubcap (force update from Steam)
        let manifest_endpoint = format!("/api/v1/manifest/{}?force_update=true", appid);
        println!("Fetching manifest from Hubcap: {}", manifest_endpoint);
        let mut manifest_resp = hubcap_api_call(&client, &manifest_endpoint)?;
        
        manifest_resp.read_to_end(&mut zip_bytes).map_err(|e| format!("Failed to read Hubcap manifest: {}", e))?;
    } else {
        // Step 1: Download from HF manifests/ folder first
        let url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/manifests/{}.zip", appid);
        println!("Downloading from HF manifests/: {}", url);
        let mut req = client.get(&url).timeout(std::time::Duration::from_secs(120));
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        let mut response = req.send().map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            // Fallback to lua-manifest games/ folder
            let fallback_url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua-manifest%20games/{}.zip", appid);
            println!("Downloading from HF lua-manifest games/: {}", fallback_url);
            let mut fallback_req = client.get(&fallback_url).timeout(std::time::Duration::from_secs(120));
            if !token.is_empty() {
                fallback_req = fallback_req.header("Authorization", format!("Bearer {}", token));
            }
            response = fallback_req.send().map_err(|e| format!("Request failed: {}", e))?;
            
            if !response.status().is_success() {
                return Err(format!("HF download failed: {}", response.status()));
            }
        }
        zip_bytes = response.bytes().map_err(|e| e.to_string())?.to_vec();
    }
    
    let reader = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid zip: {}", e))?;
    
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    let depotcache_dir = steam_path.join("depotcache");
    
    fs::create_dir_all(&stplug_in_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&depotcache_dir).map_err(|e| e.to_string())?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };

        if let Some(p) = outpath.parent() {
            if !p.exists() {
                let _ = fs::create_dir_all(p);
            }
        }

        let file_name = outpath.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file.is_file() {
            if file_name.ends_with(".lua") {
                let dest = stplug_in_dir.join(file_name);
                let mut outfile = fs::File::create(&dest).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                println!("Extracted: {:?}", dest);
            } else if file_name.ends_with(".manifest") {
                let dest = depotcache_dir.join(file_name);
                let mut outfile = fs::File::create(&dest).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                println!("Extracted: {:?}", dest);
            }
        }
    }
    
    // Download lua file manually if not already extracted (e.g. from Hubcap or HF manifests/ which lacks lua)
    let expected_lua = stplug_in_dir.join(format!("{}.lua", appid));
    if !expected_lua.exists() {
        let mut lua_bytes = Vec::new();
        if force_update {
            let lua_endpoint = format!("/api/v1/lua/{}", appid);
            if let Ok(mut resp) = hubcap_api_call(&client, &lua_endpoint) {
                let _ = resp.read_to_end(&mut lua_bytes);
            }
        } else {
            let lua_url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua/{}.lua", appid);
            let mut req = client.get(&lua_url).timeout(std::time::Duration::from_secs(120));
            if !token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", token));
            }
            if let Ok(mut resp) = req.send() {
                if resp.status().is_success() {
                    let _ = resp.read_to_end(&mut lua_bytes);
                }
            }
        }
        
        if !lua_bytes.is_empty() {
            let _ = fs::write(&expected_lua, &lua_bytes);
            println!("Downloaded separate lua to {:?}", expected_lua);
        }
    }
    
    // Update .sync_state file to notify Steam about new lua files
    update_sync_state(&stplug_in_dir)?;
    
    Ok(())
}

/// Update or create .sync_state file with all lua filenames in stplug-in directory
fn update_sync_state(stplug_in_dir: &Path) -> Result<(), String> {
    let sync_state_path = stplug_in_dir.join(".sync_state");
    
    // Collect all lua files in the directory
    let mut lua_files = Vec::new();
    if let Ok(entries) = fs::read_dir(stplug_in_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".lua") {
                    lua_files.push(name.to_string());
                }
            }
        }
    }
    
    if lua_files.is_empty() {
        return Ok(());
    }
    
    // Sort for consistent output
    lua_files.sort();
    
    // Write to .sync_state (one filename per line)
    let content = lua_files.join("\n") + "\n";
    fs::write(&sync_state_path, content)
        .map_err(|e| format!("Failed to write .sync_state: {}", e))?;
    
    println!("Updated .sync_state with {} lua files", lua_files.len());
    Ok(())
}

fn quoted_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if in_quote && character == '\\' {
            escaped = true;
            current.push(character);
            continue;
        }
        if character == '"' {
            if in_quote {
                fields.push(current.clone());
                current.clear();
            }
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            current.push(character);
        }
    }
    fields
}

#[command]
pub fn get_installed_steam_apps() -> Result<Vec<u32>, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    
    let mut apps = Vec::new();
    
    if !stplug_in_dir.exists() {
        return Ok(apps);
    }
    
    if let Ok(entries) = fs::read_dir(&stplug_in_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(appid) = stem.parse::<u32>() {
                        apps.push(appid);
                    }
                }
            }
        }
    }
    
    Ok(apps)
}


#[tauri::command]
pub fn install_lua_from_zip(appid: String, zip_data_base64: String) -> Result<(), String> {
    use std::io::Cursor;
    use zip::ZipArchive;
    
    // Decode base64 to bytes
    let zip_bytes = base64::engine::general_purpose::STANDARD
        .decode(&zip_data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    
    // Get Steam path
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    let depotcache_dir = steam_path.join("depotcache");
    
    fs::create_dir_all(&stplug_in_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&depotcache_dir).map_err(|e| e.to_string())?;
    
    let file_name = appid.to_lowercase();
    let is_lua = file_name.ends_with(".lua");
    let is_manifest = file_name.ends_with(".manifest");
    let is_zip = file_name.ends_with(".zip");
    let is_7z = file_name.ends_with(".7z");
    let is_rar = file_name.ends_with(".rar");

    if is_rar {
        return Err("Định dạng .rar chưa được hỗ trợ giải nén trực tiếp, vui lòng giải nén trước hoặc dùng .zip/.7z!".to_string());
    }

    if is_lua {
        let dest = stplug_in_dir.join(&appid);
        fs::write(&dest, &zip_bytes).map_err(|e| format!("Failed to write lua: {}", e))?;
        println!("Extracted lua directly: {:?}", dest);
    } else if is_manifest {
        let dest = depotcache_dir.join(&appid);
        fs::write(&dest, &zip_bytes).map_err(|e| format!("Failed to write manifest: {}", e))?;
        println!("Extracted manifest directly: {:?}", dest);
    } else if is_zip {
        // Extract zip
        let reader = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid zip: {}", e))?;
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let outpath = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            let file_name = outpath.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file.is_file() {
                if file_name.ends_with(".lua") {
                    let dest = stplug_in_dir.join(file_name);
                    let mut outfile = fs::File::create(&dest).map_err(|e| e.to_string())?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                    println!("Extracted lua: {:?}", dest);
                } else if file_name.ends_with(".manifest") {
                    let dest = depotcache_dir.join(file_name);
                    let mut outfile = fs::File::create(&dest).map_err(|e| e.to_string())?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                    println!("Extracted manifest: {:?}", dest);
                }
            }
        }
    } else if is_7z {
        // Extract 7z using sevenz_rust
        let reader = Cursor::new(zip_bytes);
        
        // sevenz_rust doesn't provide an easy streaming in-memory extraction for specific extensions, 
        // but we can extract everything to a temp dir and then move .lua and .manifest
        let temp_dir = std::env::temp_dir().join(format!("0xoLemon_7z_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        
        match sevenz_rust::decompress(reader, &temp_dir) {
            Ok(_) => {
                // Find all .lua and .manifest in temp_dir
                for entry in walkdir::WalkDir::new(&temp_dir).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if fname.ends_with(".lua") {
                            let dest = stplug_in_dir.join(fname);
                            let _ = fs::copy(path, &dest);
                            println!("Extracted lua from 7z: {:?}", dest);
                        } else if fname.ends_with(".manifest") {
                            let dest = depotcache_dir.join(fname);
                            let _ = fs::copy(path, &dest);
                            println!("Extracted manifest from 7z: {:?}", dest);
                        }
                    }
                }
            },
            Err(e) => {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(format!("Invalid 7z: {}", e));
            }
        }
        let _ = fs::remove_dir_all(&temp_dir);
    } else {
        return Err(format!("Unsupported file extension for: {}", appid));
    }
    
    // Update .sync_state file
    update_sync_state(&stplug_in_dir)?;
    
    Ok(())
}


// ─────────────────────────────────────────────────────────────────────────────
//  DEPOT PATCH — Version Switcher (HuggingFace-backed)
//
//  HF repo layout (admin uploads):
//    {hf_repo_id}/
//    └── {appid}/
//        ├── {appid}.key                           ← depot decryption key
//        ├── {appid}.token                         ← optional app token
//        └── BuildID_{buildid}/
//            ├── {depotA}_{manifestA}.manifest
//            └── {depotB}_{manifestB}.manifest
//
//  User flow:
//    1. Launcher fetches file list from HF API for {appid}/
//    2. UI shows available BuildIDs
//    3. User clicks Patch → launcher downloads manifest + key to temp dir
//    4. Launcher runs bundled DepotDownloaderMod sidecar
//    5. DepotDownloaderMod delta-patches the Steam game install
// ─────────────────────────────────────────────────────────────────────────────

/// One .manifest entry (info only, no local path yet)
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DepotManifestEntry {
    pub depot_id: String,
    pub manifest_id: String,
    /// Filename in HF repo: "{depotId}_{manifestId}.manifest"
    pub manifest_file: String,
}

/// A single buildID version with its list of depots
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DepotVersionEntry {
    pub build_id: String,
    pub depots: Vec<DepotManifestEntry>,
}

/// HuggingFace tree API response item
#[derive(Deserialize)]
struct HfTreeItem {
    path: String,
    #[serde(rename = "type")]
    item_type: String,
}


/// Base URL for HuggingFace dataset raw files
fn hf_raw_url(repo_id: &str, path: &str, _hf_token: &str) -> String {
    format!(
        "https://huggingface.co/datasets/{}/resolve/main/{}",
        repo_id, path
    )
}

/// Look up HF token for a given repo ID from huggingface-repos.json
fn get_hf_token_for(repo_id: &str) -> String {
    let json_str = include_str!("../huggingface-repos.json");
    #[derive(Deserialize)]
    struct Repo { #[serde(rename = "repoId")] repo_id: String, token: String }
    #[derive(Deserialize)]
    struct Config { repositories: Vec<Repo> }
    if let Ok(cfg) = serde_json::from_str::<Config>(json_str) {
        for r in cfg.repositories {
            if r.repo_id == repo_id {
                return r.token;
            }
        }
    }
    String::new()
}

/// Find the game subfolder under "Depotdownloader/" whose name ends with "({appid})".
/// Returns the full HF-relative path, e.g. "Depotdownloader/Hello Kitty Island Adventure (2495100)"
fn find_game_folder(
    client: &Client,
    repo_id: &str,
    appid: u32,
    token: &str,
) -> Option<String> {
    let url = format!(
        "https://huggingface.co/api/datasets/{}/tree/main/Depotdownloader/",
        repo_id
    );
    let mut req = client.get(&url);
    if !token.is_empty() { req = req.bearer_auth(token); }
    let items: Vec<HfTreeItem> = req.send().ok()?.json().ok()?;
    let suffix = format!("({})", appid);
    for item in items {
        if item.item_type == "directory" {
            let name = item.path.split('/').last().unwrap_or("");
            if name.ends_with(&suffix) {
                return Some(item.path);
            }
        }
    }
    None
}

/// Fetch all available build versions for an appid from HF repo.
/// HF structure:
///   Depotdownloader/{game} ({appid})/{appid}/BuildID_{id}/{depotId}_{manifestId}.manifest
/// Returns sorted list (newest BuildID first by numeric value).
#[command]
pub fn list_depot_versions(
    appid: u32,
    hf_repo_id: String,
) -> Result<Vec<DepotVersionEntry>, String> {
    if hf_repo_id.is_empty() {
        return Ok(vec![]);
    }

    let token = get_hf_token_for(&hf_repo_id);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client: {}", e))?;

    // ── Step 1: Find game folder under Depotdownloader/ ────────────────────
    let game_folder = match find_game_folder(&client, &hf_repo_id, appid, &token) {
        Some(f) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("Found game_folder: {}", f));
            f
        },
        None => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", "find_game_folder returned None");
            return Ok(vec![]);
        }
    };

    // ── Step 2: List {game_folder}/{appid}/ for BuildID folders ───────────
    // e.g. "Depotdownloader/Hello Kitty Island Adventure (2495100)/2495100"
    let appid_path = format!("{}/{}", game_folder, appid);
    let encoded_path = urlencoding::encode(&appid_path).replace("%2F", "/");
    let api_url = format!(
        "https://huggingface.co/api/datasets/{}/tree/main/{}/",
        hf_repo_id, encoded_path
    );
    let mut req = client.get(&api_url);
    if !token.is_empty() { req = req.bearer_auth(&token); }

    let resp = match req.send() {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("HF API Step 2 request failed: {}", e));
            return Err(format!("HF API: {}", e));
        }
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("HF API Step 2 returned non-success: {}", status));
        return if status.as_u16() == 404 { Ok(vec![]) }
               else { Err(format!("HF API error: {}", status)) };
    }
    
    let text = match resp.text() {
        Ok(t) => t,
        Err(e) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("HF API Step 2 get text failed: {}", e));
            return Err(format!("HF API parse: {}", e));
        }
    };
    let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("HF API Step 2 response: {}", text));
    
    let build_entries: Vec<HfTreeItem> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("HF API Step 2 json parse failed: {}", e));
            return Err(format!("HF API parse: {}", e));
        }
    };

    // ── Step 3: For each BuildID_ folder, list its manifests ───────────────
    let mut versions: Vec<DepotVersionEntry> = vec![];

    for entry in &build_entries {
        if entry.item_type != "directory" { continue; }
        let folder_name = entry.path.split('/').last().unwrap_or("");
        if !folder_name.starts_with("BuildID_") { continue; }
        let build_id = folder_name["BuildID_".len()..].to_string();

        let manifest_url = format!(
            "https://huggingface.co/api/datasets/{}/tree/main/{}/",
            hf_repo_id, entry.path
        );
        let mut mreq = client.get(&manifest_url);
        if !token.is_empty() { mreq = mreq.bearer_auth(&token); }
        let files: Vec<HfTreeItem> = match mreq.send().and_then(|r| r.json()) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mut depots: Vec<DepotManifestEntry> = vec![];
        for f in &files {
            if f.item_type != "file" { continue; }
            let fname = f.path.split('/').last().unwrap_or("").to_string();
            if !fname.ends_with(".manifest") { continue; }
            let stem = fname.trim_end_matches(".manifest");
            if let Some(idx) = stem.rfind('_') {
                let depot_id = stem[..idx].to_string();
                let manifest_id = stem[idx + 1..].to_string();
                if !depot_id.is_empty() && !manifest_id.is_empty() {
                    depots.push(DepotManifestEntry { depot_id, manifest_id, manifest_file: fname });
                }
            }
        }
        if !depots.is_empty() {
            depots.sort_by(|a, b| a.depot_id.cmp(&b.depot_id));
            versions.push(DepotVersionEntry { build_id, depots });
        }
    }

    versions.sort_by(|a, b| {
        let an: u64 = a.build_id.parse().unwrap_or(0);
        let bn: u64 = b.build_id.parse().unwrap_or(0);
        bn.cmp(&an)
    });

    Ok(versions)
}


/// Download a file from HF repo to a local path. Returns the local path.
fn hf_download_file(
    client: &Client,
    repo_id: &str,
    hf_path: &str,
    dest: &Path,
    hf_token: &str,
) -> Result<(), String> {
    let url = hf_raw_url(repo_id, hf_path, hf_token);
    let mut req = client.get(&url);
    if !hf_token.is_empty() {
        req = req.bearer_auth(hf_token);
    }
    let resp = req.send().map_err(|e| format!("Download failed {}: {}", url, e))?;
    if !resp.status().is_success() {
        return Err(format!("HF download {} → HTTP {}", hf_path, resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| format!("Read body error: {}", e))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(dest, &bytes).map_err(|e| format!("Write {} failed: {}", dest.display(), e))
}

/// Resolve path to the bundled DepotDownloaderMod sidecar.
/// Tauri names sidecars:  binaries/{name}-{target-triple}.exe
fn resolve_sidecar_exe(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    // Tauri resource dir contains sidecar as: DepotDownloaderMod-x86_64-pc-windows-msvc.exe
    let res_dir = app_handle.path().resource_dir().ok()?;
    let name = "DepotDownloaderMod-x86_64-pc-windows-msvc.exe";
    let candidate = res_dir.join(name);
    if candidate.is_file() {
        return Some(candidate);
    }
    // Dev mode: look relative to crate root
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name);
    if dev_path.is_file() {
        return Some(dev_path);
    }
    None
}

/// Progress event payload emitted to the frontend
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DepotPatchEvent {
    pub event_type: String,   // "start"|"depot-start"|"log"|"depot-done"|"complete"|"error"
    pub build_id: String,
    pub depot_id: Option<String>,
    pub message: Option<String>,
    pub index: Option<usize>,
    pub total: Option<usize>,
    pub success: Option<bool>,
}

/// Download manifests + key from HF, then run the bundled sidecar for each depot.
/// Emits "depot-patch-progress" Tauri events with DepotPatchEvent payloads.
#[command]
pub fn run_depot_patch(
    app_handle: tauri::AppHandle,
    appid: u32,
    build_id: String,
    hf_repo_id: String,
    install_dir: String,
) -> Result<String, String> {
    use std::io::BufRead;
    use tauri::Emitter;

    // ── 1. Resolve sidecar exe ──────────────────────────────────────────────
    let exe = resolve_sidecar_exe(&app_handle)
        .ok_or_else(|| "DepotDownloaderMod sidecar not found. Please reinstall the launcher.".to_string())?;

    let hf_token = get_hf_token_for(&hf_repo_id);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client: {}", e))?;

    // ── 2. Temp dir for downloaded files ────────────────────────────────────
    let tmp_dir = std::env::temp_dir().join(format!("0xo_depot_{}_{}", appid, build_id));
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("Cannot create temp dir: {}", e))?;

    // ── 3. Find game folder in HF (Depotdownloader/{game} ({appid})/) ───────
    let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
        event_type: "start".to_string(),
        build_id: build_id.clone(),
        depot_id: None,
        message: Some("Locating game depot in server…".to_string()),
        index: None, total: None, success: None,
    });

    let game_folder = find_game_folder(&client, &hf_repo_id, appid, &hf_token)
        .ok_or_else(|| format!("Game with AppID {} not found in HF repo {}. Upload depot files first.", appid, hf_repo_id))?;

    // Full HF base path for this game's appid subfolder:
    // Depotdownloader/{game} ({appid})/{appid}/
    let appid_path = format!("{}/{}", game_folder, appid);

    // ── 4. Download depot key ──────────────────────────────────────────────
    let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
        event_type: "start".to_string(),
        build_id: build_id.clone(),
        depot_id: None,
        message: Some("Downloading depot keys from server…".to_string()),
        index: None, total: None, success: None,
    });

    // Key path: Depotdownloader/{game} ({appid})/{appid}/{appid}.key
    let key_hf_path = format!("{}/{}.key", appid_path, appid);
    let key_local = tmp_dir.join(format!("{}.key", appid));
    hf_download_file(&client, &hf_repo_id, &key_hf_path, &key_local, &hf_token)?;

    // Token is optional
    let token_hf_path = format!("{}/{}.token", appid_path, appid);
    let token_local = tmp_dir.join(format!("{}.token", appid));
    let has_token = hf_download_file(&client, &hf_repo_id, &token_hf_path, &token_local, &hf_token).is_ok();

    // ── 5. Fetch manifest list for this BuildID ────────────────────────────
    let build_folder = format!("BuildID_{}", build_id);
    // Path: Depotdownloader/{game} ({appid})/{appid}/BuildID_{id}/
    let build_hf_path = format!("{}/{}", appid_path, build_folder);
    let api_url = format!(
        "https://huggingface.co/api/datasets/{}/tree/main/{}/",
        hf_repo_id, build_hf_path
    );
    let mut req = client.get(&api_url);
    if !hf_token.is_empty() { req = req.bearer_auth(&hf_token); }
    let items: Vec<HfTreeItem> = req.send()
        .map_err(|e| format!("HF manifest list failed: {}", e))?
        .json()
        .map_err(|e| format!("HF manifest list parse: {}", e))?;

    let mut manifests: Vec<DepotManifestEntry> = vec![];
    for item in &items {
        if item.item_type != "file" { continue; }
        let fname = item.path.split('/').last().unwrap_or("").to_string();
        if !fname.ends_with(".manifest") { continue; }
        let stem = fname.trim_end_matches(".manifest");
        if let Some(idx) = stem.rfind('_') {
            let depot_id = stem[..idx].to_string();
            let manifest_id = stem[idx + 1..].to_string();
            if !depot_id.is_empty() && !manifest_id.is_empty() {
                manifests.push(DepotManifestEntry { depot_id, manifest_id, manifest_file: fname });
            }
        }
    }
    manifests.sort_by(|a, b| a.depot_id.cmp(&b.depot_id));

    if manifests.is_empty() {
        return Err(format!("No manifests found for BuildID {} in HF repo", build_id));
    }


    let total = manifests.len();
    let mut any_failed = false;

    // ── 5. For each depot: download manifest → run sidecar ─────────────────
    for (i, entry) in manifests.iter().enumerate() {
        // Download manifest
        // Depotdownloader/{game} ({appid})/{appid}/BuildID_{id}/{depotId}_{manifestId}.manifest
        let manifest_hf_path = format!("{}/{}", build_hf_path, entry.manifest_file);
        let manifest_local = tmp_dir.join(&entry.manifest_file);

        let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
            event_type: "depot-start".to_string(),
            build_id: build_id.clone(),
            depot_id: Some(entry.depot_id.clone()),
            message: Some(format!("[{}/{}] Downloading manifest for depot {}…", i + 1, total, entry.depot_id)),
            index: Some(i + 1), total: Some(total), success: None,
        });

        if let Err(e) = hf_download_file(&client, &hf_repo_id, &manifest_hf_path, &manifest_local, &hf_token) {
            let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
                event_type: "error".to_string(),
                build_id: build_id.clone(),
                depot_id: Some(entry.depot_id.clone()),
                message: Some(format!("Failed to download manifest: {}", e)),
                index: Some(i + 1), total: Some(total), success: Some(false),
            });
            any_failed = true;
            continue;
        }

        // Build sidecar command
        let mut cmd = Command::new(&exe);
        cmd.arg("-app").arg(appid.to_string())
           .arg("-depot").arg(&entry.depot_id)
           .arg("-manifest").arg(&entry.manifest_id)
           .arg("-manifestfile").arg(&manifest_local)
           .arg("-dir").arg(&install_dir)
           .arg("-depotkeys").arg(&key_local)
           .arg("-max-downloads").arg("16");

        if has_token {
            // Read token value from file
            if let Ok(tok_str) = fs::read_to_string(&token_local) {
                let tok = tok_str.trim().to_string();
                if !tok.is_empty() {
                    cmd.arg("-apptoken").arg(tok);
                }
            }
        }

        cmd.creation_flags(0x08000000) // CREATE_NO_WINDOW
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
                    event_type: "error".to_string(),
                    build_id: build_id.clone(),
                    depot_id: Some(entry.depot_id.clone()),
                    message: Some(format!("Failed to launch sidecar: {}", e)),
                    index: Some(i + 1), total: Some(total), success: Some(false),
                });
                any_failed = true;
                continue;
            }
        };

        // Stream stdout line-by-line
        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() { continue; }
                let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
                    event_type: "log".to_string(),
                    build_id: build_id.clone(),
                    depot_id: Some(entry.depot_id.clone()),
                    message: Some(trimmed),
                    index: Some(i + 1), total: Some(total), success: None,
                });
            }
        }

        let success = child.wait().map(|s| s.success()).unwrap_or(false);
        if !success { any_failed = true; }

        let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
            event_type: "depot-done".to_string(),
            build_id: build_id.clone(),
            depot_id: Some(entry.depot_id.clone()),
            message: Some(if success {
                format!("✓ Depot {} patched", entry.depot_id)
            } else {
                format!("✗ Depot {} failed", entry.depot_id)
            }),
            index: Some(i + 1), total: Some(total), success: Some(success),
        });
    }

    // ── 6. Cleanup temp dir ─────────────────────────────────────────────────
    let _ = fs::remove_dir_all(&tmp_dir);

    let _ = app_handle.emit("depot-patch-progress", DepotPatchEvent {
        event_type: "complete".to_string(),
        build_id: build_id.clone(),
        depot_id: None,
        message: Some(if any_failed {
            format!("Patch completed with errors. Build: {}", build_id)
        } else {
            format!("✅ Game patched to build {}!", build_id)
        }),
        index: Some(total), total: Some(total), success: Some(!any_failed),
    });

    if any_failed {
        Err(format!("Some depots failed during patch to build {}", build_id))
    } else {
        Ok(format!("Successfully patched to build {}", build_id))
    }
}

