/// Local save-game backup module.
///
/// When a game exits, this module:
///   1. Resolves the correct save-file directory for the installed game version
///      using the rules in `save_paths`.
///   2. Copies all save files into a timestamped snapshot under
///      `%LOCALAPPDATA%\0xoLemon\SaveBackups\<game_id>\<timestamp>\`
///   3. Prunes old snapshots so at most `MAX_SNAPSHOTS` are kept.
///   4. Emits `launcher://save-backup-progress` events at every stage.
///   5. Leaves Google Drive synchronization to the transactional Cloud Save engine.
///
/// The backup directory is intentionally in %LOCALAPPDATA% so it:
///   - Persists across launcher reinstalls
///   - Persists across game reinstalls
///   - Is not in the cloud-save sync root
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::managed_file_transaction::{
    self, ManagedDeleteSpec, ManagedFileChange, ManagedFileSpec,
};
use crate::save_paths::{resolve_save_roots, ResolvedSaveRoot, SaveProviderKind};

const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const MAX_SNAPSHOTS: usize = 10;
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const BACKUP_ROOT_NAME: &str = "0xoLemon\\SaveBackups";
const MANIFEST_FILE: &str = "backup-manifest.json";
const PENDING_SUFFIX: &str = ".pending";
const FILE_STABILITY_DELAY: Duration = Duration::from_millis(750);
const FILE_STABILITY_ATTEMPTS: usize = 4;

// ─── Global backup-in-progress flag (used by close guard) ───────────────────

static ACTIVE_OPERATIONS: AtomicUsize = AtomicUsize::new(0);
static OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct ActiveOperationGuard;

impl ActiveOperationGuard {
    fn enter() -> Self {
        ACTIVE_OPERATIONS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for ActiveOperationGuard {
    fn drop(&mut self) {
        ACTIVE_OPERATIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn operation_guard() -> Result<MutexGuard<'static, ()>, String> {
    OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "save backup operation lock is poisoned".to_string())
}

/// Returns true when a save backup (local or GDrive) is currently running.
pub fn is_backup_in_progress() -> bool {
    ACTIVE_OPERATIONS.load(Ordering::Acquire) > 0
}

// ─── Events ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackupProgressEvent {
    pub game_id: String,
    /// "starting" | "copying" | "uploading" | "done" | "error" | "skipped"
    pub state: String,
    pub message: String,
    pub files_copied: usize,
    pub bytes_copied: u64,
    pub snapshot_id: Option<String>,
}

fn emit_progress(app: &AppHandle, ev: SaveBackupProgressEvent) {
    let _ = app.emit("launcher://save-backup-progress", ev);
}

// ─── Snapshot manifest ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackupEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SaveSnapshotPurpose {
    Automatic,
    PreRestore,
}

impl Default for SaveSnapshotPurpose {
    fn default() -> Self {
        Self::Automatic
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSnapshotRoot {
    pub index: usize,
    pub provider: SaveProviderKind,
    pub provider_save_id: String,
    pub original_path: String,
    pub root_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSnapshotV2 {
    #[serde(default)]
    pub schema_version: u32,
    pub id: String,
    pub game_id: String,
    pub game_version: String,
    #[serde(default)]
    pub runtime_version: Option<String>,
    #[serde(default)]
    pub app_id: Option<u32>,
    pub created_at: String,
    #[serde(default)]
    pub purpose: SaveSnapshotPurpose,
    #[serde(default)]
    pub roots: Vec<SaveSnapshotRoot>,
    #[serde(default)]
    pub source_paths: Vec<String>,
    pub files: Vec<SaveBackupEntry>,
    pub total_bytes: u64,
}

pub type SaveBackupSnapshot = SaveSnapshotV2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreTransaction {
    pub transaction_id: String,
    pub game_id: String,
    pub snapshot_id: String,
    pub pre_restore_snapshot_id: String,
    pub files_replaced: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreAndRelaunchResult {
    pub restore: RestoreTransaction,
    pub launch: Option<crate::job::LaunchReport>,
    pub launch_error: Option<String>,
    pub cloud_policy_state: String,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Runs after game exit without blocking the process watcher. Backup and restore
/// share one operation lock so a snapshot can never observe a half-restored save.
pub fn backup_after_exit(
    app: &AppHandle,
    game_id: &str,
    game_version: &str,
    app_id: Option<u32>,
) -> Result<(), String> {
    let _active = ActiveOperationGuard::enter();
    let result = operation_guard()
        .and_then(|_serial| do_backup(app, game_id, game_version, app_id));
    let _ = app.emit(
        "launcher://save-backup-guard-released",
        serde_json::json!({ "gameId": game_id }),
    );
    result
}

pub fn backup_after_exit_async(
    app: AppHandle,
    game_id: String,
    game_version: String,
    app_id: Option<u32>,
) {
    std::thread::spawn(move || {
        let result = backup_after_exit(&app, &game_id, &game_version, app_id);

        if let Err(error) = result {
            emit_progress(
                &app,
                SaveBackupProgressEvent {
                    game_id,
                    state: "error".to_string(),
                    message: error,
                    files_copied: 0,
                    bytes_copied: 0,
                    snapshot_id: None,
                },
            );
        }
    });
}

/// List all saved snapshots for a game, newest first.
pub fn list_snapshots(game_id: &str) -> Result<Vec<SaveBackupSnapshot>, String> {
    let backup_dir = game_backup_dir(game_id)?;
    if !backup_dir.exists() {
        return Ok(vec![]);
    }
    let mut snapshots: Vec<SaveBackupSnapshot> = fs::read_dir(&backup_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().ends_with(PENDING_SUFFIX))
        .filter_map(|entry| {
            let manifest = entry.path().join(MANIFEST_FILE);
            let data = fs::read_to_string(&manifest).ok()?;
            serde_json::from_str::<SaveBackupSnapshot>(&data).ok()
        })
        .collect();
    // Newest first
    snapshots.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(snapshots)
}

/// Restores a snapshot through the shared managed-file transaction. Existing
/// saves are snapshotted first; a failed commit restores the exact old files.
pub fn restore_snapshot(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
) -> Result<RestoreTransaction, String> {
    let _active = ActiveOperationGuard::enter();
    let _serial = operation_guard()?;
    restore_snapshot_locked(app, game_id, snapshot_id)
}

fn restore_snapshot_locked(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
) -> Result<RestoreTransaction, String> {
    if crate::cloud_save::is_game_running(game_id) {
        return Err("Close the game before restoring a local save snapshot".to_string());
    }

    validate_snapshot_id(snapshot_id)?;
    let backup_dir = game_backup_dir(game_id)?.join(snapshot_id);
    let snapshot = load_snapshot(&backup_dir)?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "snapshot {} uses unsupported schema version {}",
            snapshot.id, snapshot.schema_version
        ));
    }
    if snapshot.game_id != game_id {
        return Err("snapshot belongs to a different game".to_string());
    }

    let roots = snapshot_roots_to_resolved(&snapshot)?;
    verify_snapshot_files(&backup_dir, &snapshot)?;
    for root in &roots {
        ensure_save_root_exists(root)?;
    }

    // This snapshot is intentionally retained. It lets the user undo a valid
    // restore in addition to the transaction's automatic failure rollback.
    let pre_restore = create_snapshot_from_roots(
        game_id,
        &snapshot.game_version,
        snapshot.app_id,
        &roots,
        SaveSnapshotPurpose::PreRestore,
    )?;

    let mut expected_by_root: HashMap<usize, HashSet<PathBuf>> = HashMap::new();
    let mut changes = Vec::new();
    for entry in &snapshot.files {
        let (root_index, relative) = parse_snapshot_entry_path(&entry.relative_path)?;
        let root = roots
            .iter()
            .find(|candidate| candidate.index == root_index)
            .ok_or_else(|| format!("snapshot root index {root_index} is unavailable"))?;
        expected_by_root
            .entry(root_index)
            .or_default()
            .insert(relative.clone());
        changes.push(ManagedFileChange::Replace(ManagedFileSpec {
            source: backup_dir.join(&entry.relative_path),
            target: root.path.join(&relative),
            allowed_source_root: backup_dir.clone(),
            allowed_target_root: root.path.clone(),
            expected_sha256: entry.sha256.clone(),
        }));
    }

    for root in &roots {
        let expected = expected_by_root.get(&root.index);
        for (relative, current) in collect_current_files(&root.path)? {
            if expected.is_none_or(|files| !files.contains(&relative)) {
                changes.push(ManagedFileChange::Delete(ManagedDeleteSpec {
                    target: current,
                    allowed_target_root: root.path.clone(),
                }));
            }
        }
    }

    let receipt = if changes.is_empty() {
        None
    } else {
        Some(managed_file_transaction::apply_changes(
            app,
            "restore-local-save",
            changes,
        )?)
    };
    verify_restored_snapshot(&roots, &snapshot)?;
    prune_snapshots(game_id)?;
    Ok(RestoreTransaction {
        transaction_id: receipt
            .as_ref()
            .map(|value| value.transaction_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        game_id: game_id.to_string(),
        snapshot_id: snapshot.id,
        pre_restore_snapshot_id: pre_restore.id,
        files_replaced: receipt.map(|value| value.files_replaced).unwrap_or(0),
        status: "committed".to_string(),
    })
}

pub fn restore_and_relaunch(
    app: &AppHandle,
    game_id: &str,
    snapshot_id: &str,
    install_path: &Path,
    launch_executable: Option<String>,
    launch_option_id: Option<String>,
) -> Result<RestoreAndRelaunchResult, String> {
    let _active = ActiveOperationGuard::enter();
    let _serial = operation_guard()?;
    let restore = restore_snapshot_locked(app, game_id, snapshot_id)?;

    if let Err(error) = crate::cloud_save::prepare_local_wins_once(
        app,
        game_id,
        &restore.snapshot_id,
        &restore.transaction_id,
    ) {
        return Ok(RestoreAndRelaunchResult {
            restore,
            launch: None,
            launch_error: Some(format!(
                "Save was restored, but localWinsOnce could not be prepared: {error}"
            )),
            cloud_policy_state: "blocked".to_string(),
        });
    }

    match crate::job::launch_game(
        app,
        game_id,
        install_path,
        launch_executable,
        launch_option_id,
        true,
    ) {
        Ok(launch) => Ok(RestoreAndRelaunchResult {
            restore,
            launch: Some(launch),
            launch_error: None,
            cloud_policy_state: "localWinsOnceRunning".to_string(),
        }),
        Err(error) => Ok(RestoreAndRelaunchResult {
            restore,
            launch: None,
            launch_error: Some(error.to_string()),
            cloud_policy_state: "localWinsOncePrepared".to_string(),
        }),
    }
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn do_backup(
    app: &AppHandle,
    game_id: &str,
    game_version: &str,
    app_id: Option<u32>,
) -> Result<(), String> {
    let source_roots = resolve_save_roots(game_id, game_version, app_id);
    if source_roots.is_empty() {
        emit_progress(
            app,
            SaveBackupProgressEvent {
                game_id: game_id.to_string(),
                state: "skipped".to_string(),
                message: format!("No save path for {} @ {}", game_id, game_version),
                files_copied: 0,
                bytes_copied: 0,
                snapshot_id: None,
            },
        );
        return Ok(());
    }

    let existing_roots: Vec<ResolvedSaveRoot> = source_roots
        .into_iter()
        .filter(|root| root.path.is_dir())
        .enumerate()
        .map(|(index, mut root)| {
            root.index = index;
            root
        })
        .collect();
    if existing_roots.is_empty() {
        emit_progress(
            app,
            SaveBackupProgressEvent {
                game_id: game_id.to_string(),
                state: "skipped".to_string(),
                message: "Save folder does not exist yet (no saves?)".to_string(),
                files_copied: 0,
                bytes_copied: 0,
                snapshot_id: None,
            },
        );
        return Ok(());
    }

    emit_progress(
        app,
        SaveBackupProgressEvent {
            game_id: game_id.to_string(),
            state: "starting".to_string(),
            message: "Starting save backup…".to_string(),
            files_copied: 0,
            bytes_copied: 0,
            snapshot_id: None,
        },
    );

    let snapshot_id = next_snapshot_id();
    emit_progress(
        app,
        SaveBackupProgressEvent {
            game_id: game_id.to_string(),
            state: "copying".to_string(),
            message: "Copying save files…".to_string(),
            files_copied: 0,
            bytes_copied: 0,
            snapshot_id: Some(snapshot_id.clone()),
        },
    );

    let snapshot = create_snapshot_from_roots_with_id(
        game_id,
        game_version,
        app_id,
        &existing_roots,
        SaveSnapshotPurpose::Automatic,
        snapshot_id.clone(),
    )?;
    prune_snapshots(game_id)?;

    emit_progress(
        app,
        SaveBackupProgressEvent {
            game_id: game_id.to_string(),
            state: "uploading".to_string(),
            message: format!(
                "Local backup done ({} files). Uploading to Google Drive…",
                snapshot.files.len()
            ),
            files_copied: snapshot.files.len(),
            bytes_copied: snapshot.total_bytes,
            snapshot_id: Some(snapshot_id.clone()),
        },
    );

    Ok(())
}

fn game_backup_dir(game_id: &str) -> Result<PathBuf, String> {
    let local_app_data =
        std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA env var not set".to_string())?;
    let safe_id: String = game_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(PathBuf::from(local_app_data)
        .join(BACKUP_ROOT_NAME)
        .join(safe_id))
}

fn next_snapshot_id() -> String {
    let dt = chrono::DateTime::<chrono::Utc>::from(SystemTime::now());
    let suffix = Uuid::new_v4().simple().to_string();
    format!("{}-{}", dt.format("%Y-%m-%dT%H-%M-%S"), &suffix[..8])
}

fn prune_snapshots(game_id: &str) -> Result<(), String> {
    let backup_dir = game_backup_dir(game_id)?;
    if !backup_dir.exists() {
        return Ok(());
    }
    let mut dirs: Vec<PathBuf> = fs::read_dir(&backup_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|entry| {
            entry.path().is_dir()
                && !entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(PENDING_SUFFIX)
                && entry.path().join(MANIFEST_FILE).is_file()
        })
        .map(|e| e.path())
        .collect();
    dirs.sort();
    while dirs.len() > MAX_SNAPSHOTS {
        let oldest = dirs.remove(0);
        remove_published_snapshot_exact(&oldest)?;
    }
    Ok(())
}

fn create_snapshot_from_roots(
    game_id: &str,
    game_version: &str,
    app_id: Option<u32>,
    roots: &[ResolvedSaveRoot],
    purpose: SaveSnapshotPurpose,
) -> Result<SaveBackupSnapshot, String> {
    create_snapshot_from_roots_with_id(
        game_id,
        game_version,
        app_id,
        roots,
        purpose,
        next_snapshot_id(),
    )
}

fn create_snapshot_from_roots_with_id(
    game_id: &str,
    game_version: &str,
    app_id: Option<u32>,
    roots: &[ResolvedSaveRoot],
    purpose: SaveSnapshotPurpose,
    snapshot_id: String,
) -> Result<SaveBackupSnapshot, String> {
    let game_root = game_backup_dir(game_id)?;
    fs::create_dir_all(&game_root)
        .map_err(|error| format!("cannot create save backup root: {error}"))?;
    let pending_dir = game_root.join(format!("{snapshot_id}{PENDING_SUFFIX}"));
    let final_dir = game_root.join(&snapshot_id);
    if pending_dir.exists() || final_dir.exists() {
        return Err(format!("snapshot ID already exists: {snapshot_id}"));
    }
    fs::create_dir(&pending_dir)
        .map_err(|error| format!("cannot create pending snapshot: {error}"))?;

    let mut pending_files = Vec::new();
    let mut pending_directories = vec![pending_dir.clone()];
    let result = (|| {
        wait_for_stable_files(roots)?;
        let mut snapshot_roots = Vec::with_capacity(roots.len());
        let mut files = Vec::new();
        let mut total_bytes = 0u64;

        for root in roots {
            validate_existing_save_root(root)?;
            snapshot_roots.push(SaveSnapshotRoot {
                index: root.index,
                provider: root.provider.clone(),
                provider_save_id: root.provider_save_id.clone(),
                original_path: root.path.to_string_lossy().to_string(),
                root_fingerprint: root_fingerprint(root),
            });

            for item in WalkDir::new(&root.path).follow_links(false) {
                let item = item.map_err(|error| {
                    format!(
                        "failed to enumerate save root {}: {error}",
                        root.path.display()
                    )
                })?;
                let metadata = fs::symlink_metadata(item.path()).map_err(|error| {
                    format!(
                        "failed to inspect save path {}: {error}",
                        item.path().display()
                    )
                })?;
                if item.path() != root.path && is_reparse_or_symlink(&metadata) {
                    return Err(format!(
                        "save root contains a symlink or reparse point: {}",
                        item.path().display()
                    ));
                }
                if !metadata.is_file() {
                    continue;
                }

                let relative =
                    portable_relative_path(item.path().strip_prefix(&root.path).map_err(
                        |_| format!("save file escapes root: {}", item.path().display()),
                    )?)?;
                let manifest_path = format!("{}/{}", root.index, relative);
                let destination = pending_dir.join(Path::new(&manifest_path));
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "cannot create snapshot directory {}: {error}",
                            parent.display()
                        )
                    })?;
                    remember_parent_directories(&pending_dir, parent, &mut pending_directories)?;
                }
                pending_files.push(destination.clone());
                let (size_bytes, sha256) = copy_file_with_hash(item.path(), &destination)?;
                total_bytes = total_bytes
                    .checked_add(size_bytes)
                    .ok_or_else(|| "snapshot size overflow".to_string())?;
                files.push(SaveBackupEntry {
                    relative_path: manifest_path,
                    size_bytes,
                    sha256,
                });
            }
        }

        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let snapshot = SaveBackupSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            id: snapshot_id.clone(),
            game_id: game_id.to_string(),
            game_version: game_version.to_string(),
            runtime_version: snapshot_runtime_version(game_id)?,
            app_id,
            created_at: chrono::Utc::now().to_rfc3339(),
            purpose,
            roots: snapshot_roots,
            source_paths: roots
                .iter()
                .map(|root| root.path.to_string_lossy().to_string())
                .collect(),
            files,
            total_bytes,
        };
        write_json_durable(&pending_dir.join(MANIFEST_FILE), &snapshot)?;
        fs::rename(&pending_dir, &final_dir).map_err(|error| {
            format!(
                "failed to publish save snapshot {}: {error}",
                final_dir.display()
            )
        })?;
        Ok(snapshot)
    })();

    if result.is_err() && pending_dir.exists() {
        let _ = cleanup_pending_snapshot_exact(&pending_dir, &pending_files, &pending_directories);
    }
    result
}

fn load_snapshot(backup_dir: &Path) -> Result<SaveBackupSnapshot, String> {
    let manifest_path = backup_dir.join(MANIFEST_FILE);
    let data = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "cannot read snapshot manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    serde_json::from_str(&data).map_err(|error| format!("invalid snapshot manifest: {error}"))
}

fn verify_snapshot_files(backup_dir: &Path, snapshot: &SaveBackupSnapshot) -> Result<(), String> {
    let mut paths = HashSet::new();
    for entry in &snapshot.files {
        parse_snapshot_entry_path(&entry.relative_path)?;
        if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "snapshot file has invalid SHA-256: {}",
                entry.relative_path
            ));
        }
        if !paths.insert(entry.relative_path.clone()) {
            return Err(format!(
                "snapshot contains duplicate file: {}",
                entry.relative_path
            ));
        }
        let file = backup_dir.join(&entry.relative_path);
        let metadata = fs::symlink_metadata(&file)
            .map_err(|error| format!("snapshot file missing {}: {error}", file.display()))?;
        if !metadata.is_file() || is_reparse_or_symlink(&metadata) {
            return Err(format!(
                "snapshot entry is not a regular file: {}",
                file.display()
            ));
        }
        if metadata.len() != entry.size_bytes {
            return Err(format!("snapshot file size mismatch: {}", file.display()));
        }
        if !hash_file(&file)?.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!("snapshot file hash mismatch: {}", file.display()));
        }
    }
    Ok(())
}

fn verify_restored_snapshot(
    roots: &[ResolvedSaveRoot],
    snapshot: &SaveBackupSnapshot,
) -> Result<(), String> {
    let mut expected = HashSet::new();
    for entry in &snapshot.files {
        let (root_index, relative) = parse_snapshot_entry_path(&entry.relative_path)?;
        let root = roots
            .iter()
            .find(|candidate| candidate.index == root_index)
            .ok_or_else(|| format!("restored root index {root_index} is unavailable"))?;
        let target = root.path.join(relative);
        let metadata = fs::metadata(&target)
            .map_err(|error| format!("restored file missing {}: {error}", target.display()))?;
        if metadata.len() != entry.size_bytes
            || !hash_file(&target)?.eq_ignore_ascii_case(&entry.sha256)
        {
            return Err(format!(
                "restored file verification failed: {}",
                target.display()
            ));
        }
        expected.insert(target);
    }
    for root in roots {
        for (_, current) in collect_current_files(&root.path)? {
            if !expected.contains(&current) {
                return Err(format!(
                    "unexpected file remains after restore: {}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

fn snapshot_roots_to_resolved(
    snapshot: &SaveBackupSnapshot,
) -> Result<Vec<ResolvedSaveRoot>, String> {
    if snapshot.roots.is_empty() {
        return Err("snapshot does not contain trusted save-root metadata".to_string());
    }
    let candidates = resolve_save_roots(&snapshot.game_id, &snapshot.game_version, snapshot.app_id);
    let mut resolved = Vec::with_capacity(snapshot.roots.len());
    for expected in &snapshot.roots {
        let root = candidates
            .iter()
            .find(|candidate| {
                candidate.provider == expected.provider
                    && candidate.provider_save_id == expected.provider_save_id
                    && root_fingerprint(candidate) == expected.root_fingerprint
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "save root {} is no longer authorized for this game",
                    expected.original_path
                )
            })?;
        let mut root = root;
        root.index = expected.index;
        resolved.push(root);
    }
    resolved.sort_by_key(|root| root.index);
    Ok(resolved)
}

fn validate_existing_save_root(root: &ResolvedSaveRoot) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&root.path)
        .map_err(|error| format!("cannot inspect save root {}: {error}", root.path.display()))?;
    if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
        return Err(format!(
            "save root is not a regular directory: {}",
            root.path.display()
        ));
    }
    validate_absolute_non_root(&root.path)
}

fn ensure_save_root_exists(root: &ResolvedSaveRoot) -> Result<(), String> {
    validate_absolute_non_root(&root.path)?;
    if root.path.exists() {
        return validate_existing_save_root(root);
    }
    fs::create_dir_all(&root.path)
        .map_err(|error| format!("cannot create save root {}: {error}", root.path.display()))?;
    validate_existing_save_root(root)
}

fn validate_absolute_non_root(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.parent().is_none() {
        return Err(format!("unsafe save root: {}", path.display()));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("save root contains traversal: {}", path.display()));
    }
    Ok(())
}

fn collect_current_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for item in WalkDir::new(root).follow_links(false) {
        let item =
            item.map_err(|error| format!("failed to inspect {}: {error}", root.display()))?;
        let metadata = fs::symlink_metadata(item.path())
            .map_err(|error| format!("failed to inspect {}: {error}", item.path().display()))?;
        if item.path() != root && is_reparse_or_symlink(&metadata) {
            return Err(format!(
                "save root contains a symlink or reparse point: {}",
                item.path().display()
            ));
        }
        if metadata.is_file() {
            let relative = item
                .path()
                .strip_prefix(root)
                .map_err(|_| format!("save file escapes root: {}", item.path().display()))?
                .to_path_buf();
            files.push((relative, item.path().to_path_buf()));
        }
    }
    Ok(files)
}

fn parse_snapshot_entry_path(path: &str) -> Result<(usize, PathBuf), String> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(format!(
            "snapshot path must be relative: {}",
            path.display()
        ));
    }
    let mut components = path.components();
    let index = match components.next() {
        Some(Component::Normal(value)) => value
            .to_str()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| format!("invalid snapshot root index: {}", path.display()))?,
        _ => return Err(format!("invalid snapshot path: {}", path.display())),
    };
    let mut relative = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err(format!("unsafe snapshot path: {}", path.display())),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(format!(
            "snapshot path has no file name: {}",
            path.display()
        ));
    }
    Ok((index, relative))
}

fn portable_relative_path(path: &Path) -> Result<String, String> {
    if path.is_absolute() {
        return Err(format!("path must be relative: {}", path.display()));
    }
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => segments.push(
                value
                    .to_str()
                    .ok_or_else(|| format!("save path is not valid UTF-8: {}", path.display()))?,
            ),
            _ => return Err(format!("unsafe relative path: {}", path.display())),
        }
    }
    if segments.is_empty() {
        return Err("save file path is empty".to_string());
    }
    Ok(segments.join("/"))
}

fn root_fingerprint(root: &ResolvedSaveRoot) -> String {
    let normalized = root
        .path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{:?}\0{}\0{}",
        root.provider, root.provider_save_id, normalized
    ));
    format!("{:x}", hasher.finalize())
}

fn copy_file_with_hash(source: &Path, destination: &Path) -> Result<(u64, String), String> {
    let mut input = File::open(source)
        .map_err(|error| format!("cannot open save file {}: {error}", source.display()))?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| {
            format!(
                "cannot create snapshot file {}: {error}",
                destination.display()
            )
        })?;
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("cannot read save file {}: {error}", source.display()))?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(|error| {
            format!(
                "cannot write snapshot file {}: {error}",
                destination.display()
            )
        })?;
        hasher.update(&buffer[..read]);
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "snapshot file size overflow".to_string())?;
    }
    output.sync_all().map_err(|error| {
        format!(
            "cannot flush snapshot file {}: {error}",
            destination.display()
        )
    })?;
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open file {}: {error}", path.display()))?;
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read file {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_json_durable<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot encode snapshot manifest: {error}"))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| format!("cannot create manifest {}: {error}", temp.display()))?;
        file.write_all(&bytes)
            .map_err(|error| format!("cannot write manifest {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush manifest {}: {error}", temp.display()))?;
        fs::rename(&temp, path)
            .map_err(|error| format!("cannot publish manifest {}: {error}", path.display()))
    })();
    if result.is_err() && temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn validate_snapshot_id(snapshot_id: &str) -> Result<(), String> {
    if snapshot_id.is_empty()
        || !snapshot_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("invalid local save snapshot ID".to_string());
    }
    Ok(())
}

fn remove_published_snapshot_exact(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let snapshot = load_snapshot(root)?;
    verify_snapshot_files(root, &snapshot)?;
    let mut files = Vec::with_capacity(snapshot.files.len() + 1);
    let mut directories = vec![root.to_path_buf()];
    for entry in &snapshot.files {
        parse_snapshot_entry_path(&entry.relative_path)?;
        let file = root.join(&entry.relative_path);
        files.push(file.clone());
        if let Some(parent) = file.parent() {
            remember_parent_directories(root, parent, &mut directories)?;
        }
    }
    files.push(root.join(MANIFEST_FILE));
    cleanup_pending_snapshot_exact(root, &files, &directories)
}

fn cleanup_pending_snapshot_exact(
    root: &Path,
    files: &[PathBuf],
    directories: &[PathBuf],
) -> Result<(), String> {
    if !root.is_absolute()
        || root.parent().is_none()
        || root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("unsafe snapshot cleanup root: {}", root.display()));
    }
    let root_path = root.to_path_buf();
    let root_metadata = fs::symlink_metadata(&root_path)
        .map_err(|error| format!("cannot inspect snapshot root {}: {error}", root.display()))?;
    if !root_metadata.is_dir() || is_reparse_or_symlink(&root_metadata) {
        return Err(format!(
            "snapshot cleanup root is not a regular directory: {}",
            root.display()
        ));
    }

    for file in files {
        if !file.starts_with(&root_path) {
            return Err(format!(
                "snapshot cleanup file escaped root: {}",
                file.display()
            ));
        }
        match fs::symlink_metadata(file) {
            Ok(metadata) if metadata.file_type().is_file() => {
                fs::remove_file(file).map_err(|error| {
                    format!("cannot remove snapshot file {}: {error}", file.display())
                })?
            }
            Ok(_) => {
                return Err(format!(
                    "refusing to remove non-file snapshot entry: {}",
                    file.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot inspect snapshot file {}: {error}",
                    file.display()
                ))
            }
        }
    }

    let mut exact_directories = directories.to_vec();
    exact_directories.sort();
    exact_directories.dedup();
    exact_directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in exact_directories {
        if !directory.starts_with(&root_path) {
            return Err(format!(
                "snapshot cleanup directory escaped root: {}",
                directory.display()
            ));
        }
        match fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot remove exact snapshot directory {}: {error}",
                    directory.display()
                ))
            }
        }
    }
    Ok(())
}

fn remember_parent_directories(
    root: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !directory.starts_with(root) {
        return Err(format!(
            "snapshot directory escaped root: {}",
            directory.display()
        ));
    }
    let mut current = directory.to_path_buf();
    loop {
        output.push(current.clone());
        if current == root {
            break;
        }
        current = current
            .parent()
            .ok_or_else(|| "snapshot directory has no parent".to_string())?
            .to_path_buf();
    }
    Ok(())
}

fn snapshot_runtime_version(game_id: &str) -> Result<Option<String>, String> {
    let integration = crate::managed_game_runtime::local_runtime_integration_for_game(game_id);
    if integration.steam_runtime == crate::managed_game_runtime::SteamRuntimeMode::ManagedGse {
        return crate::managed_game_runtime::managed_runtime_version().map(Some);
    }
    Ok(None)
}

fn wait_for_stable_files(roots: &[ResolvedSaveRoot]) -> Result<(), String> {
    let mut previous = stability_fingerprint(roots)?;
    for _ in 0..FILE_STABILITY_ATTEMPTS {
        std::thread::sleep(FILE_STABILITY_DELAY);
        let current = stability_fingerprint(roots)?;
        if current == previous {
            return Ok(());
        }
        previous = current;
    }
    Err(
        "Save files did not become stable after the game exited; backup was not published."
            .to_string(),
    )
}

fn stability_fingerprint(
    roots: &[ResolvedSaveRoot],
) -> Result<Vec<(usize, PathBuf, u64, u128)>, String> {
    let mut fingerprint = Vec::new();
    for root in roots {
        for (relative, path) in collect_current_files(&root.path)? {
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("cannot inspect save file {}: {error}", path.display()))?;
            let modified = metadata
                .modified()
                .unwrap_or(UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            fingerprint.push((root.index, relative, metadata.len(), modified));
        }
    }
    fingerprint.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    Ok(fingerprint)
}

fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return metadata.file_attributes() & 0x400 != 0;
    }
    #[cfg(not(windows))]
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_relative_path_rejects_traversal() {
        assert!(portable_relative_path(Path::new("../save.dat")).is_err());
        assert_eq!(
            portable_relative_path(Path::new("slot/profile.dat")).unwrap(),
            "slot/profile.dat"
        );
    }

    #[test]
    fn parses_snapshot_root_and_relative_file() {
        let (root, path) = parse_snapshot_entry_path("2/slot/profile.dat").unwrap();
        assert_eq!(root, 2);
        assert_eq!(path, PathBuf::from("slot").join("profile.dat"));
        assert!(parse_snapshot_entry_path("2/../profile.dat").is_err());
    }

    #[test]
    fn copy_hash_and_exact_cleanup_are_verified() {
        let root = std::env::temp_dir().join(format!("0xo-save-test-{}", Uuid::new_v4()));
        let source = root.join("source.dat");
        let destination = root.join("nested").join("copy.dat");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&source, b"verified save payload").unwrap();
        let (size, hash) = copy_file_with_hash(&source, &destination).unwrap();
        assert_eq!(size, 21);
        assert_eq!(hash, hash_file(&source).unwrap());
        let directories = vec![root.clone(), destination.parent().unwrap().to_path_buf()];
        cleanup_pending_snapshot_exact(&root, &[source, destination], &directories).unwrap();
        assert!(!root.exists());
    }
}
