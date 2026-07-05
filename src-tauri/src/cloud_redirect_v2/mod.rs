// CloudRedirect V2 Integration Module
// Updated CloudRedirect with error handling, lua sync, achievements, etc.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::command;

mod dll_manager;
mod provider_config;
mod oauth;

pub use dll_manager::*;
pub use provider_config::*;
pub use oauth::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRedirectStatus {
    pub enabled: bool,
    pub provider: Option<String>,
    pub authenticated: bool,
    pub sync_active: bool,
    pub last_sync: Option<String>,
    pub error: Option<String>,
    pub auto_cloud_games: Vec<String>, // List of AppIDs with AutoCloud enabled
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSaveInfo {
    pub app_id: String,
    pub game_name: String,
    pub has_auto_cloud: bool,
    pub save_path: Option<String>,
    pub save_size: u64,
    pub last_modified: Option<String>,
}

/// Get CloudRedirect DLL path in resources
fn get_dll_resource_path() -> PathBuf {
    // This will be resolved by Tauri at runtime
    PathBuf::from("resources/cloud_redirect/0xoCloudRedirect.dll")
}

/// Get Steam installation path
fn get_steam_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(steam_key) = hkcu.open_subkey("Software\\Valve\\Steam") {
            if let Ok(path) = steam_key.get_value::<String, _>("SteamPath") {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

/// Check if CloudRedirect V2 is currently enabled
#[command]
pub fn cloud_redirect_v2_get_status() -> Result<CloudRedirectStatus, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let dll_path = steam_path.join("0xoCloudRedirect.dll");
    let marker_path = steam_path.join(".0xo-cloud-redirect-enabled");

    let enabled = dll_path.exists() && marker_path.exists();

    // Load provider config
    let config = provider_config::load_config().unwrap_or_default();

    // Detect AutoCloud games
    let auto_cloud_games = detect_auto_cloud_games().unwrap_or_default();

    Ok(CloudRedirectStatus {
        enabled,
        provider: config.provider,
        authenticated: config.authenticated,
        sync_active: false, // TODO: Check actual sync status
        last_sync: config.last_sync,
        error: None,
        auto_cloud_games,
    })
}

/// Detect games with AutoCloud enabled by scanning Steam userdata
fn detect_auto_cloud_games() -> Result<Vec<String>, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let mut auto_cloud_games = Vec::new();

    // Get userdata directory
    let userdata_path = steam_path.join("userdata");
    if !userdata_path.exists() {
        return Ok(auto_cloud_games);
    }

    // Scan each user's folders
    if let Ok(entries) = std::fs::read_dir(&userdata_path) {
        for entry in entries.flatten() {
            let user_path = entry.path();
            if !user_path.is_dir() {
                continue;
            }

            // Scan app folders
            if let Ok(app_entries) = std::fs::read_dir(&user_path) {
                for app_entry in app_entries.flatten() {
                    let app_path = app_entry.path();
                    if !app_path.is_dir() {
                        continue;
                    }

                    // Check for remotecache.vdf (indicates AutoCloud)
                    let remotecache = app_path.join("remotecache.vdf");
                    if remotecache.exists() {
                        if let Some(app_id) = app_path.file_name() {
                            auto_cloud_games.push(app_id.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(auto_cloud_games)
}

/// Enable CloudRedirect by copying DLL to Steam directory
#[command]
pub async fn cloud_redirect_enable() -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    
    // Get DLL from resources
    let dll_resource = get_dll_resource_path();
    
    // Copy DLL to Steam directory
    dll_manager::install_dll(&dll_resource, &steam_path)?;

    Ok(())
}

/// Disable CloudRedirect by removing DLL from Steam directory
#[command]
pub async fn cloud_redirect_disable() -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    
    dll_manager::uninstall_dll(&steam_path)?;

    Ok(())
}

/// Set local folder path for local provider
#[command]
pub async fn cloud_redirect_set_local_path(path: String) -> Result<(), String> {
    let mut config = provider_config::load_config().unwrap_or_default();
    config.provider = Some("local".to_string());
    config.local_path = Some(path);
    config.authenticated = true;
    provider_config::save_config(&config)?;
    Ok(())
}

/// Start OAuth flow for cloud provider
#[command]
pub async fn cloud_redirect_start_oauth(provider: String) -> Result<String, String> {
    oauth::start_oauth_flow(&provider).await
}

/// Complete OAuth flow with authorization code
#[command]
pub async fn cloud_redirect_complete_oauth(provider: String, code: String) -> Result<(), String> {
    oauth::complete_oauth_flow(&provider, &code).await?;
    
    // Update config
    let mut config = provider_config::load_config().unwrap_or_default();
    config.provider = Some(provider);
    config.authenticated = true;
    provider_config::save_config(&config)?;
    
    Ok(())
}

/// Trigger manual sync
#[command]
pub async fn cloud_redirect_trigger_sync() -> Result<(), String> {
    // TODO: Implement sync trigger via IPC to CloudRedirect DLL
    Ok(())
}

/// Get real-time sync status from CloudRedirect
#[command]
pub async fn cloud_redirect_get_sync_status() -> Result<SyncStatus, String> {
    // TODO: Query actual DLL status via IPC or shared memory
    // For now, return mock data
    
    let _config = provider_config::load_config().unwrap_or_default();
    
    Ok(SyncStatus {
        is_syncing: false,
        current_file: None,
        progress: 0.0,
        files_uploaded: 0,
        files_downloaded: 0,
        bytes_transferred: 0,
        error: None,
    })
}

/// Poll for OAuth authorization code
#[command]
pub fn cloud_redirect_poll_oauth_code() -> Option<String> {
    oauth::get_oauth_code()
}

/// Get list of games with saves (for backup)
#[command]
pub fn cloud_redirect_list_game_saves() -> Result<Vec<GameSaveInfo>, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let userdata_path = steam_path.join("userdata");
    let mut games = Vec::new();

    if !userdata_path.exists() {
        return Ok(games);
    }

    // Scan each user's folders
    if let Ok(entries) = std::fs::read_dir(&userdata_path) {
        for entry in entries.flatten() {
            let user_path = entry.path();
            if !user_path.is_dir() {
                continue;
            }

            // Scan app folders
            if let Ok(app_entries) = std::fs::read_dir(&user_path) {
                for app_entry in app_entries.flatten() {
                    let app_path = app_entry.path();
                    if !app_path.is_dir() {
                        continue;
                    }

                    let app_id = app_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    // Skip system folders
                    if app_id == "config" || app_id == "7" {
                        continue;
                    }

                    // Check for remotecache folder
                    let remotecache_path = app_path.join("remote");
                    let has_auto_cloud = app_path.join("remotecache.vdf").exists();

                    let save_path = if remotecache_path.exists() {
                        Some(remotecache_path.to_string_lossy().to_string())
                    } else {
                        None
                    };

                    // Calculate save size
                    let save_size = calculate_folder_size(&app_path).unwrap_or(0);

                    // Get last modified time
                    let last_modified = std::fs::metadata(&app_path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                        .flatten()
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());

                    if save_size > 0 {
                        games.push(GameSaveInfo {
                            app_id: app_id.clone(),
                            game_name: format!("Game {}", app_id), // TODO: Get real name from Steam API
                            has_auto_cloud,
                            save_path,
                            save_size,
                            last_modified,
                        });
                    }
                }
            }
        }
    }

    Ok(games)
}

/// Backup game save to cloud provider
#[command]
pub async fn cloud_redirect_backup_save(app_id: String, upload_to_cloud: bool) -> Result<String, String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let config = provider_config::load_config()?;

    // Find save path
    let userdata_path = steam_path.join("userdata");
    let mut save_path: Option<PathBuf> = None;

    if let Ok(entries) = std::fs::read_dir(&userdata_path) {
        for entry in entries.flatten() {
            let user_path = entry.path();
            let app_path = user_path.join(&app_id);
            if app_path.exists() && app_path.is_dir() {
                save_path = Some(app_path);
                break;
            }
        }
    }

    let save_path = save_path.ok_or(format!("Save not found for app {}", app_id))?;

    // Create backup zip
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_filename = format!("{}_backup_{}.zip", app_id, timestamp);
    let backup_zip = std::env::temp_dir().join(&backup_filename);
    
    create_backup_zip(&save_path, &backup_zip)?;

    // Save local copy first
    let backup_dir = dirs::data_local_dir()
        .ok_or("Failed to get data dir")?
        .join("0xoLemon")
        .join("cloud_redirect_backups");
    
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    
    let local_backup = backup_dir.join(&backup_filename);
    std::fs::copy(&backup_zip, &local_backup).map_err(|e| e.to_string())?;

    let mut result_message = format!("Local backup: {}", local_backup.display());

    // Upload to cloud if requested and authenticated
    if upload_to_cloud && config.authenticated {
        match config.provider.as_deref() {
            Some("google_drive") => {
                match oauth::upload_to_google_drive(
                    &backup_zip,
                    &backup_filename,
                    "0xoLemon_Backups"
                ).await {
                    Ok(file_id) => {
                        result_message.push_str(&format!("\nGoogle Drive backup: {}", file_id));
                    }
                    Err(e) => {
                        result_message.push_str(&format!("\nCloud upload failed: {}", e));
                    }
                }
            }
            Some("onedrive") => {
                result_message.push_str("\nOneDrive upload not yet implemented");
            }
            _ => {}
        }
    }

    // Cleanup temp file
    std::fs::remove_file(&backup_zip).ok();

    Ok(result_message)
}

/// Reset game progress (delete saves)
#[command]
pub async fn cloud_redirect_reset_game(app_id: String) -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    let userdata_path = steam_path.join("userdata");

    let mut deleted = false;

    if let Ok(entries) = std::fs::read_dir(&userdata_path) {
        for entry in entries.flatten() {
            let user_path = entry.path();
            let app_path = user_path.join(&app_id);
            
            if app_path.exists() && app_path.is_dir() {
                // Create backup first
                let backup_name = format!("reset_backup_{}_{}.zip",
                    app_id,
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                );
                let backup_path = std::env::temp_dir().join(&backup_name);
                
                create_backup_zip(&app_path, &backup_path)?;
                
                // Delete save folder
                std::fs::remove_dir_all(&app_path).map_err(|e| e.to_string())?;
                
                deleted = true;
            }
        }
    }

    if !deleted {
        return Err(format!("No save found for app {}", app_id));
    }

    Ok(())
}

/// Helper: Calculate folder size recursively
fn calculate_folder_size(path: &Path) -> Result<u64, String> {
    let mut total_size = 0u64;

    if path.is_file() {
        return Ok(std::fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0));
    }

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total_size += calculate_folder_size(&path)?;
            } else {
                total_size += std::fs::metadata(&path)
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
        }
    }

    Ok(total_size)
}

/// Helper: Create backup zip
fn create_backup_zip(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);

    let options: zip::write::FileOptions<()> = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let walkdir = walkdir::WalkDir::new(source);

    for entry in walkdir.into_iter().flatten() {
        let path = entry.path();
        let name = path.strip_prefix(source)
            .map_err(|e| e.to_string())?;

        if path.is_file() {
            zip.start_file(name.to_string_lossy().to_string(), options)
                .map_err(|e| e.to_string())?;
            
            let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(name.to_string_lossy().to_string(), options)
                .map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// List available backups (local + cloud)
#[command]
pub async fn cloud_redirect_list_backups(app_id: Option<String>) -> Result<Vec<BackupInfo>, String> {
    let mut backups = Vec::new();
    
    // List local backups
    let backup_dir = dirs::data_local_dir()
        .ok_or("Failed to get data dir")?
        .join("0xoLemon")
        .join("cloud_redirect_backups");
    
    if backup_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("zip") {
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    
                    // Filter by app_id if specified
                    if let Some(ref filter_id) = app_id {
                        if !filename.starts_with(filter_id) {
                            continue;
                        }
                    }
                    
                    let metadata = std::fs::metadata(&path).ok();
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified = metadata
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                        .flatten()
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());
                    
                    backups.push(BackupInfo {
                        id: filename.clone(),
                        name: filename,
                        app_id: app_id.clone(),
                        location: "local".to_string(),
                        size,
                        created_at: modified,
                        cloud_id: None,
                    });
                }
            }
        }
    }
    
    // List cloud backups if authenticated
    let config = provider_config::load_config().unwrap_or_default();
    if config.authenticated {
        match config.provider.as_deref() {
            Some("google_drive") => {
                if let Ok(cloud_files) = oauth::list_google_drive_backups("0xoLemon_Backups").await {
                    for file in cloud_files {
                        // Filter by app_id if specified
                        if let Some(ref filter_id) = app_id {
                            if !file.name.starts_with(filter_id) {
                                continue;
                            }
                        }
                        
                        backups.push(BackupInfo {
                            id: file.id.clone(),
                            name: file.name,
                            app_id: app_id.clone(),
                            location: "google_drive".to_string(),
                            size: file.size,
                            created_at: file.modified_time,
                            cloud_id: Some(file.id),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    
    // Sort by created_at descending
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    
    Ok(backups)
}

/// Restore backup
#[command]
pub async fn cloud_redirect_restore_backup(backup_id: String, location: String) -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("Steam not found")?;
    
    // Extract app_id from backup filename (format: {appid}_backup_{timestamp}.zip)
    let app_id = backup_id.split('_').next().ok_or("Invalid backup ID")?;
    
    // Get backup file
    let backup_file = if location == "local" {
        let backup_dir = dirs::data_local_dir()
            .ok_or("Failed to get data dir")?
            .join("0xoLemon")
            .join("cloud_redirect_backups");
        backup_dir.join(&backup_id)
    } else if location == "google_drive" {
        // Download from Google Drive
        let temp_path = std::env::temp_dir().join(&backup_id);
        oauth::download_from_google_drive(&backup_id, &temp_path).await?;
        temp_path
    } else {
        return Err("Unsupported backup location".to_string());
    };
    
    if !backup_file.exists() {
        return Err("Backup file not found".to_string());
    }
    
    // Find save directory
    let userdata_path = steam_path.join("userdata");
    let mut save_path: Option<PathBuf> = None;
    
    if let Ok(entries) = std::fs::read_dir(&userdata_path) {
        for entry in entries.flatten() {
            let user_path = entry.path();
            let app_path = user_path.join(app_id);
            
            // Create directory if it doesn't exist
            if !app_path.exists() {
                std::fs::create_dir_all(&app_path).map_err(|e| e.to_string())?;
            }
            
            save_path = Some(app_path);
            break;
        }
    }
    
    let save_path = save_path.ok_or("Could not determine save path")?;
    
    // Extract backup
    extract_backup_zip(&backup_file, &save_path)?;
    
    // Cleanup downloaded file if from cloud
    if location != "local" {
        std::fs::remove_file(&backup_file).ok();
    }
    
    Ok(())
}

/// Helper: Extract backup zip
fn extract_backup_zip(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = dest.join(file.name());
        
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub name: String,
    pub app_id: Option<String>,
    pub location: String, // "local" or "google_drive"
    pub size: u64,
    pub created_at: Option<String>,
    pub cloud_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_file: Option<String>,
    pub progress: f64,
    pub files_uploaded: u32,
    pub files_downloaded: u32,
    pub bytes_transferred: u64,
    pub error: Option<String>,
}
