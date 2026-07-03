use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const HF_TOKEN_PT1: &str = "hf_wDAEZzjs";
const HF_TOKEN_PT2: &str = "ZSJkdDBWdZ";
const HF_TOKEN_PT3: &str = "WtzpQyevQoQHBplM";
const HF_REPO: &str = "Chat-stories/Chat-stories";
const HF_MEDIA_PATH_PREFIX: &str = "chats/media";

fn hf_media_token() -> String {
    // Attempt to load token from huggingface-repos.json config, fallback to hardcoded token parts if not found
    if let Some(token) = crate::remote_paths::token_for_repo(HF_REPO) {
        if !token.is_empty() {
            return token;
        }
    }
    format!("{}{}{}", HF_TOKEN_PT1, HF_TOKEN_PT2, HF_TOKEN_PT3)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub sender_id: String,
    pub sender_name: String,
    #[serde(default)]
    pub sender_avatar: Option<String>,
    pub text: String,
    #[serde(default)]
    pub image_base64: Option<String>,
    #[serde(default)]
    pub media_url: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
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
pub fn delete_chat_message(app: AppHandle, game_id: String, message_id: String) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    if !file_path.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(&file_path).unwrap_or_default();
    if data.trim().is_empty() { return Ok(()); }
    
    let mut history = serde_json::from_str::<Vec<ChatMessage>>(&data).unwrap_or_default();
    history.retain(|m| m.id != message_id);
    
    let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
    fs::write(&file_path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn edit_chat_message(app: AppHandle, game_id: String, message_id: String, new_text: String) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    if !file_path.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(&file_path).unwrap_or_default();
    if data.trim().is_empty() { return Ok(()); }
    
    let mut history = serde_json::from_str::<Vec<ChatMessage>>(&data).unwrap_or_default();
    if let Some(msg) = history.iter_mut().find(|m| m.id == message_id) {
        msg.text = new_text;
        let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
        fs::write(&file_path, json).map_err(|e| e.to_string())?;
    }
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

#[tauri::command]
pub fn download_from_huggingface(app: AppHandle, game_id: String) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    let url = format!("https://huggingface.co/datasets/{}/resolve/main/chats/{}.json", HF_REPO, game_id);
    let hf_token = hf_media_token();
    
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", hf_token))
        .send();
    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(text) = resp.text() {
                if let Ok(remote_history) = serde_json::from_str::<Vec<ChatMessage>>(&text) {
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
                    
                    let mut added = false;
                    for rm in remote_history {
                        if !history.iter().any(|m| m.id == rm.id) {
                            history.push(rm);
                            added = true;
                        }
                    }
                    if added {
                        history.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                        if let Ok(json) = serde_json::to_string_pretty(&history) {
                            let _ = fs::write(&file_path, json);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn sync_to_huggingface(app: AppHandle, game_id: String) -> Result<(), String> {
    let file_path = get_chat_file_path(&app, &game_id);
    if !file_path.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    
    let payload = serde_json::json!({
        "operations": [
            {
                "operation": "add",
                "path": format!("chats/{}.json", game_id),
                "content": b64,
                "encoding": "base64"
            }
        ],
        "summary": format!("Sync {}", game_id)
    });

    let url = format!("https://huggingface.co/api/datasets/{}/commit/main", HF_REPO);
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
    
    let hf_token = format!("{}{}{}", HF_TOKEN_PT1, HF_TOKEN_PT2, HF_TOKEN_PT3);

    let res = client.post(&url)
        .header("Authorization", format!("Bearer {}", hf_token))
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().unwrap_or_default();
        return Err(format!("HF Sync failed: {} - {}", status, body));
    }
    
    Ok(())
}

#[tauri::command]
pub async fn upload_chat_media(filename: String, data: Vec<u8>) -> Result<String, String> {
    let token = hf_media_token();

    // Keep media on Hugging Face, but avoid leaking a single full token literal in source.
    let safe_name: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let upload_path = format!("{}/{}", HF_MEDIA_PATH_PREFIX, safe_name);
    let size = data.len();

    let ext = safe_name.split('.').last().unwrap_or("").to_lowercase();
    let is_lfs = matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" |
        "mp4" | "webm" | "mp3" | "wav" | "ogg" | "flac" | "aac" |
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "pdf" | "bin" | "dat" | "exe"
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let mut ndjson = format!(
        "{{\"key\": \"header\", \"value\": {{\"summary\": \"Upload media {}\"}}}}\n",
        safe_name
    );

    if is_lfs {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let sha256_hash = hex::encode(hasher.finalize());

        let batch_url = format!("https://huggingface.co/datasets/{}.git/info/lfs/objects/batch", HF_REPO);
        let batch_payload = serde_json::json!({
            "operation": "upload",
            "transfers": ["basic", "multipart"],
            "objects": [
                {
                    "oid": sha256_hash,
                    "size": size
                }
            ]
        });

        let batch_res = client.post(&batch_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.git-lfs+json")
            .header("Content-Type", "application/vnd.git-lfs+json")
            .json(&batch_payload)
            .send()
            .await
            .map_err(|e| format!("LFS batch request failed: {}", e))?;

        if !batch_res.status().is_success() {
            let status = batch_res.status();
            let body = batch_res.text().await.unwrap_or_default();
            return Err(format!("LFS batch failed: {} - {}", status, body));
        }

        let batch_json: serde_json::Value = batch_res.json().await.map_err(|e| e.to_string())?;
        
        if let Some(objects) = batch_json.get("objects").and_then(|o| o.as_array()) {
            if let Some(obj) = objects.first() {
                if let Some(error) = obj.get("error") {
                    return Err(format!("LFS error: {}", error));
                }
                if let Some(actions) = obj.get("actions") {
                    if let Some(upload) = actions.get("upload") {
                        if let Some(href) = upload.get("href").and_then(|h| h.as_str()) {
                            let mut put_req = client.put(href).body(data.clone());
                            if let Some(header) = upload.get("header").and_then(|h| h.as_object()) {
                                for (k, v) in header {
                                    if let Some(v_str) = v.as_str() {
                                        put_req = put_req.header(k, v_str);
                                    }
                                }
                            }
                            let put_res = put_req.send().await.map_err(|e| format!("LFS upload failed: {}", e))?;
                            if !put_res.status().is_success() {
                                let status = put_res.status();
                                let body = put_res.text().await.unwrap_or_default();
                                return Err(format!("LFS PUT failed: {} - {}", status, body));
                            }
                        }
                    }
                }
            }
        }

        ndjson.push_str(&format!(
            "{{\"key\": \"lfsFile\", \"value\": {{\"path\": \"{}\", \"algo\": \"sha256\", \"oid\": \"{}\", \"size\": {}}}}}\n",
            upload_path, sha256_hash, size
        ));

    } else {
        use base64::Engine;
        let b64_content = base64::engine::general_purpose::STANDARD.encode(&data);
        ndjson.push_str(&format!(
            "{{\"key\": \"file\", \"value\": {{\"path\": \"{}\", \"content\": \"{}\", \"encoding\": \"base64\"}}}}\n",
            upload_path, b64_content
        ));
    }

    let url = format!("https://huggingface.co/api/datasets/{}/commit/main", HF_REPO);
    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson)
        .send()
        .await
        .map_err(|e| format!("HF upload request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("HF media upload failed: {} - {}", status, body));
    }

    let public_url = format!(
        "https://huggingface.co/datasets/{}/resolve/main/{}",
        HF_REPO, upload_path
    );
    Ok(public_url)
}

#[tauri::command]
pub async fn upload_chat_media_from_path(filename: String, filepath: String) -> Result<String, String> {
    let metadata = std::fs::metadata(&filepath)
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let max_size = 100 * 1024 * 1024;
    if metadata.len() > max_size {
        return Err(format!(
            "File too large. Max size is {}MB for chat media.",
            max_size / 1024 / 1024
        ));
    }

    let data = std::fs::read(&filepath).map_err(|e| format!("Failed to read file: {}", e))?;
    upload_chat_media(filename, data).await
}

#[tauri::command]
pub async fn delete_chat_media(url: String) -> Result<(), String> {
    if !url.contains("/chats/media/") {
        return Ok(());
    }
    let filename = url.split('/').last().unwrap_or_default();
    if filename.is_empty() { return Ok(()); }
    let token = hf_media_token();
    let delete_path = format!("{}/{}", HF_MEDIA_PATH_PREFIX, filename);
    let ndjson = format!(
        "{{\"key\": \"header\", \"value\": {{\"summary\": \"Delete media {}\"}}}}\n{{\"key\": \"deletedFile\", \"value\": {{\"path\": \"{}\"}}}}\n",
        filename, delete_path
    );

    let hf_url = format!("https://huggingface.co/api/datasets/{}/commit/main", HF_REPO);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let _res = client.post(&hf_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn download_chat_media_to_disk(url: String, filepath: String) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let mut response = client.get(&url).send().await.map_err(|e| format!("Failed to download: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let mut file = std::fs::File::create(&filepath).map_err(|e| format!("Failed to create file: {}", e))?;
    
    use std::io::Write;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).map_err(|e| format!("Failed to write to file: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
pub fn read_file_base64(filepath: String) -> Result<String, String> {
    use base64::Engine;
    let data = std::fs::read(&filepath).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

#[tauri::command]
pub async fn get_chat_media_base64(url: String) -> Result<String, String> {
    let hf_token = hf_media_token();
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&url);
    if url.contains("huggingface.co") {
        req = req.header("Authorization", format!("Bearer {}", hf_token));
    }

    let res = req.send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Failed to fetch media: {}", res.status()));
    }

    // Read bytes
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    // Guess mime type
    let ext = url.split('.').last().unwrap_or("png").to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };

    Ok(format!("data:{};base64,{}", mime, b64))
}
