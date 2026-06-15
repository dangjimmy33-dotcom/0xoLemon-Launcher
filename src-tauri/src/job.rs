use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, RANGE, USER_AGENT};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use walkdir::WalkDir;

use crate::manifest::{Catalog, ChunkRef, FileEntry, VersionManifest};
use crate::scanner::{safe_join, scan_install};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DEFAULT_DEPOT_BASE: &str =
    "https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main/007-first-light";
const DEFAULT_LOCAL_DEPOT: &str = "E:\\007Launcher\\depot\\007-first-light";
const DEFAULT_GAME_ID: &str = "007-first-light";
const DEFAULT_GAME_DIR_NAME: &str = "007 First Light";
const DEFAULT_STORE_ROOT: &str = "E:\\0xoLemon store";
const INSTALL_MARKER_DIR: &str = ".0xolemon";
const INSTALL_MARKER_FILE: &str = "state.0xo";
const LEGACY_INSTALL_MARKER_FILE: &str = "install.json";
const INSTALLED_MANIFEST_FILE: &str = "manifest.0xo";
const DOWNLOAD_SESSION_FILE: &str = "depot-session.json";
const STATE_MAGIC: &[u8] = b"0XOSTATE1\n";
const STATE_KEY: &[u8] = b"0xoLemon-local-install-state-v1";
const DEFAULT_DOWNLOAD_WORKERS: usize = 8;
const MAX_DOWNLOAD_WORKERS: usize = 64;
const DEFAULT_DOWNLOAD_RETRIES: u32 = 5;
const MAX_DOWNLOAD_RETRIES: u32 = 12;
const PACK_RANGE_MERGE_GAP: u64 = 4 * 1024 * 1024;
const DEFAULT_PACK_RANGE_TASK_BYTES: u64 = 16 * 1024 * 1024;
const MIN_PACK_RANGE_TASK_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PACK_RANGE_TASK_BYTES: u64 = 64 * 1024 * 1024;
const VERIFY_PROGRESS_EVENT: &str = "launcher://verify-progress";
const VERIFY_READ_BUFFER_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct JobControl {
    paused: AtomicBool,
    canceled: AtomicBool,
}

fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

impl JobControl {
    pub fn reset(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.canceled.store(false, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path error: {0}")]
    Path(#[from] tauri::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("depot error: {0}")]
    Depot(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSnapshot {
    pub current_version: String,
    pub latest_version: String,
    pub available_versions: Vec<String>,
    pub detected_install_path: Option<String>,
    pub update_size: u64,
    pub proxy_status: String,
    pub cache: CacheSnapshot,
    pub changed_files: Vec<ChangedFile>,
    pub last_job: Option<JobJournal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSnapshot {
    pub cache_size: u64,
    pub free_space: u64,
    pub health_percent: u8,
    pub rollback_ready: bool,
    pub rollback_missing_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInstallState {
    pub game_id: String,
    pub installed: bool,
    pub current_version: String,
    pub install_path: String,
    pub launch_executable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInstallReport {
    pub ok: bool,
    pub checked_files: usize,
    pub missing_files: Vec<String>,
    pub mismatched_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyProgressEvent {
    pub game_id: String,
    pub phase: String,
    pub current_file: Option<String>,
    pub checked_files: usize,
    pub total_files: usize,
    pub checked_bytes: u64,
    pub total_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub game_id: String,
    pub removed_files: usize,
    pub removed_dirs: usize,
    pub install_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReport {
    pub game_id: String,
    pub executable: String,
    pub shortcut_path: Option<String>,
    pub dependencies_installed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobJournal {
    pub id: String,
    #[serde(default = "default_game_id_string")]
    pub game_id: String,
    pub kind: String,
    pub status: JobStatus,
    pub install_path: String,
    pub from_version: String,
    pub to_version: String,
    pub phase: String,
    pub overall_progress: f32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub retry_count: u32,
    pub resumable: bool,
    pub updated_at: String,
    pub steps: Vec<JobStep>,
    pub logs: Vec<JobLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Planned,
    Running,
    Paused,
    Downloading,
    Assembling,
    Verified,
    Committed,
    Canceled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStep {
    pub name: String,
    pub detail: String,
    pub status: StepStatus,
    pub progress: f32,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Waiting,
    Running,
    Completed,
    Paused,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLog {
    pub at: String,
    pub level: String,
    pub message: String,
}

pub fn snapshot(app: &AppHandle) -> Result<LauncherSnapshot, JobError> {
    let source = DepotSource::from_env();
    // Try local catalog first (fast), then fall back to remote HF catalog
    let catalog = source.load_local_catalog().ok().or_else(|| source.load_catalog().ok());
    let latest_version = catalog
        .as_ref()
        .and_then(|catalog| catalog.effective_latest_version().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let available_versions = catalog_versions(catalog.as_ref());
    let cache_size = persistent_cache_size(app).unwrap_or(0);
    let default_install = default_common_game_dir();
    let marker = read_install_marker(&default_install).ok().flatten();

    let (current_version, update_size, changed_files, detected_install_path) =
        match (catalog.as_ref(), marker) {
            (Some(catalog), Some(marker)) => {
                let target_version = catalog.effective_latest_version().unwrap_or("unknown").to_string();
                let (changed_files, update_size) = if marker.version == target_version {
                    (Vec::new(), 0)
                } else if catalog_has_version(catalog, &marker.version) {
                    let (from, to) =
                        load_manifest_pair(&source, catalog, &marker.version, &target_version)?;
                    (
                        changed_files_between(&from, &to),
                        estimate_missing_download_bytes(None, &from, &to),
                    )
                } else {
                    (Vec::new(), 0)
                };
                (
                    marker.version,
                    update_size,
                    changed_files,
                    Some(default_install.display().to_string()),
                )
            }
            (Some(catalog), None) => {
                let target_manifest = source.load_manifest(catalog, &latest_version)?;
                (
                    "not installed".to_string(),
                    estimate_install_download_bytes(None, &target_manifest),
                    install_changed_files(&target_manifest),
                    None,
                )
            }
            _ => ("not installed".to_string(), 0, Vec::new(), None),
        };

    Ok(LauncherSnapshot {
        current_version,
        latest_version,
        available_versions,
        detected_install_path,
        update_size,
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            free_space: 0,
            health_percent: if cache_size > 0 { 100 } else { 0 },
            rollback_ready: false,
            rollback_missing_bytes: 0,
        },
        changed_files,
        last_job: read_latest_journal(app)?.filter(is_active_real_journal),
    })
}

pub fn snapshot_for_fresh_install(
    app: &AppHandle,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<LauncherSnapshot, JobError> {
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    let selected_version = resolve_target_version(&catalog, target_version)?;
    let manifest = source.load_manifest(&catalog, &selected_version)?;
    let cache_size = persistent_cache_size(app).unwrap_or(0);

    Ok(LauncherSnapshot {
        current_version: "not installed".to_string(),
        latest_version: catalog.effective_latest_version().unwrap_or("unknown").to_string(),
        available_versions: catalog_versions(Some(&catalog)),
        detected_install_path: None,
        update_size: estimate_install_download_bytes(None, &manifest),
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            free_space: 0,
            health_percent: if cache_size > 0 { 100 } else { 0 },
            rollback_ready: false,
            rollback_missing_bytes: 0,
        },
        changed_files: install_changed_files(&manifest),
        last_job: read_latest_journal(app)?.filter(is_active_real_journal),
    })
}

pub fn snapshot_for_install(
    app: &AppHandle,
    install_path: &Path,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<LauncherSnapshot, JobError> {
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let scan = scan_install(install_path).map_err(|err| JobError::Depot(err.to_string()))?;
    let current_version = scan
        .detected_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let catalog = source.load_catalog()?;
    let selected_version = resolve_target_version(&catalog, target_version)?;
    let latest_version = catalog.effective_latest_version().unwrap_or("unknown").to_string();
    let (changed_files, update_size) = if current_version == "unknown" {
        (Vec::new(), 0)
    } else if current_version == selected_version {
        (Vec::new(), 0)
    } else {
        let (from, to) =
            load_manifest_pair(&source, &catalog, &current_version, &selected_version)?;
        (
            changed_files_between(&from, &to),
            estimate_missing_download_bytes(None, &from, &to),
        )
    };
    let cache_size = persistent_cache_size(app).unwrap_or(0);

    Ok(LauncherSnapshot {
        current_version,
        latest_version,
        available_versions: catalog_versions(Some(&catalog)),
        detected_install_path: Some(install_path.display().to_string()),
        update_size,
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            free_space: 0,
            health_percent: if cache_size > 0 { 100 } else { 0 },
            rollback_ready: false,
            rollback_missing_bytes: 0,
        },
        changed_files,
        last_job: read_latest_journal(app)?.filter(is_active_real_journal),
    })
}

pub fn spawn_update_job(
    app: AppHandle,
    control: Arc<JobControl>,
    install_path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, target_version)?;
    let journal = default_journal(
        &source.game_id,
        "update",
        install_path,
        "detecting",
        &target_version,
        0,
    );
    persist_and_emit(&app, &journal)?;
    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    thread::spawn(move || {
        if let Err(err) = run_real_update_job(&app_for_thread, control, initial) {
            let mut failed = read_latest_journal(&app_for_thread)
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    default_journal(
                        DEFAULT_GAME_ID,
                        "update",
                        String::new(),
                        "detecting",
                        "unknown",
                        0,
                    )
                });
            failed.status = JobStatus::Failed;
            failed.phase = "Failed".to_string();
            mark_running_step_failed(&mut failed);
            append_log(&mut failed, "error", &err.to_string());
            let _ = persist_and_emit(&app_for_thread, &failed);
        }
    });

    Ok(return_journal)
}

pub fn spawn_install_job(
    app: AppHandle,
    control: Arc<JobControl>,
    target_version: Option<String>,
    install_path: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, target_version)?;
    let install_root = resolve_install_root(install_path, &source);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&downloading_root)?;

    let manifest = source.load_manifest(&catalog, &target_version)?;
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    let initial_bytes = estimate_install_download_bytes(Some(&staged_chunks_root), &manifest);
    let journal = default_journal(
        &source.game_id,
        "install",
        install_root.display().to_string(),
        "not installed",
        &target_version,
        initial_bytes,
    );
    persist_and_emit(&app, &journal)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "planned",
        install_root.display().to_string(),
    )?;

    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    thread::spawn(move || {
        if let Err(err) = run_real_install_job(&app_for_thread, control, initial) {
            let mut failed = read_latest_journal(&app_for_thread)
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    default_journal(
                        DEFAULT_GAME_ID,
                        "install",
                        String::new(),
                        "not installed",
                        "unknown",
                        0,
                    )
                });
            failed.status = JobStatus::Failed;
            failed.phase = "Failed".to_string();
            mark_running_step_failed(&mut failed);
            append_log(&mut failed, "error", &err.to_string());
            let _ = persist_and_emit(&app_for_thread, &failed);
        }
    });

    Ok(return_journal)
}

pub fn spawn_repair_job(
    app: AppHandle,
    control: Arc<JobControl>,
    game_id: &str,
    install_path: String,
    target_version: Option<String>,
    file_paths: Vec<String>,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(game_id);
    let install_root = PathBuf::from(install_path);
    let marker =
        read_install_marker(&install_root)?.filter(|marker| marker.game_id == source.game_id);
    let catalog = source.load_catalog()?;
    let version = target_version
        .filter(|value| !value.trim().is_empty() && value != "not installed")
        .or_else(|| marker.as_ref().map(|marker| marker.version.clone()))
        .unwrap_or_else(|| catalog.effective_latest_version().unwrap_or("unknown").to_string());
    let version = resolve_target_version(&catalog, Some(version))?;
    let target_manifest = source.load_manifest(&catalog, &version)?;
    let requested = file_paths
        .into_iter()
        .map(|path| path.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    if requested.is_empty() {
        return Err(JobError::Depot("no files selected for repair".to_string()));
    }
    let repair_files = target_manifest
        .files
        .iter()
        .filter(|file| requested.contains(&file.path.to_ascii_lowercase()))
        .cloned()
        .collect::<Vec<_>>();
    if repair_files.is_empty() {
        return Err(JobError::Depot(
            "selected repair files are not in the manifest".to_string(),
        ));
    }

    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&staged_chunks_root)?;
    let missing_chunks = plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &repair_files)?;
    let bytes_total = download_transfer_bytes(&missing_chunks);
    let mut journal = default_journal(
        &source.game_id,
        "repair",
        install_root.display().to_string(),
        marker
            .as_ref()
            .map(|marker| marker.version.as_str())
            .unwrap_or("unknown"),
        &version,
        bytes_total,
    );
    append_log(
        &mut journal,
        "info",
        &format!(
            "Repair planned for {} files, {} chunks need network/staging work ({})",
            repair_files.len(),
            missing_chunks.len(),
            human_bytes(bytes_total)
        ),
    );
    persist_and_emit(&app, &journal)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "planned",
        install_root.display().to_string(),
    )?;

    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();
    thread::spawn(move || {
        if let Err(err) = run_real_repair_job(
            &app_for_thread,
            control,
            initial,
            repair_files,
            target_manifest,
        ) {
            let mut failed = read_latest_journal(&app_for_thread)
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    default_journal(
                        &source.game_id,
                        "repair",
                        String::new(),
                        "unknown",
                        "unknown",
                        0,
                    )
                });
            failed.status = JobStatus::Failed;
            failed.phase = "Failed".to_string();
            mark_running_step_failed(&mut failed);
            append_log(&mut failed, "error", &err.to_string());
            let _ = persist_and_emit(&app_for_thread, &failed);
        }
    });

    Ok(return_journal)
}

pub fn game_install_state(game_id: &str) -> Result<GameInstallState, JobError> {
    let source = DepotSource::for_game(game_id);
    let install_root = source.default_common_game_dir();
    let marker = read_install_marker(&install_root).ok().flatten();
    let marker = marker.filter(|marker| marker.game_id == source.game_id);
    let launch_executable = marker
        .as_ref()
        .and_then(|marker| marker.launch_executable.clone())
        .unwrap_or_else(|| default_launch_executable(&source.game_id));
    if let Some(marker) = marker.as_ref() {
        write_sanitized_install_marker(&install_root, &source, marker, &launch_executable)?;
        let downloading_root = downloading_dir_for_install(&install_root, &source);
        let _ = cleanup_committed_download_session(&downloading_root, &source);
        if read_installed_manifest(&install_root)?.is_none() {
            if let Ok(catalog) = source.load_local_catalog() {
                if let Ok(manifest) = source.load_local_manifest(&catalog, &marker.version) {
                    let _ = write_installed_manifest(&install_root, &manifest);
                }
            }
        }
    }

    Ok(GameInstallState {
        game_id: source.game_id,
        installed: marker.is_some() && install_root.exists(),
        current_version: marker
            .as_ref()
            .map(|marker| marker.version.clone())
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| "not installed".to_string()),
        install_path: install_root.display().to_string(),
        launch_executable,
    })
}

pub fn launch_game(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    launch_executable: Option<String>,
) -> Result<LaunchReport, JobError> {
    let source = DepotSource::for_game(game_id);
    let marker = read_install_marker(install_path)?
        .ok_or_else(|| JobError::Depot(format!("{} is not installed", source.game_dir_name)))?;
    if marker.game_id != source.game_id {
        return Err(JobError::Depot(format!(
            "install marker belongs to {}, not {}",
            marker.game_id, source.game_id
        )));
    }
    let relative_exe = launch_executable
        .filter(|value| !value.trim().is_empty())
        .or(marker.launch_executable)
        .unwrap_or_else(|| default_launch_executable(&source.game_id));
    let executable = safe_join(install_path, &relative_exe)
        .ok_or_else(|| JobError::Depot(format!("unsafe executable path: {relative_exe}")))?;
    if !executable.exists() {
        return Err(JobError::Depot(format!(
            "game executable is missing: {}",
            executable.display()
        )));
    }
    let dependencies_installed = ensure_game_dependencies(app, &source)?;
    let shortcut_path =
        create_game_shortcut(app, &source, install_path, &executable, &relative_exe)
            .ok()
            .flatten()
            .map(|path| path.display().to_string());
    launch_executable_elevated(&executable)?;
    Ok(LaunchReport {
        game_id: source.game_id,
        executable: executable.display().to_string(),
        shortcut_path,
        dependencies_installed,
    })
}

#[derive(Debug, Clone, Copy)]
enum DependencyArch {
    X64,
    X86,
}

#[derive(Debug, Clone, Copy)]
struct DependencySpec {
    _id: &'static str,
    display_name: &'static str,
    arch: DependencyArch,
    url: &'static str,
    file_name: &'static str,
}

fn dependency_specs_for_game(game_id: &str) -> Vec<DependencySpec> {
    const VC_REDIST_X64: DependencySpec = DependencySpec {
        _id: "vc-redist-x64",
        display_name: "Microsoft Visual C++ Redistributable x64",
        arch: DependencyArch::X64,
        url: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        file_name: "vc_redist.x64.exe",
    };
    const VC_REDIST_X86: DependencySpec = DependencySpec {
        _id: "vc-redist-x86",
        display_name: "Microsoft Visual C++ Redistributable x86",
        arch: DependencyArch::X86,
        url: "https://aka.ms/vs/17/release/vc_redist.x86.exe",
        file_name: "vc_redist.x86.exe",
    };

    match game_id {
        "among-us" => vec![VC_REDIST_X64, VC_REDIST_X86],
        DEFAULT_GAME_ID => vec![VC_REDIST_X64],
        _ => vec![VC_REDIST_X64],
    }
}

fn ensure_game_dependencies(
    app: &AppHandle,
    source: &DepotSource,
) -> Result<Vec<String>, JobError> {
    let mut installed = Vec::new();
    for spec in dependency_specs_for_game(&source.game_id) {
        if dependency_installed(spec) {
            continue;
        }
        let installer = download_dependency_installer(app, spec)?;
        run_elevated(
            &installer,
            &["/install", "/quiet", "/norestart"],
            installer.parent(),
            true,
        )?;
        installed.push(spec.display_name.to_string());
    }
    Ok(installed)
}

fn download_dependency_installer(
    app: &AppHandle,
    spec: DependencySpec,
) -> Result<PathBuf, JobError> {
    let redist_dir = app.path().app_data_dir()?.join("redist");
    fs::create_dir_all(&redist_dir)?;
    let destination = redist_dir.join(spec.file_name);
    if destination
        .metadata()
        .map(|metadata| metadata.len() > 512 * 1024)
        .unwrap_or(false)
    {
        return Ok(destination);
    }

    let temp = destination.with_extension("download");
    let mut response = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(240))
        .build()
        .unwrap_or_else(|_| Client::new())
        .get(spec.url)
        .header(USER_AGENT, "0xoLemon-launcher-redist/0.1")
        .send()?
        .error_for_status()?;
    let mut file = File::create(&temp)?;
    response.copy_to(&mut file)?;
    file.flush()?;
    fs::rename(temp, &destination)?;
    Ok(destination)
}

fn dependency_installed(spec: DependencySpec) -> bool {
    #[cfg(target_os = "windows")]
    {
        let paths: &[&str] = match spec.arch {
            DependencyArch::X64 => &[
                r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            ],
            DependencyArch::X86 => &[
                r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            ],
        };
        return paths.iter().any(|path| {
            hidden_command("reg.exe")
                .args(["query", path, "/v", "Installed"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| {
                    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                    text.contains("0x1") || text.split_whitespace().any(|part| part == "1")
                })
                .unwrap_or(false)
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = spec;
        true
    }
}

fn create_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    executable: &Path,
    relative_executable: &str,
) -> Result<Option<PathBuf>, JobError> {
    if !executable.exists() {
        return Ok(None);
    }
    #[cfg(target_os = "windows")]
    {
        let desktop = env::var("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join("Desktop"))
            .unwrap_or_else(|_| {
                app.path()
                    .app_data_dir()
                    .unwrap_or_else(|_| install_root.to_path_buf())
            });
        fs::create_dir_all(&desktop)?;
        let shortcut_path = desktop.join(format!("{}.lnk", source.game_dir_name));
        let launcher_exe = std::env::current_exe().unwrap_or_else(|_| executable.to_path_buf());
        let app_data = app.path().app_data_dir().unwrap_or_else(|_| install_root.to_path_buf());
        let _ = fs::create_dir_all(&app_data);
        let bootstrap_exe = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
        if !bootstrap_exe.exists() {
            let _ = fs::hard_link(&launcher_exe, &bootstrap_exe)
                .or_else(|_| fs::copy(&launcher_exe, &bootstrap_exe).map(|_| ()));
        }
        let icon_location = format!("{},0", executable.display());
        let working_dir = launcher_exe.parent().unwrap_or(install_root);
        let arguments = shortcut_argument_line(&[
            ("--launch-game", &source.game_id),
            ("--install-path", &install_root.display().to_string()),
            ("--launch-executable", relative_executable),
        ]);
        let script = format!(
            "$shell = New-Object -ComObject WScript.Shell; \
             $shortcut = $shell.CreateShortcut({}); \
             $shortcut.TargetPath = {}; \
             $shortcut.Arguments = {}; \
             $shortcut.WorkingDirectory = {}; \
             $shortcut.IconLocation = {}; \
             $shortcut.Description = {}; \
             $shortcut.Save()",
            ps_quote(&shortcut_path.display().to_string()),
            ps_quote(&bootstrap_exe.display().to_string()),
            ps_quote(&arguments),
            ps_quote(&working_dir.display().to_string()),
            ps_quote(&icon_location),
            ps_quote(&format!("Launch {}", source.game_dir_name)),
        );
        let status = hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()?;
        if status.success() {
            Ok(Some(shortcut_path))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root, executable, relative_executable);
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn shortcut_argument_line(args: &[(&str, &str)]) -> String {
    args.iter()
        .flat_map(|(flag, value)| [(*flag).to_string(), win_arg_quote(value)])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn win_arg_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '\\' | '/')
        })
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

fn launch_executable_elevated(executable: &Path) -> Result<(), JobError> {
    run_elevated(executable, &[], executable.parent(), false)
}

fn run_elevated(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
) -> Result<(), JobError> {
    #[cfg(target_os = "windows")]
    {
        run_elevated_windows(executable, args, working_dir, wait)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new(executable);
        command.args(args);
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        if wait {
            command.status()?;
        } else {
            command.spawn()?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn run_elevated_windows(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
) -> Result<(), JobError> {
    let mut script = format!(
        "$p = Start-Process -FilePath {} -Verb RunAs -WindowStyle Normal",
        ps_quote_os(executable.as_os_str())
    );
    if let Some(dir) = working_dir {
        script.push_str(&format!(
            " -WorkingDirectory {}",
            ps_quote_os(dir.as_os_str())
        ));
    }
    if !args.is_empty() {
        let quoted_args = args
            .iter()
            .map(|arg| ps_quote(arg))
            .collect::<Vec<_>>()
            .join(", ");
        script.push_str(&format!(" -ArgumentList @({quoted_args})"));
    }
    if wait {
        script.push_str(" -Wait -PassThru; if ($p.ExitCode -ne 0) { exit $p.ExitCode }");
    }

    let status = hidden_command("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(JobError::Depot(
            "admin launch was canceled or failed".to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
fn ps_quote_os(value: &OsStr) -> String {
    ps_quote(&value.to_string_lossy())
}

pub fn verify_install_integrity(
    app: Option<&AppHandle>,
    game_id: &str,
    install_path: &Path,
    target_version: Option<String>,
) -> Result<VerifyInstallReport, JobError> {
    let source = DepotSource::for_game(game_id);
    let marker =
        read_install_marker(install_path)?.filter(|marker| marker.game_id == source.game_id);
    let version = target_version
        .filter(|value| !value.trim().is_empty() && value != "not installed")
        .or_else(|| marker.as_ref().map(|marker| marker.version.clone()))
        .unwrap_or_else(|| {
            source
                .load_local_catalog()
                .ok()
                .and_then(|catalog| catalog.effective_latest_version().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string())
        });
    let manifest = if marker
        .as_ref()
        .is_some_and(|marker| marker.version == version)
    {
        match read_installed_manifest(install_path)? {
            Some(manifest) => manifest,
            None => load_manifest_for_version(&source, &version)?,
        }
    } else {
        load_manifest_for_version(&source, &version)?
    };
    let mut missing_files = Vec::new();
    let mut mismatched_files = Vec::new();
    let mut checked_files = 0_usize;
    let total_files = manifest.files.len();
    let total_bytes = manifest.files.iter().map(|file| file.size).sum::<u64>();
    let mut checked_bytes = 0_u64;

    emit_verify_progress(
        app,
        VerifyProgressEvent {
            game_id: source.game_id.clone(),
            phase: "Preparing verify".to_string(),
            current_file: None,
            checked_files,
            total_files,
            checked_bytes,
            total_bytes,
            percent: verify_percent(checked_bytes, total_bytes, checked_files, total_files),
        },
    )?;

    for file in &manifest.files {
        let Some(path) = safe_join(install_path, &file.path) else {
            mismatched_files.push(file.path.clone());
            checked_files += 1;
            checked_bytes = checked_bytes.saturating_add(file.size).min(total_bytes);
            emit_verify_progress(
                app,
                VerifyProgressEvent {
                    game_id: source.game_id.clone(),
                    phase: "Invalid manifest path".to_string(),
                    current_file: Some(file.path.clone()),
                    checked_files,
                    total_files,
                    checked_bytes,
                    total_bytes,
                    percent: verify_percent(checked_bytes, total_bytes, checked_files, total_files),
                },
            )?;
            continue;
        };
        if !path.exists() {
            missing_files.push(file.path.clone());
            checked_files += 1;
            checked_bytes = checked_bytes.saturating_add(file.size).min(total_bytes);
            emit_verify_progress(
                app,
                VerifyProgressEvent {
                    game_id: source.game_id.clone(),
                    phase: "Missing file".to_string(),
                    current_file: Some(file.path.clone()),
                    checked_files,
                    total_files,
                    checked_bytes,
                    total_bytes,
                    percent: verify_percent(checked_bytes, total_bytes, checked_files, total_files),
                },
            )?;
            continue;
        }
        let metadata = fs::metadata(&path)?;
        let mut current_checked_bytes = checked_bytes;
        emit_verify_progress(
            app,
            VerifyProgressEvent {
                game_id: source.game_id.clone(),
                phase: "Hashing files".to_string(),
                current_file: Some(file.path.clone()),
                checked_files,
                total_files,
                checked_bytes,
                total_bytes,
                percent: verify_percent(checked_bytes, total_bytes, checked_files, total_files),
            },
        )?;
        let hash_matches = if metadata.len() == file.size {
            sha256_file_with_progress(&path, |read| {
                current_checked_bytes = current_checked_bytes.saturating_add(read).min(total_bytes);
                emit_verify_progress(
                    app,
                    VerifyProgressEvent {
                        game_id: source.game_id.clone(),
                        phase: "Hashing files".to_string(),
                        current_file: Some(file.path.clone()),
                        checked_files,
                        total_files,
                        checked_bytes: current_checked_bytes,
                        total_bytes,
                        percent: verify_percent(
                            current_checked_bytes,
                            total_bytes,
                            checked_files,
                            total_files,
                        ),
                    },
                )
            })? == file.sha256
        } else {
            current_checked_bytes = current_checked_bytes
                .saturating_add(file.size)
                .min(total_bytes);
            false
        };
        checked_bytes = current_checked_bytes;
        checked_files += 1;
        if !hash_matches {
            mismatched_files.push(file.path.clone());
        }
        emit_verify_progress(
            app,
            VerifyProgressEvent {
                game_id: source.game_id.clone(),
                phase: "Hashing files".to_string(),
                current_file: Some(file.path.clone()),
                checked_files,
                total_files,
                checked_bytes,
                total_bytes,
                percent: verify_percent(checked_bytes, total_bytes, checked_files, total_files),
            },
        )?;
    }

    emit_verify_progress(
        app,
        VerifyProgressEvent {
            game_id: source.game_id,
            phase: if missing_files.is_empty() && mismatched_files.is_empty() {
                "Verified".to_string()
            } else {
                "Verify failed".to_string()
            },
            current_file: None,
            checked_files,
            total_files,
            checked_bytes: total_bytes,
            total_bytes,
            percent: 1.0,
        },
    )?;

    Ok(VerifyInstallReport {
        ok: missing_files.is_empty() && mismatched_files.is_empty(),
        checked_files,
        missing_files,
        mismatched_files,
    })
}

pub fn uninstall_game(game_id: &str, install_path: &Path) -> Result<UninstallReport, JobError> {
    let source = DepotSource::for_game(game_id);
    let marker = read_install_marker(install_path)?
        .ok_or_else(|| JobError::Depot(format!("{} is not installed", source.game_dir_name)))?;
    if marker.game_id != source.game_id {
        return Err(JobError::Depot(format!(
            "install marker belongs to {}, not {}",
            marker.game_id, source.game_id
        )));
    }
    let manifest = match read_installed_manifest(install_path)? {
        Some(manifest) => manifest,
        None => load_manifest_for_version(&source, &marker.version)?,
    };
    let mut removed_files = 0_usize;
    let mut candidate_dirs = Vec::new();

    for file in &manifest.files {
        let Some(path) = safe_join(install_path, &file.path) else {
            continue;
        };
        if path.exists() && path.is_file() {
            fs::remove_file(&path)?;
            removed_files += 1;
        }
        if let Some(parent) = path.parent() {
            candidate_dirs.push(parent.to_path_buf());
        }
    }

    let marker_path = install_marker_path(install_path);
    if marker_path.exists() {
        fs::remove_file(&marker_path)?;
        removed_files += 1;
    }
    let manifest_path = installed_manifest_path(install_path);
    if manifest_path.exists() {
        fs::remove_file(&manifest_path)?;
        removed_files += 1;
    }
    let legacy_marker_path = legacy_install_marker_path(install_path);
    if legacy_marker_path.exists() {
        fs::remove_file(&legacy_marker_path)?;
        removed_files += 1;
    }
    if let Some(parent) = marker_path.parent() {
        candidate_dirs.push(parent.to_path_buf());
    }
    candidate_dirs.push(install_path.to_path_buf());
    candidate_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    candidate_dirs.dedup();

    let mut removed_dirs = 0_usize;
    for dir in candidate_dirs {
        if dir.starts_with(install_path) && fs::remove_dir(&dir).is_ok() {
            removed_dirs += 1;
        }
    }

    Ok(UninstallReport {
        game_id: source.game_id,
        removed_files,
        removed_dirs,
        install_path: install_path.display().to_string(),
    })
}

fn run_real_update_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    append_log(&mut journal, "info", "Real update job started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Scan")?;
    let scan = scan_install(&install_root).map_err(|err| JobError::Depot(err.to_string()))?;
    let from_version = scan.detected_version.ok_or_else(|| {
        JobError::Depot(
            "Cannot detect installed version safely; run repair/verify first".to_string(),
        )
    })?;
    journal.from_version = from_version.clone();
    append_log(
        &mut journal,
        "info",
        &format!("Scanned {} files, detected {from_version}", scan.file_count),
    );
    complete_step(app, &mut journal, 0)?;

    set_step_running(app, &mut journal, 1, JobStatus::Running, "Verify manifests")?;
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, Some(journal.to_version.clone()))?;
    journal.to_version = target_version.clone();
    let from_manifest = source.load_manifest(&catalog, &from_version)?;
    let target_manifest = source.load_manifest(&catalog, &target_version)?;
    let changed = changed_target_files(&from_manifest, &target_manifest);
    let local_sources = build_local_chunk_sources(&install_root, &from_manifest)?;
    fs::create_dir_all(&staged_chunks_root)?;
    let missing_chunks = plan_missing_chunks(&local_sources, &staged_chunks_root, &changed)?;
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    journal.bytes_done = 0;
    let planned_bytes = human_bytes(journal.bytes_total);
    append_log(
        &mut journal,
        "info",
        &format!(
            "{} changed files, {} chunks need network download ({})",
            changed.len(),
            missing_chunks.len(),
            planned_bytes
        ),
    );
    complete_step(app, &mut journal, 1)?;

    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download missing chunks",
    )?;
    let mut downloaded = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();
    source.download_chunks_to_store_parallel(
        &staged_chunks_root,
        &missing_chunks,
        Arc::clone(&control),
        |progress| {
            if progress.clear_in_flight {
                in_flight.remove(&progress.task_id);
            } else {
                in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
            }
            downloaded += progress.committed_bytes;
            wait_for_control(app, &control, &mut journal, 2)?;
            let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
            journal.bytes_done = display_done.min(journal.bytes_total);
            journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
            journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
            journal.overall_progress = overall_progress(2, journal.steps[2].progress);
            touch(&mut journal);
            persist_and_emit(app, &journal)
        },
    )?;
    complete_step(app, &mut journal, 2)?;

    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Assemble changed files",
    )?;
    for (index, file) in changed.iter().enumerate() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(&mut journal, "info", &format!("Assembling {}", file.path));
        assemble_target_file(
            &install_root,
            None,
            &staged_chunks_root,
            file,
            &local_sources,
        )?;
        journal.steps[3].progress = progress_fraction(index + 1, changed.len());
        journal.overall_progress = overall_progress(3, journal.steps[3].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)?;
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize")?;
    journal.status = JobStatus::Committed;
    journal.phase = "Committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    complete_step(app, &mut journal, 4)?;
    write_install_marker(app, &install_root, &target_manifest, &source)?;
    if let Err(err) = cleanup_staged_chunks(&staged_chunks_root, &target_manifest) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean staged chunks: {err}"),
        );
    }
    if let Err(err) = cleanup_empty_owned_download_dirs(&downloading_root) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean empty downloading folders: {err}"),
        );
    }
    append_log(&mut journal, "info", "Real update committed");
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

fn run_real_install_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let staging_root = downloading_root.join("files");
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    append_log(&mut journal, "info", "Real install job started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Prepare store")?;
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&staging_root)?;
    fs::create_dir_all(&staged_chunks_root)?;
    journal.install_path = install_root.display().to_string();
    append_log(
        &mut journal,
        "info",
        &format!("Installing to {}", install_root.display()),
    );
    append_log(
        &mut journal,
        "info",
        &format!("Staging downloads in {}", downloading_root.display()),
    );
    write_download_session_marker(
        &downloading_root,
        &journal,
        "preparing",
        install_root.display().to_string(),
    )?;
    complete_step(app, &mut journal, 0)?;

    set_step_running(app, &mut journal, 1, JobStatus::Running, "Verify manifests")?;
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, Some(journal.to_version.clone()))?;
    journal.to_version = target_version.clone();
    let target_manifest = source.load_manifest(&catalog, &target_version)?;
    let changed = target_manifest.files.clone();
    let local_sources = HashMap::new();
    let missing_chunks = plan_missing_chunks(&local_sources, &staged_chunks_root, &changed)?;
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    journal.bytes_done = 0;
    let planned_bytes = human_bytes(journal.bytes_total);
    append_log(
        &mut journal,
        "info",
        &format!(
            "{} files, {} chunks need network/staging work ({})",
            changed.len(),
            missing_chunks.len(),
            planned_bytes
        ),
    );
    write_download_session_marker(
        &downloading_root,
        &journal,
        "planned",
        install_root.display().to_string(),
    )?;
    complete_step(app, &mut journal, 1)?;

    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download missing chunks",
    )?;
    let mut downloaded = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();
    source.download_chunks_to_store_parallel(
        &staged_chunks_root,
        &missing_chunks,
        Arc::clone(&control),
        |progress| {
            if progress.clear_in_flight {
                in_flight.remove(&progress.task_id);
            } else {
                in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
            }
            downloaded += progress.committed_bytes;
            wait_for_control(app, &control, &mut journal, 2)?;
            let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
            journal.bytes_done = display_done.min(journal.bytes_total);
            journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
            journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
            journal.overall_progress = overall_progress(2, journal.steps[2].progress);
            touch(&mut journal);
            persist_and_emit(app, &journal)
        },
    )?;
    complete_step(app, &mut journal, 2)?;

    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Assemble install files",
    )?;
    for (index, file) in changed.iter().enumerate() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(&mut journal, "info", &format!("Assembling {}", file.path));
        assemble_target_file(
            &install_root,
            Some(&staging_root),
            &staged_chunks_root,
            file,
            &local_sources,
        )?;
        journal.steps[3].progress = progress_fraction(index + 1, changed.len());
        journal.overall_progress = overall_progress(3, journal.steps[3].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)?;
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize")?;
    write_install_marker(app, &install_root, &target_manifest, &source)?;
    journal.status = JobStatus::Committed;
    journal.phase = "Committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    complete_step(app, &mut journal, 4)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "committed",
        install_root.display().to_string(),
    )?;
    if let Err(err) = cleanup_staged_chunks(&staged_chunks_root, &target_manifest) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean staged chunks: {err}"),
        );
    }
    if let Err(err) = cleanup_committed_download_session(&downloading_root, &source) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean completed download session: {err}"),
        );
    }
    append_log(&mut journal, "info", "Real install committed");
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

fn run_real_repair_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
    repair_files: Vec<FileEntry>,
    target_manifest: VersionManifest,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    append_log(&mut journal, "info", "Real repair job started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Prepare repair")?;
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&staged_chunks_root)?;
    append_log(
        &mut journal,
        "info",
        &format!("Repairing {} manifest-owned files", repair_files.len()),
    );
    complete_step(app, &mut journal, 0)?;

    set_step_running(app, &mut journal, 1, JobStatus::Running, "Plan repair")?;
    let local_sources = HashMap::new();
    let missing_chunks = plan_missing_chunks(&local_sources, &staged_chunks_root, &repair_files)?;
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    journal.bytes_done = 0;
    let total_repair_bytes = journal.bytes_total;
    append_log(
        &mut journal,
        "info",
        &format!(
            "{} chunks need network/staging work ({})",
            missing_chunks.len(),
            human_bytes(total_repair_bytes)
        ),
    );
    write_download_session_marker(
        &downloading_root,
        &journal,
        "planned",
        install_root.display().to_string(),
    )?;
    complete_step(app, &mut journal, 1)?;

    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download repair chunks",
    )?;
    let mut downloaded = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();
    source.download_chunks_to_store_parallel(
        &staged_chunks_root,
        &missing_chunks,
        Arc::clone(&control),
        |progress| {
            if progress.clear_in_flight {
                in_flight.remove(&progress.task_id);
            } else {
                in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
            }
            downloaded += progress.committed_bytes;
            wait_for_control(app, &control, &mut journal, 2)?;
            let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
            journal.bytes_done = display_done.min(journal.bytes_total);
            journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
            journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
            journal.overall_progress = overall_progress(2, journal.steps[2].progress);
            touch(&mut journal);
            persist_and_emit(app, &journal)
        },
    )?;
    complete_step(app, &mut journal, 2)?;

    set_step_running(app, &mut journal, 3, JobStatus::Assembling, "Repair files")?;
    for (index, file) in repair_files.iter().enumerate() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(&mut journal, "info", &format!("Repairing {}", file.path));
        assemble_target_file(
            &install_root,
            None,
            &staged_chunks_root,
            file,
            &local_sources,
        )?;
        journal.steps[3].progress = progress_fraction(index + 1, repair_files.len());
        journal.overall_progress = overall_progress(3, journal.steps[3].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)?;
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize repair")?;
    write_install_marker(app, &install_root, &target_manifest, &source)?;
    journal.status = JobStatus::Committed;
    journal.phase = "Repair committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    complete_step(app, &mut journal, 4)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "committed",
        install_root.display().to_string(),
    )?;
    if let Err(err) = cleanup_staged_chunks(&staged_chunks_root, &target_manifest) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean staged chunks: {err}"),
        );
    }
    if let Err(err) = cleanup_committed_download_session(&downloading_root, &source) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not clean completed repair session: {err}"),
        );
    }
    append_log(&mut journal, "info", "Real repair committed");
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

#[derive(Debug, Clone)]
struct DepotSource {
    game_id: String,
    game_dir_name: String,
    base_url: String,
    token: Option<String>,
    local_root: Option<PathBuf>,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallMarker {
    #[serde(default = "default_game_id_string")]
    game_id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    launch_executable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadSessionMarker {
    game_id: String,
    target_version: String,
    status: String,
    install_path: String,
    downloading_path: String,
    bytes_done: u64,
    bytes_total: u64,
    updated_at: String,
}

impl DepotSource {
    fn from_env() -> Self {
        Self::for_game(DEFAULT_GAME_ID)
    }

    fn for_game(game_id: &str) -> Self {
        let game_id = sanitize_game_id(game_id);
        let is_default = game_id == DEFAULT_GAME_ID;
        let local_root = if is_default {
            env::var("FIRST_LIGHT_LOCAL_DEPOT")
                .ok()
                .map(PathBuf::from)
                .or_else(|| {
                    let path = PathBuf::from(DEFAULT_LOCAL_DEPOT);
                    path.exists().then_some(path)
                })
        } else {
            let path = PathBuf::from(r"E:\007Launcher\depot").join(&game_id);
            path.exists().then_some(path)
        };
        let base_url = if is_default {
            env::var("FIRST_LIGHT_DEPOT_BASE").unwrap_or_else(|_| DEFAULT_DEPOT_BASE.to_string())
        } else {
            format!(
                "https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main/{}",
                game_id
            )
        };

        Self {
            game_dir_name: game_dir_name(&game_id).to_string(),
            game_id,
            base_url,
            token: env::var("FIRST_LIGHT_HF_TOKEN")
                .ok()
                .or_else(|| env::var("HF_TOKEN").ok()),
            local_root,
            client: Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    fn default_common_game_dir(&self) -> PathBuf {
        default_store_root()
            .join("common")
            .join(&self.game_dir_name)
    }

    fn default_downloading_game_dir(&self) -> PathBuf {
        default_store_root()
            .join("downloading")
            .join(&self.game_dir_name)
    }

    fn status_label(&self) -> String {
        match (&self.local_root, &self.token) {
            (Some(_), Some(_)) | (None, Some(_)) => "Content service ready".to_string(),
            (Some(_), None) => "Offline metadata ready".to_string(),
            (None, None) => "Remote content service ready".to_string(),
        }
    }

    fn load_catalog(&self) -> Result<Catalog, JobError> {
        self.load_json("catalog.json")
    }

    fn load_local_catalog(&self) -> Result<Catalog, JobError> {
        let root = self
            .local_root
            .as_ref()
            .ok_or_else(|| JobError::Depot("local depot is not configured".to_string()))?;
        let bytes = fs::read(root.join("catalog.json"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn load_local_manifest(
        &self,
        catalog: &Catalog,
        version: &str,
    ) -> Result<VersionManifest, JobError> {
        let root = self
            .local_root
            .as_ref()
            .ok_or_else(|| JobError::Depot("local depot is not configured".to_string()))?;
        let path = catalog
            .versions
            .iter()
            .find(|entry| entry.version == version)
            .map(|entry| entry.manifest_path.as_str())
            .ok_or_else(|| JobError::Depot(format!("version not found in catalog: {version}")))?;
        let bytes = fs::read(root.join(relative_to_path(path)))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn load_manifest(&self, catalog: &Catalog, version: &str) -> Result<VersionManifest, JobError> {
        let path = catalog
            .versions
            .iter()
            .find(|entry| entry.version == version)
            .map(|entry| entry.manifest_path.as_str())
            .ok_or_else(|| JobError::Depot(format!("version not found in catalog: {version}")))?;
        self.load_json(path)
    }

    fn load_json<T: for<'de> Deserialize<'de>>(&self, relative_path: &str) -> Result<T, JobError> {
        if let Some(root) = &self.local_root {
            let path = root.join(relative_to_path(relative_path));
            if path.exists() {
                let bytes = fs::read(path)?;
                return Ok(serde_json::from_slice(&bytes)?);
            }
        }

        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            relative_path.trim_start_matches('/')
        );
        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, "first-light-launcher/0.1");
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = request.send()?.error_for_status()?;
        Ok(response.json()?)
    }

    fn fetch_pack_span_with_progress(
        &self,
        pack_id: &str,
        start: u64,
        end_exclusive: u64,
        relative_path: &str,
        task_id: &str,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
    ) -> Result<Vec<u8>, JobError> {
        let expected_len = (end_exclusive - start) as usize;
        if let Some(root) = &self.local_root {
            let path = root.join(relative_to_path(&relative_path));
            if path.exists() {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(start))?;
                let buffer =
                    read_stream_with_progress(&mut file, expected_len, task_id, progress_tx)?;
                return Ok(buffer);
            }
        }

        let end = end_exclusive - 1;
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), relative_path);
        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, "first-light-launcher/0.1")
            .header(RANGE, format!("bytes={start}-{end}"));
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let mut response = request.send()?.error_for_status()?;
        let bytes = read_stream_with_progress(&mut response, expected_len, task_id, progress_tx)?;
        if bytes.len() != expected_len {
            return Err(JobError::Depot(format!(
                "range size mismatch for {pack_id}: expected {}, got {}",
                expected_len,
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    fn download_chunks_to_store_parallel<F>(
        &self,
        staged_chunks_root: &Path,
        chunks: &[ChunkRef],
        control: Arc<JobControl>,
        mut on_progress: F,
    ) -> Result<(), JobError>
    where
        F: FnMut(DownloadProgress) -> Result<(), JobError>,
    {
        if chunks.is_empty() {
            return Ok(());
        }

        let tasks = build_pack_download_tasks(chunks);
        let worker_count = download_worker_count().min(tasks.len()).max(1);
        let tasks = Arc::new(Mutex::new(VecDeque::from(tasks)));
        let abort = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Result<DownloadProgress, String>>();
        let mut first_error: Option<String> = None;

        thread::scope(|scope| {
            for _ in 0..worker_count {
                let tasks = Arc::clone(&tasks);
                let abort = Arc::clone(&abort);
                let tx = tx.clone();
                let source = self.clone();
                let control = Arc::clone(&control);
                let staged_chunks_root = staged_chunks_root.to_path_buf();
                scope.spawn(move || loop {
                    if abort.load(Ordering::SeqCst) {
                        break;
                    }
                    if control.is_canceled() {
                        abort.store(true, Ordering::SeqCst);
                        let _ = tx.send(Err("job canceled".to_string()));
                        break;
                    }
                    while control.is_paused() {
                        if control.is_canceled() {
                            abort.store(true, Ordering::SeqCst);
                            let _ = tx.send(Err("job canceled".to_string()));
                            return;
                        }
                        thread::sleep(Duration::from_millis(150));
                    }

                    let task = {
                        let mut guard = tasks.lock().expect("download task queue poisoned");
                        guard.pop_front()
                    };
                    let Some(task) = task else {
                        break;
                    };
                    let task_id = task.id();
                    let max_retries = download_retry_count();
                    let mut retry_count = 0_u32;
                    loop {
                        match source.download_pack_task_to_store(
                            &staged_chunks_root,
                            &task,
                            &task_id,
                            &tx,
                        ) {
                            Ok(()) => break,
                            Err(_) if retry_count < max_retries && !control.is_canceled() => {
                                retry_count += 1;
                                let _ = tx.send(Ok(DownloadProgress {
                                    task_id: task_id.clone(),
                                    committed_bytes: 0,
                                    in_flight_bytes: 0,
                                    clear_in_flight: true,
                                    retry_count,
                                }));
                                thread::sleep(download_retry_delay(retry_count));
                            }
                            Err(err) => {
                                abort.store(true, Ordering::SeqCst);
                                let _ = tx.send(Err(err.to_string()));
                                break;
                            }
                        }

                        if abort.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                });
            }
            drop(tx);

            for message in rx {
                match message {
                    Ok(progress) => {
                        if let Err(err) = on_progress(progress) {
                            abort.store(true, Ordering::SeqCst);
                            first_error = Some(err.to_string());
                            break;
                        }
                    }
                    Err(err) => {
                        abort.store(true, Ordering::SeqCst);
                        first_error = Some(err);
                        break;
                    }
                }
            }
        });

        if let Some(error) = first_error {
            return Err(JobError::Depot(error));
        }
        Ok(())
    }

    fn download_pack_task_to_store(
        &self,
        staged_chunks_root: &Path,
        task: &PackDownloadTask,
        task_id: &str,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
    ) -> Result<(), JobError> {
        let relative_path = format!("packs/{}.bin", task.pack_id);
        let range = self.fetch_pack_span_with_progress(
            &task.pack_id,
            task.range_start,
            task.range_end,
            &relative_path,
            task_id,
            progress_tx,
        )?;
        for chunk in &task.chunks {
            let path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
            if compressed_chunk_file_valid(&path, chunk)? {
                continue;
            }

            let start = (chunk.pack_offset - task.range_start) as usize;
            let end = start + chunk.compressed_size as usize;
            if end > range.len() {
                return Err(JobError::Depot(format!(
                    "pack range does not contain chunk {}",
                    chunk.hash
                )));
            }
            let compressed = &range[start..end];
            verify_compressed_chunk_bytes(chunk, compressed)?;
            write_chunk_file(&path, compressed)?;
        }
        progress_tx
            .send(Ok(DownloadProgress {
                task_id: task_id.to_string(),
                committed_bytes: task.range_end - task.range_start,
                in_flight_bytes: 0,
                clear_in_flight: true,
                retry_count: 0,
            }))
            .map_err(|err| JobError::Depot(err.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct LocalChunkSource {
    path: PathBuf,
    offset: u64,
    size: u64,
}

#[derive(Debug, Clone)]
struct PackDownloadTask {
    pack_id: String,
    range_start: u64,
    range_end: u64,
    chunks: Vec<ChunkRef>,
}

impl PackDownloadTask {
    fn id(&self) -> String {
        format!("{}:{}-{}", self.pack_id, self.range_start, self.range_end)
    }
}

#[derive(Debug, Clone)]
struct DownloadProgress {
    task_id: String,
    committed_bytes: u64,
    in_flight_bytes: u64,
    clear_in_flight: bool,
    retry_count: u32,
}

fn load_manifest_pair(
    source: &DepotSource,
    catalog: &Catalog,
    from_version: &str,
    to_version: &str,
) -> Result<(VersionManifest, VersionManifest), JobError> {
    Ok((
        source.load_manifest(catalog, from_version)?,
        source.load_manifest(catalog, to_version)?,
    ))
}

fn catalog_versions(catalog: Option<&Catalog>) -> Vec<String> {
    catalog
        .map(|catalog| {
            catalog
                .versions
                .iter()
                .map(|entry| entry.version.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn catalog_has_version(catalog: &Catalog, version: &str) -> bool {
    catalog
        .versions
        .iter()
        .any(|entry| entry.version == version)
}

fn resolve_target_version(
    catalog: &Catalog,
    target_version: Option<String>,
) -> Result<String, JobError> {
    let target = target_version
        .filter(|version| !version.trim().is_empty() && version != "unknown")
        .unwrap_or_else(|| catalog.effective_latest_version().unwrap_or("unknown").to_string());
    if catalog_has_version(catalog, &target) {
        Ok(target)
    } else {
        Err(JobError::Depot(format!(
            "version not found in catalog: {target}"
        )))
    }
}

fn changed_files_between(from: &VersionManifest, to: &VersionManifest) -> Vec<ChangedFile> {
    let from_map = from
        .files
        .iter()
        .map(|file| (file.path.to_ascii_lowercase(), file))
        .collect::<HashMap<_, _>>();
    to.files
        .iter()
        .filter_map(|file| match from_map.get(&file.path.to_ascii_lowercase()) {
            Some(old) if old.sha256 != file.sha256 || old.size != file.size => Some(ChangedFile {
                path: file.path.clone(),
                old_size: old.size,
                new_size: file.size,
            }),
            None => Some(ChangedFile {
                path: file.path.clone(),
                old_size: 0,
                new_size: file.size,
            }),
            _ => None,
        })
        .collect()
}

fn install_changed_files(manifest: &VersionManifest) -> Vec<ChangedFile> {
    manifest
        .files
        .iter()
        .map(|file| ChangedFile {
            path: file.path.clone(),
            old_size: 0,
            new_size: file.size,
        })
        .collect()
}

fn changed_target_files(from: &VersionManifest, to: &VersionManifest) -> Vec<FileEntry> {
    let from_map = from
        .files
        .iter()
        .map(|file| (file.path.to_ascii_lowercase(), file))
        .collect::<HashMap<_, _>>();
    to.files
        .iter()
        .filter(|file| {
            from_map
                .get(&file.path.to_ascii_lowercase())
                .map(|old| old.sha256 != file.sha256 || old.size != file.size)
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

fn build_local_chunk_sources(
    install_root: &Path,
    manifest: &VersionManifest,
) -> Result<HashMap<String, LocalChunkSource>, JobError> {
    let mut out = HashMap::new();
    for file in &manifest.files {
        let path = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
        if !path.exists() {
            continue;
        }
        for chunk in &file.chunks {
            out.entry(chunk.hash.clone())
                .or_insert_with(|| LocalChunkSource {
                    path: path.clone(),
                    offset: chunk.file_offset,
                    size: chunk.uncompressed_size,
                });
        }
    }
    Ok(out)
}

fn plan_missing_chunks(
    local_sources: &HashMap<String, LocalChunkSource>,
    staged_chunks_root: &Path,
    changed: &[FileEntry],
) -> Result<Vec<ChunkRef>, JobError> {
    let mut seen = HashSet::new();
    let mut missing = Vec::new();
    for file in changed {
        for chunk in &file.chunks {
            if !seen.insert(chunk.hash.clone()) {
                continue;
            }
            if local_sources.contains_key(&chunk.hash) {
                continue;
            }
            let staged_path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
            if compressed_chunk_file_valid(&staged_path, chunk)? {
                continue;
            }
            missing.push(chunk.clone());
        }
    }
    Ok(missing)
}

fn build_pack_download_tasks(chunks: &[ChunkRef]) -> Vec<PackDownloadTask> {
    let mut by_pack: HashMap<String, Vec<ChunkRef>> = HashMap::new();
    for chunk in chunks {
        by_pack
            .entry(chunk.pack_id.clone())
            .or_default()
            .push(chunk.clone());
    }

    let mut tasks = Vec::new();
    let max_task_bytes = pack_range_task_bytes();
    for (pack_id, mut pack_chunks) in by_pack {
        pack_chunks.sort_by_key(|chunk| chunk.pack_offset);
        let mut current_start = 0_u64;
        let mut current_end = 0_u64;
        let mut current_chunks: Vec<ChunkRef> = Vec::new();

        for chunk in pack_chunks {
            let chunk_start = chunk.pack_offset;
            let chunk_end = chunk.pack_offset + chunk.compressed_size;
            if current_chunks.is_empty() {
                current_start = chunk_start;
                current_end = chunk_end;
                current_chunks.push(chunk);
                continue;
            }

            let merged_end = current_end.max(chunk_end);
            let merged_len = merged_end.saturating_sub(current_start);
            if chunk_start <= current_end.saturating_add(PACK_RANGE_MERGE_GAP)
                && merged_len <= max_task_bytes
            {
                current_end = current_end.max(chunk_end);
                current_chunks.push(chunk);
            } else {
                tasks.push(PackDownloadTask {
                    pack_id: pack_id.clone(),
                    range_start: current_start,
                    range_end: current_end,
                    chunks: current_chunks,
                });
                current_start = chunk_start;
                current_end = chunk_end;
                current_chunks = vec![chunk];
            }
        }

        if !current_chunks.is_empty() {
            tasks.push(PackDownloadTask {
                pack_id,
                range_start: current_start,
                range_end: current_end,
                chunks: current_chunks,
            });
        }
    }

    tasks.sort_by(|a, b| {
        a.pack_id
            .cmp(&b.pack_id)
            .then_with(|| a.range_start.cmp(&b.range_start))
    });
    tasks
}

fn download_worker_count() -> usize {
    env::var("OXO_DOWNLOAD_WORKERS")
        .ok()
        .or_else(|| env::var("OXO_HF_DOWNLOAD_WORKERS").ok())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_DOWNLOAD_WORKERS)
        .clamp(1, MAX_DOWNLOAD_WORKERS)
}

fn download_retry_count() -> u32 {
    env::var("OXO_DOWNLOAD_RETRIES")
        .ok()
        .or_else(|| env::var("OXO_HF_DOWNLOAD_RETRIES").ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_DOWNLOAD_RETRIES)
        .clamp(0, MAX_DOWNLOAD_RETRIES)
}

fn download_retry_delay(retry_count: u32) -> Duration {
    let capped = retry_count.min(6);
    Duration::from_millis(500_u64.saturating_mul(1_u64 << capped))
}

fn pack_range_task_bytes() -> u64 {
    env::var("OXO_PACK_RANGE_MB")
        .ok()
        .or_else(|| env::var("OXO_HF_RANGE_MB").ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|value| value.saturating_mul(1024 * 1024))
        .unwrap_or(DEFAULT_PACK_RANGE_TASK_BYTES)
        .clamp(MIN_PACK_RANGE_TASK_BYTES, MAX_PACK_RANGE_TASK_BYTES)
}

fn download_transfer_bytes(chunks: &[ChunkRef]) -> u64 {
    build_pack_download_tasks(chunks)
        .iter()
        .map(|task| task.range_end - task.range_start)
        .sum()
}

fn read_stream_with_progress<R: Read>(
    reader: &mut R,
    expected_len: usize,
    task_id: &str,
    progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
) -> Result<Vec<u8>, JobError> {
    let mut buffer = Vec::with_capacity(expected_len);
    let mut scratch = [0_u8; 256 * 1024];
    while buffer.len() < expected_len {
        let remaining = expected_len - buffer.len();
        let read_len = remaining.min(scratch.len());
        let read = reader.read(&mut scratch[..read_len])?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&scratch[..read]);
        progress_tx
            .send(Ok(DownloadProgress {
                task_id: task_id.to_string(),
                committed_bytes: 0,
                in_flight_bytes: buffer.len() as u64,
                clear_in_flight: false,
                retry_count: 0,
            }))
            .map_err(|err| JobError::Depot(err.to_string()))?;
    }
    Ok(buffer)
}

fn estimate_missing_download_bytes(
    staged_chunks_root: Option<&Path>,
    from: &VersionManifest,
    to: &VersionManifest,
) -> u64 {
    let local_hashes = from
        .files
        .iter()
        .flat_map(|file| file.chunks.iter().map(|chunk| chunk.hash.clone()))
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    changed_target_files(from, to)
        .iter()
        .flat_map(|file| file.chunks.iter())
        .filter(|chunk| seen.insert(chunk.hash.clone()))
        .filter(|chunk| !local_hashes.contains(&chunk.hash))
        .filter(|chunk| {
            staged_chunks_root
                .map(|root| !staged_chunk_path_from(root, &chunk.hash).exists())
                .unwrap_or(true)
        })
        .map(|chunk| chunk.compressed_size)
        .sum()
}

fn estimate_install_download_bytes(
    staged_chunks_root: Option<&Path>,
    manifest: &VersionManifest,
) -> u64 {
    let mut seen = HashSet::new();
    manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter())
        .filter(|chunk| seen.insert(chunk.hash.clone()))
        .filter(|chunk| {
            staged_chunks_root
                .map(|root| !staged_chunk_path_from(root, &chunk.hash).exists())
                .unwrap_or(true)
        })
        .map(|chunk| chunk.compressed_size)
        .sum()
}

fn assemble_target_file(
    install_root: &Path,
    staging_root: Option<&Path>,
    staged_chunks_root: &Path,
    file: &FileEntry,
    local_sources: &HashMap<String, LocalChunkSource>,
) -> Result<(), JobError> {
    let target = safe_join(install_root, &file.path)
        .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
    if staging_root.is_some() && target_file_valid(&target, file)? {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_base = match staging_root {
        Some(root) => {
            let staged = safe_join(root, &file.path)
                .ok_or_else(|| JobError::Depot(format!("unsafe staging path: {}", file.path)))?;
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            staged
        }
        None => target.clone(),
    };
    let temp = sibling_path(&temp_base, "007launcher.tmp")?;
    let backup = sibling_path(&target, "007launcher.bak")?;
    let mut output = File::create(&temp)?;
    let mut hasher = Sha256::new();

    for chunk in &file.chunks {
        let data = read_chunk_bytes(chunk, local_sources, staged_chunks_root)?;
        hasher.update(&data);
        output.write_all(&data)?;
    }
    output.flush()?;
    drop(output);

    let actual = hex::encode(hasher.finalize());
    if actual != file.sha256 {
        let _ = fs::remove_file(&temp);
        return Err(JobError::Depot(format!(
            "assembled file hash mismatch: {}",
            file.path
        )));
    }

    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    if target.exists() {
        fs::rename(&target, &backup)?;
    }
    if let Err(err) = fs::rename(&temp, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(err.into());
    }
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    Ok(())
}

fn target_file_valid(path: &Path, file: &FileEntry) -> Result<bool, JobError> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() != file.size {
        return Ok(false);
    }
    Ok(sha256_file(path)? == file.sha256)
}

fn cleanup_staged_chunks(
    staged_chunks_root: &Path,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let mut seen = HashSet::new();
    let mut candidate_dirs = Vec::new();
    for file in &manifest.files {
        for chunk in &file.chunks {
            if !seen.insert(chunk.hash.clone()) {
                continue;
            }
            let path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if let Some(parent) = path.parent() {
                candidate_dirs.push(parent.to_path_buf());
            }
        }
    }

    candidate_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    candidate_dirs.dedup();
    for dir in candidate_dirs {
        if dir.starts_with(staged_chunks_root) {
            let _ = fs::remove_dir(&dir);
        }
    }
    let _ = fs::remove_dir(staged_chunks_root);
    Ok(())
}

pub fn abort_and_clean_job(game_id: &str) -> Result<(), JobError> {
    let source = DepotSource::for_game(game_id);
    let install_root = source.default_common_game_dir();
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let _ = cleanup_committed_download_session(&downloading_root, &source);
    let _ = std::fs::remove_dir_all(&downloading_root);
    Ok(())
}

pub fn cleanup_committed_download_session(
    downloading_root: &Path,
    source: &DepotSource,
) -> Result<(), JobError> {
    let session_path = downloading_root.join(DOWNLOAD_SESSION_FILE);
    if session_path.exists() {
        let bytes = fs::read(&session_path)?;
        let session: DownloadSessionMarker = serde_json::from_slice(&bytes)?;
        if session.status != "committed" || session.game_id != source.game_id {
            return Ok(());
        }
        fs::remove_file(&session_path)?;
    }
    cleanup_empty_owned_download_dirs(downloading_root)
}

fn cleanup_empty_owned_download_dirs(downloading_root: &Path) -> Result<(), JobError> {
    if !downloading_root.exists() {
        return Ok(());
    }

    for owned_root in [
        downloading_root.join("chunks"),
        downloading_root.join("files"),
    ] {
        if !owned_root.exists() {
            continue;
        }

        let mut dirs = Vec::new();
        for entry in WalkDir::new(&owned_root)
            .min_depth(1)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_dir() {
                dirs.push(entry.path().to_path_buf());
            }
        }

        dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        dirs.dedup();
        for dir in dirs {
            if dir.starts_with(&owned_root) {
                let _ = fs::remove_dir(&dir);
            }
        }
        let _ = fs::remove_dir(&owned_root);
    }

    let _ = fs::remove_dir(downloading_root);
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, JobError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

fn sha256_file_with_progress<F>(path: &Path, mut on_bytes: F) -> Result<String, JobError>
where
    F: FnMut(u64) -> Result<(), JobError>,
{
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; VERIFY_READ_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        on_bytes(read as u64)?;
    }
    Ok(hex::encode(hasher.finalize()))
}

fn verify_percent(
    checked_bytes: u64,
    total_bytes: u64,
    checked_files: usize,
    total_files: usize,
) -> f32 {
    if total_bytes > 0 {
        (checked_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0)
    } else if total_files > 0 {
        (checked_files as f32 / total_files as f32).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn emit_verify_progress(
    app: Option<&AppHandle>,
    progress: VerifyProgressEvent,
) -> Result<(), JobError> {
    if let Some(app) = app {
        app.emit(VERIFY_PROGRESS_EVENT, progress)?;
    }
    Ok(())
}

fn read_chunk_bytes(
    chunk: &ChunkRef,
    local_sources: &HashMap<String, LocalChunkSource>,
    staged_chunks_root: &Path,
) -> Result<Vec<u8>, JobError> {
    if let Some(source) = local_sources.get(&chunk.hash) {
        let mut file = File::open(&source.path)?;
        file.seek(SeekFrom::Start(source.offset))?;
        let mut buffer = vec![0_u8; source.size as usize];
        file.read_exact(&mut buffer)?;
        verify_chunk_bytes(chunk, &buffer)?;
        return Ok(buffer);
    }

    let path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
    if !path.exists() {
        return Err(JobError::Depot(format!(
            "missing staged chunk: {}",
            chunk.hash
        )));
    }
    let compressed = fs::read(path)?;
    verify_compressed_chunk_bytes(chunk, &compressed)?;
    let data = zstd::bulk::decompress(&compressed, chunk.uncompressed_size as usize)?;
    verify_chunk_bytes(chunk, &data)?;
    Ok(data)
}

fn verify_compressed_chunk_bytes(chunk: &ChunkRef, data: &[u8]) -> Result<(), JobError> {
    if data.len() != chunk.compressed_size as usize {
        return Err(JobError::Depot(format!(
            "compressed chunk size mismatch: {}",
            chunk.hash
        )));
    }
    let actual = sha256_bytes(data);
    if actual != chunk.compressed_sha256 {
        return Err(JobError::Depot(format!(
            "compressed chunk hash mismatch: {}",
            chunk.hash
        )));
    }
    Ok(())
}

fn verify_chunk_bytes(chunk: &ChunkRef, data: &[u8]) -> Result<(), JobError> {
    if data.len() != chunk.uncompressed_size as usize {
        return Err(JobError::Depot(format!(
            "chunk size mismatch: {}",
            chunk.hash
        )));
    }
    let actual = blake3::hash(data).to_hex().to_string();
    if actual != chunk.hash {
        return Err(JobError::Depot(format!(
            "chunk hash mismatch: {}",
            chunk.hash
        )));
    }
    Ok(())
}

fn compressed_chunk_file_valid(path: &Path, chunk: &ChunkRef) -> Result<bool, JobError> {
    if !path.exists() {
        return Ok(false);
    }
    let data = fs::read(path)?;
    if verify_compressed_chunk_bytes(chunk, &data).is_err() {
        fs::remove_file(path)?;
        return Ok(false);
    }
    Ok(true)
}

fn write_chunk_file(path: &Path, data: &[u8]) -> Result<(), JobError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, data)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn staged_chunk_dir(downloading_root: &Path) -> PathBuf {
    downloading_root.join("chunks")
}

fn staged_chunk_path_from(staged_chunks_root: &Path, hash: &str) -> PathBuf {
    let prefix = hash.get(0..2).unwrap_or("xx");
    staged_chunks_root
        .join(prefix)
        .join(format!("{hash}.chunk"))
}

fn persistent_cache_size(_app: &AppHandle) -> Result<u64, JobError> {
    Ok(0)
}

fn default_game_id_string() -> String {
    DEFAULT_GAME_ID.to_string()
}

fn sanitize_game_id(game_id: &str) -> String {
    let clean = game_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if clean.is_empty() {
        DEFAULT_GAME_ID.to_string()
    } else {
        clean
    }
}

fn game_dir_name(game_id: &str) -> &str {
    match game_id {
        "among-us" => "Among Us",
        DEFAULT_GAME_ID => DEFAULT_GAME_DIR_NAME,
        other => other,
    }
}

fn default_launch_executable(game_id: &str) -> String {
    match game_id {
        "among-us" => "Among Us.exe".to_string(),
        DEFAULT_GAME_ID => r"Retail\007FirstLight.exe".to_string(),
        other => format!("{}.exe", game_dir_name(other)),
    }
}

fn default_store_root() -> PathBuf {
    PathBuf::from(DEFAULT_STORE_ROOT)
}

fn default_common_game_dir() -> PathBuf {
    default_store_root()
        .join("common")
        .join(DEFAULT_GAME_DIR_NAME)
}

fn resolve_install_root(install_path: Option<String>, source: &DepotSource) -> PathBuf {
    install_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| source.default_common_game_dir())
}

pub fn downloading_dir_for_install(install_root: &Path, source: &DepotSource) -> PathBuf {
    if install_root == source.default_common_game_dir() {
        source.default_downloading_game_dir()
    } else {
        install_root.join(INSTALL_MARKER_DIR).join("downloading")
    }
}

fn install_marker_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(INSTALL_MARKER_FILE)
}

fn legacy_install_marker_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(LEGACY_INSTALL_MARKER_FILE)
}

fn installed_manifest_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(INSTALLED_MANIFEST_FILE)
}

fn read_install_marker(install_root: &Path) -> Result<Option<InstallMarker>, JobError> {
    let path = install_marker_path(install_root);
    if path.exists() {
        return Ok(Some(read_state_file(&path)?));
    }

    let path = legacy_install_marker_path(install_root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let marker: InstallMarker = serde_json::from_slice(&bytes)?;
    if let Some(parent) = install_marker_path(install_root).parent() {
        fs::create_dir_all(parent)?;
    }
    write_state_file(&install_marker_path(install_root), &marker)?;
    let _ = fs::remove_file(path);
    Ok(Some(marker))
}

fn write_install_marker(
    app: &AppHandle,
    install_root: &Path,
    manifest: &VersionManifest,
    source: &DepotSource,
) -> Result<(), JobError> {
    let launch_executable = manifest
        .launch_executable
        .clone()
        .unwrap_or_else(|| default_launch_executable(&source.game_id));
    let marker = InstallMarker {
        game_id: manifest.game_id.clone(),
        version: manifest.version.clone(),
        installed_at: Utc::now().to_rfc3339(),
        launch_executable: Some(launch_executable.clone()),
    };
    write_install_marker_file(install_root, &marker)?;
    write_installed_manifest(install_root, manifest)?;
    if let Some(executable) = safe_join(install_root, &launch_executable) {
        let _ = create_game_shortcut(app, source, install_root, &executable, &launch_executable);
    }
    Ok(())
}

fn write_sanitized_install_marker(
    install_root: &Path,
    source: &DepotSource,
    marker: &InstallMarker,
    launch_executable: &str,
) -> Result<(), JobError> {
    let sanitized = InstallMarker {
        game_id: source.game_id.clone(),
        version: marker.version.clone(),
        installed_at: if marker.installed_at.is_empty() {
            Utc::now().to_rfc3339()
        } else {
            marker.installed_at.clone()
        },
        launch_executable: Some(launch_executable.to_string()),
    };
    write_install_marker_file(install_root, &sanitized)?;
    Ok(())
}

fn write_install_marker_file(install_root: &Path, marker: &InstallMarker) -> Result<(), JobError> {
    let marker_path = install_marker_path(install_root);
    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_state_file(&marker_path, marker)?;
    let legacy_path = legacy_install_marker_path(install_root);
    if legacy_path.exists() {
        fs::remove_file(legacy_path)?;
    }
    Ok(())
}

fn write_installed_manifest(
    install_root: &Path,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let path = installed_manifest_path(install_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_state_file(&path, manifest)?;
    Ok(())
}

fn read_installed_manifest(install_root: &Path) -> Result<Option<VersionManifest>, JobError> {
    let path = installed_manifest_path(install_root);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_state_file(&path)?))
}

fn write_state_file<T: Serialize>(path: &Path, value: &T) -> Result<(), JobError> {
    let mut payload = serde_json::to_vec(value)?;
    transform_state_payload(&mut payload);
    let mut bytes = STATE_MAGIC.to_vec();
    bytes.extend_from_slice(&payload);
    fs::write(path, bytes)?;
    Ok(())
}

fn read_state_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, JobError> {
    let bytes = fs::read(path)?;
    if !bytes.starts_with(STATE_MAGIC) {
        return Ok(serde_json::from_slice(&bytes)?);
    }
    let mut payload = bytes[STATE_MAGIC.len()..].to_vec();
    transform_state_payload(&mut payload);
    Ok(serde_json::from_slice(&payload)?)
}

fn transform_state_payload(bytes: &mut [u8]) {
    for (index, byte) in bytes.iter_mut().enumerate() {
        let key = STATE_KEY[index % STATE_KEY.len()].rotate_left((index % 7) as u32);
        *byte ^= key ^ ((index as u8).wrapping_mul(31));
    }
}

fn load_manifest_for_version(
    source: &DepotSource,
    version: &str,
) -> Result<VersionManifest, JobError> {
    let catalog = source.load_catalog()?;
    source.load_manifest(&catalog, version)
}

fn write_download_session_marker(
    downloading_root: &Path,
    journal: &JobJournal,
    status: &str,
    install_path: String,
) -> Result<(), JobError> {
    fs::create_dir_all(downloading_root)?;
    let marker = DownloadSessionMarker {
        game_id: journal.game_id.clone(),
        target_version: journal.to_version.clone(),
        status: status.to_string(),
        install_path,
        downloading_path: downloading_root.display().to_string(),
        bytes_done: journal.bytes_done,
        bytes_total: journal.bytes_total,
        updated_at: Utc::now().to_rfc3339(),
    };
    fs::write(
        downloading_root.join(DOWNLOAD_SESSION_FILE),
        serde_json::to_vec_pretty(&marker)?,
    )?;
    Ok(())
}

fn relative_to_path(relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<PathBuf>()
}

fn sibling_path(path: &Path, suffix: &str) -> Result<PathBuf, JobError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| JobError::Depot(format!("invalid target path: {}", path.display())))?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{file_name}.{suffix}")))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn wait_for_control(
    app: &AppHandle,
    control: &JobControl,
    journal: &mut JobJournal,
    step_index: usize,
) -> Result<(), JobError> {
    if control.is_canceled() {
        journal.status = JobStatus::Canceled;
        journal.phase = "Canceled".to_string();
        append_log(journal, "warn", "Job canceled by user");
        persist_and_emit(app, journal)?;
        return Err(JobError::Depot("job canceled".to_string()));
    }

    if control.is_paused() {
        // Mark paused and keep emitting while waiting
        journal.status = JobStatus::Paused;
        journal.steps[step_index].status = StepStatus::Paused;
        journal.resumable = true;
        touch(journal);
        persist_and_emit(app, journal)?;

        loop {
            thread::sleep(Duration::from_millis(300));
            if control.is_canceled() {
                journal.status = JobStatus::Canceled;
                journal.phase = "Canceled".to_string();
                append_log(journal, "warn", "Paused job canceled by user");
                persist_and_emit(app, journal)?;
                return Err(JobError::Depot("job canceled".to_string()));
            }
            if !control.is_paused() {
                break;
            }
        }

        // Resumed — restore step to running and emit immediately so UI updates
        journal.status = JobStatus::Downloading;
        journal.steps[step_index].status = StepStatus::Running;
        touch(journal);
        persist_and_emit(app, journal)?;
    }

    Ok(())
}

fn set_step_running(
    app: &AppHandle,
    journal: &mut JobJournal,
    step_index: usize,
    status: JobStatus,
    phase: &str,
) -> Result<(), JobError> {
    journal.status = status;
    journal.phase = phase.to_string();
    journal.steps[step_index].status = StepStatus::Running;
    journal.steps[step_index].progress = 0.0;
    journal.overall_progress = overall_progress(step_index, 0.0);
    touch(journal);
    persist_and_emit(app, journal)
}

fn complete_step(
    app: &AppHandle,
    journal: &mut JobJournal,
    step_index: usize,
) -> Result<(), JobError> {
    journal.steps[step_index].status = StepStatus::Completed;
    journal.steps[step_index].progress = 1.0;
    journal.overall_progress = overall_progress(step_index, 1.0);
    touch(journal);
    persist_and_emit(app, journal)
}

fn mark_running_step_failed(journal: &mut JobJournal) {
    if let Some(step) = journal
        .steps
        .iter_mut()
        .find(|step| step.status == StepStatus::Running || step.status == StepStatus::Paused)
    {
        step.status = StepStatus::Failed;
    }
    touch(journal);
}

fn progress_fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        done as f32 / total as f32
    }
}

fn byte_progress(done: u64, total: u64) -> f32 {
    if total == 0 {
        1.0
    } else {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    }
}

fn overall_progress(step_index: usize, step_progress: f32) -> f32 {
    (step_index as f32 + step_progress) / 5.0
}

fn human_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if value == 0 {
        return "0 B".to_string();
    }
    let mut size = value as f64;
    let mut index = 0usize;
    while size >= 1024.0 && index < UNITS.len() - 1 {
        size /= 1024.0;
        index += 1;
    }
    format!("{size:.2} {}", UNITS[index])
}

pub fn read_latest_journal(app: &AppHandle) -> Result<Option<JobJournal>, JobError> {
    let path = journal_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&data)?))
}

fn is_active_real_journal(journal: &JobJournal) -> bool {
    let is_active = matches!(
        journal.status,
        JobStatus::Planned
            | JobStatus::Running
            | JobStatus::Paused
            | JobStatus::Downloading
            | JobStatus::Assembling
            | JobStatus::Verified
            | JobStatus::Failed
    );
    journal.logs.iter().any(|log| {
        log.message.contains("Real update job") || log.message.contains("Real install job")
    }) && is_active
}

fn default_journal(
    game_id: &str,
    kind: &str,
    install_path: String,
    from_version: &str,
    to_version: &str,
    bytes_total: u64,
) -> JobJournal {
    let now = Utc::now().to_rfc3339();
    JobJournal {
        id: format!("job-{}", Utc::now().timestamp_millis()),
        game_id: game_id.to_string(),
        kind: kind.to_string(),
        status: JobStatus::Planned,
        install_path,
        from_version: from_version.to_string(),
        to_version: to_version.to_string(),
        phase: "Planned".to_string(),
        overall_progress: 0.0,
        bytes_done: 0,
        bytes_total,
        retry_count: 0,
        resumable: true,
        updated_at: now.clone(),
        steps: vec![
            step("Scan", "Find local files and detect version"),
            step("Verify", "Hash manifest-owned files"),
            step("Download packs", "Resume missing byte ranges from proxy"),
            step("Assemble files", "Rebuild files into verified temp outputs"),
            step("Finalize", "Replace only after full-file hash match"),
        ],
        logs: vec![JobLog {
            at: now,
            level: "info".to_string(),
            message: "Ready to start resumable update".to_string(),
        }],
    }
}

fn step(name: &str, detail: &str) -> JobStep {
    JobStep {
        name: name.to_string(),
        detail: detail.to_string(),
        status: StepStatus::Waiting,
        progress: 0.0,
        retry_count: 0,
    }
}

fn append_log(journal: &mut JobJournal, level: &str, message: &str) {
    journal.logs.push(JobLog {
        at: Utc::now().format("%H:%M:%S").to_string(),
        level: level.to_string(),
        message: message.to_string(),
    });
    if journal.logs.len() > 80 {
        let excess = journal.logs.len() - 80;
        journal.logs.drain(0..excess);
    }
    touch(journal);
}

fn touch(journal: &mut JobJournal) {
    journal.updated_at = Utc::now().to_rfc3339();
}

fn persist_and_emit(app: &AppHandle, journal: &JobJournal) -> Result<(), JobError> {
    let path = journal_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(journal)?;
    fs::write(&path, data)?;
    let _ = app.emit("launcher://job", journal);
    Ok(())
}

fn journal_path(app: &AppHandle) -> Result<PathBuf, JobError> {
    Ok(app
        .path()
        .app_data_dir()?
        .join("journals")
        .join("current-job.json"))
}
