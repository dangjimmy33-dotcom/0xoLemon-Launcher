use crate::cloud_redirect::steam_detector::{find_steam_path, is_steam_running};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;
use zip::write::FileOptions;

use super::models::LocalBackupInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupMetadata {
    version: u32,
    id: String,
    account_id: String,
    app_id: String,
    created_at: String,
    reason: String,
}

fn safe_component(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 64
        || !trimmed.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(format!("Invalid {label}: {value}"));
    }
    Ok(trimmed.to_string())
}

fn backup_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("cloud_redirect")
        .join("backups"))
}

fn source_paths(steam: &Path, account_id: &str, app_id: &str) -> Vec<(&'static str, PathBuf)> {
    vec![
        (
            "storage",
            steam
                .join("cloud_redirect")
                .join("storage")
                .join(account_id)
                .join(app_id),
        ),
        (
            "blobs",
            steam
                .join("cloud_redirect")
                .join("blobs")
                .join(account_id)
                .join(app_id),
        ),
    ]
}

fn add_directory(
    writer: &mut zip::ZipWriter<File>,
    source: &Path,
    archive_root: &str,
) -> Result<(), String> {
    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);

    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        let entry = entry.map_err(|error| format!("Cannot scan {}: {error}", source.display()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(source)
            .map_err(|error| format!("Cannot make backup path relative: {error}"))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let archive_name = format!(
            "{}/{}",
            archive_root,
            relative.to_string_lossy().replace('\\', "/")
        );
        if entry.file_type().is_symlink() {
            return Err(format!("Refusing to back up symbolic link: {}", path.display()));
        }
        if entry.file_type().is_dir() {
            writer
                .add_directory(format!("{archive_name}/"), options)
                .map_err(|error| format!("Cannot add backup directory: {error}"))?;
        } else if entry.file_type().is_file() {
            writer
                .start_file(archive_name, options)
                .map_err(|error| format!("Cannot add backup file: {error}"))?;
            let mut input = File::open(path)
                .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
            std::io::copy(&mut input, writer)
                .map_err(|error| format!("Cannot write backup archive: {error}"))?;
        }
    }
    Ok(())
}

fn create_internal(
    app: &AppHandle,
    account_id: &str,
    app_id: &str,
    reason: &str,
    allow_empty: bool,
) -> Result<Option<LocalBackupInfo>, String> {
    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    let account_id = safe_component(account_id, "Steam account ID")?;
    let app_id = safe_component(app_id, "Steam AppID")?;
    let sources = source_paths(&steam, &account_id, &app_id);
    if !sources.iter().any(|(_, path)| path.is_dir()) {
        return if allow_empty {
            Ok(None)
        } else {
            Err("No synchronized CloudRedirect cache exists for this app. Sync it first.".to_string())
        };
    }

    let now = Utc::now();
    let id = format!(
        "{}_{}_{}_{}",
        account_id,
        app_id,
        now.format("%Y%m%dT%H%M%SZ"),
        reason.chars().map(|character| if character.is_ascii_alphanumeric() { character } else { '-' }).collect::<String>()
    );
    let root = backup_root(app)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Cannot create backup directory: {error}"))?;
    let destination = root.join(format!("{id}.zip"));
    let temporary = root.join(format!("{id}.zip.part"));

    let file = File::create(&temporary)
        .map_err(|error| format!("Cannot create {}: {error}", temporary.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let metadata = BackupMetadata {
        version: 1,
        id: id.clone(),
        account_id: account_id.clone(),
        app_id: app_id.clone(),
        created_at: now.to_rfc3339(),
        reason: reason.to_string(),
    };
    writer
        .start_file("backup.json", options)
        .map_err(|error| format!("Cannot write backup metadata: {error}"))?;
    writer
        .write_all(
            &serde_json::to_vec_pretty(&metadata)
                .map_err(|error| format!("Cannot serialize backup metadata: {error}"))?,
        )
        .map_err(|error| format!("Cannot write backup metadata: {error}"))?;

    for (name, path) in sources {
        if path.is_dir() {
            add_directory(&mut writer, &path, name)?;
        }
    }
    let mut file = writer
        .finish()
        .map_err(|error| format!("Cannot finalize backup archive: {error}"))?;
    file.flush()
        .map_err(|error| format!("Cannot flush backup archive: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Cannot sync backup archive: {error}"))?;
    fs::rename(&temporary, &destination)
        .map_err(|error| format!("Cannot commit backup archive: {error}"))?;
    let size = fs::metadata(&destination).map(|value| value.len()).unwrap_or_default();

    Ok(Some(LocalBackupInfo {
        id,
        account_id,
        app_id,
        size,
        created_at: now.to_rfc3339(),
        reason: reason.to_string(),
        path: destination.to_string_lossy().into_owned(),
    }))
}

pub fn create(app: &AppHandle, account_id: &str, app_id: &str) -> Result<LocalBackupInfo, String> {
    if is_steam_running() {
        return Err("Close Steam before creating a consistent CloudRedirect backup".to_string());
    }
    create_internal(app, account_id, app_id, "manual", false)?
        .ok_or_else(|| "No CloudRedirect data was available to back up".to_string())
}

/// Create a mandatory rollback point before a destructive CloudRedirect action.
/// The caller must synchronize the remote app into the local cache first.
pub fn create_safety(
    app: &AppHandle,
    account_id: &str,
    app_id: &str,
    reason: &str,
) -> Result<LocalBackupInfo, String> {
    if is_steam_running() {
        return Err("Close Steam before creating a CloudRedirect safety backup".to_string());
    }
    create_internal(app, account_id, app_id, reason, false)?
        .ok_or_else(|| "No synchronized CloudRedirect data was available for the safety backup".to_string())
}

fn metadata_from_archive(path: &Path) -> Result<BackupMetadata, String> {
    let file = File::open(path)
        .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid backup archive {}: {error}", path.display()))?;
    let mut entry = archive
        .by_name("backup.json")
        .map_err(|error| format!("Backup metadata missing in {}: {error}", path.display()))?;
    let mut raw = String::new();
    entry
        .read_to_string(&mut raw)
        .map_err(|error| format!("Cannot read backup metadata: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("Invalid backup metadata: {error}"))
}

pub fn list(app: &AppHandle, app_filter: Option<&str>) -> Result<Vec<LocalBackupInfo>, String> {
    let root = backup_root(app)?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Cannot list {}: {error}", root.display()))?
    {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("zip") {
            continue;
        }
        let metadata = match metadata_from_archive(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if app_filter.is_some_and(|filter| metadata.app_id != filter) {
            continue;
        }
        backups.push(LocalBackupInfo {
            id: metadata.id,
            account_id: metadata.account_id,
            app_id: metadata.app_id,
            size: fs::metadata(&path).map(|value| value.len()).unwrap_or_default(),
            created_at: metadata.created_at,
            reason: metadata.reason,
            path: path.to_string_lossy().into_owned(),
        });
    }
    backups.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(backups)
}

fn extract_archive(path: &Path, destination: &Path) -> Result<BackupMetadata, String> {
    let file = File::open(path)
        .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid backup archive: {error}"))?;
    let mut metadata = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Cannot read backup entry: {error}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe path in backup: {}", entry.name()))?
            .to_path_buf();
        if enclosed == Path::new("backup.json") {
            let mut raw = String::new();
            entry
                .read_to_string(&mut raw)
                .map_err(|error| format!("Cannot read backup metadata: {error}"))?;
            metadata = Some(
                serde_json::from_str(&raw)
                    .map_err(|error| format!("Invalid backup metadata: {error}"))?,
            );
            continue;
        }
        let out = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out)
                .map_err(|error| format!("Cannot create {}: {error}", out.display()))?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
            }
            let mut output = File::create(&out)
                .map_err(|error| format!("Cannot create {}: {error}", out.display()))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("Cannot restore {}: {error}", out.display()))?;
            output
                .sync_all()
                .map_err(|error| format!("Cannot sync {}: {error}", out.display()))?;
        }
    }
    metadata.ok_or_else(|| "Backup metadata is missing".to_string())
}

pub fn restore(app: &AppHandle, backup_id: &str, confirmation: &str) -> Result<LocalBackupInfo, String> {
    if is_steam_running() {
        return Err("Close Steam before restoring a CloudRedirect backup".to_string());
    }
    let id = safe_component(backup_id, "backup ID")?;
    if confirmation != format!("RESTORE {id}") {
        return Err(format!("Type RESTORE {id} to confirm"));
    }
    let root = backup_root(app)?;
    let archive_path = root.join(format!("{id}.zip"));
    if !archive_path.is_file() {
        return Err("Backup archive was not found".to_string());
    }
    let expected = metadata_from_archive(&archive_path)?;
    if expected.id != id {
        return Err("Backup identity does not match its archive".to_string());
    }

    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    let restore_root = steam
        .join("cloud_redirect")
        .join(format!(".0xolemon-restore-{id}"));
    if restore_root.exists() {
        fs::remove_dir_all(&restore_root)
            .map_err(|error| format!("Cannot clear restore staging: {error}"))?;
    }
    fs::create_dir_all(&restore_root)
        .map_err(|error| format!("Cannot create restore staging: {error}"))?;
    let metadata = extract_archive(&archive_path, &restore_root)?;
    if metadata.account_id != expected.account_id || metadata.app_id != expected.app_id {
        let _ = fs::remove_dir_all(&restore_root);
        return Err("Backup metadata changed while reading the archive".to_string());
    }

    let _ = create_internal(app, &metadata.account_id, &metadata.app_id, "pre-restore", true)?;
    let mut committed: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for name in ["storage", "blobs"] {
        let staged = restore_root.join(name);
        if !staged.is_dir() {
            continue;
        }
        let target = steam
            .join("cloud_redirect")
            .join(name)
            .join(&metadata.account_id)
            .join(&metadata.app_id);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
        }
        let rollback = if target.exists() {
            let path = target.with_extension(format!("rollback-{}", Utc::now().timestamp_millis()));
            fs::rename(&target, &path)
                .map_err(|error| format!("Cannot prepare restore rollback: {error}"))?;
            Some(path)
        } else {
            None
        };
        match fs::rename(&staged, &target) {
            Ok(()) => committed.push((target, rollback)),
            Err(error) => {
                if let Some(path) = rollback.as_ref() {
                    let _ = fs::rename(path, &target);
                }
                for (committed_target, old) in committed.iter().rev() {
                    let _ = fs::remove_dir_all(committed_target);
                    if let Some(path) = old {
                        let _ = fs::rename(path, committed_target);
                    }
                }
                let _ = fs::remove_dir_all(&restore_root);
                return Err(format!("Cannot commit restored backup: {error}"));
            }
        }
    }
    for (_, rollback) in committed {
        if let Some(path) = rollback {
            let _ = fs::remove_dir_all(path);
        }
    }
    let _ = fs::remove_dir_all(&restore_root);

    Ok(LocalBackupInfo {
        id: metadata.id,
        account_id: metadata.account_id,
        app_id: metadata.app_id,
        size: fs::metadata(&archive_path).map(|value| value.len()).unwrap_or_default(),
        created_at: metadata.created_at,
        reason: metadata.reason,
        path: archive_path.to_string_lossy().into_owned(),
    })
}
