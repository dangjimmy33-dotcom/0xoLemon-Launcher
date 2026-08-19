use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CACHE_CONTROL, ETAG, LAST_MODIFIED, RANGE};
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const HUBCAP_BASE: &str = "https://hubcapmanifest.com";
const BACKEND_BASE: &str = "https://zeroxolemon-launcher.onrender.com/api/0xolemon/lua-shop";
const HF_RAW_BASE: &str = "https://huggingface.co/datasets/Immaking/Luas/resolve/main";
const SUSHI_GITHUB_CONTENTS_BASE: &str =
    "https://api.github.com/repos/sushi-dev55-alt/sushitools-games-repo-alt/contents";
const RYUU_BASE: &str = "https://generator.ryuu.lol";
const DEPOTBOX_BASE: &str = "https://depotbox.org";
const SKYFLARE_GITHUB_CONTENTS_BASE: &str =
    "https://api.github.com/repos/skyflarefox/Skyapi/contents";
const LUATOOLS_API_BASE: &str = "https://lua.tools";
const LUATOOLS_SUPABASE_URL: &str = "https://db.lua.tools";
// Public Supabase anon client key shipped by the official LuaTools desktop app/web bundle.
const LUATOOLS_SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpYXQiOjE3NzYwMzkzNzYsImV4cCI6MTg5MzQ1NjAwMCwicm9sZSI6ImFub24iLCJpc3MiOiJzdXBhYmFzZSJ9.f_-K38u3odjltP-g_67FVmG32Vg-_-k-lNBvIaVUVBM";
const LUATOOLS_MANIFEST_BACKEND: &str = "http://167.235.229.108";
const LUATOOLS_MANIFEST_USER_AGENT: &str = "secretgoonpoon";
const LUATOOLS_OAUTH_PORT: u16 = 53789;
const LUATOOLS_OAUTH_CALLBACK: &str = "http://localhost:53789/callback";
const HUBCAP_FREE_DAILY_LIMIT: u64 = 25;
const GITHUB_MANIFEST_MIRRORS: [&str; 3] = [
    "https://raw.githubusercontent.com/qwe213312/k25FCdfEOoEJ42S6/main",
    "https://raw.githubusercontent.com/mejikuhibiniu1/k25FCdfEOoEJ42S6/main",
    "https://raw.githubusercontent.com/Sainan/k25FCdfEOoEJ42S6/main",
];
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
    #[serde(default)]
    encrypted_ryuu_key: Option<String>,
    #[serde(default)]
    ryuu_key_saved_at: Option<String>,
    #[serde(default)]
    encrypted_depotbox_key: Option<String>,
    #[serde(default)]
    depotbox_key_saved_at: Option<String>,
    encrypted_manifesthub_key: Option<String>,
    manifesthub_key_saved_at: Option<String>,
    #[serde(default = "default_sushi_enabled")]
    sushi_enabled: bool,
    #[serde(default = "default_github_mirrors_enabled")]
    github_mirrors_enabled: bool,
    #[serde(default = "default_openlua_enabled")]
    openlua_enabled: bool,
    #[serde(default = "default_steamtools_enabled")]
    steamtools_enabled: bool,
    #[serde(default)]
    ryuu_enabled: bool,
    #[serde(default = "default_luatools_dynamic_enabled")]
    luie_enabled: bool,
    #[serde(default = "default_luatools_dynamic_enabled")]
    twenty_two_cloud_enabled: bool,
    #[serde(default = "default_luatools_dynamic_enabled")]
    skyflare_enabled: bool,
}

fn default_sushi_enabled() -> bool {
    true
}

fn default_github_mirrors_enabled() -> bool {
    true
}

fn default_openlua_enabled() -> bool {
    true
}

fn default_steamtools_enabled() -> bool {
    true
}

fn default_luatools_dynamic_enabled() -> bool {
    true
}

impl Default for LuaSourceSettingsDisk {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            encrypted_hubcap_key: None,
            key_saved_at: None,
            encrypted_ryuu_key: None,
            ryuu_key_saved_at: None,
            encrypted_depotbox_key: None,
            depotbox_key_saved_at: None,
            encrypted_manifesthub_key: None,
            manifesthub_key_saved_at: None,
            sushi_enabled: true,
            github_mirrors_enabled: true,
            openlua_enabled: true,
            steamtools_enabled: true,
            ryuu_enabled: false,
            luie_enabled: true,
            twenty_two_cloud_enabled: true,
            skyflare_enabled: true,
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
    pub manifesthub_key: Option<String>,
    pub manifesthub_configured: bool,
    pub ryuu_key: Option<String>,
    pub ryuu_configured: bool,
    pub depotbox_key: Option<String>,
    pub depotbox_configured: bool,
    pub sushi_enabled: bool,
    pub github_mirrors_enabled: bool,
    pub openlua_enabled: bool,
    pub steamtools_enabled: bool,
    pub ryuu_enabled: bool,
    pub luie_enabled: bool,
    pub twenty_two_cloud_enabled: bool,
    pub skyflare_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourcePreferencesRequest {
    pub sushi_enabled: bool,
    pub github_mirrors_enabled: bool,
    pub openlua_enabled: bool,
    pub steamtools_enabled: bool,
    pub ryuu_enabled: bool,
    pub luie_enabled: bool,
    pub twenty_two_cloud_enabled: bool,
    pub skyflare_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaPackageProvider {
    Curated,
    Community,
    Hubcap,
    Sushi,
    #[serde(rename = "githubMirrors", alias = "gitHubMirrors")]
    GitHubMirrors,
    #[serde(rename = "openLua", alias = "openlua")]
    OpenLua,
    #[serde(rename = "steamTools", alias = "steamtools")]
    SteamTools,
    Ryuu,
    Luie,
    #[serde(rename = "twentyTwoCloud", alias = "twentytwoCloud")]
    TwentyTwoCloud,
    Skyflare,
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
    #[serde(rename = "githubMirrors", alias = "gitHubMirrors")]
    GitHubMirrors,
    #[serde(rename = "openLua", alias = "openlua")]
    OpenLua,
    #[serde(rename = "steamTools", alias = "steamtools")]
    SteamTools,
    Ryuu,
    Luie,
    #[serde(rename = "twentyTwoCloud", alias = "twentytwoCloud")]
    TwentyTwoCloud,
    Skyflare,
}

impl LuaSourceProvider {
    pub(crate) fn cache_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "hugging-face",
            Self::Hubcap => "hubcap",
            Self::Sushi => "sushi",
            Self::GitHubMirrors => "github-mirrors",
            Self::OpenLua => "open-lua",
            Self::SteamTools => "steam-tools",
            Self::Ryuu => "ryuu",
            Self::Luie => "luie",
            // Legacy enum identity kept for backwards-compatible state decoding;
            // the source itself is DepotBox now.
            Self::TwentyTwoCloud => "depotbox",
            Self::Skyflare => "skyflare",
        }
    }

    pub(crate) fn supports_live(self) -> bool {
        matches!(
            self,
            Self::HuggingFace
                | Self::Hubcap
                | Self::Sushi
                | Self::OpenLua
                | Self::Ryuu
                | Self::Luie
                | Self::TwentyTwoCloud
                | Self::Skyflare
        )
    }

    pub(crate) fn supports_locked(self) -> bool {
        matches!(
            self,
            Self::Hubcap
                | Self::GitHubMirrors
                | Self::SteamTools
                | Self::Ryuu
                | Self::TwentyTwoCloud
        )
    }

    pub(crate) fn accepts(self, provider: LuaPackageProvider) -> bool {
        if provider == LuaPackageProvider::None {
            return false;
        }
        matches!(
            (self, provider),
            (
                Self::HuggingFace,
                LuaPackageProvider::Curated | LuaPackageProvider::Community
            ) | (Self::Hubcap, LuaPackageProvider::Hubcap)
                | (Self::Sushi, LuaPackageProvider::Sushi)
                | (Self::GitHubMirrors, LuaPackageProvider::GitHubMirrors)
                | (Self::OpenLua, LuaPackageProvider::OpenLua)
                | (Self::SteamTools, LuaPackageProvider::SteamTools)
                | (Self::Ryuu, LuaPackageProvider::Ryuu)
                | (Self::Luie, LuaPackageProvider::Luie)
                | (Self::TwentyTwoCloud, LuaPackageProvider::TwentyTwoCloud)
                | (Self::Skyflare, LuaPackageProvider::Skyflare)
        )
    }
}

impl LuaPackageProvider {
    pub(crate) fn source(self) -> Option<LuaSourceProvider> {
        match self {
            Self::Curated | Self::Community => Some(LuaSourceProvider::HuggingFace),
            Self::Hubcap => Some(LuaSourceProvider::Hubcap),
            Self::Sushi => Some(LuaSourceProvider::Sushi),
            Self::GitHubMirrors => Some(LuaSourceProvider::GitHubMirrors),
            Self::OpenLua => Some(LuaSourceProvider::OpenLua),
            Self::SteamTools => Some(LuaSourceProvider::SteamTools),
            Self::Ryuu => Some(LuaSourceProvider::Ryuu),
            Self::Luie => Some(LuaSourceProvider::Luie),
            Self::TwentyTwoCloud => Some(LuaSourceProvider::TwentyTwoCloud),
            Self::Skyflare => Some(LuaSourceProvider::Skyflare),
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

fn decrypt_ryuu_key(settings: &LuaSourceSettingsDisk) -> Result<Option<String>, String> {
    let Some(encoded) = settings.encrypted_ryuu_key.as_deref() else {
        return Ok(None);
    };
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "Stored Ryuu key is invalid".to_string())?;
    let clear = crate::secret_store::unprotect(&encrypted)?;
    let key = String::from_utf8(clear).map_err(|_| "Stored Ryuu key is invalid".to_string())?;
    if key.trim().is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("Stored Ryuu key is invalid".to_string());
    }
    Ok(Some(key))
}

fn decrypt_depotbox_key(settings: &LuaSourceSettingsDisk) -> Result<Option<String>, String> {
    let Some(encoded) = settings.encrypted_depotbox_key.as_deref() else {
        return Ok(None);
    };
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "Stored DepotBox key is invalid".to_string())?;
    let clear = crate::secret_store::unprotect(&encrypted)?;
    let key = String::from_utf8(clear)
        .map_err(|_| "Stored DepotBox key is invalid".to_string())?;
    if key.trim().is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_whitespace) {
        return Err("Stored DepotBox key is invalid".to_string());
    }
    Ok(Some(key))
}

fn decrypt_manifesthub_key(settings: &LuaSourceSettingsDisk) -> Result<Option<String>, String> {
    let Some(encoded) = settings.encrypted_manifesthub_key.as_deref() else {
        return Ok(None);
    };
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "Stored ManifestHub key is invalid".to_string())?;
    let clear = crate::secret_store::unprotect(&encrypted)?;
    let key = String::from_utf8(clear).map_err(|_| "Stored ManifestHub key is invalid".to_string())?;
    if key.trim().is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("Stored ManifestHub key is invalid".to_string());
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
        .redirect(Policy::limited(8))
        .user_agent(concat!(
            "0xoLemon/",
            env!("CARGO_PKG_VERSION"),
            " LuaSources-RyuuOfficial"
        ))
        .build()
        .map_err(|error| format!("Could not initialize Ryuu client: {error}"))
}

fn ryuu_url(appid: u32) -> Result<Url, String> {
    let parsed = Url::parse(&format!("{RYUU_BASE}/api/download/{appid}"))
        .map_err(|error| format!("Invalid Ryuu source URL: {error}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("generator.ryuu.lol")
        || parsed.port().is_some()
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
    // Hubcap is the only source whose quota is surfaced by the launcher.
    // Prefer the role/custom limit returned by Hubcap; when an older response
    // omits it, use the documented/default free cap expected by this launcher.
    let limit = value_u64(
        value,
        &[
            &["daily_limit"],
            &["custom_api_limit"],
            &["role_daily_limit"],
        ],
    )
    .or(Some(HUBCAP_FREE_DAILY_LIMIT));
    let remaining = limit.map(|limit| limit.saturating_sub(usage.unwrap_or(0)));
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

fn local_hubcap_state(settings: &LuaSourceSettingsDisk) -> Result<HubcapKeyState, String> {
    let Some(key) = decrypt_hubcap_key(settings)? else {
        return Ok(HubcapKeyState::default());
    };
    let mut state = HubcapKeyState {
        configured: true,
        valid: true,
        masked_key: Some(mask_key(&key)),
        service_ready: true,
        daily: HubcapUsageBucket {
            usage: Some(0),
            limit: Some(HUBCAP_FREE_DAILY_LIMIT),
            remaining: Some(HUBCAP_FREE_DAILY_LIMIT),
        },
        last_checked_at: settings.key_saved_at.clone(),
        ..HubcapKeyState::default()
    };
    apply_expiry(&mut state, estimate_expiry(settings), true);
    Ok(state)
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
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        state.last_error = Some("HUBCAP_RATE_LIMITED".to_string());
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
    state.daily = daily_usage_bucket(&payload);
    let explicit_expiry = parse_expiry(&payload);
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

fn get_lua_source_settings_internal(app: &AppHandle) -> Result<LuaSourceSettingsState, String> {
    let settings = load_settings(app)?;
    let manifesthub_key = decrypt_manifesthub_key(&settings).ok().flatten();
    let manifesthub_configured = manifesthub_key.is_some();
    let masked_key = manifesthub_key.map(|k| {
        if k.len() <= 8 {
            "********".to_string()
        } else {
            format!("{}...{}", &k[..4], &k[k.len() - 4..])
        }
    });
    let ryuu_key = decrypt_ryuu_key(&settings).ok().flatten();
    let ryuu_configured = ryuu_key.is_some();
    let ryuu_masked = ryuu_key.as_deref().map(mask_key);
    let depotbox_key = decrypt_depotbox_key(&settings).ok().flatten();
    let depotbox_configured = depotbox_key.is_some();
    let depotbox_masked = depotbox_key.as_deref().map(mask_key);
    Ok(LuaSourceSettingsState {
        // Reading Settings/Lua Shop must be side-effect free. Hubcap is contacted
        // only when the user explicitly tests/saves the key or downloads a bundle.
        hubcap: local_hubcap_state(&settings)?,
        manifesthub_key: masked_key,
        manifesthub_configured,
        ryuu_key: ryuu_masked,
        ryuu_configured,
        depotbox_key: depotbox_masked,
        depotbox_configured,
        sushi_enabled: settings.sushi_enabled,
        github_mirrors_enabled: settings.github_mirrors_enabled,
        openlua_enabled: settings.openlua_enabled,
        steamtools_enabled: settings.steamtools_enabled,
        ryuu_enabled: settings.ryuu_enabled,
        luie_enabled: settings.luie_enabled,
        twenty_two_cloud_enabled: settings.twenty_two_cloud_enabled,
        skyflare_enabled: settings.skyflare_enabled,
    })
}

#[tauri::command]
pub async fn get_lua_source_settings(app: AppHandle) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || get_lua_source_settings_internal(&app))
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
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err("HUBCAP_RATE_LIMITED".to_string());
        }
        if !response.status().is_success() {
            return Err(format!("HUBCAP_HTTP_{}", response.status().as_u16()));
        }
        let payload: Value = response
            .json()
            .map_err(|_| "Hubcap usage response is invalid".to_string())?;
        let encrypted = crate::secret_store::protect(key.as_bytes())?;
        let mut settings = load_settings(&app)?;
        settings.encrypted_hubcap_key = Some(STANDARD.encode(encrypted));
        settings.key_saved_at = Some(Utc::now().to_rfc3339());
        save_settings(&app, &settings)?;

        // The save request itself already proved the key. Reusing that response
        // avoids an immediate usage + user/stats request burst after Save.
        let mut state = HubcapKeyState {
            configured: true,
            valid: true,
            masked_key: Some(mask_key(key)),
            last_checked_at: Some(Utc::now().to_rfc3339()),
            service_ready: value_at(&payload, &[&["steam_service_ready"], &["ready"]])
                .and_then(Value::as_bool)
                .unwrap_or(true),
            single: usage_bucket(&payload, "single"),
            bundle: usage_bucket(&payload, "bundle"),
            workshop: usage_bucket(&payload, "workshop"),
            daily: daily_usage_bucket(&payload),
            ..HubcapKeyState::default()
        };
        let expiry = parse_expiry(&payload).or_else(|| estimate_expiry(&settings));
        apply_expiry(&mut state, expiry, parse_expiry(&payload).is_none());
        Ok(state)
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
pub async fn save_ryuu_auth_key(
    app: AppHandle,
    api_key: String,
) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = api_key.trim();
        if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_whitespace) {
            return Err("RYUU_KEY_INVALID".to_string());
        }
        let encrypted = crate::secret_store::protect(key.as_bytes())?;
        let mut settings = load_settings(&app)?;
        settings.encrypted_ryuu_key = Some(STANDARD.encode(encrypted));
        settings.ryuu_key_saved_at = Some(Utc::now().to_rfc3339());
        settings.ryuu_enabled = true;
        save_settings(&app, &settings)?;
        get_lua_source_settings_internal(&app)
    })
    .await
    .map_err(|error| format!("Ryuu key task failed: {error}"))?
}

#[tauri::command]
pub fn clear_ryuu_auth_key(app: AppHandle) -> Result<LuaSourceSettingsState, String> {
    let mut settings = load_settings(&app)?;
    settings.encrypted_ryuu_key = None;
    settings.ryuu_key_saved_at = None;
    save_settings(&app, &settings)?;
    get_lua_source_settings_internal(&app)
}

#[tauri::command]
pub async fn save_depotbox_api_key(
    app: AppHandle,
    api_key: String,
) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = api_key.trim();
        if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_whitespace) {
            return Err("DEPOTBOX_KEY_INVALID".to_string());
        }
        let encrypted = crate::secret_store::protect(key.as_bytes())?;
        let mut settings = load_settings(&app)?;
        settings.encrypted_depotbox_key = Some(STANDARD.encode(encrypted));
        settings.depotbox_key_saved_at = Some(Utc::now().to_rfc3339());
        settings.twenty_two_cloud_enabled = true;
        save_settings(&app, &settings)?;
        get_lua_source_settings_internal(&app)
    })
    .await
    .map_err(|error| format!("DepotBox key task failed: {error}"))?
}

#[tauri::command]
pub fn clear_depotbox_api_key(app: AppHandle) -> Result<LuaSourceSettingsState, String> {
    let mut settings = load_settings(&app)?;
    settings.encrypted_depotbox_key = None;
    settings.depotbox_key_saved_at = None;
    save_settings(&app, &settings)?;
    get_lua_source_settings_internal(&app)
}

#[tauri::command]
pub async fn save_manifesthub_api_key(
    app: AppHandle,
    api_key: String,
) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = api_key.trim();
        if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_whitespace) {
            return Err("KEY_INVALID".to_string());
        }
        let encrypted = crate::secret_store::protect(key.as_bytes())?;
        let mut settings = load_settings(&app)?;
        settings.encrypted_manifesthub_key = Some(STANDARD.encode(encrypted));
        settings.manifesthub_key_saved_at = Some(Utc::now().to_rfc3339());
        save_settings(&app, &settings)?;
        get_lua_source_settings_internal(&app)
    })
    .await
    .map_err(|error| format!("Task failed: {error}"))?
}

#[tauri::command]
pub async fn clear_manifesthub_api_key(app: AppHandle) -> Result<LuaSourceSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut settings = load_settings(&app)?;
        settings.encrypted_manifesthub_key = None;
        settings.manifesthub_key_saved_at = None;
        save_settings(&app, &settings)?;
        get_lua_source_settings_internal(&app)
    })
    .await
    .map_err(|error| format!("Task failed: {error}"))?
}

#[tauri::command]
pub async fn test_manifesthub_api_key(app: AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&app)?;
        let Some(key) = decrypt_manifesthub_key(&settings)? else {
            return Err("KEY_REQUIRED".to_string());
        };
        let client = source_client(Duration::from_secs(12))?;
        let url = format!(
            "https://api.manifesthub2.filegear-sg.me/manifest?apikey={}&depotid=731&manifestid=1",
            key.trim()
        );
        let res = client.get(url).send().map_err(|_| "NETWORK_ERROR".to_string())?;
        if res.status() == StatusCode::FORBIDDEN || res.status() == StatusCode::UNAUTHORIZED {
            return Err("KEY_INVALID".to_string());
        }
        Ok(true)
    })
    .await
    .map_err(|error| format!("Task failed: {error}"))?
}

#[tauri::command]
pub async fn fetch_donor_steamid(app: AppHandle, appid: u32) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_donor_steamid_blocking(&app, appid))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

pub(crate) fn fetch_donor_steamid_blocking(
    app: &AppHandle,
    appid: u32,
) -> Result<String, String> {
    use tauri::{Listener, Manager, WebviewUrl, WebviewWindowBuilder};

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx_clone = tx.clone();
    
    let window_label = format!("steamid-fetch-{}", appid);
    let event_id = app.listen_any("steamid-result", move |event| {
        let payload = event.payload();
        // Remove quotes from JSON payload if present
        let payload = payload.trim_matches('"');
        if let Ok(mut sender) = tx_clone.lock() {
            if let Some(tx) = sender.take() {
                let _ = tx.send(payload.to_string());
            }
        }
    });

    let inject_js = r#"
        (function() {
            var interval = setInterval(function() {
                if (window.location.href.includes('/leaderboards')) {
                    var firstRowLink = document.querySelector('table tbody tr a[href^="/users/"]');
                    if (firstRowLink) {
                        var href = firstRowLink.getAttribute('href');
                        var parts = href.split('/');
                        var id = parts[parts.length - 1];
                        if (id.length === 17 && /^\d+$/.test(id)) {
                            clearInterval(interval);
                            window.__TAURI__.event.emit('steamid-result', id);
                        } else {
                            firstRowLink.click();
                        }
                    }
                } else if (window.location.href.includes('/users/')) {
                    var steamLink = document.querySelector('a[href^="https://steamcommunity.com/profiles/"]');
                    if (steamLink) {
                        var href = steamLink.getAttribute('href');
                        var parts = href.split('/');
                        var id = parts[parts.length - 1] || parts[parts.length - 2];
                        if (id && id.length === 17 && /^\d+$/.test(id)) {
                            clearInterval(interval);
                            window.__TAURI__.event.emit('steamid-result', id);
                        }
                    }
                }
            }, 1000);
        })();
    "#;
    
    let window_result = WebviewWindowBuilder::new(
        app,
        &window_label,
        WebviewUrl::External(format!("https://steamhunters.com/apps/{}/leaderboards", appid).parse().unwrap())
    )
    .title(format!("SteamHunters Fetcher - {}", appid))
    .inner_size(800.0, 600.0)
    .initialization_script(inject_js)
    .build();

    if let Err(e) = window_result {
        app.unlisten(event_id);
        return Err(format!("Failed to open window: {}", e));
    }

    let result = match rx.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(res) => Ok(res),
        Err(_) => Err("Timeout waiting for SteamID (60s). Please ensure Cloudflare allowed access.".to_string()),
    };

    app.unlisten(event_id);
    if let Some(w) = app.get_webview_window(&window_label) {
        let _ = w.close();
    }

    result
}

#[tauri::command]
pub fn set_lua_source_preferences(
    app: AppHandle,
    request: LuaSourcePreferencesRequest,
) -> Result<LuaSourceSettingsState, String> {
    let mut settings = load_settings(&app)?;
    settings.sushi_enabled = request.sushi_enabled;
    settings.github_mirrors_enabled = request.github_mirrors_enabled;
    settings.openlua_enabled = request.openlua_enabled;
    settings.steamtools_enabled = request.steamtools_enabled;
    settings.ryuu_enabled = request.ryuu_enabled;
    settings.luie_enabled = request.luie_enabled;
    settings.twenty_two_cloud_enabled = request.twenty_two_cloud_enabled;
    settings.skyflare_enabled = request.skyflare_enabled;
    save_settings(&app, &settings)?;
    get_lua_source_settings_internal(&app)
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
        requires_key: matches!(provider, LuaSourceProvider::Hubcap | LuaSourceProvider::Ryuu),
        key_ready: !matches!(provider, LuaSourceProvider::Hubcap | LuaSourceProvider::Ryuu),
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

fn probe_hubcap_source(app: &AppHandle, _appid: u32) -> LuaSourceCandidate {
    // The source picker must never burn Hubcap request quota just to render a card.
    // A stored key was already validated when it was saved; the real bundle request
    // is the authoritative availability check. This keeps selecting Hubcap to one
    // provider request instead of status -> retry -> status -> generate chains.
    let mut candidate = empty_source_candidate(LuaSourceProvider::Hubcap);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    match decrypt_hubcap_key(&settings) {
        Ok(Some(_)) => {
            candidate.key_ready = true;
            candidate.available = true;
            candidate.on_demand = true;
            candidate.variant = Some(LuaPackageProvider::Hubcap);
        }
        Ok(None) => {
            candidate.error_code = Some("HUBCAP_KEY_REQUIRED".to_string());
        }
        Err(error) => {
            candidate.error_code = Some(error);
        }
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
    candidate.variant = Some(LuaPackageProvider::Sushi);
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }

    // Do not preflight raw.githubusercontent.com while rendering the picker.
    // The actual package GET is authoritative. Range probes here caused Sushi and
    // Skyflare to share the same public-host rate-limit bucket before the user had
    // even selected either source.
    candidate.on_demand = true;
    candidate.revision = Some(format!("sushi-on-demand-{appid}"));
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
    candidate.on_demand = true;
    candidate.variant = Some(LuaPackageProvider::Ryuu);
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    match decrypt_ryuu_key(&settings) {
        Ok(Some(_)) => {
            candidate.key_ready = true;
            candidate.available = true;
            candidate.revision = Some(format!("ryuu-official-{appid}"));
        }
        Ok(None) => {
            candidate.error_code = Some("RYUU_KEY_REQUIRED".to_string());
        }
        Err(error) => candidate.error_code = Some(error),
    }
    candidate
}

fn probe_depotbox_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::TwentyTwoCloud);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.twenty_two_cloud_enabled;
    candidate.on_demand = true;
    candidate.variant = Some(LuaPackageProvider::TwentyTwoCloud);
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    // DepotBox remains usable without a paid API key through its official website.
    // A configured key upgrades the same provider to backend-only Direct API mode.
    candidate.available = true;
    match decrypt_depotbox_key(&settings) {
        Ok(Some(_)) => {
            candidate.key_ready = true;
            candidate.revision = Some(format!("depotbox-api-{appid}"));
        }
        Ok(None) => {
            candidate.key_ready = false;
            candidate.revision = Some(format!("depotbox-web-{appid}"));
        }
        Err(error) => {
            candidate.key_ready = false;
            candidate.error_code = Some(error);
        }
    }
    candidate
}

fn probe_skyflare_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::Skyflare);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.skyflare_enabled;
    candidate.variant = Some(LuaPackageProvider::Skyflare);
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }

    // Skyapi is a public AppID -> ZIP source. Avoid a separate Range/GET probe
    // just to paint the card; fetch_skyflare_package performs the real request.
    candidate.on_demand = true;
    candidate.revision = Some(format!("skyflare-on-demand-{appid}"));
    candidate
}

fn probe_github_mirrors_source(app: &AppHandle, _appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::GitHubMirrors);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.github_mirrors_enabled;
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    candidate.available = true;
    candidate.variant = Some(LuaPackageProvider::GitHubMirrors);
    candidate.revision = Some("k25-mirrors".to_string());
    candidate
}

fn probe_openlua_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::OpenLua);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.openlua_enabled;
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    candidate.available = true;
    candidate.on_demand = true;
    candidate.variant = Some(LuaPackageProvider::OpenLua);
    candidate.revision = Some(format!("openlua-{appid}"));
    candidate
}

fn probe_steamtools_source(app: &AppHandle, appid: u32) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(LuaSourceProvider::SteamTools);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = settings.steamtools_enabled;
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    candidate.available = true;
    candidate.on_demand = true;
    candidate.variant = Some(LuaPackageProvider::SteamTools);
    candidate.revision = Some(format!("steamtools-{appid}"));
    candidate
}


fn luatools_source_aliases(provider: LuaSourceProvider) -> &'static [&'static str] {
    match provider {
        LuaSourceProvider::Luie => &["Luie"],
        // LuaTools has renamed this provider in different client/backend revisions.
        // Always prefer the current display name, then retry the wire aliases that
        // have been used by source discovery.
        LuaSourceProvider::TwentyTwoCloud => &[
            "TwentyTwo Cloud",
            "TwentyTwo",
            "TwentyTwoCloud",
            "22 Cloud",
            "22Cloud",
        ],
        _ => &[],
    }
}

fn luatools_package_provider(provider: LuaSourceProvider) -> Option<LuaPackageProvider> {
    match provider {
        LuaSourceProvider::Luie => Some(LuaPackageProvider::Luie),
        LuaSourceProvider::TwentyTwoCloud => Some(LuaPackageProvider::TwentyTwoCloud),
        _ => None,
    }
}

fn luatools_dynamic_enabled(settings: &LuaSourceSettingsDisk, provider: LuaSourceProvider) -> bool {
    match provider {
        LuaSourceProvider::Luie => settings.luie_enabled,
        LuaSourceProvider::TwentyTwoCloud => settings.twenty_two_cloud_enabled,
        _ => false,
    }
}

fn luatools_manifest_client(timeout: Duration) -> Result<Client, String> {
    // Match the official LuaTools desktop client's HttpClient behavior: redirects
    // are followed by default. The manifest backend is an HTTP endpoint and may
    // redirect while infrastructure is moved; disabling redirects loses the exact
    // dynamic source key and later makes lua.tools reject guessed display names as
    // "Unknown source".
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .redirect(Policy::limited(8))
        .user_agent(concat!("0xoLemon/", env!("CARGO_PKG_VERSION"), " LuaToolsDirect"))
        .build()
        .map_err(|error| format!("Could not initialize LuaTools source client: {error}"))
}

fn luatools_direct_available_source_entries(client: &Client, appid: u32) -> Result<Vec<String>, String> {
    let url = format!("{LUATOOLS_MANIFEST_BACKEND}/check_apis?appid={appid}");
    let response = client
        .get(&url)
        .header("User-Agent", LUATOOLS_MANIFEST_USER_AGENT)
        .send()
        .map_err(|error| format!("LUATOOLS_DISCOVERY_NETWORK:{error}"))?;
    if !response.status().is_success() {
        return Err(format!("LUATOOLS_DISCOVERY_HTTP_{}", response.status().as_u16()));
    }
    let payload: Value = response
        .json()
        .map_err(|_| "LUATOOLS_DISCOVERY_INVALID_RESPONSE".to_string())?;
    let Some(object) = payload.as_object() else {
        return Err("LUATOOLS_DISCOVERY_INVALID_RESPONSE".to_string());
    };
    Ok(object
        .iter()
        .filter_map(|(name, status)| {
            let available = status
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case("available"));
            available.then(|| name.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect())
}

fn normalize_luatools_source_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn luatools_source_cache_key(appid: u32, provider: LuaSourceProvider) -> String {
    format!("{appid}:{}", provider.cache_name())
}

fn luatools_wire_name_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_luatools_wire_name(appid: u32, provider: LuaSourceProvider, source_name: &str) {
    if let Ok(mut cache) = luatools_wire_name_cache().lock() {
        cache.insert(
            luatools_source_cache_key(appid, provider),
            source_name.trim().to_string(),
        );
    }
}

fn cached_luatools_wire_name(appid: u32, provider: LuaSourceProvider) -> Option<String> {
    luatools_wire_name_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&luatools_source_cache_key(appid, provider)).cloned())
        .filter(|name| !name.trim().is_empty())
}

fn clear_luatools_wire_name(appid: u32, provider: LuaSourceProvider) {
    if let Ok(mut cache) = luatools_wire_name_cache().lock() {
        cache.remove(&luatools_source_cache_key(appid, provider));
    }
}

fn luatools_match_discovered_source(
    names: &[String],
    provider: LuaSourceProvider,
) -> Option<String> {
    let aliases = luatools_source_aliases(provider);
    if aliases.is_empty() {
        return None;
    }

    // The official LuaTools desktop app passes the exact key returned by
    // check_apis into /api/manifest/download. Match exact normalized aliases
    // first, then accept a fuzzy match only when it is unambiguous.
    for alias in aliases {
        let alias_key = normalize_luatools_source_name(alias);
        if let Some(name) = names
            .iter()
            .find(|name| normalize_luatools_source_name(name) == alias_key)
        {
            return Some(name.clone());
        }
    }

    let alias_keys = aliases
        .iter()
        .map(|name| normalize_luatools_source_name(name))
        .collect::<Vec<_>>();
    let fuzzy = names
        .iter()
        .filter(|name| {
            let key = normalize_luatools_source_name(name);
            alias_keys
                .iter()
                .any(|alias| key.contains(alias) || alias.contains(&key))
        })
        .cloned()
        .collect::<Vec<_>>();
    (fuzzy.len() == 1).then(|| fuzzy[0].clone())
}

fn luatools_known_stable_wire_name(provider: LuaSourceProvider) -> Option<&'static str> {
    match provider {
        // LuaTools documents Luie as a named source and the authenticated download
        // endpoint accepts this stable key directly. Do not make Luie depend on the
        // separate legacy check_apis host being online. TwentyTwo/Skyflare stay
        // discovery-driven because their server-side keys may change independently.
        LuaSourceProvider::Luie => Some("Luie"),
        _ => None,
    }
}

fn luatools_exact_download_source_name(
    appid: u32,
    provider: LuaSourceProvider,
) -> Result<String, String> {
    if let Some(name) = cached_luatools_wire_name(appid, provider) {
        return Ok(name);
    }
    let client = luatools_manifest_client(Duration::from_secs(20))?;
    match luatools_direct_available_source_entries(&client, appid) {
        Ok(names) => {
            if let Some(name) = luatools_match_discovered_source(&names, provider) {
                cache_luatools_wire_name(appid, provider, &name);
                return Ok(name);
            }
            Err(format!("LUATOOLS_SOURCE_NOT_AVAILABLE:{}", provider.cache_name()))
        }
        Err(error) => {
            if let Some(name) = luatools_known_stable_wire_name(provider) {
                cache_luatools_wire_name(appid, provider, name);
                return Ok(name.to_string());
            }
            Err(error)
        }
    }
}

fn luatools_direct_source_available(
    client: &Client,
    appid: u32,
    provider: LuaSourceProvider,
) -> Result<bool, String> {
    let names = luatools_direct_available_source_entries(client, appid)?;
    if let Some(name) = luatools_match_discovered_source(&names, provider) {
        cache_luatools_wire_name(appid, provider, &name);
        Ok(true)
    } else {
        Ok(false)
    }
}

fn probe_luatools_direct_source(
    app: &AppHandle,
    appid: u32,
    provider: LuaSourceProvider,
) -> LuaSourceCandidate {
    let mut candidate = empty_source_candidate(provider);
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    candidate.enabled = luatools_dynamic_enabled(&settings, provider);
    candidate.requires_key = false;
    candidate.key_ready = true;
    candidate.variant = luatools_package_provider(provider);
    if !candidate.enabled {
        candidate.error_code = Some("SOURCE_DISABLED".to_string());
        return candidate;
    }
    let client = match luatools_manifest_client(Duration::from_secs(18)) {
        Ok(client) => client,
        Err(error) => {
            candidate.error_code = Some(error);
            return candidate;
        }
    };
    match luatools_direct_source_available(&client, appid, provider) {
        Ok(true) => {
            candidate.available = true;
            candidate.revision = Some(format!("luatools-{}-{appid}", provider.cache_name()));
        }
        Ok(false) => {}
        Err(error) => {
            // The legacy discovery service is a separate dependency from lua.tools.
            // Luie has a stable authenticated download key, so a discovery outage must
            // not take that source offline. Other dynamic sources remain strict until
            // we have an exact wire name from discovery/cache.
            if luatools_known_stable_wire_name(provider).is_some() {
                candidate.available = false;
                candidate.on_demand = true;
                candidate.error_code = Some("LUATOOLS_DISCOVERY_DEFERRED".to_string());
            } else {
                candidate.available = false;
                candidate.on_demand = false;
                candidate.error_code = Some(error);
            }
        }
    }
    candidate
}

fn probe_luatools_direct_sources(app: &AppHandle, appid: u32) -> Vec<LuaSourceCandidate> {
    let providers = [LuaSourceProvider::Luie];
    let settings = match load_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            return providers
                .into_iter()
                .map(|provider| {
                    let mut candidate = empty_source_candidate(provider);
                    candidate.error_code = Some(error.clone());
                    candidate
                })
                .collect();
        }
    };
    let mut candidates = providers
        .into_iter()
        .map(|provider| {
            let mut candidate = empty_source_candidate(provider);
            candidate.enabled = luatools_dynamic_enabled(&settings, provider);
            candidate.requires_key = false;
            candidate.key_ready = true;
            candidate.variant = luatools_package_provider(provider);
            if !candidate.enabled {
                candidate.error_code = Some("SOURCE_DISABLED".to_string());
            }
            candidate
        })
        .collect::<Vec<_>>();

    if !candidates.iter().any(|candidate| candidate.enabled) {
        return candidates;
    }
    let result = luatools_manifest_client(Duration::from_secs(18))
        .and_then(|client| luatools_direct_available_source_entries(&client, appid));
    match result {
        Ok(names) => {
            for candidate in &mut candidates {
                if !candidate.enabled {
                    continue;
                }
                if let Some(name) = luatools_match_discovered_source(&names, candidate.provider) {
                    cache_luatools_wire_name(appid, candidate.provider, &name);
                    candidate.available = true;
                    candidate.revision = Some(format!(
                        "luatools-{}-{appid}",
                        candidate.provider.cache_name()
                    ));
                }
            }
        }
        Err(error) => {
            for candidate in &mut candidates {
                if !candidate.enabled {
                    continue;
                }
                if luatools_known_stable_wire_name(candidate.provider).is_some() {
                    candidate.available = false;
                    candidate.on_demand = true;
                    candidate.error_code = Some("LUATOOLS_DISCOVERY_DEFERRED".to_string());
                } else {
                    candidate.available = false;
                    candidate.on_demand = false;
                    candidate.error_code = Some(error.clone());
                }
            }
        }
    }
    candidates
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
        LuaSourceProvider::GitHubMirrors => probe_github_mirrors_source(app, appid),
        LuaSourceProvider::OpenLua => probe_openlua_source(app, appid),
        LuaSourceProvider::SteamTools => probe_steamtools_source(app, appid),
        LuaSourceProvider::Ryuu => probe_ryuu_source(app, appid),
        LuaSourceProvider::Luie => probe_luatools_direct_source(app, appid, provider),
        LuaSourceProvider::TwentyTwoCloud => probe_depotbox_source(app, appid),
        LuaSourceProvider::Skyflare => probe_skyflare_source(app, appid),
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
        LuaSourceProvider::TwentyTwoCloud,
        LuaSourceProvider::Skyflare,
        LuaSourceProvider::GitHubMirrors,
        LuaSourceProvider::OpenLua,
        LuaSourceProvider::SteamTools,
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
    // Dynamic LuaTools providers share one discovery request so opening the picker
    // does not fan out three identical upstream checks through the desktop bridge.
    sources.extend(probe_luatools_direct_sources(app, appid));
    sources.sort_by_key(|candidate| match candidate.provider {
        LuaSourceProvider::HuggingFace => 0,
        LuaSourceProvider::Hubcap => 1,
        LuaSourceProvider::Sushi => 2,
        LuaSourceProvider::Luie => 3,
        LuaSourceProvider::TwentyTwoCloud => 4,
        LuaSourceProvider::Skyflare => 5,
        LuaSourceProvider::GitHubMirrors => 6,
        LuaSourceProvider::OpenLua => 7,
        LuaSourceProvider::SteamTools => 8,
        LuaSourceProvider::Ryuu => 9,
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
                .position(|candidate| {
                    candidate.provider == LuaSourceProvider::HuggingFace
                        && candidate.enabled
                        && candidate.available
                })
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
    // Sushi is on-demand. Do not burn a GitHub request merely to advertise
    // availability; the actual fetch is the source of truth and handles 404.
    let sushi_available = settings.sushi_enabled;
    let ryuu_available = settings.ryuu_enabled
        && decrypt_ryuu_key(settings).ok().flatten().is_some();

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
    Ok(probe_one(
        &client,
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
                    scope.spawn(move || probe_one(client, *appid, settings, key))
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
        return Ok((Vec::new(), Some(0)));
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
        .map_err(|_| "Lua catalog backend unavailable".to_string())?;
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

pub(crate) fn start_backend_keepalive() {
    std::thread::spawn(move || {
        let client = match source_client(Duration::from_secs(30)) {
            Ok(c) => c,
            Err(_) => return,
        };
        loop {
            let url = format!("{BACKEND_BASE}/catalog/search?limit=1");
            if let Err(e) = client.get(&url).send() {
                crate::debug_log::debug_log(&format!("Lua backend keepalive failed: {}", e));
            } else {
                crate::debug_log::debug_log("Lua backend keepalive ping sent.");
            }
            // Ping every 12 minutes to keep the Render free tier awake (sleeps after 15m)
            std::thread::sleep(Duration::from_secs(720));
        }
    });
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
        .map_err(|_| "Lua quota service unavailable".to_string())?;
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
        .map_err(|_| "Lua quota service unavailable".to_string())?;
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
            Err(_) => {
                last_error = Some("Lua quota service unavailable".to_string());
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


fn provider_long_bracket_open(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    if bytes.get(index) != Some(&b'[') { return None; }
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'=') { cursor += 1; }
    (bytes.get(cursor) == Some(&b'[')).then_some((cursor - index - 1, cursor + 1))
}

fn provider_long_bracket_end(bytes: &[u8], mut cursor: usize, level: usize) -> usize {
    while cursor < bytes.len() {
        if bytes[cursor] == b']' {
            let mut probe = cursor + 1;
            let mut equals = 0;
            while bytes.get(probe) == Some(&b'=') { equals += 1; probe += 1; }
            if equals == level && bytes.get(probe) == Some(&b']') { return probe + 1; }
        }
        cursor += 1;
    }
    bytes.len()
}

fn mask_provider_lua_non_code(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            let start = index;
            if let Some((level, content_start)) = provider_long_bracket_open(bytes, index + 2) {
                index = provider_long_bracket_end(bytes, content_start, level);
            } else {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' { index += 1; }
            }
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' { masked[offset] = b' '; }
            }
            continue;
        }
        if bytes[index] == b'\'' || bytes[index] == b'"' {
            let quote = bytes[index];
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' { index = (index + 2).min(bytes.len()); continue; }
                if bytes[index] == quote { index += 1; break; }
                index += 1;
            }
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' { masked[offset] = b' '; }
            }
            continue;
        }
        if let Some((level, content_start)) = provider_long_bracket_open(bytes, index) {
            let start = index;
            index = provider_long_bracket_end(bytes, content_start, level);
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' { masked[offset] = b' '; }
            }
            continue;
        }
        index += 1;
    }
    String::from_utf8(masked).unwrap_or_else(|_| " ".repeat(bytes.len()))
}

fn provider_call_args(source: &str, function_name: &str) -> Result<Vec<Vec<String>>, String> {
    let mask = mask_provider_lua_non_code(source);
    let regex = Regex::new(&format!(
        r"(?im)^[\t ]*{}[\t ]*\(",
        regex::escape(function_name)
    )).map_err(|_| "Could not prepare provider Lua parser".to_string())?;
    let bytes = mask.as_bytes();
    let mut calls = Vec::new();
    for found in regex.find_iter(&mask) {
        let open = mask[found.start()..found.end()].rfind('(')
            .map(|value| found.start() + value)
            .ok_or_else(|| "Provider Lua call is malformed".to_string())?;
        let mut cursor = open + 1;
        let mut depth = 1usize;
        while cursor < bytes.len() && depth > 0 {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 { return Err(format!("Unterminated provider Lua call: {function_name}")); }
        let args = &source[open + 1..cursor - 1];
        calls.push(split_lua_args(args)?);
    }
    Ok(calls)
}

fn inspect_provider_lua(appid: u32, source: &str) -> Result<BTreeMap<u32, String>, String> {
    if source.is_empty() || source.len() > MAX_LUA_BYTES || source.as_bytes().contains(&0) {
        return Err("Lua source is empty, too large, or contains NUL bytes".to_string());
    }
    let mut root_registered = false;
    for args in provider_call_args(source, "addappid")? {
        let Some(first) = args.first() else { continue; };
        if parse_decimal(first, "AppID")?.parse::<u32>().ok() == Some(appid) {
            root_registered = true;
        }
    }
    if !root_registered {
        return Err(format!("Lua package does not register root AppID {appid}"));
    }

    let mut manifests = BTreeMap::new();
    for args in provider_call_args(source, "setmanifestid")? {
        if !(2..=3).contains(&args.len()) {
            return Err("setManifestid requires depot ID, manifest GID and optional size".to_string());
        }
        let depot = parse_decimal(&args[0], "Depot ID")?;
        let gid = parse_decimal(&args[1], "Manifest GID")?;
        let depot_id = depot.parse::<u32>()
            .map_err(|_| "Depot ID is outside the supported range".to_string())?;
        if manifests.get(&depot_id).is_some_and(|existing| existing != &gid) {
            return Err(format!("Lua source contains conflicting manifest pins for depot {depot_id}"));
        }
        manifests.insert(depot_id, gid);
    }
    Ok(manifests)
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
    }
    if !root_registered {
        return Err(format!("Lua package does not register root AppID {appid}"));
    }
    Ok((source.to_string(), manifests))
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
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err("HUBCAP_RATE_LIMITED".to_string());
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

pub(crate) fn fetch_github_mirror_manifest(
    client: &Client,
    depot_id: u32,
    manifest_gid: &str,
) -> Result<Option<Vec<u8>>, String> {
    for mirror in GITHUB_MANIFEST_MIRRORS {
        let url = format!("{mirror}/{depot_id}_{manifest_gid}.manifest");
        let parsed = match Url::parse(&url) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if let Ok(response) = client
            .get(parsed)
            .header(ACCEPT_ENCODING, "identity")
            .send()
        {
            if response.status() == StatusCode::OK {
                if let Ok(bytes) = response.bytes() {
                    if !bytes.is_empty() {
                        return Ok(Some(bytes.to_vec()));
                    }
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn fetch_manifesthub_manifest(
    client: &Client,
    key: &str,
    depot_id: u32,
    manifest_gid: &str,
) -> Result<Option<Vec<u8>>, String> {
    let url = format!(
        "https://api.manifesthub2.filegear-sg.me/manifest?apikey={}&depotid={}&manifestid={}",
        key.trim(),
        depot_id,
        manifest_gid
    );
    let parsed = match Url::parse(&url) {
        Ok(u) => u,
        Err(_) => return Ok(None),
    };
    if let Ok(response) = client.get(parsed).send() {
        if response.status() == StatusCode::OK {
            if let Ok(bytes) = response.bytes() {
                if !bytes.is_empty() {
                    return Ok(Some(bytes.to_vec()));
                }
            }
        }
    }
    Ok(None)
}


#[derive(Debug, Clone, Serialize, Deserialize)]
struct LuaToolsAuthSession {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

fn luatools_auth_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("lua-sources").join("luatools-auth.bin"))
        .map_err(|error| format!("Could not resolve LuaTools auth storage: {error}"))
}

fn load_luatools_auth(app: &AppHandle) -> Result<Option<LuaToolsAuthSession>, String> {
    let path = luatools_auth_path(app)?;
    if !path.is_file() {
        return Ok(None);
    }
    let encrypted = fs::read(&path)
        .map_err(|error| format!("Could not read LuaTools auth session: {error}"))?;
    let clear = crate::secret_store::unprotect(&encrypted)
        .map_err(|error| format!("Could not decrypt LuaTools auth session: {error}"))?;
    let session = serde_json::from_slice(&clear)
        .map_err(|_| "Stored LuaTools auth session is invalid".to_string())?;
    Ok(Some(session))
}

fn save_luatools_auth(app: &AppHandle, session: &LuaToolsAuthSession) -> Result<(), String> {
    let clear = serde_json::to_vec(session)
        .map_err(|error| format!("Could not serialize LuaTools auth session: {error}"))?;
    let encrypted = crate::secret_store::protect(&clear)
        .map_err(|error| format!("Could not encrypt LuaTools auth session: {error}"))?;
    crate::lua_live::atomic_write_path(&luatools_auth_path(app)?, &encrypted)
}

fn clear_luatools_auth(app: &AppHandle) {
    if let Ok(path) = luatools_auth_path(app) {
        let _ = fs::remove_file(path);
    }
}

fn luatools_session_from_value(
    payload: &Value,
    fallback_refresh_token: Option<&str>,
) -> Result<LuaToolsAuthSession, String> {
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "LUATOOLS_AUTH_ACCESS_TOKEN_MISSING".to_string())?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or(fallback_refresh_token)
        .ok_or_else(|| "LUATOOLS_AUTH_REFRESH_TOKEN_MISSING".to_string())?
        .to_string();
    let expires_in = payload
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600)
        .max(60);
    Ok(LuaToolsAuthSession {
        access_token,
        refresh_token,
        expires_at: Utc::now().timestamp() + expires_in,
    })
}

fn refresh_luatools_session(
    app: &AppHandle,
    client: &Client,
    session: &LuaToolsAuthSession,
) -> Result<LuaToolsAuthSession, String> {
    let response = client
        .post(format!(
            "{LUATOOLS_SUPABASE_URL}/auth/v1/token?grant_type=refresh_token"
        ))
        .header("apikey", LUATOOLS_SUPABASE_ANON_KEY)
        .json(&json!({ "refresh_token": session.refresh_token }))
        .send()
        .map_err(|error| format!("LUATOOLS_AUTH_REFRESH_NETWORK:{error}"))?;
    if !response.status().is_success() {
        return Err(format!("LUATOOLS_AUTH_REFRESH_HTTP_{}", response.status().as_u16()));
    }
    let payload: Value = response
        .json()
        .map_err(|_| "LUATOOLS_AUTH_REFRESH_INVALID_RESPONSE".to_string())?;
    let refreshed = luatools_session_from_value(&payload, Some(&session.refresh_token))?;
    save_luatools_auth(app, &refreshed)?;
    Ok(refreshed)
}

fn luatools_pkce_pair() -> (String, String) {
    let verifier = format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn open_luatools_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|error| format!("Could not open LuaTools sign-in: {error}"))?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Could not open LuaTools sign-in: {error}"))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Could not open LuaTools sign-in: {error}"))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err("Could not open LuaTools sign-in on this platform".to_string())
}

fn write_luatools_oauth_result(
    stream: &mut std::net::TcpStream,
    ok: bool,
    detail: Option<&str>,
) {
    let title = if ok { "LuaTools connected" } else { "LuaTools sign-in failed" };
    let body = if ok {
        "Authentication completed. You can close this tab and return to 0xoLemon."
            .to_string()
    } else {
        format!(
            "Authentication did not complete: {}",
            detail.unwrap_or("unknown error")
        )
    };
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"font-family:system-ui;background:#0b0f14;color:#e8eef6;padding:32px\"><h2>{title}</h2><p>{body}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        page.as_bytes().len(),
        page
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn wait_for_luatools_oauth_callback(
    listener: &std::net::TcpListener,
) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Could not configure LuaTools OAuth listener: {error}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    while std::time::Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buffer = [0u8; 16384];
                let read = stream.read(&mut buffer).unwrap_or(0);
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("");
                if !target.starts_with("/callback") {
                    write_luatools_oauth_result(&mut stream, false, Some("unexpected callback"));
                    continue;
                }
                let callback = Url::parse(&format!("http://localhost:{LUATOOLS_OAUTH_PORT}{target}"))
                    .map_err(|_| "LUATOOLS_AUTH_CALLBACK_INVALID".to_string())?;
                let code = callback
                    .query_pairs()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.into_owned());
                let error = callback
                    .query_pairs()
                    .find(|(key, _)| key == "error_description" || key == "error")
                    .map(|(_, value)| value.into_owned());
                if let Some(code) = code {
                    write_luatools_oauth_result(&mut stream, true, None);
                    return Ok(code);
                }
                if let Some(error) = error {
                    write_luatools_oauth_result(&mut stream, false, Some(&error));
                    return Err(format!("LUATOOLS_AUTH_DENIED:{error}"));
                }
                write_luatools_oauth_result(&mut stream, false, Some("authorization code missing"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(error) => {
                return Err(format!("LUATOOLS_AUTH_CALLBACK_LISTENER:{error}"));
            }
        }
    }
    Err("LUATOOLS_AUTH_TIMEOUT".to_string())
}

fn interactive_luatools_sign_in(app: &AppHandle, client: &Client) -> Result<LuaToolsAuthSession, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", LUATOOLS_OAUTH_PORT))
        .map_err(|error| format!("LUATOOLS_AUTH_PORT_BUSY:{error}"))?;
    let (verifier, challenge) = luatools_pkce_pair();
    let authorize_url = format!(
        "{LUATOOLS_SUPABASE_URL}/auth/v1/authorize?provider=discord&redirect_to={}&code_challenge={}&code_challenge_method=s256",
        urlencoding::encode(LUATOOLS_OAUTH_CALLBACK),
        urlencoding::encode(&challenge),
    );
    open_luatools_browser(&authorize_url)?;
    let code = wait_for_luatools_oauth_callback(&listener)?;
    let response = client
        .post(format!("{LUATOOLS_SUPABASE_URL}/auth/v1/token?grant_type=pkce"))
        .header("apikey", LUATOOLS_SUPABASE_ANON_KEY)
        .json(&json!({ "auth_code": code, "code_verifier": verifier }))
        .send()
        .map_err(|error| format!("LUATOOLS_AUTH_EXCHANGE_NETWORK:{error}"))?;
    if !response.status().is_success() {
        return Err(format!("LUATOOLS_AUTH_EXCHANGE_HTTP_{}", response.status().as_u16()));
    }
    let payload: Value = response
        .json()
        .map_err(|_| "LUATOOLS_AUTH_EXCHANGE_INVALID_RESPONSE".to_string())?;
    let session = luatools_session_from_value(&payload, None)?;
    save_luatools_auth(app, &session)?;
    Ok(session)
}

fn ensure_luatools_access_token(app: &AppHandle, client: &Client) -> Result<String, String> {
    if let Some(session) = load_luatools_auth(app)? {
        if session.expires_at > Utc::now().timestamp() + 120 {
            return Ok(session.access_token);
        }
        match refresh_luatools_session(app, client, &session) {
            Ok(refreshed) => return Ok(refreshed.access_token),
            Err(_) => clear_luatools_auth(app),
        }
    }
    Ok(interactive_luatools_sign_in(app, client)?.access_token)
}

fn luatools_direct_download_bytes(
    app: &AppHandle,
    client: &Client,
    appid: u32,
    source_name: &str,
) -> Result<Vec<u8>, String> {
    if source_name.trim().is_empty() {
        return Err("LUATOOLS_SOURCE_INVALID".to_string());
    }
    let mut token = ensure_luatools_access_token(app, client)?;
    let url = format!(
        "{LUATOOLS_API_BASE}/api/manifest/download?appid={appid}&source={}",
        urlencoding::encode(source_name)
    );

    for auth_attempt in 0..2 {
        let mut response = client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .map_err(|error| format!("LUATOOLS_DOWNLOAD_NETWORK:{error}"))?;
        if response.status() == StatusCode::UNAUTHORIZED && auth_attempt == 0 {
            clear_luatools_auth(app);
            token = interactive_luatools_sign_in(app, client)?.access_token;
            continue;
        }
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err("LUATOOLS_DAILY_LIMIT_REACHED".to_string());
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let mut detail = String::new();
            let _ = response.by_ref().take(2048).read_to_string(&mut detail);
            let detail = detail.trim().replace('\r', " ").replace('\n', " ");
            if detail.to_ascii_lowercase().contains("unknown source") {
                return Err(format!(
                    "LUATOOLS_SOURCE_KEY_REJECTED:{source_name}:HTTP_{status}:{detail}"
                ));
            }
            return Err(if detail.is_empty() {
                format!("LUATOOLS_DOWNLOAD_HTTP_{status}")
            } else {
                format!("LUATOOLS_DOWNLOAD_HTTP_{status}:{detail}")
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
        {
            return Err("LUATOOLS_DOWNLOAD_TOO_LARGE".to_string());
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(MAX_ARCHIVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("LUATOOLS_DOWNLOAD_READ:{error}"))?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("LUATOOLS_DOWNLOAD_INVALID_SIZE".to_string());
        }
        return Ok(bytes);
    }
    Err("LUATOOLS_AUTH_REQUIRED".to_string())
}

fn package_from_luatools_direct_payload(
    appid: u32,
    provider: LuaSourceProvider,
    bytes: &[u8],
) -> Result<CanonicalPackage, String> {
    let package_provider = luatools_package_provider(provider)
        .ok_or_else(|| "LUATOOLS_SOURCE_INVALID".to_string())?;
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") || bytes.starts_with(b"PK\x07\x08") {
        // LuaTools direct payloads may be ZIP or bare Lua depending on the provider.
        // package_from_archive keeps provider boundaries intact for manifest-backed bundles.
        return package_from_archive(appid, package_provider, bytes);
    }
    if bytes.len() > MAX_LUA_BYTES || bytes.contains(&0) {
        return Err("LUATOOLS_LUA_INVALID".to_string());
    }
    let source = String::from_utf8(bytes.to_vec())
        .map_err(|_| "LUATOOLS_LUA_NOT_UTF8".to_string())?;
    inspect_provider_lua(appid, &source)?;
    // Luie is Live-only and may return a bare Lua.
    let archive_bytes = build_canonical_archive(appid, package_provider, &source, &[])?;
    Ok(CanonicalPackage {
        appid,
        provider: package_provider,
        revision: sha256_bytes(bytes),
        canonical_lua: source,
        manifests: Vec::new(),
        archive_bytes,
    })
}

pub(crate) fn fetch_luatools_direct_package(
    app: &AppHandle,
    appid: u32,
    provider: LuaSourceProvider,
) -> Result<Option<CanonicalPackage>, String> {
    let source_name = luatools_exact_download_source_name(appid, provider)?;
    // Preserve the official LuaTools contract exactly: send the source key returned
    // by check_apis, never a guessed display alias. If a cached key is rejected,
    // invalidate it and refresh discovery exactly once; never rotate guessed aliases.
    let client = source_client(Duration::from_secs(300))?;
    let bytes = match luatools_direct_download_bytes(app, &client, appid, &source_name) {
        Ok(bytes) => bytes,
        Err(error) if error.starts_with("LUATOOLS_SOURCE_KEY_REJECTED:") => {
            clear_luatools_wire_name(appid, provider);
            let refreshed_name = luatools_exact_download_source_name(appid, provider)?;
            if refreshed_name.eq_ignore_ascii_case(&source_name) {
                return Err(error);
            }
            luatools_direct_download_bytes(app, &client, appid, &refreshed_name)?
        }
        Err(error) => return Err(error),
    };
    package_from_luatools_direct_payload(appid, provider, &bytes).map(Some)
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
    let mut source_lua: Option<String> = None;
    let mut lua_candidates: Vec<(String, String)> = Vec::new();
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

        let is_lua = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lua"));
        if !is_lua {
            continue;
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
        let text = String::from_utf8(bytes)
            .map_err(|_| "Lua source is not valid UTF-8".to_string())?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        if file_name.eq_ignore_ascii_case(&format!("{appid}.lua")) {
            if source_lua.is_some() {
                return Err("Lua package contains duplicate root Lua files".to_string());
            }
            source_lua = Some(text);
        } else {
            lua_candidates.push((path.to_string_lossy().into_owned(), text));
        }
    }
    drop(archive);

    // Hubcap has changed archive layout/name conventions before. Prefer the exact
    // <AppID>.lua when present, but if it is absent accept exactly one other Lua
    // whose declarations validate for the requested root AppID. Never pick by
    // position and never merge Lua from another source.
    let source_lua = if let Some(source) = source_lua {
        source
    } else {
        let mut valid = Vec::new();
        let mut rejected = Vec::new();
        for (path, candidate) in lua_candidates {
            match inspect_provider_lua(appid, &candidate) {
                Ok(_) => valid.push((path, candidate)),
                Err(error) => rejected.push((path, error)),
            }
        }
        match valid.len() {
            1 => valid.pop().expect("length checked").1,
            0 => {
                let detail = rejected
                    .into_iter()
                    .take(3)
                    .map(|(path, error)| format!("{path}: {error}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                return Err(if detail.is_empty() {
                    format!("Lua package does not contain any .lua payload for AppID {appid}")
                } else {
                    format!("Lua package contains .lua files, but none register AppID {appid}: {detail}")
                })
            }
            _ => {
                return Err(format!(
                    "Lua package contains multiple Lua payloads matching AppID {appid}"
                ))
            }
        }
    };

    let expected_manifests = inspect_provider_lua(appid, &source_lua)?;
    let canonical_lua = source_lua;
    if provider == LuaPackageProvider::Skyflare {
        // Skyapi currently publishes Lua-only ZIPs. Preserve the provider Lua as
        // a Live source even when it still contains legacy setManifestid pins;
        // the Live installer removes only those pins from the active file.
        let canonical_archive = build_canonical_archive(appid, provider, &canonical_lua, &[])?;
        return Ok(CanonicalPackage {
            appid,
            provider,
            revision: sha256_bytes(archive_bytes),
            canonical_lua,
            manifests: Vec::new(),
            archive_bytes: canonical_archive,
        });
    }
    let mut manifests = inspect_manifest_archive(archive_bytes)?;
    let mut actual = manifests
        .iter()
        .map(|value| (value.depot_id, value.manifest_gid.clone()))
        .collect::<BTreeMap<_, _>>();
    for (depot_id, gid) in &expected_manifests {
        if actual.get(depot_id) != Some(gid) {
            if matches!(
                provider,
                LuaPackageProvider::Hubcap
                    | LuaPackageProvider::TwentyTwoCloud
            ) {
                return Err(format!(
                    "{provider:?} same-provider bundle is missing its paired manifest {depot_id}_{gid}.manifest"
                ));
            }
            let mut resolved = false;
            if let Ok(client) = source_client(Duration::from_secs(15)) {
                if let Ok(Some(bytes)) = fetch_github_mirror_manifest(&client, *depot_id, gid) {
                    let sha = sha256_bytes(&bytes);
                    manifests.push(CanonicalManifest {
                        depot_id: *depot_id,
                        manifest_gid: gid.clone(),
                        file_name: format!("{depot_id}_{gid}.manifest"),
                        bytes,
                        sha256: sha,
                    });
                    actual.insert(*depot_id, gid.clone());
                    resolved = true;
                }
            }
            if !resolved {
                return Err(format!(
                    "Lua package is missing manifest {depot_id}_{gid}.manifest"
                ));
            }
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


fn download_github_contents_raw_to_part(
    app: &AppHandle,
    provider: LuaSourceProvider,
    appid: u32,
    client: &Client,
    url: &str,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, String> {
    let parsed = ensure_https_host(url, &["api.github.com"])?;
    let mut response = client
        .get(parsed)
        .header(ACCEPT, "application/vnd.github.raw+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("Could not reach GitHub source API: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if response.status() == StatusCode::TOO_MANY_REQUESTS
        || response.status() == StatusCode::FORBIDDEN
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                == Some("0")
    {
        let reset = response
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown");
        return Err(format!("GITHUB_SOURCE_RATE_LIMITED:reset={reset}"));
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub source API returned HTTP {}",
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
            .map_err(|error| format!("Could not download GitHub Lua package: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            let _ = fs::remove_file(&part);
            return Err("Lua package exceeds the download safety limit".to_string());
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not cache GitHub Lua package: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not flush GitHub Lua package cache: {error}"))?;
    drop(file);
    let bytes = fs::read(&part)
        .map_err(|error| format!("Could not read GitHub Lua package cache: {error}"))?;
    Ok(Some(bytes))
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
    let Some(bytes) = download_github_contents_raw_to_part(
        app,
        LuaSourceProvider::Sushi,
        appid,
        &client,
        &format!("{SUSHI_GITHUB_CONTENTS_BASE}/{appid}.zip?ref=main"),
        "sushi.zip",
        MAX_ARCHIVE_BYTES,
    )?
    else {
        return Ok(None);
    };
    package_from_archive(appid, LuaPackageProvider::Sushi, &bytes).map(Some)
}

pub(crate) fn fetch_skyflare_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.skyflare_enabled {
        return Ok(None);
    }
    let client = source_client(Duration::from_secs(90))?;
    let Some(bytes) = download_github_contents_raw_to_part(
        app,
        LuaSourceProvider::Skyflare,
        appid,
        &client,
        &format!("{SKYFLARE_GITHUB_CONTENTS_BASE}/{appid}.zip?ref=main"),
        "skyflare.zip",
        MAX_ARCHIVE_BYTES,
    )?
    else {
        return Ok(None);
    };
    package_from_archive(appid, LuaPackageProvider::Skyflare, &bytes).map(Some)
}

const DEPOTKEYS_URLS: &[&str] = &[
    "https://pub-d3ba7941fdf24c2c84da530b93221e1c.r2.dev/fallback_depotkeys.json",
    "https://raw.githubusercontent.com/KoriaPolis/Steam-Depot/main/fallback_depotkeys.json",
];

fn get_or_download_depotkeys(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve app data dir: {e}"))?;
    let cache_file = base.join("depotkeys_database.json");
    if cache_file.exists() {
        if let Ok(meta) = fs::metadata(&cache_file) {
            if meta.len() > 1024 * 1024 {
                return Ok(cache_file);
            }
        }
    }
    let _ = fs::create_dir_all(&base);
    let client = source_client(Duration::from_secs(60))?;
    for url in DEPOTKEYS_URLS {
        if let Ok(resp) = client.get(*url).send() {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    if bytes.len() > 1024 * 1024 {
                        let _ = fs::write(&cache_file, &bytes);
                        return Ok(cache_file);
                    }
                }
            }
        }
    }
    if cache_file.exists() {
        return Ok(cache_file);
    }
    Err("Could not download depot keys database".to_string())
}

pub(crate) fn generate_lua_from_depotkeys(app: &AppHandle, appid: u32) -> Result<Option<String>, String> {
    let cache_file = match get_or_download_depotkeys(app) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    let file = fs::File::open(&cache_file).map_err(|e| format!("Failed to read depot keys: {e}"))?;
    let reader = std::io::BufReader::new(file);
    let json: Value = serde_json::from_reader(reader).map_err(|e| format!("Invalid depot keys JSON: {e}"))?;
    let Some(map) = json.as_object() else {
        return Ok(None);
    };

    let appid_str = appid.to_string();
    let main_entry = map.get(&appid_str);

    struct DepotItem {
        id: u32,
        key: String,
        name: String,
    }
    struct DlcItem {
        id: u32,
        name: String,
    }

    let mut depots: Vec<DepotItem> = Vec::new();
    let mut dlcs: Vec<DlcItem> = Vec::new();

    for (id_str, val) in map {
        if id_str == &appid_str {
            continue;
        }
        let parent_appid = val.get("parent_appid").and_then(|v| {
            if let Some(n) = v.as_u64() {
                Some(n.to_string())
            } else {
                v.as_str().map(|s| s.to_string())
            }
        });
        if parent_appid.as_deref() == Some(&appid_str) {
            let key = val.get("key").and_then(|v| v.as_str()).unwrap_or("").trim();
            let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let id = id_str.parse::<u32>().unwrap_or(0);
            if id == 0 {
                continue;
            }
            if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
                depots.push(DepotItem { id, key: key.to_ascii_lowercase(), name });
            } else {
                dlcs.push(DlcItem { id, name });
            }
        }
    }

    if main_entry.is_none() && depots.is_empty() && dlcs.is_empty() {
        return Ok(None);
    }

    depots.sort_by_key(|d| d.id);
    dlcs.sort_by_key(|d| d.id);

    let mut lines = Vec::new();
    lines.push("-- Generated by 0xoLemon Launcher from SteamTools/SFF Depot Database".to_string());

    let main_key = main_entry
        .and_then(|v| v.get("key"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if main_key.len() == 64 && main_key.chars().all(|c| c.is_ascii_hexdigit()) {
        lines.push(format!("addappid({}, 1, \"{}\")", appid, main_key.to_ascii_lowercase()));
    } else {
        lines.push(format!("addappid({})", appid));
    }

    if !depots.is_empty() {
        lines.push("".to_string());
        lines.push("-- DEPOTS".to_string());
        for d in &depots {
            let comment = if !d.name.is_empty() { format!(" -- {}", d.name) } else { String::new() };
            lines.push(format!("addappid({}, 1, \"{}\"){}", d.id, d.key, comment));
        }
    }

    if !dlcs.is_empty() {
        lines.push("".to_string());
        lines.push("-- DLCS".to_string());
        for d in &dlcs {
            let comment = if !d.name.is_empty() { format!(" -- {}", d.name) } else { String::new() };
            lines.push(format!("addappid({}){}", d.id, comment));
        }
    }

    Ok(Some(lines.join("\n") + "\n"))
}

pub(crate) fn package_from_raw_lua_and_mirrors(
    app: &AppHandle,
    appid: u32,
    provider: LuaPackageProvider,
    custom_raw_lua: Option<&str>,
) -> Result<Option<CanonicalPackage>, String> {
    let source_lua = if let Some(raw) = custom_raw_lua {
        raw.to_string()
    } else {
        let client = source_client(Duration::from_secs(15)).ok();
        let fetched = client.as_ref().and_then(|c| {
            let url = format!("{HF_RAW_BASE}/curated/{appid}.lua");
            c.get(&url).send().ok().and_then(|res| {
                if res.status() == StatusCode::OK {
                    res.text().ok()
                } else {
                    None
                }
            })
        });
        if let Some(lua) = fetched {
            lua
        } else if let Ok(Some(gen_lua)) = generate_lua_from_depotkeys(app, appid) {
            gen_lua
        } else {
            format!("addappid({appid})\n")
        }
    };

    let expected_manifests = inspect_provider_lua(appid, &source_lua)?;
    let canonical_lua = source_lua;
    let mut manifests = Vec::new();
    let client = source_client(Duration::from_secs(15)).ok();
    let settings = load_settings(app).ok();
    let mh_key = settings.as_ref().and_then(|s| decrypt_manifesthub_key(s).ok().flatten());

    for (depot_id, gid) in &expected_manifests {
        let mut resolved_bytes = None;
        if let Some(ref c) = client {
            if let Ok(Some(bytes)) = fetch_github_mirror_manifest(c, *depot_id, gid) {
                resolved_bytes = Some(bytes);
            } else if let Some(ref key) = mh_key {
                if let Ok(Some(bytes)) = fetch_manifesthub_manifest(c, key, *depot_id, gid) {
                    resolved_bytes = Some(bytes);
                }
            }
        }
        if let Some(bytes) = resolved_bytes {
            let sha = sha256_bytes(&bytes);
            manifests.push(CanonicalManifest {
                depot_id: *depot_id,
                manifest_gid: gid.clone(),
                file_name: format!("{depot_id}_{gid}.manifest"),
                bytes,
                sha256: sha,
            });
        }
    }

    let canonical_archive = build_canonical_archive(appid, provider, &canonical_lua, &manifests)?;
    Ok(Some(CanonicalPackage {
        appid,
        provider,
        revision: sha256_bytes(&canonical_archive),
        canonical_lua,
        manifests,
        archive_bytes: canonical_archive,
    }))
}

pub(crate) fn fetch_github_mirrors_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.github_mirrors_enabled {
        return Ok(None);
    }
    package_from_raw_lua_and_mirrors(app, appid, LuaPackageProvider::GitHubMirrors, None)
}

pub(crate) fn fetch_openlua_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.openlua_enabled {
        return Ok(None);
    }

    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    use tauri::webview::DownloadEvent;

    // 1. Create a local one-shot TCP listener to safely receive Lua payload without Tauri IPC issues
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind local port for OpenLua: {e}"))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local port: {e}"))?
        .port();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_clone = tx.clone();

    // Spawn listener thread
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read, Write};
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(&mut stream);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).unwrap_or(0) == 0 {
                continue;
            }
            let is_options = first_line.starts_with("OPTIONS");

            let mut content_length = 0;
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break;
                }
                if trimmed.to_lowercase().starts_with("content-length:") {
                    if let Some(val) = trimmed.split(':').nth(1) {
                        content_length = val.trim().parse::<usize>().unwrap_or(0);
                    }
                }
                line.clear();
            }

            let mut body = vec![0u8; content_length];
            if content_length > 0 && !is_options {
                let _ = reader.read_exact(&mut body);
            }

            let response = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Allow-Methods: *\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();

            if !is_options && !body.is_empty() {
                if let Ok(text) = String::from_utf8(body) {
                    // Only accept a plausible Lua payload for the requested root AppID.
                    // Never treat arbitrary HTML/ad/challenge responses as a successful download.
                    let lower = text.to_ascii_lowercase();
                    let app_marker = format!("addappid({}", appid);
                    let spaced_marker = format!("addappid ({}", appid);
                    if text.len() <= MAX_LUA_BYTES
                        && (lower.contains(&app_marker) || lower.contains(&spaced_marker))
                    {
                        let _ = tx_clone.send(text);
                        break;
                    }
                }
            }
        }
    });

    let window_label = format!("openlua-dl-{}", appid);

    let inject_js = r###"
        (function() {
            'use strict';

            var PORT = Number('__PORT__');
            var APPID = '__APPID__';
            var GUIDE_MESSAGE = '__OXO_OPENLUA_GUIDE';
            var host = '';
            try { host = (window.location && window.location.hostname || '').toLowerCase(); } catch (e) {}

            // Windows/WebView2 may install a Tauri initialization script in child
            // frames even when the main-frame API is used. Isolate strictly by origin
            // before registering *any* listener, observer, timer, or automation.
            // Third-party ads and verification widgets therefore receive native input.
            var IS_OPENLUA = host === 'openlua.cloud' || host.endsWith('.openlua.cloud');
            if (!IS_OPENLUA) return;

            var IS_TOP = false;
            try { IS_TOP = window.top === window.self; } catch (e) { IS_TOP = false; }
            // The launcher automation belongs to the OpenLua document only. Same-origin
            // subframes are also left pristine; the parent can still observe/scroll the
            // iframe element without injecting input guards inside it.
            if (!IS_TOP) return;

            function elementVisible(el) {
                try {
                    if (!el || !el.isConnected) return false;
                    var r = el.getBoundingClientRect();
                    var style = window.getComputedStyle ? window.getComputedStyle(el) : null;
                    if (style && (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0)) return false;
                    return r.width > 2 && r.height > 2;
                } catch (e) { return false; }
            }

            function controlText(el) {
                try {
                    return [
                        el.textContent || '',
                        el.getAttribute && el.getAttribute('aria-label') || '',
                        el.getAttribute && el.getAttribute('title') || '',
                        el.getAttribute && el.getAttribute('data-testid') || '',
                        el.id || '',
                        typeof el.className === 'string' ? el.className : ''
                    ].join(' ').replace(/\s+/g, ' ').trim().toLowerCase();
                } catch (e) { return ''; }
            }

            function isCloudflareControl(target) {
                try {
                    if (!target || !target.closest) return false;
                    return !!target.closest(
                        'iframe[src*="challenges.cloudflare.com"], iframe[src*="turnstile" i], ' +
                        'iframe[src*="google.com/recaptcha" i], iframe[src*="recaptcha.net" i], ' +
                        'iframe[src*="hcaptcha.com" i], ' +
                        '.cf-turnstile, .cf-turnstile-wrapper, [data-sitekey][class*="turnstile" i]'
                    );
                } catch (e) { return false; }
            }

            function isAdContainer(el) {
                try {
                    if (!el || !el.closest) return false;
                    return !!el.closest(
                        '[data-ad], [data-ad-slot], [aria-label*="advert" i], ' +
                        '[id^="ad-" i], [id*="-ad-" i], [class^="ad-" i], [class*=" ad-" i], ' +
                        '[class*="advert" i], [id*="advert" i], [class*="sponsor" i], ' +
                        '[role="dialog"], [aria-modal="true"], [class*="modal" i], [class*="overlay" i], ' +
                        '[class*="interstitial" i], [id*="interstitial" i]'
                    );
                } catch (e) { return false; }
            }

            function looksLikeBlockingAdModal(el) {
                try {
                    if (!el || !elementVisible(el)) return false;
                    var r = el.getBoundingClientRect();
                    if (r.width < 220 || r.height < 110) return false;
                    var style = window.getComputedStyle ? window.getComputedStyle(el) : null;
                    var positioned = style && (style.position === 'fixed' || style.position === 'sticky' || style.position === 'absolute');
                    var z = style ? parseInt(style.zIndex || '0', 10) || 0 : 0;
                    var text = controlText(el);
                    var adLanguage = /congratulations|claim your bonus|advertisement|sponsored|skip ad|continue to (download|site)|please wait|seconds|countdown|reward|bonus/i.test(text)
                        || /\b\d{1,2}:\d{2}\b/.test(text);
                    var modalSemantic = !!(el.matches && el.matches('[role="dialog"], [aria-modal="true"], [class*="modal" i], [class*="overlay" i], [class*="interstitial" i], [data-ad], [data-ad-slot]'));
                    // The OpenLua page itself is large; only classify a large generic
                    // element as an interstitial when it is positioned above content
                    // and its copy looks like an advertisement/countdown.
                    return adLanguage && (modalSemantic || positioned || z >= 10);
                } catch (e) { return false; }
            }

            function nearestBlockingAdModal(el) {
                try {
                    var node = el;
                    for (var depth = 0; node && depth < 9; depth++, node = node.parentElement) {
                        if (looksLikeBlockingAdModal(node)) return node;
                    }
                } catch (e) {}
                return null;
            }

            function isIconOnlyDismissControl(control) {
                try {
                    if (!control || !elementVisible(control)) return false;
                    var r = control.getBoundingClientRect();
                    if (r.width > 84 || r.height > 84 || r.width < 10 || r.height < 10) return false;
                    var hint = controlText(control);
                    if (/close|dismiss|skip|exit|cancel|no thanks|continue/i.test(hint)) return true;
                    if (hint === 'x' || hint === '×' || hint === '✕' || hint === '✖') return true;

                    // Ad SDKs frequently render their close control as an unlabeled
                    // SVG icon. Permit only a small control located in the top-right
                    // corner of a blocking ad modal (or the child-frame viewport).
                    var hasIcon = !!(control.querySelector && control.querySelector('svg, path, img'));
                    if (!hasIcon && hint.length > 2) return false;
                    var modal = nearestBlockingAdModal(control);
                    if (modal) {
                        var mr = modal.getBoundingClientRect();
                        return r.right >= mr.right - 92 && r.top <= mr.top + 92;
                    }
                    if (!IS_TOP) {
                        return r.right >= window.innerWidth - 92 && r.top <= 92;
                    }
                    return false;
                } catch (e) { return false; }
            }

            function isAdActionControl(el) {
                try {
                    if (!el || !elementVisible(el)) return false;
                    var control = el.closest ? el.closest('button, a, [role="button"], input[type="button"], input[type="submit"]') : null;
                    if (!control || !elementVisible(control)) return false;
                    if (isIconOnlyDismissControl(control)) return true;
                    var text = controlText(control);
                    var action = /(^|\b)(close|dismiss|skip|continue|no thanks|got it|exit)(\b|$)/i.test(text)
                        || text === 'x' || text === '×' || text === '✕' || text === '✖';
                    if (!action) return false;

                    if (!IS_TOP) return true;
                    if (isAdContainer(control)) return true;
                    if (nearestBlockingAdModal(control)) return true;
                    return false;
                } catch (e) { return false; }
            }

            function frameLooksLikeAdOrInterstitial(frame) {
                try {
                    if (!frame || !elementVisible(frame)) return false;
                    var hint = [
                        frame.src || '',
                        frame.name || '',
                        frame.title || '',
                        frame.id || '',
                        typeof frame.className === 'string' ? frame.className : ''
                    ].join(' ').toLowerCase();
                    if (/advert|\bad\b|ads|sponsor|promo|interstitial|popunder|popup|captcha|recaptcha/.test(hint)) return true;
                    var r = frame.getBoundingClientRect();
                    return r.width >= Math.min(250, window.innerWidth * 0.45)
                        && r.height >= Math.min(160, window.innerHeight * 0.28);
                } catch (e) { return false; }
            }

            function findAdActionControl() {
                try {
                    var controls = Array.from(document.querySelectorAll('button, a, [role="button"], input[type="button"], input[type="submit"]'));
                    for (var i = 0; i < controls.length; i++) {
                        if (isAdActionControl(controls[i])) return controls[i];
                    }
                } catch (e) {}
                return null;
            }

            function findBlockingAdTarget() {
                var action = findAdActionControl();
                if (action) return action;
                if (!IS_TOP) return null;
                try {
                    var modalCandidates = Array.from(document.querySelectorAll(
                        '[role="dialog"], [aria-modal="true"], [class*="modal" i], [class*="overlay" i], [class*="interstitial" i], [data-ad], [data-ad-slot], body > div'
                    ));
                    for (var m = modalCandidates.length - 1; m >= 0; m--) {
                        if (looksLikeBlockingAdModal(modalCandidates[m])) return modalCandidates[m];
                    }
                    var frames = Array.from(document.querySelectorAll('iframe'));
                    for (var i = 0; i < frames.length; i++) {
                        var src = (frames[i].src || '').toLowerCase();
                        if (src.indexOf('challenges.cloudflare.com') >= 0 || src.indexOf('turnstile') >= 0) continue;
                        if (frameLooksLikeAdOrInterstitial(frames[i])) return frames[i];
                    }
                } catch (e) {}
                return null;
            }

            function turnstileHasResponse() {
                if (!IS_TOP) return false;
                try {
                    var responseFields = Array.from(document.querySelectorAll(
                        'input[name="cf-turnstile-response"], textarea[name="cf-turnstile-response"], ' +
                        'input[name="g-recaptcha-response"], textarea[name="g-recaptcha-response"], ' +
                        'input[name="h-captcha-response"], textarea[name="h-captcha-response"]'
                    ));
                    if (responseFields.some(function(field) {
                        return String(field.value || field.getAttribute('value') || '').trim().length > 16;
                    })) return true;
                    var text = (document.body && document.body.innerText || '').toLowerCase();
                    return /(^|\s)(success!|verification complete|verified successfully)(\s|$)/i.test(text);
                } catch (e) { return false; }
            }

            function findCloudflareTarget() {
                if (!IS_TOP) return null;
                if (turnstileHasResponse()) return null;
                try {
                    var selectors = [
                        'iframe[src*="challenges.cloudflare.com"]',
                        'iframe[src*="turnstile" i]',
                        'iframe[src*="google.com/recaptcha" i]',
                        'iframe[src*="recaptcha.net" i]',
                        'iframe[src*="hcaptcha.com" i]',
                        'iframe[title*="recaptcha" i]',
                        'iframe[title*="captcha" i]',
                        '.cf-turnstile',
                        '.cf-turnstile-wrapper',
                        '[data-sitekey][class*="turnstile" i]'
                    ];
                    for (var i = 0; i < selectors.length; i++) {
                        var nodes = Array.from(document.querySelectorAll(selectors[i]));
                        for (var j = 0; j < nodes.length; j++) {
                            if (elementVisible(nodes[j])) return nodes[j];
                        }
                    }
                } catch (e) {}
                return null;
            }

            var lastGuideAt = 0;
            function scrollTargetIntoView(target) {
                if (!target || !elementVisible(target)) return;
                var now = Date.now();
                if (now - lastGuideAt < 900) return;
                lastGuideAt = now;
                try { target.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'center' }); } catch (e) {}
            }

            function guideToManualTarget() {
                if (!IS_TOP) {
                    var childAction = findAdActionControl();
                    if (childAction) {
                        scrollTargetIntoView(childAction);
                        try { window.parent.postMessage({ type: GUIDE_MESSAGE }, '*'); } catch (e) {}
                    }
                    return;
                }
                // Ads/interstitials are handled first so they cannot cover Turnstile.
                var blockingAd = findBlockingAdTarget();
                if (blockingAd) {
                    scrollTargetIntoView(blockingAd);
                    return;
                }
                var challenge = findCloudflareTarget();
                if (challenge) scrollTargetIntoView(challenge);
            }

            if (IS_TOP) {
                window.addEventListener('message', function(event) {
                    try {
                        if (!event || !event.data || event.data.type !== GUIDE_MESSAGE) return;
                        var frames = Array.from(document.querySelectorAll('iframe'));
                        for (var i = 0; i < frames.length; i++) {
                            if (frames[i].contentWindow === event.source && elementVisible(frames[i])) {
                                scrollTargetIntoView(frames[i]);
                                break;
                            }
                        }
                    } catch (e) {}
                }, false);
            }

            function isAllowedUserTarget(target) {
                if (IS_TOP && isCloudflareControl(target)) return true;
                return isAdActionControl(target);
            }

            // Hard-lock trusted user input everywhere except the genuine Turnstile
            // surface (which is untouched in its own frame) and explicit ad close /
            // dismiss / consent controls. Synthetic workflow clicks are unaffected.
            function lockUserPointer(e) {
                if (!e.isTrusted) return;
                if (isAllowedUserTarget(e.target)) return;
                e.preventDefault();
                e.stopImmediatePropagation();
                e.stopPropagation();
            }
            [
                'pointerdown', 'pointerup', 'mousedown', 'mouseup', 'click', 'dblclick',
                'contextmenu', 'auxclick', 'touchstart', 'touchend'
            ].forEach(function(type) {
                window.addEventListener(type, lockUserPointer, true);
            });

            // Do not monkey-patch fetch/XHR/URL.createObjectURL/Notification. Turnstile
            // explicitly expects standard browser APIs. Native Tauri on_download is the
            // primary Lua capture path. This is only a passive fallback for an anchor
            // that already contains a blob/data URL.
            function sendToRust(text) {
                if (!text || typeof text !== 'string' || text.length < 10) return;
                var lower = text.toLowerCase();
                if (lower.indexOf('addappid(' + APPID) < 0 && lower.indexOf('addappid (' + APPID) < 0) return;
                try {
                    window.fetch('http://127.0.0.1:' + PORT + '/lua', {
                        method: 'POST',
                        headers: { 'Content-Type': 'text/plain' },
                        body: text
                    }).catch(function() {});
                } catch (e) {}
            }

            var seenDownloadUrls = Object.create(null);
            function inspectDownloadAnchor(anchor) {
                if (!IS_TOP || !anchor || !anchor.href) return;
                var href = String(anchor.href);
                if (seenDownloadUrls[href]) return;
                if (!(href.indexOf('blob:') === 0 || href.indexOf('data:') === 0)) return;
                seenDownloadUrls[href] = true;
                if (href.indexOf('blob:') === 0) {
                    try {
                        window.fetch(href).then(function(r) { return r.text(); }).then(sendToRust).catch(function() {});
                    } catch (e) {}
                    return;
                }
                try {
                    var comma = href.indexOf(',');
                    if (comma < 0) return;
                    var meta = href.slice(0, comma).toLowerCase();
                    var payload = href.slice(comma + 1);
                    var text = meta.indexOf(';base64') >= 0
                        ? decodeURIComponent(escape(window.atob(payload)))
                        : decodeURIComponent(payload);
                    sendToRust(text);
                } catch (e) {}
            }

            if (IS_TOP) {
                document.addEventListener('click', function(e) {
                    try {
                        var anchor = e.target && e.target.closest ? e.target.closest('a') : null;
                        inspectDownloadAnchor(anchor);
                    } catch (err) {}
                }, true);
            }

            try {
                var observer = new MutationObserver(function(records) {
                    guideToManualTarget();
                    if (!IS_TOP) return;
                    for (var i = 0; i < records.length; i++) {
                        var added = records[i].addedNodes || [];
                        for (var j = 0; j < added.length; j++) {
                            var node = added[j];
                            if (!node || node.nodeType !== 1) continue;
                            if (node.matches && node.matches('a[href]')) inspectDownloadAnchor(node);
                            if (node.querySelectorAll) {
                                Array.from(node.querySelectorAll('a[href]')).forEach(inspectDownloadAnchor);
                            }
                        }
                    }
                });
                observer.observe(document.documentElement || document.body, { childList: true, subtree: true, attributes: true });
            } catch (e) {}

            // Ads often reveal the real dismiss button only after a countdown and
            // may do so without a useful DOM mutation. Keep guidance alive so the
            // viewport moves to the newly available close/skip control immediately.
            var guideHeartbeat = setInterval(function() {
                guideToManualTarget();
            }, 600);

            if (!IS_TOP) {
                guideToManualTarget();
                return;
            }

            var gameSelected = false;
            var downloadSubmitted = false;
            var verificationSubmitted = false;
            var lastDownloadButton = null;
            var downloadGeneration = 0;
            var blockerWasVisible = false;

            function pageMatchesTargetGame() {
                try {
                    var url = new URL(window.location.href);
                    if (url.searchParams.get('app') === APPID || url.searchParams.get('appid') === APPID) return true;
                    var text = (document.body && document.body.innerText || '').replace(/\s+/g, ' ');
                    return text.indexOf('(' + APPID + ')') >= 0
                        || text.toLowerCase().indexOf('appid ' + APPID) >= 0
                        || text.toLowerCase().indexOf('appid: ' + APPID) >= 0;
                } catch (e) { return false; }
            }

            function normalizedButtonText(el) {
                return (el && el.textContent || '').replace(/\s+/g, ' ').trim().toLowerCase();
            }

            function enabledActionButton(exactText) {
                var controls = Array.from(document.querySelectorAll('button, [role="button"], a[role="button"]'));
                for (var i = 0; i < controls.length; i++) {
                    var control = controls[i];
                    if (!elementVisible(control)) continue;
                    if (control.disabled || control.getAttribute('aria-disabled') === 'true') continue;
                    var t = normalizedButtonText(control);
                    if (t === exactText) return control;
                }
                return null;
            }

            function findDownloadActionButton() {
                var controls = Array.from(document.querySelectorAll('button, [role="button"], a[role="button"]'));
                for (var i = 0; i < controls.length; i++) {
                    var control = controls[i];
                    if (!elementVisible(control)) continue;
                    if (control.disabled || control.getAttribute('aria-disabled') === 'true') continue;
                    if (control.closest && control.closest('.game-item')) continue;
                    var t = normalizedButtonText(control);
                    if (t === 'download' || t === 'tải xuống' || t === 'tải') return control;
                }
                return null;
            }

            var attempts = 0;
            var interval = setInterval(function() {
                attempts++;
                if (attempts > 180) { clearInterval(interval); return; }

                guideToManualTarget();

                // Once the site itself enables Complete verification, Turnstile has
                // already produced its token. Submit that app-level action once. The
                // site commonly replaces the first Download control after verification,
                // so explicitly re-arm the download state for the new DOM generation.
                if (!verificationSubmitted) {
                    var verifyButton = enabledActionButton('complete verification');
                    if (verifyButton) {
                        verificationSubmitted = true;
                        downloadSubmitted = false;
                        lastDownloadButton = null;
                        downloadGeneration++;
                        verifyButton.click();
                        return;
                    }
                }

                // Never automate through an active challenge or advertisement. Track the
                // transition, though: OpenLua can keep the very same Download button in
                // the DOM while an ad/Turnstile temporarily covers it. When that blocker
                // disappears, the old click belongs to the previous generation and the
                // Download action must be armed again.
                var blockingAdNow = findBlockingAdTarget();
                var challengeNow = findCloudflareTarget();
                var readyDownloadButton = findDownloadActionButton();
                if (blockingAdNow || (challengeNow && !readyDownloadButton)) {
                    blockerWasVisible = true;
                    return;
                }
                if (blockerWasVisible) {
                    blockerWasVisible = false;
                    downloadSubmitted = false;
                    verificationSubmitted = false;
                    lastDownloadButton = null;
                    downloadGeneration++;
                }

                // A submitted Download button can disappear while OpenLua shows an ad /
                // verification stage. When it is replaced or hidden, treat the next
                // visible Download control as a new generation rather than permanently
                // suppressing automation with the old one-shot boolean.
                if (lastDownloadButton && (!lastDownloadButton.isConnected || !elementVisible(lastDownloadButton))) {
                    lastDownloadButton = null;
                    downloadSubmitted = false;
                    verificationSubmitted = false;
                    downloadGeneration++;
                }

                if (!gameSelected && pageMatchesTargetGame()) {
                    gameSelected = true;
                }

                if (!gameSelected) {
                    try {
                        var inputs = Array.from(document.querySelectorAll('input')).filter(elementVisible);
                        var input = inputs.find(function(el) {
                            var hint = ((el.placeholder || '') + ' ' + (el.getAttribute('aria-label') || '')).toLowerCase();
                            return hint.indexOf('app') >= 0 || hint.indexOf('search') >= 0 || hint.indexOf('game') >= 0;
                        }) || inputs[0];
                        if (input && String(input.value || '') !== APPID) {
                            var descriptor = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
                            if (descriptor && descriptor.set) descriptor.set.call(input, APPID);
                            else input.value = APPID;
                            input.dispatchEvent(new Event('input', { bubbles: true }));
                            input.dispatchEvent(new Event('change', { bubbles: true }));
                        }
                        var items = Array.from(document.querySelectorAll('div.cursor-pointer, .game-item, [role="option"]')).filter(function(el) {
                            return elementVisible(el) && (el.innerText || '').indexOf(APPID) >= 0;
                        });
                        if (items.length) {
                            gameSelected = true;
                            items[0].click();
                            return;
                        }
                    } catch (e) {}
                }

                if (gameSelected) {
                    try {
                        var btn = readyDownloadButton || findDownloadActionButton();
                        if (btn) {
                            if (btn === lastDownloadButton && downloadSubmitted) return;
                            lastDownloadButton = btn;
                            downloadSubmitted = true;
                            verificationSubmitted = false;
                            downloadGeneration++;
                            btn.click();
                            return;
                        }
                    } catch (e) {}
                }
            }, 750);
        })();
    "###
    .replace("__PORT__", &local_port.to_string())
    .replace("__APPID__", &appid.to_string());

    // Native download capture is the primary path. OpenLua ultimately downloads a
    // single .lua file, so use Tauri's download hook instead of depending only on
    // fetch/XHR/blob interception inside the provider page.
    let openlua_cache = cache_dir(app, LuaSourceProvider::OpenLua, appid)?;
    fs::create_dir_all(&openlua_cache)
        .map_err(|error| format!("Could not prepare OpenLua download cache: {error}"))?;
    let browser_download_path = openlua_cache.join(format!("{appid}.browser-download.lua"));
    let _ = fs::remove_file(&browser_download_path);
    let download_tx = tx.clone();
    let requested_path = browser_download_path.clone();
    let finished_fallback_path = browser_download_path.clone();

    let window_result = WebviewWindowBuilder::new(
        app,
        &window_label,
        WebviewUrl::External(format!("https://openlua.cloud/?app={}", appid).parse().unwrap())
    )
    .title(format!("OpenLua Cloud Verification / Xác thực - AppID {}", appid))
    .inner_size(600.0, 520.0)
    .center()
    .resizable(false)
    .maximizable(false)
    .visible(false) // Start completely hidden
    .on_download(move |_webview, event| {
        match event {
            DownloadEvent::Requested { destination, .. } => {
                *destination = requested_path.clone();
            }
            DownloadEvent::Finished { path, success, .. } => {
                if success {
                    let candidate_path = path.as_ref().unwrap_or(&finished_fallback_path);
                    if let Ok(bytes) = fs::read(candidate_path) {
                        if bytes.len() <= MAX_LUA_BYTES {
                            if let Ok(text) = String::from_utf8(bytes) {
                                if inspect_provider_lua(appid, &text).is_ok() {
                                    let _ = download_tx.send(text);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        true
    })
    .initialization_script(&inject_js)
    .build();

    if let Err(e) = window_result {
        return Err(format!("Failed to open OpenLua window: {}", e));
    }

    let start_time = std::time::Instant::now();
    let mut window_shown = false;

    let raw_lua = loop {
        match rx.recv_timeout(std::time::Duration::from_millis(300)) {
            Ok(res) => break Ok(res),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // If taking more than 3.5 seconds, show window so user can solve Cloudflare Turnstile if manual click is needed
                if start_time.elapsed() >= std::time::Duration::from_millis(3500) {
                    if let Some(w) = app.get_webview_window(&window_label) {
                        if !window_shown {
                            let _ = w.show();
                            let _ = w.set_focus();
                            window_shown = true;
                        }
                    } else if window_shown {
                        // User closed the window!
                        break Err("Verification cancelled by user".to_string());
                    }
                }
                if start_time.elapsed() >= std::time::Duration::from_secs(90) {
                    break Err("Timeout waiting for OpenLua download or verification (90s)".to_string());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Connection to OpenLua receiver closed".to_string());
            }
        }
    };

    // Close the browser window immediately when download finishes
    if let Some(w) = app.get_webview_window(&window_label) {
        let _ = w.close();
    }

    let raw_lua = raw_lua?;

    let canonical = CanonicalPackage {
        appid,
        provider: LuaPackageProvider::OpenLua,
        revision: format!("openlua-{}", Utc::now().timestamp()),
        canonical_lua: raw_lua,
        manifests: vec![],
        archive_bytes: vec![],
    };

    Ok(Some(canonical))
}

pub(crate) fn fetch_steamtools_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.steamtools_enabled {
        return Ok(None);
    }
    package_from_raw_lua_and_mirrors(app, appid, LuaPackageProvider::SteamTools, None)
}

pub(crate) fn fetch_ryuu_package(
    app: &AppHandle,
    appid: u32,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.ryuu_enabled {
        return Ok(None);
    }
    let key = decrypt_ryuu_key(&settings)?.ok_or_else(|| "RYUU_KEY_REQUIRED".to_string())?;
    let client = ryuu_client(Duration::from_secs(120))?;
    let mut response = client
        .get(ryuu_url(appid)?)
        // Omitting file_type returns the provider ZIP, preserving both the raw Lua
        // and manifests. Live mode strips only setManifestid while the snapshot
        // keeps the exact raw package so the same source can switch to Locked later.
        .header("X-Auth-Key", key.trim())
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("RYUU_NETWORK:{error}"))?;
    match response.status() {
        StatusCode::NOT_FOUND => return Ok(None),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err("RYUU_KEY_INVALID".to_string());
        }
        StatusCode::TOO_MANY_REQUESTS => return Err("RYUU_RATE_LIMITED".to_string()),
        status if !status.is_success() => {
            return Err(format!("RYUU_HTTP_{}", status.as_u16()));
        }
        _ => {}
    }
    if response.content_length().is_some_and(|size| size > MAX_ARCHIVE_BYTES) {
        return Err("RYUU_PACKAGE_TOO_LARGE".to_string());
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("RYUU_DOWNLOAD_READ:{error}"))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err("RYUU_PACKAGE_INVALID_SIZE".to_string());
    }
    package_from_archive(appid, LuaPackageProvider::Ryuu, &bytes).map(Some)
}

fn depotbox_package_from_bytes(
    appid: u32,
    locked: bool,
    bytes: Vec<u8>,
) -> Result<CanonicalPackage, String> {
    let is_zip = bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08");
    if locked {
        if !is_zip {
            return Err("DEPOTBOX_LOCKED_REQUIRES_ZIP_DOWNLOAD".to_string());
        }
        let package = package_from_archive(appid, LuaPackageProvider::TwentyTwoCloud, &bytes)?;
        if package.manifests.is_empty() {
            return Err("DEPOTBOX_LOCKED_REQUIRES_MANIFEST_PACKAGE".to_string());
        }
        return Ok(package);
    }
    if is_zip {
        return Err("DEPOTBOX_LIVE_REQUIRES_LUA_DOWNLOAD".to_string());
    }
    if bytes.is_empty() || bytes.len() > MAX_LUA_BYTES || bytes.contains(&0) {
        return Err("DEPOTBOX_PAYLOAD_INVALID".to_string());
    }
    let source = String::from_utf8(bytes.clone())
        .map_err(|_| "DEPOTBOX_LUA_NOT_UTF8".to_string())?;
    inspect_provider_lua(appid, &source)?;
    let archive_bytes = build_canonical_archive(
        appid,
        LuaPackageProvider::TwentyTwoCloud,
        &source,
        &[],
    )?;
    Ok(CanonicalPackage {
        appid,
        provider: LuaPackageProvider::TwentyTwoCloud,
        revision: sha256_bytes(&bytes),
        canonical_lua: source,
        manifests: Vec::new(),
        archive_bytes,
    })
}

fn depotbox_api_url(path_or_url: &str) -> Result<Url, String> {
    let url = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        Url::parse(path_or_url).map_err(|error| format!("DEPOTBOX_DOWNLOAD_URL_INVALID:{error}"))?
    } else {
        Url::parse(DEPOTBOX_BASE)
            .map_err(|error| format!("DEPOTBOX_BASE_URL_INVALID:{error}"))?
            .join(path_or_url)
            .map_err(|error| format!("DEPOTBOX_DOWNLOAD_URL_INVALID:{error}"))?
    };
    if url.scheme() != "https" || url.host_str() != Some("depotbox.org") {
        return Err("DEPOTBOX_DOWNLOAD_URL_NOT_ALLOWED".to_string());
    }
    Ok(url)
}

fn depotbox_http_error(status: StatusCode, body: Option<&str>) -> String {
    let suffix = body
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(180).collect::<String>());
    match status {
        StatusCode::UNAUTHORIZED => "DEPOTBOX_KEY_REQUIRED".to_string(),
        StatusCode::FORBIDDEN => "DEPOTBOX_KEY_INVALID_OR_FORBIDDEN".to_string(),
        StatusCode::NOT_FOUND => "DEPOTBOX_APP_NOT_AVAILABLE".to_string(),
        StatusCode::TOO_MANY_REQUESTS => suffix
            .map(|value| format!("DEPOTBOX_RATE_LIMITED:{value}"))
            .unwrap_or_else(|| "DEPOTBOX_RATE_LIMITED".to_string()),
        _ => suffix
            .map(|value| format!("DEPOTBOX_HTTP_{}:{value}", status.as_u16()))
            .unwrap_or_else(|| format!("DEPOTBOX_HTTP_{}", status.as_u16())),
    }
}

fn fetch_depotbox_api_package(
    appid: u32,
    key: &str,
    locked: bool,
) -> Result<CanonicalPackage, String> {
    let client = source_client(Duration::from_secs(930))?;
    let start_path = if locked { "/api/download" } else { "/api/lua" };
    let start_url = depotbox_api_url(start_path)?;
    let mut start = client
        .post(start_url)
        .header("X-API-Key", key)
        .json(&json!({ "appid": appid.to_string() }))
        .send()
        .map_err(|error| format!("DEPOTBOX_START_FAILED:{error}"))?;
    if start.status() != StatusCode::ACCEPTED && !start.status().is_success() {
        let status = start.status();
        let body = start.text().ok();
        return Err(depotbox_http_error(status, body.as_deref()));
    }
    let start_payload: Value = start
        .json()
        .map_err(|_| "DEPOTBOX_START_RESPONSE_INVALID".to_string())?;
    let token = start_payload
        .get("token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .ok_or_else(|| "DEPOTBOX_START_TOKEN_MISSING".to_string())?;
    let polling_path = start_payload
        .get("polling_url")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("/api/status/{token}"));
    let polling_url = depotbox_api_url(&polling_path)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(900);
    let download_link = loop {
        if std::time::Instant::now() >= deadline {
            return Err("DEPOTBOX_GENERATION_TIMEOUT".to_string());
        }
        let status_response = client
            .get(polling_url.clone())
            .send()
            .map_err(|error| format!("DEPOTBOX_STATUS_FAILED:{error}"))?;
        if !status_response.status().is_success() {
            let status = status_response.status();
            let body = status_response.text().ok();
            return Err(depotbox_http_error(status, body.as_deref()));
        }
        let payload: Value = status_response
            .json()
            .map_err(|_| "DEPOTBOX_STATUS_RESPONSE_INVALID".to_string())?;
        let state = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        match state.as_str() {
            "completed" | "completed_with_warnings" => {
                let link = payload
                    .get("download_link")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "DEPOTBOX_DOWNLOAD_LINK_MISSING".to_string())?;
                break link.to_string();
            }
            "failed" | "invalid_or_expired" => {
                let message = payload
                    .get("failureReason")
                    .or_else(|| payload.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("generation failed")
                    .chars()
                    .take(180)
                    .collect::<String>();
                return Err(format!("DEPOTBOX_GENERATION_FAILED:{message}"));
            }
            _ => std::thread::sleep(Duration::from_secs(2)),
        }
    };

    let download_url = depotbox_api_url(&download_link)?;
    let mut response = client
        .get(download_url)
        .send()
        .map_err(|error| format!("DEPOTBOX_DOWNLOAD_FAILED:{error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().ok();
        return Err(depotbox_http_error(status, body.as_deref()));
    }
    let max_bytes = if locked { MAX_ARCHIVE_BYTES } else { MAX_LUA_BYTES as u64 };
    if response.content_length().is_some_and(|size| size > max_bytes) {
        return Err("DEPOTBOX_PACKAGE_TOO_LARGE".to_string());
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("DEPOTBOX_DOWNLOAD_READ:{error}"))?;
    if bytes.is_empty() || bytes.len() as u64 > max_bytes {
        return Err("DEPOTBOX_PACKAGE_INVALID_SIZE".to_string());
    }
    depotbox_package_from_bytes(appid, locked, bytes)
}

fn fetch_depotbox_web_package(
    app: &AppHandle,
    appid: u32,
    locked: bool,
) -> Result<CanonicalPackage, String> {
    use tauri::webview::DownloadEvent;
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    let cache = cache_dir(app, LuaSourceProvider::TwentyTwoCloud, appid)?;
    fs::create_dir_all(&cache)
        .map_err(|error| format!("Could not prepare DepotBox cache: {error}"))?;
    let destination_name = if locked {
        format!("{appid}.zip")
    } else {
        format!("{appid}.lua")
    };
    let destination_path = cache.join(destination_name);
    let _ = fs::remove_file(&destination_path);

    // DepotBox's website can generate the file in page JavaScript and then try to
    // hand it to the browser through a Blob/anchor download. A synthetic click does
    // not provide a trusted browser user gesture, so WebView2 may never emit a native
    // DownloadStarting event. Keep the native Tauri download hook, but also expose a
    // one-shot loopback receiver so the page can hand the already-generated bytes to
    // Rust directly without relying on the browser download UI.
    let browser_listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Failed to bind DepotBox capture port: {error}"))?;
    let browser_port = browser_listener
        .local_addr()
        .map_err(|error| format!("Failed to inspect DepotBox capture port: {error}"))?
        .port();
    browser_listener
        .set_nonblocking(true)
        .map_err(|error| format!("Failed to prepare DepotBox capture listener: {error}"))?;
    let browser_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, String>>();
    let browser_tx = tx.clone();
    let browser_stop_thread = browser_stop.clone();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read as _, Write as _};
        use std::sync::atomic::Ordering;

        while !browser_stop_thread.load(Ordering::Relaxed) {
            let (mut stream, _) = match browser_listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(40));
                    continue;
                }
                Err(_) => break,
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
            let mut reader = BufReader::new(&mut stream);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).unwrap_or(0) == 0 {
                continue;
            }
            let is_options = first_line.starts_with("OPTIONS ");
            let is_capture = first_line.starts_with("POST /depotbox ");

            let mut content_length = 0usize;
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break;
                }
                if let Some(value) = trimmed
                    .strip_prefix("Content-Length:")
                    .or_else(|| trimmed.strip_prefix("content-length:"))
                {
                    content_length = value.trim().parse::<usize>().unwrap_or(0);
                }
                line.clear();
            }

            let response = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nCache-Control: no-store\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";

            if is_options {
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                continue;
            }
            if !is_capture || content_length == 0 || content_length as u64 > MAX_ARCHIVE_BYTES {
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                continue;
            }

            let mut body = vec![0u8; content_length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();

            let valid = if locked {
                body.starts_with(b"PK\x03\x04")
            } else {
                String::from_utf8(body.clone()).ok().is_some_and(|text| {
                    if text.len() > MAX_LUA_BYTES {
                        return false;
                    }
                    let lower = text.to_ascii_lowercase();
                    lower.contains(&format!("addappid({appid}"))
                        || lower.contains(&format!("addappid ({appid}"))
                })
            };
            if valid {
                crate::debug_log::debug_log(&format!(
                    "DEPOTBOX_BROWSER_CAPTURED: {} bytes via loopback bridge",
                    body.len()
                ));
                let _ = browser_tx.send(Ok(body));
                break;
            }
        }
    });

    let download_tx = tx.clone();
    let requested_path = destination_path.clone();
    let finished_fallback_path = destination_path.clone();
    let label = format!("depotbox-dl-{appid}");
    let mode_label = if locked { "LOCKED (.zip)" } else { "LIVE (.lua)" };

    // Free Web mode is fully automatic after the user has already chosen the
    // channel in the launcher. `locked == false` means LIVE -> Download .lua;
    // `locked == true` means LOCKED -> Download .zip. The embedded page handles
    // AppID search, game selection, and the matching purple download button. The
    // generated bytes are captured directly; the green ready/download notice is never
    // clicked. Verification widgets remain manual when DepotBox explicitly requires them.
    let inject_js = r###"
        (function() {
            'use strict';

            var APPID = '__APPID__';
            var WANT_LOCKED = __LOCKED__;
            var PORT = Number('__PORT__');
            var host = '';
            try { host = String(window.location && window.location.hostname || '').toLowerCase(); } catch (e) {}

            // Tauri/WebView2 can install init scripts in Windows subframes. Keep all
            // third-party verification frames pristine and automate only DepotBox top-level.
            var IS_DEPOTBOX = host === 'depotbox.org' || host.endsWith('.depotbox.org');
            if (!IS_DEPOTBOX) return;
            var IS_TOP = false;
            try { IS_TOP = window.top === window.self; } catch (e) { IS_TOP = false; }
            if (!IS_TOP) return;

            // DepotBox can finish generation without creating a WebView2 native
            // download event (notably when its automatic handoff is Blob/JS based
            // and the launcher initiated the purple action synthetically). Capture
            // the generated payload inside the page and pass the bytes to Rust over
            // the same one-shot loopback pattern already used by the launcher.
            var NATIVE_FETCH = window.fetch ? window.fetch.bind(window) : null;
            var NATIVE_CREATE_OBJECT_URL = window.URL && window.URL.createObjectURL
                ? window.URL.createObjectURL.bind(window.URL) : null;
            var NATIVE_ANCHOR_CLICK = window.HTMLAnchorElement && window.HTMLAnchorElement.prototype.click;
            var capturePending = false;
            var captureSent = false;
            window.__OXO_DEPOTBOX_CAPTURE = { status: 'armed', source: '' };

            function bytesMatchRequestedChannel(buffer) {
                try {
                    if (!buffer || typeof buffer.byteLength !== 'number' || buffer.byteLength <= 0) return false;
                    if (buffer.byteLength > 134217728) return false;
                    var bytes = new Uint8Array(buffer);
                    if (WANT_LOCKED) {
                        return bytes.length >= 4 && bytes[0] === 0x50 && bytes[1] === 0x4b
                            && bytes[2] === 0x03 && bytes[3] === 0x04;
                    }
                    if (bytes.length > 1048576) return false;
                    var text = new TextDecoder('utf-8').decode(bytes).toLowerCase();
                    return text.indexOf('addappid(' + APPID) >= 0
                        || text.indexOf('addappid (' + APPID) >= 0;
                } catch (e) { return false; }
            }

            function postCapturedBuffer(buffer, source) {
                if (!NATIVE_FETCH || captureSent || capturePending || !bytesMatchRequestedChannel(buffer)) return;
                capturePending = true;
                window.__OXO_DEPOTBOX_CAPTURE = { status: 'posting', source: String(source || '') };
                try {
                    NATIVE_FETCH('http://127.0.0.1:' + PORT + '/depotbox', {
                        method: 'POST',
                        mode: 'cors',
                        cache: 'no-store',
                        headers: { 'Content-Type': 'application/octet-stream' },
                        body: new Uint8Array(buffer)
                    }).then(function(response) {
                        if (!response || !response.ok) throw new Error('loopback rejected payload');
                        captureSent = true;
                        capturePending = false;
                        window.__OXO_DEPOTBOX_CAPTURE = { status: 'sent', source: String(source || '') };
                    }).catch(function() {
                        capturePending = false;
                        window.__OXO_DEPOTBOX_CAPTURE = { status: 'retryable', source: String(source || '') };
                    });
                } catch (e) {
                    capturePending = false;
                    window.__OXO_DEPOTBOX_CAPTURE = { status: 'retryable', source: String(source || '') };
                }
            }

            function captureDepotboxPayload(value, source) {
                if (captureSent || !value) return;
                try {
                    if (value instanceof Blob) {
                        value.arrayBuffer().then(function(buffer) { postCapturedBuffer(buffer, source); }).catch(function() {});
                        return;
                    }
                    if (value instanceof ArrayBuffer) {
                        postCapturedBuffer(value, source);
                        return;
                    }
                    if (ArrayBuffer.isView(value)) {
                        var copy = value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength);
                        postCapturedBuffer(copy, source);
                    }
                } catch (e) {}
            }

            function maybeCaptureResponse(response) {
                if (captureSent || !response || !response.ok) return;
                try {
                    var url = String(response.url || '').toLowerCase();
                    var disposition = String(response.headers.get('content-disposition') || '').toLowerCase();
                    var contentType = String(response.headers.get('content-type') || '').toLowerCase();
                    var likely = disposition.indexOf('attachment') >= 0
                        || disposition.indexOf('.lua') >= 0
                        || disposition.indexOf('.zip') >= 0
                        || contentType.indexOf('application/zip') >= 0
                        || contentType.indexOf('application/octet-stream') >= 0
                        || contentType.indexOf('text/plain') >= 0
                        || contentType.indexOf('lua') >= 0
                        || url.indexOf('download') >= 0
                        || url.indexOf('.lua') >= 0
                        || url.indexOf('.zip') >= 0;
                    if (!likely) return;
                    response.clone().arrayBuffer().then(function(buffer) {
                        postCapturedBuffer(buffer, 'fetch:' + String(response.url || ''));
                    }).catch(function() {});
                } catch (e) {}
            }

            if (NATIVE_FETCH) {
                window.fetch = function(input, init) {
                    return NATIVE_FETCH(input, init).then(function(response) {
                        try { maybeCaptureResponse(response); } catch (e) {}
                        return response;
                    });
                };
            }

            if (NATIVE_CREATE_OBJECT_URL) {
                window.URL.createObjectURL = function(value) {
                    try { captureDepotboxPayload(value, 'blob:createObjectURL'); } catch (e) {}
                    return NATIVE_CREATE_OBJECT_URL(value);
                };
            }

            function inspectDownloadAnchor(anchor) {
                if (captureSent || !anchor) return;
                try {
                    var href = String(anchor.href || '');
                    var downloadName = String(anchor.getAttribute('download') || '').toLowerCase();
                    var text = normalizedText(anchor);
                    var expected = (APPID + (WANT_LOCKED ? '.zip' : '.lua')).toLowerCase();
                    var relevant = downloadName === expected
                        || text.indexOf(expected) >= 0
                        || href.toLowerCase().indexOf(expected) >= 0;
                    if (!relevant || !href || !NATIVE_FETCH) return;
                    if (href.indexOf('blob:') === 0 || href.indexOf('data:') === 0) {
                        NATIVE_FETCH(href).then(function(r) { return r.arrayBuffer(); })
                            .then(function(buffer) { postCapturedBuffer(buffer, 'anchor:' + href.slice(0, 64)); })
                            .catch(function() {});
                        return;
                    }
                    var parsed = new URL(href, window.location.href);
                    if (parsed.protocol === 'https:' && (parsed.hostname === 'depotbox.org' || parsed.hostname.endsWith('.depotbox.org'))) {
                        NATIVE_FETCH(parsed.href, { credentials: 'include', cache: 'no-store' })
                            .then(function(r) { return r.arrayBuffer(); })
                            .then(function(buffer) { postCapturedBuffer(buffer, 'anchor:' + parsed.pathname); })
                            .catch(function() {});
                    }
                } catch (e) {}
            }

            if (NATIVE_ANCHOR_CLICK) {
                window.HTMLAnchorElement.prototype.click = function() {
                    try { inspectDownloadAnchor(this); } catch (e) {}
                    return NATIVE_ANCHOR_CLICK.call(this);
                };
            }

            try {
                var NativeXhrOpen = XMLHttpRequest.prototype.open;
                var NativeXhrSend = XMLHttpRequest.prototype.send;
                XMLHttpRequest.prototype.open = function(method, url) {
                    try { this.__oxoDepotboxUrl = String(url || ''); } catch (e) {}
                    return NativeXhrOpen.apply(this, arguments);
                };
                XMLHttpRequest.prototype.send = function() {
                    if (!this.__oxoDepotboxCaptureHook) {
                        this.__oxoDepotboxCaptureHook = true;
                        this.addEventListener('loadend', function() {
                            if (captureSent || this.status < 200 || this.status >= 300) return;
                            try {
                                var type = String(this.responseType || '');
                                if (type === 'blob' && this.response instanceof Blob) {
                                    captureDepotboxPayload(this.response, 'xhr:' + String(this.__oxoDepotboxUrl || ''));
                                } else if (type === 'arraybuffer' && this.response instanceof ArrayBuffer) {
                                    captureDepotboxPayload(this.response, 'xhr:' + String(this.__oxoDepotboxUrl || ''));
                                } else if ((type === '' || type === 'text') && typeof this.responseText === 'string') {
                                    var encoded = new TextEncoder().encode(this.responseText);
                                    captureDepotboxPayload(encoded, 'xhr-text:' + String(this.__oxoDepotboxUrl || ''));
                                }
                            } catch (e) {}
                        });
                    }
                    return NativeXhrSend.apply(this, arguments);
                };
            } catch (e) {}

            function visible(el) {
                try {
                    if (!el || !el.isConnected) return false;
                    var r = el.getBoundingClientRect();
                    var st = window.getComputedStyle ? window.getComputedStyle(el) : null;
                    if (st && (st.display === 'none' || st.visibility === 'hidden' || Number(st.opacity) === 0)) return false;
                    return r.width > 2 && r.height > 2;
                } catch (e) { return false; }
            }

            function normalizedText(el) {
                try {
                    return String((el && (el.innerText || el.textContent)) || '')
                        .replace(/\s+/g, ' ').trim().toLowerCase();
                } catch (e) { return ''; }
            }

            function disabled(el) {
                try {
                    return !!(el.disabled || el.getAttribute('aria-disabled') === 'true'
                        || el.classList.contains('disabled'));
                } catch (e) { return false; }
            }

            function scrollTo(el) {
                try { if (el && visible(el)) el.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'nearest' }); } catch (e) {}
            }

            function setInputValue(input, value) {
                try {
                    var proto = input instanceof HTMLTextAreaElement ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
                    var descriptor = Object.getOwnPropertyDescriptor(proto, 'value');
                    if (descriptor && descriptor.set) descriptor.set.call(input, value);
                    else input.value = value;
                    input.dispatchEvent(new Event('input', { bubbles: true }));
                    input.dispatchEvent(new Event('change', { bubbles: true }));
                    return true;
                } catch (e) { return false; }
            }

            function findSearchInput() {
                var inputs = Array.from(document.querySelectorAll('input[type="search"], input[type="text"], input:not([type])')).filter(visible);
                var scored = inputs.map(function(input) {
                    var hint = [input.placeholder || '', input.getAttribute('aria-label') || '', input.name || '', input.id || '']
                        .join(' ').toLowerCase();
                    var score = 0;
                    if (hint.indexOf('appid') >= 0) score += 8;
                    if (hint.indexOf('half-life') >= 0) score += 7;
                    if (hint.indexOf('search') >= 0) score += 5;
                    if (hint.indexOf('game') >= 0) score += 3;
                    return { input: input, score: score };
                }).sort(function(a, b) { return b.score - a.score; });
                return scored.length ? scored[0].input : null;
            }

            function findSearchButton() {
                var controls = Array.from(document.querySelectorAll('button, [role="button"], input[type="submit"]')).filter(visible);
                for (var i = 0; i < controls.length; i++) {
                    var text = normalizedText(controls[i]);
                    var value = String(controls[i].value || '').trim().toLowerCase();
                    if ((text === 'search' || value === 'search') && !disabled(controls[i])) return controls[i];
                }
                return null;
            }

            function hasSelectedTarget() {
                try {
                    var bodyText = normalizedText(document.body);
                    if (bodyText.indexOf('selected game') < 0 || bodyText.indexOf(APPID) < 0) return false;
                    return !!findRequestedDownload();
                } catch (e) { return false; }
            }

            function clickableAncestor(el) {
                try {
                    return el && el.closest && el.closest('button, a[href], [role="button"], [role="option"], [tabindex], article, li, tr, [class*="card" i], [class*="result" i], [class*="game" i]');
                } catch (e) { return null; }
            }

            function findTargetGame() {
                try {
                    var exact = document.querySelector('[data-appid="' + APPID + '"], [data-app-id="' + APPID + '"]');
                    if (exact && visible(exact)) return clickableAncestor(exact) || exact;

                    var nodes = Array.from(document.querySelectorAll(
                        '[role="option"], article, li, tr, [class*="result" i], [class*="game-card" i], [class*="game_item" i], [class*="game-item" i]'
                    ));
                    var matches = [];
                    nodes.forEach(function(node) {
                        if (!visible(node)) return;
                        var text = normalizedText(node);
                        if (text.indexOf(APPID) < 0) return;
                        // Do not mistake the already-selected panel for a search result.
                        if (text.indexOf('download .lua') >= 0 || text.indexOf('download .zip') >= 0 || text.indexOf('selected game') >= 0) return;
                        var click = clickableAncestor(node) || node;
                        var r = click.getBoundingClientRect();
                        matches.push({ node: click, area: Math.max(1, r.width * r.height) });
                    });
                    matches.sort(function(a, b) { return a.area - b.area; });
                    return matches.length ? matches[0].node : null;
                } catch (e) { return null; }
            }

            function findRequestedDownload() {
                var wanted = WANT_LOCKED ? 'download .zip' : 'download .lua';
                var controls = Array.from(document.querySelectorAll('button, a, [role="button"]')).filter(visible);
                for (var i = 0; i < controls.length; i++) {
                    if (disabled(controls[i])) continue;
                    var text = normalizedText(controls[i]);
                    if (text === wanted || text.indexOf(wanted) === 0) return controls[i];
                }
                return null;
            }

            function generationHasStarted() {
                try {
                    var text = normalizedText(document.body);
                    return text.indexOf('download process started') >= 0
                        || text.indexOf('starting .lua generation') >= 0
                        || text.indexOf('starting .zip generation') >= 0
                        || text.indexOf('generating .lua') >= 0
                        || text.indexOf('generating .zip') >= 0
                        || text.indexOf('download ready') >= 0
                        || text.indexOf('generated ' + APPID.toLowerCase() + '.lua') >= 0
                        || text.indexOf('generated ' + APPID.toLowerCase() + '.zip') >= 0;
                } catch (e) { return false; }
            }

            function verificationTokenPresent() {
                try {
                    var token = document.querySelector('input[name="cf-turnstile-response"], textarea[name="cf-turnstile-response"], input[name="g-recaptcha-response"], textarea[name="g-recaptcha-response"]');
                    return !!(token && String(token.value || token.textContent || '').trim().length > 8);
                } catch (e) { return false; }
            }

            function findVerificationTarget() {
                try {
                    var selectors = [
                        'iframe[src*="challenges.cloudflare.com"]',
                        'iframe[src*="turnstile" i]',
                        'iframe[src*="recaptcha" i]',
                        '.cf-turnstile',
                        '[data-sitekey][class*="turnstile" i]'
                    ];
                    for (var s = 0; s < selectors.length; s++) {
                        var nodes = Array.from(document.querySelectorAll(selectors[s]));
                        for (var i = 0; i < nodes.length; i++) if (visible(nodes[i])) return nodes[i];
                    }
                } catch (e) {}
                return null;
            }

            function isVerificationActive() {
                // A rendered iframe can remain visible after Cloudflare says Success.
                // A populated response token means verification is complete.
                if (verificationTokenPresent()) return false;
                return !!findVerificationTarget();
            }

            var targetSelected = false;
            var verificationWasActive = false;
            // Launcher already knows the requested channel before this WebView opens:
            // WANT_LOCKED=false => LIVE/.lua, WANT_LOCKED=true => LOCKED/.zip.
            // Search and download each get one initial click and at most one replay after
            // a verification gate. We never ask the user to choose the purple button here.
            var searchPhase = 'idle';
            var downloadPhase = 'idle';
            var lastGameClick = 0;
            window.__OXO_DEPOTBOX_NATIVE_DOWNLOAD_STARTED = false;

            function tick() {
                var now = Date.now();

                // Once generation starts, the purple action has already done its job.
                // Do not click the green "Download <file>" notice: it is not part of the
                // launcher workflow and another synthetic click has the same user-gesture
                // problem. Native on_download and the page-level byte bridge above race to
                // deliver the generated payload to Rust.
                if (generationHasStarted()) return;

                var verifyTarget = findVerificationTarget();
                if (isVerificationActive()) {
                    verificationWasActive = true;
                    if (searchPhase === 'waiting-after-click') searchPhase = 'waiting-verification';
                    if (downloadPhase === 'waiting-after-click') downloadPhase = 'waiting-verification';
                    scrollTo(verifyTarget);
                    return; // verification widgets are manual; never click/solve them.
                }
                if (verificationWasActive) {
                    verificationWasActive = false;
                    lastGameClick = 0;
                    if (searchPhase === 'waiting-verification') searchPhase = 'verification-cleared';
                    if (downloadPhase === 'waiting-verification') downloadPhase = 'verification-cleared';
                }

                if (!targetSelected && hasSelectedTarget()) {
                    targetSelected = true;
                    searchPhase = 'done';
                }

                if (!targetSelected) {
                    var input = findSearchInput();
                    if (input && String(input.value || '').trim() !== APPID) {
                        setInputValue(input, APPID);
                        scrollTo(input);
                        return;
                    }

                    var game = findTargetGame();
                    if (game && now - lastGameClick > 1800) {
                        searchPhase = 'done';
                        lastGameClick = now;
                        scrollTo(game);
                        try { game.click(); } catch (e) {}
                        return;
                    }

                    var search = findSearchButton();
                    if (input && String(input.value || '').trim() === APPID && search) {
                        if (searchPhase === 'verification-cleared') {
                            searchPhase = 'post-verify-clicked';
                            scrollTo(search);
                            try { search.click(); } catch (e) {}
                            return;
                        }
                        if (searchPhase === 'idle') {
                            searchPhase = 'waiting-after-click';
                            scrollTo(search);
                            try { search.click(); } catch (e) {}
                            return;
                        }
                    }
                    return;
                }

                // The channel was selected upstream in the launcher. Automatically click
                // exactly one purple button: LIVE -> Download .lua, LOCKED -> Download .zip.
                var download = findRequestedDownload();
                if (!download) return;
                if (downloadPhase === 'verification-cleared') {
                    downloadPhase = 'post-verify-clicked';
                    scrollTo(download);
                    try { download.click(); } catch (e) {}
                    return;
                }
                if (downloadPhase === 'idle') {
                    downloadPhase = 'waiting-after-click';
                    scrollTo(download);
                    try { download.click(); } catch (e) {}
                    return;
                }
            }

            var observer = new MutationObserver(function(records) {
                try {
                    for (var r = 0; r < records.length; r++) {
                        var added = records[r].addedNodes || [];
                        for (var a = 0; a < added.length; a++) {
                            var node = added[a];
                            if (!node || node.nodeType !== 1) continue;
                            if (node.matches && node.matches('a[href]')) inspectDownloadAnchor(node);
                            if (node.querySelectorAll) Array.from(node.querySelectorAll('a[href]')).forEach(inspectDownloadAnchor);
                        }
                    }
                } catch (e) {}
                setTimeout(tick, 60);
            });
            try { observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['disabled', 'aria-disabled', 'class', 'href', 'download'] }); } catch (e) {}
            setInterval(tick, 500);
            if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', tick, { once: true });
            else setTimeout(tick, 50);
        })();
    "###
    .replace("__APPID__", &appid.to_string())
    .replace("__LOCKED__", if locked { "true" } else { "false" })
    .replace("__PORT__", &browser_port.to_string());

    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(format!("{DEPOTBOX_BASE}/").parse().unwrap()),
    )
    .title(format!("DepotBox {mode_label} - AppID {appid}"))
    .inner_size(980.0, 760.0)
    .min_inner_size(980.0, 760.0)
    .max_inner_size(980.0, 760.0)
    .center()
    .resizable(false)
    .maximizable(false)
    .zoom_hotkeys_enabled(false)
    .initialization_script(&inject_js)
    .on_download(move |webview, event| {
        match event {
            DownloadEvent::Requested { url, destination } => {
                *destination = requested_path.clone();
                let _ = webview.eval("window.__OXO_DEPOTBOX_NATIVE_DOWNLOAD_STARTED = true;");
                crate::debug_log::debug_log(&format!(
                    "DepotBox download requested: url={} destination={}",
                    url,
                    destination.display()
                ));
            }
            DownloadEvent::Finished { url, path, success } => {
                let candidate_path = path.as_ref().unwrap_or(&finished_fallback_path);
                crate::debug_log::debug_log(&format!(
                    "DepotBox download finished: url={} success={} path={}",
                    url,
                    success,
                    candidate_path.display()
                ));

                if !success {
                    let _ = download_tx.send(Err("DEPOTBOX_DOWNLOAD_FAILED".to_string()));
                    return true;
                }

                match fs::read(candidate_path) {
                    Ok(bytes) if bytes.is_empty() => {
                        let _ = download_tx.send(Err("DEPOTBOX_DOWNLOAD_EMPTY".to_string()));
                    }
                    Ok(bytes) if bytes.len() as u64 > MAX_ARCHIVE_BYTES => {
                        let _ = download_tx.send(Err("DEPOTBOX_DOWNLOAD_TOO_LARGE".to_string()));
                    }
                    Ok(bytes) => {
                        crate::debug_log::debug_log(&format!(
                            "DepotBox download captured: {} bytes",
                            bytes.len()
                        ));
                        let _ = download_tx.send(Ok(bytes));
                    }
                    Err(error) => {
                        crate::debug_log::debug_log(&format!(
                            "DepotBox download read failed: {} ({error})",
                            candidate_path.display()
                        ));
                        let _ = download_tx.send(Err("DEPOTBOX_DOWNLOAD_READ_FAILED".to_string()));
                    }
                }
            }
            _ => {}
        }
        true
    })
    .build()
    .map_err(|error| {
        browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        format!("Could not open DepotBox: {error}")
    })?;
    let _ = window.show();
    let _ = window.set_focus();

    let started = std::time::Instant::now();
    let mut last_seen_download_size: Option<(u64, std::time::Instant)> = None;
    let bytes = loop {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(Ok(bytes)) => break bytes,
            Ok(Err(error)) => {
                browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.close();
                }
                return Err(error);
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Some WebView2 versions can materialize the destination before the
                // Finished callback reaches Tauri. Recover only after the size has
                // remained unchanged long enough to avoid reading a partial file.
                if let Ok(metadata) = fs::metadata(&destination_path) {
                    let size = metadata.len();
                    if size > MAX_ARCHIVE_BYTES {
                        browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                        if let Some(window) = app.get_webview_window(&label) {
                            let _ = window.close();
                        }
                        return Err("DEPOTBOX_DOWNLOAD_TOO_LARGE".to_string());
                    }
                    if size > 0 {
                        match last_seen_download_size {
                            Some((previous_size, stable_since))
                                if previous_size == size
                                    && stable_since.elapsed() >= Duration::from_millis(750) =>
                            {
                                if let Ok(recovered) = fs::read(&destination_path) {
                                    if recovered.len() as u64 == size {
                                        crate::debug_log::debug_log(&format!(
                                            "DepotBox download recovered from destination: {} bytes path={}",
                                            recovered.len(),
                                            destination_path.display()
                                        ));
                                        break recovered;
                                    }
                                }
                            }
                            Some((previous_size, _)) if previous_size == size => {}
                            _ => {
                                last_seen_download_size = Some((size, std::time::Instant::now()));
                            }
                        }
                    }
                }

                if app.get_webview_window(&label).is_none() {
                    browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    return Err("DEPOTBOX_DOWNLOAD_CANCELLED".to_string());
                }
                if started.elapsed() > Duration::from_secs(900) {
                    browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(window) = app.get_webview_window(&label) {
                        let _ = window.close();
                    }
                    return Err("DEPOTBOX_DOWNLOAD_TIMEOUT".to_string());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                return Err("DEPOTBOX_DOWNLOAD_CHANNEL_CLOSED".to_string());
            }
        }
    };
    browser_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    depotbox_package_from_bytes(appid, locked, bytes)
}

pub(crate) fn fetch_depotbox_package(
    app: &AppHandle,
    appid: u32,
    locked: bool,
) -> Result<Option<CanonicalPackage>, String> {
    let settings = load_settings(app)?;
    if !settings.twenty_two_cloud_enabled {
        return Ok(None);
    }
    if let Some(key) = decrypt_depotbox_key(&settings)? {
        return fetch_depotbox_api_package(appid, &key, locked).map(Some);
    }
    fetch_depotbox_web_package(app, appid, locked).map(Some)
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

    // Do not preflight /status here. The bundle generation endpoint is already
    // authoritative and a status request immediately before it doubles request
    // pressure, which is especially harmful when the provider returns HTTP 429.
    let generated = download_to_part(
        app,
        LuaSourceProvider::Hubcap,
        appid,
        &client,
        &key,
        &format!("/api/v1/manifest/{appid}"),
        "hubcap-lua-manifest-bundle.zip",
        MAX_ARCHIVE_BYTES,
    )?;
    let package = package_from_archive(appid, LuaPackageProvider::Hubcap, &generated)?;
    let final_path = cache_dir(app, LuaSourceProvider::Hubcap, appid)?
        .join(format!("{}.zip", package.revision));
    crate::lua_live::atomic_write_path(&final_path, &package.archive_bytes)?;
    Ok(package)
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
    fn canonicalizer_validates_without_reformatting_provider_lua() {
        let source = "-- provider metadata\r\naddappid(10)\r\nsetManifestid(11, \"123456789\")\r\n";
        let (validated, manifests) = canonicalize_lua(10, source).unwrap();
        assert_eq!(validated, source);
        assert_eq!(manifests.get(&11).map(String::as_str), Some("123456789"));
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

        assert!(LuaSourceProvider::OpenLua.accepts(LuaPackageProvider::OpenLua));
        assert!(!LuaSourceProvider::OpenLua.accepts(LuaPackageProvider::Hubcap));
        assert!(!LuaSourceProvider::Hubcap.accepts(LuaPackageProvider::OpenLua));

        assert!(LuaSourceProvider::Ryuu.accepts(LuaPackageProvider::Ryuu));
        assert!(!LuaSourceProvider::Ryuu.accepts(LuaPackageProvider::Sushi));

        assert!(LuaSourceProvider::Luie.accepts(LuaPackageProvider::Luie));
        assert!(LuaSourceProvider::TwentyTwoCloud.accepts(LuaPackageProvider::TwentyTwoCloud));
        assert!(LuaSourceProvider::Skyflare.accepts(LuaPackageProvider::Skyflare));
        assert!(!LuaSourceProvider::Luie.accepts(LuaPackageProvider::Skyflare));
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
            LuaPackageProvider::OpenLua.source(),
            Some(LuaSourceProvider::OpenLua)
        );
        assert_eq!(
            LuaPackageProvider::SteamTools.source(),
            Some(LuaSourceProvider::SteamTools)
        );
        assert_eq!(
            LuaPackageProvider::Ryuu.source(),
            Some(LuaSourceProvider::Ryuu)
        );
        assert_eq!(LuaPackageProvider::Luie.source(), Some(LuaSourceProvider::Luie));
        assert_eq!(
            LuaPackageProvider::TwentyTwoCloud.source(),
            Some(LuaSourceProvider::TwentyTwoCloud)
        );
        assert_eq!(
            LuaPackageProvider::Skyflare.source(),
            Some(LuaSourceProvider::Skyflare)
        );
        assert_eq!(LuaPackageProvider::None.source(), None);
    }

    #[test]
    fn source_cache_names_are_provider_namespaced() {
        assert_eq!(LuaSourceProvider::HuggingFace.cache_name(), "hugging-face");
        assert_eq!(LuaSourceProvider::Hubcap.cache_name(), "hubcap");
        assert_eq!(LuaSourceProvider::Sushi.cache_name(), "sushi");
        assert_eq!(LuaSourceProvider::OpenLua.cache_name(), "open-lua");
        assert_eq!(LuaSourceProvider::SteamTools.cache_name(), "steam-tools");
        assert_eq!(LuaSourceProvider::Ryuu.cache_name(), "ryuu");
        assert_eq!(LuaSourceProvider::Luie.cache_name(), "luie");
        assert_eq!(LuaSourceProvider::TwentyTwoCloud.cache_name(), "depotbox");
        assert_eq!(LuaSourceProvider::Skyflare.cache_name(), "skyflare");
    }
}
