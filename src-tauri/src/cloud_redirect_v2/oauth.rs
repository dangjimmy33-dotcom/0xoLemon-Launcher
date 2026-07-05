// OAuth flow for cloud providers (Google Drive, OneDrive)

use crate::cloud_redirect_v2::provider_config::{Tokens};
use crate::cloud_redirect_v2::provider_config;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

// Google Drive OAuth credentials (from existing cloud_save module)
const GOOGLE_CLIENT_ID: &str = "745435850820-k7v8oqp0g640l8eed7p7nu6f7fd8njoh.apps.googleusercontent.com";
// OneDrive - TODO: Add when needed
const ONEDRIVE_CLIENT_ID: &str = "YOUR_ONEDRIVE_CLIENT_ID";
const ONEDRIVE_CLIENT_SECRET: &str = "YOUR_ONEDRIVE_CLIENT_SECRET";
const REDIRECT_URI: &str = "http://localhost:28608/callback";

// Shared state for OAuth callback
lazy_static::lazy_static! {
    static ref OAUTH_CODE: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct OneDriveTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    client_id: String,
    client_secret: String,
    code: String,
    redirect_uri: String,
    grant_type: String,
}

/// Start OAuth flow and return authorization URL
pub async fn start_oauth_flow(provider: &str) -> Result<String, String> {
    // Clear previous auth code
    if let Ok(mut code) = OAUTH_CODE.lock() {
        *code = None;
    }

    // Start local callback server
    start_callback_server();

    match provider {
        "google_drive" => {
            let auth_url = format!(
                "https://accounts.google.com/o/oauth2/v2/auth?\
                client_id={}&\
                redirect_uri={}&\
                response_type=code&\
                scope=https://www.googleapis.com/auth/drive.appdata&\
                access_type=offline&\
                prompt=consent",
                GOOGLE_CLIENT_ID,
                urlencoding::encode(REDIRECT_URI)
            );
            Ok(auth_url)
        }
        "onedrive" => {
            let auth_url = format!(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?\
                client_id={}&\
                redirect_uri={}&\
                response_type=code&\
                scope=Files.ReadWrite.All offline_access&\
                prompt=consent",
                ONEDRIVE_CLIENT_ID,
                urlencoding::encode(REDIRECT_URI)
            );
            Ok(auth_url)
        }
        _ => Err(format!("Unsupported provider: {}", provider))
    }
}

/// Start local HTTP server to catch OAuth callback
fn start_callback_server() {
    std::thread::spawn(move || {
        if let Ok(listener) = TcpListener::bind("127.0.0.1:28608") {
            println!("OAuth callback server listening on port 28608");
            
            // Accept only one connection
            if let Ok((stream, _)) = listener.accept() {
                handle_callback(stream);
            }
        } else {
            eprintln!("Failed to start OAuth callback server on port 28608");
        }
    });
}

/// Handle OAuth callback request
fn handle_callback(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&stream);
    let request_line = buf_reader.lines().next();
    
    if let Some(Ok(line)) = request_line {
        // Parse URL: GET /callback?code=xxx&... HTTP/1.1
        if let Some(query_start) = line.find("?") {
            if let Some(query_end) = line.find(" HTTP") {
                let query = &line[query_start + 1..query_end];
                
                // Extract code parameter
                for param in query.split('&') {
                    if let Some((key, value)) = param.split_once('=') {
                        if key == "code" {
                            // Store code
                            if let Ok(mut code) = OAUTH_CODE.lock() {
                                *code = Some(value.to_string());
                            }
                            
                            // Send success response
                            let response = "HTTP/1.1 200 OK\r\n\
                                Content-Type: text/html\r\n\
                                \r\n\
                                <html><body>\
                                <h1>✅ Authentication Successful!</h1>\
                                <p>You can close this window and return to 0xoLemon Launcher.</p>\
                                <script>window.close();</script>\
                                </body></html>";
                            let _ = stream.write_all(response.as_bytes());
                            return;
                        }
                    }
                }
            }
        }
    }
    
    // Send error response
    let response = "HTTP/1.1 400 Bad Request\r\n\
        Content-Type: text/html\r\n\
        \r\n\
        <html><body><h1>❌ Authentication Failed</h1></body></html>";
    let _ = stream.write_all(response.as_bytes());
}

/// Poll for OAuth code (called from frontend)
pub fn get_oauth_code() -> Option<String> {
    if let Ok(mut code) = OAUTH_CODE.lock() {
        code.take() // Take and clear
    } else {
        None
    }
}

/// Complete OAuth flow by exchanging code for tokens
pub async fn complete_oauth_flow(provider: &str, code: &str) -> Result<(), String> {
    match provider {
        "google_drive" => {
            exchange_google_code(code).await?;
        }
        "onedrive" => {
            exchange_onedrive_code(code).await?;
        }
        _ => return Err(format!("Unsupported provider: {}", provider))
    }
    
    Ok(())
}

async fn exchange_google_code(code: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    
    // Google Drive uses public client (no client_secret for installed apps)
    let params = [
        ("client_id", GOOGLE_CLIENT_ID),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("grant_type", "authorization_code"),
    ];
    
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to exchange code: {}", e))?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("OAuth error: {}", error_text));
    }
    
    let token_response: GoogleTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;
    
    let expires_at = chrono::Utc::now().timestamp() + token_response.expires_in;
    
    let tokens = Tokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at: Some(expires_at),
    };
    
    let mut config = provider_config::load_config().unwrap_or_default();
    config.tokens = Some(tokens);
    config.authenticated = true;
    config.provider = Some("google_drive".to_string());
    provider_config::save_config(&config)?;
    
    Ok(())
}

async fn exchange_onedrive_code(code: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    
    let params = [
        ("client_id", ONEDRIVE_CLIENT_ID),
        ("client_secret", ONEDRIVE_CLIENT_SECRET),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("grant_type", "authorization_code"),
    ];
    
    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to exchange code: {}", e))?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("OAuth error: {}", error_text));
    }
    
    let token_response: OneDriveTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;
    
    let expires_at = chrono::Utc::now().timestamp() + token_response.expires_in;
    
    let tokens = Tokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at: Some(expires_at),
    };
    
    let mut config = provider_config::load_config().unwrap_or_default();
    config.tokens = Some(tokens);
    config.authenticated = true;
    config.provider = Some("onedrive".to_string());
    provider_config::save_config(&config)?;
    
    Ok(())
}

/// Refresh access token using refresh token
pub async fn refresh_token(provider: &str) -> Result<(), String> {
    let config = provider_config::load_config()?;
    
    let tokens = config.tokens.ok_or("No tokens found")?;
    let refresh_token = tokens.refresh_token.ok_or("No refresh token")?;
    
    match provider {
        "google_drive" => refresh_google_token(&refresh_token).await?,
        "onedrive" => refresh_onedrive_token(&refresh_token).await?,
        _ => return Err(format!("Unsupported provider: {}", provider))
    }
    
    Ok(())
}

async fn refresh_google_token(refresh_token: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    
    // Google Drive public client refresh (no client_secret)
    let params = [
        ("client_id", GOOGLE_CLIENT_ID),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to refresh token: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to refresh Google token".to_string());
    }
    
    let token_response: GoogleTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    let expires_at = chrono::Utc::now().timestamp() + token_response.expires_in;
    
    let tokens = Tokens {
        access_token: token_response.access_token,
        refresh_token: Some(refresh_token.to_string()),
        expires_at: Some(expires_at),
    };
    
    let mut config = provider_config::load_config()?;
    config.tokens = Some(tokens);
    provider_config::save_config(&config)?;
    
    Ok(())
}

async fn refresh_onedrive_token(refresh_token: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    
    let params = [
        ("client_id", ONEDRIVE_CLIENT_ID),
        ("client_secret", ONEDRIVE_CLIENT_SECRET),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    
    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to refresh token: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to refresh OneDrive token".to_string());
    }
    
    let token_response: OneDriveTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    let expires_at = chrono::Utc::now().timestamp() + token_response.expires_in;
    
    let tokens = Tokens {
        access_token: token_response.access_token,
        refresh_token: Some(refresh_token.to_string()),
        expires_at: Some(expires_at),
    };
    
    let mut config = provider_config::load_config()?;
    config.tokens = Some(tokens);
    provider_config::save_config(&config)?;
    
    Ok(())
}


// ===== Google Drive File Operations =====

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoogleDriveFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub modified_time: Option<String>,
}

/// Upload file to Google Drive
pub async fn upload_to_google_drive(
    file_path: &std::path::Path,
    file_name: &str,
    folder_name: &str,
) -> Result<String, String> {
    let config = provider_config::load_config()?;
    let tokens = config.tokens.ok_or("Not authenticated")?;
    
    // Check if token expired
    if let Some(expires_at) = tokens.expires_at {
        if chrono::Utc::now().timestamp() >= expires_at - 300 {
            // Refresh token if expired or expiring soon
            if let Some(provider) = &config.provider {
                refresh_token(provider).await?;
            }
        }
    }
    
    let config = provider_config::load_config()?;
    let tokens = config.tokens.ok_or("Failed to get refreshed token")?;
    
    let client = reqwest::Client::new();
    
    // Step 1: Find or create CloudRedirect folder
    let folder_id = get_or_create_folder(&client, &tokens.access_token, folder_name).await?;
    
    // Step 2: Upload file
    let file_content = std::fs::read(file_path).map_err(|e| e.to_string())?;
    
    let metadata = serde_json::json!({
        "name": file_name,
        "parents": [folder_id]
    });
    
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    
    // Add metadata part
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(metadata.to_string().as_bytes());
    body.extend_from_slice(b"\r\n");
    
    // Add file content part
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/zip\r\n\r\n");
    body.extend_from_slice(&file_content);
    body.extend_from_slice(format!("\r\n--{}--", boundary).as_bytes());
    
    let response = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .header("Content-Type", format!("multipart/related; boundary={}", boundary))
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;
    
    if !response.status().is_success() {
        let error = response.text().await.unwrap_or_default();
        return Err(format!("Upload error: {}", error));
    }
    
    let result: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let file_id = result["id"].as_str().ok_or("No file ID in response")?;
    
    Ok(file_id.to_string())
}

/// Get or create folder in Google Drive
async fn get_or_create_folder(
    client: &reqwest::Client,
    access_token: &str,
    folder_name: &str,
) -> Result<String, String> {
    // Search for existing folder
    let query = format!("name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false", folder_name);
    
    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[("q", &query), ("fields", &"files(id, name)".to_string())])
        .send()
        .await
        .map_err(|e| format!("Search failed: {}", e))?;
    
    if response.status().is_success() {
        let result: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = result["files"].as_array() {
            if !files.is_empty() {
                if let Some(folder_id) = files[0]["id"].as_str() {
                    return Ok(folder_id.to_string());
                }
            }
        }
    }
    
    // Create new folder if not found
    let metadata = serde_json::json!({
        "name": folder_name,
        "mimeType": "application/vnd.google-apps.folder"
    });
    
    let response = client
        .post("https://www.googleapis.com/drive/v3/files")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&metadata)
        .send()
        .await
        .map_err(|e| format!("Create folder failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to create folder".to_string());
    }
    
    let result: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let folder_id = result["id"].as_str().ok_or("No folder ID in response")?;
    
    Ok(folder_id.to_string())
}

/// List backups from Google Drive
pub async fn list_google_drive_backups(folder_name: &str) -> Result<Vec<GoogleDriveFile>, String> {
    let config = provider_config::load_config()?;
    let tokens = config.tokens.ok_or("Not authenticated")?;
    
    let client = reqwest::Client::new();
    
    // Get folder ID
    let folder_id = get_or_create_folder(&client, &tokens.access_token, folder_name).await?;
    
    // List files in folder
    let query = format!("'{}' in parents and trashed=false", folder_id);
    
    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .query(&[
            ("q", &query),
            ("fields", &"files(id, name, size, modifiedTime)".to_string())
        ])
        .send()
        .await
        .map_err(|e| format!("List failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to list backups".to_string());
    }
    
    let result: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    
    if let Some(files) = result["files"].as_array() {
        for file in files {
            backups.push(GoogleDriveFile {
                id: file["id"].as_str().unwrap_or("").to_string(),
                name: file["name"].as_str().unwrap_or("").to_string(),
                size: file["size"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
                modified_time: file["modifiedTime"].as_str().map(|s| s.to_string()),
            });
        }
    }
    
    Ok(backups)
}

/// Download file from Google Drive
pub async fn download_from_google_drive(file_id: &str, dest_path: &std::path::Path) -> Result<(), String> {
    let config = provider_config::load_config()?;
    let tokens = config.tokens.ok_or("Not authenticated")?;
    
    let client = reqwest::Client::new();
    
    let response = client
        .get(format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", file_id))
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to download file".to_string());
    }
    
    let content = response.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(dest_path, &content).map_err(|e| e.to_string())?;
    
    Ok(())
}
