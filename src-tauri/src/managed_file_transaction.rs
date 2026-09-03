use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const TRANSACTION_SCHEMA_VERSION: u32 = 1;
const COPY_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ManagedFileSpec {
    pub source: PathBuf,
    pub target: PathBuf,
    pub allowed_source_root: PathBuf,
    pub allowed_target_root: PathBuf,
    pub expected_sha256: String,
}

/// A replacement that has already been streamed and flushed next to its target.
///
/// This is intended for very large files such as Steam's appinfo cache: preparing a
/// second transaction stage would temporarily require another full-file copy. The
/// caller transfers ownership of `prepared` to the transaction and provides the
/// exact path where the replaced original must be retained after commit.
#[derive(Debug, Clone)]
pub struct ManagedPreparedFileSpec {
    pub prepared: PathBuf,
    pub target: PathBuf,
    pub retained_backup: PathBuf,
    pub allowed_target_root: PathBuf,
    pub expected_sha256: String,
    pub expected_original_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManagedDeleteSpec {
    pub target: PathBuf,
    pub allowed_target_root: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ManagedFileChange {
    Replace(ManagedFileSpec),
    PreparedReplace(ManagedPreparedFileSpec),
    Delete(ManagedDeleteSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFileReceipt {
    pub transaction_id: String,
    pub operation: String,
    pub files_replaced: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransactionState {
    Prepared,
    Committing,
    Committed,
    RollingBack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum FileOperation {
    Replace,
    Delete,
}

impl Default for FileOperation {
    fn default() -> Self {
        Self::Replace
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRecord {
    #[serde(default)]
    operation: FileOperation,
    target: PathBuf,
    stage: Option<PathBuf>,
    backup: PathBuf,
    expected_sha256: Option<String>,
    #[serde(default)]
    expected_original_sha256: Option<String>,
    had_original: bool,
    backup_created: bool,
    replacement_installed: bool,
    #[serde(default)]
    retain_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionJournal {
    schema_version: u32,
    transaction_id: String,
    operation: String,
    state: TransactionState,
    files: Vec<FileRecord>,
}

pub fn apply_files(
    app: &AppHandle,
    operation: &str,
    specs: Vec<ManagedFileSpec>,
) -> Result<ManagedFileReceipt, String> {
    apply_changes(
        app,
        operation,
        specs.into_iter().map(ManagedFileChange::Replace).collect(),
    )
}

pub fn apply_changes(
    app: &AppHandle,
    operation: &str,
    changes: Vec<ManagedFileChange>,
) -> Result<ManagedFileReceipt, String> {
    let root = transaction_root(app)?;
    apply_changes_at_with_id(&root, Uuid::new_v4(), operation, changes)
}

pub fn apply_changes_with_id(
    app: &AppHandle,
    transaction_id: Uuid,
    operation: &str,
    changes: Vec<ManagedFileChange>,
) -> Result<ManagedFileReceipt, String> {
    let root = transaction_root(app)?;
    apply_changes_at_with_id(&root, transaction_id, operation, changes)
}

pub fn apply_prepared_file(
    app: &AppHandle,
    operation: &str,
    spec: ManagedPreparedFileSpec,
) -> Result<ManagedFileReceipt, String> {
    apply_changes(
        app,
        operation,
        vec![ManagedFileChange::PreparedReplace(spec)],
    )
}

pub fn recover_pending(app: &AppHandle) -> Result<usize, String> {
    let root = transaction_root(app)?;
    recover_pending_at(&root)
}

fn transaction_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("managed transaction directory unavailable: {error}"))?
        .join("managed-file-transactions");
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create managed transaction directory {}: {error}",
            root.display()
        )
    })?;
    Ok(root)
}

#[cfg(test)]
fn apply_files_at(
    transaction_root: &Path,
    operation: &str,
    specs: Vec<ManagedFileSpec>,
) -> Result<ManagedFileReceipt, String> {
    apply_changes_at(
        transaction_root,
        operation,
        specs.into_iter().map(ManagedFileChange::Replace).collect(),
    )
}

#[cfg(test)]
fn apply_changes_at(
    transaction_root: &Path,
    operation: &str,
    changes: Vec<ManagedFileChange>,
) -> Result<ManagedFileReceipt, String> {
    apply_changes_at_with_id(transaction_root, Uuid::new_v4(), operation, changes)
}

fn apply_changes_at_with_id(
    transaction_root: &Path,
    transaction_id: Uuid,
    operation: &str,
    changes: Vec<ManagedFileChange>,
) -> Result<ManagedFileReceipt, String> {
    if changes.is_empty() {
        return Err("managed transaction requires at least one file".to_string());
    }

    fs::create_dir_all(transaction_root).map_err(|error| {
        format!(
            "failed to create transaction directory {}: {error}",
            transaction_root.display()
        )
    })?;

    let transaction_id = transaction_id.to_string();
    let journal_path = transaction_root.join(format!("{transaction_id}.json"));
    let mut records = Vec::with_capacity(changes.len());
    let mut targets = HashSet::with_capacity(changes.len());

    for (index, change) in changes.iter().enumerate() {
        let prepared = match change {
            ManagedFileChange::Replace(spec) => {
                prepare_replace_record(&transaction_id, index, spec)
            }
            ManagedFileChange::PreparedReplace(spec) => prepare_prepared_replace_record(spec),
            ManagedFileChange::Delete(spec) => prepare_delete_record(&transaction_id, index, spec),
        };
        match prepared {
            Ok(record) => {
                let target_key = transaction_path_key(&record.target);
                if !targets.insert(target_key) {
                    let mut error = format!(
                        "managed transaction contains a duplicate target: {}",
                        record.target.display()
                    );
                    let mut cleanup_failures = Vec::new();
                    if let Err(cleanup_error) = cleanup_staged_records(&records) {
                        cleanup_failures.push(cleanup_error);
                    }
                    if let Some(stage) = record.stage.as_deref() {
                        if let Err(cleanup_error) = remove_exact_file(stage) {
                            cleanup_failures.push(cleanup_error);
                        }
                    }
                    if !cleanup_failures.is_empty() {
                        error.push_str("; exact staged-file cleanup failed: ");
                        error.push_str(&cleanup_failures.join("; "));
                    }
                    return Err(error);
                }
                records.push(record)
            }
            Err(error) => {
                return match cleanup_staged_records(&records) {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; exact staged-file cleanup also failed: {cleanup_error}"
                    )),
                };
            }
        }
    }

    let mut journal = TransactionJournal {
        schema_version: TRANSACTION_SCHEMA_VERSION,
        transaction_id: transaction_id.clone(),
        operation: operation.trim().to_string(),
        state: TransactionState::Prepared,
        files: records,
    };
    if let Err(error) = persist_journal(&journal_path, &journal) {
        return match cleanup_staged_records(&journal.files) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!(
                "{error}; staged-file cleanup also failed: {cleanup_error}"
            )),
        };
    }

    #[cfg(test)]
    abort_transaction_at_test_checkpoint("prepared");

    journal.state = TransactionState::Committing;
    persist_journal(&journal_path, &journal)?;
    let commit_result = commit_files(&journal_path, &mut journal);
    if let Err(commit_error) = commit_result {
        let rollback_result = rollback_files(&journal_path, &mut journal);
        return match rollback_result {
            Ok(()) => Err(commit_error),
            Err(rollback_error) => Err(format!(
                "{commit_error}; rollback also failed: {rollback_error}. Recovery journal retained at {}",
                journal_path.display()
            )),
        };
    }

    journal.state = TransactionState::Committed;
    persist_journal(&journal_path, &journal)?;
    #[cfg(test)]
    abort_transaction_at_test_checkpoint("committed");
    cleanup_committed(&journal_path, &journal)?;

    Ok(ManagedFileReceipt {
        transaction_id,
        operation: journal.operation,
        files_replaced: journal
            .files
            .iter()
            .filter(|record| record.operation == FileOperation::Replace)
            .count(),
    })
}

fn transaction_path_key(path: &Path) -> String {
    let key = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

fn prepare_replace_record(
    transaction_id: &str,
    index: usize,
    spec: &ManagedFileSpec,
) -> Result<FileRecord, String> {
    let source = validate_existing_file(&spec.source, &spec.allowed_source_root, "source")?;
    let target = validate_target_file(&spec.target, &spec.allowed_target_root)?;
    if source == target {
        return Err(format!(
            "managed source and target must be different files: {}",
            source.display()
        ));
    }

    let expected_sha256 = normalize_sha256(&spec.expected_sha256)?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("target has no parent: {}", target.display()))?;
    let stage = parent.join(format!(".0xo-stage-{transaction_id}-{index}"));
    let backup = parent.join(format!(".0xo-backup-{transaction_id}-{index}"));
    ensure_absent(&stage)?;
    ensure_absent(&backup)?;
    let source_size = fs::metadata(&source)
        .map_err(|error| {
            format!(
                "failed to inspect managed source {}: {error}",
                source.display()
            )
        })?
        .len();
    ensure_available_space(parent, source_size)?;
    #[cfg(test)]
    exhaust_disposable_test_volume_after_preflight(parent, source_size)?;

    if let Err(error) = copy_verified(&source, &stage, &expected_sha256) {
        remove_exact_file(&stage).ok();
        return Err(error);
    }

    Ok(FileRecord {
        operation: FileOperation::Replace,
        had_original: target.exists(),
        target,
        stage: Some(stage),
        backup,
        expected_sha256: Some(expected_sha256),
        expected_original_sha256: None,
        backup_created: false,
        replacement_installed: false,
        retain_backup: false,
    })
}

fn prepare_prepared_replace_record(spec: &ManagedPreparedFileSpec) -> Result<FileRecord, String> {
    let prepared = validate_existing_file(
        &spec.prepared,
        &spec.allowed_target_root,
        "prepared replacement",
    )?;
    let target = validate_target_file(&spec.target, &spec.allowed_target_root)?;
    let backup = validate_target_file(&spec.retained_backup, &spec.allowed_target_root)?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("target has no parent: {}", target.display()))?;
    if prepared.parent() != Some(parent) || backup.parent() != Some(parent) {
        return Err(format!(
            "prepared replacement, target and retained backup must share one directory: {}",
            target.display()
        ));
    }
    if prepared == target || prepared == backup || target == backup {
        return Err("prepared replacement paths must be distinct".to_string());
    }
    ensure_absent(&backup)?;

    let expected_sha256 = normalize_sha256(&spec.expected_sha256)?;
    let actual_sha256 = sha256_file(&prepared)?;
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "managed prepared hash mismatch for {}: expected {}, got {}",
            prepared.display(),
            expected_sha256,
            actual_sha256
        ));
    }
    let expected_original_sha256 = spec
        .expected_original_sha256
        .as_deref()
        .map(normalize_sha256)
        .transpose()?;
    if let Some(expected_original) = expected_original_sha256.as_deref() {
        if !target.is_file() {
            return Err("prepared replacement expected an original target file".to_string());
        }
        let actual_original = sha256_file(&target)?;
        if actual_original != expected_original {
            return Err(format!(
                "managed original hash mismatch for {}: expected {}, got {}",
                target.display(),
                expected_original,
                actual_original
            ));
        }
    }

    Ok(FileRecord {
        operation: FileOperation::Replace,
        had_original: target.exists(),
        target,
        stage: Some(prepared),
        backup,
        expected_sha256: Some(expected_sha256),
        expected_original_sha256,
        backup_created: false,
        replacement_installed: false,
        retain_backup: true,
    })
}

fn prepare_delete_record(
    transaction_id: &str,
    index: usize,
    spec: &ManagedDeleteSpec,
) -> Result<FileRecord, String> {
    let target = validate_target_file(&spec.target, &spec.allowed_target_root)?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("target has no parent: {}", target.display()))?;
    let backup = parent.join(format!(".0xo-backup-{transaction_id}-{index}"));
    ensure_absent(&backup)?;

    Ok(FileRecord {
        operation: FileOperation::Delete,
        had_original: target.exists(),
        target,
        stage: None,
        backup,
        expected_sha256: None,
        expected_original_sha256: None,
        backup_created: false,
        replacement_installed: false,
        retain_backup: false,
    })
}

fn commit_files(journal_path: &Path, journal: &mut TransactionJournal) -> Result<(), String> {
    for index in 0..journal.files.len() {
        if journal.files[index].had_original && !journal.files[index].backup_created {
            rename_same_volume(&journal.files[index].target, &journal.files[index].backup)?;
            #[cfg(test)]
            abort_transaction_at_test_checkpoint("backupRenamed");
            journal.files[index].backup_created = true;
            persist_journal(journal_path, journal)?;
            if let Some(expected) = journal.files[index].expected_original_sha256.as_deref() {
                let actual = sha256_file(&journal.files[index].backup)?;
                if actual != expected {
                    return Err(format!(
                        "managed original backup hash mismatch for {}: expected {}, got {}",
                        journal.files[index].target.display(),
                        expected,
                        actual
                    ));
                }
            }
        }

        match journal.files[index].operation {
            FileOperation::Replace => {
                if !journal.files[index].replacement_installed {
                    let stage = journal.files[index].stage.as_ref().ok_or_else(|| {
                        format!(
                            "replace transaction has no stage file for {}",
                            journal.files[index].target.display()
                        )
                    })?;
                    rename_same_volume(stage, &journal.files[index].target)?;
                    #[cfg(test)]
                    abort_transaction_at_test_checkpoint("replacementRenamed");
                    journal.files[index].replacement_installed = true;
                    persist_journal(journal_path, journal)?;
                }

                let expected =
                    journal.files[index]
                        .expected_sha256
                        .as_deref()
                        .ok_or_else(|| {
                            format!(
                                "replace transaction has no expected hash for {}",
                                journal.files[index].target.display()
                            )
                        })?;
                let installed_hash = sha256_file(&journal.files[index].target)?;
                if installed_hash != expected {
                    return Err(format!(
                        "managed file hash mismatch after commit for {}: expected {}, got {}",
                        journal.files[index].target.display(),
                        expected,
                        installed_hash
                    ));
                }
            }
            FileOperation::Delete => {
                if !journal.files[index].replacement_installed {
                    if journal.files[index].target.exists() {
                        return Err(format!(
                            "managed delete target still exists after backup: {}",
                            journal.files[index].target.display()
                        ));
                    }
                    journal.files[index].replacement_installed = true;
                    persist_journal(journal_path, journal)?;
                }
            }
        }
    }
    Ok(())
}

fn rollback_files(journal_path: &Path, journal: &mut TransactionJournal) -> Result<(), String> {
    reconcile_journal_with_disk(journal);
    journal.state = TransactionState::RollingBack;
    persist_journal(journal_path, journal)?;

    let mut failures = Vec::new();
    for record in journal.files.iter_mut().rev() {
        if record.operation == FileOperation::Replace
            && record.replacement_installed
            && record.target.exists()
        {
            let expected = record.expected_sha256.as_deref().unwrap_or_default();
            match sha256_file(&record.target) {
                Ok(actual) if actual == expected => {
                    if let Err(error) = remove_exact_file(&record.target) {
                        failures.push(error);
                        continue;
                    }
                }
                Ok(_) => {
                    failures.push(format!(
                        "refusing to remove a modified managed target during rollback: {}",
                        record.target.display()
                    ));
                    continue;
                }
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            }
            record.replacement_installed = false;
        }

        if record.backup_created && record.backup.exists() {
            if record.target.exists() {
                failures.push(format!(
                    "cannot restore backup because target exists: {}",
                    record.target.display()
                ));
                continue;
            }
            match rename_same_volume(&record.backup, &record.target) {
                Ok(()) => record.backup_created = false,
                Err(error) => failures.push(error),
            }
        }
        record.replacement_installed = false;
    }

    persist_journal(journal_path, journal)?;
    if failures.is_empty() {
        if let Err(error) = cleanup_staged_records(&journal.files) {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        remove_exact_file(journal_path)?;
        Ok(())
    } else {
        Err(format!(
            "managed rollback is incomplete; recovery journal retained at {}: {}",
            journal_path.display(),
            failures.join("; ")
        ))
    }
}

fn reconcile_journal_with_disk(journal: &mut TransactionJournal) {
    for record in &mut journal.files {
        if record.had_original && record.backup.exists() {
            record.backup_created = true;
        }

        match record.operation {
            FileOperation::Replace => {
                let stage_exists = record.stage.as_ref().is_some_and(|stage| stage.exists());
                if record.target.exists() && !stage_exists {
                    record.replacement_installed = true;
                }
            }
            FileOperation::Delete => {
                if !record.target.exists() && (!record.had_original || record.backup.exists()) {
                    record.replacement_installed = true;
                }
            }
        }
    }
}

fn cleanup_committed(journal_path: &Path, journal: &TransactionJournal) -> Result<(), String> {
    let mut failures = Vec::new();
    for record in &journal.files {
        if let Some(stage) = &record.stage {
            if let Err(error) = remove_exact_file(stage) {
                failures.push(error);
            }
        }
        if !record.retain_backup {
            if let Err(error) = remove_exact_file(&record.backup) {
                failures.push(error);
            }
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "managed transaction committed but cleanup is pending: {}",
            failures.join("; ")
        ));
    }
    remove_exact_file(journal_path)
}

fn recover_pending_at(transaction_root: &Path) -> Result<usize, String> {
    if !transaction_root.exists() {
        return Ok(0);
    }

    let mut recovered = 0usize;
    for entry in fs::read_dir(transaction_root).map_err(|error| {
        format!(
            "failed to inspect managed transactions in {}: {error}",
            transaction_root.display()
        )
    })? {
        let entry =
            entry.map_err(|error| format!("failed to inspect transaction entry: {error}"))?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
            .is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }

        let bytes = fs::read(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let mut journal: TransactionJournal = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid transaction journal {}: {error}", path.display()))?;
        if journal.schema_version != TRANSACTION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported managed transaction schema {} in {}",
                journal.schema_version,
                path.display()
            ));
        }

        match journal.state {
            TransactionState::Committed => cleanup_committed(&path, &journal)?,
            TransactionState::Prepared
            | TransactionState::Committing
            | TransactionState::RollingBack => rollback_files(&path, &mut journal)?,
        }
        recovered = recovered.saturating_add(1);
    }
    Ok(recovered)
}

fn validate_existing_file(
    path: &Path,
    allowed_root: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    let root = canonical_directory_without_reparse(allowed_root, &format!("allowed {label} root"))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve {label} {}: {error}", path.display()))?;
    if !canonical.starts_with(&root) {
        return Err(format!(
            "{label} path escapes its allowed root: {}",
            path.display()
        ));
    }
    reject_reparse_chain(&root, &canonical)?;
    if !canonical.is_file() {
        return Err(format!("{label} is not a file: {}", canonical.display()));
    }
    Ok(canonical)
}

fn validate_target_file(path: &Path, allowed_root: &Path) -> Result<PathBuf, String> {
    let root = canonical_directory_without_reparse(allowed_root, "allowed target root")?;
    let target_path = absolute_path(path)?;
    let parent = target_path
        .parent()
        .ok_or_else(|| format!("target has no parent: {}", target_path.display()))?;
    let file_name = target_path
        .file_name()
        .ok_or_else(|| format!("target has no file name: {}", target_path.display()))?;

    // Resolve the deepest existing ancestor before creating anything. This prevents an
    // untrusted target from causing directory creation outside the transaction root.
    let mut existing_ancestor = parent.to_path_buf();
    while !existing_ancestor.exists() {
        if !existing_ancestor.pop() {
            return Err(format!(
                "target has no existing ancestor: {}",
                target_path.display()
            ));
        }
    }
    let canonical_ancestor = existing_ancestor.canonicalize().map_err(|error| {
        format!(
            "failed to resolve target ancestor {}: {error}",
            existing_ancestor.display()
        )
    })?;
    if !canonical_ancestor.starts_with(&root) {
        return Err(format!(
            "target path escapes its allowed root: {}",
            target_path.display()
        ));
    }
    reject_reparse_chain(&root, &canonical_ancestor)?;

    let missing = parent.strip_prefix(&existing_ancestor).map_err(|_| {
        format!(
            "target parent cannot be resolved safely: {}",
            parent.display()
        )
    })?;
    let mut canonical_parent = canonical_ancestor;
    for component in missing.components() {
        let Component::Normal(segment) = component else {
            return Err(format!(
                "target contains an unsafe path component: {}",
                target_path.display()
            ));
        };
        canonical_parent.push(segment);
        match fs::symlink_metadata(&canonical_parent) {
            Ok(metadata) => {
                if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
                    return Err(format!(
                        "target parent contains a non-directory or reparse point: {}",
                        canonical_parent.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&canonical_parent).map_err(|error| {
                    format!(
                        "failed to create target directory {}: {error}",
                        canonical_parent.display()
                    )
                })?;
                let metadata = fs::symlink_metadata(&canonical_parent).map_err(|error| {
                    format!(
                        "failed to inspect new target directory {}: {error}",
                        canonical_parent.display()
                    )
                })?;
                if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
                    return Err(format!(
                        "new target directory is not safe: {}",
                        canonical_parent.display()
                    ));
                }
            }
            Err(error) => {
                return Err(format!(
                    "failed to inspect target directory {}: {error}",
                    canonical_parent.display()
                ));
            }
        }
    }

    let target = canonical_parent.join(file_name);
    if target.exists() {
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("failed to inspect target {}: {error}", target.display()))?;
        if is_reparse_or_symlink(&metadata) || !metadata.is_file() {
            return Err(format!(
                "target must be a regular file without reparse points: {}",
                target.display()
            ));
        }
    }
    Ok(target)
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| {
            format!(
                "failed to resolve relative path {}: {error}",
                path.display()
            )
        })
}

fn canonical_directory_without_reparse(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve {label} {}: {error}", path.display()))?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|error| format!("failed to inspect {label} {}: {error}", canonical.display()))?;
    if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
        return Err(format!(
            "{label} must be a regular directory without reparse points: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn reject_reparse_chain(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("path {} is outside {}", path.display(), root.display()))?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component.as_os_str());
        if !cursor.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&cursor)
            .map_err(|error| format!("failed to inspect {}: {error}", cursor.display()))?;
        if is_reparse_or_symlink(&metadata) {
            return Err(format!(
                "managed path contains a symlink or reparse point: {}",
                cursor.display()
            ));
        }
    }
    Ok(())
}

fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn copy_verified(source: &Path, stage: &Path, expected_sha256: &str) -> Result<(), String> {
    let mut reader = File::open(source)
        .map_err(|error| format!("failed to open source {}: {error}", source.display()))?;
    let mut writer = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(stage)
        .map_err(|error| format!("failed to create stage {}: {error}", stage.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("failed to read source {}: {error}", source.display()))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|error| format!("failed to write stage {}: {error}", stage.display()))?;
        hasher.update(&buffer[..read]);
    }
    writer
        .sync_all()
        .map_err(|error| format!("failed to flush stage {}: {error}", stage.display()))?;
    let actual = hex::encode(hasher.finalize());
    if actual != expected_sha256 {
        return Err(format!(
            "managed source hash mismatch for {}: expected {}, got {}",
            source.display(),
            expected_sha256,
            actual
        ));
    }
    Ok(())
}

fn ensure_available_space(directory: &Path, required_bytes: u64) -> Result<(), String> {
    let available = fs2::available_space(directory).map_err(|error| {
        format!(
            "failed to inspect free space for {}: {error}",
            directory.display()
        )
    })?;
    if available < required_bytes {
        return Err(format!(
            "INSUFFICIENT_SPACE: managed transaction needs {required_bytes} bytes in {}, found {available}",
            directory.display()
        ));
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("expectedSha256 must contain exactly 64 hexadecimal characters".to_string());
    }
    Ok(normalized)
}

fn persist_journal(path: &Path, journal: &TransactionJournal) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| format!("failed to serialize managed transaction: {error}"))?;
    let next = path.with_extension("json.next");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&next)
        .map_err(|error| format!("failed to create journal {}: {error}", next.display()))?;
    file.write_all(&bytes)
        .map_err(|error| format!("failed to write journal {}: {error}", next.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to flush journal {}: {error}", next.display()))?;
    replace_file_atomic(&next, path)
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "failed to atomically replace {} with {}: {}",
            target.display(),
            source.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| {
        format!(
            "failed to atomically replace {} with {}: {error}",
            target.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
fn rename_same_volume(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    if source.parent() != target.parent() {
        return Err(format!(
            "managed rename must remain in one directory: {} -> {}",
            source.display(),
            target.display()
        ));
    }
    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "failed to write-through rename {} to {}: {}",
            source.display(),
            target.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn rename_same_volume(source: &Path, target: &Path) -> Result<(), String> {
    if source.parent() != target.parent() {
        return Err(format!(
            "managed rename must remain in one directory: {} -> {}",
            source.display(),
            target.display()
        ));
    }
    fs::rename(source, target).map_err(|error| {
        format!(
            "failed to rename {} to {}: {error}",
            source.display(),
            target.display()
        )
    })
}

fn ensure_absent(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "managed transaction path already exists: {}",
            path.display()
        ));
    }
    Ok(())
}

fn remove_exact_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
                return Err(format!(
                    "refusing to remove non-file transaction path: {}",
                    path.display()
                ));
            }
            fs::remove_file(path)
                .map_err(|error| format!("failed to remove {}: {error}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
    }
}

fn cleanup_staged_records(records: &[FileRecord]) -> Result<(), String> {
    let mut failures = Vec::new();
    for record in records {
        if let Some(stage) = &record.stage {
            if let Err(error) = remove_exact_file(stage) {
                failures.push(error);
            }
        }
        if !record.backup_created {
            if let Err(error) = remove_exact_file(&record.backup) {
                failures.push(error);
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

#[cfg(test)]
fn abort_transaction_at_test_checkpoint(checkpoint: &str) {
    if std::env::var("OXO_TEST_TRANSACTION_ABORT_AT")
        .ok()
        .as_deref()
        == Some(checkpoint)
    {
        std::process::abort();
    }
}

#[cfg(test)]
fn exhaust_disposable_test_volume_after_preflight(
    directory: &Path,
    required_bytes: u64,
) -> Result<(), String> {
    let Some(filler_value) = std::env::var_os("OXO_TEST_FILL_DISK_AFTER_PREFLIGHT") else {
        return Ok(());
    };
    let filler = PathBuf::from(filler_value);
    let volume_root = PathBuf::from(
        std::env::var_os("OXO_TEST_FULL_DISK_ROOT")
            .ok_or_else(|| "Full-disk test root is not configured".to_string())?,
    );
    let canonical_root = volume_root
        .canonicalize()
        .map_err(|error| format!("Could not resolve disposable test volume: {error}"))?;
    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| format!("Could not resolve full-disk target directory: {error}"))?;
    if !canonical_directory.starts_with(&canonical_root) {
        return Err("Refusing to fill a volume outside the disposable test root".to_string());
    }
    let filler_parent = filler
        .parent()
        .ok_or_else(|| "Full-disk filler has no parent".to_string())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve full-disk filler parent: {error}"))?;
    if filler_parent != canonical_directory {
        return Err("Full-disk filler must be an exact child of the transaction target directory".to_string());
    }
    let marker = canonical_root.join(".0xo-disposable-full-disk-test");
    if fs::read_to_string(&marker).ok().as_deref() != Some("0xo-disposable-full-disk-test-v1") {
        return Err("Disposable full-disk test marker is missing or invalid".to_string());
    }
    let total = fs2::total_space(&canonical_root)
        .map_err(|error| format!("Could not inspect disposable volume size: {error}"))?;
    if total > 512 * 1024 * 1024 {
        return Err("Refusing to fill a test volume larger than 512 MiB".to_string());
    }

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&filler)
        .map_err(|error| format!("Could not create exact full-disk filler: {error}"))?;
    let block = vec![0xA5; 1024 * 1024];
    loop {
        match file.write_all(&block) {
            Ok(()) => {}
            Err(_) => break,
        }
    }
    let _ = file.sync_all();
    drop(file);
    let remaining = fs2::available_space(&canonical_directory)
        .map_err(|error| format!("Could not recheck disposable volume space: {error}"))?;
    if remaining >= required_bytes {
        return Err(format!(
            "Disposable volume did not become full enough: {remaining} bytes remain"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("0xo-managed-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test root");
        root
    }

    fn write_and_hash(path: &Path, bytes: &[u8]) -> String {
        fs::write(path, bytes).expect("write fixture");
        hex::encode(Sha256::digest(bytes))
    }

    fn cleanup_test_root(root: &Path) {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(path).ok();
                } else if path.is_dir() {
                    if let Ok(children) = fs::read_dir(&path) {
                        for child in children.flatten() {
                            if child.path().is_file() {
                                fs::remove_file(child.path()).ok();
                            }
                        }
                    }
                    fs::remove_dir(path).ok();
                }
            }
        }
        fs::remove_dir(root).ok();
    }

    #[test]
    fn replaces_file_and_removes_transaction_artifacts() {
        let root = test_root("commit");
        let source_root = root.join("source");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        let source = source_root.join("save.bin");
        let target = target_root.join("save.bin");
        let hash = write_and_hash(&source, b"new-save");
        fs::write(&target, b"old-save").unwrap();

        let receipt = apply_files_at(
            &journal_root,
            "test",
            vec![ManagedFileSpec {
                source,
                target: target.clone(),
                allowed_source_root: source_root,
                allowed_target_root: target_root,
                expected_sha256: hash,
            }],
        )
        .unwrap();

        assert_eq!(receipt.files_replaced, 1);
        assert_eq!(fs::read(&target).unwrap(), b"new-save");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn caller_supplied_transaction_id_is_preserved_in_receipt() {
        let root = test_root("explicit-id");
        let source_root = root.join("source");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        let source = source_root.join("runtime.bin");
        let target = target_root.join("runtime.bin");
        let hash = write_and_hash(&source, b"managed-runtime");
        let transaction_id = Uuid::new_v4();

        let receipt = apply_changes_at_with_id(
            &journal_root,
            transaction_id,
            "managed-runtime",
            vec![ManagedFileChange::Replace(ManagedFileSpec {
                source,
                target,
                allowed_source_root: source_root,
                allowed_target_root: target_root,
                expected_sha256: hash,
            })],
        )
        .unwrap();

        assert_eq!(receipt.transaction_id, transaction_id.to_string());
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn source_hash_mismatch_preserves_original_target() {
        let root = test_root("hash-mismatch");
        let source_root = root.join("source");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        let source = source_root.join("save.bin");
        let target = target_root.join("save.bin");
        fs::write(&source, b"corrupt").unwrap();
        fs::write(&target, b"old-save").unwrap();

        let error = apply_files_at(
            &journal_root,
            "test",
            vec![ManagedFileSpec {
                source,
                target: target.clone(),
                allowed_source_root: source_root,
                allowed_target_root: target_root,
                expected_sha256: "00".repeat(32),
            }],
        )
        .unwrap_err();

        assert!(error.contains("hash mismatch"));
        assert_eq!(fs::read(&target).unwrap(), b"old-save");
        if journal_root.exists() {
            fs::remove_dir(&journal_root).ok();
        }
        cleanup_test_root(&root);
    }

    #[test]
    fn rejected_target_does_not_create_directories_outside_allowed_root() {
        let root = test_root("target-escape");
        let target_root = root.join("target");
        let outside_parent = root.join("outside");
        fs::create_dir_all(&target_root).unwrap();

        let error = validate_target_file(
            &outside_parent.join("nested").join("save.bin"),
            &target_root,
        )
        .unwrap_err();

        assert!(error.contains("escapes its allowed root"));
        assert!(!outside_parent.exists());
        cleanup_test_root(&root);
    }

    #[test]
    fn delete_change_removes_only_the_named_file() {
        let root = test_root("delete");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        let deleted = target_root.join("stale.sav");
        let retained = target_root.join("current.sav");
        fs::write(&deleted, b"stale").unwrap();
        fs::write(&retained, b"current").unwrap();

        let receipt = apply_changes_at(
            &journal_root,
            "test-delete",
            vec![ManagedFileChange::Delete(ManagedDeleteSpec {
                target: deleted.clone(),
                allowed_target_root: target_root.clone(),
            })],
        )
        .unwrap();

        assert_eq!(receipt.files_replaced, 0);
        assert!(!deleted.exists());
        assert_eq!(fs::read(&retained).unwrap(), b"current");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(retained).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn recovery_restores_original_when_crash_happens_before_backup_flag_persist() {
        let root = test_root("recovery-backup-boundary");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        fs::create_dir_all(&journal_root).unwrap();
        let target = target_root.join("save.bin");
        let stage = target_root.join(".0xo-stage-test-0");
        let backup = target_root.join(".0xo-backup-test-0");
        let expected = write_and_hash(&stage, b"new-save");
        fs::write(&target, b"old-save").unwrap();
        fs::rename(&target, &backup).unwrap();

        let journal_path = journal_root.join("test.json");
        let journal = TransactionJournal {
            schema_version: TRANSACTION_SCHEMA_VERSION,
            transaction_id: "test".to_string(),
            operation: "recovery-test".to_string(),
            state: TransactionState::Committing,
            files: vec![FileRecord {
                operation: FileOperation::Replace,
                target: target.clone(),
                stage: Some(stage),
                backup,
                expected_sha256: Some(expected),
                expected_original_sha256: None,
                had_original: true,
                backup_created: false,
                replacement_installed: false,
                retain_backup: false,
            }],
        };
        persist_journal(&journal_path, &journal).unwrap();

        assert_eq!(recover_pending_at(&journal_root).unwrap(), 1);
        assert_eq!(fs::read(&target).unwrap(), b"old-save");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(target).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn recovery_restores_original_after_backup_flag_was_persisted() {
        let root = test_root("recovery-after-backup-flag");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        fs::create_dir_all(&journal_root).unwrap();
        let target = target_root.join("save.bin");
        let stage = target_root.join(".0xo-stage-test-0");
        let backup = target_root.join(".0xo-backup-test-0");
        let expected = write_and_hash(&stage, b"new-save");
        fs::write(&backup, b"old-save").unwrap();
        let journal_path = journal_root.join("test.json");
        persist_journal(
            &journal_path,
            &TransactionJournal {
                schema_version: TRANSACTION_SCHEMA_VERSION,
                transaction_id: "test".to_string(),
                operation: "recovery-test".to_string(),
                state: TransactionState::Committing,
                files: vec![FileRecord {
                    operation: FileOperation::Replace,
                    target: target.clone(),
                    stage: Some(stage),
                    backup,
                    expected_sha256: Some(expected),
                    expected_original_sha256: None,
                    had_original: true,
                    backup_created: true,
                    replacement_installed: false,
                    retain_backup: false,
                }],
            },
        )
        .unwrap();

        assert_eq!(recover_pending_at(&journal_root).unwrap(), 1);
        assert_eq!(fs::read(&target).unwrap(), b"old-save");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(target).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn recovery_detects_replacement_rename_before_flag_and_rolls_back() {
        let root = test_root("recovery-after-replacement-rename");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        fs::create_dir_all(&journal_root).unwrap();
        let target = target_root.join("save.bin");
        let stage = target_root.join(".0xo-stage-test-0");
        let backup = target_root.join(".0xo-backup-test-0");
        let expected = write_and_hash(&target, b"new-save");
        fs::write(&backup, b"old-save").unwrap();
        let journal_path = journal_root.join("test.json");
        persist_journal(
            &journal_path,
            &TransactionJournal {
                schema_version: TRANSACTION_SCHEMA_VERSION,
                transaction_id: "test".to_string(),
                operation: "recovery-test".to_string(),
                state: TransactionState::Committing,
                files: vec![FileRecord {
                    operation: FileOperation::Replace,
                    target: target.clone(),
                    stage: Some(stage),
                    backup,
                    expected_sha256: Some(expected),
                    expected_original_sha256: None,
                    had_original: true,
                    backup_created: true,
                    replacement_installed: false,
                    retain_backup: false,
                }],
            },
        )
        .unwrap();

        assert_eq!(recover_pending_at(&journal_root).unwrap(), 1);
        assert_eq!(fs::read(&target).unwrap(), b"old-save");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(target).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn committed_recovery_keeps_verified_target_and_cleans_exact_artifacts() {
        let root = test_root("recovery-committed-cleanup");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        fs::create_dir_all(&journal_root).unwrap();
        let target = target_root.join("save.bin");
        let stage = target_root.join(".0xo-stage-test-0");
        let backup = target_root.join(".0xo-backup-test-0");
        let expected = write_and_hash(&target, b"new-save");
        fs::write(&backup, b"old-save").unwrap();
        let journal_path = journal_root.join("test.json");
        persist_journal(
            &journal_path,
            &TransactionJournal {
                schema_version: TRANSACTION_SCHEMA_VERSION,
                transaction_id: "test".to_string(),
                operation: "recovery-test".to_string(),
                state: TransactionState::Committed,
                files: vec![FileRecord {
                    operation: FileOperation::Replace,
                    target: target.clone(),
                    stage: Some(stage),
                    backup: backup.clone(),
                    expected_sha256: Some(expected),
                    expected_original_sha256: None,
                    had_original: true,
                    backup_created: true,
                    replacement_installed: true,
                    retain_backup: false,
                }],
            },
        )
        .unwrap();

        assert_eq!(recover_pending_at(&journal_root).unwrap(), 1);
        assert_eq!(fs::read(&target).unwrap(), b"new-save");
        assert!(!backup.exists());
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(target).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn prepared_replace_avoids_second_copy_and_retains_original() {
        let root = test_root("prepared-retained-backup");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&target_root).unwrap();
        let target = target_root.join("appinfo.vdf");
        let prepared = target_root.join(".0xolemon-appinfo-stage.vdf");
        let backup = target_root.join(".0xolemon-appinfo-backup.vdf");
        fs::write(&target, b"old-cache").unwrap();
        let expected = write_and_hash(&prepared, b"new-cache");

        let receipt = apply_changes_at(
            &journal_root,
            "steam-appinfo",
            vec![ManagedFileChange::PreparedReplace(
                ManagedPreparedFileSpec {
                    prepared: prepared.clone(),
                    target: target.clone(),
                    retained_backup: backup.clone(),
                    allowed_target_root: target_root.clone(),
                    expected_sha256: expected,
                    expected_original_sha256: Some(sha256_file(&target).unwrap()),
                },
            )],
        )
        .unwrap();

        assert_eq!(receipt.files_replaced, 1);
        assert_eq!(fs::read(&target).unwrap(), b"new-cache");
        assert_eq!(fs::read(&backup).unwrap(), b"old-cache");
        assert!(!prepared.exists());
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        fs::remove_file(target).ok();
        fs::remove_file(backup).ok();
        fs::remove_dir(&journal_root).ok();
        cleanup_test_root(&root);
    }

    /// Child entry point for the parent crash-recovery test below. The fault
    /// switch is compiled only into the Rust test harness, never the launcher.
    #[test]
    #[ignore = "spawned by process_crash_recovery_converges_at_every_commit_checkpoint"]
    fn transaction_crash_helper() {
        let source_root = PathBuf::from(
            std::env::var_os("OXO_TEST_SOURCE_ROOT").expect("source root env"),
        );
        let target_root = PathBuf::from(
            std::env::var_os("OXO_TEST_TARGET_ROOT").expect("target root env"),
        );
        let journal_root = PathBuf::from(
            std::env::var_os("OXO_TEST_JOURNAL_ROOT").expect("journal root env"),
        );
        let transaction_id = std::env::var("OXO_TEST_TRANSACTION_ID")
            .expect("transaction id env")
            .parse::<Uuid>()
            .expect("valid transaction id");
        let source = source_root.join("save.bin");
        let target = target_root.join("save.bin");
        let expected_sha256 = sha256_file(&source).expect("source hash");

        let outcome = apply_changes_at_with_id(
            &journal_root,
            transaction_id,
            "process-crash-helper",
            vec![ManagedFileChange::Replace(ManagedFileSpec {
                source,
                target,
                allowed_source_root: source_root,
                allowed_target_root: target_root,
                expected_sha256,
            })],
        );
        panic!("transaction fault hook did not abort: {outcome:?}");
    }

    #[test]
    fn process_crash_recovery_converges_at_every_commit_checkpoint() {
        use std::process::Command;

        for checkpoint in [
            "prepared",
            "backupRenamed",
            "replacementRenamed",
            "committed",
        ] {
            let root = test_root(&format!("real-process-crash-{checkpoint}"));
            let source_root = root.join("source");
            let target_root = root.join("target");
            let journal_root = root.join("journal");
            fs::create_dir_all(&source_root).unwrap();
            fs::create_dir_all(&target_root).unwrap();
            let source = source_root.join("save.bin");
            let target = target_root.join("save.bin");
            fs::write(&source, b"intended-replacement").unwrap();
            fs::write(&target, b"exact-original").unwrap();
            let transaction_id = Uuid::new_v4();

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .arg("--ignored")
                .arg("--exact")
                .arg("managed_file_transaction::tests::transaction_crash_helper")
                .arg("--test-threads=1")
                .env("OXO_TEST_SOURCE_ROOT", &source_root)
                .env("OXO_TEST_TARGET_ROOT", &target_root)
                .env("OXO_TEST_JOURNAL_ROOT", &journal_root)
                .env("OXO_TEST_TRANSACTION_ID", transaction_id.to_string())
                .env("OXO_TEST_TRANSACTION_ABORT_AT", checkpoint)
                .output()
                .expect("spawn transaction helper");
            assert!(
                !output.status.success(),
                "checkpoint {checkpoint} must terminate the helper"
            );

            assert_eq!(recover_pending_at(&journal_root).unwrap(), 1);
            let expected = if checkpoint == "committed" {
                b"intended-replacement".as_slice()
            } else {
                b"exact-original".as_slice()
            };
            assert_eq!(fs::read(&target).unwrap(), expected, "{checkpoint}");
            assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);

            fs::remove_dir(&journal_root).ok();
            cleanup_test_root(&root);
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires the disposable NTFS VHDX created by scripts/test-managed-transaction-full-disk.ps1"]
    fn full_disk_transaction_helper() {
        let volume_root = PathBuf::from(
            std::env::var_os("OXO_TEST_FULL_DISK_ROOT").expect("disposable volume root"),
        );
        let canonical_root = volume_root.canonicalize().expect("canonical test volume");
        assert_eq!(
            fs::read_to_string(canonical_root.join(".0xo-disposable-full-disk-test")).unwrap(),
            "0xo-disposable-full-disk-test-v1"
        );
        assert!(fs2::total_space(&canonical_root).unwrap() <= 512 * 1024 * 1024);
        let mode = std::env::var("OXO_TEST_FULL_DISK_MODE").expect("full-disk mode");
        assert!(matches!(mode.as_str(), "preflight" | "postPreflight"));

        let case_root = canonical_root.join(format!("0xo-full-disk-{mode}"));
        let source_root = case_root.join("source");
        let target_root = case_root.join("target");
        let journal_root = case_root.join("journal");
        fs::create_dir(&case_root).unwrap();
        fs::create_dir(&source_root).unwrap();
        fs::create_dir(&target_root).unwrap();
        fs::create_dir(&journal_root).unwrap();
        let source = source_root.join("save.bin");
        let target = target_root.join("save.bin");
        let filler = target_root.join("exact-full-disk-filler.bin");

        let mut source_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&source)
            .unwrap();
        let source_block = vec![0x5A; 1024 * 1024];
        for _ in 0..8 {
            source_file.write_all(&source_block).unwrap();
        }
        source_file.sync_all().unwrap();
        drop(source_file);
        fs::write(&target, b"exact-original").unwrap();
        let expected_sha256 = sha256_file(&source).unwrap();
        let required_bytes = fs::metadata(&source).unwrap().len();

        if mode == "preflight" {
            let mut filler_file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&filler)
                .unwrap();
            let block = vec![0xC3; 1024 * 1024];
            while fs2::available_space(&target_root).unwrap() >= required_bytes {
                if filler_file.write_all(&block).is_err() {
                    break;
                }
            }
            let _ = filler_file.sync_all();
            drop(filler_file);
        } else {
            std::env::set_var("OXO_TEST_FILL_DISK_AFTER_PREFLIGHT", &filler);
        }

        let error = apply_files_at(
            &journal_root,
            &format!("full-disk-{mode}"),
            vec![ManagedFileSpec {
                source: source.clone(),
                target: target.clone(),
                allowed_source_root: source_root.clone(),
                allowed_target_root: target_root.clone(),
                expected_sha256,
            }],
        )
        .expect_err("a genuinely full disposable volume must reject the transaction");
        std::env::remove_var("OXO_TEST_FILL_DISK_AFTER_PREFLIGHT");

        assert!(
            error.contains("INSUFFICIENT_SPACE")
                || error.contains("failed to write stage")
                || error.contains("failed to create stage"),
            "unexpected full-disk error: {error}"
        );
        assert_eq!(fs::read(&target).unwrap(), b"exact-original");
        assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
        assert!(!fs::read_dir(&target_root).unwrap().any(|entry| {
            entry
                .ok()
                .and_then(|item| item.file_name().to_str().map(str::to_string))
                .is_some_and(|name| name.starts_with(".0xo-stage-") || name.starts_with(".0xo-backup-"))
        }));

        fs::remove_file(&filler).ok();
        fs::remove_file(source).ok();
        fs::remove_file(target).ok();
        fs::remove_dir(source_root).ok();
        fs::remove_dir(target_root).ok();
        fs::remove_dir(journal_root).ok();
        fs::remove_dir(case_root).ok();
    }

    #[cfg(windows)]
    #[test]
    fn windows_exclusive_file_locks_fail_bounded_and_preserve_original() {
        use std::os::windows::fs::OpenOptionsExt;

        fn open_exclusive(path: &Path) -> File {
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(path)
                .expect("exclusive Win32 file handle")
        }

        let root = test_root("real-win32-file-locks");
        let source_root = root.join("source");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        fs::create_dir_all(&journal_root).unwrap();
        let source = source_root.join("save.bin");
        let target = target_root.join("save.bin");
        let source_hash = write_and_hash(&source, b"replacement");
        fs::write(&target, b"exact-original").unwrap();

        let source_lock = open_exclusive(&source);
        let source_error = apply_files_at(
            &journal_root,
            "locked-source",
            vec![ManagedFileSpec {
                source: source.clone(),
                target: target.clone(),
                allowed_source_root: source_root.clone(),
                allowed_target_root: target_root.clone(),
                expected_sha256: source_hash.clone(),
            }],
        )
        .unwrap_err();
        assert!(!source_error.is_empty());
        drop(source_lock);
        assert_eq!(fs::read(&target).unwrap(), b"exact-original");

        let target_lock = open_exclusive(&target);
        let target_error = apply_files_at(
            &journal_root,
            "locked-target",
            vec![ManagedFileSpec {
                source: source.clone(),
                target: target.clone(),
                allowed_source_root: source_root.clone(),
                allowed_target_root: target_root.clone(),
                expected_sha256: source_hash,
            }],
        )
        .unwrap_err();
        assert!(target_error.contains("rename") || target_error.contains("resolve"));
        drop(target_lock);
        assert_eq!(fs::read(&target).unwrap(), b"exact-original");

        for locked_name in ["stage.bin", "backup.bin"] {
            let locked_path = target_root.join(locked_name);
            let destination = target_root.join(format!("{locked_name}.moved"));
            fs::write(&locked_path, b"locked-artifact").unwrap();
            let lock = open_exclusive(&locked_path);
            assert!(rename_same_volume(&locked_path, &destination).is_err());
            drop(lock);
            assert_eq!(fs::read(&locked_path).unwrap(), b"locked-artifact");
            fs::remove_file(locked_path).ok();
        }

        let journal_path = journal_root.join("locked-journal.json");
        let mut journal = TransactionJournal {
            schema_version: TRANSACTION_SCHEMA_VERSION,
            transaction_id: "locked-journal".to_string(),
            operation: "lock-test".to_string(),
            state: TransactionState::Prepared,
            files: Vec::new(),
        };
        persist_journal(&journal_path, &journal).unwrap();
        let journal_lock = open_exclusive(&journal_path);
        journal.state = TransactionState::Committing;
        let journal_error = persist_journal(&journal_path, &journal).unwrap_err();
        assert!(journal_error.contains("atomically replace"));
        drop(journal_lock);
        assert_eq!(
            serde_json::from_slice::<TransactionJournal>(&fs::read(&journal_path).unwrap())
                .unwrap()
                .state,
            TransactionState::Prepared
        );

        fs::remove_file(journal_path.with_extension("json.next")).ok();
        fs::remove_file(journal_path).ok();
        fs::remove_dir(journal_root).ok();
        cleanup_test_root(&root);
    }

    #[test]
    fn duplicate_targets_are_rejected_before_the_first_rename() {
        let root = test_root("duplicate-target");
        let source_root = root.join("source");
        let target_root = root.join("target");
        let journal_root = root.join("journal");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        let first = source_root.join("first.bin");
        let second = source_root.join("second.bin");
        let target = target_root.join("save.bin");
        let first_hash = write_and_hash(&first, b"first");
        let second_hash = write_and_hash(&second, b"second");
        fs::write(&target, b"original").unwrap();

        let result = apply_files_at(
            &journal_root,
            "duplicate-target",
            vec![
                ManagedFileSpec {
                    source: first.clone(),
                    target: target.clone(),
                    allowed_source_root: source_root.clone(),
                    allowed_target_root: target_root.clone(),
                    expected_sha256: first_hash,
                },
                ManagedFileSpec {
                    source: second.clone(),
                    target: target.clone(),
                    allowed_source_root: source_root.clone(),
                    allowed_target_root: target_root.clone(),
                    expected_sha256: second_hash,
                },
            ],
        );
        assert!(result.unwrap_err().contains("duplicate target"));
        assert_eq!(fs::read(&target).unwrap(), b"original");

        for path in [first, second, target] {
            fs::remove_file(path).ok();
        }
        fs::remove_dir(source_root).ok();
        fs::remove_dir(target_root).ok();
        fs::remove_dir(journal_root).ok();
        cleanup_test_root(&root);
    }
}
