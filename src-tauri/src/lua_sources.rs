use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, CACHE_CONTROL, ETAG, LAST_MODIFIED, RANGE};
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const HUBCAP_BASE: &str = "https://hubcapmanifest.com";
const BACKEND_BASE: &str = "https://zeroxolemon-launcher.onrender.com/api/0xolemon/lua-shop";
const HF_RAW_BASE: &str = "https://huggingface.co/datasets/Immaking/Luas/resolve/main";
const SUSHI_BASE: &str =
    "https://raw.githubusercontent.com/sushi-dev55-alt/sushitools-games-repo-alt/refs/heads/main";
const RYUU_BASE: &str = "http://167.235.229.108";
const SETTINGS_VERSION: u32 = 1;
const MAX_KEY_BYTES: usize = 4096;
const MAX_LUA_BYTES: usize = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const DEFAULT_KEY_LIFETIME_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaSourceSettingsDisk {
    version: u32,
    encrypted_hubcap_key: Option<String>,
    key_saved_at: Option<String>,
    #[serde(default = "default_sushi_enabled")]
    sushi_enabled: bool,
    #[serde(default)]
    ryuu_enabled: bool,
}

fn default_sushi_enabled() -> bool {
    true
}

impl Default for LuaSourceSettingsDisk {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            encrypted_hubcap_key: None,
            key_saved_at: None,
            sushi_enabled: true,
            ryuu_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HubcapUsageBucket {
    pub usage: Option<u64>,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubcapKeyState {
    pub configured: bool,
    pub valid: bool,
    pub masked_key: Option<String>,
    pub expires_at: Option<String>,
    pub expiry_estimated: bool,
    pub expiring_soon: bool,
    pub expired: bool,
    pub service_ready: bool,
    pub daily: HubcapUsageBucket,
    pub single: HubcapUsageBucket,
    pub bundle: HubcapUsageBucket,
    pub workshop: HubcapUsageBucket,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for HubcapKeyState {
    fn default() -> Self {
        Self {
            configured: false,
            valid: false,
            masked_key: None,
            expires_at: None,
            expiry_estimated: false,
            expiring_soon: false,
            expired: false,
            service_ready: false,
            daily: HubcapUsageBucket::default(),
            single: HubcapUsageBucket::default(),
            bundle: HubcapUsageBucket::default(),
            workshop: HubcapUsageBucket::default(),
            last_checked_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceSettingsState {
    pub hubcap: HubcapKeyState,
    pub sushi_enabled: bool,
    pub ryuu_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourcePreferencesRequest {
    pub sushi_enabled: bool,
    pub ryuu_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaPackageProvider {
    Curated,
    Community,
    Hubcap,
    Sushi,
    Ryuu,
    None,
}

impl Default for LuaPackageProvider {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum LuaSourceProvider {
    HuggingFace,
    Hubcap,
    Sushi,
    Ryuu,
}

impl LuaSourceProvider {
    pub(crate) fn cache_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "hugging-face",
            Self::Hubcap => "hubcap",
            Self::Sushi => "sushi",
            Self::Ryuu => "ryuu",
        }
    }

    pub(crate) fn accepts(self, provider: LuaPackageProvider) -> bool {
        matches!(
            (self, provider),
            (
                Self::HuggingFace,
                LuaPackageProvider::Curated | LuaPackageProvider::Community
            ) | (Self::Hubcap, LuaPackageProvider::Hubcap)
                | (Self::Sushi, LuaPackageProvider::Sushi)
                | (Self::Ryuu, LuaPackageProvider::Ryuu)
        )
    }
}

impl LuaPackageProvider {
    pub(crate) fn source(self) -> Option<LuaSourceProvider> {
        match self {
            Self::Curated | Self::Community => Some(LuaSourceProvider::HuggingFace),
            Self::Hubcap => Some(LuaSourceProvider::Hubcap),
            Self::Sushi => Some(LuaSourceProvider::Sushi),
            Self::Ryuu => Some(LuaSourceProvider::Ryuu),
            Self::None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaSourceOperation {
    Add,
    Update,
    Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceCandidate {
    pub provider: LuaSourceProvider,
    pub available: bool,
    pub enabled: bool,
    pub on_demand: bool,
    pub requires_key: bool,
    pub key_ready: bool,
    pub recommended: bool,
    pub variant: Option<LuaPackageProvider>,
    pub revision: Option<String>,
    pub modified_at: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceScanResult {
    pub appid: u32,
    pub operation: LuaSourceOperation,
    pub sources: Vec<LuaSourceCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceScanRequest {
    pub appid: u32,
    pub operation: LuaSourceOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceAvailability {
    pub appid: u32,
    pub curated_available: bool,
    pub community_available: bool,
    pub hubcap_available: bool,
    pub sushi_available: bool,
    pub ryuu_available: bool,
    pub preferred_provider: LuaPackageProvider,
    pub revision: Option<String>,
    pub source_modified_at: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaCatalogSearchRequest {
    pub query: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaCatalogItem {
    pub appid: u32,
    pub name: String,
    pub header_image: String,
    pub installed: bool,
    pub availability: LuaSourceAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaCatalogSearchPage {
    pub items: Vec<LuaCatalogItem>,
    pub next_cursor: Option<String>,
    pub total_estimate: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendCatalogItem {
    appid: u32,
    name: String,
    #[serde(default)]
    header_image: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendCatalogPage {
    items: Vec<BackendCatalogItem>,
    next_cursor: Option<String>,
    total_estimate: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LuaAddQuotaState {
    pub limit: u32,
    pub used: u32,
    pub remaining: u32,
    pub reset_at: Option<String>,
    pub server_time: Option<String>,
    pub timezone: Option<String>,
    pub available: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CanonicalManifest {
    pub depot_id: u32,
    pub manifest_gid: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CanonicalPackage {
    pub appid: u32,
    pub provider: LuaPackageProvider,
    pub revision: String,
    pub canonical_lua: String,
    pub manifests: Vec<CanonicalManifest>,
    pub archive_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct LuaAddReservation {
    pub request_id: String,
    pub appid: u32,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("lua-sources").join("settings.json"))
        .map_err(|error| format!("Could not resolve Lua source settings: {error}"))
}

fn load_settings(app: &AppHandle) -> Result<LuaSourceSettingsDisk, String> {
    let path = settings_path(app)?;
    if !path.is_file() {
        return Ok(LuaSourceSettingsDisk::default());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read Lua source settings: {error}"))?;
    let mut settings: LuaSourceSettingsDisk = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Lua source settings are invalid: {error}"))?;
    settings.version = SETTINGS_VERSION;
    Ok(settings)
}

fn save_settings(app: &AppHandle, settings: &LuaSourceSettingsDisk) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Could not serialize Lua source settings: {error}"))?;
    crate::lua_live::atomic_write_path(&settings_path(app)?, &bytes)
}

fn decrypt_hubcap_key(settings: &LuaSourceSettingsDisk) -> Result<Option<String>, String> {
    let Some(encoded) = settings.encrypted_hubcap_key.as_deref() else {
        return Ok(None);
    };
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "Stored Hubcap key is invalid".to_string())?;
    let clear = crate::secret_store::unprotect(&encrypted)?;
    let key = String::from_utf8(clear).map_err(|_| "Stored Hubcap key is invalid".to_string())?;
    if key.trim().is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("Stored Hubcap key is invalid".to_string());
    }
    Ok(Some(key))
}

fn source_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .user_agent(concat!(
            "0xoLemon/",
            env!("CARGO_PKG_VERSION"),
            " LuaSources"
        ))
        .build()
        .map_err(|error| format!("Could not initialize source client: {error}"))
}

fn ryuu_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .redirect(Policy::none())
        .user_agent(concat!(
            "0xoLemon/",
            env!("CARGO_PKG_VERSION"),
            " LuaSources-Ryuu"
        ))
        .build()
        .map_err(|error| format!("Could not initialize Ryuu client: {error}"))
}

fn ryuu_url(appid: u32) -> Result<Url, String> {
    let parsed = Url::parse(&format!("{RYUU_BASE}/{appid}"))
        .map_err(|error| format!("Invalid Ryuu source URL: {error}"))?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some("167.235.229.108")
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Ryuu source URL is not allowed".to_string());
    }
    Ok(parsed)
}

fn ensure_https_host(url: &str, hosts: &[&str]) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|error| format!("Invalid source URL: {error}"))?;
    if parsed.scheme() != "https"
        || !hosts.iter().any(|host| {
            parsed
                .host_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(host))
        })
    {
        return Err("Lua source host is not allowed".to_string());
    }
    Ok(parsed)
}

fn hubcap_get(client: &Client, key: &str, path: &str) -> Result<Response, String> {
    let url = ensure_https_host(&format!("{HUBCAP_BASE}{path}"), &["hubcapmanifest.com"])?;
    client
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {}", key.trim()))
        .header(ACCEPT_ENCODING, "identity")
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .map_err(|error| format!("Could not reach Hubcap: {error}"))
}

fn value_at<'a>(value: &'a Value, candidates: &[&[&str]]) -> Option<&'a Value> {
    candidates.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        Some(current)
    })
}

fn value_u64(value: &Value, candidates: &[&[&str]]) -> Option<u64> {
    value_at(value, candidates).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<u64>().ok()))
    })
}

fn usage_bucket(value: &Value, name: &str) -> HubcapUsageBucket {
    let bucket = value.get(name).unwrap_or(&Value::Null);
    let usage = value_u64(bucket, &[&["usage"], &["used"]]);
    let limit = value_u64(bucket, &[&["limit"], &["capacity"], &["daily_limit"]]);
    let remaining = value_u64(bucket, &[&["remaining"]]).or_else(|| {
        limit
            .zip(usage)
            .map(|(limit, usage)| limit.saturating_sub(usage))
    });
    HubcapUsageBucket {
        usage,
        limit,
        remaining,
    }
}

fn parse_expiry(value: &Value) -> Option<DateTime<Utc>> {
    let raw = value_at(
        value,
        &[
            &["expires_at"],
            &["expiresAt"],
            &["api_key_expires_at"],
            &["key", "expires_at"],
            &["user", "key_expires_at"],
        ],
    )?;
    if let Some(text) = raw.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|value| value.with_timezone(&Utc))
            .or_else(|| {
                NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
                    .ok()
                    .map(|value| value.and_utc())
            });
    }
    raw.as_i64()
        .and_then(|value| DateTime::from_timestamp(value, 0))
}

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    let suffix = trimmed.chars().rev().take(4).collect::<String>();
    format!("hub_****{}", suffix.chars().rev().collect::<String>())
}

fn daily_usage_bucket(value: &Value) -> HubcapUsageBucket {
    let usage = value_u64(value, &[&["daily_usage"]]);
    let limit = value_u64(
        value,
        &[
            &["daily_limit"],
            &["custom_api_limit"],
            &["role_daily_limit"],
        ],
    );
    let remaining = limit
        .zip(usage)
        .map(|(limit, usage)| limit.saturating_sub(usage));
    HubcapUsageBucket {
        usage,
        limit,
        remaining,
    }
}

fn estimate_expiry(settings: &LuaSourceSettingsDisk) -> Option<DateTime<Utc>> {
    settings
        .key_saved_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc) + ChronoDuration::days(DEFAULT_KEY_LIFETIME_DAYS))
}

fn refresh_hubcap_state_blocking(app: &AppHandle) -> Result<HubcapKeyState, String> {
    let settings = load_settings(app)?;
    let Some(key) = decrypt_hubcap_key(&settings)? else {
        return Ok(HubcapKeyState::default());
    };
    let mut state = HubcapKeyState {
        configured: true,
        masked_key: Some(mask_key(&key)),
        last_checked_at: Some(Utc::now().to_rfc3339()),
        ..HubcapKeyState::default()
    };
    let client = source_client(Duration::from_secs(15))?;
    let response = match hubcap_get(&client, &key, "/api/v1/generate/usage") {
        Ok(value) => value,
        Err(error) => {
            state.last_error = Some(error);
            let expiry = estimate_expiry(&settings);
            apply_expiry(&mut state, expiry, true);
            return Ok(state);
        }
    };
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        state.last_error = Some("HUBCAP_KEY_INVALID".to_string());
        let expiry = estimate_expiry(&settings);
        apply_expiry(&mut state, expiry, true);
        return Ok(state);
    }
    if !response.status().is_success() {
        state.last_error = Some(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
        let expiry = estimate_expiry(&settings);
        apply_expiry(&mut state, expiry, true);
        return Ok(state);
    }
    let payload: Value = response
        .json()
        .map_err(|_| "Hubcap usage response is invalid".to_string())?;
    state.valid = true;
    state.service_ready = value_at(&payload, &[&["steam_service_ready"], &["ready"]])
        .and_then(Value::as_bool)
        .unwrap_or(true);
    state.single = usage_bucket(&payload, "single");
    state.bundle = usage_bucket(&payload, "bundle");
    state.workshop = usage_bucket(&payload, "workshop");
    let mut explicit_expiry = None;
    match hubcap_get(&client, &key, "/api/v1/user/stats") {
        Ok(response) if response.status().is_success() => match response.json::<Value>() {
            Ok(user_stats) => {
                state.daily = daily_usage_bucket(&user_stats);
                explicit_expiry = parse_expiry(&user_stats);
            }
            Err(_) => state.last_error = Some("HUBCAP_USER_STATS_INVALID".to_string()),
        },
        Ok(response)
            if response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::FORBIDDEN =>
        {
            state.valid = false;
            state.last_error = Some("HUBCAP_KEY_INVALID".to_string());
        }
        Ok(response) => {
            state.last_error = Some(format!(
                "HUBCAP_USER_STATS_HTTP_{}",
                response.status().as_u16()
            ));
        }
        Err(_) => state.last_error = Some("HUBCAP_USER_STATS_UNAVAILABLE".to_string()),
    }
    apply_expiry(
        &mut state,
        explicit_expiry.or_else(|| estimate_expiry(&settings)),
        explicit_expiry.is_none(),
    );
    Ok(state)
}

fn apply_expiry(state: &mut HubcapKeyState, expiry: Option<DateTime<Utc>>, estimated: bool) {
    let now = Utc::now();
    state.expires_at = expiry.map(|value| value.to_rfc3339());
    state.expiry_estimated = expiry.is_some() && estimated;
    state.expired = !estimated && expiry.is_some_and(|value| value <= now);
    state.expiring_soon =
        expiry.is_some_and(|value| value > now && value <= now + ChronoDuration::hours(72));
    if state.expired {
        state.valid = false;
    }
}

#[tauri::command]
pub async fn get_lua_source_settings(app: AppHandle) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&app)?;
        Ok(LuaSourceSettingsState {
            hubcap: refresh_hubcap_state_blocking(&app)?,
            sushi_enabled: settings.sushi_enabled,
            ryuu_enabled: settings.ryuu_enabled,
        })
    })
    .await
    .map_err(|error| format!("Lua source settings task failed: {error}"))?
}

#[tauri::command]
pub async fn refresh_hubcap_key_state(app: AppHandle) -> Result<HubcapKeyState, String> {
    tauri::async_runtime::spawn_blocking(move || refresh_hubcap_state_blocking(&app))
        .await
        .map_err(|error| format!("Hubcap status task failed: {error}"))?
}

#[tauri::command]
pub async fn save_hubcap_api_key(
    app: AppHandle,
    api_key: String,
) -> Result<HubcapKeyState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = api_key.trim();
        if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_whitespace) {
            return Err("HUBCAP_KEY_INVALID".to_string());
        }
        let client = source_client(Duration::from_secs(15))?;
        let response = hubcap_get(&client, key, "/api/v1/generate/usage")?;
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err("HUBCAP_KEY_INVALID".to_string());
        }
        if !response.status().is_success() {
            return Err(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
        }
        let _: Value = response
            .json()
            .map_err(|_| "Hubcap usage response is invalid".to_string())?;
        let encrypted = crate::secret_store::protect(key.as_bytes())?;
        let mut settings = load_settings(&app)?;
        settings.encrypted_hubcap_key = Some(STANDARD.encode(encrypted));
        settings.key_saved_at = Some(Utc::now().to_rfc3339());
        save_settings(&app, &settings)?;
        refresh_hubcap_state_blocking(&app)
    })
    .await
    .map_err(|error| format!("Hubcap key task failed: {error}"))?
}

#[tauri::command]
pub fn clear_hubcap_api_key(app: AppHandle) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    settings.encrypted_hubcap_key = None;
    settings.key_saved_at = None;
    save_settings(&app, &settings)
}

#[tauri::command]
pub fn set_lua_source_preferences(
    app: AppHandle,
    request: LuaSourcePreferencesRequest,
) -> Result<LuaSourceSettingsState, String> {
    let mut settings = load_settings(&app)?;
    settings.sushi_enabled = request.sushi_enabled;
    settings.ryuu_enabled = request.ryuu_enabled;
    save_settings(&app, &settings)?;
    Ok(LuaSourceSettingsState {
        hubcap: refresh_hubcap_state_blocking(&app)?,
        sushi_enabled: settings.sushi_enabled,
        ryuu_enabled: settings.ryuu_enabled,
    })
}

#[derive(Debug, Clone)]
struct SourceProbe {
    etag: Option<String>,
    modified_at: Option<String>,
}

#[derive(Debug, Clone)]
struct CommunityProbe {
    revision: String,
    updated_at: Option<String>,
}

fn probe_url(client: &Client, parsed: Url) -> Result<Option<SourceProbe>, String> {
    let response = client
        .get(parsed)
        .header(RANGE, "bytes=0-0")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("Source probe failed: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() && response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(format!("Source probe returned HTTP {}", response.status()));
    }
    Ok(Some(SourceProbe {
        etag: response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        modified_at: response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    }))
}

fn url_exists(client: &Client, url: &str, hosts: &[&str]) -> Result<Option<SourceProbe>, String> {
    probe_url(client, ensure_https_host(url, hosts)?)
}

fn parse_source_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|value| value.with_timezone(&Utc))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|value| value.and_utc())
        })
}

fn empty_source_candidate(provider: LuaSourceProvider) -> LuaSourceCandidate {
    LuaSourceCandidate {
        provider,
        available: false,
        enabled: true,
        on_demand: false,
        requires_key: provider == LuaSourceProvider::Hubcap,
        key_ready: provider != LuaSourceProvider::Hubcap,
        recommended: false,
        variant: None,
        revision: None,
        modified_at: None,
        error_code: None,
    }
}

fn probe_hugging_face_source(appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::HuggingFace);
    let client = match source_client(Duration::from_secs(12)) {
        Ok(client) => client,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    let community = community_index_probe(
        &client,
        appid,
        &format!("{HF_RAW_BASE}/community/index/{appid}.json"),
    );
    let curated = url_exists(
        &client,
        &format!("{HF_RAW_BASE}/lua/{appid}.lua"),
        &["huggingface.co"],
    );

    match community {
        Ok(Some(value)) => {
            candidate.available = true;
            candidate.variant = Some(LuaPackageProvider::Community);
            candidate.revision = Some(value.revision);
            candidate.modified_at = value.updated_at;
        }
        Ok(None) => {}
        Err(error) => candidate.error_code = Some(error),
    }
    if !candidate.available {
        match curated {
            Ok(Some(value)) => {
                candidate.available = true;
                candidate.variant = Some(LuaPackageProvider::Curated);
                candidate.revision = value.etag.clone().or_else(|| value.modified_at.clone());
                candidate.modified_at = value.modified_at;
                candidate.error_code = None;
            }
            Ok(None) => {}
            Err(error) if candidate.error_code.is_none() => candidate.error_code = Some(error),
            Err(_) => {}
        }
    }
    candidate
}

fn probe_hubcap_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::Hubcap);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    let key = match decrypt_hubcap_key(&settings) {
        Ok(Some(key)) => key,
        Ok(None) => {
            candidate.error_code = Some("HUBCAP_KEY_REQUIRED".to_string());
            return candidate;
        }
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.key_ready = true;
    let client = match source_client(Duration::from_secs(12)) {
        Ok(client) => client,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    let response = match hubcap_get(&client, &key, &format!("/api/v1/status/{appid}")) {
        Ok(response) => response,
        Err(_) => {
            candidate.error_code = Some("HUBCAP_UNAVAILABLE".to_string());
            return candidate;
        }
    };
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        candidate.key_ready = false;
        candidate.error_code = Some("HUBCAP_KEY_INVALID".to_string());
        return candidate;
    }
    if response.status() == StatusCode::NOT_FOUND {
        candidate.on_demand = true;
        return candidate;
    }
    if !response.status().is_success() {
        candidate.error_code = Some(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
        return candidate;
    }
    let payload = match response.json::<Value>() {
        Ok(payload) => payload,
        Err(_) => {
            candidate.error_code = Some("HUBCAP_STATUS_INVALID".to_string());
            return candidate;
        }
    };
    candidate.available = value_at(
        &payload,
        &[&["manifest_file_exists"], &["available"], &["exists"]],
    )
    .and_then(Value::as_bool)
    .unwrap_or_else(|| {
        value_at(&payload, &[&["status"]])
            .and_then(Value::as_str)
            .is_some_and(|status| {
                matches!(
                    status.to_ascii_lowercase().as_str(),
                    "ready" | "available" | "ok" | "cached"
                )
            })
    });
    candidate.on_demand = !candidate.available;
    candidate.variant = Some(LuaPackageProvider::Hubcap);
    candidate.revision = value_at(&payload, &[&["revision"], &["etag"], &["hash"]])
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    candidate.modified_at = value_at(
        &payload,
        &[&["file_modified"], &["modified_at"], &["updated_at"]],
    )
    .and_then(Value::as_str)
    .map(ToOwned::to_owned);
    if candidate.revision.is_none() {
        candidate.revision = candidate.modified_at.clone();
    }
    candidate
}

fn probe_sushi_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::Sushi);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.sushi_enabled;
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    let client = match source_client(Duration::from_secs(12)) {
        Ok(client) => client,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    match url_exists(
        &client,
        &format!("{SUSHI_BASE}/{appid}.zip"),
        &["raw.githubusercontent.com"],
    ) {
        Ok(Some(value)) => {
            candidate.available = true;
            candidate.variant = Some(LuaPackageProvider::Sushi);
            candidate.revision = value.etag.clone().or_else(|| value.modified_at.clone());
            candidate.modified_at = value.modified_at;
        }
        Ok(None) => {}
        Err(error) => candidate.error_code = Some(error),
    }
    candidate
}

fn probe_ryuu_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::Ryuu);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.ryuu_enabled;
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    let client = match ryuu_client(Duration::from_secs(12)) {
        Ok(client) => client,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    match probe_url(
        &client,
        match ryuu_url(appid) {
            Ok(url) => url,
            Err(error) => {
                candidate.error_code = Some(error);
                return candidate;
            }
        },
    ) {
        Ok(Some(value)) => {
            candidate.available = true;
            candidate.variant = Some(LuaPackageProvider::Ryuu);
            candidate.revision = value.etag.clone().or_else(|| value.modified_at.clone());
            candidate.modified_at = value.modified_at;
        }
        Ok(None) => {}
        Err(error) => candidate.error_code = Some(error),
    }
    candidate
}

pub(crate) fn probe_source(
    app: &AppHandle,
    appid: u32,
    provider: LuaSourceProvider,
) -> LuaSourceCandidate {
    match provider {
        LuaSourceProvider::HuggingFace => probe_hugging_face_source(appid),
        LuaSourceProvider::Hubcap => probe_hubcap_source(app, appid),
        LuaSourceProvider::Sushi => probe_sushi_source(app, appid),
        LuaSourceProvider::Ryuu => probe_ryuu_source(app, appid),
    }
}

fn scan_lua_sources_blocking(
    app: &AppHandle,
    request: LuaSourceScanRequest,
) -> Result<LuaSourceScanResult, String> {
    if request.appid == 0 {
        return Err("AppID must be greater than zero".to_string());
    }
    let providers = [
        LuaSourceProvider::HuggingFace,
        LuaSourceProvider::Hubcap,
        LuaSourceProvider::Sushi,
        LuaSourceProvider::Ryuu,
    ];
    let appid = request.appid;
    let mut sources = std::thread::scope(|scope| {
        let handles = providers
            .into_iter()
            .map(|provider| {
                let app = app.clone();
                scope.spawn(move || probe_source(&app, appid, provider))
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect::<Vec<_>>()
    });
    sources.sort_by_key(|candidate| match candidate.provider {
        LuaSourceProvider::HuggingFace => 0,
        LuaSourceProvider::Hubcap => 1,
        LuaSourceProvider::Sushi => 2,
        LuaSourceProvider::Ryuu => 3,
    });
    let recommended = sources
        .iter()
        .position(|candidate| {
            candidate.provider == LuaSourceProvider::Hubcap
                && candidate.key_ready
                && candidate.enabled
                && (candidate.available || candidate.on_demand)
                && candidate.error_code.is_none()
        })
        .or_else(|| {
            sources
                .iter()
                .position(|candidate| candidate.enabled && candidate.available)
        });
    if let Some(index) = recommended {
        sources[index].recommended = true;
    }
    Ok(LuaSourceScanResult {
        appid,
        operation: request.operation,
        sources,
    })
}

#[tauri::command]
pub async fn scan_lua_sources(
    app: AppHandle,
    request: LuaSourceScanRequest,
) -> Result<LuaSourceScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || scan_lua_sources_blocking(&app, request))
        .await
        .map_err(|error| format!("Lua source scan task failed: {error}"))?
}

fn community_index_probe(
    client: &Client,
    appid: u32,
    url: &str,
) -> Result<Option<CommunityProbe>, String> {
    let parsed = ensure_https_host(url, &["huggingface.co"])?;
    let mut response = client
        .get(parsed)
        .header(ACCEPT_ENCODING, "identity")
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .map_err(|error| format!("Community index probe failed: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "Community index probe returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_LUA_BYTES as u64)
    {
        return Err("Community index is too large".to_string());
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take((MAX_LUA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read community index: {error}"))?;
    if bytes.len() > MAX_LUA_BYTES {
        return Err("Community index is too large".to_string());
    }
    let payload: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Community index is invalid".to_string())?;
    if value_u64(&payload, &[&["appid"]]) != Some(appid as u64) {
        return Err("Community index belongs to another AppID".to_string());
    }
    let revision = value_at(&payload, &[&["latestRevision"]])
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| "Community index revision is invalid".to_string())?;
    let updated_at = value_at(&payload, &[&["sourceModifiedAt"], &["updatedAt"]])
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(Some(CommunityProbe {
        revision: revision.to_ascii_lowercase(),
        updated_at,
    }))
}

fn probe_one(
    client: &Client,
    ryuu: Option<&Client>,
    appid: u32,
    settings: &LuaSourceSettingsDisk,
    key: Option<&str>,
) -> LuaSourceAvailability {
    let curated_url = format!("{HF_RAW_BASE}/lua/{appid}.lua");
    let community_url = format!("{HF_RAW_BASE}/community/index/{appid}.json");
    let curated = url_exists(client, &curated_url, &["huggingface.co"])
        .ok()
        .flatten();
    let community = community_index_probe(client, appid, &community_url)
        .ok()
        .flatten();
    let mut hubcap_available = false;
    let mut hubcap_revision = None;
    let mut modified = None;
    let mut error_code = None;
    if let Some(key) = key {
        match hubcap_get(client, key, &format!("/api/v1/status/{appid}")) {
            Ok(response) if response.status().is_success() => {
                if let Ok(payload) = response.json::<Value>() {
                    hubcap_available = value_at(
                        &payload,
                        &[&["manifest_file_exists"], &["available"], &["exists"]],
                    )
                    .and_then(Value::as_bool)
                    .unwrap_or_else(|| {
                        value_at(&payload, &[&["status"]])
                            .and_then(Value::as_str)
                            .is_some_and(|status| {
                                matches!(
                                    status.to_ascii_lowercase().as_str(),
                                    "ready" | "available" | "ok" | "cached"
                                )
                            })
                    });
                    modified = value_at(
                        &payload,
                        &[&["file_modified"], &["modified_at"], &["updated_at"]],
                    )
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                    hubcap_revision = value_at(&payload, &[&["revision"], &["etag"], &["hash"]])
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
            }
            Ok(response) if response.status() == StatusCode::NOT_FOUND => {}
            Ok(response)
                if response.status() == StatusCode::UNAUTHORIZED
                    || response.status() == StatusCode::FORBIDDEN =>
            {
                error_code = Some("HUBCAP_KEY_INVALID".to_string());
            }
            Ok(response) => {
                error_code = Some(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
            }
            Err(_) => {
                error_code = Some("HUBCAP_UNAVAILABLE".to_string());
            }
        }
    }
    let sushi_available = settings.sushi_enabled
        && url_exists(
            client,
            &format!("{SUSHI_BASE}/{appid}.zip"),
            &["raw.githubusercontent.com"],
        )
        .ok()
        .flatten()
        .is_some();
    let ryuu_available = settings.ryuu_enabled
        && ryuu
            .ok_or_else(|| "Ryuu source client is unavailable".to_string())
            .and_then(|client| probe_url(client, ryuu_url(appid)?))
            .ok()
            .flatten()
            .is_some();

    let hubcap_time = modified.as_deref().and_then(parse_source_time);
    let community_matches_revision = hubcap_revision.as_deref().is_some_and(|source| {
        community
            .as_ref()
            .is_some_and(|cached| source.eq_ignore_ascii_case(&cached.revision))
    });
    let community_fresh = !hubcap_available
        || community_matches_revision
        || hubcap_time
            .zip(
                community
                    .as_ref()
                    .and_then(|value| value.updated_at.as_deref())
                    .and_then(parse_source_time),
            )
            .is_some_and(|(source, cached)| source <= cached);
    let community_stale = community.is_some() && !community_fresh;
    let curated_stale = hubcap_available
        && hubcap_time
            .zip(
                curated
                    .as_ref()
                    .and_then(|value| value.modified_at.as_deref())
                    .and_then(parse_source_time),
            )
            .is_some_and(|(source, cached)| source > cached);

    let preferred_provider = if community.is_some() && !community_stale {
        LuaPackageProvider::Community
    } else if curated.is_some() && !curated_stale {
        LuaPackageProvider::Curated
    } else if hubcap_available {
        LuaPackageProvider::Hubcap
    } else if sushi_available {
        LuaPackageProvider::Sushi
    } else if ryuu_available {
        LuaPackageProvider::Ryuu
    } else {
        LuaPackageProvider::None
    };
    let revision = match preferred_provider {
        LuaPackageProvider::Community => community.as_ref().map(|value| value.revision.clone()),
        LuaPackageProvider::Curated => curated.as_ref().and_then(|value| value.etag.clone()),
        LuaPackageProvider::Hubcap => hubcap_revision.or_else(|| modified.clone()),
        _ => None,
    };
    LuaSourceAvailability {
        appid,
        curated_available: curated.is_some(),
        community_available: community.is_some(),
        hubcap_available,
        sushi_available,
        ryuu_available,
        preferred_provider,
        revision,
        source_modified_at: modified,
        error_code,
    }
}

pub(crate) fn probe_installed_source(
    app: &AppHandle,
    appid: u32,
) -> Result<LuaSourceAvailability, String> {
    let settings = load_settings(app)?;
    let key = decrypt_hubcap_key(&settings)?;
    let client = source_client(Duration::from_secs(12))?;
    let ryuu = settings
        .ryuu_enabled
        .then(|| ryuu_client(Duration::from_secs(12)))
        .transpose()?;
    Ok(probe_one(
        &client,
        ryuu.as_ref(),
        appid,
        &settings,
        key.as_deref(),
    ))
}

fn probe_many_blocking(
    app: &AppHandle,
    appids: &[u32],
) -> Result<Vec<LuaSourceAvailability>, String> {
    if appids.len() > 40 {
        return Err("At most 40 AppIDs may be probed at once".to_string());
    }
    let settings = load_settings(app)?;
    let key = decrypt_hubcap_key(&settings)?;
    let client = source_client(Duration::from_secs(12))?;
    let ryuu = settings
        .ryuu_enabled
        .then(|| ryuu_client(Duration::from_secs(12)))
        .transpose()?;
    let mut unique = BTreeSet::new();
    unique.extend(appids.iter().copied().filter(|appid| *appid > 0));
    let ids = unique.into_iter().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(4) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .map(|appid| {
                    let settings = &settings;
                    let key = key.as_deref();
                    let client = &client;
                    let ryuu = ryuu.as_ref();
                    scope.spawn(move || probe_one(client, ryuu, *appid, settings, key))
                })
                .collect::<Vec<_>>();
            for handle in handles {
                if let Ok(value) = handle.join() {
                    output.push(value);
                }
            }
        });
    }
    output.sort_by_key(|value| value.appid);
    Ok(output)
}

#[tauri::command]
pub async fn probe_lua_source_availability(
    app: AppHandle,
    appids: Vec<u32>,
) -> Result<Vec<LuaSourceAvailability>, String> {
    tauri::async_runtime::spawn_blocking(move || probe_many_blocking(&app, &appids))
        .await
        .map_err(|error| format!("Lua source probe task failed: {error}"))?
}

fn steam_search(
    client: &Client,
    query: &str,
    offset: usize,
    limit: usize,
) -> Result<(Vec<(u32, String)>, Option<u64>), String> {
    if query.trim().is_empty() {
        return Ok((Vec::new(), None));
    }
    if let Ok(appid) = query.trim().parse::<u32>() {
        let response = client
            .get("https://store.steampowered.com/api/appdetails")
            .query(&[("appids", appid.to_string()), ("l", "english".to_string())])
            .send()
            .map_err(|error| format!("Steam lookup failed: {error}"))?;
        if response.status().is_success() {
            let payload: Value = response
                .json()
                .map_err(|_| "Steam lookup response is invalid".to_string())?;
            if let Some(name) = payload
                .get(appid.to_string())
                .and_then(|value| value.get("data"))
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
            {
                return Ok((vec![(appid, name.to_string())], Some(1)));
            }
        }
    }
    let response = client
        .get("https://store.steampowered.com/api/storesearch/")
        .query(&[
            ("term", query.trim().to_string()),
            ("cc", "us".to_string()),
            ("l", "english".to_string()),
            ("start", offset.to_string()),
            ("count", limit.to_string()),
        ])
        .send()
        .map_err(|error| format!("Steam search failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Steam search returned HTTP {}", response.status()));
    }
    let payload: Value = response
        .json()
        .map_err(|_| "Steam search response is invalid".to_string())?;
    let total = payload.get("total").and_then(Value::as_u64);
    let mut items = Vec::new();
    for item in payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if item.get("type").and_then(Value::as_str) != Some("app") {
            continue;
        }
        let Some(appid) = item
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        items.push((appid, name.to_string()));
    }
    Ok((items, total))
}

fn backend_catalog_search(
    client: &Client,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<BackendCatalogPage, String> {
    let mut request = client
        .get(format!("{BACKEND_BASE}/catalog/search"))
        .query(&[("query", query.trim()), ("limit", &limit.to_string())]);
    if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
        request = request.query(&[("cursor", cursor)]);
    }
    let response = request
        .send()
        .map_err(|error| format!("Lua catalog backend unavailable: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Lua catalog backend returned HTTP {}",
            response.status()
        ));
    }
    response
        .json::<BackendCatalogPage>()
        .map_err(|_| "Lua catalog backend response is invalid".to_string())
}

fn lua_is_installed(appid: u32) -> bool {
    crate::steam::get_steam_path()
        .map(|root| {
            root.join("config")
                .join("stplug-in")
                .join(format!("{appid}.lua"))
        })
        .is_some_and(|path| path.is_file())
}

fn search_lua_games_blocking(
    app: &AppHandle,
    request: LuaCatalogSearchRequest,
) -> Result<LuaCatalogSearchPage, String> {
    let limit = request.limit.unwrap_or(40).clamp(1, 40) as usize;
    let client = source_client(Duration::from_secs(12))?;
    let backend_page =
        backend_catalog_search(&client, &request.query, request.cursor.as_deref(), limit);
    let (raw_items, total, next_cursor) = match backend_page {
        Ok(page) => (
            page.items
                .into_iter()
                .map(|item| (item.appid, item.name, item.header_image))
                .collect::<Vec<_>>(),
            page.total_estimate,
            page.next_cursor,
        ),
        Err(backend_error) => {
            crate::debug_log::debug_log(&format!("Lua catalog backend fallback: {backend_error}"));
            let offset = request
                .cursor
                .as_deref()
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| backend_error)?;
            if request.query.trim().is_empty() {
                let mut catalog = crate::shop_lua::catalog_blocking()?;
                catalog.sort_by(|left, right| {
                    left.name.to_lowercase().cmp(&right.name.to_lowercase())
                });
                let total = catalog.len() as u64;
                let items = catalog
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(|item| (item.appid, item.name, String::new()))
                    .collect::<Vec<_>>();
                let consumed = offset.saturating_add(items.len());
                let next = (consumed < total as usize).then(|| consumed.to_string());
                (items, Some(total), next)
            } else {
                let (items, total) = steam_search(&client, &request.query, offset, limit)?;
                let items = items
                    .into_iter()
                    .map(|(appid, name)| (appid, name, String::new()))
                    .collect::<Vec<_>>();
                let consumed = offset.saturating_add(items.len());
                let next = total
                    .map(|value| consumed < value as usize)
                    .unwrap_or(items.len() == limit)
                    .then(|| consumed.to_string());
                (items, total, next)
            }
        }
    };
    let ids = raw_items.iter().map(|value| value.0).collect::<Vec<_>>();
    let availability = probe_many_blocking(app, &ids)?
        .into_iter()
        .map(|value| (value.appid, value))
        .collect::<BTreeMap<_, _>>();
    let items = raw_items
        .into_iter()
        .map(|(appid, name, header_image)| LuaCatalogItem {
            appid,
            name,
            header_image,
            installed: lua_is_installed(appid),
            availability: availability
                .get(&appid)
                .cloned()
                .unwrap_or(LuaSourceAvailability {
                    appid,
                    curated_available: false,
                    community_available: false,
                    hubcap_available: false,
                    sushi_available: false,
                    ryuu_available: false,
                    preferred_provider: LuaPackageProvider::None,
                    revision: None,
                    source_modified_at: None,
                    error_code: Some("SOURCE_PROBE_INCOMPLETE".to_string()),
                }),
        })
        .collect::<Vec<_>>();
    Ok(LuaCatalogSearchPage {
        items,
        next_cursor,
        total_estimate: total,
    })
}

#[tauri::command]
pub async fn search_lua_games(
    app: AppHandle,
    request: LuaCatalogSearchRequest,
) -> Result<LuaCatalogSearchPage, String> {
    tauri::async_runtime::spawn_blocking(move || search_lua_games_blocking(&app, request))
        .await
        .map_err(|error| format!("Lua catalog search task failed: {error}"))?
}

fn backend_client() -> Result<Client, String> {
    source_client(Duration::from_secs(20))
}

fn backend_error(response: Response) -> String {
    let status = response.status();
    response
        .json::<Value>()
        .ok()
        .and_then(|value| {
            value
                .get("code")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("LUA_BACKEND_HTTP_{}", status.as_u16()))
}

fn get_lua_add_quota_blocking(
    app: &AppHandle,
    timezone: Option<&str>,
) -> Result<LuaAddQuotaState, String> {
    let token = crate::discord_auth::access_token_for_backend(app)?;
    let timezone = timezone
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .unwrap_or("UTC");
    let response = backend_client()?
        .get(format!("{BACKEND_BASE}/quota"))
        .query(&[("timezone", timezone)])
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("Lua quota service unavailable: {error}"))?;
    if !response.status().is_success() {
        return Err(backend_error(response));
    }
    response
        .json()
        .map_err(|_| "Lua quota response is invalid".to_string())
}

pub(crate) fn reserve_lua_add(
    app: &AppHandle,
    appid: u32,
    timezone: Option<&str>,
    request_id: Option<&str>,
) -> Result<LuaAddReservation, String> {
    let request_id = request_id
        .filter(|value| Uuid::parse_str(value).is_ok())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timezone = timezone
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .unwrap_or("UTC");
    let token = crate::discord_auth::access_token_for_backend(app)?;
    let response = backend_client()?
        .post(format!("{BACKEND_BASE}/add/reserve"))
        .bearer_auth(token)
        .json(&json!({
            "appid": appid,
            "requestId": request_id,
            "timezone": timezone,
        }))
        .send()
        .map_err(|error| format!("Lua quota service unavailable: {error}"))?;
    if !response.status().is_success() {
        return Err(backend_error(response));
    }
    Ok(LuaAddReservation { request_id, appid })
}

fn settle_lua_add(
    app: &AppHandle,
    reservation: &LuaAddReservation,
    completed: bool,
) -> Result<(), String> {
    let token = crate::discord_auth::access_token_for_backend(app)?;
    let action = if completed { "complete" } else { "fail" };
    let client = backend_client()?;
    let mut last_error = None;
    for attempt in 0..3 {
        let response = client
            .post(format!("{BACKEND_BASE}/add/{action}"))
            .bearer_auth(&token)
            .json(&json!({
                "appid": reservation.appid,
                "requestId": reservation.request_id,
            }))
            .send();
        match response {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => {
                let status = response.status();
                let error = backend_error(response);
                if !status.is_server_error() && status != StatusCode::TOO_MANY_REQUESTS {
                    return Err(error);
                }
                last_error = Some(error);
            }
            Err(error) => {
                last_error = Some(format!("Lua quota service unavailable: {error}"));
            }
        }
        if attempt < 2 {
            std::thread::sleep(Duration::from_millis(250 * (attempt + 1) as u64));
        }
    }
    Err(last_error.unwrap_or_else(|| "Lua quota settlement failed".to_string()))
}

pub(crate) fn complete_lua_add(
    app: &AppHandle,
    reservation: &LuaAddReservation,
) -> Result<(), String> {
    settle_lua_add(app, reservation, true)
}

pub(crate) fn fail_lua_add(app: &AppHandle, reservation: &LuaAddReservation) {
    let _ = settle_lua_add(app, reservation, false);
}

#[tauri::command]
pub async fn get_lua_add_quota(
    app: AppHandle,
    timezone: Option<String>,
) -> Result<LuaAddQuotaState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        get_lua_add_quota_blocking(&app, timezone.as_deref())
    })
    .await
    .map_err(|error| format!("Lua quota task failed: {error}"))?
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn validate_manifest_magic(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 8 {
        return Err("Manifest binary is too small".to_string());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap_or_default());
    if magic != 0x71F6_17D0 {
        return Err("Manifest binary has an invalid Steam depot magic".to_string());
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Package contains an unsafe path".to_string());
    }
    Ok(())
}

fn parse_manifest_name(name: &str) -> Option<(u32, String)> {
    let stem = name.strip_suffix(".manifest")?;
    let (depot, gid) = stem.split_once('_')?;
    let depot_id = depot.parse::<u32>().ok()?;
    if gid.is_empty() || !gid.chars().all(|value| value.is_ascii_digit()) {
        return None;
    }
    gid.parse::<u64>().ok()?;
    Some((depot_id, gid.to_string()))
}

fn decode_lua_string(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    let quote = value
        .chars()
        .next()
        .ok_or_else(|| "Empty Lua argument".to_string())?;
    if (quote != '\'' && quote != '\"') || value.chars().last() != Some(quote) {
        return Err("Only quoted strings are accepted for this Lua argument".to_string());
    }
    let mut output = String::new();
    let inner = &value[quote.len_utf8()..value.len() - quote.len_utf8()];
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            let escaped = chars
                .next()
                .ok_or_else(|| "Invalid Lua string escape".to_string())?;
            output.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '\'' => '\'',
                '\"' => '\"',
                _ => return Err("Unsupported Lua string escape".to_string()),
            });
        } else {
            output.push(character);
        }
    }
    if output.chars().any(char::is_control) {
        return Err("Lua string contains control characters".to_string());
    }
    Ok(output)
}

fn quote_lua(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    )
}

fn parse_decimal(raw: &str, field: &str) -> Result<String, String> {
    let value = raw.trim().trim_matches(['\'', '\"']);
    if value.is_empty() || !value.chars().all(|char| char.is_ascii_digit()) {
        return Err(format!("{field} must contain decimal digits only"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("{field} is outside the supported range"))?;
    Ok(value.to_string())
}

fn split_lua_args(raw: &str) -> Result<Vec<String>, String> {
    let bytes = raw.as_bytes();
    let mut output = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == active {
                quote = None;
            }
        } else if byte == b'\'' || byte == b'\"' {
            quote = Some(byte);
        } else if byte == b',' {
            let value = raw[start..index].trim();
            if value.is_empty() {
                return Err("Lua call contains an empty argument".to_string());
            }
            output.push(value.to_string());
            start = index + 1;
        } else if matches!(byte, b'(' | b')' | b'{' | b'}' | b'[' | b']') {
            return Err("Nested Lua expressions are not accepted".to_string());
        }
        index += 1;
    }
    if quote.is_some() {
        return Err("Lua call contains an unterminated string".to_string());
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        output.push(tail.to_string());
    }
    Ok(output)
}

fn skip_space_and_comments(source: &str, mut index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    loop {
        while bytes
            .get(index)
            .is_some_and(|value| value.is_ascii_whitespace())
        {
            index += 1;
        }
        if bytes.get(index..index + 2) != Some(b"--") {
            return Ok(index);
        }
        index += 2;
        if bytes.get(index) == Some(&b'[') {
            let mut probe = index + 1;
            while bytes.get(probe) == Some(&b'=') {
                probe += 1;
            }
            if bytes.get(probe) == Some(&b'[') {
                let level = probe - index - 1;
                index = probe + 1;
                let mut found = false;
                while index < bytes.len() {
                    if bytes[index] == b']' {
                        let mut close = index + 1;
                        let mut equals = 0;
                        while bytes.get(close) == Some(&b'=') {
                            equals += 1;
                            close += 1;
                        }
                        if equals == level && bytes.get(close) == Some(&b']') {
                            index = close + 1;
                            found = true;
                            break;
                        }
                    }
                    index += 1;
                }
                if !found {
                    return Err("Lua source contains an unterminated block comment".to_string());
                }
                continue;
            }
        }
        while bytes.get(index).is_some_and(|value| *value != b'\n') {
            index += 1;
        }
    }
}

fn parse_top_level_calls(source: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    if source.is_empty() || source.len() > MAX_LUA_BYTES || source.as_bytes().contains(&0) {
        return Err("Lua source is empty or too large".to_string());
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut calls = Vec::new();
    while index < bytes.len() {
        index = skip_space_and_comments(source, index)?;
        while bytes.get(index) == Some(&b';') {
            index = skip_space_and_comments(source, index + 1)?;
        }
        if index >= bytes.len() {
            break;
        }
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|value| value.is_ascii_alphanumeric() || *value == b'_')
        {
            index += 1;
        }
        if start == index {
            return Err(format!("Unsupported Lua statement at byte {index}"));
        }
        let function = source[start..index].to_ascii_lowercase();
        index = skip_space_and_comments(source, index)?;
        if bytes.get(index) != Some(&b'(') {
            return Err(format!("Unsupported Lua statement: {function}"));
        }
        let args_start = index + 1;
        index += 1;
        let mut quote = None;
        while index < bytes.len() {
            let byte = bytes[index];
            if let Some(active) = quote {
                if byte == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if byte == active {
                    quote = None;
                }
            } else if byte == b'\'' || byte == b'\"' {
                quote = Some(byte);
            } else if byte == b')' {
                break;
            } else if byte == b'(' {
                return Err("Nested Lua calls are not accepted".to_string());
            }
            index += 1;
        }
        if index >= bytes.len() || quote.is_some() {
            return Err(format!("Unterminated Lua call: {function}"));
        }
        let args = split_lua_args(&source[args_start..index])?;
        calls.push((function, args));
        index += 1;
        index = skip_space_and_comments(source, index)?;
        if bytes.get(index) == Some(&b';') {
            index += 1;
        }
    }
    Ok(calls)
}

fn canonicalize_lua(appid: u32, source: &str) -> Result<(String, BTreeMap<u32, String>), String> {
    let calls = parse_top_level_calls(source)?;
    let allowed: HashSet<&str> = [
        "addappid",
        "addtoken",
        "setmanifestid",
        "setappticket",
        "seteticket",
        "setstat",
        "forcedenuvo",
        "skipmanifestpin",
        "addprocess",
    ]
    .into_iter()
    .collect();
    let mut output = String::from("-- Canonical Lua package managed by 0xoLemon\n");
    let mut root_registered = false;
    let mut manifests = BTreeMap::new();
    for (function, args) in calls {
        if !allowed.contains(function.as_str()) {
            return Err(format!("Unsupported Lua declaration: {function}"));
        }
        let line = match function.as_str() {
            "addappid" => {
                if !(1..=3).contains(&args.len()) {
                    return Err("addappid accepts one to three arguments".to_string());
                }
                let id = parse_decimal(&args[0], "AppID")?;
                root_registered |= id.parse::<u32>().ok() == Some(appid);
                let mut rendered = vec![id];
                if args.len() >= 2 {
                    rendered.push(parse_decimal(&args[1], "addappid flag")?);
                }
                if args.len() == 3 {
                    let key = decode_lua_string(&args[2])?;
                    if key.len() != 64 || !key.chars().all(|value| value.is_ascii_hexdigit()) {
                        return Err(
                            "Depot key must be a 64-character hexadecimal value".to_string()
                        );
                    }
                    rendered.push(quote_lua(&key.to_ascii_lowercase()));
                }
                format!("addappid({})", rendered.join(", "))
            }
            "addtoken" => {
                if args.len() != 2 {
                    return Err("addtoken requires AppID and token".to_string());
                }
                format!(
                    "addtoken({}, {})",
                    parse_decimal(&args[0], "AppID")?,
                    quote_lua(&parse_decimal(&args[1], "token")?)
                )
            }
            "setmanifestid" => {
                if !(2..=3).contains(&args.len()) {
                    return Err(
                        "setManifestid requires depot ID, manifest GID and optional size"
                            .to_string(),
                    );
                }
                let depot = parse_decimal(&args[0], "Depot ID")?;
                let gid = parse_decimal(&args[1], "Manifest GID")?;
                let depot_id = depot
                    .parse::<u32>()
                    .map_err(|_| "Depot ID is outside the supported range".to_string())?;
                if manifests
                    .get(&depot_id)
                    .is_some_and(|existing| existing != &gid)
                {
                    return Err(format!(
                        "Lua source contains conflicting manifest pins for depot {depot_id}"
                    ));
                }
                manifests.insert(depot_id, gid.clone());
                if args.len() == 3 {
                    format!(
                        "setManifestid({}, {}, {})",
                        depot,
                        quote_lua(&gid),
                        parse_decimal(&args[2], "Manifest size")?
                    )
                } else {
                    format!("setManifestid({}, {})", depot, quote_lua(&gid))
                }
            }
            "setappticket" | "seteticket" => {
                if !(1..=2).contains(&args.len()) {
                    return Err(format!("{function} accepts one or two arguments"));
                }
                let rendered = args
                    .iter()
                    .map(|value| {
                        if value.trim().starts_with(['\'', '\"']) {
                            decode_lua_string(value).map(|value| quote_lua(&value))
                        } else {
                            parse_decimal(value, "ticket AppID")
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let canonical_name = if function == "setappticket" {
                    "setAppTicket"
                } else {
                    "setETicket"
                };
                format!("{}({})", canonical_name, rendered.join(", "))
            }
            "setstat" => {
                if !(1..=2).contains(&args.len()) {
                    return Err("setStat accepts AppID and optional SteamID".to_string());
                }
                let mut rendered = vec![parse_decimal(&args[0], "AppID")?];
                if args.len() == 2 {
                    rendered.push(quote_lua(&parse_decimal(&args[1], "SteamID")?));
                }
                format!("setStat({})", rendered.join(", "))
            }
            "forcedenuvo" => {
                if args.len() != 1 {
                    return Err("forceDenuvo requires one AppID".to_string());
                }
                format!("forceDenuvo({})", parse_decimal(&args[0], "AppID")?)
            }
            "skipmanifestpin" => {
                if args.len() != 1 {
                    return Err("skipManifestPin requires one depot ID".to_string());
                }
                format!("skipManifestPin({})", parse_decimal(&args[0], "Depot ID")?)
            }
            "addprocess" => {
                if args.len() != 2 {
                    return Err("addProcess requires AppID and executable name".to_string());
                }
                let name = decode_lua_string(&args[1])?;
                if name.is_empty()
                    || name.len() > 260
                    || name.contains(['/', '\\'])
                    || !name.to_ascii_lowercase().ends_with(".exe")
                {
                    return Err("addProcess executable name is invalid".to_string());
                }
                format!(
                    "addProcess({}, {})",
                    parse_decimal(&args[0], "AppID")?,
                    quote_lua(&name)
                )
            }
            _ => unreachable!(),
        };
        output.push_str(&line);
        output.push('\n');
    }
    if !root_registered {
        return Err(format!("Lua package does not register root AppID {appid}"));
    }
    Ok((output, manifests))
}

fn inspect_manifest_archive(bytes: &[u8]) -> Result<Vec<CanonicalManifest>, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Manifest package is invalid: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Manifest package contains too many entries".to_string());
    }
    let mut seen = HashSet::new();
    let mut seen_depots = HashSet::new();
    let mut expanded = 0_u64;
    let mut manifests = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect manifest package: {error}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "Manifest package contains an unsafe path".to_string())?;
        validate_archive_path(&enclosed)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Manifest package contains a symbolic link".to_string());
        }
        if !entry.is_file() {
            continue;
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err("Manifest package expands beyond the safety limit".to_string());
        }
        let file_name = enclosed
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Manifest package contains a non-UTF-8 file name".to_string())?
            .to_string();
        let normalized = file_name.to_ascii_lowercase();
        if !seen.insert(normalized) {
            return Err("Manifest package contains duplicate file names".to_string());
        }
        if !file_name.to_ascii_lowercase().ends_with(".manifest") {
            continue;
        }
        let (depot_id, manifest_gid) = parse_manifest_name(&file_name)
            .ok_or_else(|| format!("Invalid manifest file name: {file_name}"))?;
        if !seen_depots.insert(depot_id) {
            return Err(format!(
                "Manifest package contains multiple files for depot {depot_id}"
            ));
        }
        let entry_size = entry.size();
        let mut data = Vec::with_capacity(entry_size as usize);
        entry
            .by_ref()
            .take(entry_size.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|error| format!("Could not read {file_name}: {error}"))?;
        validate_manifest_magic(&data)?;
        manifests.push(CanonicalManifest {
            depot_id,
            manifest_gid,
            file_name,
            sha256: sha256_bytes(&data),
            bytes: data,
        });
    }
    if manifests.is_empty() {
        return Err("Manifest package does not contain any depot manifests".to_string());
    }
    manifests.sort_by_key(|value| (value.depot_id, value.manifest_gid.clone()));
    Ok(manifests)
}

fn build_canonical_archive(
    appid: u32,
    provider: LuaPackageProvider,
    canonical_lua: &str,
    manifests: &[CanonicalManifest],
) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut output);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        writer
            .start_file(format!("lua/{appid}.lua"), options)
            .map_err(|error| format!("Could not create canonical package: {error}"))?;
        writer
            .write_all(canonical_lua.as_bytes())
            .map_err(|error| format!("Could not create canonical package: {error}"))?;
        for manifest in manifests {
            writer
                .start_file(format!("manifests/{}", manifest.file_name), options)
                .map_err(|error| format!("Could not create canonical package: {error}"))?;
            writer
                .write_all(&manifest.bytes)
                .map_err(|error| format!("Could not create canonical package: {error}"))?;
        }
        let metadata = json!({
            "schemaVersion": 1,
            "appid": appid,
            "provider": provider,
            "manifests": manifests.iter().map(|value| json!({
                "depotId": value.depot_id,
                "manifestGid": value.manifest_gid,
                "fileName": value.file_name,
                "sha256": value.sha256,
                "size": value.bytes.len(),
            })).collect::<Vec<_>>(),
        });
        writer
            .start_file("metadata.json", options)
            .map_err(|error| format!("Could not create canonical package: {error}"))?;
        writer
            .write_all(
                &serde_json::to_vec_pretty(&metadata)
                    .map_err(|error| format!("Could not serialize package metadata: {error}"))?,
            )
            .map_err(|error| format!("Could not create canonical package: {error}"))?;
        writer
            .finish()
            .map_err(|error| format!("Could not finalize canonical package: {error}"))?;
    }
    Ok(output.into_inner())
}

fn cache_dir(app: &AppHandle, provider: LuaSourceProvider, appid: u32) -> Result<PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map(|path| {
            path.join("lua-sources")
                .join(provider.cache_name())
                .join(appid.to_string())
        })
        .map_err(|error| format!("Could not resolve Lua package cache: {error}"))
}

fn download_to_part(
    app: &AppHandle,
    provider: LuaSourceProvider,
    appid: u32,
    client: &Client,
    key: &str,
    endpoint: &str,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let response = hubcap_get(client, key, endpoint)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("HUBCAP_PACKAGE_NOT_FOUND".to_string());
    }
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        return Err("HUBCAP_KEY_INVALID".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
    }
    if response.content_length().is_some_and(|size| size > limit) {
        return Err("Hubcap package exceeds the download safety limit".to_string());
    }
    let dir = cache_dir(app, provider, appid)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not prepare Lua package cache: {error}"))?;
    let part = dir.join(format!("{name}.part"));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&part)
        .map_err(|error| format!("Could not create Lua package cache: {error}"))?;
    let mut response = response;
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Could not download Hubcap package: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            let _ = fs::remove_file(&part);
            return Err("Hubcap package exceeds the download safety limit".to_string());
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not cache Hubcap package: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not flush Hubcap package cache: {error}"))?;
    drop(file);
    let bytes =
        fs::read(&part).map_err(|error| format!("Could not read Hubcap package cache: {error}"))?;
    Ok(bytes)
}

fn download_public_to_part(
    app: &AppHandle,
    provider: LuaSourceProvider,
    appid: u32,
    client: &Client,
    url: &str,
    hosts: &[&str],
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, String> {
    let parsed = ensure_https_host(url, hosts)?;
    let mut response = client
        .get(parsed)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("Could not reach Lua package source: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "Lua package source returned HTTP {}",
            response.status()
        ));
    }
    if response.content_length().is_some_and(|size| size > limit) {
        return Err("Lua package exceeds the download safety limit".to_string());
    }
    let dir = cache_dir(app, provider, appid)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not prepare Lua package cache: {error}"))?;
    let part = dir.join(format!("{name}.part"));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&part)
        .map_err(|error| format!("Could not create Lua package cache: {error}"))?;
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Could not download Lua package: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            let _ = fs::remove_file(&part);
            return Err("Lua package exceeds the download safety limit".to_string());
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not cache Lua package: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not flush Lua package cache: {error}"))?;
    drop(file);
    fs::read(&part)
        .map(Some)
        .map_err(|error| format!("Could not read Lua package cache: {error}"))
}

fn package_from_archive(
    appid: u32,
    provider: LuaPackageProvider,
    archive_bytes: &[u8],
) -> Result<CanonicalPackage, String> {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes))
        .map_err(|error| format!("Lua package is invalid: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Lua package contains too many entries".to_string());
    }
    let mut expanded = 0_u64;
    let mut seen_paths = HashSet::new();
    let mut source_lua = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect Lua package: {error}"))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| "Lua package contains an unsafe path".to_string())?;
        validate_archive_path(&path)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Lua package contains a symbolic link".to_string());
        }
        if !entry.is_file() {
            continue;
        }
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !seen_paths.insert(normalized) {
            return Err("Lua package contains duplicate paths".to_string());
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err("Lua package expands beyond the safety limit".to_string());
        }
        let is_target_lua = path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(&format!("{appid}.lua")));
        if is_target_lua {
            if source_lua.is_some() {
                return Err("Lua package contains duplicate root Lua files".to_string());
            }
            if entry.size() > MAX_LUA_BYTES as u64 {
                return Err("Lua source exceeds the safety limit".to_string());
            }
            let entry_size = entry.size();
            let mut bytes = Vec::with_capacity(entry_size as usize);
            entry
                .by_ref()
                .take(entry_size.saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(|error| format!("Could not read Lua source: {error}"))?;
            source_lua = Some(
                String::from_utf8(bytes)
                    .map_err(|_| "Lua source is not valid UTF-8".to_string())?,
            );
        }
    }
    drop(archive);
    let source_lua =
        source_lua.ok_or_else(|| format!("Lua package does not contain {appid}.lua"))?;
    let (canonical_lua, expected_manifests) = canonicalize_lua(appid, &source_lua)?;
    let manifests = inspect_manifest_archive(archive_bytes)?;
    let actual = manifests
        .iter()
        .map(|value| (value.depot_id, value.manifest_gid.clone()))
        .collect::<BTreeMap<_, _>>();
    for (depot_id, gid) in &expected_manifests {
        if actual.get(depot_id) != Some(gid) {
            return Err(format!(
                "Lua package is missing manifest {depot_id}_{gid}.manifest"
            ));
        }
    }
    let canonical_archive = build_canonical_archive(appid, provider, &canonical_lua, &manifests)?;
    Ok(CanonicalPackage {
        appid,
        provider,
        revision: sha256_bytes(&canonical_archive),
        canonical_lua,
        manifests,
        archive_bytes: canonical_archive,
    })
}

fn community_index_package_path(payload: &Value, appid: u32) -> Option<String> {
    value_at(
        payload,
        &[
            &["packagePath"],
            &["package_path"],
            &["latest", "packagePath"],
            &["latest", "path"],
        ],
    )
    .and_then(Value::as_str)
    .map(ToOwned::to_owned)
    .or_else(|| {
        value_at(
            payload,
            &[&["latestRevision"], &["latest_revision"], &["revision"]],
        )
        .and_then(Value::as_str)
        .map(|revision| format!("community/packages/{appid}/{revision}.zip"))
    })
}

pub(crate) fn fetch_community_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let client = source_client(Duration::from_secs(90))?;
    let Some(index_bytes) = download_public_to_part(
        app,
        LuaSourceProvider::HuggingFace,
        appid,
        &client,
        &format!("{HF_RAW_BASE}/community/index/{appid}.json"),
        &["huggingface.co"],
        "community-index.json",
        256 * 1024,
    )?
    else {
        return Ok(None);
    };
    let payload: Value = serde_json::from_slice(&index_bytes)
        .map_err(|_| "Community package index is invalid".to_string())?;
    if payload
        .get("appid")
        .and_then(Value::as_u64)
        .is_some_and(|value| value != appid as u64)
    {
        return Err("Community package index belongs to another AppID".to_string());
    }
    let path = community_index_package_path(&payload, appid)
        .ok_or_else(|| "Community package index is missing its package path".to_string())?;
    if path.starts_with('/') || path.contains("..") || !path.ends_with(".zip") {
        return Err("Community package index contains an unsafe package path".to_string());
    }
    let Some(bytes) = download_public_to_part(
        app,
        LuaSourceProvider::HuggingFace,
        appid,
        &client,
        &format!("{HF_RAW_BASE}/{path}"),
        &["huggingface.co"],
        "community.zip",
        MAX_ARCHIVE_BYTES,
    )?
    else {
        return Err("Community package is missing".to_string());
    };
    package_from_archive(appid, LuaPackageProvider::Community, &bytes).map(Some)
}

pub(crate) fn fetch_sushi_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.sushi_enabled {
        return Ok(None);
    }
    let client = source_client(Duration::from_secs(90))?;
    let Some(bytes) = download_public_to_part(
        app,
        LuaSourceProvider::Sushi,
        appid,
        &client,
        &format!("{SUSHI_BASE}/{appid}.zip"),
        &["raw.githubusercontent.com"],
        "sushi.zip",
        MAX_ARCHIVE_BYTES,
    )?
    else {
        return Ok(None);
    };
    package_from_archive(appid, LuaPackageProvider::Sushi, &bytes).map(Some)
}

pub(crate) fn fetch_ryuu_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.ryuu_enabled {
        return Ok(None);
    }
    let client = ryuu_client(Duration::from_secs(90))?;
    let mut response = client
        .get(ryuu_url(appid)?)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("Could not reach Ryuu: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("Ryuu returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE_BYTES)
    {
        return Err("Ryuu package exceeds the download safety limit".to_string());
    }
    let dir = cache_dir(app, LuaSourceProvider::Ryuu, appid)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not prepare Lua package cache: {error}"))?;
    let part = dir.join("ryuu.zip.part");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&part)
        .map_err(|error| format!("Could not create Ryuu package cache: {error}"))?;
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Could not download Ryuu package: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_ARCHIVE_BYTES {
            let _ = fs::remove_file(&part);
            return Err("Ryuu package exceeds the download safety limit".to_string());
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not cache Ryuu package: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not flush Ryuu package cache: {error}"))?;
    drop(file);
    let bytes =
        fs::read(&part).map_err(|error| format!("Could not read Ryuu package cache: {error}"))?;
    package_from_archive(appid, LuaPackageProvider::Ryuu, &bytes).map(Some)
}

pub(crate) fn fetch_hubcap_package(
    app: &AppHandle,
    appid: u32,
) -> Result<CanonicalPackage, String> {
    if appid == 0 {
        return Err("AppID must be greater than zero".to_string());
    }
    let settings = load_settings(app)?;
    let key = decrypt_hubcap_key(&settings)?.ok_or_else(|| "HUBCAP_KEY_REQUIRED".to_string())?;
    let client = source_client(Duration::from_secs(120))?;
    let status_response = hubcap_get(&client, &key, &format!("/api/v1/status/{appid}"))?;
    if status_response.status() == StatusCode::UNAUTHORIZED
        || status_response.status() == StatusCode::FORBIDDEN
    {
        return Err("HUBCAP_KEY_INVALID".to_string());
    }
    let package_available = if status_response.status().is_success() {
        let payload: Value = status_response
            .json()
            .map_err(|_| "Hubcap status response is invalid".to_string())?;
        value_at(
            &payload,
            &[&["manifest_file_exists"], &["available"], &["exists"]],
        )
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            value_at(&payload, &[&["status"]])
                .and_then(Value::as_str)
                .is_some_and(|status| {
                    matches!(
                        status.to_ascii_lowercase().as_str(),
                        "ready" | "available" | "ok" | "cached"
                    )
                })
        })
    } else if status_response.status() == StatusCode::NOT_FOUND {
        false
    } else {
        return Err(format!("HUBCAP_HTTP_{}", status_response.status().as_u16()));
    };

    if !package_available {
        let generated = download_to_part(
            app,
            LuaSourceProvider::Hubcap,
            appid,
            &client,
            &key,
            &format!("/api/v1/generate/appmanifest/{appid}?branch=public"),
            "generated-app-bundle.zip",
            MAX_ARCHIVE_BYTES,
        )?;
        let package = package_from_archive(appid, LuaPackageProvider::Hubcap, &generated)?;
        let final_path = cache_dir(app, LuaSourceProvider::Hubcap, appid)?
            .join(format!("{}.zip", package.revision));
        crate::lua_live::atomic_write_path(&final_path, &package.archive_bytes)?;
        return Ok(package);
    }
    let lua_bytes = download_to_part(
        app,
        LuaSourceProvider::Hubcap,
        appid,
        &client,
        &key,
        &format!("/api/v1/lua/{appid}"),
        "source.lua",
        MAX_LUA_BYTES as u64,
    )?;
    let lua_source = String::from_utf8(lua_bytes)
        .map_err(|_| "Hubcap Lua source is not valid UTF-8".to_string())?;
    let (canonical_lua, expected_manifests) = canonicalize_lua(appid, &lua_source)?;
    let manifest_zip = download_to_part(
        app,
        LuaSourceProvider::Hubcap,
        appid,
        &client,
        &key,
        &format!("/api/v1/manifest/{appid}"),
        "manifest.zip",
        MAX_ARCHIVE_BYTES,
    )?;
    let manifests = inspect_manifest_archive(&manifest_zip)?;
    let actual = manifests
        .iter()
        .map(|value| (value.depot_id, value.manifest_gid.clone()))
        .collect::<BTreeMap<_, _>>();
    for (depot_id, gid) in &expected_manifests {
        if actual.get(depot_id) != Some(gid) {
            return Err(format!(
                "Hubcap package is missing manifest {depot_id}_{gid}.manifest"
            ));
        }
    }
    let archive_bytes = build_canonical_archive(
        appid,
        LuaPackageProvider::Hubcap,
        &canonical_lua,
        &manifests,
    )?;
    let revision = sha256_bytes(&archive_bytes);
    let final_path =
        cache_dir(app, LuaSourceProvider::Hubcap, appid)?.join(format!("{revision}.zip"));
    crate::lua_live::atomic_write_path(&final_path, &archive_bytes)?;
    Ok(CanonicalPackage {
        appid,
        provider: LuaPackageProvider::Hubcap,
        revision,
        canonical_lua,
        manifests,
        archive_bytes,
    })
}

pub(crate) fn live_lua_from_hubcap(
    app: &AppHandle,
    appid: u32,
) -> Result<CanonicalPackage, String> {
    fetch_hubcap_package(app, appid)
}

pub(crate) fn contribute_package_async(app: AppHandle, package: CanonicalPackage) {
    std::thread::spawn(move || {
        let token = match crate::discord_auth::access_token_for_backend(&app) {
            Ok(value) => value,
            Err(_) => return,
        };
        let client = match backend_client() {
            Ok(value) => value,
            Err(_) => return,
        };
        let request_id = Uuid::new_v4().to_string();
        let response = client
            .post(format!("{BACKEND_BASE}/community/contributions"))
            .bearer_auth(token)
            .header("Content-Type", "application/zip")
            .header("X-Request-Id", request_id)
            .header("X-App-Id", package.appid.to_string())
            .header("X-Revision", package.revision)
            .body(package.archive_bytes)
            .send();
        if let Ok(response) = response {
            if !response.status().is_success() && response.status() != StatusCode::CONFLICT {
                crate::debug_log::debug_log(&format!(
                    "Lua community contribution failed for AppID {}: HTTP {}",
                    package.appid,
                    response.status()
                ));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hubcap_user_stats_parse_daily_quota_and_expiry() {
        let payload = json!({
            "daily_usage": 42,
            "daily_limit": 1500,
            "api_key_expires_at": "2026-08-19T12:00:00"
        });
        let daily = daily_usage_bucket(&payload);
        assert_eq!(daily.usage, Some(42));
        assert_eq!(daily.limit, Some(1500));
        assert_eq!(daily.remaining, Some(1458));
        assert_eq!(
            parse_expiry(&payload).map(|value| value.to_rfc3339()),
            Some("2026-08-19T12:00:00+00:00".to_string())
        );
    }

    #[test]
    fn canonicalizer_accepts_supported_hubcap_declarations() {
        let source = r#"
            -- metadata
            addappid(10)
            addappid(11, 1, "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
            setManifestid(11, "123456789", 4096)
            setStat(10)
            addProcess(10, "game.exe")
        "#;
        let (canonical, manifests) = canonicalize_lua(10, source).unwrap();
        assert!(canonical.contains("addappid(10)"));
        assert!(canonical.contains("setManifestid(11, \"123456789\", 4096)"));
        assert_eq!(manifests.get(&11).map(String::as_str), Some("123456789"));
    }

    #[test]
    fn canonicalizer_rejects_arbitrary_lua() {
        let source = "addappid(10)\nos.execute('calc.exe')\n";
        assert!(canonicalize_lua(10, source).is_err());
    }

    #[test]
    fn canonicalizer_rejects_wrong_root_appid() {
        assert!(canonicalize_lua(10, "addappid(11)\n").is_err());
    }

    #[test]
    fn manifest_name_requires_decimal_depot_and_gid() {
        assert_eq!(
            parse_manifest_name("123_456.manifest"),
            Some((123, "456".to_string()))
        );
        assert!(parse_manifest_name("123_latest.manifest").is_none());
        assert!(parse_manifest_name("../123_456.manifest").is_none());
    }

    #[test]
    fn unsafe_archive_paths_are_rejected() {
        assert!(validate_archive_path(Path::new("../escape.manifest")).is_err());
        assert!(validate_archive_path(Path::new("manifests/1_2.manifest")).is_ok());
    }

    #[test]
    fn source_provider_accepts_only_its_own_package_variants() {
        assert!(LuaSourceProvider::HuggingFace.accepts(LuaPackageProvider::Curated));
        assert!(LuaSourceProvider::HuggingFace.accepts(LuaPackageProvider::Community));
        assert!(!LuaSourceProvider::HuggingFace.accepts(LuaPackageProvider::Hubcap));

        assert!(LuaSourceProvider::Hubcap.accepts(LuaPackageProvider::Hubcap));
        assert!(!LuaSourceProvider::Hubcap.accepts(LuaPackageProvider::Community));

        assert!(LuaSourceProvider::Sushi.accepts(LuaPackageProvider::Sushi));
        assert!(!LuaSourceProvider::Sushi.accepts(LuaPackageProvider::Ryuu));

        assert!(LuaSourceProvider::Ryuu.accepts(LuaPackageProvider::Ryuu));
        assert!(!LuaSourceProvider::Ryuu.accepts(LuaPackageProvider::Sushi));
    }

    #[test]
    fn package_provider_maps_to_exact_source_provider() {
        assert_eq!(
            LuaPackageProvider::Curated.source(),
            Some(LuaSourceProvider::HuggingFace)
        );
        assert_eq!(
            LuaPackageProvider::Community.source(),
            Some(LuaSourceProvider::HuggingFace)
        );
        assert_eq!(
            LuaPackageProvider::Hubcap.source(),
            Some(LuaSourceProvider::Hubcap)
        );
        assert_eq!(
            LuaPackageProvider::Sushi.source(),
            Some(LuaSourceProvider::Sushi)
        );
        assert_eq!(
            LuaPackageProvider::Ryuu.source(),
            Some(LuaSourceProvider::Ryuu)
        );
        assert_eq!(LuaPackageProvider::None.source(), None);
    }

    #[test]
    fn source_cache_names_are_provider_namespaced() {
        assert_eq!(LuaSourceProvider::HuggingFace.cache_name(), "hugging-face");
        assert_eq!(LuaSourceProvider::Hubcap.cache_name(), "hubcap");
        assert_eq!(LuaSourceProvider::Sushi.cache_name(), "sushi");
        assert_eq!(LuaSourceProvider::Ryuu.cache_name(), "ryuu");
    }
}
