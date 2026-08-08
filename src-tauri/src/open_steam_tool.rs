use std::path::PathBuf;
use std::fs;
use tauri::{AppHandle, Manager};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OSTConfig {
    pub log_level: String,
    pub manifest_url: String,
    pub enable_stats_api: bool,
    pub enable_inject: bool,
}

impl Default for OSTConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            manifest_url: "opensteamtool".to_string(),
            enable_stats_api: true,
            enable_inject: true,
        }
    }
}

pub fn get_steam_root() -> Result<PathBuf, String> {
    crate::steam::get_steam_path().ok_or_else(|| "Steam installation not found".to_string())
}

fn resolve_hook_resource_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|_| "Engine resources are unavailable".to_string())?;

    let mut candidates = vec![
        // Tauri v2 preserves the configured resource path when resources are
        // listed as strings, so resources/steam_hooks/* lands here.
        resource_root.join("resources").join("steam_hooks"),
        // Development/portable fallback used by older launcher builds.
        resource_root.join("steam_hooks"),
    ];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("resources").join("steam_hooks"));
        }
    }

    const REQUIRED: [&str; 4] = ["0xoCore.dll", "0xoPayload.dll", "dwmapi.dll", "xinput1_4.dll"];
    candidates
        .into_iter()
        .find(|dir| REQUIRED.iter().all(|file| dir.join(file).is_file()))
        .ok_or_else(|| "Engine resources are unavailable".to_string())
}

#[tauri::command]
pub async fn ost_install_hook(app: tauri::AppHandle) -> Result<(), String> {
    let steam_root = get_steam_root().map_err(|_| "Steam installation was not found".to_string())?;
    let resource_dir = resolve_hook_resource_dir(&app)?;
    const DLLS: [&str; 4] = ["0xoCore.dll", "0xoPayload.dll", "dwmapi.dll", "xinput1_4.dll"];

    // Preflight first so a missing bundled file can never leave a half-installed engine.
    if !DLLS.iter().all(|dll| resource_dir.join(dll).is_file()) {
        return Err("Engine resources are unavailable".to_string());
    }

    for dll in DLLS {
        let src = resource_dir.join(dll);
        let dest = steam_root.join(dll);
        fs::copy(&src, &dest).map_err(|_| {
            "Engine installation failed. Close Steam and try again.".to_string()
        })?;
    }

    let lua_dir = steam_root.join("config").join("lua");
    fs::create_dir_all(&lua_dir)
        .map_err(|_| "Engine installation failed. Close Steam and try again.".to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn ost_remove_hook(_app: AppHandle) -> Result<(), String> {
    let steam_root = get_steam_root().map_err(|_| "Steam installation was not found".to_string())?;
    const DLLS: [&str; 4] = ["0xoCore.dll", "0xoPayload.dll", "dwmapi.dll", "xinput1_4.dll"];

    for dll in DLLS {
        let dest = steam_root.join(dll);
        if dest.exists() {
            fs::remove_file(&dest)
                .map_err(|_| "Engine removal failed. Close Steam and try again.".to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn ost_check_hook_status(app: tauri::AppHandle) -> Result<bool, String> {
    let steam_root = get_steam_root().map_err(|_| "Steam installation was not found".to_string())?;
    const DLLS: [&str; 4] = ["0xoCore.dll", "0xoPayload.dll", "dwmapi.dll", "xinput1_4.dll"];
    
    if !DLLS.iter().all(|dll| steam_root.join(dll).is_file()) {
        return Ok(false);
    }

    let resource_dir = match resolve_hook_resource_dir(&app) {
        Ok(dir) => dir,
        Err(_) => return Ok(false),
    };

    for dll in DLLS {
        let dest = steam_root.join(dll);
        let src = resource_dir.join(dll);

        let dest_meta = match fs::metadata(&dest) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };
        let src_meta = match fs::metadata(&src) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };

        if dest_meta.len() != src_meta.len() {
            return Ok(false);
        }

        let dest_bytes = match fs::read(&dest) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };
        let src_bytes = match fs::read(&src) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };

        if dest_bytes != src_bytes {
            return Ok(false);
        }
    }

    Ok(true)
}

#[tauri::command]
pub async fn ost_save_lua(app: AppHandle, appid: String, content: String) -> Result<(), String> {
    let steam_root = get_steam_root()?;
    let lua_dir = steam_root.join("config").join("lua");
    if !lua_dir.exists() {
        fs::create_dir_all(&lua_dir).map_err(|e| format!("Failed to create lua dir: {}", e))?;
    }
    
    let file_path = lua_dir.join(format!("{}.lua", appid));
    fs::write(&file_path, content).map_err(|e| format!("Failed to write lua script: {}", e))?;
    Ok(())
}
