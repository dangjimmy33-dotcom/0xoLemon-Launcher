use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

mod google_drive;

const STATE_SCHEMA: u32 = 1;
const STATE_FILE: &str = "cloud-save-state.json";
const CLOUD_FOLDER: &str = "0xoLemon Cloud Saves";
const MAX_SNAPSHOTS: usize = 5;
const FILE_STABILITY_DELAY: Duration = Duration::from_secs(2);

static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNNING_GAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn state_lock() -> &'static Mutex<()> {
    STATE_LOCK.get_or_init(|| Mutex::new(()))
}

fn running_games() -> &'static Mutex<HashSet<String>> {
    RUNNING_GAMES.get_or_init(|| Mutex::new(HashSet::new()))
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
    pub path: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudFileEntry {
    path: String,
    size: u64,
    modified_at_ms: u64,
    blake3: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSnapshotSummary {
    pub id: String,
    pub created_at: String,
    pub source: String,
    pub file_count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameCloudRecord {
    enabled: bool,
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
    if let Err(error) = restore_missing_from_google_drive(app, game_id) {
        let _ = app.emit(
            "launcher://cloud-save-error",
            CloudSaveErrorEvent {
                game_id: game_id.to_string(),
                message: format!(
                    "Google Drive restore check failed; launch will continue without it: {error}"
                ),
            },
        );
    }
    let status = sync_game(app, game_id, SyncDirection::Auto)?;
    if !status.conflicts.is_empty() {
        return Err(format!(
            "CLOUD_SAVE_CONFLICT:{} cloud save conflict(s) require a choice",
            status.conflicts.len()
        ));
    }
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
    ensure_not_running(game_id)?;
    let record = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let mut state = load_state_unlocked(app)?;
        if seed_metadata_defaults(app, game_id, &mut state) {
            write_state_unlocked(app, &state)?;
        }
        state
            .games
            .get(game_id)
            .cloned()
            .ok_or_else(|| "cloud save metadata is unavailable for this game".to_string())?
    };
    if !google_drive::connected(app) {
        google_drive::authorize(app)?;
    }
    google_drive::backup(app, game_id, &record)?;
    let now = Utc::now().to_rfc3339();
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    let record = state.games.entry(game_id.to_string()).or_default();
    record.google_drive_last_backup_at = Some(now);
    record.google_drive_message = "Save files backed up to Google Drive.".to_string();
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn restore_missing_from_google_drive(
    app: &AppHandle,
    game_id: &str,
) -> Result<CloudSaveStatus, String> {
    ensure_not_running(game_id)?;
    let record = {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let mut state = load_state_unlocked(app)?;
        if seed_metadata_defaults(app, game_id, &mut state) {
            write_state_unlocked(app, &state)?;
        }
        state
            .games
            .get(game_id)
            .cloned()
            .ok_or_else(|| "cloud save metadata is unavailable for this game".to_string())?
    };
    let restored = google_drive::restore_missing(app, game_id, &record)?;
    let message = if restored == 0 {
        "Google Drive backup check found no missing save files.".to_string()
    } else {
        format!("Restored {restored} missing save file(s) from Google Drive.")
    };
    update_google_message(app, game_id, &message, Some(restored))
}

pub fn start_google_drive_restore_monitor(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(30));
        loop {
            if google_drive::connected(&app) {
                if let Ok(installs) = crate::platform::install_records(&app) {
                    for install in installs {
                        if ensure_not_running(&install.game_id).is_err() {
                            continue;
                        }
                        let record = {
                            let Ok(_guard) = state_lock().lock() else {
                                continue;
                            };
                            let Ok(mut state) = load_state_unlocked(&app) else {
                                continue;
                            };
                            if seed_metadata_defaults(&app, &install.game_id, &mut state) {
                                let _ = write_state_unlocked(&app, &state);
                            }
                            state.games.get(&install.game_id).cloned()
                        };
                        let Some(record) = record else {
                            continue;
                        };
                        match google_drive::restore_missing(&app, &install.game_id, &record) {
                            Ok(restored) if restored > 0 => {
                                let message = format!(
                                    "Restored {restored} missing save file(s) from Google Drive."
                                );
                                if let Ok(status) = update_google_message(
                                    &app,
                                    &install.game_id,
                                    &message,
                                    Some(restored),
                                ) {
                                    let _ = app.emit(
                                        "launcher://cloud-save",
                                        CloudSaveEvent {
                                            game_id: install.game_id,
                                            status,
                                        },
                                    );
                                }
                            }
                            Ok(_) => {}
                            Err(error) => {
                                let _ = app.emit(
                                    "launcher://cloud-save-error",
                                    CloudSaveErrorEvent {
                                        game_id: install.game_id,
                                        message: error,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            thread::sleep(Duration::from_secs(10 * 60));
        }
    });
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
            let _ = app.emit(
                "launcher://cloud-save-error",
                CloudSaveErrorEvent {
                    game_id,
                    message: error,
                },
            );
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
    let mut previous = scan_local_roots(&roots, &record.include, &record.exclude)?;
    for _ in 0..5 {
        thread::sleep(FILE_STABILITY_DELAY);
        let current = scan_local_roots(&roots, &record.include, &record.exclude)?;
        if manifests_equal(&previous, &current) {
            return Ok(());
        }
        previous = current;
    }
    Err("save files did not become stable after the game exited".to_string())
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
    let root = configured_sync_root()?;
    {
        let _guard = state_lock()
            .lock()
            .map_err(|_| "cloud save state lock poisoned".to_string())?;
        let state = load_state_unlocked(app)?;
        let record = state
            .games
            .get(game_id)
            .ok_or_else(|| "cloud save is not configured for this game".to_string())?;
        if !record
            .conflicts
            .iter()
            .any(|conflict| conflict.id == conflict_id)
        {
            return Err("cloud save conflict not found".to_string());
        }
    }
    sync_game(app, game_id, direction)?;

    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app)?;
    let record = state.games.entry(game_id.to_string()).or_default();
    record
        .conflicts
        .retain(|conflict| conflict.id != conflict_id);
    let conflict_root = game_cloud_root(&root, game_id)
        .join("conflicts")
        .join(conflict_id);
    if conflict_root.exists() {
        clear_tree_safely(&conflict_root)?;
    }
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

pub fn restore_snapshot(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    ensure_not_running(game_id)?;
    let mut state = load_state_unlocked(app)?;
    let record = state
        .games
        .get_mut(game_id)
        .ok_or_else(|| "cloud save is not configured for this game".to_string())?;
    let root = configured_sync_root()?;
    let expanded_roots = expanded_save_roots(app, game_id, record)?;
    let snapshot = record
        .snapshots
        .iter()
        .find(|snapshot| snapshot.id == snapshot_id)
        .cloned()
        .ok_or_else(|| "cloud save snapshot not found".to_string())?;
    let snapshot_root = game_cloud_root(&root, game_id)
        .join("snapshots")
        .join(&snapshot.id);
    let manifest = read_manifest(&snapshot_root.join("manifest.json"))?;
    validate_root_layout(&root, game_id, &expanded_roots)?;
    let current_local = scan_local_roots(&expanded_roots, &record.include, &record.exclude)?;
    let game_root = game_cloud_root(&root, game_id);
    let current_cloud = scan_cloud_current(&game_root.join("current"))?;
    snapshot_cloud_current(&game_root, record, &current_cloud, "cloud")?;
    apply_cloud_manifest_to_local(
        &expanded_roots,
        &snapshot_root.join("files"),
        &current_local,
        &manifest,
    )?;
    commit_local_to_cloud(&game_root, &expanded_roots, &manifest)?;
    record.baseline = Some(manifest);
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
    record.last_message = format!("Restored snapshot {}.", snapshot.id);
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
}

fn sync_game(
    app: &AppHandle,
    game_id: &str,
    direction: SyncDirection,
) -> Result<CloudSaveStatus, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "cloud save state lock poisoned".to_string())?;
    ensure_not_running(game_id)?;
    let mut state = load_state_unlocked(app)?;
    let record = state.games.entry(game_id.to_string()).or_default();
    if !record.enabled {
        return Ok(build_status(app, game_id, Some(record)));
    }
    let root = configured_sync_root()?;
    let expanded_roots = expanded_save_roots(app, game_id, record)?;
    if expanded_roots.is_empty() {
        return Err("cloud save has no valid local save folders".to_string());
    }
    validate_root_layout(&root, game_id, &expanded_roots)?;

    let local_manifest = scan_local_roots(&expanded_roots, &record.include, &record.exclude)?;
    let game_root = game_cloud_root(&root, game_id);
    let current_root = game_root.join("current");
    let cloud_manifest = scan_cloud_current(&current_root)?;
    let baseline = record.baseline.clone().unwrap_or_default();
    let action = determine_sync_action(
        direction,
        record.baseline.is_some(),
        &baseline,
        &local_manifest,
        &cloud_manifest,
    );

    match action {
        SyncAction::Push => {
            snapshot_cloud_current(&game_root, record, &cloud_manifest, "cloud")?;
            commit_local_to_cloud(&game_root, &expanded_roots, &local_manifest)?;
            record.baseline = Some(local_manifest);
            record.last_message = "Uploaded local saves to the sync folder.".to_string();
        }
        SyncAction::Pull => {
            snapshot_local(&game_root, record, &expanded_roots, &local_manifest)?;
            apply_cloud_manifest_to_local(
                &expanded_roots,
                &current_root.join("files"),
                &local_manifest,
                &cloud_manifest,
            )?;
            record.baseline = Some(cloud_manifest);
            record.last_message = "Downloaded cloud saves to local folders.".to_string();
        }
        SyncAction::Conflict => {
            create_conflict(
                &game_root,
                record,
                &expanded_roots,
                &local_manifest,
                &current_root,
                &cloud_manifest,
            )?;
            record.last_message =
                "Local and cloud saves both changed. No files were overwritten.".to_string();
        }
        SyncAction::Baseline => {
            record.baseline = Some(local_manifest);
            record.last_message = "Cloud save baseline initialized.".to_string();
        }
        SyncAction::Noop => {
            record.last_message = "Cloud saves are already up to date.".to_string();
        }
    }
    record.last_sync_at = Some(Utc::now().to_rfc3339());
    trim_snapshots(&game_root, record)?;
    write_state_unlocked(app, &state)?;
    Ok(build_status(app, game_id, state.games.get(game_id)))
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

fn configured_sync_root() -> Result<PathBuf, String> {
    let value = crate::platform::current_settings().cloud_save_root;
    if value.trim().is_empty() {
        return Err("choose a Cloud Save root in Settings first".to_string());
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("Cloud Save root must be an absolute path".to_string());
    }
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
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(CloudFileEntry {
        path: relative,
        size: metadata.len(),
        modified_at_ms,
        blake3: hasher.finalize().to_hex().to_string(),
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
    record.conflicts.push(CloudConflictSummary {
        id,
        created_at: Utc::now().to_rfc3339(),
        local_file_count: local.files.len(),
        cloud_file_count: cloud.files.len(),
        local_bytes: manifest_bytes(local),
        cloud_bytes: manifest_bytes(cloud),
    });
    Ok(())
}

fn trim_snapshots(game_root: &Path, record: &mut GameCloudRecord) -> Result<(), String> {
    record
        .snapshots
        .sort_by(|left, right| right.created_at.cmp(&left.created_at));
    while record.snapshots.len() > MAX_SNAPSHOTS {
        if let Some(snapshot) = record.snapshots.pop() {
            let path = game_root.join("snapshots").join(snapshot.id);
            if path.exists() {
                clear_tree_safely(&path)?;
            }
        }
    }
    Ok(())
}

fn snapshot_summary(id: &str, source: &str, manifest: &CloudManifest) -> CloudSnapshotSummary {
    CloudSnapshotSummary {
        id: id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        source: source.to_string(),
        file_count: manifest.files.len(),
        bytes: manifest_bytes(manifest),
    }
}

fn manifest_bytes(manifest: &CloudManifest) -> u64 {
    manifest.files.iter().map(|entry| entry.size).sum()
}

fn build_status(
    _app: &AppHandle,
    game_id: &str,
    record: Option<&GameCloudRecord>,
) -> CloudSaveStatus {
    let settings = crate::platform::current_settings();
    let record = record.cloned().unwrap_or_default();
    let game_running = running_games()
        .lock()
        .map(|games| games.contains(game_id))
        .unwrap_or(false);
    let state = if !record.conflicts.is_empty() {
        "conflict"
    } else if record.enabled {
        "ready"
    } else {
        "disabled"
    };
    CloudSaveStatus {
        game_id: game_id.to_string(),
        enabled: record.enabled,
        sync_root: settings.cloud_save_root.clone(),
        save_roots: record.save_roots,
        include: record.include,
        exclude: record.exclude,
        state: state.to_string(),
        last_sync_at: record.last_sync_at,
        last_message: record.last_message,
        conflicts: record.conflicts,
        snapshots: record.snapshots,
        can_sync: !settings.cloud_save_root.is_empty() && !game_running,
        game_running,
        google_drive_configured: google_drive::client_configured(),
        google_drive_connected: google_drive::connected(_app),
        google_drive_last_backup_at: record.google_drive_last_backup_at,
        google_drive_last_restore_count: record.google_drive_last_restore_count,
        google_drive_message: record.google_drive_message,
    }
}

fn seed_metadata_defaults(app: &AppHandle, game_id: &str, state: &mut CloudStateFile) -> bool {
    let mut metadata = crate::asset_pack::get_game_detail(app, game_id, None)
        .map(|detail| detail.cloud_save)
        .unwrap_or_default();

    if let Ok(Some(install_record)) = crate::platform::install_record(app, game_id) {
        let override_path =
            std::path::PathBuf::from(install_record.install_path).join("0xo-save.json");
        if override_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&override_path) {
                if let Ok(override_meta) = serde_json::from_str::<CloudSaveMetadata>(&content) {
                    for root in override_meta.save_roots {
                        if !metadata.save_roots.contains(&root) {
                            metadata.save_roots.push(root);
                        }
                    }
                    for inc in override_meta.include {
                        if !metadata.include.contains(&inc) {
                            metadata.include.push(inc);
                        }
                    }
                    for exc in override_meta.exclude {
                        if !metadata.exclude.contains(&exc) {
                            metadata.exclude.push(exc);
                        }
                    }
                }
            }
        }
    }
    let metadata_roots = metadata
        .save_roots
        .into_iter()
        .enumerate()
        .map(|(index, path)| CloudSaveRoot {
            label: default_root_label(game_id, index, &path),
            path,
        })
        .collect::<Vec<_>>();

    if let Some(record) = state.games.get_mut(game_id) {
        let mut changed = false;
        if record.save_roots.is_empty() && !metadata_roots.is_empty() {
            record.save_roots = metadata_roots;
            changed = true;
        }
        if record.include.is_empty() && !metadata.include.is_empty() {
            record.include = metadata.include;
            changed = true;
        }
        if record.exclude.is_empty() && !metadata.exclude.is_empty() {
            record.exclude = metadata.exclude;
            changed = true;
        }
        return changed;
    }

    state.games.insert(
        game_id.to_string(),
        GameCloudRecord {
            enabled: false,
            save_roots: metadata_roots,
            include: metadata.include,
            exclude: metadata.exclude,
            last_message: "Cloud save is disabled.".to_string(),
            ..GameCloudRecord::default()
        },
    );
    true
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
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
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
                label: if root.label.trim().is_empty() {
                    Path::new(&path)
                        .file_name()
                        .map(|value| value.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Save folder".to_string())
                } else {
                    root.label.trim().to_string()
                },
                path,
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
