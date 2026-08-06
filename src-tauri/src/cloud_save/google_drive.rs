use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Datelike, Utc};
use base64::Engine;
use rand_core::{OsRng, RngCore};
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use url::Url;

use crate::secret_store::{protect as protect_secret, unprotect as unprotect_secret};

use super::{
    copy_file_synced, read_manifest, replace_file_with_rollback, safe_join,
    write_manifest, CloudFileEntry, CloudManifest,
};

const AUTH_FILE: &str = "google-drive-auth.json";
const DEFAULT_CLIENT_ID: &str =
    "745435850820-k7v8oqp0g640l8eed7p7nu6f7fd8njoh.apps.googleusercontent.com";
const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
const APP_DATA_SPACE: &str = "appDataFolder";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_RETRIES: u32 = 6;
const RESUMABLE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_SCHEMA: u32 = 2;

static ACCESS_TOKEN: OnceLock<Mutex<Option<CachedAccessToken>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<Option<CachedAccessToken>> {
    ACCESS_TOKEN.get_or_init(|| Mutex::new(None))
}

fn cache_access_token(value: String, expires_in: u64) {
    let expires_at = unix_seconds().saturating_add(expires_in);
    if let Ok(mut cached) = token_cache().lock() {
        *cached = Some(CachedAccessToken { value, expires_at });
    }
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    value: String,
    expires_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAuth {
    #[serde(default)]
    client_id: String,
    encrypted_refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum DriveFailureKind {
    Offline,
    RateLimited,
    StorageFull,
    AuthRequired,
    NotFound,
    CorruptRemote,
    PermissionDenied,
    RemoteChanged,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DriveFailure {
    pub kind: DriveFailureKind,
    pub message: String,
    pub retryable: bool,
    pub retry_after_seconds: Option<u64>,
    pub http_status: Option<u16>,
    pub reason: String,
}

impl fmt::Display for DriveFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DriveFailure {}

impl DriveFailure {
    fn offline(error: impl ToString) -> Self {
        Self {
            kind: DriveFailureKind::Offline,
            message: "Google Drive tạm thời không kết nối được. Save mới nhất vẫn an toàn trên máy này.".to_string(),
            retryable: true,
            retry_after_seconds: Some(30),
            http_status: None,
            reason: error.to_string(),
        }
    }

    fn other(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind: DriveFailureKind::Other,
            reason: message.clone(),
            message,
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
        }
    }

    fn remote_changed(expected: Option<&str>, actual: Option<&str>) -> Self {
        Self {
            kind: DriveFailureKind::RemoteChanged,
            message: "Save trên Cloud vừa được cập nhật từ thiết bị khác. Launcher đã giữ nguyên Save trên máy để so sánh an toàn.".to_string(),
            retryable: true,
            retry_after_seconds: Some(5),
            http_status: Some(409),
            reason: format!(
                "remote head changed: expected={}, actual={}",
                expected.unwrap_or("<none>"),
                actual.unwrap_or("<none>")
            ),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DriveQuota {
    pub limit_bytes: Option<u64>,
    pub usage_bytes: u64,
    pub usage_in_drive_bytes: u64,
    pub usage_in_trash_bytes: u64,
    pub available_bytes: Option<u64>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteHead {
    pub schema_version: u32,
    pub game_id: String,
    pub snapshot_id: String,
    pub parent_snapshot_id: Option<String>,
    pub generation: u64,
    pub created_at: String,
    pub device_id: String,
    pub device_name: String,
    pub manifest_file_id: String,
    pub manifest_sha256: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteManifest {
    schema_version: u32,
    game_id: String,
    snapshot_id: String,
    parent_snapshot_id: Option<String>,
    created_at: String,
    device_id: String,
    device_name: String,
    snapshot_class: String,
    pinned: bool,
    files: Vec<CloudFileEntry>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteFetch {
    pub head: RemoteHead,
    pub manifest: CloudManifest,
    pub files_root: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GarbageCollectionReport {
    pub scanned_objects: usize,
    pub deleted_objects: usize,
    pub reclaimed_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct DriveFileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    modified_time: String,
    #[serde(default)]
    app_properties: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DriveFileCreated {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveAbout {
    storage_quota: DriveStorageQuota,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveStorageQuota {
    limit: Option<String>,
    #[serde(default)]
    usage: String,
    #[serde(default)]
    usage_in_drive: String,
    #[serde(default)]
    usage_in_drive_trash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPageToken {
    start_page_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeList {
    next_page_token: Option<String>,
    new_start_page_token: Option<String>,
    #[serde(default)]
    changes: Vec<DriveChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveChange {
    #[serde(default)]
    file_id: String,
    #[serde(default)]
    removed: bool,
}

#[derive(Debug, Deserialize)]
struct GoogleErrorEnvelope {
    error: GoogleErrorBody,
}

#[derive(Debug, Deserialize)]
struct GoogleErrorBody {
    #[serde(default)]
    message: String,
    #[serde(default)]
    errors: Vec<GoogleErrorItem>,
}

#[derive(Debug, Deserialize)]
struct GoogleErrorItem {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
}

pub(super) fn client_configured() -> bool {
    !client_id().is_empty()
}

pub(super) fn connected(app: &AppHandle) -> bool {
    read_stored_auth(app).is_ok()
}

pub(super) fn disconnect(app: &AppHandle) -> Result<(), String> {
    let path = auth_path(app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    if let Ok(mut cached) = token_cache().lock() {
        *cached = None;
    }
    Ok(())
}

pub(super) fn authorize(app: &AppHandle) -> Result<(), String> {
    let client_id = client_id();
    if client_id.is_empty() {
        return Err("Google Drive OAuth client ID chưa được cấu hình.".to_string());
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random_urlsafe(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(32);
    let mut auth_url = Url::parse(AUTH_ENDPOINT).map_err(|error| error.to_string())?;
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", DRIVE_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    open_system_browser(auth_url.as_str())?;

    let deadline = Instant::now() + OAUTH_TIMEOUT;
    let mut authorization_code = None;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = [0_u8; 8192];
                let read = stream.read(&mut request).map_err(|error| error.to_string())?;
                let request = String::from_utf8_lossy(&request[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| "Google OAuth callback không hợp lệ".to_string())?;
                let callback = Url::parse(&format!("http://127.0.0.1:{port}{target}"))
                    .map_err(|error| error.to_string())?;
                let params = callback.query_pairs().into_owned().collect::<Vec<_>>();
                let returned_state = params
                    .iter()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.as_str());
                let error = params
                    .iter()
                    .find(|(key, _)| key == "error")
                    .map(|(_, value)| value.clone());
                let code = params
                    .iter()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.clone());
                let success = returned_state == Some(state.as_str()) && error.is_none() && code.is_some();
                let body = if success {
                    "<html><body style='font-family:system-ui;background:#111;color:#fff;padding:32px'><h2>0xoLemon đã kết nối Google Drive.</h2><p>Bạn có thể đóng tab này và quay lại launcher.</p></body></html>"
                } else {
                    "<html><body style='font-family:system-ui;background:#111;color:#fff;padding:32px'><h2>Không thể kết nối Google Drive.</h2><p>Hãy quay lại launcher để xem hướng dẫn.</p></body></html>"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                stream.write_all(response.as_bytes()).map_err(|error| error.to_string())?;
                if returned_state != Some(state.as_str()) {
                    return Err("Google OAuth state validation failed".to_string());
                }
                if let Some(error) = error {
                    return Err(format!("Google authorization was denied: {error}"));
                }
                authorization_code = code;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let code = authorization_code.ok_or_else(|| "Google Drive sign-in timed out".to_string())?;
    let token = http_client()
        .map_err(|error| error.to_string())?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .map_err(|error| error.to_string())?;
    let token = checked_json::<TokenResponse>(token).map_err(|error| error.to_string())?;
    let refresh_token = token.refresh_token.ok_or_else(|| {
        "Google không trả refresh token; hãy thu hồi quyền và đăng nhập lại.".to_string()
    })?;
    write_refresh_token(app, &refresh_token)?;
    cache_access_token(token.access_token, token.expires_in);
    Ok(())
}

// `fetch_remote_snapshot` is the public contract name used by the sync engine design.
// The implementation materializes the verified snapshot into a private cache.
#[allow(dead_code)]
pub(super) fn fetch_remote_snapshot(
    app: &AppHandle,
    game_id: &str,
    cache_root: &Path,
) -> Result<Option<RemoteFetch>, DriveFailure> {
    fetch_remote_to_cache(app, game_id, cache_root)
}

pub(super) fn fetch_remote_to_cache(
    app: &AppHandle,
    game_id: &str,
    cache_root: &Path,
) -> Result<Option<RemoteFetch>, DriveFailure> {
    if !connected(app) {
        return Ok(None);
    }
    let token = access_token(app)?;
    let Some(head_file) = find_single_by_properties(
        &token,
        &[("kind", "head"), ("gameId", game_id)],
    )? else {
        return Ok(None);
    };
    let head_bytes = download_bytes(&token, &head_file.id)?;
    let head: RemoteHead = serde_json::from_slice(&head_bytes).map_err(|error| DriveFailure {
        kind: DriveFailureKind::CorruptRemote,
        message: "Cloud Save trên Google Drive bị hỏng metadata; launcher chưa ghi đè dữ liệu local.".to_string(),
        retryable: false,
        retry_after_seconds: None,
        http_status: None,
        reason: error.to_string(),
    })?;
    if head.schema_version != REMOTE_SCHEMA || head.game_id != game_id {
        return Err(DriveFailure {
            kind: DriveFailureKind::CorruptRemote,
            message: "Cloud Save metadata không tương thích; save local vẫn an toàn.".to_string(),
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
            reason: "remote head schema/game mismatch".to_string(),
        });
    }
    let manifest_bytes = download_bytes(&token, &head.manifest_file_id)?;
    if !hex_sha256(&manifest_bytes).eq_ignore_ascii_case(&head.manifest_sha256) {
        return Err(DriveFailure {
            kind: DriveFailureKind::CorruptRemote,
            message: "Cloud Save manifest không vượt qua kiểm tra toàn vẹn.".to_string(),
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
            reason: "manifest sha256 mismatch".to_string(),
        });
    }
    let remote: RemoteManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| DriveFailure {
        kind: DriveFailureKind::CorruptRemote,
        message: "Cloud Save manifest không đọc được.".to_string(),
        retryable: false,
        retry_after_seconds: None,
        http_status: None,
        reason: error.to_string(),
    })?;
    if remote.snapshot_id != head.snapshot_id || remote.game_id != game_id {
        return Err(DriveFailure {
            kind: DriveFailureKind::CorruptRemote,
            message: "Cloud Save head và manifest không khớp nhau.".to_string(),
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
            reason: "head/manifest mismatch".to_string(),
        });
    }

    let staging = cache_root.with_extension(format!("{}.new", head.snapshot_id));
    if staging.exists() {
        clear_tree(&staging).map_err(DriveFailure::other)?;
    }
    fs::create_dir_all(staging.join("files")).map_err(DriveFailure::offline)?;
    let object_index = list_files_by_kind(&token, "object")?
        .into_iter()
        .filter_map(|file| file.app_properties.get("sha256").cloned().map(|hash| (hash, file)))
        .collect::<HashMap<_, _>>();

    for entry in &remote.files {
        let object_hash = if entry.sha256.trim().is_empty() {
            return Err(DriveFailure {
                kind: DriveFailureKind::CorruptRemote,
                message: "Cloud Save cũ thiếu SHA-256 object.".to_string(),
                retryable: false,
                retry_after_seconds: None,
                http_status: None,
                reason: entry.path.clone(),
            });
        } else {
            entry.sha256.clone()
        };
        let file = object_index.get(&object_hash).ok_or_else(|| DriveFailure {
            kind: DriveFailureKind::CorruptRemote,
            message: "Cloud Save thiếu một phần dữ liệu; launcher chưa ghi đè save local.".to_string(),
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
            reason: format!("missing object {object_hash}"),
        })?;
        let target = safe_join(&staging.join("files"), &entry.path).map_err(DriveFailure::other)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(DriveFailure::offline)?;
        }
        download_file(&token, &file.id, &target)?;
        let actual = sha256_file(&target).map_err(DriveFailure::offline)?;
        let size = fs::metadata(&target).map_err(DriveFailure::offline)?.len();
        if actual != object_hash || size != entry.size {
            return Err(DriveFailure {
                kind: DriveFailureKind::CorruptRemote,
                message: "Cloud Save object không vượt qua kiểm tra toàn vẹn.".to_string(),
                retryable: false,
                retry_after_seconds: None,
                http_status: None,
                reason: entry.path.clone(),
            });
        }
    }
    let manifest = CloudManifest {
        generated_at: remote.created_at,
        files: remote.files,
    };
    write_manifest(&staging.join("manifest.json"), &manifest).map_err(DriveFailure::other)?;
    replace_tree_atomic(&staging, cache_root).map_err(DriveFailure::other)?;

    Ok(Some(RemoteFetch {
        head,
        manifest,
        files_root: cache_root.join("files"),
    }))
}

pub(super) fn upload_current_snapshot(
    app: &AppHandle,
    game_id: &str,
    device_id: &str,
    device_name: &str,
    snapshot_id: &str,
    parent_snapshot_id: Option<&str>,
    generation: u64,
    current_root: &Path,
    snapshot_class: &str,
    pinned: bool,
) -> Result<RemoteHead, DriveFailure> {
    let manifest = read_manifest(&current_root.join("manifest.json")).map_err(DriveFailure::other)?;
    if manifest.files.is_empty() {
        return Err(DriveFailure::other("Không có save file để đồng bộ."));
    }
    let quota = storage_quota(app)?;
    let token = access_token(app)?;
    ensure_remote_parent_unchanged(&token, game_id, parent_snapshot_id)?;
    let existing_hashes = list_files_by_kind(&token, "object")?
        .into_iter()
        .filter_map(|file| file.app_properties.get("sha256").cloned())
        .collect::<HashSet<_>>();
    let projected_new_bytes = manifest
        .files
        .iter()
        .filter(|entry| !existing_hashes.contains(&entry.sha256))
        .map(|entry| entry.size)
        .sum::<u64>();
    if quota
        .available_bytes
        .is_some_and(|available| available < projected_new_bytes.saturating_add(16 * 1024 * 1024))
    {
        return Err(DriveFailure {
            kind: DriveFailureKind::StorageFull,
            message: "Google Drive không còn đủ dung lượng. Save mới nhất vẫn được bảo vệ trên máy này.".to_string(),
            retryable: true,
            retry_after_seconds: Some(15 * 60),
            http_status: Some(403),
            reason: "storageQuotaExceeded (preflight)".to_string(),
        });
    }

    for entry in &manifest.files {
        let source = safe_join(&current_root.join("files"), &entry.path).map_err(DriveFailure::other)?;
        upload_object_if_missing(&token, game_id, &entry.sha256, &source, entry.size)?;
    }

    let remote_manifest = RemoteManifest {
        schema_version: REMOTE_SCHEMA,
        game_id: game_id.to_string(),
        snapshot_id: snapshot_id.to_string(),
        parent_snapshot_id: parent_snapshot_id.map(ToOwned::to_owned),
        created_at: chrono::Utc::now().to_rfc3339(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        snapshot_class: snapshot_class.to_string(),
        pinned,
        files: manifest.files.clone(),
    };
    let manifest_bytes = serde_json::to_vec(&remote_manifest).map_err(|error| DriveFailure::other(error.to_string()))?;
    let manifest_hash = hex_sha256(&manifest_bytes);
    let manifest_file_id = upload_manifest(
        &token,
        game_id,
        snapshot_id,
        snapshot_class,
        pinned,
        &manifest_bytes,
    )?;
    let head = RemoteHead {
        schema_version: REMOTE_SCHEMA,
        game_id: game_id.to_string(),
        snapshot_id: snapshot_id.to_string(),
        parent_snapshot_id: parent_snapshot_id.map(ToOwned::to_owned),
        generation,
        created_at: chrono::Utc::now().to_rfc3339(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        manifest_file_id,
        manifest_sha256: manifest_hash,
        file_count: manifest.files.len(),
        total_bytes: manifest.files.iter().map(|entry| entry.size).sum(),
    };
    // Check again immediately before moving the singleton head. This prevents a
    // stale device from silently overwriting a newer branch created while its
    // objects were uploading. If another device won the race, remove only this
    // uncommitted manifest; immutable objects remain safe for later GC/reuse.
    if let Err(error) = ensure_remote_parent_unchanged(&token, game_id, parent_snapshot_id) {
        let _ = delete_file(&token, &head.manifest_file_id);
        return Err(error);
    }
    commit_head(&token, game_id, &head)?;
    let _ = write_device_record(&token, device_id, device_name);
    Ok(head)
}

pub(super) fn storage_quota(app: &AppHandle) -> Result<DriveQuota, DriveFailure> {
    let token = access_token(app)?;
    let response = send_with_backoff(|| {
        http_client()?.get(format!("{DRIVE_API}/about"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .query(&[("fields", "storageQuota(limit,usage,usageInDrive,usageInDriveTrash)")])
            .send()
            .map_err(DriveFailure::offline)
    })?;
    let about = checked_json::<DriveAbout>(response)?;
    let limit = parse_u64_opt(about.storage_quota.limit.as_deref());
    let usage = parse_u64(&about.storage_quota.usage);
    Ok(DriveQuota {
        limit_bytes: limit,
        usage_bytes: usage,
        usage_in_drive_bytes: parse_u64(&about.storage_quota.usage_in_drive),
        usage_in_trash_bytes: parse_u64(&about.storage_quota.usage_in_drive_trash),
        available_bytes: limit.map(|value| value.saturating_sub(usage)),
        checked_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub(super) fn get_start_page_token(app: &AppHandle) -> Result<String, DriveFailure> {
    let token = access_token(app)?;
    let response = send_with_backoff(|| {
        http_client()?.get(format!("{DRIVE_API}/changes/startPageToken"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .map_err(DriveFailure::offline)
    })?;
    Ok(checked_json::<StartPageToken>(response)?.start_page_token)
}

pub(super) fn changes_since(
    app: &AppHandle,
    page_token: &str,
) -> Result<(Vec<String>, String), DriveFailure> {
    let token = access_token(app)?;
    let mut next = Some(page_token.to_string());
    let mut changed = Vec::new();
    let mut new_start = page_token.to_string();
    while let Some(token_page) = next.take() {
        let response = send_with_backoff(|| {
            http_client()?.get(format!("{DRIVE_API}/changes"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .query(&[
                    ("pageToken", token_page.as_str()),
                    ("spaces", APP_DATA_SPACE),
                    ("includeRemoved", "true"),
                    ("fields", "nextPageToken,newStartPageToken,changes(fileId,removed)"),
                ])
                .send()
                .map_err(DriveFailure::offline)
        })?;
        let list = checked_json::<ChangeList>(response)?;
        changed.extend(
            list.changes
                .into_iter()
                .filter(|change| !change.file_id.is_empty())
                .map(|change| if change.removed { format!("removed:{}", change.file_id) } else { change.file_id }),
        );
        if let Some(value) = list.new_start_page_token {
            new_start = value;
        }
        next = list.next_page_token;
    }
    Ok((changed, new_start))
}

pub(super) fn garbage_collect_unreferenced_objects(
    app: &AppHandle,
) -> Result<GarbageCollectionReport, DriveFailure> {
    let token = access_token(app)?;
    let manifests = list_files_by_kind(&token, "manifest")?;
    let mut referenced = HashSet::new();
    for manifest_file in manifests {
        let bytes = download_bytes(&token, &manifest_file.id)?;
        let manifest = match serde_json::from_slice::<RemoteManifest>(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        referenced.extend(
            manifest
                .files
                .into_iter()
                .filter(|entry| !entry.sha256.is_empty())
                .map(|entry| entry.sha256),
        );
    }
    let objects = list_files_by_kind(&token, "object")?;
    let mut report = GarbageCollectionReport {
        scanned_objects: objects.len(),
        ..GarbageCollectionReport::default()
    };
    for object in objects {
        let Some(hash) = object.app_properties.get("sha256") else {
            continue;
        };
        if referenced.contains(hash) {
            continue;
        }
        delete_file(&token, &object.id)?;
        report.deleted_objects += 1;
        report.reclaimed_bytes = report
            .reclaimed_bytes
            .saturating_add(object.size.as_deref().map(parse_u64).unwrap_or(0));
    }
    Ok(report)
}

pub(super) fn prune_remote_history(
    app: &AppHandle,
    game_id: &str,
    current_snapshot_id: Option<&str>,
) -> Result<GarbageCollectionReport, DriveFailure> {
    let token = access_token(app)?;
    let manifest_files = list_by_properties(
        &token,
        &[("kind", "manifest"), ("gameId", game_id)],
    )?;

    struct HistoryEntry {
        file_id: String,
        snapshot_id: String,
        snapshot_class: String,
        pinned: bool,
        created_at: Option<DateTime<Utc>>,
    }

    let mut history = Vec::new();
    let mut unreadable_file_ids = HashSet::new();
    for file in manifest_files {
        let bytes = match download_bytes(&token, &file.id) {
            Ok(bytes) => bytes,
            Err(_) => {
                unreadable_file_ids.insert(file.id);
                continue;
            }
        };
        let manifest = match serde_json::from_slice::<RemoteManifest>(&bytes) {
            Ok(manifest) => manifest,
            Err(_) => {
                // Never delete metadata that cannot be understood automatically.
                unreadable_file_ids.insert(file.id);
                continue;
            }
        };
        history.push(HistoryEntry {
            file_id: file.id,
            snapshot_id: manifest.snapshot_id,
            snapshot_class: manifest.snapshot_class,
            pinned: manifest.pinned,
            created_at: DateTime::parse_from_rfc3339(&manifest.created_at)
                .ok()
                .map(|value| value.with_timezone(&Utc)),
        });
    }
    history.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    let now = Utc::now();
    let mut keep = HashSet::new();
    keep.extend(unreadable_file_ids);
    if let Some(snapshot_id) = current_snapshot_id {
        for entry in &history {
            if entry.snapshot_id == snapshot_id {
                keep.insert(entry.file_id.clone());
            }
        }
    }
    for entry in &history {
        if entry.pinned {
            keep.insert(entry.file_id.clone());
        }
    }
    for entry in history.iter().take(10) {
        keep.insert(entry.file_id.clone());
    }

    let mut daily = HashSet::new();
    let mut weekly = HashSet::new();
    for entry in &history {
        let Some(created) = entry.created_at.clone() else {
            keep.insert(entry.file_id.clone());
            continue;
        };
        let age = now.signed_duration_since(created);
        if age.num_days() <= 7 && daily.insert(created.date_naive()) {
            keep.insert(entry.file_id.clone());
        }
        let iso = created.iso_week();
        if age.num_weeks() <= 4 && weekly.insert((iso.year(), iso.week())) {
            keep.insert(entry.file_id.clone());
        }
        if entry.snapshot_class == "conflict" && age.num_days() <= 90 {
            keep.insert(entry.file_id.clone());
        }
    }

    for entry in history {
        if !keep.contains(&entry.file_id) {
            delete_file(&token, &entry.file_id)?;
        }
    }
    garbage_collect_unreferenced_objects(app)
}

pub(super) fn delete_remote_manifest(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
) -> Result<(), DriveFailure> {
    let token = access_token(app)?;
    if let Some(file) = find_single_by_properties(
        &token,
        &[("kind", "manifest"), ("gameId", game_id), ("snapshotId", snapshot_id)],
    )? {
        delete_file(&token, &file.id)?;
    }
    Ok(())
}

pub(super) fn export_remote_snapshot(
    app: &AppHandle,
    game_id: &str,
    target: &Path,
) -> Result<(), DriveFailure> {
    let cache = target.with_extension("0xo-export-cache");
    let Some(remote) = fetch_remote_to_cache(app, game_id, &cache)? else {
        return Err(DriveFailure::other("Game chưa có Cloud Save trên Google Drive."));
    };
    if target.exists() {
        clear_tree(target).map_err(DriveFailure::other)?;
    }
    fs::create_dir_all(target).map_err(DriveFailure::offline)?;
    for entry in &remote.manifest.files {
        let source = safe_join(&remote.files_root, &entry.path).map_err(DriveFailure::other)?;
        let destination = safe_join(target, &entry.path).map_err(DriveFailure::other)?;
        copy_file_synced(&source, &destination).map_err(DriveFailure::other)?;
    }
    write_manifest(&target.join("manifest.json"), &remote.manifest).map_err(DriveFailure::other)?;
    let _ = clear_tree(&cache);
    Ok(())
}

pub(super) fn truncated_backoff(attempt: u32) -> Duration {
    let exponent = attempt.min(6);
    let base_ms = (1u64 << exponent) * 1_000;
    let jitter_ms = (OsRng.next_u32() as u64) % 1_000;
    Duration::from_millis((base_ms + jitter_ms).min(65_000))
}

fn retry_delay(attempt: u32, failure: Option<&DriveFailure>) -> Duration {
    failure
        .and_then(|failure| failure.retry_after_seconds)
        .map(|seconds| Duration::from_secs(seconds.min(5 * 60)))
        .unwrap_or_else(|| truncated_backoff(attempt))
}

fn upload_object_if_missing(
    token: &str,
    game_id: &str,
    sha256: &str,
    source: &Path,
    size: u64,
) -> Result<String, DriveFailure> {
    if sha256.trim().is_empty() {
        return Err(DriveFailure::other("Object SHA-256 bị trống."));
    }
    if let Some(existing) = find_single_by_properties(token, &[("kind", "object"), ("sha256", sha256)])? {
        return Ok(existing.id);
    }
    let metadata = serde_json::json!({
        "name": format!("oxo-object-{sha256}.bin"),
        "parents": [APP_DATA_SPACE],
        "mimeType": "application/octet-stream",
        "appProperties": {
            "kind": "object",
            "sha256": sha256,
            "gameId": game_id,
            "schema": REMOTE_SCHEMA.to_string()
        }
    });
    upload_file(token, None, &metadata, source, size, "application/octet-stream")
}

fn upload_manifest(
    token: &str,
    game_id: &str,
    snapshot_id: &str,
    snapshot_class: &str,
    pinned: bool,
    bytes: &[u8],
) -> Result<String, DriveFailure> {
    let temporary = std::env::temp_dir().join(format!("oxo-manifest-{snapshot_id}.json"));
    fs::write(&temporary, bytes).map_err(DriveFailure::offline)?;
    let metadata = serde_json::json!({
        "name": format!("oxo-manifest-{}-{}.json", sanitize_name(game_id), snapshot_id),
        "parents": [APP_DATA_SPACE],
        "mimeType": "application/json",
        "appProperties": {
            "kind": "manifest",
            "gameId": game_id,
            "snapshotId": snapshot_id,
            "snapshotClass": snapshot_class,
            "pinned": if pinned { "true" } else { "false" },
            "schema": REMOTE_SCHEMA.to_string()
        }
    });
    let result = upload_file(token, None, &metadata, &temporary, bytes.len() as u64, "application/json");
    let _ = fs::remove_file(temporary);
    result
}

fn read_remote_head(token: &str, game_id: &str) -> Result<Option<RemoteHead>, DriveFailure> {
    let Some(head_file) = find_single_by_properties(
        token,
        &[("kind", "head"), ("gameId", game_id)],
    )? else {
        return Ok(None);
    };
    let bytes = download_bytes(token, &head_file.id)?;
    let head = serde_json::from_slice::<RemoteHead>(&bytes).map_err(|error| DriveFailure {
        kind: DriveFailureKind::CorruptRemote,
        message: "Cloud Save head không đọc được; launcher chưa ghi đè dữ liệu local.".to_string(),
        retryable: false,
        retry_after_seconds: None,
        http_status: None,
        reason: error.to_string(),
    })?;
    if head.schema_version != REMOTE_SCHEMA || head.game_id != game_id {
        return Err(DriveFailure {
            kind: DriveFailureKind::CorruptRemote,
            message: "Cloud Save head không tương thích; launcher chưa ghi đè dữ liệu local.".to_string(),
            retryable: false,
            retry_after_seconds: None,
            http_status: None,
            reason: "remote head schema/game mismatch".to_string(),
        });
    }
    Ok(Some(head))
}

fn ensure_remote_parent_unchanged(
    token: &str,
    game_id: &str,
    expected_parent_snapshot_id: Option<&str>,
) -> Result<(), DriveFailure> {
    let current = read_remote_head(token, game_id)?;
    let actual = current.as_ref().map(|head| head.snapshot_id.as_str());
    if actual != expected_parent_snapshot_id {
        return Err(DriveFailure::remote_changed(
            expected_parent_snapshot_id,
            actual,
        ));
    }
    Ok(())
}

fn commit_head(token: &str, game_id: &str, head: &RemoteHead) -> Result<String, DriveFailure> {
    let bytes = serde_json::to_vec(head).map_err(|error| DriveFailure::other(error.to_string()))?;
    let temporary = std::env::temp_dir().join(format!("oxo-head-{}.json", sanitize_name(game_id)));
    fs::write(&temporary, &bytes).map_err(DriveFailure::offline)?;
    let existing = find_single_by_properties(token, &[("kind", "head"), ("gameId", game_id)])?;
    let mut metadata = serde_json::json!({
        "name": format!("oxo-head-{}.json", sanitize_name(game_id)),
        "mimeType": "application/json",
        "appProperties": {
            "kind": "head",
            "gameId": game_id,
            "snapshotId": head.snapshot_id,
            "schema": REMOTE_SCHEMA.to_string()
        }
    });
    if existing.is_none() {
        metadata["parents"] = serde_json::json!([APP_DATA_SPACE]);
    }
    let result = upload_file(
        token,
        existing.as_ref().map(|file| file.id.as_str()),
        &metadata,
        &temporary,
        bytes.len() as u64,
        "application/json",
    );
    let _ = fs::remove_file(temporary);
    result
}

fn write_device_record(token: &str, device_id: &str, device_name: &str) -> Result<String, DriveFailure> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": REMOTE_SCHEMA,
        "deviceId": device_id,
        "deviceName": device_name,
        "lastSeenAt": chrono::Utc::now().to_rfc3339()
    }))
    .map_err(|error| DriveFailure::other(error.to_string()))?;
    let temporary = std::env::temp_dir().join(format!("oxo-device-{device_id}.json"));
    fs::write(&temporary, &bytes).map_err(DriveFailure::offline)?;
    let existing = find_single_by_properties(token, &[("kind", "device"), ("deviceId", device_id)])?;
    let mut metadata = serde_json::json!({
        "name": format!("oxo-device-{device_id}.json"),
        "mimeType": "application/json",
        "appProperties": {
            "kind": "device",
            "deviceId": device_id,
            "schema": REMOTE_SCHEMA.to_string()
        }
    });
    if existing.is_none() {
        metadata["parents"] = serde_json::json!([APP_DATA_SPACE]);
    }
    let result = upload_file(
        token,
        existing.as_ref().map(|file| file.id.as_str()),
        &metadata,
        &temporary,
        bytes.len() as u64,
        "application/json",
    );
    let _ = fs::remove_file(temporary);
    result
}

fn upload_file(
    token: &str,
    existing_file_id: Option<&str>,
    metadata: &serde_json::Value,
    source: &Path,
    size: u64,
    mime_type: &str,
) -> Result<String, DriveFailure> {
    let method_url = if let Some(file_id) = existing_file_id {
        format!("{DRIVE_UPLOAD_API}/files/{file_id}?uploadType=resumable")
    } else {
        format!("{DRIVE_UPLOAD_API}/files?uploadType=resumable")
    };
    let session = send_with_backoff(|| {
        let client = http_client()?;
        let request = if existing_file_id.is_some() {
            client.patch(&method_url)
        } else {
            client.post(&method_url)
        };
        request
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json; charset=UTF-8")
            .header("X-Upload-Content-Type", mime_type)
            .header("X-Upload-Content-Length", size)
            .json(metadata)
            .send()
            .map_err(DriveFailure::offline)
    })?;
    let location = session
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| DriveFailure::other("Google Drive không trả resumable session URL."))?
        .to_string();

    if size == 0 {
        let response = http_client()?
            .put(&location)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_LENGTH, 0)
            .header("Content-Range", "bytes */0")
            .send()
            .map_err(DriveFailure::offline)?;
        return checked_json::<DriveFileCreated>(response).map(|file| file.id);
    }

    let mut file = File::open(source).map_err(DriveFailure::offline)?;
    let mut offset = 0u64;
    let mut attempts = 0u32;
    while offset < size {
        file.seek(SeekFrom::Start(offset)).map_err(DriveFailure::offline)?;
        let remaining = size.saturating_sub(offset);
        let chunk_len = remaining.min(RESUMABLE_CHUNK_BYTES as u64) as usize;
        let mut chunk = vec![0u8; chunk_len];
        file.read_exact(&mut chunk).map_err(DriveFailure::offline)?;
        let end = offset + chunk_len as u64 - 1;
        let upload = http_client()?
            .put(&location)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, mime_type)
            .header(CONTENT_LENGTH, chunk_len as u64)
            .header("Content-Range", format!("bytes {offset}-{end}/{size}"))
            .body(chunk)
            .send();

        match upload {
            Ok(response) if response.status().is_success() => {
                return Ok(checked_json::<DriveFileCreated>(response)?.id);
            }
            Ok(response) if response.status().as_u16() == 308 => {
                offset = committed_offset(&response).unwrap_or(end + 1).min(size);
                attempts = 0;
            }
            Ok(response) => {
                let failure = classify_response(response);
                if !failure.retryable || attempts >= MAX_RETRIES {
                    return Err(failure);
                }
                attempts += 1;
                if let Ok(state) = query_resumable_session(token, &location, size) {
                    match state {
                        ResumableSessionState::Complete(id) => return Ok(id),
                        ResumableSessionState::Incomplete(committed) => offset = committed,
                    }
                }
                thread::sleep(retry_delay(attempts, Some(&failure)));
            }
            Err(error) => {
                if attempts >= MAX_RETRIES {
                    return Err(DriveFailure::offline(error));
                }
                attempts += 1;
                if let Ok(state) = query_resumable_session(token, &location, size) {
                    match state {
                        ResumableSessionState::Complete(id) => return Ok(id),
                        ResumableSessionState::Incomplete(committed) => offset = committed,
                    }
                }
                thread::sleep(truncated_backoff(attempts));
            }
        }
    }
    Err(DriveFailure::other("Google Drive resumable upload ended without a file ID."))
}

enum ResumableSessionState {
    Complete(String),
    Incomplete(u64),
}

fn query_resumable_session(
    token: &str,
    location: &str,
    size: u64,
) -> Result<ResumableSessionState, DriveFailure> {
    let response = http_client()?
        .put(location)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_LENGTH, 0)
        .header("Content-Range", format!("bytes */{size}"))
        .send()
        .map_err(DriveFailure::offline)?;
    if response.status().is_success() {
        return Ok(ResumableSessionState::Complete(
            checked_json::<DriveFileCreated>(response)?.id,
        ));
    }
    if response.status().as_u16() == 308 {
        return Ok(ResumableSessionState::Incomplete(
            committed_offset(&response).unwrap_or(0).min(size),
        ));
    }
    Err(classify_response(response))
}

fn committed_offset(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit_once('-'))
        .and_then(|(_, end)| end.parse::<u64>().ok())
        .map(|end| end.saturating_add(1))
}

fn find_single_by_properties(
    token: &str,
    properties: &[(&str, &str)],
) -> Result<Option<DriveFile>, DriveFailure> {
    Ok(list_by_properties(token, properties)?
        .into_iter()
        .max_by(|left, right| left.modified_time.cmp(&right.modified_time)))
}

fn list_files_by_kind(token: &str, kind: &str) -> Result<Vec<DriveFile>, DriveFailure> {
    list_by_properties(token, &[("kind", kind)])
}

fn list_by_properties(
    token: &str,
    properties: &[(&str, &str)],
) -> Result<Vec<DriveFile>, DriveFailure> {
    let clauses = properties
        .iter()
        .map(|(key, value)| {
            format!(
                "appProperties has {{ key='{}' and value='{}' }}",
                escape_query(key),
                escape_query(value)
            )
        })
        .chain(std::iter::once("trashed = false".to_string()))
        .collect::<Vec<_>>()
        .join(" and ");
    let mut files = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let token_page = page_token.clone();
        let response = send_with_backoff(|| {
            let client = http_client()?;
            let mut request = client
                .get(format!("{DRIVE_API}/files"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .query(&[
                    ("spaces", APP_DATA_SPACE),
                    ("q", clauses.as_str()),
                    ("fields", "nextPageToken,files(id,name,size,modifiedTime,appProperties)"),
                    ("pageSize", "1000"),
                ]);
            if let Some(page) = token_page.as_deref() {
                request = request.query(&[("pageToken", page)]);
            }
            request.send().map_err(DriveFailure::offline)
        })?;
        let list = checked_json::<DriveFileList>(response)?;
        files.extend(list.files);
        page_token = list.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    Ok(files)
}

fn download_bytes(token: &str, file_id: &str) -> Result<Vec<u8>, DriveFailure> {
    let response = send_with_backoff(|| {
        http_client()?.get(format!("{DRIVE_API}/files/{file_id}?alt=media"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .map_err(DriveFailure::offline)
    })?;
    response.bytes().map(|value| value.to_vec()).map_err(DriveFailure::offline)
}

fn download_file(token: &str, file_id: &str, target: &Path) -> Result<(), DriveFailure> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(DriveFailure::offline)?;
    }
    let mut response = send_with_backoff(|| {
        http_client()?.get(format!("{DRIVE_API}/files/{file_id}?alt=media"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .map_err(DriveFailure::offline)
    })?;
    let mut output = File::create(target).map_err(DriveFailure::offline)?;
    response.copy_to(&mut output).map_err(DriveFailure::offline)?;
    output.sync_all().map_err(DriveFailure::offline)
}

fn delete_file(token: &str, file_id: &str) -> Result<(), DriveFailure> {
    let mut last_failure = None;
    for attempt in 0..=MAX_RETRIES {
        let response = http_client()?
            .delete(format!("{DRIVE_API}/files/{file_id}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send();
        match response {
            Ok(response) if response.status().is_success() || response.status().as_u16() == 404 => {
                return Ok(());
            }
            Ok(response) => {
                let failure = classify_response(response);
                if !failure.retryable || attempt == MAX_RETRIES {
                    return Err(failure);
                }
                last_failure = Some(failure);
            }
            Err(error) => {
                let failure = DriveFailure::offline(error);
                if attempt == MAX_RETRIES {
                    return Err(failure);
                }
                last_failure = Some(failure);
            }
        }
        thread::sleep(retry_delay(attempt, last_failure.as_ref()));
    }
    Err(last_failure.unwrap_or_else(|| DriveFailure::other("Google Drive delete failed")))
}

fn send_with_backoff<F>(mut operation: F) -> Result<Response, DriveFailure>
where
    F: FnMut() -> Result<Response, DriveFailure>,
{
    let mut last_failure = None;
    for attempt in 0..=MAX_RETRIES {
        match operation() {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let failure = classify_response(response);
                if !failure.retryable || attempt == MAX_RETRIES {
                    return Err(failure);
                }
                last_failure = Some(failure);
            }
            Err(failure) => {
                if !failure.retryable || attempt == MAX_RETRIES {
                    return Err(failure);
                }
                last_failure = Some(failure);
            }
        }
        thread::sleep(retry_delay(attempt, last_failure.as_ref()));
    }
    Err(last_failure.unwrap_or_else(|| DriveFailure::other("Google Drive request failed")))
}

fn classify_response(response: Response) -> DriveFailure {
    let status = response.status();
    let retry_after_seconds = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let body = response.text().unwrap_or_default();
    let parsed = serde_json::from_str::<GoogleErrorEnvelope>(&body).ok();
    let reason = parsed
        .as_ref()
        .and_then(|envelope| envelope.error.errors.first())
        .map(|error| error.reason.clone())
        .unwrap_or_default();
    let raw_message = parsed
        .as_ref()
        .map(|envelope| envelope.error.message.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| body.clone());

    let (kind, message, retryable) = match reason.as_str() {
        "storageQuotaExceeded" | "activeItemCreationLimitExceeded" => (
            DriveFailureKind::StorageFull,
            "Google Drive đã hết dung lượng. Save mới nhất vẫn an toàn trên máy này.".to_string(),
            true,
        ),
        "userRateLimitExceeded" | "rateLimitExceeded" | "sharingRateLimitExceeded" => (
            DriveFailureKind::RateLimited,
            "Google Drive đang giới hạn tạm thời. Launcher sẽ tự đồng bộ lại.".to_string(),
            true,
        ),
        "authError" | "invalidCredentials" => (
            DriveFailureKind::AuthRequired,
            "Google Drive cần được kết nối lại. Save local không bị ảnh hưởng.".to_string(),
            false,
        ),
        "notFound" => (
            DriveFailureKind::NotFound,
            "Cloud Save item không còn tồn tại trên Google Drive.".to_string(),
            false,
        ),
        "insufficientFilePermissions" | "forbidden" => (
            DriveFailureKind::PermissionDenied,
            "Google Drive không cho phép thao tác này. Hãy kết nối lại tài khoản.".to_string(),
            false,
        ),
        _ if status.as_u16() == 401 => (
            DriveFailureKind::AuthRequired,
            "Google Drive cần được kết nối lại. Save local không bị ảnh hưởng.".to_string(),
            false,
        ),
        _ if status.as_u16() == 404 => (
            DriveFailureKind::NotFound,
            "Cloud Save item không tồn tại.".to_string(),
            false,
        ),
        _ if status.as_u16() == 429 || status.is_server_error() => (
            DriveFailureKind::RateLimited,
            "Google Drive đang bận. Launcher sẽ tự thử lại.".to_string(),
            true,
        ),
        _ => (
            DriveFailureKind::Other,
            "Không thể hoàn tất thao tác Google Drive; save local vẫn an toàn.".to_string(),
            false,
        ),
    };
    DriveFailure {
        kind,
        message,
        retryable,
        retry_after_seconds,
        http_status: Some(status.as_u16()),
        reason: if reason.is_empty() { raw_message } else { format!("{reason}: {raw_message}") },
    }
}

fn access_token(app: &AppHandle) -> Result<String, DriveFailure> {
    let now = unix_seconds();
    if let Ok(cached) = token_cache().lock() {
        if let Some(token) = cached.as_ref().filter(|token| token.expires_at > now + 60) {
            return Ok(token.value.clone());
        }
    }
    let refresh_token = read_refresh_token(app).map_err(|error| DriveFailure {
        kind: DriveFailureKind::AuthRequired,
        message: "Google Drive chưa được kết nối.".to_string(),
        retryable: false,
        retry_after_seconds: None,
        http_status: None,
        reason: error,
    })?;
    let client_id = client_id();
    let response = http_client()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(DriveFailure::offline)?;
    let token = checked_json::<TokenResponse>(response)?;
    cache_access_token(token.access_token.clone(), token.expires_in);
    Ok(token.access_token)
}

fn checked_json<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, DriveFailure> {
    if !response.status().is_success() {
        return Err(classify_response(response));
    }
    response.json::<T>().map_err(|error| DriveFailure {
        kind: DriveFailureKind::CorruptRemote,
        message: "Google Drive trả dữ liệu không đọc được.".to_string(),
        retryable: false,
        retry_after_seconds: None,
        http_status: None,
        reason: error.to_string(),
    })
}

fn http_client() -> Result<Client, DriveFailure> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .user_agent("0xoLemon-GoogleDrive/0.2")
        .build()
        .map_err(DriveFailure::offline)
}

fn replace_tree_atomic(staging: &Path, destination: &Path) -> Result<(), String> {
    let backup = destination.with_extension("0xo-drive-backup");
    if backup.exists() {
        clear_tree(&backup)?;
    }
    if destination.exists() {
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    match fs::rename(staging, destination) {
        Ok(()) => {
            if backup.exists() {
                clear_tree(&backup)?;
            }
            Ok(())
        }
        Err(error) => {
            if backup.exists() {
                let _ = fs::rename(&backup, destination);
            }
            Err(error.to_string())
        }
    }
}

fn clear_tree(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = walkdir::WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.depth()));
    for entry in entries {
        if entry.path() == root {
            continue;
        }
        if entry.file_type().is_dir() {
            fs::remove_dir(entry.path()).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    fs::remove_dir(root).map_err(|error| error.to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn parse_u64(value: &str) -> u64 {
    value.parse::<u64>().unwrap_or(0)
}

fn parse_u64_opt(value: Option<&str>) -> Option<u64> {
    value.and_then(|value| value.parse::<u64>().ok())
}

fn escape_query(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn client_id() -> String {
    std::env::var("OXO_GOOGLE_DRIVE_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
        .trim()
        .to_string()
}

fn auth_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(AUTH_FILE))
}

fn random_urlsafe(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect()
}

fn open_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err("Opening the system browser is not supported on this platform".to_string())
}

fn write_refresh_token(app: &AppHandle, token: &str) -> Result<(), String> {
    let encrypted = protect_secret(token.as_bytes())?;
    let stored = StoredAuth {
        client_id: client_id(),
        encrypted_refresh_token: STANDARD.encode(encrypted),
    };
    let path = auth_path(app)?;
    let temporary = path.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&stored).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    replace_file_with_rollback(&temporary, &path)
}

fn read_refresh_token(app: &AppHandle) -> Result<String, String> {
    let stored = read_stored_auth(app)?;
    let encrypted = STANDARD
        .decode(stored.encrypted_refresh_token)
        .map_err(|error| error.to_string())?;
    String::from_utf8(unprotect_secret(&encrypted)?).map_err(|error| error.to_string())
}

fn read_stored_auth(app: &AppHandle) -> Result<StoredAuth, String> {
    serde_json::from_slice(&fs::read(auth_path(app)?).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_truncated() {
        assert!(truncated_backoff(0) >= Duration::from_secs(1));
        assert!(truncated_backoff(99) <= Duration::from_secs(65));
    }

    #[test]
    fn error_reason_tokens_remain_distinct() {
        assert_ne!("storageQuotaExceeded", "userRateLimitExceeded");
        assert_ne!("userRateLimitExceeded", "rateLimitExceeded");
    }

    #[test]
    fn object_names_are_content_addressed() {
        let hash = hex_sha256(b"save-data");
        assert_eq!(hash.len(), 64);
        assert!(format!("oxo-object-{hash}.bin").contains(&hash));
    }
}
