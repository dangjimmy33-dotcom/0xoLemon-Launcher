use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

// ── Obfuscated credentials ────────────────────────────────────────────────────
// Token encoded with 16-byte rotating key. Key segments stored separately to
// resist naive binary string-search. Key = OxoLemonGameRuns (ASCII).
const _K_A: [u8; 8] = [0x4F, 0x78, 0x6F, 0x4C, 0x65, 0x6D, 0x6F, 0x6E];
const _K_B: [u8; 8] = [0x47, 0x61, 0x6D, 0x65, 0x52, 0x75, 0x6E, 0x73];

#[rustfmt::skip]
const _T: &[u8] = &[
    0x28, 0x11, 0x1B, 0x24, 0x10, 0x0F, 0x30, 0x1E, 0x26, 0x15, 0x32, 0x54,
    0x63, 0x37, 0x58, 0x3C, 0x1C, 0x2E, 0x29, 0x05, 0x55, 0x14, 0x07, 0x02,
    0x0D, 0x23, 0x58, 0x2A, 0x26, 0x01, 0x37, 0x37, 0x27, 0x27, 0x2C, 0x0F,
    0x2B, 0x15, 0x26, 0x26, 0x71, 0x06, 0x54, 0x35, 0x0A, 0x30, 0x22, 0x16,
    0x0C, 0x4B, 0x20, 0x0F, 0x3F, 0x5B, 0x3C, 0x5D, 0x7F, 0x30, 0x55, 0x0F,
    0x3F, 0x46, 0x57, 0x39, 0x7D, 0x1A, 0x1C, 0x1A, 0x11, 0x01, 0x56, 0x3F,
    0x16, 0x50, 0x35, 0x2A, 0x01, 0x3A, 0x2D, 0x34, 0x1A, 0x2D, 0x38, 0x1F,
    0x3D, 0x04, 0x3E, 0x36, 0x0A, 0x37, 0x18, 0x50, 0x1C,
];

fn credential() -> String {
    let mut key = [0u8; 16];
    key[..8].copy_from_slice(&_K_A);
    key[8..].copy_from_slice(&_K_B);
    _T.iter()
        .enumerate()
        .map(|(i, &b)| (b ^ key[i % 16]) as char)
        .collect()
}

// ── Public structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateInfo {
    pub version: String,
    pub notes: String,
    pub download_url: String,
    pub published_at: String,
}

// ── GitHub API ────────────────────────────────────────────────────────────────

const REPO: &str = "dangjimmy33-dotcom/0xoLemon-Launcher";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn check_update() -> Option<LauncherUpdateInfo> {
    let token = credential();
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let resp = client
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "0xoLemon-Launcher/0.1")
        .header("Accept", "application/vnd.github+json")
        .send()
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let release: serde_json::Value = resp.json().ok()?;
    let tag = release["tag_name"]
        .as_str()?
        .trim_start_matches('v')
        .to_string();

    if tag == CURRENT_VERSION || tag.is_empty() {
        return None;
    }

    // Find the Windows installer asset
    let download_url = release["assets"]
        .as_array()?
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.ends_with("-setup.exe") || n.ends_with("_x64-setup.exe"))
                .unwrap_or(false)
        })?["browser_download_url"]
        .as_str()?
        .to_string();

    let notes = release["body"].as_str().unwrap_or("").to_string();
    let published_at = release["published_at"].as_str().unwrap_or("").to_string();

    Some(LauncherUpdateInfo {
        version: tag,
        notes,
        download_url,
        published_at,
    })
}

pub fn download_and_apply(app: &AppHandle, download_url: String) -> Result<(), String> {
    let token = credential();
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&download_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "0xoLemon-Launcher/0.1")
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }

    // Save to temp file
    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join("0xolemon-launcher-update-setup.exe");

    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    let mut file =
        std::fs::File::create(&installer_path).map_err(|e| format!("Write temp: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Write bytes: {e}"))?;

    // Launch installer silently (NSIS /S flag), then exit
    launch_installer(&installer_path)?;

    // Give installer a moment to start, then exit
    std::thread::sleep(std::time::Duration::from_millis(500));
    app.exit(0);
    Ok(())
}

fn launch_installer(path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new(path)
            .arg("/S") // NSIS silent install
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Launch installer: {e}"))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new(path)
            .spawn()
            .map_err(|e| format!("Launch installer: {e}"))?;
    }
    Ok(())
}
