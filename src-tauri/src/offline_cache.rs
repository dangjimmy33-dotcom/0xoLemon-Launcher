use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use std::fs;
use serde::Serialize;
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedAssetBlob {
    pub mime_type: String,
    pub data_base64: String,
}

fn get_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let cache_dir = app_dir.join("offline_cache");
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;
    }
    Ok(cache_dir)
}

fn sanitize_asset_id(asset_id: &str) -> String {
    asset_id.replace(|c: char| !c.is_alphanumeric() && c != '-', "_")
}

#[tauri::command]
pub async fn cache_remote_asset(
    app: AppHandle,
    url: String,
    game_id: String,
    asset_id: String,
) -> Result<bool, String> {
    let cache_dir = get_cache_dir(&app)?;
    let game_cache_dir = cache_dir.join(&game_id);
    if !game_cache_dir.exists() {
        fs::create_dir_all(&game_cache_dir).map_err(|e| format!("Failed to create game cache dir: {}", e))?;
    }

    let file_path = game_cache_dir.join(sanitize_asset_id(&asset_id));
    if file_path.exists() {
        return Ok(true);
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read bytes: {}", e))?;

    // Store mime type on the first line, then raw bytes
    let mut data = content_type.into_bytes();
    data.push(b'\n');
    data.extend_from_slice(&bytes);

    fs::write(&file_path, data).map_err(|e| format!("Failed to write cache file: {}", e))?;

    Ok(true)
}

#[tauri::command]
pub fn get_cached_asset(
    app: AppHandle,
    game_id: String,
    asset_id: String,
) -> Result<CachedAssetBlob, String> {
    let cache_dir = get_cache_dir(&app)?;
    let file_path = cache_dir.join(&game_id).join(sanitize_asset_id(&asset_id));

    if !file_path.exists() {
        return Err("Asset not found in cache".to_string());
    }

    let data = fs::read(&file_path).map_err(|e| format!("Failed to read cache file: {}", e))?;
    let split_idx = data.iter().position(|&b| b == b'\n').unwrap_or(0);
    
    let mime_type = String::from_utf8_lossy(&data[..split_idx]).to_string();
    let image_data = &data[split_idx + 1..];

    Ok(CachedAssetBlob {
        mime_type,
        data_base64: STANDARD.encode(image_data),
    })
}
