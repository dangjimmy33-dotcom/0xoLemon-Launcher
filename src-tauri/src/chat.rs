use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub text: String,
    pub image_base64: Option<String>,
    pub timestamp: u64,
}

fn get_chat_file_path(app: &AppHandle, game_id: &str) -> PathBuf {
    let app_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let chat_dir = app_dir.join("chats");
    if !chat_dir.exists() {
        let _ = fs::create_dir_all(&chat_dir);
    }
    chat_dir.join(format!("{}.json", game_id))
}

#[tauri::command]
pub fn load_chat_history(app: AppHandle, game_id: String) -> Result<Vec<ChatMessage>, String> {
    let file_path = get_chat_file_path(&app, &game_id);
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    if data.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_chat_message(app: AppHandle, game_id: String, message: ChatMessage) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    let mut history = if file_path.exists() {
        let data = fs::read_to_string(&file_path).unwrap_or_default();
        if data.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str::<Vec<ChatMessage>>(&data).unwrap_or_default()
        }
    } else {
        Vec::new()
    };
    
    if history.iter().any(|m| m.id == message.id) {
        return Ok(());
    }

    history.push(message);
    history.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
    fs::write(&file_path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn clear_chat_history(app: AppHandle, game_id: String) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
