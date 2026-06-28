// CloudRedirect integration module.
// Exposes Tauri commands for STFixer + status checking.

pub mod crypto;
pub mod downloader;
pub mod file_util;
pub mod patcher;
pub mod pe;
pub mod signatures;
pub mod steam_detector;

use serde::{Deserialize, Serialize};
use tauri::command;

use patcher::Patcher;
use steam_detector::{
    find_steam_path, get_steam_version, is_steam_running, is_supported_steam_version,
    shutdown_steam, SUPPORTED_STEAM_VERSIONS,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRedirectStatus {
    pub steam_path: Option<String>,
    pub steam_version: Option<i64>,
    pub steam_version_supported: bool,
    pub steam_running: bool,
    pub core_dll_present: bool,
    pub cloud_redirect_dll_present: bool,
    pub stfixer_applied: bool,
    pub supported_versions: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StfixerResult {
    pub succeeded: bool,
    pub log: Vec<String>,
    pub error: Option<String>,
}

/// Cloud provider configuration read from CloudRedirect's config.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProviderConfig {
    pub provider: String,         // "gdrive" | "onedrive" | "folder" | "" (not configured)
    pub token_path: String,       // path to token file, empty if not set
    pub authenticated: bool,      // whether token file exists and is non-empty
    pub config_found: bool,       // whether config.json was found at all
}

/// Get current CloudRedirect / STFixer status.
#[command]
pub async fn cloud_redirect_get_status() -> CloudRedirectStatus {
    let steam_path = find_steam_path();
    let steam_version = steam_path.as_ref().and_then(|p| get_steam_version(p));
    let steam_version_supported = steam_version.map(is_supported_steam_version).unwrap_or(false);
    let steam_running = is_steam_running();

    let (core_dll_present, cloud_redirect_dll_present, stfixer_applied) =
        if let Some(ref sp) = steam_path {
            let patcher = Patcher::new(sp.clone());
            let core = patcher.has_core_dll();
            let cr_dll = sp.join("cloud_redirect.dll").is_file();
            // A rough check: if the CR DLL is present and core is patched, STFixer is applied.
            let applied = core && cr_dll;
            (core, cr_dll, applied)
        } else {
            (false, false, false)
        };

    CloudRedirectStatus {
        steam_path: steam_path.map(|p| p.to_string_lossy().into_owned()),
        steam_version,
        steam_version_supported,
        steam_running,
        core_dll_present,
        cloud_redirect_dll_present,
        stfixer_applied,
        supported_versions: SUPPORTED_STEAM_VERSIONS.to_vec(),
    }
}

/// Run the full STFixer flow (may take 30-60s due to downloads). Blocking.
#[command]
pub fn cloud_redirect_run_stfixer(install_core_if_missing: bool) -> StfixerResult {
    let steam_path = match find_steam_path() {
        Some(p) => p,
        None => {
            return StfixerResult {
                succeeded: false,
                log: vec!["ERROR: Steam installation not found.".to_string()],
                error: Some("Steam not found".to_string()),
            }
        }
    };

    let patcher = Patcher::new(steam_path.clone());
    let log_ref = patcher.log_lines.clone();

    let push = |msg: &str| {
        if let Ok(mut lines) = log_ref.lock() {
            lines.push(msg.to_string());
        }
    };

    push(&format!("Steam: {}", steam_path.display()));

    match get_steam_version(&steam_path) {
        None => push("WARNING: Could not read Steam version -- continuing anyway."),
        Some(v) if !is_supported_steam_version(v) => {
            push(&format!("WARNING: Steam version {} not in whitelist -- continuing anyway.", v))
        }
        Some(v) => push(&format!("Steam version: {} (OK)", v)),
    }

    // Close Steam if running.
    if is_steam_running() {
        push("Steam is running -- shutting it down...");
        shutdown_steam(&steam_path);
        if is_steam_running() {
            let result = collect_result(&log_ref, false, Some("Could not close Steam. Close it manually and retry.".to_string()));
            return result;
        }
        push("Steam closed.");
    }

    // Download core DLLs if missing.
    if !patcher.has_core_dll() {
        if install_core_if_missing {
            push("Downloading SteamTools core DLLs...");
            let repair = patcher.repair_core_dlls();
            if !repair.succeeded {
                return collect_result(&log_ref, false, repair.error);
            }
            push("Core DLLs OK.");
        } else {
            push("ERROR: SteamTools Core DLL not found.");
            push("Vui lòng check chọn 'Install SteamTools Core' nếu bạn muốn tự động cài đặt.");
            return collect_result(&log_ref, false, Some("SteamTools Core DLL missing".to_string()));
        }
    }

    // Apply STFixer patches.
    push("Applying STFixer patches...");
    let result = patcher.apply_offline_setup();
    if !result.succeeded {
        return collect_result(&log_ref, false, result.error);
    }
    push("STFixer patches applied.");

    // Patch SteamTools.exe.
    push("Patching SteamTools.exe...");
    match patcher.patch_steamtools_exe() {
        1 => push("SteamTools.exe patched."),
        0 => push("SteamTools.exe not installed -- skipped."),
        _ => push("WARNING: SteamTools.exe patch failed (see above). STFixer still applied."),
    }

    // Deploy cloud_redirect.dll.
    push("Deploying cloud_redirect.dll...");
    let dll_dest = steam_path.join("cloud_redirect.dll");
    if let Some(err) = downloader::download_cloud_redirect_dll(&dll_dest) {
        push(&format!("WARNING: Could not deploy cloud_redirect.dll: {}", err));
        push("You can download it manually from https://github.com/Selectively11/CloudRedirect/releases");
    } else {
        push(&format!("cloud_redirect.dll deployed to {}", dll_dest.display()));
    }

    // Enable auto-update config.
    enable_auto_update(&steam_path);
    push("DLL auto-update enabled.");

    push("All patches applied. Start Steam to use STFixer.");
    collect_result(&log_ref, true, None)
}

fn collect_result(
    log_ref: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    succeeded: bool,
    error: Option<String>,
) -> StfixerResult {
    let log = log_ref.lock().map(|l| l.clone()).unwrap_or_default();
    StfixerResult { succeeded, log, error }
}

fn enable_auto_update(steam_path: &std::path::Path) {
    let config_dir = steam_path.join("cloud_redirect");
    let config_path = config_dir.join("config.json");
    if std::fs::create_dir_all(&config_dir).is_err() { return; }
    let json = if let Ok(existing) = std::fs::read_to_string(&config_path) {
        if existing.contains("auto_update_dll") { existing }
        else {
            let trimmed = existing.trim_end().trim_end_matches('}');
            format!("{},\n  \"auto_update_dll\": true\n}}", trimmed)
        }
    } else {
        "{\n  \"auto_update_dll\": true\n}".to_string()
    };
    let _ = std::fs::write(&config_path, json);
}

/// Resolve the path(s) where CloudRedirect stores config.json.
/// Priority: %APPDATA%/CloudRedirect/config.json, then <steam>/cloud_redirect/config.json
fn find_cr_config_path() -> Option<std::path::PathBuf> {
    // 1) %APPDATA%/CloudRedirect/config.json
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = std::path::PathBuf::from(&appdata).join("CloudRedirect").join("config.json");
        if p.is_file() { return Some(p); }
    }
    // 2) <steam>/cloud_redirect/config.json
    if let Some(steam) = find_steam_path() {
        let p = steam.join("cloud_redirect").join("config.json");
        if p.is_file() { return Some(p); }
    }
    None
}

/// Read the current CloudRedirect provider config from disk.
#[command]
pub fn cloud_redirect_get_provider_config() -> CloudProviderConfig {
    let config_path = match find_cr_config_path() {
        Some(p) => p,
        None => return CloudProviderConfig {
            provider: String::new(),
            token_path: String::new(),
            authenticated: false,
            config_found: false,
        },
    };

    let json_str = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(_) => return CloudProviderConfig {
            provider: String::new(),
            token_path: String::new(),
            authenticated: false,
            config_found: false,
        },
    };

    let doc: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return CloudProviderConfig {
            provider: String::new(),
            token_path: String::new(),
            authenticated: false,
            config_found: true,
        },
    };

    let provider = doc.get("provider").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let token_path = doc.get("token_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let sync_path = doc.get("sync_path").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let display_path = if !token_path.is_empty() {
        token_path.clone()
    } else if !sync_path.is_empty() {
        sync_path.clone()
    } else {
        String::new()
    };

    // Check if the token file actually exists
    let authenticated = if !token_path.is_empty() {
        let tp = std::path::Path::new(&token_path);
        tp.is_file() && tp.metadata().map(|m| m.len() > 2).unwrap_or(false)
    } else {
        false
    };

    CloudProviderConfig {
        provider,
        token_path: display_path,
        authenticated,
        config_found: true,
    }
}

/// Save the CloudRedirect provider config to disk.
/// Writes to both %APPDATA%/CloudRedirect/config.json and <steam>/cloud_redirect/config.json.
#[command]
pub fn cloud_redirect_save_provider_config(
    provider: String,
    token_path: String,
) -> Result<(), String> {
    let mut targets: Vec<std::path::PathBuf> = Vec::new();

    // %APPDATA%/CloudRedirect/config.json
    if let Ok(appdata) = std::env::var("APPDATA") {
        targets.push(std::path::PathBuf::from(&appdata).join("CloudRedirect").join("config.json"));
    }
    // <steam>/cloud_redirect/config.json
    if let Some(steam) = find_steam_path() {
        targets.push(steam.join("cloud_redirect").join("config.json"));
    }

    if targets.is_empty() {
        return Err("Cannot determine config directory".to_string());
    }

    for config_path in &targets {
        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Read existing config and merge (preserve auto_update_dll etc.)
        let mut doc: serde_json::Value = if let Ok(existing) = std::fs::read_to_string(config_path) {
            serde_json::from_str(&existing).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = doc.as_object_mut() {
            obj.insert("provider".to_string(), serde_json::json!(provider));
            if provider == "folder" {
                obj.insert("sync_path".to_string(), serde_json::json!(token_path));
                obj.remove("token_path");
            } else {
                obj.insert("token_path".to_string(), serde_json::json!(token_path));
                obj.remove("sync_path");
            }
        }

        let json_out = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
        std::fs::write(config_path, json_out).map_err(|e| format!("{}: {}", config_path.display(), e))?;
    }

    Ok(())
}
