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
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use crate::save_paths::resolve_save_paths;

const MAX_SNAPSHOTS: usize = 10;
const BACKUP_ROOT_NAME: &str = "0xoLemon\\SaveBackups";
const MANIFEST_FILE: &str = "backup-manifest.json";

// ─── Global backup-in-progress flag (used by close guard) ───────────────────

static BACKUP_IN_PROGRESS: OnceLock<AtomicBool> = OnceLock::new();

fn backup_flag() -> &'static AtomicBool {
    BACKUP_IN_PROGRESS.get_or_init(|| AtomicBool::new(false))
}

/// Returns true when a save backup (local or GDrive) is currently running.
pub fn is_backup_in_progress() -> bool {
    backup_flag().load(Ordering::Relaxed)
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackupSnapshot {
    pub id: String,
    pub game_id: String,
    pub game_version: String,
    pub created_at: String,
    pub source_paths: Vec<String>,
    pub files: Vec<SaveBackupEntry>,
    pub total_bytes: u64,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Async wrapper — spawns a background thread so it never blocks the game-exit
/// event chain. Emits `launcher://save-backup-progress` events.
pub fn backup_after_exit_async(app: AppHandle, game_id: String, game_version: String) {
    std::thread::spawn(move || {
        backup_flag().store(true, Ordering::Relaxed);
        let result = do_backup(&app, &game_id, &game_version);
        backup_flag().store(false, Ordering::Relaxed);
        // Notify frontend that close guard can be released
        let _ = app.emit(
            "launcher://save-backup-guard-released",
            serde_json::json!({ "gameId": &game_id }),
        );

        if let Err(e) = result {
            emit_progress(
                &app,
                SaveBackupProgressEvent {
                    game_id,
                    state: "error".to_string(),
                    message: e,
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
        .filter(|e| e.path().is_dir())
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

/// Restore a snapshot: copies backed-up files back to their original save paths.
pub fn restore_snapshot(game_id: &str, snapshot_id: &str) -> Result<(), String> {
    let backup_dir = game_backup_dir(game_id)?.join(snapshot_id);
    let manifest_path = backup_dir.join(MANIFEST_FILE);
    let data = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let snapshot: SaveBackupSnapshot = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    let source_roots: Vec<PathBuf> = snapshot.source_paths.iter().map(PathBuf::from).collect();

    for entry in &snapshot.files {
        let rel = std::path::Path::new(&entry.relative_path);
        let mut components = rel.components();
        let root_idx_component = components.next();
        let rest: PathBuf = components.collect();

        let root_idx: usize = root_idx_component
            .and_then(|c| c.as_os_str().to_str())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("malformed path in manifest: {}", entry.relative_path))?;

        let source_root = source_roots
            .get(root_idx)
            .ok_or_else(|| format!("source root index {root_idx} out of range"))?;

        let dest = source_root.join(&rest);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let src_file = backup_dir.join(rel);
        fs::copy(&src_file, &dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn do_backup(app: &AppHandle, game_id: &str, game_version: &str) -> Result<(), String> {
    // ── 1. Resolve save paths for this version ────────────────────────────────
    let source_paths = resolve_save_paths(game_id, game_version);
    if source_paths.is_empty() {
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

    let existing_sources: Vec<&PathBuf> = source_paths.iter().filter(|p| p.exists()).collect();
    if existing_sources.is_empty() {
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

    // ── 2. Emit starting ──────────────────────────────────────────────────────
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

    // ── 3. Create snapshot directory ─────────────────────────────────────────
    let snapshot_id = timestamp_id();
    let backup_dir = game_backup_dir(game_id)?.join(&snapshot_id);
    fs::create_dir_all(&backup_dir).map_err(|e| format!("Cannot create backup dir: {e}"))?;

    // ── 4. Emit copying ──────────────────────────────────────────────────────
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

    // ── 5. Copy files ────────────────────────────────────────────────────────
    let mut entries: Vec<SaveBackupEntry> = Vec::new();
    let mut files_copied = 0usize;
    let mut bytes_copied = 0u64;

    for (root_idx, src_root) in existing_sources.iter().enumerate() {
        for entry in WalkDir::new(src_root).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let full = entry.path();
            let relative_from_root = full.strip_prefix(src_root).map_err(|e| e.to_string())?;
            let rel_in_backup: PathBuf =
                PathBuf::from(root_idx.to_string()).join(relative_from_root);
            let dest = backup_dir.join(&rel_in_backup);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let size = fs::metadata(full).map(|m| m.len()).unwrap_or(0);
            fs::copy(full, &dest)
                .map_err(|e| format!("copy failed for {}: {e}", full.display()))?;
            entries.push(SaveBackupEntry {
                relative_path: rel_in_backup.to_string_lossy().to_string(),
                size_bytes: size,
            });
            files_copied += 1;
            bytes_copied += size;
        }
    }

    let total_bytes = bytes_copied;

    // ── 6. Write manifest ────────────────────────────────────────────────────
    let now = chrono::Utc::now().to_rfc3339();
    let snapshot = SaveBackupSnapshot {
        id: snapshot_id.clone(),
        game_id: game_id.to_string(),
        game_version: game_version.to_string(),
        created_at: now,
        source_paths: existing_sources
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        files: entries,
        total_bytes,
    };
    let manifest_json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    fs::write(backup_dir.join(MANIFEST_FILE), manifest_json)
        .map_err(|e| format!("Cannot write manifest: {e}"))?;

    // ── 7. Prune old snapshots ────────────────────────────────────────────────
    let _ = prune_snapshots(game_id);

    // ── 8. Emit local-done ────────────────────────────────────────────────────
    emit_progress(
        app,
        SaveBackupProgressEvent {
            game_id: game_id.to_string(),
            state: "uploading".to_string(),
            message: format!(
                "Local backup done ({files_copied} files). Uploading to Google Drive…"
            ),
            files_copied,
            bytes_copied: total_bytes,
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

fn timestamp_id() -> String {
    let _ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dt = chrono::DateTime::<chrono::Utc>::from(SystemTime::now());
    format!("{}", dt.format("%Y-%m-%dT%H-%M-%S"))
}

fn prune_snapshots(game_id: &str) -> Result<(), String> {
    let backup_dir = game_backup_dir(game_id)?;
    if !backup_dir.exists() {
        return Ok(());
    }
    let mut dirs: Vec<PathBuf> = fs::read_dir(&backup_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    while dirs.len() > MAX_SNAPSHOTS {
        let oldest = dirs.remove(0);
        let _ = fs::remove_dir_all(&oldest);
    }
    Ok(())
}
