use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike, Utc};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, AUTHORIZATION, CONTENT_RANGE, RANGE, RETRY_AFTER, USER_AGENT};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use walkdir::WalkDir;

use crate::asset_pack;
use crate::depot_crypto::{self, DEPOT_ENCRYPTION_ALGORITHM};
use crate::launch::{
    fallback_launch_config, load_install_override, main_process, normalize_launch_config,
    option_unavailable_reason, process_path, resolve_launch_config, select_launch_option,
    GameLaunchConfig, ResolvedGameLaunchConfig,
};
use crate::manifest::{
    Catalog, CatalogVersion, ChunkCodec, ChunkRef, FileEntry, VersionManifest, FORMAT_VERSION,
    LEGACY_FORMAT_VERSION,
};
use crate::scanner::safe_join;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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
const MAX_DOWNLOAD_WORKERS: usize = 64;
const MAX_DOWNLOAD_RETRIES: u32 = 12;
const PACK_RANGE_MERGE_GAP: u64 = 4 * 1024 * 1024;
const MIN_PACK_RANGE_TASK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PACK_RANGE_TASK_BYTES: u64 = 64 * 1024 * 1024;
const MIN_ADAPTIVE_RANGE_BYTES: u64 = 8 * 1024 * 1024;
const VERIFY_PROGRESS_EVENT: &str = "launcher://verify-progress";
const VERIFY_READ_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const DOWNLOAD_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_CHECKPOINT_MIN_INTERVAL: Duration = Duration::from_secs(1);
const DOWNLOAD_CHECKPOINT_MAX_INTERVAL: Duration = Duration::from_secs(5);
const CIRCUIT_BREAKER_FAILURES: u32 = 3;
const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(30);

mod dependencies;
mod direct;
mod paths;
mod progress;

use dependencies::{
    create_game_shortcut, ensure_game_dependencies, launch_option_processes, remove_game_shortcut,
};
use direct::DirectStagePlan;
use paths::*;
use progress::*;

#[derive(Debug)]
struct AdaptiveRangeState {
    range_bytes: u64,
    ewma_rate: f64,
    successful_samples: u32,
}

static ADAPTIVE_RANGE_STATE: OnceLock<Mutex<AdaptiveRangeState>> = OnceLock::new();
static RUNNING_GAMES: OnceLock<Mutex<std::collections::HashMap<String, u32>>> = OnceLock::new();

fn running_games() -> &'static Mutex<std::collections::HashMap<String, u32>> {
    RUNNING_GAMES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[derive(Default)]
pub struct JobControl {
    paused: AtomicBool,
    canceled: AtomicBool,
    running: AtomicBool,
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
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
    #[error("rate limited: {detail}")]
    RateLimited { detail: String, retry_after_ms: u64 },
    #[error("authorization failed: {0}")]
    Unauthorized(String),
    #[error("remote object not found: {0}")]
    NotFound(String),
    #[error("transient download failure: {0}")]
    Transient(String),
    #[error("job canceled")]
    Canceled,
}

impl JobError {
    fn retry_delay(&self, retry_count: u32) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after_ms, .. } => {
                Some(Duration::from_millis((*retry_after_ms).max(250)))
            }
            Self::Transient(_) => Some(download_retry_delay(retry_count)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSnapshot {
    pub current_version: String,
    pub latest_version: String,
    pub available_versions: Vec<String>,
    pub detected_install_path: Option<String>,
    pub update_size: u64,
    pub install_size: u64,
    pub temporary_space: u64,
    pub required_free_space: u64,
    pub proxy_status: String,
    pub cache: CacheSnapshot,
    pub changed_files: Vec<ChangedFile>,
    pub last_job: Option<JobJournal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoUpdateEvent {
    state: String,
    message: String,
    game_id: Option<String>,
}

pub fn start_auto_update_scheduler(app: AppHandle, control: Arc<JobControl>) {
    thread::spawn(move || {
        let mut last_attempts = HashMap::<String, Instant>::new();
        thread::sleep(Duration::from_secs(20));
        loop {
            if let Err(error) = auto_update_tick(&app, &control, &mut last_attempts) {
                let _ = app.emit(
                    "launcher://auto-update",
                    AutoUpdateEvent {
                        state: "error".to_string(),
                        message: error,
                        game_id: None,
                    },
                );
            }
            thread::sleep(Duration::from_secs(300));
        }
    });
}

fn auto_update_tick(
    app: &AppHandle,
    control: &Arc<JobControl>,
    last_attempts: &mut HashMap<String, Instant>,
) -> Result<(), String> {
    let settings = crate::platform::current_settings();
    if settings.game_update_mode == crate::platform::GameUpdateMode::Manual {
        return Ok(());
    }
    if settings.game_update_mode == crate::platform::GameUpdateMode::Scheduled
        && !time_in_update_window(
            Local::now().hour() as u16 * 60 + Local::now().minute() as u16,
            &settings.game_update_schedule_start,
            &settings.game_update_schedule_end,
        )
    {
        return Ok(());
    }
    if control.is_running() {
        return Ok(());
    }
    if crate::platform::get_runtime_states(app)?
        .iter()
        .any(|runtime| runtime.running)
    {
        return Ok(());
    }
    if read_latest_journal(app)
        .map_err(|error| error.to_string())?
        .is_some_and(|journal| {
            matches!(
                journal.status,
                JobStatus::Planned
                    | JobStatus::Running
                    | JobStatus::Paused
                    | JobStatus::Downloading
                    | JobStatus::Assembling
            )
        })
    {
        return Ok(());
    }

    let mut installs = crate::platform::install_records(app)?;
    installs.sort_by(|left, right| left.game_id.cmp(&right.game_id));
    for install in installs {
        if last_attempts
            .get(&install.game_id)
            .is_some_and(|attempt| attempt.elapsed() < Duration::from_secs(30 * 60))
        {
            continue;
        }
        let source = DepotSource::for_game(&install.game_id);
        let catalog = match source.load_catalog() {
            Ok(catalog) => catalog,
            Err(_) => continue,
        };
        let Some(latest) = catalog.effective_latest_version().map(str::to_string) else {
            continue;
        };
        if latest == install.version {
            continue;
        }
        last_attempts.insert(install.game_id.clone(), Instant::now());
        let _ = app.emit(
            "launcher://auto-update",
            AutoUpdateEvent {
                state: "starting".to_string(),
                message: format!(
                    "Starting automatic update for {}: {} → {}",
                    install.game_id, install.version, latest
                ),
                game_id: Some(install.game_id.clone()),
            },
        );
        spawn_update_job(
            app.clone(),
            control.clone(),
            install.install_path,
            Some(latest),
            Some(install.game_id),
        )
        .map_err(|error| error.to_string())?;
        break;
    }
    Ok(())
}

fn time_in_update_window(now_minutes: u16, start: &str, end: &str) -> bool {
    let parse = |value: &str| {
        value.split_once(':').and_then(|(hour, minute)| {
            Some(hour.parse::<u16>().ok()? * 60 + minute.parse::<u16>().ok()?)
        })
    };
    let Some(start) = parse(start) else {
        return false;
    };
    let Some(end) = parse(end) else {
        return false;
    };
    if start == end {
        return true;
    }
    if start < end {
        now_minutes >= start && now_minutes < end
    } else {
        now_minutes >= start || now_minutes < end
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSnapshot {
    pub cache_size: u64,
    pub cache_path: String,
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
    pub removed_shortcuts: usize,
    pub steam_shortcut_removed: bool,
    pub install_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReport {
    pub game_id: String,
    pub executable: String,
    pub shortcut_path: Option<String>,
    pub dependencies_installed: Vec<String>,
    #[serde(default)]
    pub launch_option_id: String,
    #[serde(default)]
    pub launch_option_title: String,
    #[serde(default)]
    pub launched_processes: Vec<String>,
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
    #[serde(default)]
    pub metrics: DownloadMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadMetrics {
    pub pipeline: String,
    pub payload_bytes: u64,
    pub network_bytes: u64,
    pub overfetch_bytes: u64,
    pub retry_wait_ms: u64,
    pub rate_limit_wait_ms: u64,
    pub peak_in_flight_bytes: u64,
    pub throughput_p50_bytes_per_second: u64,
    pub throughput_p95_bytes_per_second: u64,
    #[serde(skip)]
    throughput_samples: Vec<u64>,
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
    // Startup snapshot must be instant. Do not fetch remote catalog/manifest or
    // calculate changed files here; those heavier checks run when the user opens
    // a game/update flow. This prevents the WebView from feeling frozen at launch.
    let catalog = source.load_local_catalog().ok();
    let latest_version = catalog
        .as_ref()
        .and_then(|catalog| catalog.effective_latest_version().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let available_versions = catalog_versions(catalog.as_ref());
    let default_install = default_common_game_dir();
    // Keep the startup snapshot cheap. The exact chunk-store size is calculated
    // by the install/update planning snapshots, where disk work is expected.
    let cache_size = 0;
    let cache_path = downloading_chunk_cache_path(&default_install, &source)
        .display()
        .to_string();
    let cache_free_space = downloading_cache_free_space(&default_install, &source);
    let marker = read_install_marker(&default_install).ok().flatten();

    let (current_version, detected_install_path) = if let Some(marker) = marker {
        (marker.version, Some(default_install.display().to_string()))
    } else {
        ("not installed".to_string(), None)
    };

    Ok(LauncherSnapshot {
        current_version,
        latest_version,
        available_versions,
        detected_install_path,
        update_size: 0,
        install_size: 0,
        temporary_space: 0,
        required_free_space: 0,
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            cache_path: cache_path.clone(),
            free_space: cache_free_space,
            health_percent: if cache_size > 0 { 100 } else { 0 },
            rollback_ready: false,
            rollback_missing_bytes: 0,
        },
        changed_files: Vec::new(),
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
    let default_install = source.default_common_game_dir();
    let cache_size = downloading_chunk_cache_size(&default_install, &source).unwrap_or(0);
    let cache_path = downloading_chunk_cache_path(&default_install, &source)
        .display()
        .to_string();
    let cache_free_space = downloading_cache_free_space(&default_install, &source);

    let update_size = estimate_install_download_bytes(
        Some(&downloading_chunk_cache_path(&default_install, &source)),
        &manifest,
    );
    let temporary_space = planned_temporary_space(&manifest.files, update_size);
    Ok(LauncherSnapshot {
        current_version: "not installed".to_string(),
        latest_version: catalog
            .effective_latest_version()
            .unwrap_or("unknown")
            .to_string(),
        available_versions: catalog_versions(Some(&catalog)),
        detected_install_path: None,
        update_size,
        install_size: manifest.total_size,
        temporary_space,
        required_free_space: required_free_space(temporary_space),
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            cache_path: cache_path.clone(),
            free_space: cache_free_space,
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
    let catalog = source.load_catalog()?;
    let selected_version = resolve_target_version(&catalog, target_version)?;
    let latest_version = catalog
        .effective_latest_version()
        .unwrap_or("unknown")
        .to_string();
    let installed_base = load_installed_update_base(install_path, &source, &catalog)?;
    let current_version = installed_base
        .as_ref()
        .map(|base| base.version.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let staged_chunks_root = staged_chunk_dir(&downloading_dir_for_install(install_path, &source));
    let (changed_files, update_size, install_size, temporary_space) = match installed_base {
        None => (Vec::new(), 0, 0, 0),
        Some(base) if base.version == selected_version => {
            (Vec::new(), 0, base.manifest.total_size, 0)
        }
        Some(base) => {
            let to = source.load_manifest(&catalog, &selected_version)?;
            let changed_targets = changed_target_files(&base.manifest, &to);
            (
                changed_files_between(&base.manifest, &to),
                estimate_missing_download_bytes(Some(&staged_chunks_root), &base.manifest, &to),
                to.total_size,
                planned_temporary_space(
                    &changed_targets,
                    estimate_missing_download_bytes(Some(&staged_chunks_root), &base.manifest, &to),
                ),
            )
        }
    };
    let cache_size = downloading_chunk_cache_size(install_path, &source).unwrap_or(0);
    let cache_path = downloading_chunk_cache_path(install_path, &source)
        .display()
        .to_string();
    let cache_free_space = downloading_cache_free_space(install_path, &source);

    Ok(LauncherSnapshot {
        current_version,
        latest_version,
        available_versions: catalog_versions(Some(&catalog)),
        detected_install_path: Some(install_path.display().to_string()),
        update_size,
        install_size,
        temporary_space,
        required_free_space: required_free_space(temporary_space),
        proxy_status: source.status_label(),
        cache: CacheSnapshot {
            cache_size,
            cache_path: cache_path.clone(),
            free_space: cache_free_space,
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
    let mut journal = default_journal(
        &source.game_id,
        "update",
        install_path,
        "detecting",
        &target_version,
        0,
    );
    journal.steps[0] = step(
        "Read install state",
        "Load .0xolemon state and the installed manifest",
    );
    journal.steps[1] = step(
        "Plan update",
        "Compare manifests and validate reusable local chunks",
    );
    persist_and_emit(&app, &journal)?;
    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        let result = run_real_update_job(&app_for_thread, control_for_thread.clone(), initial);
        let canceled = control_for_thread.is_canceled();
        control_for_thread.set_running(false);
        if canceled {
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(JobError::Canceled) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(err) => {
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
    fs::create_dir_all(&staged_chunks_root)?;
    let initial_missing =
        plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &manifest.files)?;
    let initial_bytes = download_transfer_bytes(&initial_missing);
    let initial_in_flight = existing_partial_task_progress(&staged_chunks_root, &initial_missing);
    let mut journal = default_journal(
        &source.game_id,
        "install",
        install_root.display().to_string(),
        "not installed",
        &target_version,
        initial_bytes,
    );
    journal.bytes_done = initial_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
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

    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        let result = run_real_install_job(&app_for_thread, control_for_thread.clone(), initial);
        let canceled = control_for_thread.is_canceled();
        control_for_thread.set_running(false);
        if canceled {
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(JobError::Canceled) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(err) => {
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
        .unwrap_or_else(|| {
            catalog
                .effective_latest_version()
                .unwrap_or("unknown")
                .to_string()
        });
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
    journal.bytes_done = existing_partial_task_progress(&staged_chunks_root, &missing_chunks)
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
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
    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        let result = run_real_repair_job(
            &app_for_thread,
            control_for_thread.clone(),
            initial,
            repair_files,
            target_manifest,
        );
        let canceled = control_for_thread.is_canceled();
        control_for_thread.set_running(false);
        if canceled {
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(JobError::Canceled) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(err) => {
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
        }
    });

    Ok(return_journal)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn install_root_candidates(app: &AppHandle, source: &DepotSource) -> Vec<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    // Preferred source: the path persisted when the marker was committed.
    if let Ok(Some(path)) = crate::platform::registered_install_path(app, &source.game_id) {
        push_unique_path(&mut candidates, path);
    }

    // Recovery source for a just-finished or interrupted job.
    if let Ok(Some(journal)) = read_latest_journal(app) {
        if sanitize_game_id(&journal.game_id) == source.game_id {
            let path = journal.install_path.trim();
            if !path.is_empty() {
                push_unique_path(&mut candidates, PathBuf::from(path));
            }
        }
    }

    push_unique_path(&mut candidates, source.default_common_game_dir());

    // Recover installs made before path persistence was enabled. Only direct
    // children of known library/common directories are inspected.
    let configured_library = crate::platform::current_settings().default_library;
    let mut common_roots = vec![default_store_root().join("common")];
    if !configured_library.trim().is_empty() {
        common_roots.push(PathBuf::from(configured_library).join("common"));
    }
    common_roots.sort();
    common_roots.dedup();

    for common_root in common_roots {
        let Ok(entries) = fs::read_dir(&common_root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                push_unique_path(&mut candidates, path);
            }
        }
    }

    candidates
}

fn locate_registered_install(
    app: &AppHandle,
    source: &DepotSource,
) -> Option<(PathBuf, InstallMarker)> {
    for install_root in install_root_candidates(app, source) {
        let Some(marker) = read_install_marker(&install_root).ok().flatten() else {
            continue;
        };
        if install_marker_matches_source(&marker, source) {
            return Some((install_root, marker));
        }
    }
    None
}

fn job_is_active_for_game(app: &AppHandle, game_id: &str) -> bool {
    read_latest_journal(app)
        .ok()
        .flatten()
        .is_some_and(|journal| {
            sanitize_game_id(&journal.game_id) == sanitize_game_id(game_id)
                && matches!(
                    journal.status,
                    JobStatus::Planned
                        | JobStatus::Running
                        | JobStatus::Paused
                        | JobStatus::Downloading
                        | JobStatus::Assembling
                        | JobStatus::Verified
                        | JobStatus::Failed
                )
        })
}

fn remove_dir_all_with_retry(path: &Path, attempts: usize) -> Result<(), JobError> {
    if !path.exists() {
        return Ok(());
    }

    let attempts = attempts.max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < attempts {
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "cleanup failed"))
        .into())
}

fn remove_file_with_retry(path: &Path, attempts: usize) -> Result<(), JobError> {
    if !path.exists() {
        return Ok(());
    }

    let attempts = attempts.max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < attempts {
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "file cleanup failed"))
        .into())
}

fn clear_completed_journal_for_game(app: &AppHandle, game_id: &str) {
    let Ok(Some(journal)) = read_latest_journal(app) else {
        return;
    };
    if journal.status == JobStatus::Committed
        && sanitize_game_id(&journal.game_id) == sanitize_game_id(game_id)
    {
        let _ = clear_current_journal_if_matches(app, &journal.id);
    }
}

fn cleanup_completed_download_data_if_idle(
    app: &AppHandle,
    install_root: &Path,
    source: &DepotSource,
) {
    if job_is_active_for_game(app, &source.game_id) {
        return;
    }
    let downloading_root = downloading_dir_for_install(install_root, source);
    let _ = remove_dir_all_with_retry(&downloading_root, 24);
}

pub fn game_install_state(app: &AppHandle, game_id: &str) -> Result<GameInstallState, JobError> {
    let source = DepotSource::for_game(game_id);
    let resolved = locate_registered_install(app, &source);

    if let Some((install_root, marker)) = resolved {
        let launch_executable = marker
            .launch_executable
            .clone()
            .unwrap_or_else(|| default_launch_executable(&source.game_id));
        let current_version = if marker.version.is_empty() {
            "installed".to_string()
        } else {
            marker.version.clone()
        };

        write_sanitized_install_marker(&install_root, &source, &marker, &launch_executable)?;
        crate::platform::register_install(
            app,
            &source.game_id,
            &install_root,
            &current_version,
            &launch_executable,
        )
        .map_err(JobError::Depot)?;

        if read_installed_manifest(&install_root)?.is_none() {
            if let Ok(catalog) = source.load_local_catalog() {
                if let Ok(manifest) = source.load_local_manifest(&catalog, &marker.version) {
                    let _ = write_installed_manifest(&install_root, &manifest);
                }
            }
        }

        // Also repairs installs completed by an older launcher build: clear a
        // stale committed journal and remove its completed downloading folder.
        clear_completed_journal_for_game(app, &source.game_id);
        cleanup_completed_download_data_if_idle(app, &install_root, &source);

        return Ok(GameInstallState {
            game_id: source.game_id,
            installed: install_root.exists(),
            current_version,
            install_path: install_root.display().to_string(),
            launch_executable,
        });
    }

    let install_root = source.default_common_game_dir();
    Ok(GameInstallState {
        game_id: source.game_id.clone(),
        installed: false,
        current_version: "not installed".to_string(),
        install_path: install_root.display().to_string(),
        launch_executable: default_launch_executable(&source.game_id),
    })
}

pub fn game_install_state_quick(
    app: &AppHandle,
    game_id: &str,
) -> Result<GameInstallState, JobError> {
    game_install_state(app, game_id)
}

pub fn game_install_states_quick(
    app: &AppHandle,
    game_ids: &[String],
) -> Result<Vec<GameInstallState>, JobError> {
    game_ids
        .iter()
        .map(|game_id| game_install_state_quick(app, game_id))
        .collect()
}

pub fn game_launch_config(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    launch_executable: Option<String>,
) -> Result<ResolvedGameLaunchConfig, JobError> {
    let (config, source, _) =
        effective_launch_config(app, game_id, install_path, launch_executable)?;
    Ok(resolve_launch_config(&config, install_path, source))
}

fn effective_launch_config(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    launch_executable: Option<String>,
) -> Result<(GameLaunchConfig, String, String), JobError> {
    let source = DepotSource::for_game(game_id);
    let marker_executable = read_install_marker(install_path)?
        .filter(|marker| install_marker_matches_source(marker, &source))
        .and_then(|marker| marker.launch_executable);
    let fallback_executable = launch_executable
        .filter(|value| !value.trim().is_empty())
        .or(marker_executable)
        .unwrap_or_else(|| default_launch_executable(&source.game_id));

    if let Some((override_config, path)) =
        load_install_override(install_path).map_err(JobError::Depot)?
    {
        let config =
            normalize_launch_config(override_config, &source.game_id, &fallback_executable)
                .map_err(JobError::Depot)?;
        return Ok((
            config,
            format!("install override: {path}"),
            fallback_executable,
        ));
    }

    let embedded = asset_pack::get_game_detail(app, &source.game_id, None)
        .map(|detail| detail.launch)
        .unwrap_or_default();
    let normalized = normalize_launch_config(embedded, &source.game_id, &fallback_executable);

    // Asset packs can outlive a mapping change. If their embedded launch config
    // is invalid or every configured process points to a missing file, prefer the
    // executable stored in the install marker / remote path table instead of
    // making the Play button silently unusable.
    if let Ok(config) = normalized {
        let has_available_option = config.options.iter().any(|option| {
            option_unavailable_reason(option, install_path, &source.game_id).is_none()
        });
        if has_available_option {
            return Ok((config, "asset pack".to_string(), fallback_executable));
        }
    }

    let fallback = fallback_launch_config(&source.game_id, &fallback_executable);
    Ok((
        fallback,
        "install marker / game mapping fallback".to_string(),
        fallback_executable,
    ))
}

pub fn refresh_registered_game_shortcuts(app: &AppHandle) -> Result<usize, JobError> {
    let records = crate::platform::install_records(app).map_err(JobError::Depot)?;
    let mut refreshed = 0_usize;
    for record in records {
        let install_root = PathBuf::from(&record.install_path);
        if !install_root.is_dir() {
            continue;
        }
        let source = DepotSource::for_game(&record.game_id);
        let relative_executable = if record.launch_executable.trim().is_empty() {
            default_launch_executable(&source.game_id)
        } else {
            record.launch_executable.clone()
        };
        let Some(executable) = safe_join(&install_root, &relative_executable) else {
            continue;
        };
        if !executable.is_file() {
            continue;
        }
        if create_game_shortcut(
            app,
            &source,
            &install_root,
            &executable,
            &relative_executable,
        )?
        .is_some()
        {
            refreshed += 1;
        }
    }
    Ok(refreshed)
}

pub fn launch_game(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    launch_executable: Option<String>,
    launch_option_id: Option<String>,
    skip_cloud_sync: bool,
) -> Result<LaunchReport, JobError> {
    let source = DepotSource::for_game(game_id);
    let marker = read_install_marker(install_path)?
        .ok_or_else(|| JobError::Depot(format!("{} is not installed", source.game_dir_name)))?;
    if !install_marker_matches_source(&marker, &source) {
        return Err(JobError::Depot(format!(
            "install marker belongs to {}, not {}",
            marker.game_id, source.game_id
        )));
    }

    let requested_executable = launch_executable
        .filter(|value| !value.trim().is_empty())
        .or(marker.launch_executable.clone());
    let (config, _, fallback_executable) = effective_launch_config(
        app,
        &source.game_id,
        install_path,
        requested_executable.clone(),
    )?;
    let option = select_launch_option(
        &config,
        launch_option_id.as_deref(),
        requested_executable
            .as_deref()
            .or(Some(fallback_executable.as_str())),
    )
    .ok_or_else(|| JobError::Depot("no launch option is configured".to_string()))?;

    if let Some(reason) = option_unavailable_reason(option, install_path, &source.game_id) {
        return Err(JobError::Depot(format!(
            "launch option '{}' is unavailable: {reason}",
            option.title
        )));
    }

    let main = main_process(option)
        .ok_or_else(|| JobError::Depot("launch option has no process".to_string()))?;
    let executable = process_path(install_path, main, &source.game_id)
        .ok_or_else(|| JobError::Depot(format!("unsafe executable path: {}", main.path)))?;
    if !executable.exists() {
        return Err(JobError::Depot(format!(
            "game executable is missing: {}",
            executable.display()
        )));
    }

    if !skip_cloud_sync {
        crate::cloud_save::sync_before_launch(app, &source.game_id).map_err(JobError::Depot)?;
    }

    let dependencies_installed = ensure_game_dependencies(app, &source)?;
    let shortcut_path = create_game_shortcut(app, &source, install_path, &executable, &main.path)
        .ok()
        .flatten()
        .map(|path| path.display().to_string());
    let _ = crate::steam_integration::ensure_game_shortcut(
        app,
        &source.game_id,
        &source.game_dir_name,
        install_path,
        &main.path,
        Some(&executable),
    );

    let mut launched = launch_option_processes(&source.game_id, install_path, option)?;
    if let Some(mut child) = launched.main_child.take() {
        crate::cloud_save::mark_game_running(&source.game_id, true);
        let pid = child.id();
        running_games().lock().unwrap().insert(source.game_id.clone(), pid);
        let _ = app.emit("launcher://game-started", serde_json::json!({ "gameId": &source.game_id }));

        let app_for_exit = app.clone();
        let game_id_for_exit = source.game_id.clone();
        thread::spawn(move || {
            let exit_status = child.wait();
            running_games().lock().unwrap().remove(&game_id_for_exit);
            let exit_code = exit_status.ok().and_then(|s| s.code());
            let _ = app_for_exit.emit("launcher://game-exited", serde_json::json!({
                "gameId": &game_id_for_exit,
                "exitCode": exit_code,
                "sessionSeconds": 0
            }));
            crate::cloud_save::sync_after_exit_async(app_for_exit, game_id_for_exit);
        });
    }
    Ok(LaunchReport {
        game_id: source.game_id,
        executable: executable.display().to_string(),
        shortcut_path,
        dependencies_installed,
        launch_option_id: option.id.clone(),
        launch_option_title: option.title.clone(),
        launched_processes: launched
            .paths
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
    })
}

pub fn kill_game(game_id: &str) -> Result<(), JobError> {
    let pid = running_games().lock().unwrap().get(game_id).copied();
    if let Some(pid) = pid {
        let mut cmd = hidden_command("taskkill");
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        let status = cmd
            .status()
            .map_err(|e| JobError::Depot(format!("Failed to taskkill: {}", e)))?;
        if !status.success() {
            return Err(JobError::Depot(format!("taskkill failed: {}", status)));
        }
        running_games().lock().unwrap().remove(game_id);
    }
    Ok(())
}

// Launch dependency/install/shortcut helpers live in job/dependencies.rs

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

pub fn uninstall_game(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
) -> Result<UninstallReport, JobError> {
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

    let removed_shortcuts = remove_game_shortcut(app, &source, install_path)
        .unwrap_or_default()
        .len();
    let steam_shortcut_removed =
        crate::steam_integration::remove_game_shortcut(app, &source.game_id)
            .map(|outcome| outcome.changed || outcome.queued)
            .unwrap_or(false);
    let _ = crate::platform::unregister_install(app, &source.game_id);

    if let Ok(Some(active)) = read_latest_journal(app) {
        if sanitize_game_id(&active.game_id) == source.game_id {
            let _ = clear_current_journal_if_matches(app, &active.id);
        }
    }
    let downloading_root = downloading_dir_for_install(install_path, &source);
    let _ = remove_dir_all_with_retry(&downloading_root, 24);

    Ok(UninstallReport {
        game_id: source.game_id,
        removed_files,
        removed_dirs,
        removed_shortcuts,
        steam_shortcut_removed,
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
    let staging_root = downloading_root.join("files");
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    append_log(&mut journal, "info", "Real update job started");

    set_step_running(
        app,
        &mut journal,
        0,
        JobStatus::Running,
        "Read install state",
    )?;
    let catalog = source.load_catalog()?;
    let installed_base = load_installed_update_base(&install_root, &source, &catalog)?
        .ok_or_else(|| {
            JobError::Depot(
                "Cannot determine the installed version: .0xolemon/state.0xo and manifest.0xo are missing or invalid"
                    .to_string(),
            )
        })?;
    let InstalledUpdateBase {
        version: from_version,
        manifest: from_manifest,
        source_label,
    } = installed_base;
    journal.from_version = from_version.clone();
    append_log(
        &mut journal,
        "info",
        &format!(
            "Installed version {from_version} resolved from {source_label} ({} manifest files)",
            from_manifest.files.len()
        ),
    );
    complete_step(app, &mut journal, 0)?;

    set_step_running(app, &mut journal, 1, JobStatus::Running, "Verify manifests")?;
    let target_version = resolve_target_version(&catalog, Some(journal.to_version.clone()))?;
    journal.to_version = target_version.clone();
    let target_manifest = source.load_manifest(&catalog, &target_version)?;
    let changed = changed_target_files(&from_manifest, &target_manifest);
    let (local_sources, reused_chunks, rejected_chunks) =
        build_verified_local_chunk_sources(&install_root, &from_manifest, &changed, &control)?;
    append_log(
        &mut journal,
        "info",
        &format!(
            "Validated {reused_chunks} reusable local chunks; {rejected_chunks} invalid or unavailable chunks will be downloaded"
        ),
    );
    fs::create_dir_all(&staged_chunks_root)?;
    let direct_stage = prepare_direct_stage(
        &downloading_root,
        &staging_root,
        &changed,
        &target_version,
        &local_sources,
        &control,
    )?;
    let missing_chunks = if let Some(stage) = direct_stage.as_ref() {
        stage.filter_missing_chunks(&local_sources, &changed)
    } else {
        plan_missing_chunks(&local_sources, &staged_chunks_root, &changed)?
    };
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    journal.bytes_done = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
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
    let mut in_flight = resumed_in_flight;
    let mut progress_callback = |progress: DownloadProgress| {
        if progress.clear_in_flight {
            in_flight.remove(&progress.task_id);
        } else {
            in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
        }
        downloaded += progress.committed_bytes;
        wait_for_control(app, &control, &mut journal, 2)?;
        let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
        observe_download_progress(&mut journal, &progress, in_flight.values().copied().sum());
        journal.bytes_done = display_done.min(journal.bytes_total);
        journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
        journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
        journal.overall_progress = overall_progress(2, journal.steps[2].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)
    };
    if let Some(stage) = direct_stage.as_ref() {
        source.download_chunks_direct_to_staging(
            &staged_chunks_root,
            stage,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    }
    complete_step(app, &mut journal, 2)?;

    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Assemble changed files",
    )?;
    if let Some(stage) = direct_stage.as_ref() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(
            &mut journal,
            "info",
            "Verifying and committing direct staging files",
        );
        stage.commit_files(&install_root, &changed)?;
        journal.steps[3].progress = 1.0;
        journal.overall_progress = overall_progress(3, 1.0);
        persist_and_emit(app, &journal)?;
    } else {
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
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize")?;
    write_install_marker(
        app,
        &install_root,
        &target_manifest,
        &source,
        &target_version,
    )?;
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
    append_log(
        &mut journal,
        "info",
        &format!(
            "Cleaning completed download data from {}",
            downloading_root.display()
        ),
    );
    if let Err(err) = cleanup_committed_download_session(&downloading_root, &source) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not remove completed update download data: {err}"),
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
    let direct_stage = prepare_direct_stage(
        &downloading_root,
        &staging_root,
        &changed,
        &target_version,
        &local_sources,
        &control,
    )?;
    let missing_chunks = if let Some(stage) = direct_stage.as_ref() {
        stage.filter_missing_chunks(&local_sources, &changed)
    } else {
        plan_missing_chunks(&local_sources, &staged_chunks_root, &changed)?
    };
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    journal.bytes_done = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
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
    let mut in_flight = resumed_in_flight;
    let mut progress_callback = |progress: DownloadProgress| {
        if progress.clear_in_flight {
            in_flight.remove(&progress.task_id);
        } else {
            in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
        }
        downloaded += progress.committed_bytes;
        wait_for_control(app, &control, &mut journal, 2)?;
        let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
        observe_download_progress(&mut journal, &progress, in_flight.values().copied().sum());
        journal.bytes_done = display_done.min(journal.bytes_total);
        journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
        journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
        journal.overall_progress = overall_progress(2, journal.steps[2].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)
    };
    if let Some(stage) = direct_stage.as_ref() {
        source.download_chunks_direct_to_staging(
            &staged_chunks_root,
            stage,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    }
    complete_step(app, &mut journal, 2)?;

    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Assemble install files",
    )?;
    if let Some(stage) = direct_stage.as_ref() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(
            &mut journal,
            "info",
            "Verifying and committing direct staging files",
        );
        stage.commit_files(&install_root, &changed)?;
        journal.steps[3].progress = 1.0;
        journal.overall_progress = overall_progress(3, 1.0);
        persist_and_emit(app, &journal)?;
    } else {
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
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize")?;
    write_install_marker(
        app,
        &install_root,
        &target_manifest,
        &source,
        &target_version,
    )?;
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
    append_log(
        &mut journal,
        "info",
        &format!(
            "Cleaning completed download data from {}",
            downloading_root.display()
        ),
    );
    if let Err(err) = cleanup_committed_download_session(&downloading_root, &source) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not remove completed install download data: {err}"),
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
    let staging_root = downloading_root.join("files");
    let direct_stage = prepare_direct_stage(
        &downloading_root,
        &staging_root,
        &repair_files,
        &journal.to_version,
        &local_sources,
        &control,
    )?;
    let missing_chunks = if let Some(stage) = direct_stage.as_ref() {
        stage.filter_missing_chunks(&local_sources, &repair_files)
    } else {
        plan_missing_chunks(&local_sources, &staged_chunks_root, &repair_files)?
    };
    journal.bytes_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    journal.bytes_done = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
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
    let mut in_flight = resumed_in_flight;
    let mut progress_callback = |progress: DownloadProgress| {
        if progress.clear_in_flight {
            in_flight.remove(&progress.task_id);
        } else {
            in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
        }
        downloaded += progress.committed_bytes;
        wait_for_control(app, &control, &mut journal, 2)?;
        let display_done = downloaded.saturating_add(in_flight.values().copied().sum::<u64>());
        observe_download_progress(&mut journal, &progress, in_flight.values().copied().sum());
        journal.bytes_done = display_done.min(journal.bytes_total);
        journal.steps[2].progress = byte_progress(journal.bytes_done, journal.bytes_total);
        journal.steps[2].retry_count = journal.steps[2].retry_count.max(progress.retry_count);
        journal.overall_progress = overall_progress(2, journal.steps[2].progress);
        touch(&mut journal);
        persist_and_emit(app, &journal)
    };
    if let Some(stage) = direct_stage.as_ref() {
        source.download_chunks_direct_to_staging(
            &staged_chunks_root,
            stage,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    }
    complete_step(app, &mut journal, 2)?;

    set_step_running(app, &mut journal, 3, JobStatus::Assembling, "Repair files")?;
    if let Some(stage) = direct_stage.as_ref() {
        wait_for_control(app, &control, &mut journal, 3)?;
        append_log(
            &mut journal,
            "info",
            "Verifying and committing repaired staging files",
        );
        stage.commit_files(&install_root, &repair_files)?;
        journal.steps[3].progress = 1.0;
        journal.overall_progress = overall_progress(3, 1.0);
        persist_and_emit(app, &journal)?;
    } else {
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
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize repair")?;
    write_install_marker(
        app,
        &install_root,
        &target_manifest,
        &source,
        &journal.to_version,
    )?;
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
    append_log(
        &mut journal,
        "info",
        &format!(
            "Cleaning completed download data from {}",
            downloading_root.display()
        ),
    );
    if let Err(err) = cleanup_committed_download_session(&downloading_root, &source) {
        append_log(
            &mut journal,
            "warning",
            &format!("Could not remove completed repair download data: {err}"),
        );
    }
    append_log(&mut journal, "info", "Real repair committed");
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

#[derive(Debug, Default)]
struct HostRateState {
    blocked_until: Option<Instant>,
    circuit_open_until: Option<Instant>,
    consecutive_transient_failures: u32,
}

#[derive(Debug, Default)]
struct RateCoordinator {
    hosts: Mutex<HashMap<String, HostRateState>>,
}

impl RateCoordinator {
    fn wait_until_ready(
        &self,
        base_url: &str,
        control: Option<&JobControl>,
    ) -> Result<(), JobError> {
        loop {
            if control.is_some_and(JobControl::is_canceled) {
                return Err(JobError::Canceled);
            }
            let delay = self.hosts.lock().ok().and_then(|hosts| {
                let state = hosts.get(base_url)?;
                let now = Instant::now();
                [state.blocked_until, state.circuit_open_until]
                    .into_iter()
                    .flatten()
                    .filter_map(|deadline| deadline.checked_duration_since(now))
                    .max()
            });
            let Some(delay) = delay else {
                return Ok(());
            };
            thread::sleep(delay.min(Duration::from_millis(250)));
        }
    }

    fn record_success(&self, base_url: &str) {
        if let Ok(mut hosts) = self.hosts.lock() {
            let state = hosts.entry(base_url.to_string()).or_default();
            state.consecutive_transient_failures = 0;
            state.circuit_open_until = None;
        }
    }

    fn record_transient_failure(&self, base_url: &str) {
        if let Ok(mut hosts) = self.hosts.lock() {
            let state = hosts.entry(base_url.to_string()).or_default();
            state.consecutive_transient_failures =
                state.consecutive_transient_failures.saturating_add(1);
            if state.consecutive_transient_failures >= CIRCUIT_BREAKER_FAILURES {
                state.circuit_open_until = Some(Instant::now() + CIRCUIT_BREAKER_COOLDOWN);
                state.consecutive_transient_failures = 0;
            }
        }
    }

    fn block_for(&self, base_url: &str, delay: Duration) {
        if let Ok(mut hosts) = self.hosts.lock() {
            let state = hosts.entry(base_url.to_string()).or_default();
            let deadline = Instant::now() + delay;
            if state.blocked_until.is_none_or(|current| deadline > current) {
                state.blocked_until = Some(deadline);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct DepotRemoteBase {
    url: String,
    token: Option<String>,
}

#[derive(Debug, Clone)]
struct DepotSource {
    game_id: String,
    game_dir_name: String,
    base_urls: Vec<DepotRemoteBase>,
    active_base_url: Arc<Mutex<Option<String>>>,
    local_root: Option<PathBuf>,
    client: OnceLock<Client>,
    rate_coordinator: Arc<RateCoordinator>,
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

fn sanitize_token(token: Option<String>) -> Option<String> {
    token
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

fn hf_environment_token() -> Option<String> {
    env::var("FIRST_LIGHT_HF_TOKEN")
        .or_else(|_| env::var("HF_TOKEN"))
        .ok()
        .and_then(|value| sanitize_token(Some(value)))
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
        let global_token = hf_environment_token();
        let base_urls_with_tokens = remote_repo_base_urls(&game_id);
        let mut base_urls = Vec::new();
        for (url, token) in base_urls_with_tokens {
            base_urls.push(DepotRemoteBase {
                url,
                token: sanitize_token(token).or_else(|| global_token.clone()),
            });
        }
        if is_default {
            if let Ok(legacy_base) = env::var("FIRST_LIGHT_DEPOT_BASE") {
                let legacy_base = legacy_base.trim().trim_end_matches('/');
                if !legacy_base.is_empty() {
                    let legacy_token = base_urls
                        .iter()
                        .find(|candidate| candidate.url == legacy_base)
                        .and_then(|candidate| candidate.token.clone())
                        .or_else(|| global_token.clone());
                    base_urls.retain(|candidate| candidate.url != legacy_base);
                    base_urls.insert(
                        0,
                        DepotRemoteBase {
                            url: legacy_base.to_string(),
                            token: legacy_token,
                        },
                    );
                }
            }
        }

        Self {
            game_dir_name: game_dir_name(&game_id).to_string(),
            game_id,
            base_urls,
            active_base_url: Arc::new(Mutex::new(None)),
            local_root,
            client: OnceLock::new(),
            rate_coordinator: Arc::new(RateCoordinator::default()),
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
        let has_token = self.base_urls.iter().any(|base| base.token.is_some());
        match (&self.local_root, has_token) {
            (Some(_), true) | (None, true) => "Content service ready".to_string(),
            (Some(_), false) => "Offline metadata ready".to_string(),
            (None, false) => "Remote content service ready".to_string(),
        }
    }

    fn get_client(&self) -> &Client {
        self.client.get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .build()
                .unwrap_or_else(|_| Client::new())
        })
    }

    fn ordered_base_urls(&self) -> Vec<DepotRemoteBase> {
        let active = self
            .active_base_url
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let mut ordered = Vec::with_capacity(self.base_urls.len());
        if let Some(active) = active {
            if let Some(base) = self.base_urls.iter().find(|candidate| candidate.url == active) {
                ordered.push(base.clone());
            }
        }
        for candidate in &self.base_urls {
            if !ordered.iter().any(|existing| existing.url == candidate.url) {
                ordered.push(candidate.clone());
            }
        }
        ordered
    }

    fn mark_active_base_url(&self, base_url: &str) {
        if let Ok(mut guard) = self.active_base_url.lock() {
            *guard = Some(base_url.to_string());
        }
    }

    fn send_remote_get(
        &self,
        base: &DepotRemoteBase,
        url: &str,
        range: Option<(u64, u64)>,
        control: Option<&JobControl>,
    ) -> Result<reqwest::blocking::Response, JobError> {
        let base_url = base.url.as_str();
        let token = base.token.as_deref();
        self.rate_coordinator.wait_until_ready(base_url, control)?;
        let send = |with_token: bool| {
            let mut request = self
                .get_client()
                .get(url)
                .header(USER_AGENT, "0xolemon-launcher/0.2");
            if let Some((start, end)) = range {
                request = request.header(RANGE, format!("bytes={start}-{end}"));
            }
            if with_token {
                if let Some(token) = token {
                    request = request.header(AUTHORIZATION, format!("Bearer {token}"));
                }
            }
            request.send()
        };

        let has_token = token.is_some();
        let mut response = send(has_token).map_err(|error| {
            self.rate_coordinator.record_transient_failure(base_url);
            JobError::Transient(format!("{url}: {error}"))
        })?;

        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) && has_token
        {
            // Public Hugging Face repositories remain usable when a stale inherited
            // HF_TOKEN is present. Private repositories still fail clearly below.
            response = send(false).map_err(|error| {
                self.rate_coordinator.record_transient_failure(base_url);
                JobError::Transient(format!("{url}: anonymous retry failed ({error})"))
            })?;
        }

        let status = response.status();
        let rate_delay = rate_limit_delay(response.headers());
        if let Some(delay) =
            rate_delay.filter(|_| rate_limit_remaining(response.headers()) == Some(0))
        {
            self.rate_coordinator.block_for(base_url, delay);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let delay = rate_delay.unwrap_or(Duration::from_secs(30));
            self.rate_coordinator.block_for(base_url, delay);
            return Err(JobError::RateLimited {
                detail: format!("{url}: HTTP 429"),
                retry_after_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
            });
        }
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(JobError::Unauthorized(format!("{url}: HTTP {status}")));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(JobError::NotFound(format!("{url}: HTTP 404")));
        }
        if status == StatusCode::REQUEST_TIMEOUT || status.is_server_error() {
            self.rate_coordinator.record_transient_failure(base_url);
            return Err(JobError::Transient(format!("{url}: HTTP {status}")));
        }
        if !status.is_success() {
            return Err(JobError::Depot(format!("{url}: HTTP {status}")));
        }

        self.rate_coordinator.record_success(base_url);
        Ok(response)
    }

    fn load_catalog(&self) -> Result<Catalog, JobError> {
        let catalog: Catalog = self.load_json("catalog.json")?;
        validate_format_version(catalog.format_version, "catalog")?;
        Ok(catalog)
    }

    fn load_local_catalog(&self) -> Result<Catalog, JobError> {
        let root = self
            .local_root
            .as_ref()
            .ok_or_else(|| JobError::Depot("local depot is not configured".to_string()))?;
        let bytes = fs::read(root.join("catalog.json"))?;
        let catalog: Catalog = serde_json::from_slice(&bytes)?;
        validate_format_version(catalog.format_version, "catalog")?;
        Ok(catalog)
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
        let path = find_catalog_version_entry(catalog, version)
            .map(|entry| entry.manifest_path.as_str())
            .ok_or_else(|| JobError::Depot(format!("version not found in catalog: {version}")))?;
        let bytes = fs::read(root.join(relative_to_path(path)))?;
        let manifest: VersionManifest = serde_json::from_slice(&bytes)?;
        validate_format_version(manifest.format_version, "manifest")?;
        Ok(canonicalize_manifest_version(manifest, version))
    }

    fn load_manifest(&self, catalog: &Catalog, version: &str) -> Result<VersionManifest, JobError> {
        let path = find_catalog_version_entry(catalog, version)
            .map(|entry| entry.manifest_path.as_str())
            .ok_or_else(|| JobError::Depot(format!("version not found in catalog: {version}")))?;
        let manifest: VersionManifest = self.load_json(path)?;
        validate_format_version(manifest.format_version, "manifest")?;
        Ok(canonicalize_manifest_version(manifest, version))
    }

    fn load_json<T: for<'de> Deserialize<'de>>(&self, relative_path: &str) -> Result<T, JobError> {
        if let Some(root) = &self.local_root {
            let path = root.join(relative_to_path(relative_path));
            if path.exists() {
                let bytes = fs::read(path)?;
                return Ok(serde_json::from_slice(&bytes)?);
            }
        }

        let encoded_relative_path = encode_hf_relative_path(relative_path);
        let mut failures = Vec::new();
        for base in self.ordered_base_urls() {
            let url = format!(
                "{}/{}",
                base.url.trim_end_matches('/'),
                encoded_relative_path
            );
            match self.send_remote_get(&base, &url, None, None) {
                Ok(response) => match response.json::<T>() {
                    Ok(value) => {
                        self.mark_active_base_url(&base.url);
                        return Ok(value);
                    }
                    Err(err) => failures.push(format!("{url}: invalid JSON ({err})")),
                },
                Err(err) => failures.push(err.to_string()),
            }
        }

        let detail = if failures.is_empty() {
            "no download server is configured".to_string()
        } else {
            // Sanitize URLs to not expose internal server details
            let sanitized: Vec<String> = failures
                .iter()
                .map(|f| {
                    if let Some(colon_pos) = f.find(": ") {
                        format!("server error: {}", &f[colon_pos + 2..])
                    } else {
                        "server error: download failed".to_string()
                    }
                })
                .collect();
            sanitized.join(" | ")
        };
        Err(JobError::Depot(format!(
            "unable to load {relative_path}: {detail}"
        )))
    }

    fn fetch_pack_span_with_progress(
        &self,
        pack_id: &str,
        start: u64,
        end_exclusive: u64,
        relative_path: &str,
        task_id: &str,
        partial_path: &Path,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
    ) -> Result<Vec<u8>, JobError> {
        let expected_len = end_exclusive.saturating_sub(start);
        if expected_len == 0 {
            return Ok(Vec::new());
        }
        if let Some(parent) = partial_path.parent() {
            fs::create_dir_all(parent)?;
        }
        normalize_partial_file(partial_path, expected_len)?;

        if let Some(root) = &self.local_root {
            let path = root.join(relative_to_path(relative_path));
            if path.exists() {
                let existing = durable_partial_len(partial_path).min(expected_len);
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(start.saturating_add(existing)))?;
                append_stream_to_partial(
                    &mut file,
                    partial_path,
                    existing,
                    expected_len,
                    task_id,
                    control,
                    progress_tx,
                )?;
                return read_completed_partial(partial_path, expected_len, pack_id);
            }
        }

        let encoded_relative_path = encode_hf_relative_path(relative_path);
        let mut failures = Vec::new();
        for base in self.ordered_base_urls() {
            normalize_partial_file(partial_path, expected_len)?;
            let existing = durable_partial_len(partial_path).min(expected_len);
            if existing == expected_len {
                return read_completed_partial(partial_path, expected_len, pack_id);
            }

            let request_start = start.saturating_add(existing);
            let end = end_exclusive - 1;
            let url = format!(
                "{}/{}",
                base.url.trim_end_matches('/'),
                encoded_relative_path
            );
            let mut response = match self.send_remote_get(
                &base,
                &url,
                Some((request_start, end)),
                Some(control),
            ) {
                Ok(response) => response,
                Err(JobError::NotFound(err)) => {
                    failures.push(err);
                    continue;
                }
                Err(JobError::Unauthorized(err)) => {
                    failures.push(err);
                    continue;
                }
                Err(err @ JobError::RateLimited { .. }) | Err(err @ JobError::Transient(_)) => {
                    return Err(err)
                }
                Err(err) => {
                    failures.push(err.to_string());
                    continue;
                }
            };

            if response.status() != StatusCode::PARTIAL_CONTENT {
                failures.push(format!(
                    "{url}: server ignored byte range {request_start}-{end} (status {})",
                    response.status()
                ));
                continue;
            }
            if !content_range_starts_at(&response, request_start) {
                failures.push(format!(
                    "{url}: invalid Content-Range for requested offset {request_start}"
                ));
                continue;
            }

            match append_stream_to_partial(
                &mut response,
                partial_path,
                existing,
                expected_len,
                task_id,
                control,
                progress_tx,
            ) {
                Ok(final_len) if final_len == expected_len => {
                    self.mark_active_base_url(&base.url);
                    return read_completed_partial(partial_path, expected_len, pack_id);
                }
                Ok(final_len) => failures.push(format!(
                    "{url}: range size mismatch for {pack_id}; expected {expected_len}, got {final_len}"
                )),
                Err(JobError::Canceled) => return Err(JobError::Canceled),
                Err(err) => failures.push(format!("{url}: {err}")),
            }
        }

        let detail = if failures.is_empty() {
            "no download server is configured".to_string()
        } else {
            // Sanitize URLs to not expose internal server details
            let sanitized: Vec<String> = failures
                .iter()
                .map(|f| {
                    if let Some(colon_pos) = f.find(": ") {
                        format!("server error: {}", &f[colon_pos + 2..])
                    } else {
                        "server error: download failed".to_string()
                    }
                })
                .collect();
            sanitized.join(" | ")
        };
        Err(JobError::Depot(format!(
            "unable to download pack {pack_id}: {detail}"
        )))
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
        let settings = crate::platform::current_settings();
        let queue_budget = settings.download_queue_mb.saturating_mul(1024 * 1024);
        let workers_by_budget = (queue_budget / pack_range_task_bytes()).max(1) as usize;
        let worker_count = download_worker_count()
            .min(workers_by_budget)
            .min(tasks.len())
            .max(1);
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
                            &control,
                            &tx,
                        ) {
                            Ok(()) => break,
                            Err(err) if retry_count < max_retries && !control.is_canceled() => {
                                let next_retry = retry_count.saturating_add(1);
                                let Some(delay) = err.retry_delay(next_retry) else {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(err.to_string()));
                                    break;
                                };
                                retry_count = next_retry;
                                observe_adaptive_range(0, true);
                                let _ = tx.send(Ok(DownloadProgress {
                                    task_id: task_id.clone(),
                                    committed_bytes: 0,
                                    in_flight_bytes: 0,
                                    clear_in_flight: true,
                                    retry_count,
                                    rate_bytes_per_second: 0,
                                    retry_wait_ms: delay.as_millis().min(u128::from(u64::MAX))
                                        as u64,
                                    rate_limit_wait_ms: if matches!(
                                        err,
                                        JobError::RateLimited { .. }
                                    ) {
                                        delay.as_millis().min(u128::from(u64::MAX)) as u64
                                    } else {
                                        0
                                    },
                                }));
                                if let Err(err) = sleep_with_control(delay, &control) {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(err.to_string()));
                                    break;
                                }
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
            if error.eq_ignore_ascii_case("job canceled") {
                return Err(JobError::Canceled);
            }
            return Err(JobError::Depot(error));
        }
        Ok(())
    }

    fn download_pack_task_to_store(
        &self,
        staged_chunks_root: &Path,
        task: &PackDownloadTask,
        task_id: &str,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
    ) -> Result<(), JobError> {
        let relative_path = format!("packs/{}.bin", task.pack_id);
        let partial_path = partial_range_path(staged_chunks_root, task);
        let range = self.fetch_pack_span_with_progress(
            &task.pack_id,
            task.range_start,
            task.range_end,
            &relative_path,
            task_id,
            &partial_path,
            control,
            progress_tx,
        )?;

        let write_result = (|| -> Result<(), JobError> {
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
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&partial_path);
            let _ = fs::remove_file(partial_checkpoint_path(&partial_path));
            return Err(err);
        }
        let _ = fs::remove_file(&partial_path);
        let _ = fs::remove_file(partial_checkpoint_path(&partial_path));
        progress_tx
            .send(Ok(DownloadProgress {
                task_id: task_id.to_string(),
                committed_bytes: task.range_end - task.range_start,
                in_flight_bytes: 0,
                clear_in_flight: true,
                retry_count: 0,
                rate_bytes_per_second: 0,
                retry_wait_ms: 0,
                rate_limit_wait_ms: 0,
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

fn partial_range_path(staged_chunks_root: &Path, task: &PackDownloadTask) -> PathBuf {
    let safe_pack_id = task
        .pack_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    let pack_file_id = if safe_pack_id.is_empty() {
        "pack"
    } else {
        safe_pack_id.as_str()
    };
    staged_chunks_root.join("_ranges").join(format!(
        "{pack_file_id}-{}-{}.part",
        task.range_start, task.range_end
    ))
}

fn partial_file_len(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn partial_checkpoint_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!("{file_name}.checkpoint"))
}

fn durable_partial_len(path: &Path) -> u64 {
    let actual = partial_file_len(path);
    let checkpoint = fs::read_to_string(partial_checkpoint_path(path))
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok());
    checkpoint.unwrap_or(actual).min(actual)
}

fn persist_partial_checkpoint(path: &Path, durable_len: u64) -> Result<(), JobError> {
    let checkpoint = partial_checkpoint_path(path);
    let temporary = checkpoint.with_extension("checkpoint.tmp");
    {
        let mut file = File::create(&temporary)?;
        write!(file, "{durable_len}")?;
        file.sync_all()?;
    }
    if checkpoint.exists() {
        fs::remove_file(&checkpoint)?;
    }
    fs::rename(temporary, checkpoint)?;
    Ok(())
}

fn normalize_partial_file(path: &Path, expected_len: u64) -> Result<(), JobError> {
    if path.exists() && partial_file_len(path) > expected_len {
        fs::remove_file(path)?;
        let checkpoint = partial_checkpoint_path(path);
        if checkpoint.exists() {
            fs::remove_file(checkpoint)?;
        }
    } else if path.exists() {
        let durable = durable_partial_len(path).min(expected_len);
        let file = OpenOptions::new().write(true).open(path)?;
        if file.metadata()?.len() != durable {
            file.set_len(durable)?;
            file.sync_all()?;
        }
        persist_partial_checkpoint(path, durable)?;
    }
    Ok(())
}

fn read_completed_partial(
    path: &Path,
    expected_len: u64,
    pack_id: &str,
) -> Result<Vec<u8>, JobError> {
    if durable_partial_len(path) != expected_len {
        return Err(JobError::Depot(format!(
            "partial range for {pack_id} is not durably checkpointed"
        )));
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 != expected_len {
        return Err(JobError::Depot(format!(
            "partial range size mismatch for {pack_id}; expected {expected_len}, got {}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn content_range_starts_at(response: &reqwest::blocking::Response, expected_start: u64) -> bool {
    let Some(value) = response.headers().get(CONTENT_RANGE) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some(range) = value.strip_prefix("bytes ") else {
        return false;
    };
    range
        .split_once('-')
        .and_then(|(start, _)| start.parse::<u64>().ok())
        .is_some_and(|start| start == expected_start)
}

fn rate_limit_remaining(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get("ratelimit")?.to_str().ok()?;
    value.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("r=")
            .and_then(|value| value.trim_matches('"').parse::<u64>().ok())
    })
}

fn rate_limit_delay(headers: &HeaderMap) -> Option<Duration> {
    if let Some(seconds) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return Some(Duration::from_secs(seconds.clamp(1, 3600)));
    }
    let value = headers.get("ratelimit")?.to_str().ok()?;
    value.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("t=")
            .and_then(|value| value.trim_matches('"').parse::<u64>().ok())
            .map(|seconds| Duration::from_secs(seconds.clamp(1, 3600)))
    })
}

fn append_stream_to_partial<R: Read>(
    reader: &mut R,
    partial_path: &Path,
    existing_len: u64,
    expected_len: u64,
    task_id: &str,
    control: &JobControl,
    progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
) -> Result<u64, JobError> {
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(partial_path)?;
    let mut written = existing_len.min(expected_len);
    let mut durable = written;
    let mut unsynced = 0_u64;
    let mut last_progress_emit = Instant::now();
    let mut last_progress_bytes = written;
    let mut last_checkpoint = Instant::now();

    progress_tx
        .send(Ok(DownloadProgress {
            task_id: task_id.to_string(),
            committed_bytes: 0,
            in_flight_bytes: durable,
            clear_in_flight: false,
            retry_count: 0,
            rate_bytes_per_second: 0,
            retry_wait_ms: 0,
            rate_limit_wait_ms: 0,
        }))
        .map_err(|err| JobError::Depot(err.to_string()))?;

    let mut scratch = [0_u8; 256 * 1024];
    while written < expected_len {
        if control.is_canceled() {
            let _ = checkpoint_partial(&mut output, partial_path, written);
            return Err(JobError::Canceled);
        }
        if control.is_paused() {
            checkpoint_partial(&mut output, partial_path, written)?;
            durable = written;
            unsynced = 0;
            last_checkpoint = Instant::now();
        }
        while control.is_paused() {
            if control.is_canceled() {
                let _ = checkpoint_partial(&mut output, partial_path, written);
                return Err(JobError::Canceled);
            }
            thread::sleep(Duration::from_millis(150));
        }

        let remaining = (expected_len - written) as usize;
        let read_len = remaining.min(scratch.len());
        let read = match reader.read(&mut scratch[..read_len]) {
            Ok(read) => read,
            Err(error) => {
                let _ = checkpoint_partial(&mut output, partial_path, written);
                return Err(error.into());
            }
        };
        if read == 0 {
            break;
        }
        output.write_all(&scratch[..read])?;
        written = written.saturating_add(read as u64);
        unsynced = unsynced.saturating_add(read as u64);
        let checkpoint_due = (unsynced >= DOWNLOAD_CHECKPOINT_BYTES
            && last_checkpoint.elapsed() >= DOWNLOAD_CHECKPOINT_MIN_INTERVAL)
            || last_checkpoint.elapsed() >= DOWNLOAD_CHECKPOINT_MAX_INTERVAL;
        if checkpoint_due {
            checkpoint_partial(&mut output, partial_path, written)?;
            durable = written;
            unsynced = 0;
            last_checkpoint = Instant::now();
        }
        if last_progress_emit.elapsed() >= Duration::from_millis(250) || written == expected_len {
            let elapsed = last_progress_emit.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 {
                ((written.saturating_sub(last_progress_bytes)) as f64 / elapsed) as u64
            } else {
                0
            };
            progress_tx
                .send(Ok(DownloadProgress {
                    task_id: task_id.to_string(),
                    committed_bytes: 0,
                    in_flight_bytes: durable,
                    clear_in_flight: false,
                    retry_count: 0,
                    rate_bytes_per_second: rate,
                    retry_wait_ms: 0,
                    rate_limit_wait_ms: 0,
                }))
                .map_err(|err| JobError::Depot(err.to_string()))?;
            last_progress_emit = Instant::now();
            last_progress_bytes = written;
        }
    }
    checkpoint_partial(&mut output, partial_path, written)?;
    Ok(written)
}

fn checkpoint_partial(output: &mut File, path: &Path, written: u64) -> Result<(), JobError> {
    output.flush()?;
    output.sync_data()?;
    persist_partial_checkpoint(path, written)
}

fn existing_partial_task_progress(
    staged_chunks_root: &Path,
    chunks: &[ChunkRef],
) -> HashMap<String, u64> {
    build_pack_download_tasks(chunks)
        .into_iter()
        .filter_map(|task| {
            let expected = task.range_end.saturating_sub(task.range_start);
            let existing =
                durable_partial_len(&partial_range_path(staged_chunks_root, &task)).min(expected);
            (existing > 0).then(|| (task.id(), existing))
        })
        .collect()
}

#[derive(Debug, Clone)]
struct DownloadProgress {
    task_id: String,
    committed_bytes: u64,
    in_flight_bytes: u64,
    clear_in_flight: bool,
    retry_count: u32,
    rate_bytes_per_second: u64,
    retry_wait_ms: u64,
    rate_limit_wait_ms: u64,
}

#[derive(Debug, Clone)]
struct InstalledUpdateBase {
    version: String,
    manifest: VersionManifest,
    source_label: String,
}

fn usable_installed_version(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("unknown")
        || value.eq_ignore_ascii_case("not installed")
        || value.eq_ignore_ascii_case("installed")
        || value.eq_ignore_ascii_case("detecting")
    {
        None
    } else {
        Some(value.to_string())
    }
}

fn canonicalize_manifest_version(
    mut manifest: VersionManifest,
    requested_version: &str,
) -> VersionManifest {
    if let Some(version) = usable_installed_version(requested_version) {
        manifest.version = version;
    }
    manifest
}

fn load_installed_update_base(
    install_root: &Path,
    source: &DepotSource,
    catalog: &Catalog,
) -> Result<Option<InstalledUpdateBase>, JobError> {
    let marker = read_install_marker(install_root)?;
    if let Some(marker) = marker.as_ref() {
        if !install_marker_matches_source(marker, source) {
            return Err(JobError::Depot(format!(
                "install metadata belongs to '{}', not '{}'",
                marker.game_id, source.game_id
            )));
        }
    }

    let installed_manifest = read_installed_manifest(install_root)?;
    if let Some(manifest) = installed_manifest.as_ref() {
        let manifest_matches = sanitize_game_id(&manifest.game_id) == source.game_id
            || compact_game_id(&manifest.game_id) == compact_game_id(&source.game_id)
            || compact_game_id(&manifest.game_id) == compact_game_id(&source.game_dir_name);
        if !manifest_matches {
            return Err(JobError::Depot(format!(
                "installed manifest belongs to '{}', not '{}'",
                manifest.game_id, source.game_id
            )));
        }
    }

    let marker_version = marker
        .as_ref()
        .and_then(|marker| usable_installed_version(&marker.version));
    let manifest_version = installed_manifest
        .as_ref()
        .and_then(|manifest| usable_installed_version(&manifest.version));

    // state.0xo is authoritative because it is committed only after a completed
    // install/update. If manifest.0xo carries a stale version label, load the
    // canonical manifest for the committed marker version instead of guessing.
    if let Some(version) = marker_version {
        if !catalog_has_version(catalog, &version) {
            return Err(JobError::Depot(format!(
                "installed version '{}' from .0xolemon/state.0xo is not present in this game's catalog",
                version
            )));
        }

        let (manifest, source_label) = match installed_manifest {
            Some(manifest) if manifest_version.as_deref() == Some(version.as_str()) => (
                canonicalize_manifest_version(manifest, &version),
                ".0xolemon/state.0xo + manifest.0xo".to_string(),
            ),
            _ => (
                source.load_manifest(catalog, &version)?,
                ".0xolemon/state.0xo + catalog manifest".to_string(),
            ),
        };

        return Ok(Some(InstalledUpdateBase {
            version,
            manifest,
            source_label,
        }));
    }

    // Generic recovery path for installs whose marker was lost but whose
    // installed manifest is intact. This works for every game and never invokes
    // a title-specific signature scanner.
    if let Some(manifest) = installed_manifest {
        if let Some(version) = manifest_version {
            if !catalog_has_version(catalog, &version) {
                return Err(JobError::Depot(format!(
                    "installed version '{}' from .0xolemon/manifest.0xo is not present in this game's catalog",
                    version
                )));
            }
            return Ok(Some(InstalledUpdateBase {
                version: version.clone(),
                manifest: canonicalize_manifest_version(manifest, &version),
                source_label: ".0xolemon/manifest.0xo".to_string(),
            }));
        }
    }

    Ok(None)
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
    find_catalog_version_entry(catalog, version).is_some()
}

fn find_catalog_version_entry<'a>(
    catalog: &'a Catalog,
    requested_version: &str,
) -> Option<&'a CatalogVersion> {
    let requested = requested_version.trim();
    if requested.is_empty() {
        return None;
    }

    if let Some(exact) = catalog
        .versions
        .iter()
        .find(|entry| entry.version == requested)
    {
        return Some(exact);
    }

    // Firestore is the user-facing source of truth, while legacy depot catalogs
    // can still carry accidental suffixes in their version label. Among Us was
    // published as `v17.4I` in the remote catalog while Firestore correctly
    // exposes `v17.4`. We only use this relaxed match after exact lookup fails,
    // and only when it is unique, so labels like `v1.0-beta` and `v1.0-hotfix`
    // never collapse into an arbitrary target.
    let requested_key = version_numeric_core(requested)?;
    let mut matches = catalog.versions.iter().filter(|entry| {
        version_numeric_core(&entry.version).as_deref() == Some(requested_key.as_str())
    });

    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(first)
}

fn version_numeric_core(version: &str) -> Option<String> {
    let mut value = version.trim().to_ascii_lowercase();
    if let Some(stripped) = value.strip_prefix('v') {
        value = stripped.to_string();
    }

    let mut result = String::new();
    let mut saw_digit = false;
    let mut last_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_digit() {
            result.push(ch);
            saw_digit = true;
            last_was_separator = false;
            continue;
        }

        if saw_digit && matches!(ch, '.' | '-' | '_') {
            if !last_was_separator {
                result.push(ch);
                last_was_separator = true;
            }
            continue;
        }

        if saw_digit {
            break;
        }

        if !ch.is_whitespace() {
            return None;
        }
    }

    while result.ends_with(['.', '-', '_']) {
        result.pop();
    }

    if saw_digit && !result.is_empty() {
        Some(result)
    } else {
        None
    }
}

fn resolve_target_version(
    catalog: &Catalog,
    target_version: Option<String>,
) -> Result<String, JobError> {
    let target = target_version
        .filter(|version| !version.trim().is_empty() && version != "unknown")
        .unwrap_or_else(|| {
            catalog
                .effective_latest_version()
                .unwrap_or("unknown")
                .to_string()
        });
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

fn build_verified_local_chunk_sources(
    install_root: &Path,
    manifest: &VersionManifest,
    changed: &[FileEntry],
    control: &JobControl,
) -> Result<(HashMap<String, LocalChunkSource>, usize, usize), JobError> {
    let required_hashes = changed
        .iter()
        .flat_map(|file| file.chunks.iter().map(|chunk| chunk.hash.clone()))
        .collect::<HashSet<_>>();
    let mut out = HashMap::new();
    let mut reused = 0_usize;
    let mut rejected = 0_usize;

    for file in &manifest.files {
        let candidates = file
            .chunks
            .iter()
            .filter(|chunk| required_hashes.contains(&chunk.hash))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }

        let path = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
        let Ok(mut local_file) = File::open(&path) else {
            rejected = rejected.saturating_add(candidates.len());
            continue;
        };
        let local_size = local_file
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        for chunk in candidates {
            if control.is_canceled() {
                return Err(JobError::Canceled);
            }
            let end = chunk.file_offset.saturating_add(chunk.uncompressed_size);
            if end > local_size {
                rejected = rejected.saturating_add(1);
                continue;
            }
            if local_file.seek(SeekFrom::Start(chunk.file_offset)).is_err() {
                rejected = rejected.saturating_add(1);
                continue;
            }
            let mut bytes = vec![0_u8; chunk.uncompressed_size as usize];
            if local_file.read_exact(&mut bytes).is_err()
                || verify_chunk_bytes(chunk, &bytes).is_err()
            {
                rejected = rejected.saturating_add(1);
                continue;
            }
            if out
                .insert(
                    chunk.hash.clone(),
                    LocalChunkSource {
                        path: path.clone(),
                        offset: chunk.file_offset,
                        size: chunk.uncompressed_size,
                    },
                )
                .is_none()
            {
                reused = reused.saturating_add(1);
            }
        }
    }

    Ok((out, reused, rejected))
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

fn prepare_direct_stage(
    downloading_root: &Path,
    staging_root: &Path,
    files: &[FileEntry],
    target_version: &str,
    local_sources: &HashMap<String, LocalChunkSource>,
    control: &JobControl,
) -> Result<Option<DirectStagePlan>, JobError> {
    if !crate::platform::current_settings().direct_to_staging {
        return Ok(None);
    }
    let stage = DirectStagePlan::prepare(downloading_root, staging_root, files, target_version)?;
    stage.write_local_chunks(files, local_sources, control)?;
    Ok(Some(stage))
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
    let settings = crate::platform::current_settings();
    env::var("OXO_DOWNLOAD_WORKERS")
        .ok()
        .or_else(|| env::var("OXO_HF_DOWNLOAD_WORKERS").ok())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(settings.download_workers)
        .clamp(1, MAX_DOWNLOAD_WORKERS)
}

fn download_retry_count() -> u32 {
    let settings = crate::platform::current_settings();
    env::var("OXO_DOWNLOAD_RETRIES")
        .ok()
        .or_else(|| env::var("OXO_HF_DOWNLOAD_RETRIES").ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(settings.download_retries)
        .clamp(0, MAX_DOWNLOAD_RETRIES)
}

fn download_retry_delay(retry_count: u32) -> Duration {
    let capped = retry_count.min(6);
    let ceiling = 500_u64.saturating_mul(1_u64 << capped);
    let entropy = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .unsigned_abs();
    Duration::from_millis(entropy % ceiling.max(1))
}

fn sleep_with_control(delay: Duration, control: &JobControl) -> Result<(), JobError> {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if control.is_canceled() {
            return Err(JobError::Canceled);
        }
        while control.is_paused() {
            if control.is_canceled() {
                return Err(JobError::Canceled);
            }
            thread::sleep(Duration::from_millis(150));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(Duration::from_millis(250)));
    }
    Ok(())
}

fn pack_range_task_bytes() -> u64 {
    let settings = crate::platform::current_settings();
    if let Some(bytes) = env::var("OXO_PACK_RANGE_MB")
        .ok()
        .or_else(|| env::var("OXO_HF_RANGE_MB").ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|value| value.saturating_mul(1024 * 1024))
    {
        return bytes.clamp(MIN_PACK_RANGE_TASK_BYTES, MAX_PACK_RANGE_TASK_BYTES);
    }
    let initial = settings
        .pack_range_mb
        .saturating_mul(1024 * 1024)
        .clamp(MIN_ADAPTIVE_RANGE_BYTES, MAX_PACK_RANGE_TASK_BYTES);
    ADAPTIVE_RANGE_STATE
        .get_or_init(|| {
            Mutex::new(AdaptiveRangeState {
                range_bytes: initial,
                ewma_rate: 0.0,
                successful_samples: 0,
            })
        })
        .lock()
        .map(|state| state.range_bytes)
        .unwrap_or(initial)
}

fn observe_adaptive_range(rate_bytes_per_second: u64, failed: bool) {
    let initial = crate::platform::current_settings()
        .pack_range_mb
        .saturating_mul(1024 * 1024)
        .clamp(MIN_ADAPTIVE_RANGE_BYTES, MAX_PACK_RANGE_TASK_BYTES);
    let Ok(mut state) = ADAPTIVE_RANGE_STATE
        .get_or_init(|| {
            Mutex::new(AdaptiveRangeState {
                range_bytes: initial,
                ewma_rate: 0.0,
                successful_samples: 0,
            })
        })
        .lock()
    else {
        return;
    };
    if failed {
        state.range_bytes = (state.range_bytes / 2).max(MIN_ADAPTIVE_RANGE_BYTES);
        state.successful_samples = 0;
        return;
    }
    if rate_bytes_per_second == 0 {
        return;
    }
    let rate = rate_bytes_per_second as f64;
    state.ewma_rate = if state.ewma_rate == 0.0 {
        rate
    } else {
        state.ewma_rate * 0.8 + rate * 0.2
    };
    state.successful_samples = state.successful_samples.saturating_add(1);
    if state.successful_samples >= 8 {
        if state.ewma_rate >= 80.0 * 1024.0 * 1024.0 {
            state.range_bytes =
                (state.range_bytes.saturating_mul(2)).min(MAX_PACK_RANGE_TASK_BYTES);
        } else if state.ewma_rate <= 8.0 * 1024.0 * 1024.0 {
            state.range_bytes = (state.range_bytes / 2).max(MIN_ADAPTIVE_RANGE_BYTES);
        }
        state.successful_samples = 0;
    }
}

fn download_transfer_bytes(chunks: &[ChunkRef]) -> u64 {
    build_pack_download_tasks(chunks)
        .iter()
        .map(|task| task.range_end - task.range_start)
        .sum()
}

fn configure_download_metrics(
    journal: &mut JobJournal,
    chunks: &[ChunkRef],
    direct_to_staging: bool,
) {
    let payload_bytes = chunks
        .iter()
        .map(|chunk| chunk.compressed_size)
        .sum::<u64>();
    journal.metrics = DownloadMetrics {
        pipeline: if direct_to_staging {
            "direct-v2".to_string()
        } else {
            "chunk-cache-v1".to_string()
        },
        payload_bytes,
        overfetch_bytes: journal.bytes_total.saturating_sub(payload_bytes),
        ..DownloadMetrics::default()
    };
}

fn observe_download_progress(
    journal: &mut JobJournal,
    progress: &DownloadProgress,
    in_flight_bytes: u64,
) {
    let metrics = &mut journal.metrics;
    metrics.network_bytes = metrics
        .network_bytes
        .saturating_add(progress.committed_bytes);
    metrics.retry_wait_ms = metrics.retry_wait_ms.saturating_add(progress.retry_wait_ms);
    metrics.rate_limit_wait_ms = metrics
        .rate_limit_wait_ms
        .saturating_add(progress.rate_limit_wait_ms);
    metrics.peak_in_flight_bytes = metrics.peak_in_flight_bytes.max(in_flight_bytes);
    if progress.rate_bytes_per_second > 0 {
        observe_adaptive_range(progress.rate_bytes_per_second, false);
        metrics
            .throughput_samples
            .push(progress.rate_bytes_per_second);
        if metrics.throughput_samples.len() > 128 {
            metrics.throughput_samples.remove(0);
        }
        let mut sorted = metrics.throughput_samples.clone();
        sorted.sort_unstable();
        metrics.throughput_p50_bytes_per_second = percentile(&sorted, 50);
        metrics.throughput_p95_bytes_per_second = percentile(&sorted, 95);
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index.min(sorted.len() - 1)]
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

fn planned_temporary_space(files: &[FileEntry], network_bytes: u64) -> u64 {
    let staged_files = files.iter().map(|file| file.size).sum::<u64>();
    if crate::platform::current_settings().direct_to_staging {
        staged_files
    } else {
        staged_files.saturating_add(network_bytes)
    }
}

fn required_free_space(temporary_space: u64) -> u64 {
    const TWO_GIB: u64 = 2 * 1024 * 1024 * 1024;
    let safety_margin = temporary_space.saturating_mul(5).div_ceil(100).max(TWO_GIB);
    temporary_space.saturating_add(safety_margin)
}

fn validate_format_version(version: u32, label: &str) -> Result<(), JobError> {
    if matches!(version, LEGACY_FORMAT_VERSION | FORMAT_VERSION) {
        return Ok(());
    }
    Err(JobError::Depot(format!(
        "unsupported {label} format version {version}; supported versions are {LEGACY_FORMAT_VERSION} and {FORMAT_VERSION}"
    )))
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

pub fn abort_and_clean_job(
    app: &AppHandle,
    requested_game_id: Option<&str>,
) -> Result<(), JobError> {
    // Read the journal before deleting it so custom install locations are cleaned
    // correctly. The previous implementation always derived the default library
    // path from the currently selected game, which could leave the real staging
    // directory and journal behind.
    let active_journal = read_latest_journal(app).ok().flatten();
    let canceled_job_id = active_journal.as_ref().map(|journal| journal.id.clone());
    let game_id = active_journal
        .as_ref()
        .map(|journal| journal.game_id.as_str())
        .or(requested_game_id)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_GAME_ID);
    let source = DepotSource::for_game(game_id);
    let install_root = active_journal
        .as_ref()
        .map(|journal| journal.install_path.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| source.default_common_game_dir());
    let downloading_root = downloading_dir_for_install(&install_root, &source);

    // Remove current-job.json immediately so the Downloads tab clears at once.
    // The worker also clears it again when it observes cancellation, preventing a
    // final progress event from recreating the stale journal.
    if let Some(job_id) = canceled_job_id.as_deref() {
        clear_current_journal_if_matches(app, job_id)?;
    } else {
        clear_current_journal(app)?;
    }

    // Open pack/chunk handles can remain alive for a short moment while the worker
    // exits. Retry both cleanup and journal removal asynchronously. The job-id
    // guard prevents this late cleanup from deleting a newer job.
    let app_for_cleanup = app.clone();
    thread::spawn(move || {
        for attempt in 0..24 {
            if let Some(job_id) = canceled_job_id.as_deref() {
                let _ = clear_current_journal_if_matches(&app_for_cleanup, job_id);
            }
            match fs::remove_dir_all(&downloading_root) {
                Ok(()) => {
                    if let Some(job_id) = canceled_job_id.as_deref() {
                        let _ = clear_current_journal_if_matches(&app_for_cleanup, job_id);
                    }
                    return;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) if attempt < 23 => thread::sleep(Duration::from_millis(250)),
                Err(_) => return,
            }
        }
    });
    Ok(())
}

fn cleanup_committed_download_session(
    downloading_root: &Path,
    source: &DepotSource,
) -> Result<(), JobError> {
    if !downloading_root.exists() {
        return Ok(());
    }

    let session_path = downloading_root.join(DOWNLOAD_SESSION_FILE);
    if session_path.exists() {
        let bytes = fs::read(&session_path)?;
        let session: DownloadSessionMarker = serde_json::from_slice(&bytes)?;
        if session.status != "committed" || sanitize_game_id(&session.game_id) != source.game_id {
            return Ok(());
        }
    }

    // The install marker and assembled files are already committed. Remove the
    // complete game-specific staging tree: chunks, files, temporary files and
    // leftovers from older versions. Retries cover short Windows handle delays.
    remove_dir_all_with_retry(downloading_root, 24)
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

// Progress math helpers live in job/progress.rs

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
    let transport = fs::read(path)?;
    verify_compressed_chunk_bytes(chunk, &transport)?;
    let compressed = decode_transport_chunk(chunk, &transport)?;
    let data = decode_chunk_payload(chunk, &compressed)?;
    verify_chunk_bytes(chunk, &data)?;
    Ok(data)
}

fn decode_chunk_payload(chunk: &ChunkRef, encoded: &[u8]) -> Result<Vec<u8>, JobError> {
    match chunk.codec {
        ChunkCodec::Raw => Ok(encoded.to_vec()),
        ChunkCodec::Zstd => Ok(zstd::bulk::decompress(
            encoded,
            chunk.uncompressed_size as usize,
        )?),
    }
}

fn decode_transport_chunk(chunk: &ChunkRef, transport: &[u8]) -> Result<Vec<u8>, JobError> {
    let Some(encryption) = chunk.encryption.as_ref() else {
        return Ok(transport.to_vec());
    };
    if encryption.algorithm != DEPOT_ENCRYPTION_ALGORITHM {
        return Err(JobError::Depot(format!(
            "unsupported chunk encryption algorithm for {}: {}",
            chunk.hash, encryption.algorithm
        )));
    }
    let key_material = depot_crypto::resolve_key_material(None);
    let compressed = depot_crypto::decrypt_compressed_chunk(
        transport,
        &chunk.hash,
        &encryption.plaintext_compressed_sha256,
        &encryption.nonce,
        &key_material,
        &encryption.algorithm,
    )
    .map_err(|err| JobError::Depot(format!("decrypt chunk {} failed: {err}", chunk.hash)))?;
    if compressed.len() != encryption.plaintext_compressed_size as usize {
        return Err(JobError::Depot(format!(
            "decrypted compressed chunk size mismatch: {}",
            chunk.hash
        )));
    }
    let actual = sha256_bytes(&compressed);
    if actual != encryption.plaintext_compressed_sha256 {
        return Err(JobError::Depot(format!(
            "decrypted compressed chunk hash mismatch: {}",
            chunk.hash
        )));
    }
    Ok(compressed)
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

// Path/default helper functions live in job/paths.rs

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
    installed_version: &str,
) -> Result<(), JobError> {
    let installed_version = installed_version.trim();
    if installed_version.is_empty() || installed_version == "unknown" {
        return Err(JobError::Depot(
            "refusing to commit an install with an unknown version".to_string(),
        ));
    }
    let launch_executable = manifest
        .launch_executable
        .clone()
        .unwrap_or_else(|| default_launch_executable(&source.game_id));
    let marker = InstallMarker {
        // Always write the canonical launcher game id and the version selected
        // from catalog.json. Do not trust a stale `version` field inside a remote
        // manifest, otherwise a successful latest install can appear to roll back.
        game_id: source.game_id.clone(),
        version: installed_version.to_string(),
        installed_at: Utc::now().to_rfc3339(),
        launch_executable: Some(launch_executable.clone()),
    };
    write_install_marker_file(install_root, &marker)?;
    if manifest.version == installed_version {
        write_installed_manifest(install_root, manifest)?;
    } else {
        let canonical_manifest = canonicalize_manifest_version(manifest.clone(), installed_version);
        write_installed_manifest(install_root, &canonical_manifest)?;
    }
    crate::platform::register_install(
        app,
        &source.game_id,
        install_root,
        installed_version,
        &launch_executable,
    )
    .map_err(JobError::Depot)?;
    if let Some(executable) = safe_join(install_root, &launch_executable) {
        let _ = create_game_shortcut(app, source, install_root, &executable, &launch_executable);
        let _ = crate::steam_integration::ensure_game_shortcut(
            app,
            &source.game_id,
            &source.game_dir_name,
            install_root,
            &launch_executable,
            Some(&executable),
        );
    }
    Ok(())
}

fn compact_game_id(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
}

fn install_marker_matches_source(marker: &InstallMarker, source: &DepotSource) -> bool {
    if marker.game_id == source.game_id {
        return true;
    }

    let marker_clean = sanitize_game_id(&marker.game_id);
    if marker_clean == source.game_id {
        return true;
    }

    // Backward compatibility for markers written from remote/display names, e.g.
    // "Geometry-Dash" or "Geometry Dash" vs canonical id "geometry-dash".
    let marker_compact = compact_game_id(&marker.game_id);
    marker_compact == compact_game_id(&source.game_id)
        || marker_compact == compact_game_id(&source.game_dir_name)
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
        clear_current_journal_if_matches(app, &journal.id)?;
        return Err(JobError::Canceled);
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
                clear_current_journal_if_matches(app, &journal.id)?;
                return Err(JobError::Canceled);
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

// Journal progress helper functions live in job/progress.rs

pub fn read_latest_journal(app: &AppHandle) -> Result<Option<JobJournal>, JobError> {
    let path = journal_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read(&path)?;
    match serde_json::from_slice::<JobJournal>(&data) {
        Ok(journal) => Ok(Some(journal)),
        Err(_) => {
            // A process exit can interrupt an old non-atomic write. Do not let a
            // malformed current-job.json permanently trap the launcher in Downloads.
            let corrupt_path = path.with_file_name(format!(
                "current-job.corrupt-{}.json",
                Utc::now().timestamp_millis()
            ));
            if fs::rename(&path, &corrupt_path).is_err() {
                let _ = remove_file_with_retry(&path, 12);
            }
            let _ = app.emit("launcher://job-cleared", ());
            Ok(None)
        }
    }
}

/// Delete the active job journal and tell every frontend window to clear its
/// download state. Missing files are treated as success, making cancel idempotent.
pub fn clear_current_journal(app: &AppHandle) -> Result<(), JobError> {
    let path = journal_path(app)?;
    remove_file_with_retry(&path, 12)?;
    let _ = app.emit("launcher://job-cleared", ());
    Ok(())
}

/// Clear only the journal belonging to the canceled job. This protects a newly
/// started job from a late exit callback belonging to the previous worker.
fn clear_current_journal_if_matches(
    app: &AppHandle,
    expected_job_id: &str,
) -> Result<(), JobError> {
    let path = journal_path(app)?;
    if !path.exists() {
        let _ = app.emit("launcher://job-cleared", ());
        return Ok(());
    }

    let matches = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<JobJournal>(&bytes).ok())
        .map(|journal| journal.id == expected_job_id)
        .unwrap_or(true);
    if matches {
        remove_file_with_retry(&path, 12)?;
        let _ = app.emit("launcher://job-cleared", ());
    }
    Ok(())
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
        metrics: DownloadMetrics::default(),
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
    let temp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&temp)?;
        file.write_all(&data)?;
        file.sync_all()?;
    }
    if let Err(first_error) = fs::rename(&temp, &path) {
        // Some Windows filesystems refuse replacement by rename. Fall back to a
        // short remove-and-rename sequence while keeping the complete temp file.
        remove_file_with_retry(&path, 4)?;
        fs::rename(&temp, &path).map_err(|_| first_error)?;
    }
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

#[cfg(test)]
mod downloader_v2_tests {
    use super::*;

    #[test]
    fn v1_chunk_without_codec_defaults_to_zstd() {
        let chunk: ChunkRef = serde_json::from_value(serde_json::json!({
            "hash": "abc",
            "fileOffset": 0,
            "uncompressedSize": 3,
            "packId": "pack-00000",
            "packOffset": 0,
            "compressedSize": 3,
            "compressedSha256": "def"
        }))
        .unwrap();
        assert_eq!(chunk.codec, ChunkCodec::Zstd);
    }

    #[test]
    fn rate_limit_headers_drive_shared_cooldown() {
        let mut headers = HeaderMap::new();
        headers.insert("ratelimit", "\"resolvers\";r=0;t=271".parse().unwrap());
        assert_eq!(rate_limit_remaining(&headers), Some(0));
        assert_eq!(rate_limit_delay(&headers), Some(Duration::from_secs(271)));
    }

    #[test]
    fn retry_policy_does_not_retry_terminal_http_failures() {
        assert!(JobError::NotFound("missing".to_string())
            .retry_delay(1)
            .is_none());
        assert!(JobError::Unauthorized("denied".to_string())
            .retry_delay(1)
            .is_none());
        assert!(JobError::Transient("timeout".to_string())
            .retry_delay(1)
            .is_some());
    }

    #[test]
    fn disk_admission_includes_two_gib_minimum_margin() {
        let one_gib = 1024_u64 * 1024 * 1024;
        assert_eq!(required_free_space(one_gib), 3 * one_gib);
        let hundred_gib = 100 * one_gib;
        assert_eq!(required_free_space(hundred_gib), 105 * one_gib);
    }

    #[test]
    fn partial_resume_truncates_to_durable_checkpoint() {
        let root = env::temp_dir().join(format!(
            "0xolemon-checkpoint-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&root).unwrap();
        let partial = root.join("range.part");
        fs::write(&partial, vec![7_u8; 32]).unwrap();
        persist_partial_checkpoint(&partial, 16).unwrap();
        normalize_partial_file(&partial, 32).unwrap();
        assert_eq!(partial_file_len(&partial), 16);
        assert_eq!(durable_partial_len(&partial), 16);
        fs::remove_file(partial_checkpoint_path(&partial)).unwrap();
        fs::remove_file(&partial).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn scheduled_update_window_supports_daytime_and_overnight_ranges() {
        assert!(time_in_update_window(3 * 60, "02:00", "06:00"));
        assert!(!time_in_update_window(8 * 60, "02:00", "06:00"));
        assert!(time_in_update_window(23 * 60, "22:00", "04:00"));
        assert!(time_in_update_window(2 * 60, "22:00", "04:00"));
        assert!(!time_in_update_window(12 * 60, "22:00", "04:00"));
        assert!(time_in_update_window(12 * 60, "00:00", "00:00"));
    }

    #[test]
    fn catalog_version_lookup_accepts_unique_legacy_suffix_alias() {
        let catalog = test_catalog(&["v17.4I"]);
        let entry = find_catalog_version_entry(&catalog, "v17.4")
            .expect("Firestore version should resolve to legacy depot label");

        assert_eq!(entry.version, "v17.4I");
        assert_eq!(
            resolve_target_version(&catalog, Some("v17.4".to_string())).unwrap(),
            "v17.4"
        );
        assert!(catalog_has_version(&catalog, "v17.4"));
    }

    #[test]
    fn catalog_version_lookup_rejects_ambiguous_suffix_aliases() {
        let catalog = test_catalog(&["v1.0-beta", "v1.0-hotfix"]);

        assert!(find_catalog_version_entry(&catalog, "v1.0").is_none());
        assert!(resolve_target_version(&catalog, Some("v1.0".to_string())).is_err());
    }

    fn test_catalog(versions: &[&str]) -> Catalog {
        Catalog {
            format_version: LEGACY_FORMAT_VERSION,
            game_id: "among-us".to_string(),
            latest_version: versions.last().map(|value| value.to_string()),
            versions: versions
                .iter()
                .map(|version| CatalogVersion {
                    version: (*version).to_string(),
                    manifest_path: format!("versions/{version}/manifest.json"),
                    total_size: 0,
                    file_count: 0,
                    chunk_count: 0,
                    created_at: "2026-06-22T00:00:00Z".to_string(),
                })
                .collect(),
            packs: Vec::new(),
            signature: None,
        }
    }
}
