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

use tauri::command;
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
    let libraryfolders_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    
    let mut apps = Vec::new();
    if let Ok(text) = fs::read_to_string(&libraryfolders_path) {
        for line in text.lines() {
            let fields = quoted_fields(line);
            if fields.len() >= 2 && fields[1].chars().all(|c| c.is_ascii_digit()) {
                if let Ok(appid) = fields[0].parse::<u32>() {
                    apps.push(appid);
                }
            }
        }
    }
    
    Ok(apps)
}