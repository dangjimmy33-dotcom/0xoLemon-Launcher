use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::{AppHandle, Manager};

use crate::asset_pack::AssetBlob;

#[derive(Serialize, Deserialize, Default)]
struct LruTracker {
    games: Vec<String>,
}

pub fn get_cache_dir(app: &AppHandle) -> PathBuf {
    let mut path = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            app.path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
        });
    path.push("0xoLemon");
    fs::create_dir_all(&path).ok();
    path
}

pub fn perform_ttl_cleanup(app: &AppHandle) {
    let cache_dir = get_cache_dir(app);
    let marker_file = cache_dir.join("last_run.txt");
    let now = SystemTime::now();

    if let Ok(metadata) = fs::metadata(&marker_file) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = now.duration_since(modified) {
                if duration.as_secs() > 4 * 24 * 3600 {
                    clear_cache_root_shallow(&cache_dir);
                    let _ = fs::create_dir_all(&cache_dir);
                }
            }
        }
    }
    let _ = fs::write(&marker_file, "1");
}

fn touch_lru(cache_dir: &Path, game_id: &str) {
    let tracker_file = cache_dir.join("lru.json");
    let mut tracker: LruTracker = fs::read_to_string(&tracker_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    tracker.games.retain(|id| id != game_id);
    tracker.games.push(game_id.to_string());

    if tracker.games.len() > 50 {
        let removed = tracker.games.remove(0);
        clear_game_cache_dir_shallow(&cache_dir.join(removed));
    }

    let _ = fs::write(
        &tracker_file,
        serde_json::to_string(&tracker).unwrap_or_default(),
    );
}

fn safe_cache_segment(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();

    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn clear_cache_root_shallow(cache_dir: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            clear_game_cache_dir_shallow(&path);
        } else if file_type.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), "last_run.txt" | "lru.json") {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn clear_game_cache_dir_shallow(game_dir: &Path) {
    let Ok(entries) = fs::read_dir(game_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let _ = fs::remove_dir(game_dir);
}

fn read_cached_asset(
    webp_file: &Path,
    raw_file: &Path,
    mime_file: &Path,
) -> Result<Option<AssetBlob>, String> {
    use base64::{engine::general_purpose, Engine as _};

    if webp_file.exists() {
        let bytes = fs::read(webp_file).map_err(|e| e.to_string())?;
        return Ok(Some(AssetBlob {
            mime_type: "image/webp".to_string(),
            data_base64: general_purpose::STANDARD.encode(bytes),
        }));
    }

    if raw_file.exists() {
        let bytes = fs::read(raw_file).map_err(|e| e.to_string())?;
        let mime_type = fs::read_to_string(mime_file)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        return Ok(Some(AssetBlob {
            mime_type,
            data_base64: general_purpose::STANDARD.encode(bytes),
        }));
    }

    Ok(None)
}

#[tauri::command]
pub async fn fetch_asset_cache(
    app: tauri::AppHandle,
    game_id: String,
    asset_type: String, // e.g. "hero", "grid"
    url: String,
) -> Result<AssetBlob, String> {
    let cache_dir = get_cache_dir(&app);
    let cache_game_id = safe_cache_segment(&game_id);
    let cache_asset_type = safe_cache_segment(&asset_type);
    let game_dir = cache_dir.join(&cache_game_id);
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    let webp_file = game_dir.join(format!("{}.webp", cache_asset_type));
    let raw_file = game_dir.join(format!("{}.asset", cache_asset_type));
    let mime_file = game_dir.join(format!("{}.mime", cache_asset_type));

    if let Some(blob) = read_cached_asset(&webp_file, &raw_file, &mime_file)? {
        touch_lru(&cache_dir, &cache_game_id);
        return Ok(blob);
    }

    touch_lru(&cache_dir, &cache_game_id);

    let clean_url = url.trim();
    if !(clean_url.starts_with("https://") || clean_url.starts_with("http://")) {
        return Err("asset cache only accepts http or https URLs".to_string());
    }

    let res = reqwest::get(clean_url)
        .await
        .map_err(|e| format!("Failed to download: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Failed to download: HTTP {}", res.status()));
    }
    let remote_mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read bytes: {e}"))?;

    let mut saved_bytes = bytes.to_vec();
    let mut mime_type = remote_mime;
    let mut saved_as_webp = false;

    match image::load_from_memory(&bytes) {
        Ok(img) => {
            let mut webp_bytes = std::io::Cursor::new(Vec::new());
            if img
                .write_to(&mut webp_bytes, image::ImageFormat::WebP)
                .is_ok()
            {
                saved_bytes = webp_bytes.into_inner();
                mime_type = "image/webp".to_string();
                saved_as_webp = true;
            }
        }
        Err(_) => {}
    }

    if saved_as_webp {
        fs::write(&webp_file, &saved_bytes).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(&raw_file);
        let _ = fs::remove_file(&mime_file);
    } else {
        fs::write(&raw_file, &saved_bytes).map_err(|e| e.to_string())?;
        let _ = fs::write(&mime_file, &mime_type);
        let _ = fs::remove_file(&webp_file);
    }

    use base64::{engine::general_purpose, Engine as _};
    Ok(AssetBlob {
        mime_type,
        data_base64: general_purpose::STANDARD.encode(saved_bytes),
    })
}

#[tauri::command]
pub fn clear_game_cache(app: tauri::AppHandle, game_id: String) -> Result<(), String> {
    let cache_dir = get_cache_dir(&app);
    let game_dir = cache_dir.join(safe_cache_segment(&game_id));
    clear_game_cache_dir_shallow(&game_dir);
    Ok(())
}
