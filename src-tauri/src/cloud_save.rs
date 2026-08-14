use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

mod google_drive;
mod save_map;
pub use save_map::MapUpdateReport;

const STATE_SCHEMA: u32 = 2;
const STATE_FILE: &str = "cloud-save-state.json";
const CLOUD_FOLDER: &str = "0xoLemon Cloud Saves";
const MAX_SNAPSHOTS: usize = 64;
const FILE_STABILITY_DELAY: Duration = Duration::from_secs(2);

static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNNING_GAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static SYNCING_GAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn state_lock() -> &'static Mutex<()> {
    STATE_LOCK.get_or_init(|| Mutex::new(()))
}

fn running_games() -> &'static Mutex<HashSet<String>> {
    RUNNING_GAMES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn syncing_games() -> &'static Mutex<HashSet<String>> {
    SYNCING_GAMES.get_or_init(|| Mutex::new(HashSet::new()))
}

struct SyncGuard {
    game_id: String,
}

impl SyncGuard {
    fn acquire(game_id: &str) -> Result<Self, String> {
        let mut games = syncing_games()
            .lock()
            .map_err(|_| "cloud save sync lock poisoned".to_string())?;
        if !games.insert(game_id.to_string()) {
            return Err("Cloud Save đang đồng bộ game này.".to_string());
        }
        Ok(Self {
            game_id: game_id.to_string(),
        })
    }
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        if let Ok(mut games) = syncing_games().lock() {
            games.remove(&self.game_id);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSaveMetadata {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub save_roots: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSaveRoot {
    #[serde(default)]
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_root_purpose")]
    pub purpose: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub legacy: bool,
    #[serde(default)]
    pub legacy_expires_at: Option<String>,
}

fn default_root_purpose() -> String {
    "save".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudFileEntry {
    path: String,
    size: u64,
    modified_at_ms: u64,
    blake3: String,
    #[serde(default)]
    sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudManifest {
    generated_at: String,
    files: Vec<CloudFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConflictSummary {
    pub id: String,
    pub created_at: String,
    pub local_file_count: usize,
    pub cloud_file_count: usize,
    pub local_bytes: u64,
    pub cloud_bytes: u64,
    #[serde(default)]
    pub recommended: String,
    #[serde(default)]
    pub local_device: String,
    #[serde(default)]
    pub cloud_device: String,
    #[serde(default)]
    pub recommendation_reason: String,
    #[serde(default)]
    pub recommendation_confidence: String,
    #[serde(default)]
    pub local_latest_write_at_ms: u64,
    #[serde(default)]
    pub cloud_latest_write_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSnapshotSummary {
    pub id: String,
    pub created_at: String,
    pub source: String,
    pub file_count: usize,
    pub bytes: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub snapshot_class: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudQuotaStatus {
    pub limit_bytes: Option<u64>,
    pub usage_bytes: u64,
    pub available_bytes: Option<u64>,
    pub checked_at: String,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudMapStatus {
    pub version: String,
    pub source: String,
    pub healthy: bool,
    pub message: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingCloudOperation {
    id: String,
    kind: String,
    created_at: String,
    next_retry_at: String,
    attempts: u32,
    snapshot_id: String,
    bytes: u64,
    reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameCloudRecord {
    enabled: bool,
    #[serde(default = "default_true_value")]
    automatic_protection: bool,
    save_roots: Vec<CloudSaveRoot>,
    include: Vec<String>,
    exclude: Vec<String>,
    baseline: Option<CloudManifest>,
    last_sync_at: Option<String>,
    last_message: String,
    conflicts: Vec<CloudConflictSummary>,
    snapshots: Vec<CloudSnapshotSummary>,
    google_drive_last_backup_at: Option<String>,
    google_drive_last_restore_count: usize,
    google_drive_message: String,
    #[serde(default)]
    pending_operations: Vec<PendingCloudOperation>,
    #[serde(default)]
    quota: Option<CloudQuotaStatus>,
    #[serde(default)]
    map_status: CloudMapStatus,
    #[serde(default)]
    remote_newer_known: bool,
    #[serde(default)]
    remote_snapshot_id: Option<String>,
    #[serde(default)]
    remote_generation: u64,
    #[serde(default)]
    remote_device_name: String,
    #[serde(default)]
    drive_change_token: Option<String>,
    #[serde(default)]
    cloud_state: String,
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    device_name: String,
    #[serde(default = "default_max_save_files")]
    max_files: u64,
    #[serde(default = "default_max_save_bytes")]
    max_total_bytes: u64,
    #[serde(default = "default_max_save_file_bytes")]
    max_file_bytes: u64,
    #[serde(default = "default_settle_time_ms")]
    settle_time_ms: u64,
    #[serde(default = "default_poll_interval_ms")]
    poll_interval_ms: u64,
    #[serde(default = "default_stability_wait_ms")]
    max_stability_wait_ms: u64,
}

fn default_max_save_files() -> u64 {
    10_000
}
fn default_max_save_bytes() -> u64 {
    5 * 1024 * 1024 * 1024
}
fn default_max_save_file_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}
fn default_settle_time_ms() -> u64 {
    2_000
}
fn default_poll_interval_ms() -> u64 {
    500
}
fn default_stability_wait_ms() -> u64 {
    30_000
}

fn default_true_value() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudStateFile {
    schema_version: u32,
    games: HashMap<String, GameCloudRecord>,
}

impl Default for CloudStateFile {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA,
            games: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSaveStatus {
    pub game_id: String,
    pub enabled: bool,
    pub automatic_protection: bool,
    pub sync_root: String,
    pub save_roots: Vec<CloudSaveRoot>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub state: String,
    pub last_sync_at: Option<String>,
    pub last_message: String,
    pub conflicts: Vec<CloudConflictSummary>,
    pub snapshots: Vec<CloudSnapshotSummary>,
    pub can_sync: bool,
    pub game_running: bool,
    pub google_drive_configured: bool,
    pub google_drive_connected: bool,
    pub google_drive_last_backup_at: Option<String>,
    pub google_drive_last_restore_count: usize,
    pub google_drive_message: String,
    pub pending_operation_count: usize,
    pub pending_upload_bytes: u64,
    pub quota: Option<CloudQuotaStatus>,
    pub map_status: CloudMapStatus,
    pub remote_newer_known: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSaveEvent {
    game_id: String,
    status: CloudSaveStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSaveErrorEvent {
    game_id: String,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncDirection {
    Auto,
    Push,
    Pull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    Push,
    Pull,
    Conflict,
    Baseline,
    Noop,
}

pub fn get_status(app: &AppHandle, game_id: &str) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    if seed_metadata_defaults(app, game_id, &mut state) {
        write_state_unlocked(app, &state)?;
    }
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn set_config(
    app: &AppHandle,
    game_id: &str,
    enabled: bool,
    save_roots: Vec<CloudSaveRoot>,
    include: Vec<String>,
    exclude: Vec<String>,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    seed_metadata_defaults(app, game_id, &mut state);
    let record = state.games.entry(game_id.to_string()).or_default();
    let normalized_roots = normalize_roots(save_roots);
    record.enabled = enabled;
    if !normalized_roots.is_empty() || record.save_roots.is_empty() {
        record.save_roots = normalized_roots;
    }
    record.include = normalize_patterns(include);
    record.exclude = normalize_patterns(exclude);
    record.last_message = if enabled {
        "Cloud save is enabled.".to_string()
    } else {
        "Cloud save is disabled.".to_string()
    };
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn sync_manual(
    app: &AppHandle,
    game_id: &str,
    direction: Option<&str>,
) -> Result<CloudSaveStatus, String> {
    let direction = match direction.unwrap_or("auto").to_ascii_lowercase().as_str() {
        "push" => SyncDirection::Push,
        "pull" => SyncDirection::Pull,
        _ => SyncDirection::Auto,
    };
    sync_game(app, game_id, direction)
}

pub fn sync_before_launch(app: &AppHandle, game_id: &str) -> Result<CloudSaveStatus, String> {
    let status = sync_game(app, game_id, SyncDirection::Auto)?;
    if !status.conflicts.is_empty() {
        return Err(format!(
            "CLOUD_SAVE_CONFLICT:{}:Hai bản Save đều đã thay đổi; launcher đã giữ an toàn cả hai.",
            status.conflicts.len()
        ));
    }
    if status.remote_newer_known
        && matches!(
            status.state.as_str(),
            "offline" | "rate_limited" | "auth_required"
        )
    {
        return Err(
            "CLOUD_SAVE_REMOTE_NEWER:Cloud có Save mới hơn nhưng hiện chưa thể tải xuống."
                .to_string(),
        );
    }
    // Offline-first policy: all other Drive failures are non-blocking because the
    // newest local save and the pending operation journal are already durable.
    Ok(status)
}

pub fn connect_google_drive(app: &AppHandle, game_id: &str) -> Result<CloudSaveStatus, String> {
    google_drive::authorize(app)?;
    update_google_message(
        app,
        game_id,
        "Google Drive connected. You can back up save files now.",
        None,
    )
}

pub fn disconnect_google_drive(app: &AppHandle, game_id: &str) -> Result<CloudSaveStatus, String> {
    google_drive::disconnect(app)?;
    update_google_message(app, game_id, "Google Drive disconnected.", None)
}

pub fn global_connect_google_drive(app: &AppHandle) -> Result<(), String> {
    google_drive::authorize(app)
}

pub fn global_disconnect_google_drive(app: &AppHandle) -> Result<(), String> {
    google_drive::disconnect(app)
}

pub fn global_is_google_drive_connected(app: &AppHandle) -> bool {
    google_drive::connected(app)
}

pub fn backup_to_google_drive(app: &AppHandle, game_id: &str) -> Result<CloudSaveStatus, String> {
    if !google_drive::connected(app) {
        google_drive::authorize(app)?;
    }
    sync_game(app, game_id, SyncDirection::Push)
}

pub fn restore_missing_from_google_drive(
    app: &AppHandle,
    game_id: &str,
) -> Result<CloudSaveStatus, String> {
    sync_game(app, game_id, SyncDirection::Pull)
}

/// Starts a conservative retry worker. It never restores files blindly: every
/// retry re-runs the three-way comparison against the recorded baseline.
pub fn start_google_drive_restore_monitor(app: AppHandle) {
    let retry_app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(30));
        loop {
            let _ = retry_pending_syncs(&retry_app, None);
            thread::sleep(Duration::from_secs(5 * 60));
        }
    });

    // Save-path metadata must update without requiring a technical user to
    // visit Settings. Failure is non-blocking because load_active_map() keeps
    // using the verified last-known-good or the built-in fallback.
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(45));
        loop {
            if save_map::remote_repository_configured() {
                if let Ok(report) = refresh_save_map(&app) {
                    let _ = app.emit("launcher://cloud-save-map", report);
                }
            }
            thread::sleep(Duration::from_secs(6 * 60 * 60));
        }
    });
}

pub fn retry_pending_syncs(
    app: &AppHandle,
    only_game_id: Option<&str>,
) -> Result<Vec<CloudSaveStatus>, String> {
    let game_ids = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let state = load_state_unlocked(app)?;
        state
            .games
            .iter()
            .filter(|(game_id, record)| {
                record.pending_operations.iter().any(pending_retry_due)
                    && only_game_id
                        .map(|value| value == game_id.as_str())
                        .unwrap_or(true)
            })
            .map(|(game_id, _)| game_id.clone())
            .collect::<Vec<_>>()
    };
    let mut statuses = Vec::new();
    for game_id in game_ids {
        if ensure_not_running(&game_id).is_err() {
            continue;
        }
        match sync_game(app, &game_id, SyncDirection::Auto) {
            Ok(status) => statuses.push(status),
            Err(error) => {
                let _ = app.emit(
                    "launcher://cloud-save-error",
                    CloudSaveErrorEvent {
                        game_id,
                        message: error,
                    },
                );
            }
        }
    }
    Ok(statuses)
}

pub fn pin_snapshot(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
    pinned: bool,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    seed_metadata_defaults(app, game_id, &mut state);
    let record = state.games.entry(game_id.to_string()).or_default();
    let snapshot = record
        .snapshots
        .iter_mut()
        .find(|snapshot| snapshot.id == snapshot_id)
        .ok_or_else(|| "Không tìm thấy bản sao lưu.".to_string())?;
    snapshot.pinned = pinned;
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn export_snapshot(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: Option<&str>,
    target: &Path,
) -> Result<CloudSaveStatus, String> {
    ensure_not_running(game_id)?;
    if let Some(snapshot_id) = snapshot_id {
        let root = configured_sync_root(app)?;
        let snapshot_root = game_cloud_root(&root, game_id)
            .join("snapshots")
            .join(snapshot_id);
        if !snapshot_root.is_dir() {
            return Err("Không tìm thấy bản sao lưu cục bộ.".to_string());
        }
        if target.exists() {
            clear_tree_safely(target)?;
        }
        copy_tree_safely(&snapshot_root, target)?;
    } else {
        google_drive::export_remote_snapshot(app, game_id, target)
            .map_err(|error| error.message)?;
    }
    get_status(app, game_id)
}

pub fn refresh_save_map(app: &AppHandle) -> Result<MapUpdateReport, String> {
    let (device_id, _) = device_identity(app)?;
    let report = save_map::refresh_remote_map(app, &device_id)?;
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    let game_ids = state.games.keys().cloned().collect::<Vec<_>>();
    for game_id in game_ids {
        seed_metadata_defaults(app, &game_id, &mut state);
    }
    write_state_unlocked(app, &state)?;
    Ok(report)
}

/// Nhận CloudSaveMap JSON từ Firestore realtime listener (JS) và lưu vào LKG cache.
pub fn push_save_map_from_firestore(app: &AppHandle, payload: &str) -> Result<(), String> {
    save_map::push_map_from_firestore(app, payload)
}

fn update_google_message(
    app: &AppHandle,
    game_id: &str,
    message: &str,
    restored: Option<usize>,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    let record = state.games.entry(game_id.to_string()).or_default();
    record.google_drive_message = message.to_string();
    if let Some(restored) = restored {
        record.google_drive_last_restore_count = restored;
    }
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn mark_game_running(game_id: &str, running: bool) {
    if let Ok(mut games) = running_games().lock() {
        if running {
            games.insert(game_id.to_string());
        } else {
            games.remove(game_id);
        }
    }
}

pub fn sync_after_exit_async(app: AppHandle, game_id: String) {
    thread::spawn(move || {
        mark_game_running(&game_id, false);
        if let Err(error) = wait_for_local_stability(&app, &game_id) {
            match record_background_retry(&app, &game_id, &error) {
                Ok(status) => {
                    let _ = app.emit("launcher://cloud-save", CloudSaveEvent { game_id, status });
                }
                Err(state_error) => {
                    let _ = app.emit(
                        "launcher://cloud-save-error",
                        CloudSaveErrorEvent {
                            game_id,
                            message: format!("{error} ({state_error})"),
                        },
                    );
                }
            }
            return;
        }
        let result = sync_game(&app, &game_id, SyncDirection::Auto);
        match result {
            Ok(status) => {
                let _ = app.emit("launcher://cloud-save", CloudSaveEvent { game_id, status });
            }
            Err(error) => {
                let _ = app.emit(
                    "launcher://cloud-save-error",
                    CloudSaveErrorEvent {
                        game_id,
                        message: error,
                    },
                );
            }
        }
    });
}

fn record_background_retry(
    app: &AppHandle,
    game_id: &str,
    reason: &str,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    seed_metadata_defaults(app, game_id, &mut state);
    let record = state.games.entry(game_id.to_string()).or_default();
    queue_pending_upload(record, 0, reason);
    record.cloud_state = "waiting_for_save".to_string();
    record.google_drive_message =
        "Game vẫn đang hoàn tất ghi Save. Launcher đã giữ bản an toàn và sẽ tự thử lại ở nền."
            .to_string();
    record.last_message = record.google_drive_message.clone();
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

fn wait_for_local_stability(app: &AppHandle, game_id: &str) -> Result<(), String> {
    let record = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        load_state_unlocked(app)?.games.get(game_id).cloned()
    };
    let Some(record) = record else {
        return Ok(());
    };
    if !record.enabled {
        return Ok(());
    }
    let roots = expanded_save_roots(app, game_id, &record)?;
    let poll = Duration::from_millis(record.poll_interval_ms.max(100));
    let settle = Duration::from_millis(record.settle_time_ms.max(record.poll_interval_ms));
    let deadline = std::time::Instant::now()
        + Duration::from_millis(record.max_stability_wait_ms.max(record.settle_time_ms));
    let mut previous = scan_local_roots(&roots, &record.include, &record.exclude)?;
    enforce_manifest_limits(&record, &previous)?;
    let mut stable_since = std::time::Instant::now();
    while std::time::Instant::now() < deadline {
        thread::sleep(poll);
        let current = scan_local_roots(&roots, &record.include, &record.exclude)?;
        enforce_manifest_limits(&record, &current)?;
        if manifests_equal(&previous, &current) {
            if stable_since.elapsed() >= settle {
                return Ok(());
            }
        } else {
            stable_since = std::time::Instant::now();
            previous = current;
        }
    }
    Err("Game vẫn đang ghi Save. Launcher đã giữ bản cũ và sẽ tự thử lại ở nền.".to_string())
}

fn enforce_manifest_limits(
    record: &GameCloudRecord,
    manifest: &CloudManifest,
) -> Result<(), String> {
    if manifest.files.len() as u64 > record.max_files {
        return Err(format!(
            "Cloud Save map vượt giới hạn an toàn: {} file (tối đa {}).",
            manifest.files.len(),
            record.max_files
        ));
    }
    let total = manifest_bytes(manifest);
    if total > record.max_total_bytes {
        return Err(format!(
            "Cloud Save map vượt giới hạn dung lượng an toàn: {total} byte."
        ));
    }
    if let Some(entry) = manifest
        .files
        .iter()
        .find(|entry| entry.size > record.max_file_bytes)
    {
        return Err(format!(
            "File Save {} vượt giới hạn dung lượng an toàn.",
            entry.path
        ));
    }
    Ok(())
}

pub fn resolve_conflict(
    app: &AppHandle,
    game_id: &str,
    conflict_id: &str,
    resolution: &str,
) -> Result<CloudSaveStatus, String> {
    let direction = match resolution.to_ascii_lowercase().as_str() {
        "local" | "uselocal" => SyncDirection::Push,
        "cloud" | "usecloud" => SyncDirection::Pull,
        _ => return Err("resolution must be local or cloud".to_string()),
    };
    let root = configured_sync_root(app)?;

    // Temporarily clear only the selected conflict before the forced sync. The
    // upload path intentionally refuses to commit while any conflict is active;
    // leaving it in state here would make "Use this PC" update the local mirror
    // but never advance the remote head. The conflict folder itself is retained
    // until the chosen resolution completes successfully.
    let selected_conflict = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let mut state = load_state_unlocked(app)?;
        let record = state
            .games
            .get_mut(game_id)
            .ok_or_else(|| "cloud save is not configured for this game".to_string())?;
        let position = record
            .conflicts
            .iter()
            .position(|conflict| conflict.id == conflict_id)
            .ok_or_else(|| "cloud save conflict not found".to_string())?;
        let selected = record.conflicts.remove(position);
        write_state_unlocked(app, &state)?;
        selected
    };

    if let Err(error) = sync_game(app, game_id, direction) {
        // Restore the unresolved conflict so the UI can offer the choice again.
        if let Ok(_guard) = state_lock().lock() {
            if let Ok(mut state) = load_state_unlocked(app) {
                let record = state.games.entry(game_id.to_string()).or_default();
                if !record
                    .conflicts
                    .iter()
                    .any(|item| item.id == selected_conflict.id)
                {
                    record.conflicts.push(selected_conflict);
                }
                let _ = write_state_unlocked(app, &state);
            }
        }
        return Err(error);
    }

    let conflict_root = game_cloud_root(&root, game_id)
        .join("conflicts")
        .join(conflict_id);
    if conflict_root.exists() {
        clear_tree_safely(&conflict_root)?;
    }
    get_status(app, game_id)
}

pub fn restore_snapshot(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
) -> Result<CloudSaveStatus, String> {
    ensure_not_running(game_id)?;
    {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let mut state = load_state_unlocked(app)?;
        let record = state
            .games
            .get_mut(game_id)
            .ok_or_else(|| "cloud save is not configured for this game".to_string())?;
        let root = configured_sync_root(app)?;
        let expanded_roots = expanded_save_roots(app, game_id, record)?;
        let snapshot = record
            .snapshots
            .iter()
            .find(|snapshot| snapshot.id == snapshot_id)
            .cloned()
            .ok_or_else(|| "cloud save snapshot not found".to_string())?;
        let game_root = game_cloud_root(&root, game_id);
        let snapshot_root = game_root.join("snapshots").join(&snapshot.id);
        let manifest = read_manifest(&snapshot_root.join("manifest.json"))?;
        validate_root_layout(&root, game_id, &expanded_roots)?;

        // The real local branch may be newer than the local mirror. Preserve it
        // before replacing a single file so a mistaken restore remains reversible.
        let current_local = scan_local_roots(&expanded_roots, &record.include, &record.exclude)?;
        snapshot_local(&game_root, record, &expanded_roots, &current_local)?;
        apply_cloud_manifest_to_local(
            &expanded_roots,
            &snapshot_root.join("files"),
            &current_local,
            &manifest,
        )?;
        commit_local_to_cloud(&game_root, &expanded_roots, &manifest)?;

        if let Some(restored) = record
            .snapshots
            .iter_mut()
            .find(|item| item.id == snapshot.id)
        {
            restored.source = "restored".to_string();
        }
        let conflict_ids = record
            .conflicts
            .iter()
            .map(|conflict| conflict.id.clone())
            .collect::<Vec<_>>();
        record.conflicts.clear();
        for conflict_id in conflict_ids {
            let conflict_root = game_root.join("conflicts").join(conflict_id);
            if conflict_root.exists() {
                clear_tree_safely(&conflict_root)?;
            }
        }
        record.last_sync_at = Some(Utc::now().to_rfc3339());
        record.cloud_state = "syncing".to_string();
        record.last_message = format!(
            "Đã khôi phục bản {}. Launcher đang cập nhật Google Drive.",
            snapshot.id
        );
        trim_snapshots(&game_root, record)?;
        write_state_unlocked(app, &state)?;
    }

    // A restore is an explicit user decision. Force-push that selected branch;
    // otherwise the next automatic sync could pull the newer remote head back
    // and appear to undo the restore. Offline/quota failures are queued safely.
    sync_game(app, game_id, SyncDirection::Push)
}

fn sync_game(
    app: &AppHandle,
    game_id: &str,
    direction: SyncDirection,
) -> Result<CloudSaveStatus, String> {
    let _sync_guard = SyncGuard::acquire(game_id)?;
    ensure_not_running(game_id)?;

    let mut record = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let mut state = load_state_unlocked(app)?;
        seed_metadata_defaults(app, game_id, &mut state);
        write_state_unlocked(app, &state)?;
        state.games.get(game_id).cloned().unwrap_or_default()
    };
    if !record.enabled {
        return persist_game_record(app, game_id, record);
    }

    let root = configured_sync_root(app)?;
    let expanded_roots = expanded_save_roots(app, game_id, &record)?;
    if expanded_roots.is_empty() {
        record.cloud_state = "waiting_for_first_save".to_string();
        record.last_message = "Đang chờ game tạo file Save lần đầu.".to_string();
        return persist_game_record(app, game_id, record);
    }
    validate_root_layout(&root, game_id, &expanded_roots)?;

    let local_manifest = scan_local_roots(&expanded_roots, &record.include, &record.exclude)?;
    enforce_manifest_limits(&record, &local_manifest)?;
    let game_root = game_cloud_root(&root, game_id);
    fs::create_dir_all(&game_root).map_err(|error| error.to_string())?;
    let current_root = game_root.join("current");
    let local_mirror_manifest = scan_cloud_current(&current_root)?;
    let remote_cache = game_root.join("remote-cache");

    // The local mirror is a durable safety copy, not the last common cloud
    // ancestor. Never treat it as remote state or advance the baseline while an
    // upload is pending; doing so could hide a genuine two-device conflict.
    let mut cloud_manifest = CloudManifest::default();
    let mut cloud_files_root = remote_cache.join("files");
    let mut remote_head: Option<google_drive::RemoteHead> = None;
    let mut remote_available = false;

    if !google_drive::connected(app) {
        if !manifests_equal(&local_manifest, &local_mirror_manifest) {
            snapshot_local(&game_root, &mut record, &expanded_roots, &local_manifest)?;
            commit_local_to_cloud(&game_root, &expanded_roots, &local_manifest)?;
        }
        if !local_manifest.files.is_empty() {
            queue_pending_upload(
                &mut record,
                manifest_bytes(&local_manifest),
                "Cần kết nối Google Drive.",
            );
        }
        record.cloud_state = "auth_required".to_string();
        record.google_drive_message =
            "Save được bảo vệ trên máy này. Hãy kết nối Google Drive để đồng bộ đa thiết bị."
                .to_string();
        record.last_message = "Save mới nhất đã được bảo vệ trên máy này.".to_string();
        trim_snapshots(&game_root, &mut record)?;
        return persist_game_record(app, game_id, record);
    }

    if google_drive::connected(app) {
        if let Some(token) = record.drive_change_token.clone() {
            match google_drive::changes_since(app, &token) {
                Ok((changed, new_token)) => {
                    record.drive_change_token = Some(new_token);
                    if !changed.is_empty() {
                        record.remote_newer_known = true;
                    }
                }
                Err(error) => {
                    // Change tracking is an optimization. A full head fetch below remains authoritative.
                    if matches!(error.kind, google_drive::DriveFailureKind::AuthRequired) {
                        apply_drive_failure(&mut record, &error, manifest_bytes(&local_manifest));
                        return persist_game_record(app, game_id, record);
                    }
                }
            }
        } else if let Ok(token) = google_drive::get_start_page_token(app) {
            record.drive_change_token = Some(token);
        }

        match google_drive::fetch_remote_to_cache(app, game_id, &remote_cache) {
            Ok(Some(remote)) => {
                remote_available = true;
                cloud_manifest = remote.manifest.clone();
                enforce_manifest_limits(&record, &cloud_manifest)?;
                cloud_files_root = remote.files_root.clone();
                record.remote_newer_known = record
                    .remote_snapshot_id
                    .as_deref()
                    .is_some_and(|snapshot| snapshot != remote.head.snapshot_id)
                    || remote.head.generation > record.remote_generation;
                record.remote_snapshot_id = Some(remote.head.snapshot_id.clone());
                record.remote_generation = remote.head.generation;
                record.remote_device_name = remote.head.device_name.clone();
                remote_head = Some(remote.head);
            }
            Ok(None) => {
                record.remote_newer_known = false;
            }
            Err(error) => {
                if record.remote_newer_known && direction != SyncDirection::Push {
                    apply_drive_failure(&mut record, &error, manifest_bytes(&local_manifest));
                    record.last_message = "Cloud có thể có Save mới hơn nhưng Google Drive chưa phản hồi. Save trên máy chưa bị thay đổi.".to_string();
                    return persist_game_record(app, game_id, record);
                }
                apply_drive_failure(&mut record, &error, manifest_bytes(&local_manifest));
                // Offline-first: keep playing and retain the durable upload queue.
                if direction != SyncDirection::Pull {
                    if !manifests_equal(&local_manifest, &local_mirror_manifest) {
                        snapshot_local(&game_root, &mut record, &expanded_roots, &local_manifest)?;
                        commit_local_to_cloud(&game_root, &expanded_roots, &local_manifest)?;
                    }
                    trim_snapshots(&game_root, &mut record)?;
                    return persist_game_record(app, game_id, record);
                }
                return persist_game_record(app, game_id, record);
            }
        }
    }

    let baseline = record.baseline.clone().unwrap_or_default();
    let action = determine_sync_action(
        direction,
        record.baseline.is_some(),
        &baseline,
        &local_manifest,
        &cloud_manifest,
    );
    let mut upload_required = false;

    match action {
        SyncAction::Push => {
            if !cloud_manifest.files.is_empty() {
                if remote_available {
                    create_snapshot_from_tree(
                        &game_root,
                        &mut record,
                        &cloud_files_root,
                        &cloud_manifest,
                        "cloud",
                    )?;
                } else {
                    snapshot_cloud_current(&game_root, &mut record, &cloud_manifest, "cloud")?;
                }
            }
            if !manifests_equal(&local_manifest, &local_mirror_manifest) {
                snapshot_local(&game_root, &mut record, &expanded_roots, &local_manifest)?;
            }
            commit_local_to_cloud(&game_root, &expanded_roots, &local_manifest)?;
            record.last_message =
                "Save trên máy đã được bảo vệ; đang cập nhật Google Drive.".to_string();
            upload_required = !local_manifest.files.is_empty();
        }
        SyncAction::Pull => {
            if cloud_manifest.files.is_empty() {
                record.last_message = "Google Drive chưa có Save cho game này.".to_string();
            } else {
                snapshot_local(&game_root, &mut record, &expanded_roots, &local_manifest)?;
                apply_cloud_manifest_to_local(
                    &expanded_roots,
                    &cloud_files_root,
                    &local_manifest,
                    &cloud_manifest,
                )?;
                commit_local_to_cloud(&game_root, &expanded_roots, &cloud_manifest)?;
                record.baseline = Some(cloud_manifest.clone());
                record.google_drive_last_restore_count = cloud_manifest.files.len();
                record.last_message =
                    "Save mới nhất từ Google Drive đã sẵn sàng trên máy này.".to_string();
                record.remote_newer_known = false;
            }
        }
        SyncAction::Conflict => {
            create_conflict(
                &game_root,
                &mut record,
                &expanded_roots,
                &local_manifest,
                cloud_files_root
                    .parent()
                    .ok_or_else(|| "Cloud cache path is invalid".to_string())?,
                &cloud_manifest,
            )?;
            record.cloud_state = "conflict".to_string();
            record.last_message =
                "Hai bản Save đều đã thay đổi. Launcher đã giữ an toàn cả hai và cần bạn chọn."
                    .to_string();
        }
        SyncAction::Baseline => {
            record.baseline = Some(local_manifest.clone());
            if !local_manifest.files.is_empty() {
                commit_local_to_cloud(&game_root, &expanded_roots, &local_manifest)?;
            }
            record.last_message = "Cloud Save đã ghi nhận trạng thái hiện tại.".to_string();
        }
        SyncAction::Noop => {
            record.last_message = "Save đã được đồng bộ.".to_string();
            upload_required =
                !record.pending_operations.is_empty() && !local_manifest.files.is_empty();
        }
    }

    if upload_required && record.conflicts.is_empty() {
        if google_drive::connected(app) {
            let snapshot_id = format!("{}-auto", timestamp_id());
            let parent = remote_head
                .as_ref()
                .map(|head| head.snapshot_id.as_str())
                .or(record.remote_snapshot_id.as_deref());
            let generation = remote_head
                .as_ref()
                .map(|head| head.generation)
                .unwrap_or(record.remote_generation)
                .saturating_add(1);
            match upload_snapshot_with_recovery(
                app,
                game_id,
                &record,
                &snapshot_id,
                parent,
                generation,
                &current_root,
            ) {
                Ok(head) => {
                    let committed_snapshot_id = head.snapshot_id.clone();
                    let committed_generation = head.generation;
                    record.remote_snapshot_id = Some(committed_snapshot_id.clone());
                    record.remote_generation = committed_generation;
                    record.baseline = Some(local_manifest.clone());
                    record.remote_device_name = head.device_name;
                    record.remote_newer_known = false;
                    record.pending_operations.clear();
                    record.google_drive_last_backup_at = Some(Utc::now().to_rfc3339());
                    record.google_drive_message = "Đã đồng bộ với Google Drive.".to_string();
                    record.cloud_state = "synced".to_string();
                    record.last_message =
                        "Save đã được bảo vệ trên máy này và Google Drive.".to_string();
                    // Remote retention is best-effort and intentionally amortized
                    // so thousands of users do not generate a full Drive listing
                    // after every short play session. Storage-full recovery still
                    // runs maintenance immediately.
                    if committed_generation % 10 == 0 {
                        let _ = google_drive::prune_remote_history(
                            app,
                            game_id,
                            Some(&committed_snapshot_id),
                        );
                    }
                }
                Err(error) => {
                    apply_drive_failure(&mut record, &error, manifest_bytes(&local_manifest));
                }
            }
        } else {
            queue_pending_upload(
                &mut record,
                manifest_bytes(&local_manifest),
                "Cần kết nối Google Drive.",
            );
            record.cloud_state = "auth_required".to_string();
            record.google_drive_message =
                "Save được bảo vệ trên máy này. Hãy kết nối Google Drive để đồng bộ đa thiết bị."
                    .to_string();
        }
    } else if record.conflicts.is_empty() && google_drive::connected(app) {
        record.cloud_state = "synced".to_string();
        record.remote_newer_known = false;
    }

    if google_drive::connected(app) {
        if let Ok(quota) = google_drive::storage_quota(app) {
            record.quota = Some(quota_status(quota));
        }
    }
    record.last_sync_at = Some(Utc::now().to_rfc3339());
    trim_snapshots(&game_root, &mut record)?;
    persist_game_record(app, game_id, record)
}

fn persist_game_record(
    app: &AppHandle,
    game_id: &str,
    record: GameCloudRecord,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    state.games.insert(game_id.to_string(), record);
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

fn upload_snapshot_with_recovery(
    app: &AppHandle,
    game_id: &str,
    record: &GameCloudRecord,
    snapshot_id: &str,
    parent_snapshot_id: Option<&str>,
    generation: u64,
    current_root: &Path,
) -> Result<google_drive::RemoteHead, google_drive::DriveFailure> {
    let upload = || {
        google_drive::upload_current_snapshot(
            app,
            game_id,
            &record.device_id,
            &record.device_name,
            snapshot_id,
            parent_snapshot_id,
            generation,
            current_root,
            "automatic",
            false,
        )
    };
    match upload() {
        Err(error) if error.kind == google_drive::DriveFailureKind::StorageFull => {
            // Prune only superseded, unpinned history according to the approved
            // retention policy, then remove objects no remaining manifest uses.
            let _ = google_drive::prune_remote_history(
                app,
                game_id,
                record.remote_snapshot_id.as_deref(),
            );
            upload()
        }
        result => result,
    }
}

fn apply_drive_failure(
    record: &mut GameCloudRecord,
    error: &google_drive::DriveFailure,
    pending_bytes: u64,
) {
    use google_drive::DriveFailureKind;
    record.google_drive_message = error.message.clone();
    record.cloud_state = match error.kind {
        DriveFailureKind::Offline => "offline",
        DriveFailureKind::RateLimited => "rate_limited",
        DriveFailureKind::StorageFull => "storage_full",
        DriveFailureKind::AuthRequired => "auth_required",
        DriveFailureKind::CorruptRemote => "remote_damaged",
        DriveFailureKind::PermissionDenied => "permission_denied",
        DriveFailureKind::NotFound => "waiting_for_first_save",
        DriveFailureKind::RemoteChanged => "conflict_check_required",
        DriveFailureKind::Other => "attention",
    }
    .to_string();
    record.last_message = error.message.clone();
    if error.kind == DriveFailureKind::RemoteChanged {
        record.remote_newer_known = true;
    }
    if error.retryable
        || matches!(
            error.kind,
            DriveFailureKind::AuthRequired | DriveFailureKind::StorageFull
        )
    {
        queue_pending_upload(record, pending_bytes, &error.reason);
    }
    if error.kind == DriveFailureKind::StorageFull {
        let mut quota = record.quota.clone().unwrap_or_default();
        quota.state = "full".to_string();
        quota.checked_at = Utc::now().to_rfc3339();
        record.quota = Some(quota);
    }
}

fn queue_pending_upload(record: &mut GameCloudRecord, bytes: u64, reason: &str) {
    if let Some(operation) = record
        .pending_operations
        .iter_mut()
        .find(|operation| operation.kind == "upload_latest")
    {
        operation.attempts = operation.attempts.saturating_add(1);
        operation.next_retry_at = next_background_retry(operation.attempts).to_rfc3339();
        operation.bytes = operation.bytes.max(bytes);
        operation.reason = reason.to_string();
        return;
    }
    record.pending_operations.push(PendingCloudOperation {
        id: timestamp_id(),
        kind: "upload_latest".to_string(),
        created_at: Utc::now().to_rfc3339(),
        next_retry_at: next_background_retry(0).to_rfc3339(),
        attempts: 0,
        snapshot_id: record
            .remote_snapshot_id
            .clone()
            .unwrap_or_else(|| "local-head".to_string()),
        bytes,
        reason: reason.to_string(),
    });
}

fn next_background_retry(attempts: u32) -> DateTime<Utc> {
    // Immediate API retries use second-scale exponential backoff in the Drive
    // client. Durable retries are deliberately slower to protect user quota.
    let exponent = attempts.min(6);
    let minutes = (5_i64 * (1_i64 << exponent)).min(6 * 60);
    Utc::now() + chrono::Duration::minutes(minutes)
}

fn pending_retry_due(operation: &PendingCloudOperation) -> bool {
    DateTime::parse_from_rfc3339(&operation.next_retry_at)
        .map(|value| value.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(true)
}

fn quota_status(quota: google_drive::DriveQuota) -> CloudQuotaStatus {
    let state = match (quota.limit_bytes, quota.available_bytes) {
        (_, Some(0)) => "full",
        (Some(limit), Some(available)) if available < limit / 20 => "low",
        _ => "healthy",
    };
    CloudQuotaStatus {
        limit_bytes: quota.limit_bytes,
        usage_bytes: quota.usage_bytes,
        available_bytes: quota.available_bytes,
        checked_at: quota.checked_at,
        state: state.to_string(),
    }
}

fn determine_sync_action(
    direction: SyncDirection,
    has_baseline: bool,
    baseline: &CloudManifest,
    local: &CloudManifest,
    cloud: &CloudManifest,
) -> SyncAction {
    match direction {
        SyncDirection::Push => SyncAction::Push,
        SyncDirection::Pull => SyncAction::Pull,
        SyncDirection::Auto if !has_baseline => {
            if local.files.is_empty() && !cloud.files.is_empty() {
                SyncAction::Pull
            } else if !local.files.is_empty() && cloud.files.is_empty() {
                SyncAction::Push
            } else if manifests_equal(local, cloud) {
                SyncAction::Baseline
            } else {
                SyncAction::Conflict
            }
        }
        SyncDirection::Auto => {
            let local_changed = !manifests_equal(local, baseline);
            let cloud_changed = !manifests_equal(cloud, baseline);
            if local_changed && cloud_changed {
                if manifests_equal(local, cloud) {
                    SyncAction::Baseline
                } else {
                    SyncAction::Conflict
                }
            } else if local_changed {
                SyncAction::Push
            } else if cloud_changed {
                SyncAction::Pull
            } else {
                SyncAction::Noop
            }
        }
    }
}

fn configured_sync_root(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("cloud-save-v2");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn ensure_not_running(game_id: &str) -> Result<(), String> {
    let running = running_games()
        .lock()
        .map_err(|_| "cloud save runtime lock poisoned".to_string())?
        .contains(game_id);
    if running {
        Err("cloud saves cannot sync while the game is running".to_string())
    } else {
        Ok(())
    }
}

fn expanded_save_roots(
    app: &AppHandle,
    game_id: &str,
    record: &GameCloudRecord,
) -> Result<Vec<PathBuf>, String> {
    let install_dir = crate::platform::install_record(app, game_id)?
        .map(|record| record.install_path)
        .unwrap_or_default();
    let steam_environment = crate::steam_integration::environment_info(app);
    let steam_user_data_root = steam_environment
        .root_path
        .as_deref()
        .map(|root| PathBuf::from(root).join("userdata").display().to_string());
    record
        .save_roots
        .iter()
        .map(|root| {
            expand_path_template(
                &root.path,
                game_id,
                &install_dir,
                steam_user_data_root.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .filter(|path| path.is_absolute())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|path| {
            fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            Ok(path)
        })
        .collect()
}

fn validate_root_layout(
    sync_root: &Path,
    game_id: &str,
    save_roots: &[PathBuf],
) -> Result<(), String> {
    let sync_root = fs::canonicalize(sync_root).map_err(|error| error.to_string())?;
    let game_root = sync_root.join(CLOUD_FOLDER).join(sanitize_id(game_id));
    let mut canonical_roots = Vec::with_capacity(save_roots.len());
    for root in save_roots {
        if is_reparse_or_symlink(root)? {
            return Err(format!(
                "cloud save root cannot be a symlink or reparse point: {}",
                root.display()
            ));
        }
        let canonical = fs::canonicalize(root).map_err(|error| error.to_string())?;
        if canonical.starts_with(&sync_root)
            || sync_root.starts_with(&canonical)
            || canonical.starts_with(&game_root)
        {
            return Err(format!(
                "local save folder overlaps the cloud provider folder: {}",
                root.display()
            ));
        }
        if canonical_roots.iter().any(|existing: &PathBuf| {
            canonical.starts_with(existing) || existing.starts_with(&canonical)
        }) {
            return Err(format!(
                "configured save folders cannot overlap: {}",
                root.display()
            ));
        }
        canonical_roots.push(canonical);
    }
    Ok(())
}

fn expand_path_template(
    value: &str,
    game_id: &str,
    install_dir: &str,
    steam_user_data_root: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let user_profile = env::var("USERPROFILE").unwrap_or_default();
    let app_data = env::var("APPDATA").unwrap_or_default();
    let local_app_data = env::var("LOCALAPPDATA").unwrap_or_default();
    let expanded = value
        .replace("{gameId}", game_id)
        .replace("{installDir}", install_dir)
        .replace("{userProfile}", &user_profile)
        .replace("{documents}", &format!("{user_profile}\\Documents"))
        .replace("{appData}", &app_data)
        .replace("{localAppData}", &local_app_data);

    if expanded.contains("{steamUserData}") {
        let steam_user_data_root = steam_user_data_root.ok_or_else(|| {
            "Steam userdata path is unavailable because Steam installation was not detected"
                .to_string()
        })?;

        let root_path = Path::new(steam_user_data_root);
        let mut results = Vec::new();

        if let Ok(entries) = std::fs::read_dir(root_path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let account_id = entry.file_name().to_string_lossy().to_string();
                        // Ignore non-numeric folders like '0' or 'anonymous' or 'config' if desired,
                        // but Steam usually keeps actual IDs as numbers.
                        if account_id.chars().all(|c| c.is_ascii_digit()) {
                            let account_path = root_path.join(&account_id).display().to_string();
                            let replaced = expanded.replace("{steamUserData}", &account_path);
                            results.push(PathBuf::from(replaced));
                        }
                    }
                }
            }
        }

        // If no numeric folders found, return empty or fallback
        if results.is_empty() {
            // fallback so it doesn't just error out silently if they have no login
            let replaced = expanded.replace(
                "{steamUserData}",
                &root_path.join("0").display().to_string(),
            );
            results.push(PathBuf::from(replaced));
        }

        Ok(results)
    } else {
        Ok(vec![PathBuf::from(expanded)])
    }
}

fn scan_local_roots(
    roots: &[PathBuf],
    include: &[String],
    exclude: &[String],
) -> Result<CloudManifest, String> {
    let mut files = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        if !root.exists() {
            continue;
        }
        let mut walker = WalkDir::new(root).follow_links(false).into_iter();
        while let Some(entry) = walker.next() {
            let entry = entry.map_err(|error| error.to_string())?;
            if is_reparse_or_symlink(entry.path())? {
                if entry.file_type().is_dir() {
                    walker.skip_current_dir();
                }
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| error.to_string())?;
            let relative = normalize_relative_path(relative)?;
            if !path_selected(&relative, include, exclude) {
                continue;
            }
            files.push(file_entry(
                entry.path(),
                format!("root-{index}/{relative}"),
            )?);
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(CloudManifest {
        generated_at: Utc::now().to_rfc3339(),
        files,
    })
}

fn scan_cloud_current(current_root: &Path) -> Result<CloudManifest, String> {
    recover_cloud_current(current_root)?;
    let files_root = current_root.join("files");
    if !files_root.exists() {
        return Ok(CloudManifest::default());
    }
    if is_reparse_or_symlink(&files_root)? {
        return Err("cloud save current folder cannot be a symlink or reparse point".to_string());
    }
    let mut files = Vec::new();
    let mut walker = WalkDir::new(&files_root).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry.map_err(|error| error.to_string())?;
        if is_reparse_or_symlink(entry.path())? {
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = normalize_relative_path(
            entry
                .path()
                .strip_prefix(&files_root)
                .map_err(|error| error.to_string())?,
        )?;
        files.push(file_entry(entry.path(), relative)?);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(CloudManifest {
        generated_at: Utc::now().to_rfc3339(),
        files,
    })
}

fn file_entry(path: &Path, relative: String) -> Result<CloudFileEntry, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_at_ms = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut blake3_hasher = blake3::Hasher::new();
    let mut sha256_hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        blake3_hasher.update(&buffer[..read]);
        sha256_hasher.update(&buffer[..read]);
    }
    Ok(CloudFileEntry {
        path: relative,
        size: metadata.len(),
        modified_at_ms,
        blake3: blake3_hasher.finalize().to_hex().to_string(),
        sha256: format!("{:x}", sha256_hasher.finalize()),
    })
}

fn manifests_equal(left: &CloudManifest, right: &CloudManifest) -> bool {
    if left.files.len() != right.files.len() {
        return false;
    }
    left.files.iter().zip(&right.files).all(|(left, right)| {
        left.path == right.path && left.size == right.size && left.blake3 == right.blake3
    })
}

fn commit_local_to_cloud(
    game_root: &Path,
    local_roots: &[PathBuf],
    manifest: &CloudManifest,
) -> Result<(), String> {
    fs::create_dir_all(game_root).map_err(|error| error.to_string())?;
    let stage = game_root.join(format!(".stage-{}", timestamp_id()));
    let stage_files = stage.join("files");
    fs::create_dir_all(&stage_files).map_err(|error| error.to_string())?;
    for entry in &manifest.files {
        let (root_index, relative) = parse_manifest_path(&entry.path)?;
        let source_root = local_roots
            .get(root_index)
            .ok_or_else(|| "cloud manifest root index is invalid".to_string())?;
        let source = safe_join(source_root, &relative)?;
        let target = safe_join(&stage_files, &entry.path)?;
        copy_file_synced(&source, &target)?;
    }
    write_manifest(&stage.join("manifest.json"), manifest)?;

    let current = game_root.join("current");
    let previous = game_root.join(".current-backup");
    if previous.exists() {
        clear_tree_safely(&previous)?;
    }
    if current.exists() {
        fs::rename(&current, &previous).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&stage, &current) {
        if previous.exists() {
            let _ = fs::rename(&previous, &current);
        }
        return Err(error.to_string());
    }
    if previous.exists() {
        clear_tree_safely(&previous)?;
    }
    Ok(())
}

fn recover_cloud_current(current_root: &Path) -> Result<(), String> {
    let Some(game_root) = current_root.parent() else {
        return Err("cloud save current folder has no parent".to_string());
    };
    let backup = game_root.join(".current-backup");
    if !current_root.exists() && backup.exists() {
        fs::rename(&backup, current_root).map_err(|error| error.to_string())?;
    } else if current_root.exists() && backup.exists() {
        clear_tree_safely(&backup)?;
    }
    Ok(())
}

fn apply_cloud_manifest_to_local(
    roots: &[PathBuf],
    cloud_files_root: &Path,
    current_local: &CloudManifest,
    manifest: &CloudManifest,
) -> Result<(), String> {
    let transaction_id = timestamp_id();
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    for entry in &manifest.files {
        let (root_index, relative) = parse_manifest_path(&entry.path)?;
        let root = roots
            .get(root_index)
            .ok_or_else(|| "cloud manifest root index is invalid".to_string())?;
        let source = safe_join(cloud_files_root, &entry.path)?;
        let target = secure_local_target(root, &relative)?;
        let temporary = sibling_path(&target, &format!("0xo.cloud.{transaction_id}.tmp"))?;
        if let Err(error) = copy_file_synced(&source, &temporary) {
            for (temporary, _) in &staged {
                if temporary.exists() {
                    let _ = fs::remove_file(temporary);
                }
            }
            return Err(error);
        }
        staged.push((temporary, target));
    }

    let desired_paths = manifest
        .files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<HashSet<_>>();
    let obsolete = current_local
        .files
        .iter()
        .filter(|entry| !desired_paths.contains(entry.path.as_str()))
        .map(|entry| {
            let (root_index, relative) = parse_manifest_path(&entry.path)?;
            let root = roots
                .get(root_index)
                .ok_or_else(|| "cloud manifest root index is invalid".to_string())?;
            secure_local_target(root, &relative)
        })
        .collect::<Result<Vec<_>, String>>()?;

    for (_, target) in &staged {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }

    let mut committed: Vec<(PathBuf, PathBuf, bool, bool)> = Vec::new();
    for (temporary, target) in staged {
        let backup = sibling_path(&target, &format!("0xo.cloud.{transaction_id}.bak"))?;
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        let had_original = target.exists();
        if had_original {
            fs::rename(&target, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&temporary, &target) {
            if backup.exists() {
                let _ = fs::rename(&backup, &target);
            }
            rollback_files(&committed);
            return Err(error.to_string());
        }
        committed.push((target, backup, had_original, true));
    }

    for target in obsolete {
        if !target.exists() {
            continue;
        }
        let backup = sibling_path(&target, &format!("0xo.cloud.{transaction_id}.bak"))?;
        if let Err(error) = fs::rename(&target, &backup) {
            rollback_files(&committed);
            return Err(error.to_string());
        }
        committed.push((target, backup, true, false));
    }

    for (_, backup, _, _) in committed {
        if backup.exists() {
            fs::remove_file(backup).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn snapshot_cloud_current(
    game_root: &Path,
    record: &mut GameCloudRecord,
    manifest: &CloudManifest,
    source: &str,
) -> Result<(), String> {
    if manifest.files.is_empty() {
        return Ok(());
    }
    let current = game_root.join("current");
    create_snapshot_from_tree(game_root, record, &current.join("files"), manifest, source)
}

fn snapshot_local(
    game_root: &Path,
    record: &mut GameCloudRecord,
    roots: &[PathBuf],
    manifest: &CloudManifest,
) -> Result<(), String> {
    if manifest.files.is_empty() {
        return Ok(());
    }
    let id = format!("{}-local", timestamp_id());
    let target = game_root.join("snapshots").join(&id);
    for entry in &manifest.files {
        let (root_index, relative) = parse_manifest_path(&entry.path)?;
        let source = safe_join(
            roots
                .get(root_index)
                .ok_or_else(|| "snapshot root index is invalid".to_string())?,
            &relative,
        )?;
        copy_file_synced(&source, &safe_join(&target.join("files"), &entry.path)?)?;
    }
    write_manifest(&target.join("manifest.json"), manifest)?;
    record
        .snapshots
        .push(snapshot_summary(&id, "local", manifest));
    Ok(())
}

fn create_snapshot_from_tree(
    game_root: &Path,
    record: &mut GameCloudRecord,
    source_files: &Path,
    manifest: &CloudManifest,
    source: &str,
) -> Result<(), String> {
    let id = format!("{}-{source}", timestamp_id());
    let target = game_root.join("snapshots").join(&id);
    for entry in &manifest.files {
        copy_file_synced(
            &safe_join(source_files, &entry.path)?,
            &safe_join(&target.join("files"), &entry.path)?,
        )?;
    }
    write_manifest(&target.join("manifest.json"), manifest)?;
    record
        .snapshots
        .push(snapshot_summary(&id, source, manifest));
    Ok(())
}

fn create_conflict(
    game_root: &Path,
    record: &mut GameCloudRecord,
    local_roots: &[PathBuf],
    local: &CloudManifest,
    cloud_current: &Path,
    cloud: &CloudManifest,
) -> Result<(), String> {
    if let Some(existing) = record.conflicts.last() {
        if existing.local_file_count == local.files.len()
            && existing.cloud_file_count == cloud.files.len()
        {
            return Ok(());
        }
    }
    let id = timestamp_id();
    let conflict_root = game_root.join("conflicts").join(&id);
    for entry in &local.files {
        let (root_index, relative) = parse_manifest_path(&entry.path)?;
        let source = safe_join(
            local_roots
                .get(root_index)
                .ok_or_else(|| "conflict root index is invalid".to_string())?,
            &relative,
        )?;
        copy_file_synced(
            &source,
            &safe_join(&conflict_root.join("local").join("files"), &entry.path)?,
        )?;
    }
    for entry in &cloud.files {
        copy_file_synced(
            &safe_join(&cloud_current.join("files"), &entry.path)?,
            &safe_join(&conflict_root.join("cloud").join("files"), &entry.path)?,
        )?;
    }
    write_manifest(&conflict_root.join("local").join("manifest.json"), local)?;
    write_manifest(&conflict_root.join("cloud").join("manifest.json"), cloud)?;
    let recommendation = recommend_conflict_branch(local, cloud);
    record.conflicts.push(CloudConflictSummary {
        id,
        created_at: Utc::now().to_rfc3339(),
        local_file_count: local.files.len(),
        cloud_file_count: cloud.files.len(),
        local_bytes: manifest_bytes(local),
        cloud_bytes: manifest_bytes(cloud),
        recommended: recommendation.branch,
        local_device: record.device_name.clone(),
        cloud_device: if record.remote_device_name.is_empty() {
            "Google Drive".to_string()
        } else {
            record.remote_device_name.clone()
        },
        recommendation_reason: recommendation.reason,
        recommendation_confidence: recommendation.confidence,
        local_latest_write_at_ms: recommendation.local_latest_write_at_ms,
        cloud_latest_write_at_ms: recommendation.cloud_latest_write_at_ms,
    });
    Ok(())
}

struct ConflictRecommendation {
    branch: String,
    reason: String,
    confidence: String,
    local_latest_write_at_ms: u64,
    cloud_latest_write_at_ms: u64,
}

fn recommend_conflict_branch(
    local: &CloudManifest,
    cloud: &CloudManifest,
) -> ConflictRecommendation {
    let local_latest = local
        .files
        .iter()
        .map(|entry| entry.modified_at_ms)
        .max()
        .unwrap_or(0);
    let cloud_latest = cloud
        .files
        .iter()
        .map(|entry| entry.modified_at_ms)
        .max()
        .unwrap_or(0);
    let difference = local_latest.abs_diff(cloud_latest);

    if difference >= 5 * 60 * 1_000 {
        let (branch, label) = if local_latest > cloud_latest {
            ("local", "Bản trên máy có dữ liệu được ghi gần đây hơn.")
        } else {
            ("cloud", "Bản trên Cloud có dữ liệu được ghi gần đây hơn.")
        };
        return ConflictRecommendation {
            branch: branch.to_string(),
            reason: label.to_string(),
            confidence: "medium".to_string(),
            local_latest_write_at_ms: local_latest,
            cloud_latest_write_at_ms: cloud_latest,
        };
    }

    // File timestamps can drift between devices and do not reveal in-game
    // progress. Prefer the branch on the device the user is actively using, but
    // explicitly report low confidence and preserve both branches.
    ConflictRecommendation {
        branch: "local".to_string(),
        reason: "Không thể xác định chắc chắn từ dữ liệu kỹ thuật; launcher ưu tiên phiên trên máy này và vẫn giữ an toàn cả hai bản.".to_string(),
        confidence: "low".to_string(),
        local_latest_write_at_ms: local_latest,
        cloud_latest_write_at_ms: cloud_latest,
    }
}

fn trim_snapshots(game_root: &Path, record: &mut GameCloudRecord) -> Result<(), String> {
    record
        .snapshots
        .sort_by(|left, right| right.created_at.cmp(&left.created_at));
    let keep = retention_keep_set(&record.snapshots, Utc::now());
    let mut retained = Vec::new();
    for snapshot in record.snapshots.drain(..) {
        if keep.contains(&snapshot.id) && retained.len() < MAX_SNAPSHOTS {
            retained.push(snapshot);
        } else {
            let path = game_root.join("snapshots").join(&snapshot.id);
            if path.exists() {
                clear_tree_safely(&path)?;
            }
        }
    }
    record.snapshots = retained;
    Ok(())
}

fn retention_keep_set(snapshots: &[CloudSnapshotSummary], now: DateTime<Utc>) -> HashSet<String> {
    const RECENT: usize = 10;
    const DAILY_DAYS: i64 = 7;
    const WEEKLY_WEEKS: i64 = 4;
    const CONFLICT_DAYS: i64 = 90;

    let mut ordered = snapshots.to_vec();
    ordered.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    let mut keep = HashSet::new();
    for snapshot in &ordered {
        if snapshot.pinned {
            keep.insert(snapshot.id.clone());
        }
    }
    for snapshot in ordered.iter().take(RECENT) {
        keep.insert(snapshot.id.clone());
    }

    let mut daily = BTreeSet::new();
    let mut weekly = BTreeSet::new();
    for snapshot in &ordered {
        let Ok(created) = DateTime::parse_from_rfc3339(&snapshot.created_at) else {
            continue;
        };
        let created = created.with_timezone(&Utc);
        let age = now.signed_duration_since(created);
        if age.num_days() <= DAILY_DAYS && daily.insert(created.date_naive()) {
            keep.insert(snapshot.id.clone());
        }
        let iso = created.iso_week();
        let week_key = (iso.year(), iso.week());
        if age.num_weeks() <= WEEKLY_WEEKS && weekly.insert(week_key) {
            keep.insert(snapshot.id.clone());
        }
        if snapshot.snapshot_class == "conflict" && age.num_days() <= CONFLICT_DAYS {
            keep.insert(snapshot.id.clone());
        }
        if matches!(
            snapshot.source.as_str(),
            "restored" | "previous" | "current"
        ) {
            keep.insert(snapshot.id.clone());
        }
    }
    keep
}

fn snapshot_summary(id: &str, source: &str, manifest: &CloudManifest) -> CloudSnapshotSummary {
    CloudSnapshotSummary {
        id: id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        source: source.to_string(),
        file_count: manifest.files.len(),
        bytes: manifest_bytes(manifest),
        pinned: false,
        snapshot_class: if source == "conflict" {
            "conflict".to_string()
        } else {
            "automatic".to_string()
        },
    }
}

fn manifest_bytes(manifest: &CloudManifest) -> u64 {
    manifest.files.iter().map(|entry| entry.size).sum()
}

fn build_status(
    app: &AppHandle,
    game_id: &str,
    record: Option<&GameCloudRecord>,
) -> CloudSaveStatus {
    let record = record.cloned().unwrap_or_default();
    let game_running = running_games()
        .lock()
        .map(|games| games.contains(game_id))
        .unwrap_or(false);
    let state = if !record.conflicts.is_empty() {
        "conflict".to_string()
    } else if !record.cloud_state.is_empty() {
        record.cloud_state.clone()
    } else if record.enabled {
        "ready".to_string()
    } else {
        "disabled".to_string()
    };
    let pending_operation_count = record.pending_operations.len();
    let pending_upload_bytes = record
        .pending_operations
        .iter()
        .map(|operation| operation.bytes)
        .sum();
    let can_sync = !game_running && record.enabled && !record.save_roots.is_empty();
    CloudSaveStatus {
        game_id: game_id.to_string(),
        enabled: record.enabled,
        automatic_protection: record.automatic_protection,
        sync_root: configured_sync_root(app)
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        save_roots: record.save_roots,
        include: record.include,
        exclude: record.exclude,
        state,
        last_sync_at: record.last_sync_at,
        last_message: record.last_message,
        conflicts: record.conflicts,
        snapshots: record.snapshots,
        can_sync,
        game_running,
        google_drive_configured: google_drive::client_configured(),
        google_drive_connected: google_drive::connected(app),
        google_drive_last_backup_at: record.google_drive_last_backup_at,
        google_drive_last_restore_count: record.google_drive_last_restore_count,
        google_drive_message: record.google_drive_message,
        pending_operation_count,
        pending_upload_bytes,
        quota: record.quota,
        map_status: record.map_status,
        remote_newer_known: record.remote_newer_known,
    }
}

fn seed_metadata_defaults(app: &AppHandle, game_id: &str, state: &mut CloudStateFile) -> bool {
    let before = serde_json::to_vec(&state.games.get(game_id)).unwrap_or_default();
    let (device_id, device_name) = device_identity(app)
        .unwrap_or_else(|_| ("unknown-device".to_string(), "This PC".to_string()));
    let record = state.games.entry(game_id.to_string()).or_default();
    if record.device_id.is_empty() {
        record.device_id = device_id;
    }
    if record.device_name.is_empty() {
        record.device_name = device_name;
    }

    let install = crate::platform::install_record(app, game_id).ok().flatten();
    if let Some(install) = install {
        match save_map::load_for_game(
            app,
            game_id,
            Path::new(&install.install_path),
            &install.version,
        ) {
            Ok(resolved) => {
                let legacy_expiry = Utc::now()
                    + chrono::Duration::days(resolved.migration.legacy_retention_days as i64);
                let mut active_roots = resolved
                    .roots
                    .into_iter()
                    .map(|root| CloudSaveRoot {
                        id: root.id,
                        path: root.path,
                        label: root.label,
                        purpose: root.purpose,
                        include: normalize_patterns(root.include),
                        exclude: normalize_patterns(root.exclude),
                        fingerprint: root.fingerprint,
                        legacy: false,
                        legacy_expires_at: None,
                    })
                    .collect::<Vec<_>>();
                let active_fingerprints = active_roots
                    .iter()
                    .map(|root| root.fingerprint.clone())
                    .filter(|value| !value.is_empty())
                    .collect::<HashSet<_>>();

                let mut legacy_roots = record
                    .save_roots
                    .clone()
                    .into_iter()
                    .filter(|root| {
                        root.fingerprint.is_empty()
                            || !active_fingerprints.contains(&root.fingerprint)
                    })
                    .filter_map(|mut root| {
                        if !root.legacy {
                            root.legacy = true;
                            root.legacy_expires_at = Some(legacy_expiry.to_rfc3339());
                        }
                        if legacy_root_still_valid(&root) {
                            Some(root)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                active_roots.append(&mut legacy_roots);
                record.save_roots = normalize_roots(active_roots);
                record.include = normalize_patterns(
                    record
                        .save_roots
                        .iter()
                        .flat_map(|root| root.include.clone())
                        .collect(),
                );
                record.exclude = normalize_patterns(
                    record
                        .save_roots
                        .iter()
                        .flat_map(|root| root.exclude.clone())
                        .collect(),
                );
                record.enabled = !record.save_roots.is_empty();
                record.automatic_protection = true;
                record.max_files = resolved.limits.max_files;
                record.max_total_bytes = resolved.limits.max_total_bytes;
                record.max_file_bytes = resolved.limits.max_file_bytes;
                record.settle_time_ms = resolved.stability.settle_time_ms;
                record.poll_interval_ms = resolved.stability.poll_interval_ms;
                record.max_stability_wait_ms = resolved.stability.max_wait_ms;
                record.map_status = CloudMapStatus {
                    version: resolved.map_version,
                    source: resolved.source,
                    healthy: true,
                    message: if resolved.warnings.is_empty() {
                        "Đường dẫn Save đã được xác minh tự động.".to_string()
                    } else {
                        "Cloud Save đang theo dõi cấu hình an toàn; một số vị trí chưa được game tạo.".to_string()
                    },
                    warnings: resolved.warnings,
                };
                if record.last_message.is_empty() || record.last_message.contains("disabled") {
                    record.last_message = if record.enabled {
                        "Bảo vệ tự động đã sẵn sàng. Launcher sẽ đồng bộ trước và sau khi chơi."
                            .to_string()
                    } else {
                        "Đang chờ game tạo file Save lần đầu.".to_string()
                    };
                }
                if record.cloud_state.is_empty() {
                    record.cloud_state = if google_drive::connected(app) {
                        "ready".to_string()
                    } else {
                        "auth_required".to_string()
                    };
                }
            }
            Err(error) => {
                record.save_roots.retain(legacy_root_still_valid);
                record.enabled = !record.save_roots.is_empty() && record.enabled;
                record.map_status = CloudMapStatus {
                    version: String::new(),
                    source: "built-in-fallback".to_string(),
                    healthy: false,
                    message: "Game này chưa có cấu hình Cloud Save đã xác minh.".to_string(),
                    warnings: vec![error],
                };
                if record.last_message.is_empty() {
                    record.last_message =
                        "Cloud Save chưa được bật vì chưa có đường dẫn Save an toàn cho game này."
                            .to_string();
                }
            }
        }
    } else {
        record.enabled = false;
        record.map_status = CloudMapStatus {
            version: String::new(),
            source: "local-only".to_string(),
            healthy: false,
            message: "Cloud Save chỉ áp dụng cho game được cài bằng chế độ local.".to_string(),
            warnings: Vec::new(),
        };
    }

    let after = serde_json::to_vec(&state.games.get(game_id)).unwrap_or_default();
    before != after
}

fn legacy_root_still_valid(root: &CloudSaveRoot) -> bool {
    if !root.legacy {
        return true;
    }
    root.legacy_expires_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|expiry| expiry.with_timezone(&Utc) > Utc::now())
        .unwrap_or(true)
}

fn device_identity(app: &AppHandle) -> Result<(String, String), String> {
    let root = configured_sync_root(app)?;
    let path = root.join("device-id.txt");
    let name = env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "This PC".to_string());
    if let Ok(existing) = fs::read_to_string(&path) {
        let value = existing.trim();
        if !value.is_empty() {
            return Ok((value.to_string(), name));
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(timestamp_id().as_bytes());
    let id = format!("{:x}", hasher.finalize());
    write_atomic_text(&path, &id)?;
    Ok((id, name))
}

fn write_atomic_text(path: &Path, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, value.as_bytes()).map_err(|error| error.to_string())?;
    replace_file_with_rollback(&temporary, path)
}

fn default_root_label(game_id: &str, index: usize, path: &str) -> String {
    if game_id == "007-first-light" {
        return match index {
            0 => "Steam userdata (3768760)".to_string(),
            1 => "GSE Saves (3768760)".to_string(),
            2 => "Game folder userdata (22202)".to_string(),
            _ => format!("Save location {}", index + 1),
        };
    }
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Save location {}", index + 1))
}

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(STATE_FILE))
}

fn load_state_unlocked(app: &AppHandle) -> Result<CloudStateFile, String> {
    let path = state_path(app)?;
    recover_file_backup(&path)?;
    if !path.exists() {
        return Ok(CloudStateFile::default());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut state: CloudStateFile =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    // Older files are migrated lazily through serde defaults and are written
    // back with the current schema on the next state mutation.
    state.schema_version = STATE_SCHEMA;
    Ok(state)
}

fn write_state_unlocked(app: &AppHandle, state: &CloudStateFile) -> Result<(), String> {
    let path = state_path(app)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    replace_file_with_rollback(&temporary, &path)
}

fn game_cloud_root(sync_root: &Path, game_id: &str) -> PathBuf {
    sync_root.join(CLOUD_FOLDER).join(sanitize_id(game_id))
}

fn sanitize_id(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown-game".to_string()
    } else {
        sanitized
    }
}

fn normalize_roots(roots: Vec<CloudSaveRoot>) -> Vec<CloudSaveRoot> {
    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter_map(|root| {
            let path = root.path.trim().to_string();
            if path.is_empty() || !seen.insert(path.to_ascii_lowercase()) {
                return None;
            }
            Some(CloudSaveRoot {
                id: if root.id.trim().is_empty() {
                    format!("manual-{}", seen.len())
                } else {
                    root.id.trim().to_string()
                },
                label: if root.label.trim().is_empty() {
                    Path::new(&path)
                        .file_name()
                        .map(|value| value.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Save folder".to_string())
                } else {
                    root.label.trim().to_string()
                },
                path,
                purpose: root.purpose,
                include: normalize_patterns(root.include),
                exclude: normalize_patterns(root.exclude),
                fingerprint: root.fingerprint,
                legacy: root.legacy,
                legacy_expires_at: root.legacy_expires_at,
            })
        })
        .collect()
}

fn normalize_patterns(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|pattern| pattern.trim().replace('\\', "/"))
        .filter(|pattern| !pattern.is_empty())
        .collect()
}

fn path_selected(path: &str, include: &[String], exclude: &[String]) -> bool {
    let included =
        include.is_empty() || include.iter().any(|pattern| wildcard_match(pattern, path));
    included && !exclude.iter().any(|pattern| wildcard_match(pattern, path))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut p, mut v, mut star, mut match_at) = (0_usize, 0_usize, None, 0_usize);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&value[v])) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            match_at = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            match_at += 1;
            v = match_at;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn parse_manifest_path(value: &str) -> Result<(usize, String), String> {
    let (root, relative) = value
        .split_once('/')
        .ok_or_else(|| "cloud manifest path is invalid".to_string())?;
    let index = root
        .strip_prefix("root-")
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| "cloud manifest root is invalid".to_string())?;
    validate_relative(relative)?;
    Ok((index, relative.to_string()))
}

fn normalize_relative_path(path: &Path) -> Result<String, String> {
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("unsafe cloud save relative path".to_string());
    }
    let value = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    validate_relative(&value)?;
    Ok(value)
}

fn validate_relative(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        Err("unsafe cloud save path".to_string())
    } else {
        Ok(())
    }
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative(relative)?;
    let joined = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(root.to_path_buf(), |path, part| path.join(part));
    if joined.starts_with(root) {
        Ok(joined)
    } else {
        Err("cloud save path escaped its root".to_string())
    }
}

fn secure_local_target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative(relative)?;
    if is_reparse_or_symlink(root)? {
        return Err(format!(
            "local save folder cannot be a symlink or reparse point: {}",
            root.display()
        ));
    }
    let parts = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        let is_file = index + 1 == parts.len();
        if current.exists() {
            if is_reparse_or_symlink(&current)? {
                return Err(format!(
                    "cloud save path contains a symlink or reparse point: {}",
                    current.display()
                ));
            }
        } else if !is_file {
            fs::create_dir(&current).map_err(|error| error.to_string())?;
        }
    }
    Ok(current)
}

fn sibling_path(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .ok_or_else(|| "cloud save target has no file name".to_string())?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{name}.{suffix}")))
}

fn copy_file_synced(source: &Path, target: &Path) -> Result<(), String> {
    if is_reparse_or_symlink(source)? {
        return Err(format!(
            "refusing to copy linked save file: {}",
            source.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut input = File::open(source).map_err(|error| error.to_string())?;
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(target)
        .map_err(|error| error.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())
}

fn copy_tree_safely(source: &Path, target: &Path) -> Result<(), String> {
    if is_reparse_or_symlink(source)? {
        return Err(format!(
            "refusing to copy linked folder: {}",
            source.display()
        ));
    }
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    let mut walker = WalkDir::new(source).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry.map_err(|error| error.to_string())?;
        if is_reparse_or_symlink(entry.path())? {
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            copy_file_synced(entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn write_manifest(path: &Path, manifest: &CloudManifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    replace_file_with_rollback(&temporary, path)
}

fn read_manifest(path: &Path) -> Result<CloudManifest, String> {
    recover_file_backup(path)?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn backup_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .map(|value| format!("{}.bak", value.to_string_lossy()))
        .unwrap_or_else(|| "bak".to_string());
    path.with_extension(extension)
}

fn recover_file_backup(path: &Path) -> Result<(), String> {
    let backup = backup_path(path);
    if !path.exists() && backup.exists() {
        fs::rename(backup, path).map_err(|error| error.to_string())?;
    } else if path.exists() && backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn replace_file_with_rollback(temporary: &Path, destination: &Path) -> Result<(), String> {
    let backup = backup_path(destination);
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(temporary, destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, destination);
        }
        return Err(error.to_string());
    }
    if backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn rollback_files(committed: &[(PathBuf, PathBuf, bool, bool)]) {
    for (target, backup, had_original, installed_new) in committed.iter().rev() {
        if *installed_new && target.exists() {
            let _ = fs::remove_file(target);
        }
        if *had_original && backup.exists() {
            let _ = fs::rename(backup, target);
        }
    }
}

fn clear_tree_safely(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = Vec::new();
    let mut walker = WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_dir() && is_reparse_or_symlink(entry.path())? {
            walker.skip_current_dir();
        }
        entries.push(entry);
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.depth()));
    for entry in entries {
        if entry.file_type().is_dir() {
            fs::remove_dir(entry.path()).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    fs::remove_dir(root).map_err(|error| error.to_string())
}

fn timestamp_id() -> String {
    format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S"),
        Utc::now().timestamp_millis().unsigned_abs()
    )
}

fn is_reparse_or_symlink(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return Ok(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0);
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(hash: &str) -> CloudManifest {
        CloudManifest {
            generated_at: String::new(),
            files: vec![CloudFileEntry {
                path: "root-0/save.dat".to_string(),
                size: 4,
                modified_at_ms: 1,
                blake3: hash.to_string(),
                sha256: String::new(),
            }],
        }
    }

    #[test]
    fn wildcard_filters_are_case_insensitive() {
        assert!(wildcard_match(
            "Save/*/profile?.dat",
            "save/slot/profile1.dat"
        ));
        assert!(!wildcard_match("*.sav", "profile.dat"));
    }

    #[test]
    fn manifest_path_rejects_parent_traversal() {
        assert!(parse_manifest_path("root-0/../secret.dat").is_err());
        assert!(parse_manifest_path("root-2/save/profile.dat").is_ok());
    }

    #[test]
    fn manifest_equality_ignores_mtime_only_changes() {
        let left = manifest_with("hash");
        let mut right = left.clone();
        right.files[0].modified_at_ms = 99;
        assert!(manifests_equal(&left, &right));
    }

    #[test]
    fn sync_decision_covers_initial_and_three_way_cases() {
        let empty = CloudManifest::default();
        let baseline = manifest_with("base");
        let local = manifest_with("local");
        let cloud = manifest_with("cloud");

        assert_eq!(
            determine_sync_action(SyncDirection::Auto, false, &empty, &local, &empty),
            SyncAction::Push
        );
        assert_eq!(
            determine_sync_action(SyncDirection::Auto, false, &empty, &empty, &cloud),
            SyncAction::Pull
        );
        assert_eq!(
            determine_sync_action(SyncDirection::Auto, true, &baseline, &baseline, &baseline),
            SyncAction::Noop
        );
        assert_eq!(
            determine_sync_action(SyncDirection::Auto, true, &baseline, &local, &baseline),
            SyncAction::Push
        );
        assert_eq!(
            determine_sync_action(SyncDirection::Auto, true, &baseline, &baseline, &cloud),
            SyncAction::Pull
        );
        assert_eq!(
            determine_sync_action(SyncDirection::Auto, true, &baseline, &local, &cloud),
            SyncAction::Conflict
        );
    }

    #[test]
    fn pull_replaces_changed_files_and_removes_cloud_deletions() {
        let test_root = env::temp_dir().join(format!("0xo-cloud-test-{}", timestamp_id()));
        let local_root = test_root.join("local");
        let current_root = test_root.join("provider").join("current");
        fs::create_dir_all(&local_root).unwrap();
        fs::create_dir_all(current_root.join("files").join("root-0")).unwrap();
        fs::write(local_root.join("keep.dat"), b"old").unwrap();
        fs::write(local_root.join("removed.dat"), b"remove-me").unwrap();
        fs::write(
            current_root.join("files").join("root-0").join("keep.dat"),
            b"new",
        )
        .unwrap();

        let roots = vec![local_root.clone()];
        let local_manifest = scan_local_roots(&roots, &[], &[]).unwrap();
        let cloud_manifest = scan_cloud_current(&current_root).unwrap();
        apply_cloud_manifest_to_local(
            &roots,
            &current_root.join("files"),
            &local_manifest,
            &cloud_manifest,
        )
        .unwrap();

        assert_eq!(fs::read(local_root.join("keep.dat")).unwrap(), b"new");
        assert!(!local_root.join("removed.dat").exists());
        clear_tree_safely(&test_root).unwrap();
    }

    #[test]
    fn interrupted_atomic_file_replace_recovers_backup() {
        let test_root = env::temp_dir().join(format!("0xo-cloud-backup-{}", timestamp_id()));
        fs::create_dir_all(&test_root).unwrap();
        let destination = test_root.join("manifest.json");
        let backup = backup_path(&destination);
        fs::write(&backup, b"recover").unwrap();

        recover_file_backup(&destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"recover");
        assert!(!backup.exists());
        clear_tree_safely(&test_root).unwrap();
    }

    #[test]
    fn steam_userdata_template_uses_the_active_account_root() {
        let expanded = expand_path_template(
            r"{steamUserData}\3768760\remote",
            "007-first-light",
            r"E:\Games\007 First Light",
            Some(r"C:\Program Files (x86)\Steam\userdata\123456"),
        )
        .unwrap();
        assert_eq!(
            expanded,
            vec![PathBuf::from(
                r"C:\Program Files (x86)\Steam\userdata\123456\0\3768760\remote"
            )]
        );
    }

    #[test]
    fn save_roots_keep_identical_file_names_in_separate_namespaces() {
        let test_root = env::temp_dir().join(format!("0xo-cloud-roots-{}", timestamp_id()));
        let first = test_root.join("first");
        let second = test_root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("profile.sav"), b"steam").unwrap();
        fs::write(second.join("profile.sav"), b"gse").unwrap();

        let manifest = scan_local_roots(&[first, second], &[], &[]).unwrap();
        let paths = manifest
            .files
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["root-0/profile.sav", "root-1/profile.sav"]);
        clear_tree_safely(&test_root).unwrap();
    }
}
