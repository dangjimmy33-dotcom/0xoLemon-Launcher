use std::fs;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{ImageReader, Limits};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_WIDTH: u32 = 2_560;
const MAX_HEIGHT: u32 = 1_440;
const MAX_ENCODED_BYTES: usize = 6 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeWallpaperAsset {
    pub asset_id: String,
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn pick_home_wallpaper(app: AppHandle) -> Result<Option<HomeWallpaperAsset>, String> {
    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Choose a Home wallpaper")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let source = selected
        .into_path()
        .map_err(|error| format!("HOME_WALLPAPER_PATH_INVALID: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || import_wallpaper(&app, &source))
        .await
        .map_err(|error| format!("HOME_WALLPAPER_TASK_FAILED: {error}"))?
        .map(Some)
}

#[tauri::command]
pub fn get_home_wallpaper_asset(
    app: AppHandle,
    asset_id: String,
) -> Result<HomeWallpaperAsset, String> {
    validate_asset_id(&asset_id)?;
    let path = wallpaper_root(&app)?.join(format!("{asset_id}.webp"));
    let metadata =
        fs::metadata(&path).map_err(|error| format!("HOME_WALLPAPER_ASSET_NOT_FOUND: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ENCODED_BYTES as u64 {
        return Err("HOME_WALLPAPER_ASSET_INVALID".to_string());
    }
    let (width, height) = ImageReader::open(&path)
        .map_err(|error| format!("HOME_WALLPAPER_ASSET_INVALID: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("HOME_WALLPAPER_ASSET_INVALID: {error}"))?
        .into_dimensions()
        .map_err(|error| format!("HOME_WALLPAPER_ASSET_INVALID: {error}"))?;
    Ok(HomeWallpaperAsset {
        asset_id,
        file_path: path.to_string_lossy().into_owned(),
        width,
        height,
        size_bytes: metadata.len(),
    })
}

fn import_wallpaper(app: &AppHandle, source: &Path) -> Result<HomeWallpaperAsset, String> {
    let source = validate_source(source)?;
    let metadata =
        fs::metadata(&source).map_err(|error| format!("HOME_WALLPAPER_READ_FAILED: {error}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
        return Err("HOME_WALLPAPER_SOURCE_SIZE_INVALID".to_string());
    }

    let mut reader = ImageReader::open(&source)
        .map_err(|error| format!("HOME_WALLPAPER_READ_FAILED: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("HOME_WALLPAPER_FORMAT_INVALID: {error}"))?;
    let format = reader
        .format()
        .ok_or_else(|| "HOME_WALLPAPER_FORMAT_INVALID".to_string())?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return Err("HOME_WALLPAPER_FORMAT_REJECTED".to_string());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(384 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| format!("HOME_WALLPAPER_DECODE_FAILED: {error}"))?;
    let resized = decoded.resize(MAX_WIDTH, MAX_HEIGHT, FilterType::Lanczos3);
    let width = resized.width();
    let height = resized.height();
    if width < 640 || height < 360 {
        return Err("HOME_WALLPAPER_DIMENSIONS_TOO_SMALL".to_string());
    }
    let rgba = resized.to_rgba8();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), width, height)
        .encode(82.0)
        .to_vec();
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_BYTES {
        return Err("HOME_WALLPAPER_ENCODE_SIZE_INVALID".to_string());
    }
    let asset_id = hex::encode(Sha256::digest(&encoded));
    let root = wallpaper_root(app)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("HOME_WALLPAPER_DIRECTORY_FAILED: {error}"))?;
    let path = root.join(format!("{asset_id}.webp"));
    if !path.exists() {
        let stage = root.join(format!(".{asset_id}.{}.tmp", Uuid::new_v4()));
        fs::write(&stage, &encoded)
            .map_err(|error| format!("HOME_WALLPAPER_STAGE_FAILED: {error}"))?;
        if let Err(error) = fs::rename(&stage, &path) {
            let _ = fs::remove_file(&stage);
            return Err(format!("HOME_WALLPAPER_COMMIT_FAILED: {error}"));
        }
    }
    Ok(HomeWallpaperAsset {
        asset_id,
        file_path: path.to_string_lossy().into_owned(),
        width,
        height,
        size_bytes: encoded.len() as u64,
    })
}

fn wallpaper_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|path| path.join("home-wallpapers"))
        .map_err(|error| format!("HOME_WALLPAPER_DIRECTORY_UNAVAILABLE: {error}"))
}

fn validate_asset_id(asset_id: &str) -> Result<(), String> {
    if asset_id.len() == 64 && asset_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("HOME_WALLPAPER_ASSET_ID_INVALID".to_string())
    }
}

fn validate_source(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("HOME_WALLPAPER_PATH_MUST_BE_ABSOLUTE".to_string());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("HOME_WALLPAPER_SOURCE_INVALID: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("HOME_WALLPAPER_SOURCE_NOT_REGULAR".to_string());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x0000_0400 != 0 {
            return Err("HOME_WALLPAPER_SOURCE_REPARSE_REJECTED".to_string());
        }
    }
    path.canonicalize()
        .map_err(|error| format!("HOME_WALLPAPER_SOURCE_INVALID: {error}"))
}

#[cfg(test)]
mod tests {
    use super::validate_asset_id;

    #[test]
    fn asset_id_must_be_a_full_sha256_hex_digest() {
        assert!(validate_asset_id(&"a5".repeat(32)).is_ok());
        assert!(validate_asset_id(&"z5".repeat(32)).is_err());
        assert!(validate_asset_id(&"a5".repeat(31)).is_err());
    }
}
