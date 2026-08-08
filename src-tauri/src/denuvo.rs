use std::fs;
use std::path::PathBuf;
use reqwest::Client;
use serde_json::json;
use serde::{Deserialize, Serialize};
use tauri::command;
use regex::Regex;
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Serialize)]
pub struct DenuvoTokenResponse {
    pub success: bool,
    pub token: Option<String>,
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct ServerResponse {
    token: String,
}

#[command]
pub async fn get_denuvo_token_from_server(ticket_content: String, server_url: String) -> Result<DenuvoTokenResponse, String> {
    let client = Client::new();
    let url = if server_url.ends_with('/') {
        format!("{}api/denuvo/generate", server_url)
    } else {
        format!("{}/api/denuvo/generate", server_url)
    };

    let res = client
        .post(&url)
        .json(&json!({ "ticket": ticket_content }))
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Denuvo Token Server: {}", e))?;

    if res.status().is_success() {
        let body: ServerResponse = res.json().await.map_err(|e| format!("Invalid JSON response: {}", e))?;
        Ok(DenuvoTokenResponse {
            success: true,
            token: Some(body.token),
            message: None,
        })
    } else {
        let err_text = res.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Ok(DenuvoTokenResponse {
            success: false,
            token: None,
            message: Some(err_text),
        })
    }
}

#[command]
pub async fn apply_denuvo_token_to_cfg(cfg_path: String, token: String) -> Result<(), String> {
    let path = PathBuf::from(cfg_path);
    if !path.exists() {
        return Err("anadius.cfg not found".to_string());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;
    
    // Replace placeholder or existing token with the new token
    let re = Regex::new(r#"(?i)("DenuvoToken"\s+)"[^"]+""#).map_err(|e| format!("Regex Error: {}", e))?;
    let new_content = re.replace(&content, format!(r#"${{1}}"{token}""#)).to_string();
    
    // If it wasn't there, maybe it didn't exist or used a different format, but assuming it exists:
    if !new_content.contains(&token) {
        // Fallback for strict replacement if regex somehow missed it
        let fallback_content = content.replace("PASTE_A_VALID_DENUVO_TOKEN_HERE", &token)
                                      .replace("Token_Paste_Vao_Day", &token);
        fs::write(&path, fallback_content).map_err(|e| format!("Failed to write config: {}", e))?;
    } else {
        fs::write(&path, new_content).map_err(|e| format!("Failed to write config: {}", e))?;
    }
    
    Ok(())
}

#[command]
pub async fn scan_for_denuvo_ticket(game_dir: String) -> Result<String, String> {
    let path = PathBuf::from(&game_dir);
    if !path.exists() {
        return Err("Game directory does not exist".to_string());
    }

    let mut latest_ticket: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in fs::read_dir(path).map_err(|e| format!("Failed to read game directory: {}", e))? {
        let entry = entry.map_err(|e| format!("IO Error: {}", e))?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("Denuvo_ticket_") && name.ends_with(".txt") {
                    if let Ok(metadata) = path.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Some((_, latest_time)) = latest_ticket {
                                if modified > latest_time {
                                    latest_ticket = Some((path, modified));
                                }
                            } else {
                                latest_ticket = Some((path, modified));
                            }
                        }
                    }
                }
            }
        }
    }
    
    if let Some((ticket_path, _)) = latest_ticket {
        let content = fs::read_to_string(&ticket_path).unwrap_or_default();
        // Extract the actual ticket string (usually on the 5th line or the longest word)
        for word in content.split_whitespace() {
            if word.len() > 500 && word.contains('|') {
                return Ok(word.to_string());
            }
        }
        return Err("Ticket file found but invalid format.".to_string());
    }
    
    Err("No Denuvo ticket found.".to_string())
}

#[command]
pub async fn delete_denuvo_tickets(game_dir: String) -> Result<(), String> {
    let path = PathBuf::from(&game_dir);
    if path.exists() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("Denuvo_ticket_") && name.ends_with(".txt") {
                            let _ = fs::remove_file(p);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
}

pub async fn download_and_extract_magic_file_inner<F, G>(game_dir: String, on_progress: F, on_extract: G) -> Result<(), String> 
where 
    F: Fn(DownloadProgress) + Send + 'static,
    G: Fn() + Send + 'static
{
    tauri::async_runtime::spawn_blocking(move || {
        let url = "https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main/FC_26/FC.26.NOTACRACK.7z?download=true";
        let client = reqwest::blocking::Client::new();
        let mut res = client.get(url).send().map_err(|_| "Lỗi kết nối máy chủ dữ liệu (Mã: DL_01). Vui lòng kiểm tra lại mạng.".to_string())?;
        
        let total_size = res.content_length().unwrap_or(20_000_000); // fallback if unknown
        let mut downloaded: u64 = 0;
        
        let dest_path = PathBuf::from(&game_dir).join("FC.26.NOTACRACK.7z");
        let mut file = std::fs::File::create(&dest_path).map_err(|_| "Không thể tạo file tạm trên ổ cứng (Mã: DL_02).".to_string())?;
        
        let mut buffer = [0; 32768]; // 32KB buffer
        let mut last_emit = std::time::Instant::now();
        
        loop {
            let bytes_read = std::io::Read::read(&mut res, &mut buffer).map_err(|_| "Bị ngắt kết nối mạng khi đang tải (Mã: DL_03).".to_string())?;
            if bytes_read == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buffer[..bytes_read]).map_err(|_| "Lỗi ghi đĩa khi tải (Mã: DL_04). Ổ cứng có thể đã đầy.".to_string())?;
            downloaded += bytes_read as u64;
            
            // Emit progress event every 100ms
            if last_emit.elapsed().as_millis() > 100 {
                on_progress(DownloadProgress {
                    bytes_downloaded: downloaded,
                    total_bytes: total_size,
                });
                last_emit = std::time::Instant::now();
            }
        }
        
        // Emit 100% just in case
        on_progress(DownloadProgress {
            bytes_downloaded: total_size,
            total_bytes: total_size,
        });
        
        // IMPORTANT: explicitly drop file handle before extraction
        drop(file);
        
        // Extraction via 7z.exe (more reliable with AES-encrypted archives)
        on_extract();
        
        // Find 7z.exe
        let seven_zip = if std::path::Path::new("C:\\Program Files\\7-Zip\\7z.exe").exists() {
            "C:\\Program Files\\7-Zip\\7z.exe".to_string()
        } else if std::path::Path::new("C:\\Program Files (x86)\\7-Zip\\7z.exe").exists() {
            "C:\\Program Files (x86)\\7-Zip\\7z.exe".to_string()
        } else {
            return Err("Lỗi giải nén dữ liệu (Mã: DL_05). File có thể đã bị hỏng.".to_string());
        };
        
        let extract_status = std::process::Command::new(&seven_zip)
            .args([
                "x",
                dest_path.to_str().unwrap_or(""),
                &format!("-o{}", game_dir),
                "-p0xoLemon.dll",
                "-y", // overwrite all
            ])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .status()
            .map_err(|_| "Lỗi giải nén dữ liệu (Mã: DL_05). File có thể đã bị hỏng.".to_string())?;
        
        if !extract_status.success() {
            return Err("Lỗi giải nén dữ liệu (Mã: DL_05). File có thể đã bị hỏng.".to_string());
        }
        
        // Clean up 7z file
        let _ = std::fs::remove_file(&dest_path);
        
        Ok(())
    })
    .await
    .map_err(|_| "Lỗi hệ thống khi thực hiện tác vụ ngầm (Mã: DL_06).".to_string())?
}

#[command]
pub async fn download_and_extract_magic_file(app: tauri::AppHandle, game_dir: String) -> Result<(), String> {
    use tauri::Emitter;
    
    let app_clone = app.clone();
    let on_progress = move |prog: DownloadProgress| {
        let _ = app_clone.emit("magic_download_progress", prog);
    };
    
    let app_clone2 = app.clone();
    let on_extract = move || {
        let _ = app_clone2.emit("magic_extract_progress", "Extracting");
    };
    
    download_and_extract_magic_file_inner(game_dir, on_progress, on_extract).await
}
#[command]
pub async fn launch_game_executable(game_dir: String, exe_name: String, silent: Option<bool>) -> Result<(), String> {
    let mut exe_path = PathBuf::from(&game_dir);
    exe_path.push(&exe_name);
    
    if !exe_path.exists() {
        return Err(format!("Executable not found: {}", exe_path.display()));
    }
    
    let mut cmd = Command::new(&exe_path);
    cmd.current_dir(game_dir);

    if silent.unwrap_or(false) {
        cmd.arg("-silent").arg("-quiet");
        
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }

    // Launch detached
    cmd.spawn().map_err(|e| format!("Failed to launch game: {}", e))?;
        
    Ok(())
}

