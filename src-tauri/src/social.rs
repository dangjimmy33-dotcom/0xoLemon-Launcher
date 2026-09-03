use std::{
    fs,
    io::{BufRead, BufReader, Cursor},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use image::{imageops::FilterType, ImageReader, Limits};
use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    Method, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::managed_file_transaction::{
    ManagedDeleteSpec, ManagedFileChange, ManagedFileSpec,
};

const DEFAULT_SOCIAL_API_BASE: &str =
    "https://zeroxolemon-launcher.onrender.com/api/0xolemon/social";
const MAX_SOURCE_BYTES: u64 = 24 * 1024 * 1024;
const MAX_COVER_BYTES: usize = 256 * 1024;
const COVER_WIDTH: u32 = 1920;
const COVER_HEIGHT: u32 = 640;
const RETRY_INTERVAL: Duration = Duration::from_secs(10 * 60);

static SSE_STARTED: AtomicBool = AtomicBool::new(false);
static COVER_WORKER_STARTED: AtomicBool = AtomicBool::new(false);
static COVER_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn cover_operation_lock() -> &'static Mutex<()> {
    COVER_OPERATION_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialActivity {
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde(default)]
    pub since_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialStats {
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub play_minutes: u64,
    #[serde(default)]
    pub online_minutes: u64,
    #[serde(default)]
    pub games_played: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialProfile {
    pub id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub custom_status: String,
    #[serde(default)]
    pub accent: String,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub cover_hash: Option<String>,
    #[serde(default)]
    pub cover_revision: u64,
    #[serde(default = "default_cover_position")]
    pub cover_position: f64,
    #[serde(default)]
    pub presence: String,
    #[serde(default)]
    pub activity: SocialActivity,
    #[serde(default)]
    pub relationship: String,
    #[serde(default)]
    pub leaderboard_opt_in: bool,
    #[serde(default)]
    pub stats: SocialStats,
    #[serde(default)]
    pub joined_at: String,
}

fn default_cover_position() -> f64 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialBootstrap {
    pub server_time: String,
    pub presence_heartbeat_ms: u64,
    pub presence_stale_ms: u64,
    pub self_profile: SocialProfile,
    #[serde(default)]
    pub members: Vec<SocialProfile>,
    #[serde(default)]
    pub canary: bool,
    #[serde(default)]
    pub cover_batch_ms: u64,
    #[serde(default)]
    pub offline_snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialSearchResult {
    #[serde(default)]
    pub items: Vec<SocialProfile>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSocialProfileRequest {
    pub bio: String,
    pub custom_status: String,
    pub accent: String,
    pub cover_position: f64,
    pub appear_offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialPresenceRequest {
    pub session_id: String,
    pub state: String,
    #[serde(default)]
    pub activity_label: String,
    #[serde(default)]
    pub activity_detail: String,
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde(default)]
    pub since_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialStatEventRequest {
    pub event_id: String,
    pub kind: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub play_minutes: u64,
    #[serde(default)]
    pub online_minutes: u64,
    #[serde(default)]
    pub games_played: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SocialCoverState {
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub local_hash: Option<String>,
    #[serde(default)]
    pub published_hash: Option<String>,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub queued_at: Option<String>,
    #[serde(default)]
    pub publish_by: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub public_upload_confirmed: bool,
    #[serde(default)]
    pub publish_state: CoverPublishState,
    #[serde(default)]
    pub removed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CoverPublishState {
    #[default]
    SavedLocally,
    PendingPublish,
    Published,
    PublishFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCoverDraftState {
    pub draft_id: String,
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub byte_length: usize,
    pub public_upload_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SocialCoverDraftRecord {
    draft_id: String,
    sha256: String,
    mime: String,
    width: u32,
    height: u32,
    byte_length: usize,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum SocialCoverDraftOperation {
    Keep,
    Replace {
        sha256: String,
        mime: String,
        width: u32,
        height: u32,
    },
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialProfileDraftInput {
    pub draft_id: String,
    pub bio: String,
    pub status: String,
    pub accent: String,
    pub cover_position: f64,
    pub cover: SocialCoverDraftOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalSocialProfile {
    schema_version: u32,
    bio: String,
    status: String,
    accent: String,
    cover_position: f64,
    cover_hash: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentCover {
    hash: String,
    width: u32,
    height: u32,
    source: String,
    committed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemovedCoverTombstone {
    removed_at: String,
    transaction_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingCoverRemoval {
    queued_at: String,
    attempts: u32,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingCover {
    hash: String,
    local_path: String,
    queued_at: String,
    #[serde(default)]
    publish_by: Option<String>,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedCover {
    hash: String,
    published_at: String,
    #[serde(default)]
    revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SocialEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    created_at: String,
    payload: Value,
}

fn social_api_base() -> String {
    std::env::var("OXO_SOCIAL_API_BASE")
        .ok()
        .filter(|value| {
            value.starts_with("https://")
                || cfg!(debug_assertions) && value.starts_with("http://127.0.0.1:")
        })
        .unwrap_or_else(|| DEFAULT_SOCIAL_API_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn social_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join("social");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare social cache: {error}"))?;
    Ok(directory)
}

fn bootstrap_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("bootstrap-cache.json"))
}
fn pending_cover_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("cover-pending.json"))
}
fn pending_cover_removal_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("cover-remove-pending.json"))
}
fn published_cover_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("cover-published.json"))
}
fn local_cover_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("profile-cover.webp"))
}
fn original_cover_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("profile-cover-original.bin"))
}
fn current_cover_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("profile-cover-current.json"))
}
fn canonical_profile_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("profile.json"))
}
fn cover_removed_tombstone_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("profile-cover-removed.json"))
}
fn cover_consent_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(social_dir(app)?.join("public-cover-consent.json"))
}

fn cover_drafts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = social_dir(app)?.join("cover-drafts");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("COVER_DRAFT_PREPARE_FAILED: {error}"))?;
    Ok(directory)
}

fn validated_draft_id(draft_id: &str) -> Result<String, String> {
    Uuid::parse_str(draft_id)
        .map(|value| value.to_string())
        .map_err(|_| "COVER_DRAFT_INVALID: draftId must be a UUID.".to_string())
}

fn cover_draft_paths(app: &AppHandle, draft_id: &str) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let draft_id = validated_draft_id(draft_id)?;
    let root = cover_drafts_dir(app)?;
    Ok((
        root.join(format!("{draft_id}.webp")),
        root.join(format!("{draft_id}.original.bin")),
        root.join(format!("{draft_id}.json")),
    ))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    crate::lua_live::atomic_write_path(path, &bytes)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn http_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .user_agent(concat!("0xoLemon/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())
}

fn authorized_request(
    app: &AppHandle,
    client: &Client,
    method: Method,
    path: &str,
) -> Result<RequestBuilder, String> {
    let token = crate::discord_auth::access_token_for_backend(app)?;
    Ok(client
        .request(method, format!("{}{}", social_api_base(), path))
        .bearer_auth(token))
}

fn response_error(response: Response) -> String {
    let status = response.status();
    let payload = response.json::<Value>().unwrap_or(Value::Null);
    let code = payload
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("SOCIAL_REQUEST_FAILED");
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Social service request failed.");
    format!("{code}: {message} (HTTP {})", status.as_u16())
}

fn send_json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, String> {
    let response = request
        .send()
        .map_err(|error| format!("SOCIAL_NETWORK_ERROR: {error}"))?;
    if !response.status().is_success() {
        return Err(response_error(response));
    }
    response
        .json::<T>()
        .map_err(|error| format!("SOCIAL_RESPONSE_INVALID: {error}"))
}

fn send_empty(request: RequestBuilder) -> Result<Value, String> {
    send_json(request)
}

fn reconcile_pending_cover(app: &AppHandle, bootstrap: &SocialBootstrap) -> Result<(), String> {
    let path = pending_cover_path(app)?;
    if let Some(hash) = bootstrap.self_profile.cover_hash.as_deref() {
        write_json(
            &published_cover_path(app)?,
            &PublishedCover {
                hash: hash.to_string(),
                published_at: chrono::Utc::now().to_rfc3339(),
                revision: bootstrap.self_profile.cover_revision,
            },
        )?;
        if let Ok(pending) = read_json::<PendingCover>(&path) {
            if pending.hash == hash && path.exists() {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

fn reconcile_published_cover_event(app: &AppHandle, event: &SocialEvent) -> Result<(), String> {
    let bootstrap = read_json::<SocialBootstrap>(&bootstrap_cache_path(app)?)?;
    if event.payload.get("userId").and_then(Value::as_str)
        != Some(bootstrap.self_profile.id.as_str())
    {
        return Ok(());
    }

    if event.event_type == "cover.removed" {
        let published = published_cover_path(app)?;
        if published.exists() {
            fs::remove_file(published).map_err(|error| error.to_string())?;
        }
        exact_remove(&pending_cover_removal_path(app)?)?;
        return Ok(());
    }

    if event.event_type != "cover.published" {
        return Ok(());
    }
    let Some(hash) = event.payload.get("hash").and_then(Value::as_str) else {
        return Ok(());
    };
    let revision = event
        .payload
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    write_json(
        &published_cover_path(app)?,
        &PublishedCover {
            hash: hash.to_string(),
            published_at: chrono::Utc::now().to_rfc3339(),
            revision,
        },
    )?;
    let pending_path = pending_cover_path(app)?;
    if let Ok(pending) = read_json::<PendingCover>(&pending_path) {
        if pending.hash == hash && pending_path.exists() {
            fs::remove_file(pending_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn fetch_bootstrap(app: &AppHandle) -> Result<SocialBootstrap, String> {
    let client = http_client(Duration::from_secs(20))?;
    let mut value: Value = send_json(authorized_request(app, &client, Method::GET, "/bootstrap")?)?;
    if let Some(object) = value.as_object_mut() {
        if let Some(self_value) = object.remove("self") {
            object.insert("selfProfile".to_string(), self_value);
        }
        object.insert("offlineSnapshot".to_string(), Value::Bool(false));
    }
    let bootstrap: SocialBootstrap = serde_json::from_value(value)
        .map_err(|error| format!("SOCIAL_RESPONSE_INVALID: {error}"))?;
    write_json(&bootstrap_cache_path(app)?, &bootstrap)?;
    reconcile_pending_cover(app, &bootstrap)?;
    Ok(bootstrap)
}

fn cached_bootstrap(app: &AppHandle) -> Result<SocialBootstrap, String> {
    let mut snapshot: SocialBootstrap = read_json(&bootstrap_cache_path(app)?)?;
    snapshot.offline_snapshot = true;
    for member in &mut snapshot.members {
        member.presence = "offline".to_string();
        member.activity = SocialActivity {
            kind: "none".to_string(),
            label: "Offline".to_string(),
            ..Default::default()
        };
    }
    snapshot.self_profile.presence = "offline".to_string();
    Ok(snapshot)
}

#[tauri::command]
pub async fn get_social_bootstrap(app: AppHandle) -> Result<SocialBootstrap, String> {
    start_social_event_stream(app.clone());
    start_cover_retry_worker(app.clone());
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        fetch_bootstrap(&worker_app)
            .or_else(|network_error| cached_bootstrap(&worker_app).map_err(|_| network_error))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn search_social_users(
    app: AppHandle,
    query: String,
    cursor: Option<String>,
) -> Result<SocialSearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(20))?;
        let mut url = reqwest::Url::parse(&format!("{}/users", social_api_base()))
            .map_err(|error| error.to_string())?;
        url.query_pairs_mut()
            .append_pair("query", query.trim())
            .append_pair("limit", "24");
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            url.query_pairs_mut().append_pair("cursor", &cursor);
        }
        let token = crate::discord_auth::access_token_for_backend(&app)?;
        send_json(client.get(url).bearer_auth(token))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_social_profile(app: AppHandle, user_id: String) -> Result<SocialProfile, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !user_id.chars().all(|character| character.is_ascii_digit()) {
            return Err("INVALID_USER: Discord ID is invalid.".to_string());
        }
        let client = http_client(Duration::from_secs(20))?;
        send_json(authorized_request(
            &app,
            &client,
            Method::GET,
            &format!("/users/{user_id}"),
        )?)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn update_social_profile(
    app: AppHandle,
    input: UpdateSocialProfileRequest,
) -> Result<SocialProfile, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(20))?;
        send_json(authorized_request(&app, &client, Method::PATCH, "/profile")?.json(&input))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn relationship_action(
    app: &AppHandle,
    method: Method,
    path: String,
    request_id: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    Uuid::parse_str(request_id)
        .map_err(|_| "INVALID_REQUEST_ID: A UUID request ID is required.".to_string())?;
    let client = http_client(Duration::from_secs(20))?;
    let request =
        authorized_request(app, &client, method, &path)?.header("x-request-id", request_id);
    send_empty(match body {
        Some(body) => request.json(&body),
        None => request,
    })
}

macro_rules! relationship_command {
    ($name:ident, $method:expr, $path:expr) => {
        #[tauri::command]
        pub async fn $name(
            app: AppHandle,
            user_id: String,
            request_id: String,
        ) -> Result<Value, String> {
            tauri::async_runtime::spawn_blocking(move || {
                relationship_action(&app, $method, ($path)(&user_id), &request_id, None)
            })
            .await
            .map_err(|error| error.to_string())?
        }
    };
}

#[tauri::command]
pub async fn send_social_friend_request(
    app: AppHandle,
    user_id: String,
    request_id: String,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        relationship_action(
            &app,
            Method::POST,
            "/friend-requests".to_string(),
            &request_id,
            Some(serde_json::json!({ "targetUserId": user_id })),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

relationship_command!(
    accept_social_friend_request,
    Method::POST,
    |id: &str| format!("/friend-requests/{id}/accept")
);
relationship_command!(
    cancel_social_friend_request,
    Method::DELETE,
    |id: &str| format!("/friend-requests/{id}?action=cancel")
);
relationship_command!(
    decline_social_friend_request,
    Method::DELETE,
    |id: &str| format!("/friend-requests/{id}?action=decline")
);
relationship_command!(remove_social_friend, Method::DELETE, |id: &str| format!(
    "/friends/{id}"
));
relationship_command!(block_social_user, Method::PUT, |id: &str| format!(
    "/blocks/{id}"
));
relationship_command!(unblock_social_user, Method::DELETE, |id: &str| format!(
    "/blocks/{id}"
));

#[tauri::command]
pub async fn update_social_presence(
    app: AppHandle,
    input: SocialPresenceRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(15))?;
        send_json(authorized_request(&app, &client, Method::POST, "/presence")?.json(&input))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_social_leaderboard(
    app: AppHandle,
    metric: String,
    period: String,
    scope: String,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(20))?;
        let mut url = reqwest::Url::parse(&format!("{}/leaderboard", social_api_base()))
            .map_err(|error| error.to_string())?;
        url.query_pairs_mut()
            .append_pair("metric", &metric)
            .append_pair("period", &period)
            .append_pair("scope", &scope);
        let token = crate::discord_auth::access_token_for_backend(&app)?;
        send_json(client.get(url).bearer_auth(token))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_social_leaderboard_participation(
    app: AppHandle,
    enabled: bool,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(15))?;
        send_json(
            authorized_request(&app, &client, Method::PUT, "/leaderboard/participation")?
                .json(&serde_json::json!({ "enabled": enabled })),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn record_social_stats(
    app: AppHandle,
    input: SocialStatEventRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = http_client(Duration::from_secs(15))?;
        send_json(authorized_request(&app, &client, Method::POST, "/stats/events")?.json(&input))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn encode_cover_bytes(source: &[u8]) -> Result<Vec<u8>, String> {
    if source.is_empty() || source.len() as u64 > MAX_SOURCE_BYTES {
        return Err("COVER_SOURCE_TOO_LARGE: Select an image smaller than 24 MiB.".to_string());
    }
    let mut reader = ImageReader::new(Cursor::new(source))
        .with_guessed_format()
        .map_err(|error| format!("COVER_FORMAT_INVALID: {error}"))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("COVER_DECODE_FAILED: {error}"))?;
    let resized = image
        .resize_to_fill(COVER_WIDTH, COVER_HEIGHT, FilterType::Lanczos3)
        .to_rgba8();
    for quality in [82.0, 74.0, 66.0, 58.0, 50.0, 44.0] {
        let encoded = webp::Encoder::from_rgba(resized.as_raw(), COVER_WIDTH, COVER_HEIGHT)
            .encode(quality)
            .to_vec();
        if encoded.len() <= MAX_COVER_BYTES {
            return Ok(encoded);
        }
    }
    Err("COVER_ENCODE_TOO_LARGE: This image cannot be encoded below 256 KiB safely.".to_string())
}

fn encode_cover(source: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let metadata = fs::metadata(source).map_err(|error| format!("COVER_READ_FAILED: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
        return Err("COVER_SOURCE_TOO_LARGE: Select an image smaller than 24 MiB.".to_string());
    }
    let original = fs::read(source).map_err(|error| format!("COVER_READ_FAILED: {error}"))?;
    let encoded = encode_cover_bytes(&original)?;
    Ok((original, encoded))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn exact_remove(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("COVER_LOCAL_REMOVE_FAILED: {error}"))?;
    }
    Ok(())
}

fn write_transaction_json<T: Serialize>(
    root: &Path,
    transaction_id: &str,
    label: &str,
    value: &T,
) -> Result<PathBuf, String> {
    let path = root.join(format!("{transaction_id}.{label}.json"));
    write_json(&path, value)?;
    Ok(path)
}

fn replace_change(source: PathBuf, target: PathBuf, source_root: &Path, target_root: &Path) -> Result<ManagedFileChange, String> {
    let expected_sha256 = sha256_bytes(
        &fs::read(&source).map_err(|error| format!("COVER_STAGE_READ_FAILED: {error}"))?,
    );
    Ok(ManagedFileChange::Replace(ManagedFileSpec {
        source,
        target,
        allowed_source_root: source_root.to_path_buf(),
        allowed_target_root: target_root.to_path_buf(),
        expected_sha256,
    }))
}

fn delete_change(target: PathBuf, target_root: &Path) -> ManagedFileChange {
    ManagedFileChange::Delete(ManagedDeleteSpec {
        target,
        allowed_target_root: target_root.to_path_buf(),
    })
}

fn upload_pending_cover(app: &AppHandle) -> Result<SocialCoverState, String> {
    let pending_path = pending_cover_path(app)?;
    let mut pending: PendingCover = read_json(&pending_path)?;
    let bytes =
        fs::read(&pending.local_path).map_err(|error| format!("COVER_READ_FAILED: {error}"))?;
    let client = http_client(Duration::from_secs(30))?;
    let result = (|| {
        let response = authorized_request(app, &client, Method::PUT, "/profile/cover")?
            .header(reqwest::header::CONTENT_TYPE, "image/webp")
            .header("x-cover-sha256", &pending.hash)
            .body(bytes)
            .send()
            .map_err(|error| format!("SOCIAL_NETWORK_ERROR: {error}"))?;
        if !response.status().is_success() {
            return Err(response_error(response));
        }
        let body: Value = response
            .json()
            .map_err(|error| format!("SOCIAL_RESPONSE_INVALID: {error}"))?;
        pending.publish_by = body
            .get("publishBy")
            .and_then(Value::as_str)
            .map(str::to_string);
        pending.last_error = None;
        write_json(&pending_path, &pending)?;
        Ok(SocialCoverState {
            local_path: Some(pending.local_path.clone()),
            local_hash: Some(pending.hash.clone()),
            published_hash: body
                .get("previousPublishedHash")
                .and_then(Value::as_str)
                .map(str::to_string),
            pending: true,
            queued_at: Some(pending.queued_at.clone()),
            publish_by: pending.publish_by.clone(),
            last_error: None,
            public_upload_confirmed: cover_consent_path(app).is_ok_and(|path| path.exists()),
            publish_state: CoverPublishState::PendingPublish,
            removed: false,
        })
    })();
    if let Err(error) = &result {
        pending.attempts = pending.attempts.saturating_add(1);
        pending.last_error = Some(error.clone());
        let _ = write_json(&pending_path, &pending);
    }
    result
}

fn upload_pending_cover_removal(app: &AppHandle) -> Result<SocialCoverState, String> {
    let pending_path = pending_cover_removal_path(app)?;
    let mut pending: PendingCoverRemoval = read_json(&pending_path)?;
    let client = http_client(Duration::from_secs(20))?;
    let result = (|| {
        let response = authorized_request(app, &client, Method::DELETE, "/profile/cover")?
            .send()
            .map_err(|error| format!("SOCIAL_NETWORK_ERROR: {error}"))?;
        if !response.status().is_success() {
            return Err(response_error(response));
        }
        exact_remove(&pending_path)?;
        exact_remove(&published_cover_path(app)?)?;
        get_social_cover_state_inner(app)
    })();
    if let Err(error) = &result {
        pending.attempts = pending.attempts.saturating_add(1);
        pending.last_error = Some(error.clone());
        let _ = write_json(&pending_path, &pending);
    }
    result
}

fn get_social_cover_state_inner(app: &AppHandle) -> Result<SocialCoverState, String> {
    let local = local_cover_path(&app)?;
    let pending = read_json::<PendingCover>(&pending_cover_path(&app)?).ok();
    let pending_removal = read_json::<PendingCoverRemoval>(&pending_cover_removal_path(app)?).ok();
    let published = read_json::<PublishedCover>(&published_cover_path(&app)?).ok();
    let current = read_json::<CurrentCover>(&current_cover_path(app)?).ok();
    let published_hash = published.as_ref().map(|item| item.hash.clone());
    let local_hash = current
        .as_ref()
        .map(|item| item.hash.clone())
        .or_else(|| pending.as_ref().map(|item| item.hash.clone()))
        .or_else(|| {
            local.exists()
                .then(|| fs::read(&local).ok().map(|bytes| sha256_bytes(&bytes)))
                .flatten()
        });
    let removed = cover_removed_tombstone_path(app)?.exists();
    let last_error = pending
        .as_ref()
        .and_then(|item| item.last_error.clone())
        .or_else(|| pending_removal.as_ref().and_then(|item| item.last_error.clone()));
    let is_pending = pending.is_some() || pending_removal.is_some();
    let publish_state = if last_error.is_some() {
        CoverPublishState::PublishFailed
    } else if is_pending {
        CoverPublishState::PendingPublish
    } else if removed || local_hash.as_ref().is_some_and(|hash| published_hash.as_ref() == Some(hash)) {
        CoverPublishState::Published
    } else {
        CoverPublishState::SavedLocally
    };
    Ok(SocialCoverState {
        local_path: local.exists().then(|| local.to_string_lossy().to_string()),
        local_hash,
        published_hash,
        pending: is_pending,
        queued_at: pending
            .as_ref()
            .map(|item| item.queued_at.clone())
            .or_else(|| pending_removal.as_ref().map(|item| item.queued_at.clone())),
        publish_by: pending.as_ref().and_then(|item| item.publish_by.clone()),
        last_error,
        public_upload_confirmed: cover_consent_path(&app).is_ok_and(|path| path.exists()),
        publish_state,
        removed,
    })
}

fn emit_cover_state(app: &AppHandle) {
    if let Ok(state) = get_social_cover_state_inner(app) {
        let _ = app.emit("social://cover-state", state);
    }
}

fn log_cover_publish_result(label: &str, result: &Result<SocialCoverState, String>) {
    if let Err(error) = result {
        eprintln!("[social-cover] {label} failed: {error}");
    }
}

#[tauri::command]
pub async fn stage_social_cover(
    app: AppHandle,
    public_upload_confirmed: bool,
) -> Result<Option<SocialCoverDraftState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let consent_path = cover_consent_path(&app)?;
        let consented = consent_path.exists();
        if !consented && !public_upload_confirmed {
            return Err("PUBLIC_COVER_CONSENT_REQUIRED: Profile covers are published publicly. Confirm before continuing.".to_string());
        }
        if !consented {
            write_json(&consent_path, &serde_json::json!({ "confirmed": true }))?;
        }
        let selected = app
            .dialog()
            .file()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"])
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source = selected
            .into_path()
            .map_err(|error| format!("COVER_PATH_INVALID: {error}"))?;
        let (original, encoded) = encode_cover(&source)?;
        let draft_id = Uuid::new_v4().to_string();
        let hash = sha256_bytes(&encoded);
        let (cover_path, original_path, record_path) = cover_draft_paths(&app, &draft_id)?;
        let record = SocialCoverDraftRecord {
            draft_id: draft_id.clone(),
            sha256: hash.clone(),
            mime: "image/webp".to_string(),
            width: COVER_WIDTH,
            height: COVER_HEIGHT,
            byte_length: encoded.len(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let write_result = (|| {
            crate::lua_live::atomic_write_path(&cover_path, &encoded)?;
            crate::lua_live::atomic_write_path(&original_path, &original)?;
            write_json(&record_path, &record)
        })();
        if let Err(error) = write_result {
            let _ = exact_remove(&cover_path);
            let _ = exact_remove(&original_path);
            let _ = exact_remove(&record_path);
            return Err(error);
        }
        Ok(Some(SocialCoverDraftState {
            draft_id,
            sha256: hash,
            mime: record.mime,
            width: record.width,
            height: record.height,
            byte_length: record.byte_length,
            public_upload_confirmed: true,
        }))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn read_social_cover_draft_preview(app: AppHandle, draft_id: String) -> Result<Vec<u8>, String> {
    let (path, _, _) = cover_draft_paths(&app, &draft_id)?;
    if !path.exists() {
        return Err("COVER_DRAFT_NOT_FOUND: Select the cover again.".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("COVER_READ_FAILED: {error}"))?;
    if bytes.len() > MAX_COVER_BYTES {
        return Err("COVER_DRAFT_INVALID: Draft preview exceeds the cover limit.".to_string());
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn commit_social_profile_draft(
    app: AppHandle,
    draft: SocialProfileDraftInput,
) -> Result<SocialCoverState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if draft.bio.chars().count() > 480
            || draft.status.chars().count() > 120
            || draft.accent.chars().count() > 64
            || !draft.cover_position.is_finite()
            || !(0.0..=100.0).contains(&draft.cover_position)
        {
            return Err("SOCIAL_PROFILE_DRAFT_INVALID: Profile fields exceed their limits.".to_string());
        }

        let _guard = cover_operation_lock()
            .lock()
            .map_err(|_| "COVER_OPERATION_LOCK_POISONED".to_string())?;
        let operation_id = Uuid::new_v4().to_string();
        let source_root = cover_drafts_dir(&app)?;
        let target_root = social_dir(&app)?;
        let mut changes = Vec::new();
        let mut temporary_paths = Vec::new();
        let mut committed_draft_paths: Option<(PathBuf, PathBuf, PathBuf)> = None;
        let now = chrono::Utc::now().to_rfc3339();

        let cover_hash = match &draft.cover {
            SocialCoverDraftOperation::Keep => read_json::<CurrentCover>(&current_cover_path(&app)?)
                .ok()
                .map(|current| current.hash),
            SocialCoverDraftOperation::Replace {
                sha256,
                mime,
                width,
                height,
            } => {
                let draft_id = validated_draft_id(&draft.draft_id)?;
                let paths = cover_draft_paths(&app, &draft_id)?;
                let record: SocialCoverDraftRecord = read_json(&paths.2)
                    .map_err(|_| "COVER_DRAFT_NOT_FOUND: Select the cover again.".to_string())?;
                let actual_hash = sha256_bytes(
                    &fs::read(&paths.0).map_err(|error| format!("COVER_DRAFT_READ_FAILED: {error}"))?,
                );
                if !record.sha256.eq_ignore_ascii_case(sha256)
                    || !actual_hash.eq_ignore_ascii_case(sha256)
                    || mime != "image/webp"
                    || record.mime != *mime
                    || *width != COVER_WIDTH
                    || *height != COVER_HEIGHT
                    || record.width != *width
                    || record.height != *height
                {
                    return Err("COVER_DRAFT_DRIFTED: Draft metadata or bytes changed after selection.".to_string());
                }
                let current = CurrentCover {
                    hash: actual_hash.clone(),
                    width: COVER_WIDTH,
                    height: COVER_HEIGHT,
                    source: "selected".to_string(),
                    committed_at: now.clone(),
                };
                let pending = PendingCover {
                    hash: actual_hash.clone(),
                    local_path: local_cover_path(&app)?.to_string_lossy().to_string(),
                    queued_at: now.clone(),
                    publish_by: None,
                    attempts: 0,
                    last_error: None,
                };
                let current_stage = write_transaction_json(&source_root, &operation_id, "current", &current)?;
                let pending_stage = write_transaction_json(&source_root, &operation_id, "pending", &pending)?;
                temporary_paths.extend([current_stage.clone(), pending_stage.clone()]);
                changes.push(replace_change(paths.0.clone(), local_cover_path(&app)?, &source_root, &target_root)?);
                changes.push(replace_change(paths.1.clone(), original_cover_path(&app)?, &source_root, &target_root)?);
                changes.push(replace_change(current_stage, current_cover_path(&app)?, &source_root, &target_root)?);
                changes.push(replace_change(pending_stage, pending_cover_path(&app)?, &source_root, &target_root)?);
                changes.push(delete_change(cover_removed_tombstone_path(&app)?, &target_root));
                changes.push(delete_change(pending_cover_removal_path(&app)?, &target_root));
                committed_draft_paths = Some(paths);
                Some(actual_hash)
            }
            SocialCoverDraftOperation::Remove => {
                let tombstone = RemovedCoverTombstone {
                    removed_at: now.clone(),
                    transaction_id: operation_id.clone(),
                };
                let pending = PendingCoverRemoval {
                    queued_at: now.clone(),
                    attempts: 0,
                    last_error: None,
                };
                let tombstone_stage = write_transaction_json(&source_root, &operation_id, "removed", &tombstone)?;
                let pending_stage = write_transaction_json(&source_root, &operation_id, "remove-pending", &pending)?;
                temporary_paths.extend([tombstone_stage.clone(), pending_stage.clone()]);
                changes.push(replace_change(tombstone_stage, cover_removed_tombstone_path(&app)?, &source_root, &target_root)?);
                changes.push(replace_change(pending_stage, pending_cover_removal_path(&app)?, &source_root, &target_root)?);
                for target in [
                    local_cover_path(&app)?,
                    original_cover_path(&app)?,
                    current_cover_path(&app)?,
                    pending_cover_path(&app)?,
                    published_cover_path(&app)?,
                ] {
                    changes.push(delete_change(target, &target_root));
                }
                None
            }
        };

        let canonical_profile = CanonicalSocialProfile {
            schema_version: 2,
            bio: draft.bio,
            status: draft.status,
            accent: draft.accent,
            cover_position: draft.cover_position,
            cover_hash,
            updated_at: now,
        };
        let profile_stage = write_transaction_json(&source_root, &operation_id, "profile", &canonical_profile)?;
        temporary_paths.push(profile_stage.clone());
        changes.push(replace_change(profile_stage, canonical_profile_path(&app)?, &source_root, &target_root)?);

        let commit_result = crate::managed_file_transaction::apply_changes(
            &app,
            "social-profile-draft",
            changes,
        );
        for path in &temporary_paths {
            let _ = exact_remove(path);
        }
        commit_result?;
        if let Some((cover, original, record)) = committed_draft_paths {
            exact_remove(&cover)?;
            exact_remove(&original)?;
            exact_remove(&record)?;
        }

        // Publishing is best-effort: the canonical local transaction is the Save
        // commit point, while network publication may be retried independently.
        let publish_app = app.clone();
        match draft.cover {
            SocialCoverDraftOperation::Replace { .. } => {
                tauri::async_runtime::spawn_blocking(move || {
                    let _guard = cover_operation_lock().lock().ok();
                    let result = upload_pending_cover(&publish_app);
                    log_cover_publish_result("initial publish", &result);
                    emit_cover_state(&publish_app);
                });
            }
            SocialCoverDraftOperation::Remove => {
                tauri::async_runtime::spawn_blocking(move || {
                    let _guard = cover_operation_lock().lock().ok();
                    let result = upload_pending_cover_removal(&publish_app);
                    log_cover_publish_result("initial removal publish", &result);
                    emit_cover_state(&publish_app);
                });
            }
            SocialCoverDraftOperation::Keep => {}
        }
        get_social_cover_state_inner(&app)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn discard_social_profile_draft(app: AppHandle, draft_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cover_operation_lock()
            .lock()
            .map_err(|_| "COVER_OPERATION_LOCK_POISONED".to_string())?;
        let (cover, original, record) = cover_draft_paths(&app, &draft_id)?;
        exact_remove(&cover)?;
        exact_remove(&original)?;
        exact_remove(&record)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn retry_social_cover_publish(app: AppHandle) -> Result<SocialCoverState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cover_operation_lock()
            .lock()
            .map_err(|_| "COVER_OPERATION_LOCK_POISONED".to_string())?;
        if pending_cover_removal_path(&app)?.exists() {
            let result = upload_pending_cover_removal(&app);
            log_cover_publish_result("manual removal retry", &result);
        } else if pending_cover_path(&app)?.exists() {
            let result = upload_pending_cover(&app);
            log_cover_publish_result("manual retry", &result);
        }
        let state = get_social_cover_state_inner(&app)?;
        let _ = app.emit("social://cover-state", state.clone());
        Ok(state)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn migrate_legacy_social_cover(
    app: AppHandle,
    bytes: Vec<u8>,
    mime: String,
) -> Result<SocialCoverState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cover_operation_lock()
            .lock()
            .map_err(|_| "COVER_OPERATION_LOCK_POISONED".to_string())?;
        if local_cover_path(&app)?.exists() || cover_removed_tombstone_path(&app)?.exists() {
            return get_social_cover_state_inner(&app);
        }
        if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/bmp") {
            return Err("COVER_FORMAT_INVALID: Unsupported legacy cover MIME type.".to_string());
        }
        let encoded = encode_cover_bytes(&bytes)?;
        let hash = sha256_bytes(&encoded);
        let migration_id = Uuid::new_v4().to_string();
        let source_root = cover_drafts_dir(&app)?;
        let target_root = social_dir(&app)?;
        let cover_stage = source_root.join(format!("{migration_id}.migration.webp"));
        let original_stage = source_root.join(format!("{migration_id}.migration.original.bin"));
        crate::lua_live::atomic_write_path(&cover_stage, &encoded)?;
        crate::lua_live::atomic_write_path(&original_stage, &bytes)?;
        let current = CurrentCover {
            hash,
            width: COVER_WIDTH,
            height: COVER_HEIGHT,
            source: "legacyMigration".to_string(),
            committed_at: chrono::Utc::now().to_rfc3339(),
        };
        let current_stage = write_transaction_json(&source_root, &migration_id, "migration-current", &current)?;
        let changes = vec![
            replace_change(cover_stage.clone(), local_cover_path(&app)?, &source_root, &target_root)?,
            replace_change(original_stage.clone(), original_cover_path(&app)?, &source_root, &target_root)?,
            replace_change(current_stage.clone(), current_cover_path(&app)?, &source_root, &target_root)?,
        ];
        let result = crate::managed_file_transaction::apply_changes(&app, "social-cover-legacy-migration", changes);
        for path in [&cover_stage, &original_stage, &current_stage] {
            let _ = exact_remove(path);
        }
        result?;
        get_social_cover_state_inner(&app)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn get_social_cover_state(app: AppHandle) -> Result<SocialCoverState, String> {
    get_social_cover_state_inner(&app)
}

#[tauri::command]
pub fn read_social_cover_preview(app: AppHandle) -> Result<Vec<u8>, String> {
    let path = local_cover_path(&app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path).map_err(|error| format!("COVER_READ_FAILED: {error}"))?;
    if bytes.len() > MAX_COVER_BYTES {
        return Err("COVER_LOCAL_INVALID: Local preview exceeds the cover limit.".to_string());
    }
    Ok(bytes)
}

pub fn start_cover_retry_worker(app: AppHandle) {
    if COVER_WORKER_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    thread::spawn(move || loop {
        let mut reconcile_accepted_publish = false;
        if let Ok(_guard) = cover_operation_lock().lock() {
            if pending_cover_removal_path(&app).is_ok_and(|path| path.exists()) {
                // Removal pending files only survive a failed request, so retrying them is safe.
                let result = upload_pending_cover_removal(&app);
                log_cover_publish_result("background removal retry", &result);
                emit_cover_state(&app);
            } else if let Ok(path) = pending_cover_path(&app) {
                if path.exists() {
                    match read_json::<PendingCover>(&path) {
                        Ok(pending) if pending.last_error.is_some() || pending.publish_by.is_none() => {
                            // Retry only an actual failure (or a crash before the first request).
                            // Once the server has accepted the publish and supplied publishBy,
                            // another PUT would only duplicate queue work.
                            let result = upload_pending_cover(&app);
                            log_cover_publish_result("background retry", &result);
                        }
                        Ok(_) => {
                            // Accepted already: reconcile with server state, never re-upload blindly.
                            reconcile_accepted_publish = true;
                        }
                        Err(error) => {
                            eprintln!("[social-cover] pending state unreadable: {error}");
                        }
                    }
                    emit_cover_state(&app);
                }
            }
        }

        // Do network reconciliation outside the cover-operation lock so a slow backend cannot
        // block the user from selecting/saving another cover for up to the HTTP timeout.
        if reconcile_accepted_publish {
            if let Err(error) = fetch_bootstrap(&app) {
                eprintln!("[social-cover] publish reconciliation failed: {error}");
            }
            emit_cover_state(&app);
        }

        thread::sleep(RETRY_INTERVAL);
    });
}

pub fn start_social_event_stream(app: AppHandle) {
    if SSE_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    thread::spawn(move || {
        let mut backoff = Duration::from_secs(2);
        let mut last_event_id = String::new();
        loop {
            let result = (|| -> Result<(), String> {
                let client = http_client(Duration::from_secs(5 * 60))?;
                let mut request = authorized_request(&app, &client, Method::GET, "/events")?;
                if !last_event_id.is_empty() {
                    request = request.header("last-event-id", &last_event_id);
                }
                let response = request.send().map_err(|error| error.to_string())?;
                if response.status() == StatusCode::UNAUTHORIZED || !response.status().is_success()
                {
                    return Err(response_error(response));
                }
                backoff = Duration::from_secs(2);
                let mut reader = BufReader::new(response);
                let mut event_type = String::new();
                let mut event_id = String::new();
                let mut data = String::new();
                loop {
                    let mut line = String::new();
                    if reader
                        .read_line(&mut line)
                        .map_err(|error| error.to_string())?
                        == 0
                    {
                        return Err("SSE_DISCONNECTED".to_string());
                    }
                    let line = line.trim_end_matches(['\r', '\n']);
                    if line.is_empty() {
                        if !data.is_empty() && event_type != "ready" {
                            if let Ok(event) = serde_json::from_str::<SocialEvent>(&data) {
                                last_event_id = if event.id.is_empty() {
                                    event_id.clone()
                                } else {
                                    event.id.clone()
                                };
                                if event.event_type == "cover.published"
                                    || event.event_type == "cover.removed"
                                {
                                    let _ = reconcile_published_cover_event(&app, &event);
                                }
                                let _ = app.emit("social://event", event);
                            }
                        }
                        event_type.clear();
                        event_id.clear();
                        data.clear();
                    } else if let Some(value) = line.strip_prefix("id: ") {
                        event_id = value.to_string();
                    } else if let Some(value) = line.strip_prefix("event: ") {
                        event_type = value.to_string();
                    } else if let Some(value) = line.strip_prefix("data: ") {
                        if !data.is_empty() {
                            data.push('\n');
                        }
                        data.push_str(value);
                    }
                }
            })();
            if result.is_err() {
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_rejects_arbitrary_cleartext_hosts() {
        std::env::set_var("OXO_SOCIAL_API_BASE", "http://malicious.invalid/social");
        assert_eq!(social_api_base(), DEFAULT_SOCIAL_API_BASE);
        std::env::remove_var("OXO_SOCIAL_API_BASE");
    }
}
