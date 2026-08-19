use chrono::{DateTime, Duration as ChronoDuration, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, ETAG, IF_NONE_MATCH};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use zip::ZipArchive;

const REGISTRY_SCHEMA: u32 = 4;
const HOT_RELOAD_CORE_REVISION: &str = env!("CARGO_PKG_VERSION");
const RAW_LUA_BASE: &str = "https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua";
const FALLBACK_ZIP_BASE: &str =
    "https://huggingface.co/datasets/Immaking/Luas/resolve/main/manifests";
const LEGACY_FALLBACK_ZIP_BASE: &str =
    "https://huggingface.co/datasets/Immaking/Luas/resolve/main/lua-manifest%20games";
const MAX_LUA_BYTES: usize = 1024 * 1024;
const MAX_FALLBACK_ZIP_BYTES: usize = 64 * 1024 * 1024;
const SYNC_INTERVAL_MINUTES: i64 = 30;
const MAX_BACKOFF_HOURS: i64 = 6;

static REGISTRY_IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static ACTIVE_SYNCS: Lazy<Mutex<HashSet<u32>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static PACKAGE_DOWNLOAD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static LEGACY_SCAN_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaGameChannel {
    Live,
    Locked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaSyncStatus {
    Idle,
    Checking,
    UpToDate,
    Updated,
    UpdateAvailable,
    Conflict,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaMigrationState {
    Managed,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LuaRuntimeState {
    Active,
    Missing,
    Conflict,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LuaRemoteSourceState {
    Available,
    Unavailable,
    UpdateAvailable,
    Error,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGameState {
    pub appid: u32,
    pub game_name: String,
    pub channel: LuaGameChannel,
    pub pinned_build_id: Option<String>,
    pub source_revision: Option<String>,
    pub last_sync_at: Option<String>,
    pub next_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub sync_status: LuaSyncStatus,
    pub migration_state: LuaMigrationState,
    pub requires_steam_restart: bool,
    pub shared_depot_conflicts: Vec<u32>,
    #[serde(default)]
    pub runtime_state: LuaRuntimeState,
    #[serde(default)]
    pub source_state: LuaRemoteSourceState,
    #[serde(default)]
    pub source_error_code: Option<String>,
    #[serde(default)]
    pub source_provider: Option<crate::lua_sources::LuaPackageProvider>,
    #[serde(default)]
    pub selected_source: Option<crate::lua_sources::LuaSourceProvider>,
    #[serde(default)]
    pub selected_variant: Option<crate::lua_sources::LuaPackageProvider>,
    #[serde(default)]
    pub installed_revision: Option<String>,
    #[serde(default)]
    pub installed_modified_at: Option<String>,
    #[serde(default)]
    pub available_revision: Option<String>,
    #[serde(default)]
    pub available_modified_at: Option<String>,
    #[serde(default)]
    pub last_checked_at: Option<String>,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    depot_ids: Vec<u32>,
    #[serde(default)]
    managed_hash: Option<String>,
    #[serde(default)]
    source_etag: Option<String>,
    #[serde(default)]
    retry_count: u32,
}

impl LuaGameState {
    fn new(appid: u32, game_name: String, channel: LuaGameChannel) -> Self {
        Self {
            appid,
            game_name,
            channel,
            pinned_build_id: None,
            source_revision: None,
            last_sync_at: None,
            next_sync_at: None,
            last_error: None,
            sync_status: LuaSyncStatus::Idle,
            migration_state: LuaMigrationState::Managed,
            requires_steam_restart: false,
            shared_depot_conflicts: Vec::new(),
            runtime_state: LuaRuntimeState::Unknown,
            source_state: LuaRemoteSourceState::Unknown,
            source_error_code: None,
            source_provider: None,
            selected_source: None,
            selected_variant: None,
            installed_revision: None,
            installed_modified_at: None,
            available_revision: None,
            available_modified_at: None,
            last_checked_at: None,
            update_available: false,
            depot_ids: Vec::new(),
            managed_hash: None,
            source_etag: None,
            retry_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaGameManagerState {
    pub game: LuaGameState,
    pub lua_path: String,
    pub file_exists: bool,
    pub has_user_overrides: bool,
    pub can_switch_live: bool,
    pub can_switch_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallLuaGameRequest {
    pub appid: u32,
    pub game_name: String,
    pub channel: LuaGameChannel,
    pub build_id: Option<String>,
    pub access_token: Option<String>,
    pub stat_steam_id: Option<String>,
    pub conflict_resolution: Option<LuaConflictResolution>,
    #[serde(default)]
    pub provider: Option<crate::lua_sources::LuaSourceProvider>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaConflictResolution {
    KeepCurrent,
    RestoreLive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLuaGameChannelRequest {
    pub appid: u32,
    pub channel: LuaGameChannel,
    pub build_id: Option<String>,
    pub conflict_resolution: Option<LuaConflictResolution>,
    #[serde(default)]
    pub provider: Option<crate::lua_sources::LuaSourceProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSourceActionRequest {
    pub appid: u32,
    pub provider: crate::lua_sources::LuaSourceProvider,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub conflict_resolution: Option<LuaConflictResolution>,
    #[serde(default)]
    pub stat_steam_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaLegacyResolution {
    pub appid: u32,
    pub action: LuaLegacyAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaLegacyAction {
    KeepLocked,
    RestoreLive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingLuaAddSettlement {
    request_id: String,
    appid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaRegistry {
    schema_version: u32,
    hot_reload_core_ready: bool,
    #[serde(default)]
    hot_reload_core_revision: Option<String>,
    #[serde(default)]
    pending_add_settlements: BTreeMap<String, PendingLuaAddSettlement>,
    games: BTreeMap<String, LuaGameState>,
}

impl Default for LuaRegistry {
    fn default() -> Self {
        Self {
            schema_version: REGISTRY_SCHEMA,
            hot_reload_core_ready: false,
            hot_reload_core_revision: None,
            pending_add_settlements: BTreeMap::new(),
            games: BTreeMap::new(),
        }
    }
}

struct AppSyncGuard {
    appid: u32,
}

impl Drop for AppSyncGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_SYNCS.lock() {
            active.remove(&self.appid);
        }
    }
}

fn acquire_app_sync(appid: u32) -> Result<AppSyncGuard, String> {
    let mut active = ACTIVE_SYNCS
        .lock()
        .map_err(|_| "Lua sync coordinator is unavailable".to_string())?;
    if !active.insert(appid) {
        return Err(format!("AppID {appid} is already being synchronized"));
    }
    Ok(AppSyncGuard { appid })
}

fn registry_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("lua-games").join("registry.json"))
        .map_err(|error| format!("Could not resolve Lua registry path: {error}"))
}

fn load_registry_unlocked(app: &AppHandle) -> Result<LuaRegistry, String> {
    let path = registry_path(app)?;
    if !path.is_file() {
        return Ok(LuaRegistry::default());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Could not read Lua registry: {error}"))?;
    let mut registry: LuaRegistry = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Lua registry is invalid: {error}"))?;
    for state in registry.games.values_mut() {
        if state.selected_source.is_none() {
            state.selected_source = state.source_provider.and_then(|provider| provider.source());
        }
        if state.selected_variant.is_none() {
            state.selected_variant = state
                .source_provider
                .filter(|provider| *provider != crate::lua_sources::LuaPackageProvider::None);
        }
        if state.installed_revision.is_none() {
            state.installed_revision = state.source_revision.clone();
        }
        if state.last_checked_at.is_none() {
            state.last_checked_at = state.last_sync_at.clone();
        }
        if state.selected_source.is_none() {
            state.migration_state = LuaMigrationState::ReviewRequired;
            state.update_available = false;
            state.available_revision = None;
            state.available_modified_at = None;
            state.source_error_code = Some("SOURCE_REVIEW_REQUIRED".to_string());
        }
    }
    registry.schema_version = REGISTRY_SCHEMA;
    Ok(registry)
}

fn save_registry_unlocked(app: &AppHandle, registry: &LuaRegistry) -> Result<(), String> {
    let path = registry_path(app)?;
    let bytes = serde_json::to_vec_pretty(registry)
        .map_err(|error| format!("Could not serialize Lua registry: {error}"))?;
    atomic_write_path(&path, &bytes)
}

fn with_registry<T>(
    app: &AppHandle,
    operation: impl FnOnce(&mut LuaRegistry) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = REGISTRY_IO_LOCK
        .lock()
        .map_err(|_| "Lua registry is busy".to_string())?;
    let mut registry = load_registry_unlocked(app)?;
    let result = operation(&mut registry)?;
    save_registry_unlocked(app, &registry)?;
    Ok(result)
}

fn read_registry(app: &AppHandle) -> Result<LuaRegistry, String> {
    let _guard = REGISTRY_IO_LOCK
        .lock()
        .map_err(|_| "Lua registry is busy".to_string())?;
    load_registry_unlocked(app)
}

fn remember_pending_add(
    registry: &mut LuaRegistry,
    reservation: &crate::lua_sources::LuaAddReservation,
) {
    registry.pending_add_settlements.insert(
        reservation.request_id.clone(),
        PendingLuaAddSettlement {
            request_id: reservation.request_id.clone(),
            appid: reservation.appid,
        },
    );
}

fn clear_pending_add(app: &AppHandle, request_id: &str) -> Result<(), String> {
    with_registry(app, |registry| {
        registry.pending_add_settlements.remove(request_id);
        Ok(())
    })
}

fn retry_pending_add_settlements(app: &AppHandle) {
    let pending = match read_registry(app) {
        Ok(registry) => registry
            .pending_add_settlements
            .into_values()
            .collect::<Vec<_>>(),
        Err(_) => return,
    };
    for item in pending {
        let reservation = crate::lua_sources::LuaAddReservation {
            request_id: item.request_id.clone(),
            appid: item.appid,
        };
        if crate::lua_sources::complete_lua_add(app, &reservation).is_ok() {
            let _ = clear_pending_add(app, &item.request_id);
        }
    }
}

fn emit_state(app: &AppHandle, state: &LuaGameState) {
    let _ = app.emit("launcher://lua-game-state", state.clone());
}

fn update_state(
    app: &AppHandle,
    appid: u32,
    operation: impl FnOnce(&mut LuaGameState),
) -> Result<LuaGameState, String> {
    let state = with_registry(app, |registry| {
        let state = registry
            .games
            .get_mut(&appid.to_string())
            .ok_or_else(|| format!("Lua game {appid} is not registered"))?;
        operation(state);
        Ok(state.clone())
    })?;
    emit_state(app, &state);
    Ok(state)
}

pub(crate) fn atomic_write_path(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid destination path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not prepare {}: {error}", parent.display()))?;

    let nonce = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("lua-file");
    let temp_path = parent.join(format!(".{file_name}.{nonce}.tmp"));

    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|error| format!("Could not create temporary Lua file: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("Could not write temporary Lua file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush temporary Lua file: {error}"))?;
        drop(file);
        atomic_replace(&temp_path, path)
    })();

    if write_result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(format!(
            "Could not atomically replace {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "Could not atomically replace {}: {error}",
            destination.display()
        )
    })
}

fn steam_lua_dir() -> Result<PathBuf, String> {
    crate::steam::get_steam_path()
        .map(|path| path.join("config").join("stplug-in"))
        .ok_or_else(|| "Steam installation not found".to_string())
}

fn lua_path(appid: u32) -> Result<PathBuf, String> {
    Ok(steam_lua_dir()?.join(format!("{appid}.lua")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaSourceBackupMetadata {
    schema_version: u32,
    appid: u32,
    provider: crate::lua_sources::LuaSourceProvider,
    saved_at: String,
    state: LuaGameState,
}

fn source_backup_relative_dir(
    appid: u32,
    provider: crate::lua_sources::LuaSourceProvider,
) -> PathBuf {
    PathBuf::from("lua-source-backups")
        .join(appid.to_string())
        .join(provider.cache_name())
}

fn source_backup_dir(
    app: &AppHandle,
    appid: u32,
    provider: crate::lua_sources::LuaSourceProvider,
) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|root| root.join(source_backup_relative_dir(appid, provider)))
        .map_err(|error| format!("Could not resolve Lua source backup directory: {error}"))
}

fn depotcache_dir() -> Result<PathBuf, String> {
    crate::steam::get_steam_path()
        .map(|path| path.join("depotcache"))
        .ok_or_else(|| "Steam installation not found".to_string())
}

fn copy_snapshot_manifests_to_depotcache(snapshot_dir: &Path) -> Result<(), String> {
    let manifests_dir = snapshot_dir.join("manifests");
    if !manifests_dir.is_dir() {
        return Ok(());
    }
    let depotcache = depotcache_dir()?;
    fs::create_dir_all(&depotcache)
        .map_err(|error| format!("Could not prepare Steam depotcache: {error}"))?;
    for entry in fs::read_dir(&manifests_dir)
        .map_err(|error| format!("Could not inspect Lua source manifest backup: {error}"))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("manifest")
        {
            continue;
        }
        let Some(name) = path.file_name() else { continue };
        let bytes = fs::read(&path)
            .map_err(|error| format!("Could not read Lua source manifest backup: {error}"))?;
        atomic_write_path(&depotcache.join(name), &bytes)?;
    }
    Ok(())
}

fn manifest_refs_from_lua(source: &str) -> Result<Vec<String>, String> {
    let call_re = Regex::new(
        r#"(?is)^\s*setmanifestid\s*\(\s*(\d+)\s*,\s*["'](\d+)["']"#,
    )
    .map_err(|_| "Could not prepare manifest backup parser".to_string())?;
    let mut refs = BTreeSet::new();
    for (start, end) in global_call_ranges(source, &["setmanifestid"])? {
        let call = &source[start..end];
        if let Some(captures) = call_re.captures(call) {
            let depot = captures.get(1).map(|value| value.as_str()).unwrap_or_default();
            let gid = captures.get(2).map(|value| value.as_str()).unwrap_or_default();
            if !depot.is_empty() && !gid.is_empty() {
                refs.insert(format!("{depot}_{gid}.manifest"));
            }
        }
    }
    Ok(refs.into_iter().collect())
}

fn snapshot_installed_source(
    app: &AppHandle,
    state: &LuaGameState,
    raw_source: Option<&str>,
    package: Option<&crate::lua_sources::CanonicalPackage>,
) -> Result<(), String> {
    let Some(provider) = state.selected_source else {
        return Ok(());
    };
    let active_path = lua_path(state.appid)?;
    let active_source = match fs::read_to_string(&active_path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not read Lua before source backup: {error}")),
    };
    let dir = source_backup_dir(app, state.appid, provider)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not prepare Lua source backup: {error}"))?;
    atomic_write_path(&dir.join("live.lua"), active_source.as_bytes())?;
    if let Some(raw) = raw_source {
        atomic_write_path(&dir.join("raw.lua"), raw.as_bytes())?;
    }

    let manifests_dir = dir.join("manifests");
    if let Some(package) = package {
        if manifests_dir.exists() {
            fs::remove_dir_all(&manifests_dir)
                .map_err(|error| format!("Could not refresh Lua manifest backup: {error}"))?;
        }
        fs::create_dir_all(&manifests_dir)
            .map_err(|error| format!("Could not prepare Lua manifest backup: {error}"))?;
        for manifest in &package.manifests {
            atomic_write_path(&manifests_dir.join(&manifest.file_name), &manifest.bytes)?;
        }
        if !package.archive_bytes.is_empty() {
            atomic_write_path(&dir.join("provider-package.zip"), &package.archive_bytes)?;
        }
    } else if let Ok(refs) = manifest_refs_from_lua(&active_source) {
        let depotcache = depotcache_dir()?;
        for name in refs {
            let source = depotcache.join(&name);
            if !source.is_file() {
                continue;
            }
            fs::create_dir_all(&manifests_dir)
                .map_err(|error| format!("Could not prepare Lua manifest backup: {error}"))?;
            let bytes = fs::read(&source)
                .map_err(|error| format!("Could not read Steam manifest for backup: {error}"))?;
            atomic_write_path(&manifests_dir.join(name), &bytes)?;
        }
    }

    let metadata = LuaSourceBackupMetadata {
        schema_version: 1,
        appid: state.appid,
        provider,
        saved_at: now_string(),
        state: state.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("Could not serialize Lua source backup: {error}"))?;
    atomic_write_path(&dir.join("state.json"), &bytes)
}

fn restore_live_source_snapshot(
    app: &AppHandle,
    appid: u32,
    provider: crate::lua_sources::LuaSourceProvider,
    game_name: &str,
) -> Result<Option<LuaGameState>, String> {
    let dir = source_backup_dir(app, appid, provider)?;
    let lua_file = dir.join("live.lua");
    let state_file = dir.join("state.json");
    if !lua_file.is_file() || !state_file.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&lua_file)
        .map_err(|error| format!("Could not read Lua source backup: {error}"))?;
    validate_live_source(appid, &source)?;
    if has_manifest_pin(&source) {
        return Err("Live source backup unexpectedly contains manifest pins".to_string());
    }
    let metadata: LuaSourceBackupMetadata = serde_json::from_slice(
        &fs::read(&state_file)
            .map_err(|error| format!("Could not read Lua source backup metadata: {error}"))?,
    )
    .map_err(|error| format!("Lua source backup metadata is invalid: {error}"))?;
    if metadata.appid != appid || metadata.provider != provider {
        return Err("Lua source backup identity does not match the requested provider".to_string());
    }
    copy_snapshot_manifests_to_depotcache(&dir)?;
    let destination = lua_path(appid)?;
    atomic_write_path(&destination, source.as_bytes())?;
    update_sync_state(destination.parent().unwrap_or(Path::new(".")))?;

    let mut state = metadata.state;
    state.appid = appid;
    state.game_name = game_name.trim().to_string();
    state.channel = LuaGameChannel::Live;
    state.pinned_build_id = None;
    state.selected_source = Some(provider);
    state.runtime_state = LuaRuntimeState::Active;
    state.sync_status = LuaSyncStatus::Updated;
    state.last_sync_at = Some(now_string());
    state.next_sync_at = Some(next_sync_string(SYNC_INTERVAL_MINUTES));
    state.last_checked_at = state.last_sync_at.clone();
    state.update_available = false;
    state.available_revision = None;
    state.available_modified_at = None;
    state.managed_hash = Some(managed_hash(&source));
    state.requires_steam_restart = crate::steam_integration::is_steam_running()
        && !current_hot_reload_ready(app)?;
    Ok(Some(state))
}

fn restore_locked_payload_from_snapshot(
    app: &AppHandle,
    state: &LuaGameState,
) -> Result<Option<String>, String> {
    let Some(provider) = state.selected_source else {
        return Ok(None);
    };
    let dir = source_backup_dir(app, state.appid, provider)?;
    let raw_file = dir.join("raw.lua");
    if !raw_file.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&raw_file)
        .map_err(|error| format!("Could not read provider Lua backup: {error}"))?;
    validate_live_source(state.appid, &raw)?;
    if !has_manifest_pin(&raw) {
        return Ok(None);
    }
    let manifests_dir = dir.join("manifests");
    for name in manifest_refs_from_lua(&raw)? {
        if !manifests_dir.join(&name).is_file() {
            return Err(format!(
                "Provider backup is incomplete: missing paired manifest {name}"
            ));
        }
    }
    copy_snapshot_manifests_to_depotcache(&dir)?;
    let destination = lua_path(state.appid)?;
    atomic_write_path(&destination, raw.as_bytes())?;
    update_sync_state(destination.parent().unwrap_or(Path::new(".")))?;
    Ok(Some(raw))
}

fn install_package_manifests(
    package: &crate::lua_sources::CanonicalPackage,
) -> Result<(), String> {
    if package.manifests.is_empty() {
        return Ok(());
    }
    let dir = depotcache_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not prepare Steam depotcache: {error}"))?;
    for manifest in &package.manifests {
        atomic_write_path(&dir.join(&manifest.file_name), &manifest.bytes)?;
    }
    Ok(())
}

fn update_sync_state(stplug_in_dir: &Path) -> Result<(), String> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(stplug_in_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".lua") {
                names.push(name);
            }
        }
    }
    names.sort();
    let content = if names.is_empty() {
        String::new()
    } else {
        names.join("\n") + "\n"
    };
    atomic_write_path(&stplug_in_dir.join(".sync_state"), content.as_bytes())
}

fn source_client() -> Result<Client, String> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if source_url_allowed(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    });
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(policy)
        .build()
        .map_err(|error| format!("Could not initialize Lua source client: {error}"))
}

fn source_url_allowed(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    matches!(
        url.host_str()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "huggingface.co" | "cdn-lfs.hf.co" | "cdn-lfs-us-1.hf.co" | "cas-bridge.xethub.hf.co"
    )
}

fn authorized_get(client: &Client, url: &str) -> Result<reqwest::blocking::RequestBuilder, String> {
    let parsed = Url::parse(url).map_err(|error| format!("Invalid Lua source URL: {error}"))?;
    if !source_url_allowed(&parsed) {
        return Err("Lua source host is not allowed".to_string());
    }
    let mut request = client.get(parsed).header(ACCEPT_ENCODING, "identity");
    let token = crate::shop_lua::get_hf_token();
    if !token.is_empty() {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    Ok(request)
}

enum SourceFetch {
    NotModified,
    NotFound,
    Content {
        text: String,
        etag: Option<String>,
        revision: String,
        provider: crate::lua_sources::LuaPackageProvider,
        package: Option<crate::lua_sources::CanonicalPackage>,
    },
}

fn read_limited_response(mut response: Response, limit: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .map(|length| length > limit as u64)
        .unwrap_or(false)
    {
        return Err(format!("Remote Lua source exceeds the {limit} byte limit"));
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read Lua source: {error}"))?;
    if bytes.len() > limit {
        return Err(format!("Remote Lua source exceeds the {limit} byte limit"));
    }
    Ok(bytes)
}

fn fetch_raw_lua(appid: u32, etag: Option<&str>) -> Result<SourceFetch, String> {
    let client = source_client()?;
    let url = format!("{RAW_LUA_BASE}/{appid}.lua");
    let mut request = authorized_get(&client, &url)?;
    if let Some(etag) = etag.filter(|value| !value.trim().is_empty()) {
        request = request.header(IF_NONE_MATCH, etag);
    }
    let response = request
        .send()
        .map_err(|error| format!("Could not reach Lua source: {error}"))?;
    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(SourceFetch::NotModified);
    }
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(SourceFetch::NotFound);
    }
    if !response.status().is_success() {
        return Err(format!("Lua source returned HTTP {}", response.status()));
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = read_limited_response(response, MAX_LUA_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "Lua source is not valid UTF-8".to_string())?;
    let revision = etag.clone().unwrap_or_else(|| sha256_text(&text));
    Ok(SourceFetch::Content {
        text,
        etag,
        revision,
        provider: crate::lua_sources::LuaPackageProvider::Curated,
        package: None,
    })
}

fn fetch_lua_from_zip(appid: u32) -> Result<Option<String>, String> {
    let client = source_client()?;
    let urls = [
        format!("{FALLBACK_ZIP_BASE}/{appid}.zip"),
        format!("{LEGACY_FALLBACK_ZIP_BASE}/{appid}.zip"),
    ];
    let mut last_error = None;
    for url in urls {
        let response = match authorized_get(&client, &url)?.send() {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(format!("Could not reach fallback Lua package: {error}"));
                continue;
            }
        };
        if response.status() == StatusCode::NOT_FOUND {
            continue;
        }
        if !response.status().is_success() {
            last_error = Some(format!(
                "Fallback Lua package returned HTTP {}",
                response.status()
            ));
            continue;
        }
        let bytes = read_limited_response(response, MAX_FALLBACK_ZIP_BYTES)?;
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("Fallback Lua package is invalid: {error}"))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("Could not inspect fallback Lua package: {error}"))?;
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            let is_target = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case(&format!("{appid}.lua")))
                .unwrap_or(false);
            if !entry.is_file() || !is_target {
                continue;
            }
            if entry.size() > MAX_LUA_BYTES as u64 {
                return Err("Fallback Lua file is too large".to_string());
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .by_ref()
                .take((MAX_LUA_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("Could not read fallback Lua file: {error}"))?;
            if bytes.len() > MAX_LUA_BYTES {
                return Err("Fallback Lua file is too large".to_string());
            }
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "Fallback Lua file is not valid UTF-8".to_string());
        }
        last_error = Some(format!("Fallback package does not contain {appid}.lua"));
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

fn fetch_live_source(
    app: &AppHandle,
    appid: u32,
    etag: Option<&str>,
    allow_zip_fallback: bool,
) -> Result<SourceFetch, String> {
    if allow_zip_fallback && etag.is_none() {
        match crate::lua_sources::fetch_community_package(app, appid) {
            Ok(Some(package)) => {
                return Ok(SourceFetch::Content {
                    text: package.canonical_lua.clone(),
                    etag: None,
                    revision: package.revision.clone(),
                    provider: crate::lua_sources::LuaPackageProvider::Community,
                    package: Some(package),
                });
            }
            Ok(None) => {}
            Err(error) => crate::debug_log::debug_log(&format!(
                "Community Lua package rejected for AppID {appid}: {error}"
            )),
        }
    }
    match fetch_raw_lua(appid, etag)? {
        SourceFetch::NotFound if allow_zip_fallback => match fetch_lua_from_zip(appid)? {
            Some(text) => {
                let revision = sha256_text(&text);
                Ok(SourceFetch::Content {
                    text,
                    etag: None,
                    revision,
                    provider: crate::lua_sources::LuaPackageProvider::Curated,
                    package: None,
                })
            }
            None => Ok(SourceFetch::NotFound),
        },
        other => Ok(other),
    }
}

fn fetch_selected_live_source(
    app: &AppHandle,
    appid: u32,
    provider: crate::lua_sources::LuaSourceProvider,
    etag: Option<&str>,
) -> Result<SourceFetch, String> {
    let package_fetch = |package: crate::lua_sources::CanonicalPackage| SourceFetch::Content {
        text: package.canonical_lua.clone(),
        etag: None,
        revision: package.revision.clone(),
        provider: package.provider,
        package: Some(package),
    };
    match provider {
        crate::lua_sources::LuaSourceProvider::HuggingFace => {
            fetch_live_source(app, appid, etag, true)
        }
        crate::lua_sources::LuaSourceProvider::Hubcap => {
            crate::lua_sources::live_lua_from_hubcap(app, appid).map(package_fetch)
        }
        crate::lua_sources::LuaSourceProvider::Sushi => {
            crate::lua_sources::fetch_sushi_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::GitHubMirrors => {
            crate::lua_sources::fetch_github_mirrors_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::OpenLua => {
            crate::lua_sources::fetch_openlua_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::SteamTools => {
            crate::lua_sources::fetch_steamtools_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::Ryuu => {
            crate::lua_sources::fetch_ryuu_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::TwentyTwoCloud => {
            crate::lua_sources::fetch_depotbox_package(app, appid, false)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::Luie => {
            crate::lua_sources::fetch_luatools_direct_package(app, appid, provider)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
        crate::lua_sources::LuaSourceProvider::Skyflare => {
            crate::lua_sources::fetch_skyflare_package(app, appid)?
                .map(package_fetch)
                .ok_or_else(|| "SOURCE_NOT_AVAILABLE".to_string())
        }
    }
}

fn sha256_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

fn managed_hash(text: &str) -> String {
    sha256_text(&normalize_for_compare(text))
}

fn normalize_for_compare(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

fn long_bracket_open(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'=') {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'[') {
        Some((cursor - index - 1, cursor + 1))
    } else {
        None
    }
}

fn long_bracket_end(bytes: &[u8], mut cursor: usize, level: usize) -> usize {
    while cursor < bytes.len() {
        if bytes[cursor] == b']' {
            let mut probe = cursor + 1;
            let mut equals = 0;
            while bytes.get(probe) == Some(&b'=') {
                equals += 1;
                probe += 1;
            }
            if equals == level && bytes.get(probe) == Some(&b']') {
                return probe + 1;
            }
        }
        cursor += 1;
    }
    bytes.len()
}

fn mask_lua_non_code(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            let start = index;
            if let Some((level, content_start)) = long_bracket_open(bytes, index + 2) {
                index = long_bracket_end(bytes, content_start, level);
            } else {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' {
                    masked[offset] = b' ';
                }
            }
            continue;
        }
        if bytes[index] == b'\'' || bytes[index] == b'"' {
            let quote = bytes[index];
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' {
                    masked[offset] = b' ';
                }
            }
            continue;
        }
        if let Some((level, content_start)) = long_bracket_open(bytes, index) {
            let start = index;
            index = long_bracket_end(bytes, content_start, level);
            for offset in start..index {
                if masked[offset] != b'\n' && masked[offset] != b'\r' {
                    masked[offset] = b' ';
                }
            }
            continue;
        }
        index += 1;
    }
    String::from_utf8(masked).unwrap_or_else(|_| " ".repeat(bytes.len()))
}

fn global_call_ranges(
    source: &str,
    function_names: &[&str],
) -> Result<Vec<(usize, usize)>, String> {
    let mask = mask_lua_non_code(source);
    let names = function_names
        .iter()
        .map(|name| regex::escape(name))
        .collect::<Vec<_>>()
        .join("|");
    let regex = Regex::new(&format!(r"(?im)^[\t ]*(?:{names})[\t ]*\("))
        .map_err(|_| "Could not prepare Lua parser".to_string())?;
    let bytes = mask.as_bytes();
    let mut ranges = Vec::new();
    for found in regex.find_iter(&mask) {
        let Some(relative_open) = bytes[found.start()..found.end()]
            .iter()
            .position(|byte| *byte == b'(')
        else {
            continue;
        };
        let mut cursor = found.start() + relative_open;
        let mut depth = 0usize;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        cursor += 1;
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            return Err("Lua source contains an unterminated managed call".to_string());
        }
        while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t' | b';') {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b'\r') {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b'\n') {
            cursor += 1;
        }
        ranges.push((found.start(), cursor));
    }
    Ok(ranges)
}

fn remove_global_calls(source: &str, function_names: &[&str]) -> Result<String, String> {
    let ranges = global_call_ranges(source, function_names)?;
    if ranges.is_empty() {
        return Ok(source.to_string());
    }
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end) in ranges {
        if start >= cursor {
            output.push_str(&source[cursor..start]);
            cursor = end;
        }
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

fn extract_call_text(source: &str, function_name: &str, appid: u32) -> Option<String> {
    let mask = mask_lua_non_code(source);
    let app_pattern = Regex::new(&format!(
        r"(?i)^[\t ]*{}[\t ]*\([\t ]*{}(?:[\t ]*[,\)])",
        regex::escape(function_name),
        appid
    ))
    .ok()?;
    for (start, end) in global_call_ranges(source, &[function_name]).ok()? {
        if app_pattern.is_match(&mask[start..end]) {
            return Some(source[start..end].trim().to_string());
        }
    }
    None
}

fn extract_appids(source: &str, function_name: &str) -> Vec<u32> {
    let mask = mask_lua_non_code(source);
    let Ok(regex) = Regex::new(&format!(
        r"(?i)\b{}[\t ]*\([\t ]*(\d+)",
        regex::escape(function_name)
    )) else {
        return Vec::new();
    };
    let mut ids = BTreeSet::new();
    for captures in regex.captures_iter(&mask) {
        if let Some(id) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        {
            ids.insert(id);
        }
    }
    ids.into_iter().collect()
}

fn validate_live_source(appid: u32, source: &str) -> Result<(), String> {
    if source.is_empty() || source.len() > MAX_LUA_BYTES {
        return Err("Lua source is empty or too large".to_string());
    }
    if source.as_bytes().contains(&0) {
        return Err("Lua source contains NUL bytes".to_string());
    }
    let appids = extract_appids(source, "addappid");
    if !appids.contains(&appid) {
        return Err(format!(
            "Lua source does not register its root AppID {appid}"
        ));
    }
    Ok(())
}

fn lua_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn prepare_user_override(
    existing: Option<&str>,
    appid: u32,
    access_token: Option<&str>,
    stat_steam_id: Option<&str>,
    preserve_legacy_calls: bool,
) -> Result<String, String> {
    let mut user = String::new();
    if preserve_legacy_calls {
        if let Some(existing) = existing {
            let calls = [
                extract_call_text(existing, "addtoken", appid),
                extract_call_text(existing, "setstat", appid),
            ];
            user = calls.into_iter().flatten().collect::<Vec<_>>().join("\n");
        }
    }

    user = remove_global_calls(&user, &["setmanifestid", "skipmanifestpin"])?;
    if access_token.is_some() {
        user = remove_global_calls(&user, &["addtoken"])?;
    }
    if stat_steam_id.is_some() {
        user = remove_global_calls(&user, &["setstat"])?;
    }
    if let Some(token) = access_token.filter(|value| !value.trim().is_empty()) {
        if !user.is_empty() && !user.ends_with('\n') {
            user.push('\n');
        }
        user.push_str(&format!("addtoken({appid}, {})\n", lua_quote(token.trim())));
    }
    if let Some(steam_id) = stat_steam_id.filter(|value| !value.trim().is_empty()) {
        if !user.is_empty() && !user.ends_with('\n') {
            user.push('\n');
        }
        user.push_str(&format!(
            "setStat({appid}, {})\n",
            lua_quote(steam_id.trim())
        ));
    }
    Ok(user.trim().to_string())
}

fn has_manifest_pin(source: &str) -> bool {
    global_call_ranges(source, &["setmanifestid"])
        .map(|ranges| !ranges.is_empty())
        .unwrap_or(false)
}

fn upsert_explicit_app_string_call(
    source: &str,
    function_name: &str,
    appid: u32,
    value: &str,
) -> Result<String, String> {
    let mask = mask_lua_non_code(source);
    let app_re = Regex::new(&format!(
        r"(?i)^[\t ]*{}[\t ]*\([\t ]*{}[\t ]*,",
        regex::escape(function_name),
        appid
    ))
    .map_err(|_| "Could not prepare Lua override parser".to_string())?;
    let value_re = Regex::new(&format!(
        r#"(?is)^(\s*{}\s*\(\s*{}\s*,\s*)(?:"(?:\\.|[^"])*"|'(?:\\.|[^'])*')"#,
        regex::escape(function_name),
        appid
    ))
    .map_err(|_| "Could not prepare Lua override writer".to_string())?;

    for (start, end) in global_call_ranges(source, &[function_name])? {
        if !app_re.is_match(&mask[start..end]) {
            continue;
        }
        let call = &source[start..end];
        if value_re.is_match(call) {
            let literal = lua_quote(value);
            let replaced = value_re
                .replace(call, |captures: &regex::Captures<'_>| {
                    format!("{}{}", &captures[1], literal)
                })
                .to_string();
            let mut output = String::with_capacity(source.len() + literal.len());
            output.push_str(&source[..start]);
            output.push_str(&replaced);
            output.push_str(&source[end..]);
            return Ok(output);
        }
    }

    let mut output = source.to_string();
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    if !output.is_empty() && !output.ends_with('\n') {
        output.push_str(newline);
    }
    output.push_str(&format!(
        "{}({}, {}){}",
        function_name,
        appid,
        lua_quote(value),
        newline
    ));
    Ok(output)
}

fn prepare_live_provider_source(
    source: &str,
    appid: u32,
    access_token: Option<&str>,
    stat_steam_id: Option<&str>,
) -> Result<String, String> {
    // Provider content is the source of truth. Live mode removes only version
    // pinning declarations; comments, metadata, keys, tickets, tokens and any
    // other provider-authored content remain untouched.
    let mut output = remove_global_calls(source, &["setmanifestid"])?;
    if let Some(token) = access_token.filter(|value| !value.trim().is_empty()) {
        output = upsert_explicit_app_string_call(&output, "addtoken", appid, token.trim())?;
    }
    if let Some(steam_id) = stat_steam_id.filter(|value| !value.trim().is_empty()) {
        output = upsert_explicit_app_string_call(&output, "setStat", appid, steam_id.trim())?;
    }
    validate_live_source(appid, &output)?;
    Ok(output)
}

fn render_live_file(managed: &str, user: &str) -> String {
    let mut output = String::new();
    output.push_str(managed.trim());
    output.push('\n');
    if !user.trim().is_empty() {
        output.push('\n');
        output.push_str(user.trim());
        output.push('\n');
    }
    output
}

fn live_source_from_existing(existing: &str) -> Result<String, String> {
    remove_global_calls(
        existing,
        &["setmanifestid", "skipmanifestpin", "addtoken", "setstat"],
    )
}

fn ensure_existing_managed_unchanged(
    existing: Option<&str>,
    state: Option<&LuaGameState>,
    resolution: Option<LuaConflictResolution>,
) -> Result<(), String> {
    let Some(existing) = existing else {
        return Ok(());
    };
    let Some(state) = state else {
        return Ok(());
    };
    if state.channel != LuaGameChannel::Live {
        return Ok(());
    }
    let Some(expected_hash) = state.managed_hash.as_deref() else {
        return Ok(());
    };
    let actual_hash = managed_hash(existing);
    if actual_hash == expected_hash {
        return Ok(());
    }
    if resolution == Some(LuaConflictResolution::RestoreLive) {
        return Ok(());
    }
    Err("The launcher-managed Lua section was modified locally. Keep the current file or explicitly restore Live before syncing.".to_string())
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn next_sync_string(minutes: i64) -> String {
    (Utc::now() + ChronoDuration::minutes(minutes)).to_rfc3339()
}

fn backoff_minutes(retry_count: u32) -> i64 {
    let exponent = retry_count.saturating_sub(1).min(6);
    let minutes = 5_i64.saturating_mul(1_i64 << exponent);
    minutes.min(MAX_BACKOFF_HOURS * 60)
}

fn set_checking(app: &AppHandle, appid: u32) -> Result<LuaGameState, String> {
    update_state(app, appid, |state| {
        state.sync_status = LuaSyncStatus::Checking;
        state.last_error = None;
    })
}

fn current_hot_reload_ready(app: &AppHandle) -> Result<bool, String> {
    let registry = read_registry(app)?;
    Ok(registry.hot_reload_core_ready
        && registry.hot_reload_core_revision.as_deref() == Some(HOT_RELOAD_CORE_REVISION))
}

fn source_candidate_ready(
    candidate: &crate::lua_sources::LuaSourceCandidate,
) -> Result<(), String> {
    if !candidate.enabled {
        return Err(candidate
            .error_code
            .clone()
            .unwrap_or_else(|| "SOURCE_DISABLED".to_string()));
    }
    if candidate.provider == crate::lua_sources::LuaSourceProvider::Hubcap && !candidate.key_ready {
        return Err(candidate
            .error_code
            .clone()
            .unwrap_or_else(|| "HUBCAP_KEY_REQUIRED".to_string()));
    }
    if !candidate.available && !candidate.on_demand {
        return Err(candidate
            .error_code
            .clone()
            .unwrap_or_else(|| "SOURCE_NOT_AVAILABLE".to_string()));
    }
    Ok(())
}

fn source_candidate_identity(candidate: &crate::lua_sources::LuaSourceCandidate) -> Option<&str> {
    candidate
        .revision
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            candidate
                .modified_at
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
}


struct LuaFileRollbackGuard {
    path: PathBuf,
    prior: Option<Vec<u8>>,
    active: bool,
}

impl LuaFileRollbackGuard {
    fn capture(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            prior: fs::read(path).ok(),
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for LuaFileRollbackGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        match self.prior.as_ref() {
            Some(bytes) => {
                if let Some(parent) = self.path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&self.path, bytes);
            }
            None => {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn install_live_blocking(
    app: &AppHandle,
    request: &InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    let selected_source = request
        .provider
        .ok_or_else(|| "SOURCE_SELECTION_REQUIRED".to_string())?;
    let _sync = acquire_app_sync(request.appid)?;
    let _package_download = PACKAGE_DOWNLOAD_LOCK
        .lock()
        .map_err(|_| "Lua package download queue is unavailable".to_string())?;
    let mut contribution = None;
    let result = (|| {
        let prior_state = read_registry(app)?
            .games
            .get(&request.appid.to_string())
            .cloned();
        let source_changed = prior_state.as_ref().is_some_and(|state| {
            state.channel == LuaGameChannel::Live
                && state.selected_source.is_some()
                && state.selected_source != Some(selected_source)
        });
        if source_changed {
            if let Some(previous) = prior_state.as_ref() {
                snapshot_installed_source(app, previous, None, None)?;
            }
            if let Some(restored) = restore_live_source_snapshot(
                app,
                request.appid,
                selected_source,
                &request.game_name,
            )? {
                let saved = with_registry(app, |registry| {
                    registry
                        .games
                        .insert(request.appid.to_string(), restored.clone());
                    Ok(restored.clone())
                })?;
                emit_state(app, &saved);
                return Ok(saved);
            }
        } else if prior_state.as_ref().is_some_and(|state| {
            state.channel == LuaGameChannel::Locked
                && state.selected_source == Some(selected_source)
        }) {
            if let Some(restored) = restore_live_source_snapshot(
                app,
                request.appid,
                selected_source,
                &request.game_name,
            )? {
                let saved = with_registry(app, |registry| {
                    registry
                        .games
                        .insert(request.appid.to_string(), restored.clone());
                    Ok(restored.clone())
                })?;
                emit_state(app, &saved);
                return Ok(saved);
            }
        }

        let source_before = crate::lua_sources::probe_source(app, request.appid, selected_source);
        source_candidate_ready(&source_before)?;
        let path = lua_path(request.appid)?;
        let existing = fs::read_to_string(&path).ok();
        let SourceFetch::Content {
            text: source,
            etag,
            revision,
            provider,
            package,
        } = fetch_selected_live_source(app, request.appid, selected_source, None)?
        else {
            return Err("SOURCE_NOT_AVAILABLE".to_string());
        };
        if !selected_source.accepts(provider) {
            return Err("SOURCE_PROVIDER_MISMATCH".to_string());
        }
        let source_after = if selected_source == crate::lua_sources::LuaSourceProvider::Hubcap {
            let candidate = crate::lua_sources::probe_source(app, request.appid, selected_source);
            source_candidate_ready(&candidate)?;
            if source_before.available && candidate.available {
                if let (Some(before), Some(after)) = (
                    source_candidate_identity(&source_before),
                    source_candidate_identity(&candidate),
                ) {
                    if before != after {
                        return Err("SOURCE_CHANGED_DURING_DOWNLOAD".to_string());
                    }
                }
            }
            candidate
        } else {
            source_before.clone()
        };
        let package_for_snapshot = package.clone();
        contribution = package
            .clone()
            .filter(|_| provider == crate::lua_sources::LuaPackageProvider::Hubcap);
        validate_live_source(request.appid, &source)?;

        let same_live_source = prior_state.as_ref().is_some_and(|state| {
            state.channel == LuaGameChannel::Live
                && state.selected_source == Some(selected_source)
        });
        if same_live_source {
            ensure_existing_managed_unchanged(
                existing.as_deref(),
                prior_state.as_ref(),
                request.conflict_resolution,
            )?;
        }

        let rendered = prepare_live_provider_source(
            &source,
            request.appid,
            request.access_token.as_deref(),
            request.stat_steam_id.as_deref(),
        )?;

        if prior_state.is_none()
            && existing.is_some()
            && normalize_for_compare(existing.as_deref().unwrap_or_default())
                != normalize_for_compare(&rendered)
            && request.conflict_resolution != Some(LuaConflictResolution::RestoreLive)
        {
            return Err(
                "An unmanaged local Lua file already exists. Review it before replacing it with Live."
                    .to_string(),
            );
        }

        if let Some(package) = package_for_snapshot.as_ref() {
            install_package_manifests(package)?;
        }
        // Always stage the provider's Live representation first. For adaptive Locked
        // sources this becomes the exact live.lua backup before the raw pinned Lua is
        // restored as the active file below.
        atomic_write_path(&path, rendered.as_bytes())?;
        update_sync_state(path.parent().unwrap_or(Path::new(".")))?;

        let hot_reload_ready = current_hot_reload_ready(app)?;
        let steam_running = crate::steam_integration::is_steam_running();
        let mut state = LuaGameState::new(
            request.appid,
            request.game_name.trim().to_string(),
            LuaGameChannel::Live,
        );
        state.source_revision = Some(revision);
        state.source_provider = Some(provider);
        state.selected_source = Some(selected_source);
        state.selected_variant = Some(provider);
        state.installed_revision = source_after
            .revision
            .clone()
            .or_else(|| state.source_revision.clone());
        state.installed_modified_at = source_after.modified_at.clone();
        state.available_revision = None;
        state.available_modified_at = None;
        state.last_checked_at = Some(now_string());
        state.update_available = false;
        state.runtime_state = LuaRuntimeState::Active;
        state.source_state = LuaRemoteSourceState::Available;
        state.source_error_code = None;
        state.source_etag = etag;
        state.last_sync_at = Some(now_string());
        state.next_sync_at = Some(next_sync_string(SYNC_INTERVAL_MINUTES));
        state.sync_status = LuaSyncStatus::Updated;
        state.managed_hash = Some(managed_hash(&rendered));
        state.depot_ids = extract_appids(&source, "setmanifestid");
        state.requires_steam_restart = steam_running && !hot_reload_ready;

        // Snapshot while the staged Live representation is still active so the
        // provider can later return Locked -> Live in one Apply Channel action.
        snapshot_installed_source(
            app,
            &state,
            Some(&source),
            package_for_snapshot.as_ref(),
        )?;
        let saved = with_registry(app, |registry| {
            registry
                .games
                .insert(request.appid.to_string(), state.clone());
            Ok(state.clone())
        })?;
        emit_state(app, &saved);
        Ok(saved)
    })();

    match result {
        Ok(state) => {
            if let Some(package) = contribution.take() {
                crate::lua_sources::contribute_package_async(app.clone(), package);
            }
            Ok(state)
        }
        Err(error) => Err(error),
    }
}


fn install_locked_blocking(
    app: &AppHandle,
    request: &InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if crate::steam_integration::is_steam_running() {
        return Err("Close Steam before installing a version-locked Lua game".to_string());
    }
    let _sync = acquire_app_sync(request.appid)?;
    let prior_state = read_registry(app)?
        .games
        .get(&request.appid.to_string())
        .cloned();
    if let Some(prior) = prior_state.as_ref().filter(|state| state.channel == LuaGameChannel::Live) {
        snapshot_installed_source(app, prior, None, None)?;
    }

    let build_id = request
        .build_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "A BuildID is required for Version locked".to_string())?;
    if !build_id.chars().all(|value| value.is_ascii_digit()) {
        return Err("BuildID must contain decimal digits only".to_string());
    }
    crate::shop_lua::install_locked_game_blocking(
        request.appid,
        &request.game_name,
        build_id,
        request.access_token.as_deref(),
        request.stat_steam_id.as_deref(),
    )?;
    let text = fs::read_to_string(lua_path(request.appid)?)
        .map_err(|error| format!("Could not read installed Lua file: {error}"))?;
    let mut state = LuaGameState::new(
        request.appid,
        request.game_name.trim().to_string(),
        LuaGameChannel::Locked,
    );
    let revision = sha256_text(&text);
    state.pinned_build_id = Some(build_id.to_string());
    state.last_sync_at = Some(now_string());
    state.last_checked_at = state.last_sync_at.clone();
    state.sync_status = LuaSyncStatus::Updated;
    state.runtime_state = LuaRuntimeState::Active;
    state.source_state = LuaRemoteSourceState::Available;
    state.depot_ids = extract_appids(&text, "setmanifestid");
    state.source_revision = Some(revision.clone());
    state.installed_revision = Some(revision);
    state.source_provider = Some(crate::lua_sources::LuaPackageProvider::Curated);
    state.selected_source = Some(crate::lua_sources::LuaSourceProvider::HuggingFace);
    state.selected_variant = Some(crate::lua_sources::LuaPackageProvider::Curated);
    let saved = with_registry(app, |registry| {
        registry
            .games
            .insert(request.appid.to_string(), state.clone());
        Ok(state.clone())
    })?;
    emit_state(app, &saved);
    Ok(saved)
}

fn pin_current_blocking(
    app: &AppHandle,
    request: &InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if crate::steam_integration::is_steam_running() {
        return Err("Close Steam before switching Live to Version locked".to_string());
    }
    let _sync = acquire_app_sync(request.appid)?;

    let mut state = read_registry(app)?
        .games
        .get(&request.appid.to_string())
        .cloned()
        .ok_or_else(|| "Cannot pin: Game is not managed yet".to_string())?;
    if state.channel != LuaGameChannel::Live {
        return Ok(state);
    }

    // Preserve the exact Live payload before restoring the provider's original
    // package Lua (which still contains setManifestid declarations).
    snapshot_installed_source(app, &state, None, None)?;
    let locked_source = restore_locked_payload_from_snapshot(app, &state)?
        .ok_or_else(|| {
            "LOCKED_MANIFEST_PAYLOAD_REQUIRED: this Live source has no backed-up provider Lua + manifest pair; refusing to mark it Locked without real manifest pins"
                .to_string()
        })?;

    state.channel = LuaGameChannel::Locked;
    state.pinned_build_id = state
        .pinned_build_id
        .or_else(|| crate::steam_integration::get_steam_game_buildid(request.appid));
    state.source_revision = Some(sha256_text(&locked_source));
    state.installed_revision = state.source_revision.clone();
    state.depot_ids = extract_appids(&locked_source, "setmanifestid");
    state.managed_hash = None;
    state.last_sync_at = Some(now_string());
    state.last_checked_at = state.last_sync_at.clone();
    state.sync_status = LuaSyncStatus::Updated;
    state.runtime_state = LuaRuntimeState::Active;
    state.requires_steam_restart = true;

    let saved = with_registry(app, |registry| {
        registry.games.insert(request.appid.to_string(), state.clone());
        Ok(state.clone())
    })?;
    emit_state(app, &saved);
    Ok(saved)
}

fn install_locked_source_blocking(
    app: &AppHandle,
    request: &InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if crate::steam_integration::is_steam_running() {
        return Err("Close Steam before installing a source-backed Locked Lua game".to_string());
    }
    let selected_source = request
        .provider
        .ok_or_else(|| "SOURCE_SELECTION_REQUIRED".to_string())?;
    if !selected_source.supports_locked() {
        return Err("SELECTED_SOURCE_DOES_NOT_SUPPORT_LOCKED".to_string());
    }

    let _sync = acquire_app_sync(request.appid)?;
    let _package_download = PACKAGE_DOWNLOAD_LOCK
        .lock()
        .map_err(|_| "Lua package download queue is unavailable".to_string())?;

    let prior_state = read_registry(app)?
        .games
        .get(&request.appid.to_string())
        .cloned();
    if let Some(previous) = prior_state
        .as_ref()
        .filter(|state| state.channel == LuaGameChannel::Live)
    {
        snapshot_installed_source(app, previous, None, None)?;
    }

    let source_before = crate::lua_sources::probe_source(app, request.appid, selected_source);
    source_candidate_ready(&source_before)?;
    let locked_fetch = if selected_source == crate::lua_sources::LuaSourceProvider::TwentyTwoCloud {
        crate::lua_sources::fetch_depotbox_package(app, request.appid, true)?
            .map(|package| SourceFetch::Content {
                text: package.canonical_lua.clone(),
                etag: None,
                revision: package.revision.clone(),
                provider: package.provider,
                package: Some(package),
            })
            .unwrap_or(SourceFetch::NotFound)
    } else {
        fetch_selected_live_source(app, request.appid, selected_source, None)?
    };
    let SourceFetch::Content {
        text: source,
        etag,
        revision,
        provider,
        package,
    } = locked_fetch
    else {
        return Err("SOURCE_NOT_AVAILABLE".to_string());
    };
    if !selected_source.accepts(provider) {
        return Err("SOURCE_PROVIDER_MISMATCH".to_string());
    }
    validate_live_source(request.appid, &source)?;
    if !has_manifest_pin(&source) {
        return Err("LOCKED_SOURCE_REQUIRES_MANIFEST_PACKAGE: selected source returned Lua without manifest pins".to_string());
    }
    let package = package.ok_or_else(|| {
        "LOCKED_SOURCE_REQUIRES_MANIFEST_PACKAGE: selected source did not return an atomic Lua + manifest package".to_string()
    })?;
    if package.manifests.is_empty() {
        return Err("LOCKED_SOURCE_REQUIRES_MANIFEST_PACKAGE: selected source package contains no manifests".to_string());
    }
    install_package_manifests(&package)?;

    let path = lua_path(request.appid)?;
    let live_rendered = prepare_live_provider_source(
        &source,
        request.appid,
        request.access_token.as_deref(),
        request.stat_steam_id.as_deref(),
    )?;
    let existing = fs::read_to_string(&path).ok();
    if prior_state.is_none()
        && existing.is_some()
        && normalize_for_compare(existing.as_deref().unwrap_or_default())
            != normalize_for_compare(&live_rendered)
        && request.conflict_resolution != Some(LuaConflictResolution::RestoreLive)
    {
        return Err(
            "An unmanaged local Lua file already exists. Review it before replacing it with Locked."
                .to_string(),
        );
    }

    // Stage the exact Live representation first so Locked -> Live can restore this
    // same provider without re-downloading or borrowing another provider's payload.
    atomic_write_path(&path, live_rendered.as_bytes())?;
    update_sync_state(path.parent().unwrap_or(Path::new(".")))?;

    let mut live_snapshot_state = LuaGameState::new(
        request.appid,
        request.game_name.trim().to_string(),
        LuaGameChannel::Live,
    );
    live_snapshot_state.source_revision = Some(revision.clone());
    live_snapshot_state.source_provider = Some(provider);
    live_snapshot_state.selected_source = Some(selected_source);
    live_snapshot_state.selected_variant = Some(provider);
    live_snapshot_state.installed_revision = source_before
        .revision
        .clone()
        .or_else(|| Some(revision.clone()));
    live_snapshot_state.installed_modified_at = source_before.modified_at.clone();
    live_snapshot_state.source_etag = etag.clone();
    live_snapshot_state.managed_hash = Some(managed_hash(&live_rendered));
    live_snapshot_state.depot_ids = extract_appids(&source, "setmanifestid");
    live_snapshot_state.runtime_state = LuaRuntimeState::Active;
    live_snapshot_state.source_state = LuaRemoteSourceState::Available;
    snapshot_installed_source(
        app,
        &live_snapshot_state,
        Some(&source),
        Some(&package),
    )?;

    // Locked mode keeps the provider-authored pinned Lua verbatim.
    atomic_write_path(&path, source.as_bytes())?;
    update_sync_state(path.parent().unwrap_or(Path::new(".")))?;

    let mut state = live_snapshot_state;
    state.channel = LuaGameChannel::Locked;
    state.pinned_build_id = crate::steam_integration::get_steam_game_buildid(request.appid);
    state.source_revision = Some(revision.clone());
    state.installed_revision = Some(revision);
    state.last_sync_at = Some(now_string());
    state.last_checked_at = state.last_sync_at.clone();
    state.next_sync_at = None;
    state.sync_status = LuaSyncStatus::Updated;
    state.managed_hash = None;
    state.requires_steam_restart = true;

    let saved = with_registry(app, |registry| {
        registry
            .games
            .insert(request.appid.to_string(), state.clone());
        Ok(state.clone())
    })?;
    emit_state(app, &saved);
    Ok(saved)
}

fn install_lua_game_blocking(
    app: &AppHandle,
    request: &InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if request.appid == 0 {
        return Err("AppID must be greater than zero".to_string());
    }
    match request.channel {
        LuaGameChannel::Live => {
            if request
                .build_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err("Live does not accept a BuildID".to_string());
            }
            install_live_blocking(app, request)
        }
        LuaGameChannel::Locked => {
            if request.build_id.as_deref().is_some_and(|value| !value.trim().is_empty()) {
                install_locked_blocking(app, request)
            } else if request.provider.is_some() {
                install_locked_source_blocking(app, request)
            } else {
                pin_current_blocking(app, request)
            }
        }
    }
}

#[tauri::command]
pub async fn install_lua_game(
    app: AppHandle,
    mut request: InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if request.channel == LuaGameChannel::Live && request.provider.is_none() {
        request.provider = read_registry(&app)?
            .games
            .get(&request.appid.to_string())
            .and_then(|state| state.selected_source);
        if request.provider.is_none() {
            return Err("SOURCE_SELECTION_REQUIRED".to_string());
        }
    }
    tauri::async_runtime::spawn_blocking(move || install_lua_game_blocking(&app, &request))
        .await
        .map_err(|error| format!("Lua installation task failed: {error}"))?
}

#[tauri::command]
pub async fn install_lua_game_from_source(
    app: AppHandle,
    request: InstallLuaGameRequest,
) -> Result<LuaGameState, String> {
    if request.channel == LuaGameChannel::Live && request.provider.is_none() {
        return Err("SOURCE_SELECTION_REQUIRED".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || install_lua_game_blocking(&app, &request))
        .await
        .map_err(|error| format!("Lua installation task failed: {error}"))?
}

pub(crate) fn install_live_compat(app: AppHandle, appid: u32) -> Result<LuaGameState, String> {
    let provider = read_registry(&app)?
        .games
        .get(&appid.to_string())
        .and_then(|state| state.selected_source)
        .ok_or_else(|| "SOURCE_SELECTION_REQUIRED".to_string())?;
    install_lua_game_blocking(
        &app,
        &InstallLuaGameRequest {
            appid,
            game_name: format!("AppID {appid}"),
            channel: LuaGameChannel::Live,
            build_id: None,
            access_token: None,
            stat_steam_id: None,
            conflict_resolution: None,
            provider: Some(provider),
            request_id: None,
            timezone: None,
        },
    )
}

fn sync_live_blocking(
    app: &AppHandle,
    appid: u32,
    _force: bool,
    _conflict_resolution: Option<LuaConflictResolution>,
) -> Result<LuaGameState, String> {
    check_live_metadata_blocking(app, appid)
}

#[tauri::command]
pub async fn sync_lua_game(
    app: AppHandle,
    appid: u32,
    force: Option<bool>,
    conflict_resolution: Option<LuaConflictResolution>,
) -> Result<LuaGameState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sync_live_blocking(&app, appid, force.unwrap_or(false), conflict_resolution)
    })
    .await
    .map_err(|error| format!("Lua sync task failed: {error}"))?
}

fn apply_source_action_blocking(
    app: &AppHandle,
    request: &LuaSourceActionRequest,
) -> Result<LuaGameState, String> {
    let current = get_lua_game_state_blocking(app, request.appid)?;
    if current.channel != LuaGameChannel::Live {
        return Err("VERSION_LOCKED_SOURCE_UPDATE".to_string());
    }
    install_lua_game_blocking(
        app,
        &InstallLuaGameRequest {
            appid: request.appid,
            game_name: current.game_name,
            channel: LuaGameChannel::Live,
            build_id: None,
            access_token: None,
            stat_steam_id: request.stat_steam_id.clone(),
            conflict_resolution: request.conflict_resolution,
            provider: Some(request.provider),
            request_id: request.request_id.clone(),
            timezone: request.timezone.clone(),
        },
    )
}

#[tauri::command]
pub async fn apply_lua_game_update(
    app: AppHandle,
    request: LuaSourceActionRequest,
) -> Result<LuaGameState, String> {
    tauri::async_runtime::spawn_blocking(move || apply_source_action_blocking(&app, &request))
        .await
        .map_err(|error| format!("Lua update task failed: {error}"))?
}

#[tauri::command]
pub async fn sync_lua_game_from_source(
    app: AppHandle,
    request: LuaSourceActionRequest,
) -> Result<LuaGameState, String> {
    tauri::async_runtime::spawn_blocking(move || apply_source_action_blocking(&app, &request))
        .await
        .map_err(|error| format!("Lua source sync task failed: {error}"))?
}

#[tauri::command]
pub async fn check_lua_game_update(app: AppHandle, appid: u32) -> Result<LuaGameState, String> {
    tauri::async_runtime::spawn_blocking(move || check_live_metadata_blocking(&app, appid))
        .await
        .map_err(|error| format!("Lua update check failed: {error}"))?
}

fn sync_is_due(state: &LuaGameState) -> bool {
    let Some(next) = state.next_sync_at.as_deref() else {
        return true;
    };
    DateTime::parse_from_rfc3339(next)
        .map(|next| next.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(true)
}

fn metadata_is_newer(
    state: &LuaGameState,
    source: &crate::lua_sources::LuaSourceCandidate,
) -> Option<bool> {
    if let Some(available) = source
        .revision
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return state
            .installed_revision
            .as_deref()
            .map(|installed| available != installed);
    }
    let available = source
        .modified_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let installed = state
        .installed_modified_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    available
        .zip(installed)
        .map(|(available, installed)| available > installed)
}

fn check_live_metadata_blocking(app: &AppHandle, appid: u32) -> Result<LuaGameState, String> {
    let _sync = acquire_app_sync(appid)?;
    let state = set_checking(app, appid)?;
    if state.channel != LuaGameChannel::Live {
        return Ok(state);
    }
    let Some(selected_source) = state.selected_source else {
        return update_state(app, appid, |state| {
            state.migration_state = LuaMigrationState::ReviewRequired;
            state.sync_status = LuaSyncStatus::Error;
            state.source_state = LuaRemoteSourceState::Unknown;
            state.source_error_code = Some("SOURCE_REVIEW_REQUIRED".to_string());
            state.last_error = None;
            state.last_checked_at = Some(now_string());
            state.next_sync_at = Some(next_sync_string(MAX_BACKOFF_HOURS * 60));
        });
    };
    let availability = crate::lua_sources::probe_source(app, appid, selected_source);
    if !availability.enabled || !availability.available {
        let error_code = availability.error_code.clone().unwrap_or_else(|| {
            if availability.on_demand {
                "SOURCE_NOT_CACHED".to_string()
            } else {
                "SOURCE_NOT_AVAILABLE".to_string()
            }
        });
        return update_state(app, appid, |state| {
            state.runtime_state = if lua_path(appid).is_ok_and(|path| path.is_file()) {
                LuaRuntimeState::Active
            } else {
                LuaRuntimeState::Missing
            };
            state.source_state = if availability.error_code.is_some() {
                LuaRemoteSourceState::Error
            } else {
                LuaRemoteSourceState::Unavailable
            };
            state.source_error_code = Some(error_code.clone());
            state.sync_status = LuaSyncStatus::Error;
            state.last_error = None;
            state.last_checked_at = Some(now_string());
            state.retry_count = state.retry_count.saturating_add(1);
            state.next_sync_at = Some(next_sync_string(backoff_minutes(state.retry_count)));
        });
    }
    let Some(update_available) = metadata_is_newer(&state, &availability) else {
        return update_state(app, appid, |state| {
            state.migration_state = LuaMigrationState::ReviewRequired;
            state.sync_status = LuaSyncStatus::Error;
            state.source_state = LuaRemoteSourceState::Error;
            state.source_error_code = Some("SOURCE_IDENTITY_UNKNOWN".to_string());
            state.last_error = None;
            state.last_checked_at = Some(now_string());
            state.next_sync_at = Some(next_sync_string(MAX_BACKOFF_HOURS * 60));
        });
    };
    let jitter_seconds = (appid as i64 % 301) - 150;
    update_state(app, appid, |state| {
        state.available_revision = availability.revision.clone();
        state.available_modified_at = availability.modified_at.clone();
        state.last_checked_at = Some(now_string());
        state.update_available = update_available;
        state.runtime_state = if lua_path(appid).is_ok_and(|path| path.is_file()) {
            LuaRuntimeState::Active
        } else {
            LuaRuntimeState::Missing
        };
        state.source_state = if update_available {
            LuaRemoteSourceState::UpdateAvailable
        } else {
            LuaRemoteSourceState::Available
        };
        state.sync_status = if update_available {
            LuaSyncStatus::UpdateAvailable
        } else {
            LuaSyncStatus::UpToDate
        };
        state.last_error = None;
        state.source_error_code = None;
        state.next_sync_at = Some(
            (Utc::now()
                + ChronoDuration::minutes(SYNC_INTERVAL_MINUTES)
                + ChronoDuration::seconds(jitter_seconds))
            .to_rfc3339(),
        );
        state.retry_count = 0;
    })
}

fn sync_all_live_blocking(app: &AppHandle, force: bool) -> Result<Vec<LuaGameState>, String> {
    let states = get_lua_game_states_blocking(app)?
        .into_iter()
        .filter(|state| {
            state.channel == LuaGameChannel::Live
                && (force || state.sync_status != LuaSyncStatus::Conflict)
                && (force || sync_is_due(state))
        })
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    for chunk in states.chunks(4) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .map(|state| {
                    let app = app.clone();
                    let appid = state.appid;
                    scope.spawn(move || (appid, check_live_metadata_blocking(&app, appid)))
                })
                .collect::<Vec<_>>();
            for handle in handles {
                let Ok((appid, operation)) = handle.join() else {
                    continue;
                };
                match operation {
                    Ok(state) => results.push(state),
                    Err(error) if error.contains("already being synchronized") => {}
                    Err(_) => {
                        if let Ok(state) = get_lua_game_state_blocking(app, appid) {
                            results.push(state);
                        }
                    }
                }
            }
        });
    }
    Ok(results)
}

#[tauri::command]
pub async fn sync_all_live_lua_games(
    app: AppHandle,
    force: Option<bool>,
) -> Result<Vec<LuaGameState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sync_all_live_blocking(&app, force.unwrap_or(false))
    })
    .await
    .map_err(|error| format!("Live Lua sync task failed: {error}"))?
}

#[tauri::command]
pub async fn check_all_lua_game_updates(app: AppHandle) -> Result<Vec<LuaGameState>, String> {
    tauri::async_runtime::spawn_blocking(move || sync_all_live_blocking(&app, true))
        .await
        .map_err(|error| format!("Lua update check failed: {error}"))?
}

fn apply_shared_conflicts(states: &mut [LuaGameState]) {
    let locked_depots: BTreeSet<u32> = states
        .iter()
        .filter(|state| state.channel == LuaGameChannel::Locked)
        .flat_map(|state| state.depot_ids.iter().copied())
        .collect();
    for state in states {
        state.shared_depot_conflicts.clear();
        if state.channel == LuaGameChannel::Live {
            state.shared_depot_conflicts = state
                .depot_ids
                .iter()
                .copied()
                .filter(|depot| locked_depots.contains(depot))
                .collect();
        }
    }
}

fn get_lua_game_states_blocking(app: &AppHandle) -> Result<Vec<LuaGameState>, String> {
    let registry = read_registry(app)?;
    let core_ready = registry.hot_reload_core_ready
        && registry.hot_reload_core_revision.as_deref() == Some(HOT_RELOAD_CORE_REVISION);
    let steam_running = crate::steam_integration::is_steam_running();
    let mut states = registry.games.into_values().collect::<Vec<_>>();
    for state in &mut states {
        let file_exists = lua_path(state.appid).is_ok_and(|path| path.is_file());
        state.runtime_state = if !file_exists {
            LuaRuntimeState::Missing
        } else if state.sync_status == LuaSyncStatus::Conflict {
            LuaRuntimeState::Conflict
        } else {
            LuaRuntimeState::Active
        };
        if state.source_state == LuaRemoteSourceState::Unknown
            && state
                .source_provider
                .is_some_and(|provider| provider != crate::lua_sources::LuaPackageProvider::None)
        {
            state.source_state = LuaRemoteSourceState::Available;
        }
        if state.channel == LuaGameChannel::Live && steam_running && !core_ready {
            state.requires_steam_restart = true;
        }
    }
    states.sort_by(|left, right| left.game_name.cmp(&right.game_name));
    apply_shared_conflicts(&mut states);
    Ok(states)
}

fn get_lua_game_state_blocking(app: &AppHandle, appid: u32) -> Result<LuaGameState, String> {
    let mut state = read_registry(app)?
        .games
        .get(&appid.to_string())
        .cloned()
        .ok_or_else(|| format!("Lua game {appid} is not registered"))?;
    let mut all = get_lua_game_states_blocking(app)?;
    if let Some(view) = all.iter_mut().find(|candidate| candidate.appid == appid) {
        state.shared_depot_conflicts = std::mem::take(&mut view.shared_depot_conflicts);
    }
    Ok(state)
}

#[tauri::command]
pub async fn get_lua_game_states(app: AppHandle) -> Result<Vec<LuaGameState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = scan_legacy_games(&app)?;
        get_lua_game_states_blocking(&app)
    })
    .await
    .map_err(|error| format!("Lua state task failed: {error}"))?
}

#[tauri::command]
pub async fn get_lua_game_state(
    app: AppHandle,
    appid: u32,
) -> Result<Option<LuaGameState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if get_lua_game_state_blocking(&app, appid).is_err() {
            let _ = scan_legacy_games(&app)?;
        }
        match get_lua_game_state_blocking(&app, appid) {
            Ok(state) => Ok(Some(state)),
            Err(error) if error.contains("is not registered") => Ok(None),
            Err(error) => Err(error),
        }
    })
    .await
    .map_err(|error| format!("Lua state task failed: {error}"))?
}

#[tauri::command]
pub async fn get_lua_game_manager_state(
    app: AppHandle,
    appid: u32,
) -> Result<LuaGameManagerState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = get_lua_game_state_blocking(&app, appid)?;
        let path = lua_path(appid)?;
        let content = fs::read_to_string(&path).ok();
        let has_user_overrides = content
            .as_deref()
            .is_some_and(|content| {
                extract_call_text(content, "addtoken", appid).is_some()
                    || extract_call_text(content, "setstat", appid).is_some()
            });
        let source_allows_live = game.selected_source.map(|source| source.supports_live()).unwrap_or(true);
        let source_allows_locked = game.selected_source.map(|source| source.supports_locked()).unwrap_or(true);
        let can_switch_live = source_allows_live && (content.as_deref().is_some_and(|source| {
            live_source_from_existing(source)
                .and_then(|managed| validate_live_source(appid, &managed))
                .is_ok()
        }) || matches!(
            game.source_state,
            LuaRemoteSourceState::Available | LuaRemoteSourceState::UpdateAvailable
        ));
        Ok(LuaGameManagerState {
            game,
            lua_path: path.display().to_string(),
            file_exists: path.is_file(),
            has_user_overrides,
            can_switch_live,
            can_switch_locked: source_allows_locked,
        })
    })
    .await
    .map_err(|error| format!("Lua manager task failed: {error}"))?
}

#[tauri::command]
pub async fn resolve_lua_source(
    app: AppHandle,
    appid: u32,
) -> Result<crate::lua_sources::LuaSourceAvailability, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let registered = read_registry(&app)?.games.get(&appid.to_string()).cloned();
        let Some(source) = registered.and_then(|state| state.selected_source) else {
            return crate::lua_sources::probe_installed_source(&app, appid);
        };
        let candidate = crate::lua_sources::probe_source(&app, appid, source);
        let variant = candidate
            .variant
            .unwrap_or(crate::lua_sources::LuaPackageProvider::None);
        Ok(crate::lua_sources::LuaSourceAvailability {
            appid,
            curated_available: candidate.available
                && variant == crate::lua_sources::LuaPackageProvider::Curated,
            community_available: candidate.available
                && variant == crate::lua_sources::LuaPackageProvider::Community,
            hubcap_available: candidate.available
                && source == crate::lua_sources::LuaSourceProvider::Hubcap,
            sushi_available: candidate.available
                && source == crate::lua_sources::LuaSourceProvider::Sushi,
            ryuu_available: candidate.available
                && source == crate::lua_sources::LuaSourceProvider::Ryuu,
            preferred_provider: variant,
            revision: candidate.revision,
            source_modified_at: candidate.modified_at,
            error_code: candidate.error_code,
        })
    })
    .await
    .map_err(|error| format!("Lua source resolution task failed: {error}"))?
}

#[tauri::command]
pub async fn set_lua_game_channel(
    app: AppHandle,
    request: SetLuaGameChannelRequest,
) -> Result<LuaGameState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let prior = get_lua_game_state_blocking(&app, request.appid)?;
        let selected_source = request.provider.or(prior.selected_source)
            .ok_or_else(|| "SOURCE_SELECTION_REQUIRED".to_string())?;
        match request.channel {
            LuaGameChannel::Live if !selected_source.supports_live() => {
                return Err("SOURCE_CHANNEL_LIVE_UNSUPPORTED".to_string());
            }
            LuaGameChannel::Locked if !selected_source.supports_locked() => {
                return Err("SOURCE_CHANNEL_LOCKED_UNSUPPORTED".to_string());
            }
            _ => {}
        }
        let install = InstallLuaGameRequest {
            appid: request.appid,
            game_name: prior.game_name,
            channel: request.channel,
            build_id: request.build_id,
            access_token: None,
            stat_steam_id: None,
            conflict_resolution: request.conflict_resolution,
            provider: Some(selected_source),
            request_id: None,
            timezone: None,
        };
        install_lua_game_blocking(&app, &install)
    })
    .await
    .map_err(|error| format!("Lua channel task failed: {error}"))?
}

pub(crate) fn forget_lua_game(app: &AppHandle, appid: u32) -> Result<(), String> {
    with_registry(app, |registry| {
        registry.games.remove(&appid.to_string());
        Ok(())
    })
}

fn register_legacy_game(
    app: &AppHandle,
    appid: u32,
    content: &str,
    error: Option<String>,
) -> Result<LuaGameState, String> {
    let is_locked = has_manifest_pin(content) || error.is_some();
    let channel = if is_locked { LuaGameChannel::Locked } else { LuaGameChannel::Live };
    let mut state = LuaGameState::new(appid, format!("AppID {appid}"), channel);
    state.pinned_build_id = crate::steam_integration::get_steam_game_buildid(appid);
    state.migration_state = LuaMigrationState::ReviewRequired;
    state.sync_status = if error.is_some() {
        LuaSyncStatus::Error
    } else {
        LuaSyncStatus::Idle
    };
    state.last_error = error;
    state.runtime_state = LuaRuntimeState::Active;
    state.source_state = LuaRemoteSourceState::Unknown;
    state.depot_ids = extract_appids(content, "setmanifestid");
    state.source_revision = Some(sha256_text(content));
    with_registry(app, |registry| {
        registry.games.insert(appid.to_string(), state.clone());
        Ok(state.clone())
    })
}

fn scan_legacy_games(app: &AppHandle) -> Result<Vec<LuaGameState>, String> {
    let _scan_guard = LEGACY_SCAN_LOCK
        .lock()
        .map_err(|_| "Legacy Lua migration scan is busy".to_string())?;
    let directory = steam_lua_dir()?;
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut known: HashSet<u32> = HashSet::new();
    with_registry(app, |registry| {
        for (id, state) in registry.games.iter_mut() {
            if let Ok(appid) = id.parse::<u32>() {
                known.insert(appid);
                if state.channel == LuaGameChannel::Locked {
                    if let Ok(path) = lua_path(appid) {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if !has_manifest_pin(&content) {
                                state.channel = LuaGameChannel::Live;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    })?;
    let mut added = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Could not inspect Steam Lua directory: {error}"))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("lua") {
            continue;
        }
        let Some(appid) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        if known.contains(&appid) {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                added.push(register_legacy_game(
                    app,
                    appid,
                    "",
                    Some(format!("Could not read legacy Lua file: {error}")),
                )?);
                continue;
            }
        };
        added.push(register_legacy_game(app, appid, &content, None)?);
    }
    Ok(added)
}

#[tauri::command]
pub async fn resolve_legacy_lua_games(
    app: AppHandle,
    decisions: Vec<LuaLegacyResolution>,
) -> Result<Vec<LuaGameState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = scan_legacy_games(&app)?;
        for decision in decisions {
            match decision.action {
                LuaLegacyAction::KeepLocked => {
                    let _ = update_state(&app, decision.appid, |state| {
                        state.channel = LuaGameChannel::Locked;
                        state.migration_state = LuaMigrationState::Managed;
                        state.last_error = None;
                        state.sync_status = LuaSyncStatus::Idle;
                    })?;
                }
                LuaLegacyAction::RestoreLive => {
                    let prior = get_lua_game_state_blocking(&app, decision.appid)?;
                    let provider = prior
                        .selected_source
                        .ok_or_else(|| "SOURCE_SELECTION_REQUIRED".to_string())?;
                    install_live_blocking(
                        &app,
                        &InstallLuaGameRequest {
                            appid: decision.appid,
                            game_name: prior.game_name,
                            channel: LuaGameChannel::Live,
                            build_id: None,
                            access_token: None,
                            stat_steam_id: None,
                            conflict_resolution: Some(LuaConflictResolution::RestoreLive),
                            provider: Some(provider),
                            request_id: None,
                            timezone: None,
                        },
                    )?;
                }
            }
        }
        get_lua_game_states_blocking(&app)
    })
    .await
    .map_err(|error| format!("Lua migration task failed: {error}"))?
}

pub(crate) fn reconcile_core_readiness(app: &AppHandle) -> Result<(), String> {
    if crate::steam_integration::is_steam_running() {
        return Ok(());
    }
    let steam_root = crate::steam::get_steam_path();
    let ready = steam_root
        .as_deref()
        .map(|root| crate::open_steam_tool::hook_files_match_sources(app, root))
        .unwrap_or(false);
    if !ready {
        return Ok(());
    }
    with_registry(app, |registry| {
        registry.hot_reload_core_ready = true;
        registry.hot_reload_core_revision = Some(HOT_RELOAD_CORE_REVISION.to_string());
        for state in registry.games.values_mut() {
            state.requires_steam_restart = false;
        }
        Ok(())
    })
}

fn scheduler_sleep_duration(app: &AppHandle) -> Duration {
    let now = Utc::now();
    let seconds = read_registry(app)
        .ok()
        .and_then(|registry| {
            registry
                .games
                .values()
                .filter(|state| {
                    state.channel == LuaGameChannel::Live
                        && state.sync_status != LuaSyncStatus::Conflict
                })
                .filter_map(|state| state.next_sync_at.as_deref())
                .filter_map(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|next| (next.with_timezone(&Utc) - now).num_seconds())
                .min()
        })
        .unwrap_or(SYNC_INTERVAL_MINUTES * 60)
        .clamp(15, SYNC_INTERVAL_MINUTES * 60);
    Duration::from_secs(seconds as u64)
}

pub fn start_live_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        loop {
            let authorized = crate::discord_auth::get_status(&app).state == "authorized";
            if authorized {
                retry_pending_add_settlements(&app);
                let _ = reconcile_core_readiness(&app);
                let _ = scan_legacy_games(&app);
                let _ = sync_all_live_blocking(&app, false);
                std::thread::sleep(scheduler_sleep_duration(&app));
            } else {
                std::thread::sleep(Duration::from_secs(15));
            }
        }
    });
}

pub(crate) fn compatibility_update_state(
    app: &AppHandle,
    appid: u32,
) -> Result<(bool, String, bool), String> {
    match get_lua_game_state_blocking(app, appid) {
        Ok(state) => {
            let needs_update = state.update_available
                || matches!(
                    state.sync_status,
                    LuaSyncStatus::Error | LuaSyncStatus::Conflict
                );
            Ok((
                needs_update,
                state.last_error.unwrap_or_else(|| match state.channel {
                    LuaGameChannel::Live => "Live - Steam latest".to_string(),
                    LuaGameChannel::Locked => format!(
                        "Version locked{}",
                        state
                            .pinned_build_id
                            .map(|build| format!(" at BuildID {build}"))
                            .unwrap_or_default()
                    ),
                }),
                false,
            ))
        }
        Err(error) if error.contains("is not registered") => {
            Ok((true, "Lua game is not registered".to_string(), true))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_live_transform_preserves_header_and_only_removes_manifest_pins() {
        let source = "-- 289650's Lua and Manifest Created by Hubcap Manifest\r\n-- Assassin's Creed Unity\r\n-- Created: July 01, 2026 at 04:35:09 EDT\r\n-- Website: https://hubcapmanifest.com/\r\n-- Total Depots: 7\r\naddappid(289650, 1, \"root-key\")\r\nsetManifestid(289651, \"123456789\")\r\nskipManifestPin(289650)\r\naddtoken(289650, \"keep-me\")\r\nsetStat(289650, \"76561198000000000\")\r\n";
        let rendered = prepare_live_provider_source(source, 289650, None, None).unwrap();
        assert!(rendered.starts_with("-- 289650's Lua and Manifest Created by Hubcap Manifest\r\n"));
        assert!(rendered.contains("-- Created: July 01, 2026 at 04:35:09 EDT\r\n"));
        assert!(rendered.contains("-- Website: https://hubcapmanifest.com/\r\n"));
        assert!(rendered.contains("addtoken(289650, \"keep-me\")"));
        assert!(rendered.contains("setStat(289650, \"76561198000000000\")"));
        assert!(rendered.contains("skipManifestPin(289650)"));
        assert!(!rendered.to_ascii_lowercase().contains("setmanifestid("));
        assert!(!rendered.contains("BEGIN 0XOLEMON"));
    }

    #[test]
    fn openlua_backup_is_namespaced_by_appid_and_provider() {
        let openlua = source_backup_relative_dir(
            289650,
            crate::lua_sources::LuaSourceProvider::OpenLua,
        );
        let hubcap = source_backup_relative_dir(
            289650,
            crate::lua_sources::LuaSourceProvider::Hubcap,
        );
        assert_eq!(
            openlua,
            PathBuf::from("lua-source-backups").join("289650").join("open-lua")
        );
        assert_ne!(openlua, hubcap);
    }

    #[test]
    fn locked_detection_is_based_on_manifest_pin_not_generic_set_calls() {
        assert!(!has_manifest_pin("addappid(10)\nsetStat(10, \"7656\")\n"));
        assert!(!has_manifest_pin("addappid(10)\nsetAppTicket(10, \"ticket\")\n"));
        assert!(has_manifest_pin("addappid(10)\nsetManifestid(11, \"123\")\n"));
    }

    #[test]
    fn removes_real_manifest_calls_but_not_comments_or_strings() {
        let source = r#"-- setManifestid(10, "comment")
local text = "setManifestid(11, 'string')"
setManifestid(
  12,
  "123456"
)
addappid(12)
"#;
        let stripped = remove_global_calls(source, &["setmanifestid"]).unwrap();
        assert!(stripped.contains("-- setManifestid(10"));
        assert!(stripped.contains("\"setManifestid(11"));
        assert!(!stripped.contains("123456"));
        assert!(stripped.contains("addappid(12)"));
    }

    #[test]
    fn live_render_preserves_user_override_outside_managed_hash() {
        let managed = "addappid(10)\naddappid(11, 0, \"key\")";
        let rendered = render_live_file(managed, "setStat(10, \"7656\")");
        let (parsed_managed, parsed_user) = managed_sections(&rendered).unwrap();
        assert_eq!(parsed_managed, managed);
        assert!(parsed_user.contains("setStat"));
        assert!(!parsed_managed.contains("setStat"));
    }

    #[test]
    fn live_transform_removes_pin_calls_without_touching_lua_data() {
        let source = r#"addappid(10)
local help = "setManifestid(10, 'documentation')"
-- skipManifestPin(10)
setManifestid(10, "123456")
skipManifestPin(10)
"#;
        let managed = remove_global_calls(source, &["setmanifestid", "skipmanifestpin"]).unwrap();
        assert!(managed.contains("addappid(10)"));
        assert!(managed.contains("documentation"));
        assert!(managed.contains("-- skipManifestPin(10)"));
        assert!(!managed.contains("\"123456\""));
        assert_eq!(
            global_call_ranges(&managed, &["setmanifestid"]).unwrap(),
            vec![]
        );
        assert_eq!(
            global_call_ranges(&managed, &["skipmanifestpin"]).unwrap(),
            vec![]
        );
    }

    #[test]
    fn local_live_fallback_keeps_runtime_declarations_without_remote_source() {
        let existing = r#"addappid(322170)
addappid(322171, 0, "depot-key")
addtoken(322170, "private-token")
setManifestid(322171, "123456789")
setStat(322170, "76561198000000000")
forcedenuvo(322170)
"#;
        let managed = live_source_from_existing(existing).unwrap();
        assert!(managed.contains("addappid(322170)"));
        assert!(managed.contains("depot-key"));
        assert!(managed.contains("forcedenuvo(322170)"));
        assert!(!managed.contains("private-token"));
        assert!(!managed.to_ascii_lowercase().contains("setmanifestid"));
        assert!(!managed.to_ascii_lowercase().contains("setstat"));

        let user = prepare_user_override(Some(existing), 322170, None, None, true).unwrap();
        let rendered = render_live_file(&managed, &user);
        assert!(rendered.contains("addtoken(322170, \"private-token\")"));
        assert!(rendered.contains("setStat(322170, \"76561198000000000\")"));
        assert!(!rendered.to_ascii_lowercase().contains("setmanifestid"));
    }

    #[test]
    fn runtime_and_remote_source_states_serialize_independently() {
        let mut state = LuaGameState::new(2495100, "Hello Kitty".to_string(), LuaGameChannel::Live);
        state.runtime_state = LuaRuntimeState::Active;
        state.source_state = LuaRemoteSourceState::Unavailable;
        state.source_error_code = Some("REMOTE_SOURCE_UNAVAILABLE".to_string());
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["runtimeState"], "active");
        assert_eq!(json["sourceState"], "unavailable");
        assert_eq!(json["sourceErrorCode"], "REMOTE_SOURCE_UNAVAILABLE");
    }

    #[test]
    fn managed_hash_ignores_line_ending_only_changes() {
        assert_eq!(
            managed_hash("addappid(10)\n"),
            managed_hash("addappid(10)\r\n")
        );
    }

    #[test]
    fn user_values_are_escaped_as_lua_strings() {
        let user =
            prepare_user_override(None, 10, Some("token\"\nnext"), Some("7656"), false).unwrap();
        assert!(user.contains("addtoken(10, \"token\\\"\\nnext\")"));
        assert!(user.contains("setStat(10, \"7656\")"));
        assert!(!user.contains("\nnext\n"));
    }

    #[test]
    fn rejects_source_for_a_different_root_app() {
        assert!(validate_live_source(10, "addappid(11)").is_err());
    }

    #[test]
    fn shared_locked_depot_is_reported_to_live_game() {
        let mut states = vec![
            LuaGameState {
                depot_ids: vec![10, 11],
                ..LuaGameState::new(1, "Live".to_string(), LuaGameChannel::Live)
            },
            LuaGameState {
                depot_ids: vec![11],
                ..LuaGameState::new(2, "Locked".to_string(), LuaGameChannel::Locked)
            },
        ];
        apply_shared_conflicts(&mut states);
        assert_eq!(states[0].shared_depot_conflicts, vec![11]);
    }

    #[test]
    fn backoff_is_bounded() {
        assert_eq!(backoff_minutes(1), 5);
        assert_eq!(backoff_minutes(2), 10);
        assert!(backoff_minutes(99) <= MAX_BACKOFF_HOURS * 60);
    }

    #[test]
    fn source_candidate_ready_allows_hubcap_on_demand_only_with_key() {
        let ready = crate::lua_sources::LuaSourceCandidate {
            provider: crate::lua_sources::LuaSourceProvider::Hubcap,
            available: false,
            enabled: true,
            on_demand: true,
            requires_key: true,
            key_ready: true,
            recommended: true,
            variant: Some(crate::lua_sources::LuaPackageProvider::Hubcap),
            revision: None,
            modified_at: None,
            error_code: None,
        };
        assert!(source_candidate_ready(&ready).is_ok());

        let missing_key = crate::lua_sources::LuaSourceCandidate {
            key_ready: false,
            error_code: Some("HUBCAP_KEY_REQUIRED".to_string()),
            ..ready
        };
        assert_eq!(
            source_candidate_ready(&missing_key).unwrap_err(),
            "HUBCAP_KEY_REQUIRED"
        );
    }

    #[test]
    fn metadata_comparison_prefers_revision_over_modified_time() {
        let mut state =
            LuaGameState::new(322170, "Geometry Dash".to_string(), LuaGameChannel::Live);
        state.installed_revision = Some("same-revision".to_string());
        state.installed_modified_at = Some("2026-08-01T00:00:00Z".to_string());
        let source = crate::lua_sources::LuaSourceCandidate {
            provider: crate::lua_sources::LuaSourceProvider::Hubcap,
            available: true,
            enabled: true,
            on_demand: false,
            requires_key: true,
            key_ready: true,
            recommended: true,
            variant: Some(crate::lua_sources::LuaPackageProvider::Hubcap),
            revision: Some("same-revision".to_string()),
            modified_at: Some("2026-08-02T00:00:00Z".to_string()),
            error_code: None,
        };
        assert_eq!(metadata_is_newer(&state, &source), Some(false));
    }

    #[test]
    fn metadata_comparison_refuses_unknown_identity() {
        let state = LuaGameState::new(322170, "Geometry Dash".to_string(), LuaGameChannel::Live);
        let source = crate::lua_sources::LuaSourceCandidate {
            provider: crate::lua_sources::LuaSourceProvider::Sushi,
            available: true,
            enabled: true,
            on_demand: false,
            requires_key: false,
            key_ready: true,
            recommended: false,
            variant: Some(crate::lua_sources::LuaPackageProvider::Sushi),
            revision: None,
            modified_at: None,
            error_code: None,
        };
        assert_eq!(metadata_is_newer(&state, &source), None);
        assert_eq!(source_candidate_identity(&source), None);
    }
}
