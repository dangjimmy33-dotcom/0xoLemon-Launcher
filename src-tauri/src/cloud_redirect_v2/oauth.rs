//! OAuth2 authorization-code flow compatible with the upstream CloudRedirect 2.6.3 token files.
//!
//! The launcher owns the browser/loopback UX while the native engine remains the
//! authority for provider operations. Tokens are written with Windows DPAPI via
//! `upstream_config`, in the same JSON shape consumed by the upstream DLL/CLI.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use url::Url;

use super::provider_config;
use super::upstream_config;

// Public desktop OAuth clients used by the upstream project. They are not user
// secrets; desktop clients cannot keep a client secret confidential. Keep these
// values aligned with the vendored upstream OAuthService implementation.
const GOOGLE_CLIENT_ID: &str = "1072944905499-vm2v2i5dvn0a0d2o4ca36i1vge8cvbn0.apps.googleusercontent.com";
const GOOGLE_CLIENT_SECRET: &str = "v6V3fKV_zWU7iw1DrpO1rknX";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

const ONEDRIVE_CLIENT_ID: &str = "b15665d9-eda6-4092-8539-0eec376afd59";
const ONEDRIVE_CLIENT_SECRET: &str = "qtyfaBBYA403=unZUP40~_#";
const ONEDRIVE_SCOPE: &str = "Files.ReadWrite offline_access";
const ONEDRIVE_AUTH_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const ONEDRIVE_TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const ONEDRIVE_PORT: u16 = 53682;
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
struct OAuthSession {
    provider: String,
    redirect_uri: String,
    state: String,
    verifier: String,
    code: Option<String>,
    error: Option<String>,
    created_at: Instant,
}

static SESSION: OnceLock<Mutex<Option<OAuthSession>>> = OnceLock::new();

fn session_store() -> &'static Mutex<Option<OAuthSession>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

pub fn normalize_provider(provider: &str) -> Result<&'static str, String> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "gdrive" | "google" | "google_drive" | "google-drive" => Ok("gdrive"),
        "onedrive" | "one_drive" | "one-drive" => Ok("onedrive"),
        other => Err(format!("OAuth is not supported for provider: {other}")),
    }
}

/// Start OAuth and return the browser URL. The callback listener is bound before
/// the URL is returned, avoiding the race present in the previous integration.
pub async fn start_oauth_flow(provider: &str) -> Result<String, String> {
    let provider = normalize_provider(provider)?.to_string();
    let listener = if provider == "onedrive" {
        TcpListener::bind(("127.0.0.1", ONEDRIVE_PORT)).map_err(|error| {
            format!(
                "Cannot start the OneDrive callback on port {ONEDRIVE_PORT}: {error}. Close any app using that port and retry."
            )
        })?
    } else {
        TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| format!("Cannot start the Google callback listener: {error}"))?
    };
    let port = listener
        .local_addr()
        .map_err(|error| format!("Cannot resolve callback address: {error}"))?
        .port();
    listener
        .set_nonblocking(false)
        .map_err(|error| format!("Cannot configure callback listener: {error}"))?;

    let redirect_uri = if provider == "onedrive" {
        format!("http://localhost:{port}/")
    } else {
        format!("http://localhost:{port}/callback")
    };
    let state = random_urlsafe(32);
    let verifier = random_urlsafe(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    {
        let mut store = session_store()
            .lock()
            .map_err(|_| "OAuth session lock is poisoned".to_string())?;
        *store = Some(OAuthSession {
            provider: provider.clone(),
            redirect_uri: redirect_uri.clone(),
            state: state.clone(),
            verifier,
            code: None,
            error: None,
            created_at: Instant::now(),
        });
    }

    std::thread::spawn(move || accept_callback(listener));

    let (base, client_id, scope) = if provider == "gdrive" {
        (GOOGLE_AUTH_URL, GOOGLE_CLIENT_ID, GOOGLE_SCOPE)
    } else {
        (ONEDRIVE_AUTH_URL, ONEDRIVE_CLIENT_ID, ONEDRIVE_SCOPE)
    };
    let mut url = Url::parse(base).map_err(|error| error.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", scope)
            .append_pair("prompt", "consent")
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if provider == "gdrive" {
            query.append_pair("access_type", "offline");
        }
    }
    Ok(url.to_string())
}

fn accept_callback(listener: TcpListener) {
    match listener.accept() {
        Ok((mut stream, _)) => handle_callback(&mut stream),
        Err(error) => set_callback_error(format!("OAuth callback listener failed: {error}")),
    }
}

fn handle_callback(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let request_line = BufReader::new(&mut *stream)
        .lines()
        .next()
        .and_then(Result::ok)
        .unwrap_or_default();
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let parsed = Url::parse(&format!("http://localhost{target}"));

    let result = parsed
        .map_err(|error| format!("Invalid OAuth callback URL: {error}"))
        .and_then(|url| {
            let query: std::collections::HashMap<String, String> =
                url.query_pairs().into_owned().collect();
            if let Some(error) = query.get("error") {
                return Err(query
                    .get("error_description")
                    .cloned()
                    .unwrap_or_else(|| error.clone()));
            }
            let code = query
                .get("code")
                .filter(|value| !value.is_empty())
                .cloned()
                .ok_or_else(|| "OAuth callback did not contain an authorization code".to_string())?;
            let callback_state = query
                .get("state")
                .cloned()
                .ok_or_else(|| "OAuth callback did not contain state".to_string())?;

            let mut store = session_store()
                .lock()
                .map_err(|_| "OAuth session lock is poisoned".to_string())?;
            let session = store
                .as_mut()
                .ok_or_else(|| "OAuth session expired".to_string())?;
            if session.created_at.elapsed() > SESSION_TTL {
                return Err("OAuth session expired. Start the sign-in again.".to_string());
            }
            if callback_state != session.state {
                return Err("OAuth state validation failed".to_string());
            }
            session.code = Some(code);
            session.error = None;
            Ok(())
        });

    if let Err(error) = &result {
        set_callback_error(error.clone());
    }
    let (status, title, body) = if result.is_ok() {
        (
            "200 OK",
            "Authorization received / Đã nhận xác thực",
            "Return to 0xoLemon to finish connecting. / Hãy quay lại 0xoLemon để hoàn tất kết nối.",
        )
    } else {
        (
            "400 Bad Request",
            "Authorization failed / Xác thực thất bại",
            "Return to 0xoLemon to review the error. / Hãy quay lại 0xoLemon để xem lỗi.",
        )
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title><style>body{{margin:0;background:#080d11;color:#edf2f6;font:16px system-ui;display:grid;place-items:center;min-height:100vh}}main{{max-width:620px;padding:32px;border:1px solid #29343d;border-radius:14px;background:#0f161c}}h1{{font-size:24px}}p{{color:#aeb9c2;line-height:1.6}}</style></head><body><main><h1>{title}</h1><p>{body}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.as_bytes().len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn set_callback_error(error: String) {
    if let Ok(mut store) = session_store().lock() {
        if let Some(session) = store.as_mut() {
            session.error = Some(error);
        }
    }
}

/// Poll for an authorization code. A callback error is returned rather than
/// silently swallowed, so the UI can explain why sign-in failed.
pub fn get_oauth_code() -> Option<String> {
    let mut store = session_store().lock().ok()?;
    let session = store.as_mut()?;
    if session.created_at.elapsed() > SESSION_TTL {
        session.error = Some("OAuth session expired. Start the sign-in again.".to_string());
        return None;
    }
    session.code.take()
}

pub fn get_oauth_error() -> Option<String> {
    session_store()
        .lock()
        .ok()
        .and_then(|store| store.as_ref().and_then(|session| session.error.clone()))
}

/// Exchange the callback code, persist an upstream-compatible DPAPI token file,
/// then activate the provider in CloudRedirect's own config.json.
pub async fn complete_oauth_flow(provider: &str, code: &str) -> Result<(), String> {
    let provider = normalize_provider(provider)?.to_string();
    let session = {
        let store = session_store()
            .lock()
            .map_err(|_| "OAuth session lock is poisoned".to_string())?;
        store
            .as_ref()
            .cloned()
            .ok_or_else(|| "OAuth session expired. Start the sign-in again.".to_string())?
    };
    if session.provider != provider {
        return Err("OAuth provider changed during sign-in".to_string());
    }
    if session.created_at.elapsed() > SESSION_TTL {
        return Err("OAuth session expired. Start the sign-in again.".to_string());
    }

    let (token_url, client_id, client_secret, scope) = if provider == "gdrive" {
        (GOOGLE_TOKEN_URL, GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, None)
    } else {
        (
            ONEDRIVE_TOKEN_URL,
            ONEDRIVE_CLIENT_ID,
            ONEDRIVE_CLIENT_SECRET,
            Some(ONEDRIVE_SCOPE),
        )
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Cannot create OAuth client: {error}"))?;
    let mut params = vec![
        ("client_id", client_id.to_string()),
        ("client_secret", client_secret.to_string()),
        ("code", code.to_string()),
        ("redirect_uri", session.redirect_uri.clone()),
        ("grant_type", "authorization_code".to_string()),
        ("code_verifier", session.verifier.clone()),
    ];
    if let Some(scope) = scope {
        params.push(("scope", scope.to_string()));
    }
    let response = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|error| format!("Token exchange failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Cannot read OAuth response: {error}"))?;
    if !status.is_success() {
        let detail = serde_json::from_str::<OAuthErrorResponse>(&body)
            .ok()
            .and_then(|error| error.error_description.or(error.error))
            .unwrap_or(body);
        return Err(format!("OAuth token exchange failed ({status}): {detail}"));
    }
    let tokens: TokenResponse = serde_json::from_str(&body)
        .map_err(|error| format!("Invalid OAuth token response: {error}"))?;
    let expires_at = chrono::Utc::now()
        .timestamp()
        .saturating_add(tokens.expires_in.max(0));
    upstream_config::save_oauth_tokens(
        &provider,
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
        expires_at,
    )?;

    // Keep the legacy status command coherent while older screens are still
    // present. Tokens themselves are never written to this plaintext config.
    let mut legacy = provider_config::load_config().unwrap_or_default();
    legacy.provider = Some(provider.clone());
    legacy.authenticated = true;
    legacy.tokens = None;
    provider_config::save_config(&legacy)?;

    if let Ok(mut store) = session_store().lock() {
        *store = None;
    }
    Ok(())
}

fn random_urlsafe(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    let mut rng = OsRng;
    rng.fill_bytes(&mut buffer);
    URL_SAFE_NO_PAD.encode(buffer)
}

// ── Google Drive API helpers ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    #[serde(rename = "modifiedTime")]
    pub modified_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveFileListResponse {
    files: Vec<DriveFile>,
}

/// Get a valid Google Drive access token, refreshing if necessary.
async fn get_gdrive_access_token() -> Result<String, String> {
    // Load token from upstream-compatible token store
    let token_data = upstream_config::load_oauth_tokens("gdrive")?;
    Ok(token_data.access_token)
}

/// Upload a file to a named folder on Google Drive.
/// Returns the Google Drive file ID of the uploaded file.
pub async fn upload_to_google_drive(
    file_path: &std::path::Path,
    file_name: &str,
    folder_name: &str,
) -> Result<String, String> {
    let access_token = get_gdrive_access_token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Find or create the folder
    let folder_id = find_or_create_drive_folder(&client, &access_token, folder_name).await?;

    // Read file bytes
    let file_bytes = std::fs::read(file_path)
        .map_err(|e| format!("Cannot read file for upload: {e}"))?;

    // Multipart metadata + binary upload
    let metadata = serde_json::json!({
        "name": file_name,
        "parents": [folder_id]
    });
    let metadata_part = reqwest::multipart::Part::text(metadata.to_string())
        .mime_str("application/json")
        .map_err(|e| e.to_string())?;
    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .mime_str("application/zip")
        .map_err(|e| e.to_string())?
        .file_name(file_name.to_string());
    let form = reqwest::multipart::Form::new()
        .part("metadata", metadata_part)
        .part("file", file_part);

    let response = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
        .bearer_auth(&access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Drive upload failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("Drive upload failed ({status}): {body}"));
    }

    let file: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid Drive upload response: {e}"))?;
    let id = file["id"].as_str().ok_or("Drive upload: no file ID in response")?.to_string();
    Ok(id)
}

/// List all files in a named folder on Google Drive.
pub async fn list_google_drive_backups(folder_name: &str) -> Result<Vec<DriveFile>, String> {
    let access_token = get_gdrive_access_token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let folder_id = match find_drive_folder(&client, &access_token, folder_name).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(vec![]),
        Err(e) => return Err(e),
    };

    let query = format!("'{}' in parents and trashed = false", folder_id);
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name,size,modifiedTime)&orderBy=modifiedTime+desc",
        urlencoding::encode(&query)
    );

    let response = client
        .get(&url)
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| format!("Drive list failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("Drive list failed ({status}): {body}"));
    }

    let list: DriveFileListResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid Drive list response: {e}"))?;
    Ok(list.files)
}

/// Download a file from Google Drive by file ID to a local path.
pub async fn download_from_google_drive(
    file_id: &str,
    dest_path: &std::path::Path,
) -> Result<(), String> {
    let access_token = get_gdrive_access_token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?alt=media",
        file_id
    );
    let response = client
        .get(&url)
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| format!("Drive download failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Drive download failed ({status}): {body}"));
    }

    let bytes = response.bytes().await.map_err(|e| format!("Cannot read download: {e}"))?;
    std::fs::write(dest_path, &bytes).map_err(|e| format!("Cannot write download: {e}"))?;
    Ok(())
}

/// Find a Drive folder by name. Returns None if not found.
async fn find_drive_folder(
    client: &reqwest::Client,
    access_token: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let query = format!("mimeType='application/vnd.google-apps.folder' and name='{}' and trashed=false", name);
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id)&pageSize=1",
        urlencoding::encode(&query)
    );
    let response = client.get(&url).bearer_auth(access_token).send().await
        .map_err(|e| format!("Drive folder search failed: {e}"))?;
    let body = response.text().await.unwrap_or_default();
    let list: DriveFileListResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid Drive folder response: {e}"))?;
    Ok(list.files.into_iter().next().map(|f| f.id))
}

/// Find or create a Drive folder by name, returning its ID.
async fn find_or_create_drive_folder(
    client: &reqwest::Client,
    access_token: &str,
    name: &str,
) -> Result<String, String> {
    if let Some(id) = find_drive_folder(client, access_token, name).await? {
        return Ok(id);
    }
    // Create it
    let metadata = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder"
    });
    let response = client
        .post("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .json(&metadata)
        .send()
        .await
        .map_err(|e| format!("Drive folder create failed: {e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("Drive folder create failed ({status}): {body}"));
    }
    let file: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid Drive folder response: {e}"))?;
    let id = file["id"].as_str().ok_or("Drive folder: no ID in response")?.to_string();
    Ok(id)
}
