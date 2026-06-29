use crate::remote_paths;
use reqwest::header::{USER_AGENT, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use tauri::AppHandle;
use sevenz_rust::Password;

const VIETHOA_REPO_ID: &str = "JOINCANE/0XoLemon";

fn build_client_with_token() -> (reqwest::Client, Option<String>) {
    let client = reqwest::Client::new();
    let token = remote_paths::token_for_repo(VIETHOA_REPO_ID);
    (client, token)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslationInfo {
    pub file_name: String,
    pub path: String,
    pub size: u64,
}

/// Normalize a string for fuzzy matching: lowercase, keep only alphanumeric chars
fn normalize_for_fuzzy(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Try to find the actual HuggingFace folder name for a game by scanning the repo root.
/// This handles case mismatches and slight name variations.
async fn find_hf_folder_name(
    client: &reqwest::Client,
    repo_id: &str,
    preferred_dir_name: &str,
    token: Option<&str>,
) -> Option<String> {
    let auth_header = token.map(|t| format!("Bearer {t}"));

    let make_req = |url: String| {
        let mut req = client.get(url).header(USER_AGENT, "007Launcher");
        if let Some(ref auth) = auth_header {
            req = req.header(AUTHORIZATION, auth.as_str());
        }
        req
    };

    // First try exact match
    let url = format!(
        "https://huggingface.co/api/datasets/{}/tree/main/{}/Viethoagame",
        repo_id, preferred_dir_name
    );
    if let Ok(res) = make_req(url).send().await {
        if res.status().is_success() {
            return Some(preferred_dir_name.to_string());
        }
    }

    // Fallback: scan root and do fuzzy match
    let root_url = format!("https://huggingface.co/api/datasets/{}/tree/main", repo_id);
    let res = make_req(root_url).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }

    #[derive(Deserialize)]
    struct HfDir {
        #[serde(rename = "type")]
        entry_type: String,
        path: String,
    }

    let dirs: Vec<HfDir> = res.json().await.ok()?;
    let needle = normalize_for_fuzzy(preferred_dir_name);

    // Find the folder with best matching name (exact fuzzy match preferred)
    for dir in &dirs {
        if dir.entry_type == "directory" {
            let folder_name = dir.path.split('/').last().unwrap_or(&dir.path);
            if normalize_for_fuzzy(folder_name) == needle {
                // Verify it has a Viethoagame subdirectory
                let check_url = format!(
                    "https://huggingface.co/api/datasets/{}/tree/main/{}/Viethoagame",
                    repo_id, folder_name
                );
                if let Ok(res) = make_req(check_url).send().await {
                    if res.status().is_success() {
                        return Some(folder_name.to_string());
                    }
                }
            }
        }
    }
    None
}

#[tauri::command]
pub async fn get_available_translations(_app: AppHandle, game_id: String) -> Result<Vec<TranslationInfo>, String> {
    let hf_dir_name = remote_paths::hf_dir_name_for_game_id(&game_id);
    
    let (client, token) = build_client_with_token();
    let token_ref = token.as_deref();

    // Try to find the actual folder name on HuggingFace (with fuzzy fallback)
    let actual_folder = find_hf_folder_name(&client, VIETHOA_REPO_ID, &hf_dir_name, token_ref).await;
    
    let folder = match actual_folder {
        Some(f) => f,
        None => return Ok(vec![]), // No viethoa found for this game
    };

    let url = format!(
        "https://huggingface.co/api/datasets/{}/tree/main/{}/Viethoagame",
        VIETHOA_REPO_ID, folder
    );

    let mut req = client.get(&url).header(USER_AGENT, "007Launcher");
    if let Some(ref t) = token {
        req = req.header(AUTHORIZATION, format!("Bearer {t}"));
    }
    let res = req.send().await.map_err(|e| e.to_string())?;

    if res.status() == 404 {
        return Ok(vec![]); 
    }

    if !res.status().is_success() {
        return Err(format!("Failed to fetch translations: {}", res.status()));
    }

    #[derive(Deserialize)]
    struct HfApiEntry {
        #[serde(rename = "type")]
        entry_type: String,
        path: String,
        size: Option<u64>,
    }

    let entries: Vec<HfApiEntry> = res.json().await.map_err(|e| e.to_string())?;

    let translations = entries
        .into_iter()
        .filter(|e| e.entry_type == "file" && e.path.ends_with(".7z"))
        .map(|e| {
            let file_name = e.path.split('/').last().unwrap_or("").to_string();
            TranslationInfo {
                file_name,
                path: e.path,
                size: e.size.unwrap_or(0),
            }
        })
        .collect();

    Ok(translations)
}

fn get_game_install_path(app: &AppHandle, game_id: &str) -> Result<PathBuf, String> {
    crate::platform::registered_install_path(app, game_id)
        .map_err(|e| format!("Failed to get install path: {}", e))?
        .ok_or_else(|| "Game is not installed".to_string())
}

#[tauri::command]
pub async fn install_translation(app: AppHandle, game_id: String, translation_path: String) -> Result<(), String> {
    let install_path = get_game_install_path(&app, &game_id)?;
    let backup_path = install_path.join(".viethoa_backup");
    
    // Download the 7z file to a temp location
    let repo_id = "JOINCANE/0XoLemon";
    // Construct resolve URL for the file
    let download_url = format!(
        "https://huggingface.co/datasets/{}/resolve/main/{}",
        repo_id, translation_path
    );
    
    let temp_dir = std::env::temp_dir().join("007launcher_viethoa");
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let temp_file_path = temp_dir.join(translation_path.split('/').last().unwrap_or("patch.7z"));
    
    let (client, token) = build_client_with_token();
    let mut req = client.get(&download_url).header(USER_AGENT, "007Launcher");
    if let Some(ref t) = token {
        req = req.header(AUTHORIZATION, format!("Bearer {t}"));
    }
    
    let res = req.send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Failed to download patch: {}", res.status()));
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    fs::write(&temp_file_path, bytes).map_err(|e| e.to_string())?;

    // Extract the 7z file
    tauri::async_runtime::spawn_blocking(move || {
        let password = Password::from("0xoLemon.dll");
        
        // Ensure backup dir exists
        fs::create_dir_all(&backup_path).map_err(|e| e.to_string())?;
        
        let file = std::fs::File::open(&temp_file_path).map_err(|e| e.to_string())?;

        sevenz_rust::decompress_with_extract_fn_and_password(
            file,
            &install_path,
            password,
            |entry, reader, dest| {
                if !entry.is_directory() {
                    let relative_path = entry.name();
                    let dest_path = install_path.join(relative_path);
                    if dest_path.exists() {
                        // Backup the file
                        let backup_file_path = backup_path.join(relative_path);
                        if let Some(parent) = backup_file_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::copy(&dest_path, &backup_file_path);
                    }
                }
                sevenz_rust::default_entry_extract_fn(entry, reader, dest)
            },
        ).map_err(|e| e.to_string())?;
            
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())??;
    
    Ok(())
}

#[tauri::command]
pub async fn uninstall_translation(app: AppHandle, game_id: String) -> Result<(), String> {
    let install_path = get_game_install_path(&app, &game_id)?;
    let backup_path = install_path.join(".viethoa_backup");
    
    if backup_path.exists() {
        // We do not restore backup manually because there's no tracking of what was overwritten vs added.
        // Or wait, if we only backup what was overwritten, we can restore it.
        // But what about new files added by the patch? We wouldn't know which ones to delete unless we tracked them.
        // The user said: "khi gỡ cài đặt việt hóa, nó tiến hành verify lại game."
        // We will just let the verify process fix everything.
        
        // Delete the backup folder
        fs::remove_dir_all(&backup_path).map_err(|e| format!("Failed to delete backup: {}", e))?;
    }
    
    // We should trigger the verify job here
    // Actually we can just return Ok and let the frontend call `start_verify_job(gameId)` right after this.
    
    Ok(())
}
