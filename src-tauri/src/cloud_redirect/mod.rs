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

use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use base64::engine::general_purpose::{URL_SAFE_NO_PAD};
use base64::Engine;
use std::net::TcpListener;
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use std::thread;
use url::Url;

const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);

fn random_urlsafe(length: usize) -> String {
    let mut bytes = vec![0_u8; length];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[command]
pub fn cloud_redirect_connect_google() -> Result<(), String> {
    let client_id = "745435850820-k7v8oqp0g640l8eed7p7nu6f7fd8njoh.apps.googleusercontent.com";
    
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    listener.set_nonblocking(true).map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    
    let verifier = random_urlsafe(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(32);
    
    let mut auth_url = Url::parse(AUTH_ENDPOINT).map_err(|error| error.to_string())?;
    auth_url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", DRIVE_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
        
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", auth_url.as_str()])
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    
    let deadline = Instant::now() + OAUTH_TIMEOUT;
    let mut authorization_code = None;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = [0_u8; 8192];
                let read = stream.read(&mut request).map_err(|error| error.to_string())?;
                let request = String::from_utf8_lossy(&request[..read]);
                
                let target = request.lines().next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| "Google OAuth callback request was invalid".to_string())?;
                let callback = Url::parse(&format!("http://127.0.0.1:{port}{target}")).map_err(|error| error.to_string())?;
                
                let params = callback.query_pairs().into_owned().collect::<Vec<_>>();
                let returned_state = params.iter().find(|(key, _)| key == "state").map(|(_, value)| value.as_str());
                let error = params.iter().find(|(key, _)| key == "error").map(|(_, value)| value.clone());
                let code = params.iter().find(|(key, _)| key == "code").map(|(_, value)| value.clone());
                
                let success = returned_state == Some(state.as_str()) && error.is_none() && code.is_some();
                let body = if success {
                    "<html><body><h2>0xoLemon connected to Google Drive for STFixer.</h2><p>You can close this tab and return to the launcher.</p></body></html>"
                } else {
                    "<html><body><h2>Google Drive authorization failed.</h2><p>Return to the launcher for details.</p></body></html>"
                };
                
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                stream.write_all(response.as_bytes()).map_err(|error| error.to_string())?;
                
                if returned_state != Some(state.as_str()) { return Err("Google OAuth state validation failed".to_string()); }
                if let Some(error) = error { return Err(format!("Google authorization was denied: {error}")); }
                authorization_code = code;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    
    let code = authorization_code.ok_or_else(|| "Google Drive sign-in timed out".to_string())?;
    
    let client = reqwest::blocking::Client::new();
    let token_resp = client.post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .map_err(|error| error.to_string())?;
        
    let raw_json = token_resp.text().map_err(|error| error.to_string())?;
    
    let doc: serde_json::Value = serde_json::from_str(&raw_json).map_err(|error| error.to_string())?;
    if !doc.get("access_token").is_some() {
        return Err(format!("Google did not return a valid access token: {}", raw_json));
    }
    
    let encrypted = crate::secret_store::protect(raw_json.as_bytes())?;
    
    let appdata = std::env::var("APPDATA").map_err(|_| "Could not find APPDATA".to_string())?;
    let cr_dir = std::path::PathBuf::from(&appdata).join("CloudRedirect");
    std::fs::create_dir_all(&cr_dir).map_err(|error| error.to_string())?;
    
    let token_path = cr_dir.join("google_tokens.json");
    std::fs::write(&token_path, encrypted).map_err(|error| error.to_string())?;
    
    cloud_redirect_save_provider_config("gdrive".to_string(), token_path.to_string_lossy().to_string())?;
    
    Ok(())
}
