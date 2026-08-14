// Removed log use
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::collections::HashSet;
use once_cell::sync::Lazy;

static DOWNLOADING_APPS: Lazy<Mutex<HashSet<u32>>> = Lazy::new(|| Mutex::new(HashSet::new()));

use tauri::{command, AppHandle, Manager};
use winreg::enums::*;
use winreg::RegKey;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use chrono::Utc;
use base64::Engine as _;

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
pub fn remove_from_steam(app: AppHandle, appid: u32) -> Result<(), String> {
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
    
    crate::lua_live::forget_lua_game(&app, appid)?;
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
pub fn force_restart_steam(
    app: AppHandle,
    post_restart_action: Option<String>,
) -> Result<(), String> {
    println!("Restarting steam...");
    let _ = Command::new("taskkill")
        .args(&["/F", "/IM", "steam.exe"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();
    
    // Đợi 2.5 giây để Steam cũ chết hẳn (tránh bị lỗi single-instance mutex làm steam mới exit ngay lập tức)
    std::thread::sleep(std::time::Duration::from_millis(2500));

    // Steam is fully stopped at this boundary, so a matching installed core
    // will be the one loaded by the process started below.
    let _ = crate::lua_live::reconcile_core_readiness(&app);

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




#[allow(dead_code)]
fn check_steam_update_blocking(appid: u32) -> Result<UpdateCheckResult, String> {
    let client = Client::new();
    
    // Only check HuggingFace - no more Hubcap
    let url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua/{}.lua", appid);
    let resp = client.get(&url).header("Range", "bytes=0-1024").send();
    
    let mut needs_update = false;
    let mut is_missing = false;
    let reason: String;
    
    match resp {
        Ok(response) if response.status().is_success() => {
            reason = "File lua tồn tại trên HuggingFace".to_string();
            // File exists, no update needed unless user forces
        }
        _ => {
            needs_update = true;
            is_missing = true;
            reason = "Không tìm thấy file lua trên HuggingFace".to_string();
        }
    }
    
    Ok(UpdateCheckResult { needs_update, reason, is_missing })
}

#[command]
pub async fn check_steam_update(app: AppHandle, appid: u32) -> Result<UpdateCheckResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (needs_update, reason, is_missing) =
            crate::lua_live::compatibility_update_state(&app, appid)?;
        Ok(UpdateCheckResult {
            needs_update,
            reason,
            is_missing,
        })
    })
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn add_to_steam(app: AppHandle, appid: u32, force_update: bool) -> Result<(), String> {
    let _ = force_update;
    {
        let mut apps = DOWNLOADING_APPS.lock().unwrap();
        if !apps.insert(appid) {
            return Err("Already downloading".to_string());
        }
    }

    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::lua_live::install_live_compat(app, appid).map(|_| ())
    })
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut apps = DOWNLOADING_APPS.lock().unwrap();
        apps.remove(&appid);
    }

    result
}

#[allow(dead_code)]
fn add_to_steam_internal(appid: u32, force_update: bool) -> Result<(), String> {
    use std::io::Cursor;
    use zip::ZipArchive;

    let _ = force_update;
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let client = Client::builder().timeout(std::time::Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
    
    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    let depotcache_dir = steam_path.join("depotcache");
    
    fs::create_dir_all(&stplug_in_dir).map_err(|e| format!("Failed to create stplug-in directory: {}", e))?;
    fs::create_dir_all(&depotcache_dir).map_err(|e| format!("Failed to create depotcache directory: {}", e))?;
    
    let token = get_hf_token().unwrap_or_default();
    
    // ALWAYS use HuggingFace directly - no more Hubcap API
    let url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/manifests/{}.zip", appid);
    let mut req = client.get(&url).timeout(std::time::Duration::from_secs(120));
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    let mut response = req.send().map_err(|e| format!("Lỗi tải dữ liệu: {}", e))?;

    if !response.status().is_success() {
        // Fallback to lua-manifest games/ folder
        let fallback_url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua-manifest%20games/{}.zip", appid);
        let mut fallback_req = client.get(&fallback_url).timeout(std::time::Duration::from_secs(120));
        if !token.is_empty() {
            fallback_req = fallback_req.header("Authorization", format!("Bearer {}", token));
        }
        response = fallback_req.send().map_err(|e| format!("Lỗi tải dữ liệu: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("Tải dữ liệu thất bại: {}", response.status()));
        }
    }
    
    let zip_bytes = response.bytes().map_err(|e| e.to_string())?.to_vec();
    
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
                let mut outfile = fs::File::create(&dest).map_err(|e| format!("Failed to create lua file: {}", e))?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| format!("Failed to write lua file: {}", e))?;
            } else if file_name.ends_with(".manifest") {
                let dest = depotcache_dir.join(file_name);
                let mut outfile = fs::File::create(&dest).map_err(|e| format!("Failed to create manifest file: {}", e))?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| format!("Failed to write manifest file: {}", e))?;
            }
        }
    }
    
    // Always download lua file separately from HuggingFace to ensure it exists
    let expected_lua = stplug_in_dir.join(format!("{}.lua", appid));
    
    if !expected_lua.exists() {
        let lua_url = format!("https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua/{}.lua", appid);
        let mut req = client.get(&lua_url).timeout(std::time::Duration::from_secs(120));
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        
        let resp = req.send().map_err(|e| format!("Failed to download lua file: {}", e))?;
        
        if !resp.status().is_success() {
            return Err(format!("Không thể tải file lua cho AppID {} (HTTP {})", appid, resp.status()));
        }
        
        let lua_bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        
        if lua_bytes.is_empty() {
            return Err(format!("File lua cho AppID {} rỗng", appid));
        }
        
        fs::write(&expected_lua, &lua_bytes).map_err(|e| format!("Failed to write lua file: {}", e))?;
    }
    
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
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("API Step 2 request failed: {}", e));
            return Err(format!("Lỗi kết nối máy chủ: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"));
        }
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("API Step 2 returned non-success: {}", status));
        return if status.as_u16() == 404 { Ok(vec![]) }
               else { Err(format!("Lỗi máy chủ: {}", status)) };
    }
    
    let text = match resp.text() {
        Ok(t) => t,
        Err(e) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("API Step 2 get text failed: {}", e));
            return Err(format!("Lỗi xử lý dữ liệu: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"));
        }
    };
    let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("API Step 2 response: {}", text));
    
    let build_entries: Vec<HfTreeItem> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::write("E:\\007Launcher\\hf_debug.txt", format!("API Step 2 json parse failed: {}", e));
            return Err(format!("Lỗi phân tích dữ liệu: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"));
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
    let resp = req.send().map_err(|e| format!("Lỗi tải dữ liệu từ máy chủ: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"))?;
    if !resp.status().is_success() {
        return Err(if resp.status().as_u16() == 404 {
            "Không tìm thấy dữ liệu trên máy chủ.".to_string()
        } else {
            format!("Máy chủ trả về mã lỗi HTTP {}", resp.status())
        });
    }
    let bytes = resp.bytes().map_err(|e| format!("Lỗi đọc dữ liệu: {}", e))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(dest, &bytes).map_err(|e| format!("Lỗi ghi dữ liệu: {}", e))
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
        .ok_or_else(|| format!("Game với AppID {} không tìm thấy trên máy chủ. Vui lòng tải dữ liệu lên trước.", appid))?;

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
        .map_err(|e| format!("Lỗi kết nối máy chủ dữ liệu: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"))?
        .json()
        .map_err(|e| format!("Lỗi phân tích dữ liệu máy chủ: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ"))?;

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
        return Err(format!("Không tìm thấy manifests cho BuildID {} trên máy chủ", build_id));
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
                message: Some(format!("Lỗi tải manifest: {}", e).replace("huggingface", "máy chủ").replace("HuggingFace", "máy chủ").replace("github", "máy chủ")),
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

#[derive(Serialize)]
pub struct SteamGameInfo {
    pub name: String,
    pub header_image: String,
}

#[derive(Serialize)]
pub struct SteamStoreSearchItem {
    pub id: u32,
    pub name: String,
    pub header_image: String,
}

#[command]
pub async fn search_steam_store(term: String) -> Result<Vec<SteamStoreSearchItem>, String> {
    tauri::async_runtime::spawn_blocking(move || search_steam_store_blocking(&term))
        .await
        .map_err(|e| e.to_string())?
}

fn search_steam_store_blocking(term: &str) -> Result<Vec<SteamStoreSearchItem>, String> {
    let query = term.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) 0xoLauncher/1.0")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::COOKIE,
                reqwest::header::HeaderValue::from_static("birthtime=568022401; lastagecheckage=1-January-1988; mature_content=1")
            );
            headers
        })
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get("https://store.steampowered.com/api/storesearch/")
        .query(&[("term", query), ("cc", "us"), ("l", "english")])
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Steam search failed: {}", response.status()));
    }

    let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    let items = json
        .get("items")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Steam search response missing items".to_string())?;

    let mut results = Vec::new();
    for item in items.iter().take(25) {
        if item.get("type").and_then(|value| value.as_str()) != Some("app") {
            continue;
        }
        let Some(id) = item
            .get("id")
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        let Some(name) = item.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        results.push(SteamStoreSearchItem {
            id,
            name: name.to_string(),
            header_image: format!(
                "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{}/header.jpg",
                id
            ),
        });
    }

    Ok(results)
}

fn fetch_steam_game_info_blocking(appid: u32) -> Result<SteamGameInfo, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) 0xoAssetBuilderLocal/1.0")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::COOKIE,
                reqwest::header::HeaderValue::from_static("birthtime=568022401; lastagecheckage=1-January-1988; mature_content=1")
            );
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/json,text/plain,*/*")
            );
            headers.insert(
                reqwest::header::ACCEPT_LANGUAGE,
                reqwest::header::HeaderValue::from_static("en-US,en;q=0.9")
            );
            headers
        })
        .build()
        .map_err(|e| e.to_string())?;

    let countries = ["us", "sg", "gb", "jp", "kr", "tw", "hk", "th", "vn", "de", "fr", "ca", "au"];
    let mut last_error = String::new();

    for cc in countries.iter() {
        let url = format!(
            "https://store.steampowered.com/api/appdetails?appids={}&cc={}&l=english",
            appid, cc
        );

        let resp = match client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                last_error = e.to_string();
                continue;
            }
        };

        if !resp.status().is_success() {
            last_error = format!("Status {}", resp.status());
            continue;
        }

        let json: serde_json::Value = match resp.json() {
            Ok(j) => j,
            Err(_) => continue,
        };

        let app_str = appid.to_string();
        let entry = match json.get(&app_str) {
            Some(e) => e,
            None => continue,
        };

        if !entry.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            last_error = "success=false".to_string();
            continue;
        }

        let data = match entry.get("data") {
            Some(d) => d,
            None => continue,
        };

        let name = data.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let header_image = data.get("header_image")
            .and_then(|v| v.as_str())
            .filter(|value| value.starts_with("https://") || value.starts_with("http://"))
            .unwrap_or_default()
            .to_string();

        return Ok(SteamGameInfo { name, header_image });
    }

    Err(format!("Could not fetch info for {}: {}", appid, last_error))
}

#[command]
pub async fn fetch_steam_game_name(appid: u32) -> Result<SteamGameInfo, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_steam_game_info_blocking(appid))
        .await
        .map_err(|error| format!("Steam metadata task failed: {error}"))?
}



#[command]
pub async fn list_available_manifests(force: Option<bool>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        list_available_manifests_blocking(force.unwrap_or(false))
    })
        .await
        .map_err(|e| e.to_string())?
}

fn list_available_manifests_blocking(force: bool) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    
    let token = get_hf_token().unwrap_or_default();
    
    let cache_buster = if force {
        format!("?refresh={}", Utc::now().timestamp_millis())
    } else {
        String::new()
    };
    let url = format!(
        "https://huggingface.co/api/datasets/Immaking/Luas/tree/main/manifests{}",
        cache_buster
    );
    let mut req = client.get(url);
    if force {
        req = req
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache");
    }
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    
    let response = req.send().map_err(|e| e.to_string())?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to fetch manifests: {}", response.status()));
    }
    
    // HuggingFace API returns a different format - object with "value" array
    let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    
    // Extract files array from response
    let files = json.as_array()
        .or_else(|| json.get("value").and_then(|v| v.as_array()))
        .ok_or("Invalid response format")?;
    
    let mut appids = Vec::new();
    for file in files {
        if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
            if path.ends_with(".zip") {
                let file_name = path.split('/').last().unwrap_or("");
                if let Some(appid_str) = file_name.strip_suffix(".zip") {
                    appids.push(appid_str.to_string());
                }
            }
        }
    }
    
    Ok(appids)
}
