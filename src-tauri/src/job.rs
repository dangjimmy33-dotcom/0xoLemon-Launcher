use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike, Utc};
use fastcdc::v2020::StreamCDC;
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

/// On Windows, paths longer than ~200 characters risk hitting the MAX_PATH (260)
/// limit when combined with file extensions or sub-paths added later.
/// Prepending the `\\?\` prefix opts the path into the extended-length path
/// API, allowing up to 32 767 characters.  On other platforms this is a no-op.
///
/// We apply this proactively to *all* paths that touch the staging/downloading
/// directories, because large games (50 GB+) frequently have deeply-nested
/// asset hierarchies that exceed MAX_PATH once the store root is included.
#[allow(unused_variables)]
fn long_path(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let s = path.to_string_lossy();
        // An existing extended prefix is already normalized.
        if s.starts_with("\\\\?\\") {
            return path.to_path_buf();
        }

        // Ensure the path is absolute
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };

        // We must remove all forward slashes, as the \\?\ prefix disables Windows' automatic conversion
        let abs_s = abs.to_string_lossy().replace('/', "\\");

        if let Some(unc) = abs_s.strip_prefix("\\\\") {
            return PathBuf::from(format!("\\\\?\\UNC\\{unc}"));
        }

        // The \\?\ prefix also disables parsing of . and ..
        // We do a simple string replacement for \.\ and \..\
        // For a robust solution, we can try to canonicalize the parent directory if it exists.
        if let Some(parent) = abs.parent() {
            if let Ok(mut canon) = std::fs::canonicalize(parent) {
                if let Some(file_name) = abs.file_name() {
                    canon.push(file_name);
                }
                return canon; // canonicalize already adds the \\?\ prefix on Windows
            }
        }

        PathBuf::from(format!("\\\\?\\{abs_s}"))
    }
    #[cfg(not(target_os = "windows"))]
    path.to_path_buf()
}

const DEFAULT_LOCAL_DEPOT: &str = "E:\\007Launcher\\depot\\007-first-light";
const DEFAULT_GAME_ID: &str = "007-first-light";
const DEFAULT_GAME_DIR_NAME: &str = "007 First Light";
const DEFAULT_STORE_ROOT: &str = crate::platform::DEFAULT_LIBRARY_ROOT;
const INSTALL_MARKER_DIR: &str = ".0xolemon";
const INSTALL_MARKER_FILE: &str = "state.0xo";
const LEGACY_INSTALL_MARKER_FILE: &str = "install.json";
const INSTALLED_MANIFEST_FILE: &str = "manifest.0xo";
const APPLIED_PATCH_MANIFEST_FILE: &str = "patch-manifest.0xo";
const DOWNLOAD_SESSION_FILE: &str = "depot-session.json";
const STATE_MAGIC: &[u8] = b"0XOSTATE1\n";
const STATE_KEY: &[u8] = b"0xoLemon-local-install-state-v1";
const MAX_DOWNLOAD_WORKERS: usize = 64;
const MAX_DOWNLOAD_RETRIES: u32 = 5;
const PACK_RANGE_MERGE_GAP: u64 = 4 * 1024 * 1024;
const MIN_PACK_RANGE_TASK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PACK_RANGE_TASK_BYTES: u64 = 64 * 1024 * 1024;
const VERIFY_PROGRESS_EVENT: &str = "launcher://verify-progress";
const DOWNLOAD_TELEMETRY_EVENT: &str = "launcher://download-telemetry";
const VERIFY_READ_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const DOWNLOAD_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_CHECKPOINT_MIN_INTERVAL: Duration = Duration::from_secs(1);
const DOWNLOAD_CHECKPOINT_MAX_INTERVAL: Duration = Duration::from_secs(2);
const SEQUENTIAL_PREFETCH_MAX_FILES: usize = 256;
const SEQUENTIAL_COMMIT_BATCH_FILES: usize = 256;
const JOB_UI_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const JOB_JOURNAL_PERSIST_INTERVAL: Duration = Duration::from_secs(2);
const CIRCUIT_BREAKER_FAILURES: u32 = 3;
const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(30);
const TRANSPORT_PIPELINE_V3: &str = "transport-pipeline-v3";

fn transport_pipeline_v3_enabled() -> bool {
    env::var("OXO_TRANSPORT_PIPELINE_V3")
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "disabled"
            )
        })
        .unwrap_or(true)
}

fn journal_uses_transport_v3(journal: &JobJournal) -> bool {
    journal.pipeline_version == TRANSPORT_PIPELINE_V3
}

fn journal_uses_verified_stage(journal: &JobJournal) -> bool {
    journal_uses_transport_v3(journal) || journal.pipeline_version == "verified-stage-v2"
}

mod dependencies;
mod direct;
mod paths;
mod progress;
mod sequential;
mod transport;

use dependencies::{
    create_game_shortcut, create_game_shortcut_no_exe, ensure_game_dependencies,
    launch_option_processes, remove_game_shortcut,
};
use direct::DirectStagePlan;
use paths::*;
use progress::*;
use sequential::{
    RecoveryOutcome, SequentialUpdateSession, TransactionCommitProof, VerifiedFileWriter,
    VerifiedStageSession,
};
use transport::{
    cleanup_xet_spool, probe_hf_pack, select_transport, source_identities_match,
    stream_hf_xet_pack_to_spool, AdaptiveGovernor, DownloadTelemetry, DownloadTransportKind,
    HfPackMetadata, HfRepoLocation, PackTransportPlan, TransportCandidate, TransportOperation,
};
static RUNNING_GAMES: OnceLock<Mutex<std::collections::HashMap<String, u32>>> = OnceLock::new();
static DOWNLOAD_TELEMETRY_RATES: OnceLock<Mutex<HashMap<String, TelemetryRateState>>> =
    OnceLock::new();
static DOWNLOAD_RUNTIME_TELEMETRY: OnceLock<Mutex<HashMap<String, RuntimeTransportState>>> =
    OnceLock::new();
static AUTOMATIC_SCAN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static LAST_POST_DISCOVERY_SCAN: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TelemetryRateState {
    sampled_at: Instant,
    wire_bytes: u64,
    apply_bytes: u64,
    wire_rate_ewma: f64,
    apply_rate_ewma: f64,
}

#[derive(Debug, Default, Clone, Copy)]
struct RuntimeTransportState {
    active_connections: usize,
    queue_bytes: u64,
}

fn running_games() -> &'static Mutex<std::collections::HashMap<String, u32>> {
    RUNNING_GAMES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn automatic_scan_lock() -> &'static Mutex<()> {
    AUTOMATIC_SCAN_LOCK.get_or_init(|| Mutex::new(()))
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
    #[error("staging data is missing: {0}")]
    StageMissing(String),
    #[error("staging session is missing: {0}")]
    SessionMissing(String),
    #[error("staging session does not match the active job: {0}")]
    SessionMismatch(String),
    #[error("file is locked by another process: {0}")]
    FileLocked(String),
    #[error("not enough free disk space while writing: {0}")]
    DiskFull(String),
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
            Self::FileLocked(_) => Some(download_retry_delay(retry_count)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSnapshot {
    #[serde(default = "default_game_id_string")]
    pub game_id: String,
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

#[derive(Debug, Clone)]
struct ReconciledInstall {
    game_id: String,
    install_path: PathBuf,
    marker: InstallMarker,
}

const AUTO_PATCH_POLL_INTERVAL: Duration = Duration::from_secs(60);
const AUTO_PATCH_RETRY_BACKOFF: Duration = Duration::from_secs(5 * 60);
const AUTO_PATCH_STARTUP_DELAY: Duration = Duration::from_secs(2);

pub fn start_auto_update_scheduler(app: AppHandle, control: Arc<JobControl>) {
    let patch_app = app.clone();
    let patch_control = control.clone();
    thread::spawn(move || {
        let mut last_attempts = HashMap::<String, (String, Instant)>::new();
        // Give the WebView time to subscribe to launcher://job before a small
        // hotfix can complete, otherwise the user never sees its progress.
        thread::sleep(AUTO_PATCH_STARTUP_DELAY);
        loop {
            if let Ok(_scan_guard) = automatic_scan_lock().try_lock() {
                if let Err(error) = auto_patch_tick(&patch_app, &patch_control, &mut last_attempts)
                {
                    let _ = patch_app.emit(
                        "launcher://auto-update",
                        AutoUpdateEvent {
                            state: "error".to_string(),
                            message: error,
                            game_id: None,
                        },
                    );
                }
            }
            thread::sleep(AUTO_PATCH_POLL_INTERVAL);
        }
    });

    thread::spawn(move || {
        let mut last_attempts = HashMap::<String, Instant>::new();
        thread::sleep(Duration::from_secs(20));
        loop {
            if let Ok(_scan_guard) = automatic_scan_lock().try_lock() {
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
            }
            thread::sleep(Duration::from_secs(300));
        }
    });
}

pub fn start_post_discovery_scan(app: AppHandle, control: Arc<JobControl>) {
    let Some(generation) = crate::install_discovery::automatic_jobs_generation() else {
        return;
    };
    let mut scanned = LAST_POST_DISCOVERY_SCAN.load(Ordering::Acquire);
    loop {
        if scanned >= generation {
            return;
        }
        match LAST_POST_DISCOVERY_SCAN.compare_exchange_weak(
            scanned,
            generation,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(current) => scanned = current,
        }
    }
    thread::spawn(move || {
        let Ok(_scan_guard) = automatic_scan_lock().lock() else {
            return;
        };
        let mut patch_attempts = HashMap::<String, (String, Instant)>::new();
        if let Err(error) = auto_patch_tick(&app, &control, &mut patch_attempts) {
            let _ = app.emit(
                "launcher://auto-update",
                AutoUpdateEvent {
                    state: "error".to_string(),
                    message: error,
                    game_id: None,
                },
            );
            return;
        }
        let mut update_attempts = HashMap::<String, Instant>::new();
        if let Err(error) = auto_update_tick(&app, &control, &mut update_attempts) {
            let _ = app.emit(
                "launcher://auto-update",
                AutoUpdateEvent {
                    state: "error".to_string(),
                    message: error,
                    game_id: None,
                },
            );
        }
    });
}

fn automatic_job_can_start(app: &AppHandle, control: &Arc<JobControl>) -> Result<bool, String> {
    if !crate::install_discovery::automatic_jobs_ready() {
        return Ok(false);
    }
    if control.is_running() {
        return Ok(false);
    }
    if crate::platform::get_runtime_states(app)?
        .iter()
        .any(|runtime| runtime.running)
    {
        return Ok(false);
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
                    | JobStatus::Verified
            )
        })
    {
        return Ok(false);
    }
    if current_journal_has_pending_transaction(app).map_err(|error| error.to_string())? {
        return Ok(false);
    }
    Ok(true)
}

fn current_journal_has_pending_transaction(app: &AppHandle) -> Result<bool, JobError> {
    let Some(journal) = read_latest_journal(app)? else {
        return Ok(false);
    };
    if !matches!(
        journal.kind.as_str(),
        "install" | "update" | "repair" | "patch"
    ) || journal.install_path.trim().is_empty()
    {
        return Ok(false);
    }
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    SequentialUpdateSession::has_pending_transaction(&downloading_root, &journal.id)
}

fn ensure_no_pending_file_transaction(app: &AppHandle) -> Result<(), JobError> {
    if current_journal_has_pending_transaction(app)? {
        return Err(JobError::Depot(
            "A committed file transaction still owns rollback backups. Resume the current job before starting another install, update, repair, or patch."
                .to_string(),
        ));
    }
    Ok(())
}

fn patch_attempt_is_throttled(
    last_attempts: &HashMap<String, (String, Instant)>,
    game_id: &str,
    patch_id: &str,
) -> bool {
    last_attempts
        .get(game_id)
        .is_some_and(|(last_patch_id, attempted_at)| {
            last_patch_id == patch_id && attempted_at.elapsed() < AUTO_PATCH_RETRY_BACKOFF
        })
}

fn automatic_updates_allowed_now(settings: &crate::platform::LauncherSettings) -> bool {
    match settings.game_update_mode {
        crate::platform::GameUpdateMode::Manual => false,
        crate::platform::GameUpdateMode::Automatic => true,
        crate::platform::GameUpdateMode::Scheduled => time_in_update_window(
            Local::now().hour() as u16 * 60 + Local::now().minute() as u16,
            &settings.game_update_schedule_start,
            &settings.game_update_schedule_end,
        ),
    }
}

fn auto_patch_tick(
    app: &AppHandle,
    control: &Arc<JobControl>,
    last_attempts: &mut HashMap<String, (String, Instant)>,
) -> Result<(), String> {
    if !automatic_job_can_start(app, control)? {
        return Ok(());
    }
    let installs = reconciled_installs(app)?;

    for install in installs {
        let Some(version) = usable_installed_version(&install.marker.version) else {
            continue;
        };
        let source = DepotSource::for_game(&install.game_id);
        let patch_manifest = match load_patch_manifest(&source, &version) {
            Ok(Some(manifest)) => manifest,
            Ok(None) => continue,
            Err(error) => {
                if !patch_attempt_is_throttled(
                    last_attempts,
                    &install.game_id,
                    "manifest-unavailable",
                ) {
                    last_attempts.insert(
                        install.game_id.clone(),
                        ("manifest-unavailable".to_string(), Instant::now()),
                    );
                    let _ = app.emit(
                        "launcher://auto-update",
                        AutoUpdateEvent {
                            state: "error".to_string(),
                            message: format!(
                                "Could not check hotfixes for {}: {}",
                                install.game_id, error
                            ),
                            game_id: Some(install.game_id.clone()),
                        },
                    );
                }
                continue;
            }
        };
        let patch_id = patch_manifest.created_at.trim();
        if patch_id.is_empty()
            || install.marker.applied_patch_id.as_deref() == Some(patch_id)
            || patch_attempt_is_throttled(last_attempts, &install.game_id, patch_id)
        {
            continue;
        }

        last_attempts.insert(
            install.game_id.clone(),
            (patch_id.to_string(), Instant::now()),
        );
        let _ = app.emit(
            "launcher://auto-update",
            AutoUpdateEvent {
                state: "patching".to_string(),
                message: format!("Applying hotfix for {} ({})", install.game_id, version),
                game_id: Some(install.game_id.clone()),
            },
        );
        spawn_patch_job(
            app.clone(),
            control.clone(),
            install.install_path.display().to_string(),
            Some(version),
            Some(install.game_id),
        )
        .map_err(|error| error.to_string())?;
        break;
    }
    Ok(())
}

fn auto_update_tick(
    app: &AppHandle,
    control: &Arc<JobControl>,
    last_attempts: &mut HashMap<String, Instant>,
) -> Result<(), String> {
    let settings = crate::platform::current_settings();
    if !automatic_updates_allowed_now(&settings) {
        return Ok(());
    }
    if !automatic_job_can_start(app, control)? {
        return Ok(());
    }
    let installs = reconciled_installs(app)?;

    for install in installs {
        if last_attempts
            .get(&install.game_id)
            .is_some_and(|attempt| attempt.elapsed() < Duration::from_secs(30 * 60))
        {
            continue;
        }
        let source = DepotSource::for_game(&install.game_id);
        let Some(installed_version) = usable_installed_version(&install.marker.version) else {
            continue;
        };
        let catalog = match source.load_catalog() {
            Ok(catalog) => catalog,
            Err(_) => continue,
        };
        let Some(latest) = catalog.effective_latest_version().map(str::to_string) else {
            continue;
        };
        if versions_equivalent(&installed_version, &latest) {
            continue;
        }
        last_attempts.insert(install.game_id.clone(), Instant::now());
        let _ = app.emit(
            "launcher://auto-update",
            AutoUpdateEvent {
                state: "starting".to_string(),
                message: format!(
                    "Starting automatic update for {}: {} â†’ {}",
                    install.game_id, installed_version, latest
                ),
                game_id: Some(install.game_id.clone()),
            },
        );
        spawn_update_job(
            app.clone(),
            control.clone(),
            install.install_path.display().to_string(),
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
    pub applied_patch_id: Option<String>,
    #[serde(default)]
    pub discovery_status: String,
    #[serde(default)]
    pub candidate_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoverableInstallMarker {
    pub game_id: String,
    pub version: String,
    pub launch_executable: String,
    pub applied_patch_id: Option<String>,
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
    /// Monotonic user-facing transfer bytes across resume/replanning.
    #[serde(default)]
    pub logical_bytes_done: u64,
    #[serde(default)]
    pub logical_bytes_total: u64,
    /// Bytes already available before the current remaining-work plan.
    #[serde(default)]
    pub session_base_bytes: u64,
    /// Verified bytes durably written to the staging transaction.
    #[serde(default)]
    pub apply_bytes_done: u64,
    #[serde(default)]
    pub apply_bytes_total: u64,
    #[serde(default)]
    pub durable_bytes: u64,
    /// Bytes observed at the network transport, independent of disk checkpoints.
    #[serde(default)]
    pub wire_bytes_done: u64,
    #[serde(default)]
    pub current_file: String,
    #[serde(default)]
    pub pipeline_version: String,
    #[serde(default)]
    pub transport_plans: Vec<PackTransportPlan>,
    #[serde(default)]
    pub current_transport: DownloadTransportKind,
    #[serde(default)]
    pub stall_reason: String,
    #[serde(default)]
    pub commit_state: String,
    #[serde(default)]
    pub planned_files: Vec<String>,
    pub retry_count: u32,
    pub resumable: bool,
    pub updated_at: String,
    pub steps: Vec<JobStep>,
    pub logs: Vec<JobLog>,
    #[serde(default)]
    pub metrics: DownloadMetrics,
    /// Set when a patch job commits – lets the frontend immediately clear the
    /// pending-patch indicator without waiting for a GameInstallState refresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_patch_id: Option<String>,
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
    #[serde(default)]
    pub disk_read_bytes: u64,
    #[serde(default)]
    pub disk_write_bytes: u64,
    #[serde(default)]
    pub resume_rehash_bytes: u64,
    #[serde(default)]
    pub sync_wait_ms: u64,
    #[serde(default)]
    pub commit_wait_ms: u64,
    #[serde(default)]
    pub allocation_reserved_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation_fallback_reason: Option<String>,
    #[serde(default)]
    pub xet_bytes: u64,
    #[serde(default)]
    pub raw_range_bytes: u64,
    #[serde(default)]
    pub wire_bytes: u64,
    #[serde(default)]
    pub decode_wait_ms: u64,
    #[serde(default)]
    pub writer_wait_ms: u64,
    #[serde(default)]
    pub checkpoint_wait_ms: u64,
    #[serde(default)]
    pub ttfb_p50_ms: u64,
    #[serde(default)]
    pub ttfb_p95_ms: u64,
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
        game_id: source.game_id.clone(),
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
    snapshot_for_fresh_install_cancellable(app, target_version, game_id, None)
}

pub fn start_pending_update_recovery(app: AppHandle, control: Arc<JobControl>) {
    let Ok(Some(mut journal)) = read_latest_journal(&app) else {
        return;
    };
    if !matches!(
        journal.kind.as_str(),
        "install" | "update" | "repair" | "patch"
    ) {
        return;
    }
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    if !SequentialUpdateSession::has_pending_transaction(&downloading_root, &journal.id)
        .unwrap_or(true)
    {
        return;
    }

    control.set_running(true);
    thread::spawn(move || {
        let result = (|| -> Result<(), JobError> {
            let Some(mut session) = SequentialUpdateSession::open_owned_for_recovery(
                &downloading_root,
                &journal,
                &install_root,
            )?
            else {
                return Err(JobError::SessionMissing(
                    "owned update transaction metadata is unavailable".to_string(),
                ));
            };
            match session.recover(&install_root, &journal.to_version)? {
                RecoveryOutcome::AlreadyCommitted => {
                    journal.status = JobStatus::Committed;
                    journal.phase = if journal.kind == "patch" {
                        "Patch applied".to_string()
                    } else if journal.kind == "repair" {
                        "Repair committed".to_string()
                    } else {
                        "Committed".to_string()
                    };
                    journal.commit_state = "committed".to_string();
                    journal.overall_progress = 1.0;
                    for step in &mut journal.steps {
                        step.status = StepStatus::Completed;
                        step.progress = 1.0;
                    }
                    append_log(
                        &mut journal,
                        "info",
                        "Startup recovery completed committed transaction cleanup",
                    );
                    if let Err(error) = session.cleanup_session_files() {
                        append_log(
                            &mut journal,
                            "warning",
                            &format!("Committed session cleanup remains pending: {error}"),
                        );
                    }
                }
                RecoveryOutcome::Ready => {
                    journal.status = JobStatus::Failed;
                    journal.phase = "Ready to resume".to_string();
                    mark_running_step_failed(&mut journal);
                    append_log(
                        &mut journal,
                        "warning",
                        "Startup recovery restored the complete previous install; Resume can continue safely",
                    );
                }
            }
            persist_and_emit(&app, &journal)
        })();
        if let Err(error) = result {
            journal.status = JobStatus::Failed;
            journal.phase = "Recovery failed".to_string();
            mark_running_step_failed(&mut journal);
            append_log(
                &mut journal,
                "error",
                &format!("Startup recovery failed: {error}"),
            );
            let _ = persist_and_emit(&app, &journal);
        }
        control.set_running(false);
    });
}

pub fn snapshot_for_fresh_install_cancellable(
    app: &AppHandle,
    target_version: Option<String>,
    game_id: Option<String>,
    canceled: Option<&AtomicBool>,
) -> Result<LauncherSnapshot, JobError> {
    ensure_plan_not_canceled(canceled)?;
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    ensure_plan_not_canceled(canceled)?;
    let selected_version = resolve_target_version(&catalog, target_version)?;
    let manifest = source.load_manifest(&catalog, &selected_version)?;
    ensure_plan_not_canceled(canceled)?;
    let default_install = source.default_common_game_dir();
    let cache_size =
        match downloading_chunk_cache_size_cancellable(&default_install, &source, canceled) {
            Ok(size) => size,
            Err(JobError::Canceled) => return Err(JobError::Canceled),
            Err(_) => 0,
        };
    let cache_path = downloading_chunk_cache_path(&default_install, &source)
        .display()
        .to_string();
    let cache_free_space = downloading_cache_free_space(&default_install, &source);

    let update_size = estimate_install_download_bytes(
        Some(&downloading_chunk_cache_path(&default_install, &source)),
        &manifest,
    );
    ensure_plan_not_canceled(canceled)?;
    let temporary_space = planned_temporary_space(&manifest.files, update_size);
    Ok(LauncherSnapshot {
        game_id: source.game_id.clone(),
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
    snapshot_for_install_cancellable(app, install_path, target_version, game_id, None)
}

pub fn snapshot_for_install_cancellable(
    app: &AppHandle,
    install_path: &Path,
    target_version: Option<String>,
    game_id: Option<String>,
    canceled: Option<&AtomicBool>,
) -> Result<LauncherSnapshot, JobError> {
    ensure_plan_not_canceled(canceled)?;
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    ensure_plan_not_canceled(canceled)?;
    let selected_version = resolve_target_version(&catalog, target_version)?;
    let latest_version = catalog
        .effective_latest_version()
        .unwrap_or("unknown")
        .to_string();
    let installed_base = load_installed_update_base(install_path, &source, &catalog)?;
    ensure_plan_not_canceled(canceled)?;
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
            ensure_plan_not_canceled(canceled)?;
            let changed_targets = changed_target_files(&base.manifest, &to);
            let update_size =
                estimate_missing_download_bytes(Some(&staged_chunks_root), &base.manifest, &to);
            (
                changed_files_between(&base.manifest, &to),
                update_size,
                to.total_size,
                planned_update_temporary_space(&changed_targets, update_size),
            )
        }
    };
    ensure_plan_not_canceled(canceled)?;
    let cache_size = match downloading_chunk_cache_size_cancellable(install_path, &source, canceled)
    {
        Ok(size) => size,
        Err(JobError::Canceled) => return Err(JobError::Canceled),
        Err(_) => 0,
    };
    let cache_path = downloading_chunk_cache_path(install_path, &source)
        .display()
        .to_string();
    let cache_free_space = downloading_cache_free_space(install_path, &source);

    Ok(LauncherSnapshot {
        game_id: source.game_id.clone(),
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

fn ensure_plan_not_canceled(canceled: Option<&AtomicBool>) -> Result<(), JobError> {
    if canceled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err(JobError::Canceled);
    }
    Ok(())
}

pub fn spawn_update_job(
    app: AppHandle,
    control: Arc<JobControl>,
    install_path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, JobError> {
    ensure_no_pending_file_transaction(&app)?;
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, target_version)?;

    let install_root = PathBuf::from(&install_path);

    // Try to add Windows Defender exclusion to avoid I/O errors during update
    if let Some(parent) = install_root.parent() {
        if let Err(e) = crate::defender_exclusion::add_defender_exclusion(parent) {
            eprintln!("[DEFENDER] Could not add exclusion for install: {}", e);
        }
    }

    // Also add exclusion for download folder (dl/)
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    if let Some(dl_parent) = downloading_root.parent() {
        if let Err(e) = crate::defender_exclusion::add_defender_exclusion(dl_parent) {
            eprintln!("[DEFENDER] Could not add exclusion for downloads: {}", e);
        }
    }

    let mut journal = default_journal(
        &source.game_id,
        "update",
        install_path,
        "detecting",
        &target_version,
        0,
    );
    journal.pipeline_version = if transport_pipeline_v3_enabled() {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "sequential-stage-v1+verified-patch-v2".to_string()
    };
    journal.steps[0] = step(
        "Read install state",
        "Load .0xolemon state and the installed manifest",
    );
    journal.steps[1] = step(
        "Plan update",
        "Compare manifests and validate reusable local chunks",
    );
    spawn_update_journal(app, control, journal)
}

pub fn resume_update_job(
    app: AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    if journal.kind != "update" {
        return Err(JobError::Depot("journal is not an update job".to_string()));
    }
    if journal.install_path.trim().is_empty() {
        return Err(JobError::Depot(
            "update journal has no install path".to_string(),
        ));
    }
    let source = DepotSource::for_game(&journal.game_id);
    let catalog = source.load_catalog()?;
    journal.to_version = resolve_target_version(&catalog, Some(journal.to_version.clone()))?;
    journal.status = JobStatus::Planned;
    journal.phase = "Resuming".to_string();
    for step in &mut journal.steps {
        if matches!(
            step.status,
            StepStatus::Running | StepStatus::Paused | StepStatus::Failed
        ) {
            step.status = StepStatus::Waiting;
        }
    }
    append_log(
        &mut journal,
        "info",
        "Resuming the existing owned update session",
    );
    control.reset();
    spawn_update_journal(app, control, journal)
}

fn spawn_update_journal(
    app: AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
) -> Result<JobJournal, JobError> {
    persist_and_emit(&app, &journal)?;
    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        let result =
            run_real_update_job(&app_for_thread, control_for_thread.clone(), initial.clone());
        if matches!(&result, Err(JobError::Canceled)) {
            let install_root = PathBuf::from(&initial.install_path);
            let source = DepotSource::for_game(&initial.game_id);
            let downloading_root = downloading_dir_for_install(&install_root, &source);
            let _ =
                SequentialUpdateSession::cleanup_owned_session(&downloading_root, &canceled_job_id);
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            control_for_thread.set_running(false);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(err) => {
                let mut failed = read_latest_journal(&app_for_thread)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| initial.clone());
                failed.status = JobStatus::Failed;
                failed.phase = "Failed".to_string();
                mark_running_step_failed(&mut failed);
                append_log(&mut failed, "error", &err.to_string());
                let _ = persist_and_emit(&app_for_thread, &failed);
            }
        }
        control_for_thread.set_running(false);
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
    ensure_no_pending_file_transaction(&app)?;
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, target_version)?;
    let install_root = resolve_install_root(install_path, &source);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&downloading_root)?;

    // Try to add Windows Defender exclusion to avoid I/O errors
    if let Some(parent) = install_root.parent() {
        if let Err(e) = crate::defender_exclusion::add_defender_exclusion(parent) {
            eprintln!("[DEFENDER] Could not add exclusion: {}", e);
            // Don't fail the job, just log it
        }
    }

    let manifest = source.load_manifest(&catalog, &target_version)?;
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    fs::create_dir_all(&staged_chunks_root)?;
    let initial_missing =
        plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &manifest.files, None)?;
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
    journal.pipeline_version = if transport_pipeline_v3_enabled() {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "verified-stage-v2".to_string()
    };
    journal.bytes_done = initial_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
    journal.logical_bytes_done = journal.bytes_done;
    persist_and_emit(&app, &journal)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "planned",
        install_root.display().to_string(),
    )?;

    spawn_install_journal(app, control, journal)
}

pub fn resume_install_job(
    app: AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    if journal.kind != "install" || journal.install_path.trim().is_empty() {
        return Err(JobError::Depot(
            "journal is not a resumable install".to_string(),
        ));
    }
    journal.status = JobStatus::Planned;
    journal.phase = "Resuming".to_string();
    journal.commit_state = "idle".to_string();
    for step in &mut journal.steps {
        if matches!(
            step.status,
            StepStatus::Running | StepStatus::Paused | StepStatus::Failed
        ) {
            step.status = StepStatus::Waiting;
        }
    }
    append_log(
        &mut journal,
        "info",
        "Resuming the existing owned install session",
    );
    control.reset();
    spawn_install_journal(app, control, journal)
}

fn spawn_install_journal(
    app: AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
) -> Result<JobJournal, JobError> {
    persist_and_emit(&app, &journal)?;

    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        let result =
            run_real_install_job(&app_for_thread, control_for_thread.clone(), initial.clone());
        let canceled = matches!(&result, Err(JobError::Canceled));
        control_for_thread.set_running(false);
        if canceled {
            // Cleanup temporary download files on cancel
            let install_root = PathBuf::from(&initial.install_path);
            let source = DepotSource::for_game(&initial.game_id);
            let downloading_root = downloading_dir_for_install(&install_root, &source);
            let _ =
                VerifiedStageSession::cleanup_owned_session(&downloading_root, &canceled_job_id);
            if let Some(dl_root) = downloading_root.parent() {
                cleanup_download_temp_files(dl_root);
            }
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(JobError::Canceled) => {
                // Also cleanup on Canceled error
                let install_root = PathBuf::from(&initial.install_path);
                let source = DepotSource::for_game(&initial.game_id);
                let downloading_root = downloading_dir_for_install(&install_root, &source);
                let _ = VerifiedStageSession::cleanup_owned_session(
                    &downloading_root,
                    &canceled_job_id,
                );
                if let Some(dl_root) = downloading_root.parent() {
                    cleanup_download_temp_files(dl_root);
                }
                if downloading_root.exists() {
                    let _ = fs::remove_dir(&downloading_root);
                }
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
    ensure_no_pending_file_transaction(&app)?;
    let source = DepotSource::for_game(game_id);
    let install_root = PathBuf::from(install_path);
    let marker = read_install_marker(&install_root)?;
    if let Some(marker) = marker.as_ref() {
        if !install_marker_matches_source(marker, &source) {
            return Err(JobError::Depot(format!(
                "install metadata belongs to '{}', not '{}'",
                marker.game_id, source.game_id
            )));
        }
    }
    let requested_version =
        target_version.filter(|value| !value.trim().is_empty() && value != "not installed");
    let version = match (marker.as_ref(), requested_version) {
        (Some(marker), None) => marker.version.clone(),
        (Some(marker), Some(requested))
            if usable_installed_version(&marker.version)
                == usable_installed_version(&requested) =>
        {
            marker.version.clone()
        }
        (_, Some(requested)) => {
            let catalog = source.load_catalog()?;
            resolve_target_version(&catalog, Some(requested))?
        }
        (None, None) => {
            let catalog = source.load_catalog()?;
            let latest = catalog
                .effective_latest_version()
                .unwrap_or("unknown")
                .to_string();
            resolve_target_version(&catalog, Some(latest))?
        }
    };
    let target_manifest =
        installed_manifest_for_version(&source, &install_root, marker.as_ref(), &version)?;
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
    let missing_chunks = plan_missing_chunks(
        &HashMap::new(),
        &staged_chunks_root,
        &repair_files,
        Some(&install_root),
    )?;
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
    journal.pipeline_version = if transport_pipeline_v3_enabled() {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "verified-stage-v2".to_string()
    };
    journal.planned_files = repair_files.iter().map(|file| file.path.clone()).collect();
    journal.bytes_done = existing_partial_task_progress(&staged_chunks_root, &missing_chunks)
        .values()
        .copied()
        .sum::<u64>()
        .min(journal.bytes_total);
    journal.logical_bytes_done = journal.bytes_done;
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

    spawn_repair_journal(app, control, journal, repair_files, target_manifest)
}

pub fn resume_repair_job(
    app: AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    if journal.kind != "repair"
        || journal.install_path.trim().is_empty()
        || journal.planned_files.is_empty()
    {
        return Err(JobError::Depot(
            "journal is not a resumable repair".to_string(),
        ));
    }
    let source = DepotSource::for_game(&journal.game_id);
    let install_root = PathBuf::from(&journal.install_path);
    let marker = read_install_marker(&install_root)?;
    let target_manifest = installed_manifest_for_version(
        &source,
        &install_root,
        marker.as_ref(),
        &journal.to_version,
    )?;
    let planned = journal
        .planned_files
        .iter()
        .map(|path| manifest_file_key(path))
        .collect::<HashSet<_>>();
    let repair_files = target_manifest
        .files
        .iter()
        .filter(|file| planned.contains(&manifest_file_key(&file.path)))
        .cloned()
        .collect::<Vec<_>>();
    if repair_files.len() != planned.len() {
        return Err(JobError::SessionMismatch(
            "repair manifest changed since the journal was created".to_string(),
        ));
    }
    journal.status = JobStatus::Planned;
    journal.phase = "Resuming".to_string();
    journal.commit_state = "idle".to_string();
    for step in &mut journal.steps {
        if matches!(
            step.status,
            StepStatus::Running | StepStatus::Paused | StepStatus::Failed
        ) {
            step.status = StepStatus::Waiting;
        }
    }
    append_log(
        &mut journal,
        "info",
        "Resuming the existing owned repair session",
    );
    control.reset();
    spawn_repair_journal(app, control, journal, repair_files, target_manifest)
}

fn spawn_repair_journal(
    app: AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
    repair_files: Vec<FileEntry>,
    target_manifest: VersionManifest,
) -> Result<JobJournal, JobError> {
    persist_and_emit(&app, &journal)?;
    let source = DepotSource::for_game(&journal.game_id);

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
            initial.clone(),
            repair_files,
            target_manifest,
        );
        let canceled = matches!(&result, Err(JobError::Canceled));
        control_for_thread.set_running(false);
        if canceled {
            // Cleanup temporary download files on cancel
            let install_root = PathBuf::from(&initial.install_path);
            let source = DepotSource::for_game(&initial.game_id);
            let downloading_root = downloading_dir_for_install(&install_root, &source);
            let _ =
                VerifiedStageSession::cleanup_owned_session(&downloading_root, &canceled_job_id);
            if let Some(dl_root) = downloading_root.parent() {
                cleanup_download_temp_files(dl_root);
            }
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(JobError::Canceled) => {
                // Also cleanup on Canceled error
                let install_root = PathBuf::from(&initial.install_path);
                let source = DepotSource::for_game(&initial.game_id);
                let downloading_root = downloading_dir_for_install(&install_root, &source);
                if let Some(dl_root) = downloading_root.parent() {
                    cleanup_download_temp_files(dl_root);
                }
                if downloading_root.exists() {
                    let _ = fs::remove_dir(&downloading_root);
                }
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

    for path in crate::install_discovery::discovered_candidate_paths(&source.game_id) {
        push_unique_path(&mut candidates, path);
    }

    push_unique_path(&mut candidates, source.default_common_game_dir());

    // Recover installs when the launcher is placed in the same directory as the game.
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            push_unique_path(&mut candidates, parent.to_path_buf());
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

fn clear_orphaned_terminal_journal(app: &AppHandle, game_id: &str) {
    let Ok(Some(journal)) = read_latest_journal(app) else {
        return;
    };
    if sanitize_game_id(&journal.game_id) == sanitize_game_id(game_id)
        && matches!(journal.status, JobStatus::Committed | JobStatus::Failed)
    {
        let _ = clear_current_journal_if_matches(app, &journal.id);
    }
}

fn reconcile_registered_install(
    app: &AppHandle,
    source: &DepotSource,
) -> Result<Option<(PathBuf, InstallMarker)>, JobError> {
    let record = crate::platform::install_record(app, &source.game_id).map_err(JobError::Depot)?;
    let discovery = crate::install_discovery::game_discovery_view(&source.game_id);

    // A removable/offline library is not an uninstall, and an ambiguous
    // install must never be selected by an automatic job. Discovery owns the
    // decision until the drive returns or the user resolves the conflict.
    if record.is_some() && matches!(discovery.status.as_str(), "conflict" | "unavailable") {
        return Ok(None);
    }

    if let Some((install_root, marker)) = locate_registered_install(app, source) {
        let launch_executable = marker
            .launch_executable
            .clone()
            .unwrap_or_else(|| default_launch_executable(&source.game_id));
        let version =
            usable_installed_version(&marker.version).unwrap_or_else(|| "installed".to_string());
        let install_path = install_root.display().to_string();
        let should_register = record.as_ref().is_none_or(|stored| {
            stored.install_path != install_path
                || stored.version != version
                || stored.launch_executable != launch_executable
        });
        if should_register {
            crate::platform::register_install(
                app,
                &source.game_id,
                &install_root,
                &version,
                &launch_executable,
            )
            .map_err(JobError::Depot)?;
        }
        return Ok(Some((install_root, marker)));
    }

    // Startup callers can request a state before bounded library discovery has
    // classified offline drives and conflicts. Never turn that temporary gap
    // into an uninstall; discovery will make the authoritative decision.
    if record.is_some() && !crate::install_discovery::has_completed_discovery() {
        return Ok(None);
    }

    if record.is_some() {
        crate::platform::unregister_install(app, &source.game_id).map_err(JobError::Depot)?;
        clear_orphaned_terminal_journal(app, &source.game_id);
    }
    Ok(None)
}

fn reconciled_installs(app: &AppHandle) -> Result<Vec<ReconciledInstall>, String> {
    let mut records = crate::platform::install_records(app)?;
    records.sort_by(|left, right| left.game_id.cmp(&right.game_id));

    let mut installs = Vec::with_capacity(records.len());
    for record in records {
        let source = DepotSource::for_game(&record.game_id);
        let Some((install_path, marker)) =
            reconcile_registered_install(app, &source).map_err(|error| error.to_string())?
        else {
            continue;
        };
        installs.push(ReconciledInstall {
            game_id: source.game_id,
            install_path,
            marker,
        });
    }
    Ok(installs)
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
        && journal.kind != "patch"
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
    let discovery = crate::install_discovery::game_discovery_view(&source.game_id);
    if matches!(discovery.status.as_str(), "conflict" | "unavailable") {
        let record = crate::platform::install_record(app, &source.game_id)
            .ok()
            .flatten();
        let install_path = record
            .as_ref()
            .map(|record| record.install_path.clone())
            .or_else(|| discovery.candidate_paths.first().cloned())
            .unwrap_or_else(|| source.default_common_game_dir().display().to_string());
        let unavailable = discovery.status == "unavailable";
        return Ok(GameInstallState {
            game_id: source.game_id.clone(),
            installed: unavailable,
            current_version: if unavailable {
                record
                    .as_ref()
                    .map(|record| record.version.clone())
                    .filter(|version| !version.trim().is_empty())
                    .unwrap_or_else(|| "unavailable".to_string())
            } else {
                "install conflict".to_string()
            },
            install_path,
            launch_executable: record
                .as_ref()
                .map(|record| record.launch_executable.clone())
                .filter(|executable| !executable.trim().is_empty())
                .unwrap_or_else(|| default_launch_executable(&source.game_id)),
            applied_patch_id: None,
            discovery_status: discovery.status,
            candidate_paths: discovery.candidate_paths,
            library_id: discovery.library_id,
            unavailable_reason: discovery.unavailable_reason,
        });
    }
    let resolved = reconcile_registered_install(app, &source)?;

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
            applied_patch_id: marker.applied_patch_id.clone(),
            discovery_status: if discovery.status.is_empty() {
                "registered".to_string()
            } else {
                discovery.status
            },
            candidate_paths: if discovery.candidate_paths.is_empty() {
                vec![install_root.display().to_string()]
            } else {
                discovery.candidate_paths
            },
            library_id: discovery.library_id,
            unavailable_reason: None,
        });
    }

    if discovery.status.is_empty() && !crate::install_discovery::has_completed_discovery() {
        if let Some(record) = crate::platform::install_record(app, &source.game_id)
            .ok()
            .flatten()
        {
            return Ok(GameInstallState {
                game_id: source.game_id.clone(),
                installed: true,
                current_version: record.version,
                install_path: record.install_path,
                launch_executable: record.launch_executable,
                applied_patch_id: None,
                discovery_status: "recovering".to_string(),
                candidate_paths: Vec::new(),
                library_id: None,
                unavailable_reason: None,
            });
        }
    }

    let install_root = source.default_common_game_dir();
    Ok(GameInstallState {
        game_id: source.game_id.clone(),
        installed: false,
        current_version: "not installed".to_string(),
        install_path: install_root.display().to_string(),
        launch_executable: default_launch_executable(&source.game_id),
        applied_patch_id: None,
        discovery_status: "notFound".to_string(),
        candidate_paths: Vec::new(),
        library_id: None,
        unavailable_reason: None,
    })
}

/// Resolves the launcher-managed install directory for a registered library.
/// Callers must first validate the library identity through install discovery.
pub(crate) fn install_path_for_library(game_id: &str, library_root: &Path) -> PathBuf {
    library_root.join("common").join(game_dir_name(game_id))
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

pub fn get_game_install_size(game_id: &str) -> Result<u64, JobError> {
    let source = DepotSource::for_game(game_id);
    let catalog = source.load_catalog()?;
    if catalog.versions.is_empty() {
        return Err(JobError::Depot("No versions available".to_string()));
    }

    let latest_version = &catalog.versions[0].version;
    let manifest = source.load_manifest(&catalog, latest_version)?;

    // Calculate total install size (uncompressed files)
    let install_size: u64 = manifest.files.iter().map(|f| f.size).sum();

    // Calculate total compressed chunks size (network transfer + temporary storage)
    // Chunks are stored until commit completes, so we need space for both
    use std::collections::HashSet;
    let mut unique_chunks = HashSet::new();
    let mut chunks_compressed_size: u64 = 0;

    for file in &manifest.files {
        for chunk in &file.chunks {
            if unique_chunks.insert(&chunk.hash) {
                chunks_compressed_size += chunk.compressed_size;
            }
        }
    }

    // Worst-case space = install files + compressed chunks + buffer
    // During download: chunks accumulate, staging files grow
    // After commit: chunks deleted, only install files remain
    Ok(install_size + chunks_compressed_size)
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
    let marker = read_install_marker(install_path)?
        .filter(|marker| install_marker_matches_source(marker, &source));
    let marker_executable = marker.as_ref().and_then(|m| m.launch_executable.clone());
    let fallback_executable = launch_executable
        .filter(|value| !value.trim().is_empty())
        .or(marker_executable)
        .unwrap_or_else(|| default_launch_executable(&source.game_id));

    // 1. Try 0xo-launch.json / .0xolemon/launch.json in the install dir
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

    // 2. Check the local depot's build-info.json for launchOptions.
    //    This handles games that were installed before the multi-launch feature
    //    was added, without requiring a full reinstall. If launchOptions are
    //    found, we materialise 0xo-launch.json so Play, shortcuts, and future
    //    calls all work correctly.
    if let Some(installed_version) = marker.as_ref().map(|m| m.version.as_str()) {
        let build_info_path = format!("versions/{}/build-info.json", installed_version);
        if let Ok(build_info) = source.load_json::<serde_json::Value>(&build_info_path) {
            if let Some(opts) = build_info.get("launchOptions").and_then(|v| v.as_array()) {
                let parsed: Vec<crate::manifest::LaunchOption> = opts
                    .iter()
                    .filter_map(|o| serde_json::from_value(o.clone()).ok())
                    .collect();
                if parsed.len() >= 2 {
                    use crate::launch::{GameLaunchConfig, GameLaunchOption, GameLaunchProcess};
                    let options: Vec<GameLaunchOption> = parsed
                        .iter()
                        .enumerate()
                        .map(|(idx, opt)| {
                            let id = opt
                                .name
                                .to_ascii_lowercase()
                                .chars()
                                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                                .collect::<String>();
                            let args: Vec<String> = if opt.arguments.trim().is_empty() {
                                vec![]
                            } else {
                                opt.arguments.split_whitespace().map(String::from).collect()
                            };
                            GameLaunchOption {
                                id: id.clone(),
                                title: opt.name.clone(),
                                description: String::new(),
                                recommended: idx == 0,
                                processes: vec![GameLaunchProcess {
                                    path: opt.executable.clone(),
                                    args,
                                    working_directory: String::new(),
                                    environment: std::collections::HashMap::new(),
                                    run_as_admin: false,
                                    hidden: None,
                                    wait_for_exit: false,
                                    delay_before_ms: 0,
                                    delay_after_ms: 0,
                                    optional: false,
                                    role: "main".to_string(),
                                }],
                            }
                        })
                        .collect();
                    let first_id = options.first().map(|o| o.id.clone()).unwrap_or_default();
                    let launch_config = GameLaunchConfig {
                        schema_version: 1,
                        game_id: source.game_id.clone(),
                        picker_mode: "auto".to_string(),
                        default_option_id: first_id,
                        options,
                    };
                    // Persist 0xo-launch.json so future calls and shortcuts work.
                    let launch_json_path = install_path.join("0xo-launch.json");
                    if let Ok(json) = serde_json::to_string_pretty(&launch_config) {
                        let _ = fs::write(&launch_json_path, json);
                    }
                    let config = normalize_launch_config(
                        launch_config,
                        &source.game_id,
                        &fallback_executable,
                    )
                    .map_err(JobError::Depot)?;
                    return Ok((config, "depot build-info".to_string(), fallback_executable));
                }
            }
        }
    }

    // 3. Try the embedded launch config from the asset pack.
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
        let has_multiple_options = effective_launch_config(
            app,
            &source.game_id,
            &install_root,
            Some(relative_executable.clone()),
        )
        .map(|(config, _, _)| config.options.len() >= 2)
        .unwrap_or(false);
        let shortcut = if has_multiple_options {
            create_game_shortcut_no_exe(app, &source, &install_root, &executable)?
        } else {
            create_game_shortcut(
                app,
                &source,
                &install_root,
                &executable,
                &relative_executable,
            )?
        };
        if shortcut.is_some() {
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
    if let Some(active) = read_latest_journal(app)? {
        let same_install = same_filesystem_path(Path::new(&active.install_path), install_path);
        let same_game = sanitize_game_id(&active.game_id) == source.game_id;
        if same_game && same_install {
            let mutating = matches!(
                active.kind.as_str(),
                "install" | "update" | "repair" | "patch"
            ) && matches!(
                active.status,
                JobStatus::Planned
                    | JobStatus::Running
                    | JobStatus::Paused
                    | JobStatus::Downloading
                    | JobStatus::Assembling
                    | JobStatus::Verified
            );
            let pending_install_transaction = SequentialUpdateSession::has_pending_transaction(
                &downloading_dir_for_install(install_path, &source),
                &active.id,
            )?;
            if mutating || pending_install_transaction {
                return Err(JobError::Depot(
                    "The game cannot be launched while its install transaction is active or recovering"
                        .to_string(),
                ));
            }
        }
    }
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

    // Launch is verify-only. Runtime files are installed or repaired only at an
    // explicit transaction boundary while the game is closed.
    crate::managed_game_runtime::verify_before_launch(&source.game_id, install_path, &executable)
        .map_err(JobError::Depot)?;

    if !skip_cloud_sync {
        crate::cloud_save::sync_before_launch(app, &source.game_id).map_err(JobError::Depot)?;
    }

    let dependencies_installed = ensure_game_dependencies(app, &source, install_path)?;
    let shortcut_path =
        match create_game_shortcut(app, &source, install_path, &executable, &main.path) {
            Ok(path) => path.map(|path| path.display().to_string()),
            Err(error) => {
                eprintln!(
                    "[shortcut] Could not refresh the desktop shortcut for {}: {}",
                    source.game_id, error
                );
                None
            }
        };
    let _ = crate::steam_integration::ensure_game_shortcut(
        app,
        &source.game_id,
        &source.game_dir_name,
        install_path,
        &main.path,
        Some(&executable),
    );

    let achievement_watcher =
        if crate::managed_game_runtime::supports_managed_achievements(&source.game_id) {
            Some(
                crate::achievement_watcher::start_session(
                    app.clone(),
                    &source.game_id,
                    source.app_id,
                    install_path,
                    0,
                )
                .map_err(JobError::Depot)?,
            )
        } else {
            None
        };
    let runtime_environment = achievement_watcher
        .as_ref()
        .map(crate::achievement_watcher::AchievementWatcher::environment)
        .unwrap_or_default();
    let runtime_session_id = achievement_watcher
        .as_ref()
        .map(|watcher| watcher.session_id().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let runtime_app_id = achievement_watcher
        .as_ref()
        .map(crate::achievement_watcher::AchievementWatcher::app_id)
        .or(source.app_id)
        .unwrap_or_default();
    crate::game_session_state::begin_session(
        app,
        &runtime_session_id,
        &source.game_id,
        runtime_app_id,
        crate::game_session_state::OverlayRenderer::Disabled,
    )
    .map_err(JobError::Depot)?;
    if achievement_watcher.is_none() {
        let _ = crate::game_session_state::mark_transport(
            app,
            &source.game_id,
            &runtime_session_id,
            crate::game_session_state::AchievementTransport::Closed,
            None,
        );
    }
    let mut launched = match launch_option_processes(
        &source.game_id,
        install_path,
        option,
        &runtime_environment,
    ) {
        Ok(launched) => launched,
        Err(error) => {
            let _ = crate::game_session_state::close_session(
                app,
                &source.game_id,
                &runtime_session_id,
                true,
            );
            return Err(error);
        }
    };
    if let Some(mut tracked_process) = launched.main_process.take() {
        let pid = tracked_process.id();
        if let Some(watcher) = achievement_watcher.as_ref() {
            if let Err(error) = watcher.bind_pid(pid) {
                tracked_process.terminate();
                let _ = crate::game_session_state::close_session(
                    app,
                    &source.game_id,
                    &runtime_session_id,
                    true,
                );
                return Err(JobError::Depot(error));
            }
        }
        if let Err(error) = crate::game_session_state::mark_running(
            app,
            &source.game_id,
            &runtime_session_id,
            pid,
        ) {
            tracked_process.terminate();
            let _ = crate::game_session_state::close_session(
                app,
                &source.game_id,
                &runtime_session_id,
                true,
            );
            return Err(JobError::Depot(error));
        }
        let (started_event, achievement_events) = match crate::platform::begin_game_session(
            app,
            &source.game_id,
            pid,
            install_path,
            &executable,
        ) {
            Ok(value) => value,
            Err(error) => {
                tracked_process.terminate();
                let _ = crate::game_session_state::close_session(
                    app,
                    &source.game_id,
                    &runtime_session_id,
                    true,
                );
                return Err(JobError::Depot(format!(
                    "game started but runtime tracking could not be initialized: {error}"
                )));
            }
        };
        if let Err(error) = crate::cloud_save::consume_local_wins_once_on_start(
            app,
            &source.game_id,
            &runtime_session_id,
        ) {
            tracked_process.terminate();
            let _ = crate::platform::end_game_session(app, &source.game_id, pid, 0, None);
            let _ = crate::game_session_state::close_session(
                app,
                &source.game_id,
                &runtime_session_id,
                true,
            );
            return Err(JobError::Depot(format!(
                "game started but restored-save cloud policy could not bind to the runtime session: {error}"
            )));
        }

        crate::cloud_save::mark_game_running(&source.game_id, true);
        running_games()
            .lock()
            .unwrap()
            .insert(source.game_id.clone(), pid);
        let _ = app.emit("launcher://game-started", started_event);
        crate::platform::emit_achievement_events(app, &achievement_events);

        let app_for_exit = app.clone();
        let game_id_for_exit = source.game_id.clone();
        let installed_version_for_exit = marker.version.clone();
        let app_id_for_exit = source.app_id;
        let achievement_watcher_for_exit = achievement_watcher;
        let runtime_session_id_for_exit = runtime_session_id;
        let session_started_at = Instant::now();
        thread::spawn(move || {
            let exit_code = match tracked_process.wait() {
                Ok(exit_code) => exit_code,
                Err(error) => {
                    eprintln!(
                        "[runtime] Could not wait for {} process {}: {}",
                        game_id_for_exit, pid, error
                    );
                    None
                }
            };
            let session_seconds = session_started_at.elapsed().as_secs();
            running_games().lock().unwrap().remove(&game_id_for_exit);
            let _ = crate::game_session_state::mark_exiting(
                &app_for_exit,
                &game_id_for_exit,
                &runtime_session_id_for_exit,
            );

            match crate::platform::end_game_session(
                &app_for_exit,
                &game_id_for_exit,
                pid,
                session_seconds,
                exit_code,
            ) {
                Ok((exited_event, achievement_events)) => {
                    let _ = app_for_exit.emit("launcher://game-exited", exited_event);
                    crate::platform::emit_achievement_events(&app_for_exit, &achievement_events);
                }
                Err(error) => {
                    eprintln!(
                        "[runtime] Could not finalize the session for {}: {}",
                        game_id_for_exit, error
                    );
                    let _ = app_for_exit.emit(
                        "launcher://game-exited",
                        serde_json::json!({
                            "gameId": &game_id_for_exit,
                            "pid": pid,
                            "exitCode": exit_code,
                            "sessionSeconds": session_seconds
                        }),
                    );
                }
            }

            // Stop and join the pipe worker before save/cloud finalization so
            // no late achievement event can escape the session boundary.
            drop(achievement_watcher_for_exit);
            let _ = crate::game_session_state::close_session(
                &app_for_exit,
                &game_id_for_exit,
                &runtime_session_id_for_exit,
                false,
            );
            crate::cloud_save::mark_game_running(&game_id_for_exit, false);
            let backup_result = crate::local_save_backup::backup_after_exit(
                &app_for_exit,
                &game_id_for_exit,
                &installed_version_for_exit,
                app_id_for_exit,
            );
            if let Err(error) = backup_result {
                eprintln!(
                    "[save-backup] Could not protect {} before cloud sync: {error}",
                    game_id_for_exit
                );
                let _ = app_for_exit.emit(
                    "launcher://cloud-save-error",
                    serde_json::json!({
                        "gameId": &game_id_for_exit,
                        "message": format!("Local save backup failed; cloud upload was blocked: {error}")
                    }),
                );
                return;
            }

            match crate::cloud_save::finalize_local_wins_once_after_exit(
                &app_for_exit,
                &game_id_for_exit,
                &runtime_session_id_for_exit,
                exit_code == Some(0),
            ) {
                Ok(crate::cloud_save::LocalWinsExitDisposition::ForcePush) => {
                    crate::cloud_save::sync_restored_after_exit_async(
                        app_for_exit,
                        game_id_for_exit,
                    );
                }
                Ok(crate::cloud_save::LocalWinsExitDisposition::Normal) => {
                    crate::cloud_save::sync_after_exit_async(app_for_exit, game_id_for_exit);
                }
                Ok(
                    crate::cloud_save::LocalWinsExitDisposition::Conflict
                    | crate::cloud_save::LocalWinsExitDisposition::StillPrepared,
                ) => {
                    let _ = app_for_exit.emit(
                        "launcher://cloud-save-error",
                        serde_json::json!({
                            "gameId": &game_id_for_exit,
                            "message": "Restored save did not reach a verified clean exit; automatic cloud upload is blocked."
                        }),
                    );
                }
                Err(error) => {
                    let _ = app_for_exit.emit(
                        "launcher://cloud-save-error",
                        serde_json::json!({
                            "gameId": &game_id_for_exit,
                            "message": format!("Could not finalize restored-save cloud policy: {error}")
                        }),
                    );
                }
            }
        });
    } else {
        let _ = crate::game_session_state::close_session(
            app,
            &source.game_id,
            &runtime_session_id,
            true,
        );
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

#[tauri::command]
pub fn set_process_priority(game_id: String, high: bool) -> Result<(), String> {
    let pid = {
        let running = running_games().lock().unwrap();
        running.get(&game_id).copied()
    };
    if let Some(pid) = pid {
        #[cfg(target_os = "windows")]
        {
            // 128 = HIGH_PRIORITY_CLASS, 32 = NORMAL_PRIORITY_CLASS
            let priority_class = if high { 128 } else { 32 };
            let mut cmd = hidden_command("wmic");
            cmd.args([
                "process",
                "where",
                &format!("ProcessId={pid}"),
                "CALL",
                "setpriority",
                &priority_class.to_string(),
            ]);
            let status = cmd
                .status()
                .map_err(|e| format!("Failed to wmic setpriority: {}", e))?;
            if !status.success() {
                eprintln!("wmic setpriority failed with status: {}", status);
            }
        }
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
    let manifest =
        installed_manifest_for_version(&source, install_path, marker.as_ref(), &version)?;
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

    // Remove the launcher-side manifest backup for this install path.
    // This backup is only a safety copy of .0xolemon state (not save data)
    // and should be cleaned up when the game is uninstalled.
    if let Some(backup_root) = crate::job::paths::get_launcher_backup_root() {
        let backup_dir = crate::job::paths::state_backup_dir(&backup_root, install_path);
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }
    }

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
    for recovery_cycle in 0..=2_u32 {
        match run_real_update_job_once(app, Arc::clone(&control), journal.clone()) {
            Ok(final_journal) => return Ok(final_journal),
            Err(error @ (JobError::StageMissing(_) | JobError::SessionMissing(_)))
                if recovery_cycle < 2 =>
            {
                if control.is_canceled() {
                    return Err(JobError::Canceled);
                }
                append_log(
                    &mut journal,
                    "warning",
                    &format!(
                        "Staging recovery cycle {} of 2: {error}",
                        recovery_cycle + 1
                    ),
                );
                persist_and_emit(app, &journal)?;
                thread::sleep(Duration::from_millis(250_u64 << recovery_cycle));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

fn run_real_update_job_once(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    append_log(&mut journal, "info", "Sequential update job started");

    set_step_running(
        app,
        &mut journal,
        0,
        JobStatus::Running,
        "Read install state",
    )?;
    let catalog = source.load_catalog()?;
    let target_version = resolve_target_version(&catalog, Some(journal.to_version.clone()))?;
    journal.to_version = target_version.clone();

    // A resumed job must keep its original base manifest even if the marker was
    // committed just before the process exited. The full session ownership data
    // will decide whether to finish cleanup or roll back.
    let journal_base = usable_installed_version(&journal.from_version)
        .filter(|version| catalog_has_version(&catalog, version));
    let installed_base = if let Some(version) = journal_base {
        InstalledUpdateBase {
            manifest: source.load_manifest(&catalog, &version)?,
            version,
            source_label: "resumable update journal".to_string(),
        }
    } else {
        load_installed_update_base(&install_root, &source, &catalog)?.ok_or_else(|| {
            JobError::Depot(
                "Cannot determine the installed version: .0xolemon/state.0xo and manifest.0xo are missing or invalid"
                    .to_string(),
            )
        })?
    };
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
    let target_manifest = source.load_manifest(&catalog, &target_version)?;
    let planned_changed = changed_target_files(&from_manifest, &target_manifest);
    let mut session = SequentialUpdateSession::prepare(
        &downloading_root,
        &journal,
        &from_version,
        &target_version,
        &install_root,
        &planned_changed,
    )?;

    if session.recover(&install_root, &target_version)? == RecoveryOutcome::AlreadyCommitted {
        append_log(
            &mut journal,
            "info",
            "Recovered a committed update and completed pending transaction cleanup",
        );
        for step in journal.steps.iter_mut().take(5) {
            step.status = StepStatus::Completed;
            step.progress = 1.0;
        }
        journal.status = JobStatus::Committed;
        journal.phase = "Committed".to_string();
        journal.overall_progress = 1.0;
        journal.bytes_done = journal.bytes_total;
        if let Err(error) = session.cleanup_session_files() {
            append_log(
                &mut journal,
                "warning",
                &format!("Committed session cleanup remains pending: {error}"),
            );
        }
        persist_and_emit(app, &journal)?;
        let to_version = journal.to_version.clone();
        try_apply_patch_fix(
            app,
            &mut journal,
            &source,
            &install_root,
            &to_version,
            &control,
            false,
        )?;
        return Ok(journal);
    }

    let changed =
        filter_already_assembled(app, &mut journal, &install_root, planned_changed, &control)?;
    let (mut local_sources, reused_chunks, rejected_chunks) =
        build_verified_local_chunk_sources(&install_root, &from_manifest, &changed, &control)?;
    append_log(
        &mut journal,
        "info",
        &format!(
            "Validated {reused_chunks} reusable local chunks; {rejected_chunks} chunks were rejected"
        ),
    );
    let (discovered_sources, discovered_count) =
        discover_local_chunks(&install_root, &changed, &control)?;
    local_sources.extend(discovered_sources);
    if discovered_count > 0 {
        append_log(
            &mut journal,
            "info",
            &format!("Discovered {discovered_count} reusable chunks at shifted offsets"),
        );
    }

    // Opening every stage once repairs an interrupted checkpoint before network
    // planning, including truncating bytes written after the last sync_data.
    for file in &changed {
        drop(session.open_writer(file)?);
    }
    session.reconcile_chunk_references(&changed)?;
    let remaining_files = changed
        .iter()
        .map(|file| {
            let mut remaining = file.clone();
            remaining.chunks = file.chunks[session.durable_chunks(file)..].to_vec();
            remaining
        })
        .collect::<Vec<_>>();
    let missing_chunks =
        plan_missing_chunks(&local_sources, session.cache_root(), &remaining_files, None)?;
    let prepared_transports = if journal_uses_transport_v3(&journal) {
        let operation = update_transport_operation(&catalog, &from_version, &target_version);
        let prepared = source.prepare_pack_transports(
            session.cache_root(),
            &missing_chunks,
            Some(&target_version),
            operation,
            &[],
        )?;
        debug_assert!(prepared.xet_packs.is_empty());
        Some(prepared)
    } else {
        None
    };
    let transfer_total = prepared_transports
        .as_ref()
        .map(|prepared| prepared.transfer_bytes)
        .unwrap_or_else(|| download_transfer_bytes(&missing_chunks));
    configure_transfer_plan(&mut journal, transfer_total, 0);
    configure_download_metrics(&mut journal, &missing_chunks, false);
    journal.metrics.pipeline = if journal_uses_transport_v3(&journal) {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "sequential-stage-v1".to_string()
    };
    if journal.pipeline_version.is_empty() {
        // Journals created before verified patch staging stay pinned to the
        // legacy patch implementation when they resume.
        journal.pipeline_version = "sequential-stage-v1".to_string();
    }
    let assembly_total = changed
        .iter()
        .map(|file| {
            file.size.saturating_sub(
                file.chunks
                    .iter()
                    .take(session.durable_chunks(file))
                    .map(|chunk| chunk.uncompressed_size)
                    .sum::<u64>(),
            )
        })
        .sum::<u64>();
    journal.apply_bytes_total = assembly_total;
    journal.apply_bytes_done = session.total_durable_bytes(&changed);
    journal.durable_bytes = journal.apply_bytes_done;
    journal.commit_state = "staging".to_string();
    if let Some(prepared) = prepared_transports {
        journal.transport_plans = prepared.plans;
        journal.metrics.overfetch_bytes = journal
            .transport_plans
            .iter()
            .map(|plan| plan.estimated_overfetch)
            .sum();
    }
    let planned_network = human_bytes(journal.bytes_total);
    append_log(
        &mut journal,
        "info",
        &format!(
            "{} changed files; {} reusable bytes; {} network bytes planned",
            changed.len(),
            human_bytes(
                changed
                    .iter()
                    .map(|file| file.size)
                    .sum::<u64>()
                    .saturating_sub(assembly_total)
            ),
            planned_network
        ),
    );
    complete_step(app, &mut journal, 1)?;

    journal.steps[2].name = "Stream update".to_string();
    journal.steps[2].detail =
        "Prefetch verified chunks across files and append them to short staging files".to_string();
    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download and stage chunks",
    )?;
    let queue_budget = crate::platform::current_settings()
        .download_queue_mb
        .max(8)
        .saturating_mul(1024 * 1024);
    let mut downloaded = 0_u64;
    let mut assembled = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();
    let mut last_ui_emit = Instant::now();
    let mut last_journal_persist = Instant::now();
    let mut pending_commit_files = Vec::<FileEntry>::new();
    let mut pending_commit_bytes = 0_u64;
    let mut prefetched_through = 0_usize;
    let mut committed_files = 0_usize;

    for (file_index, file) in changed.iter().enumerate() {
        wait_for_control(app, &control, &mut journal, 2)?;
        journal.current_file = file.path.clone();
        if file_index == 0 || file_index.saturating_add(1) == changed.len() || file_index % 64 == 0
        {
            append_log(
                &mut journal,
                "info",
                &format!(
                    "Streaming file {}/{}: {}",
                    file_index.saturating_add(1),
                    changed.len(),
                    file.path
                ),
            );
        }
        let mut writer = session.open_writer(file)?;
        let mut checkpointed_hashes = Vec::<String>::new();

        while writer.next_chunk() < file.chunks.len() {
            wait_for_control(app, &control, &mut journal, 2)?;
            let batch_start = writer.next_chunk();
            let batch_end = sequential_batch_end(
                file,
                batch_start,
                &local_sources,
                session.cache_root(),
                queue_budget,
            )?;
            let mut batch_file = file.clone();
            batch_file.chunks = file.chunks[batch_start..batch_end].to_vec();
            let current_file_complete = batch_end == file.chunks.len();
            let should_refill_prefetch =
                current_file_complete && file_index.saturating_add(1) >= prefetched_through;
            let prefetch_files = if should_refill_prefetch {
                let (files, through) = sequential_prefetch_files(
                    &changed,
                    file_index,
                    batch_file,
                    &local_sources,
                    session.cache_root(),
                    &session,
                    queue_budget,
                )?;
                prefetched_through = prefetched_through.max(through);
                files
            } else {
                vec![batch_file]
            };
            let batch_missing =
                plan_missing_chunks(&local_sources, session.cache_root(), &prefetch_files, None)?;

            if !batch_missing.is_empty() {
                let assembled_snapshot = assembled;
                let mut progress_callback = |progress: DownloadProgress| {
                    if progress.clear_in_flight {
                        in_flight.remove(&progress.task_id);
                    } else {
                        in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
                    }
                    downloaded = downloaded.saturating_add(progress.committed_bytes);
                    wait_for_control(app, &control, &mut journal, 2)?;
                    let active_bytes = in_flight.values().copied().sum::<u64>();
                    observe_download_progress(&mut journal, &progress, active_bytes);
                    journal.bytes_done = downloaded
                        .saturating_add(active_bytes)
                        .min(journal.bytes_total);
                    journal.steps[2].progress =
                        streamed_journal_progress(&journal, assembled_snapshot, assembly_total);
                    journal.overall_progress = overall_progress(2, journal.steps[2].progress);
                    journal.steps[2].retry_count =
                        journal.steps[2].retry_count.max(progress.retry_count);
                    touch(&mut journal);
                    publish_job_progress(
                        app,
                        &journal,
                        &mut last_ui_emit,
                        &mut last_journal_persist,
                        false,
                    )
                };
                source.download_chunks_to_store_parallel(
                    session.cache_root(),
                    &batch_missing,
                    Some(&target_version),
                    Arc::clone(&control),
                    &mut progress_callback,
                )?;
            }

            while writer.next_chunk() < batch_end {
                wait_for_control(app, &control, &mut journal, 2)?;
                let chunk = &file.chunks[writer.next_chunk()];
                let data = read_chunk_bytes(chunk, &local_sources, session.cache_root())?;
                writer.append(chunk, &data)?;
                assembled = assembled.saturating_add(data.len() as u64);
                checkpointed_hashes.push(chunk.hash.clone());

                if writer.checkpoint_due() {
                    session.checkpoint_writer(&mut writer, false)?;
                    session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
                    journal.apply_bytes_done = session.total_durable_bytes(&changed);
                    journal.durable_bytes = journal.apply_bytes_done;
                }
                if last_ui_emit.elapsed() >= JOB_UI_EMIT_INTERVAL {
                    journal.steps[2].progress =
                        streamed_journal_progress(&journal, assembled, assembly_total);
                    journal.overall_progress = overall_progress(2, journal.steps[2].progress);
                    touch(&mut journal);
                    publish_job_progress(
                        app,
                        &journal,
                        &mut last_ui_emit,
                        &mut last_journal_persist,
                        false,
                    )?;
                }
            }

            if writer.next_chunk() < file.chunks.len() {
                session.checkpoint_writer(&mut writer, false)?;
                session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
            }
        }

        session.finish_writer(&mut writer, file)?;
        session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
        journal.apply_bytes_done = session.total_durable_bytes(&changed);
        journal.durable_bytes = journal.apply_bytes_done;
        merge_verified_writer_metrics(&mut journal, &writer);
        drop(writer);
        let stage = session.stage_path(file);
        for chunk in &file.chunks {
            local_sources.insert(
                chunk.hash.clone(),
                LocalChunkSource {
                    path: stage.clone(),
                    offset: chunk.file_offset,
                    size: chunk.uncompressed_size,
                },
            );
        }
        pending_commit_bytes = pending_commit_bytes.saturating_add(file.size);
        pending_commit_files.push(file.clone());

        let commit_due = pending_commit_files.len() >= SEQUENTIAL_COMMIT_BATCH_FILES
            || pending_commit_bytes >= queue_budget
            || file_index + 1 == changed.len();
        if commit_due {
            append_log(
                &mut journal,
                "info",
                &format!(
                    "Durably committing {} staged files",
                    pending_commit_files.len()
                ),
            );
            journal.commit_state = "committing".to_string();
            let commit_started = Instant::now();
            committed_files = committed_files.saturating_add(commit_sequential_batch(
                &mut session,
                &install_root,
                &mut pending_commit_files,
                &mut local_sources,
            )?);
            journal.metrics.commit_wait_ms = journal.metrics.commit_wait_ms.saturating_add(
                commit_started
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            );
            journal.commit_state = "staging".to_string();
            pending_commit_bytes = 0;
        }
        journal.steps[2].progress = streamed_journal_progress(&journal, assembled, assembly_total);
        journal.steps[3].progress = progress_fraction(committed_files, changed.len());
        journal.overall_progress = overall_progress(2, journal.steps[2].progress);
        touch(&mut journal);
        publish_job_progress(
            app,
            &journal,
            &mut last_ui_emit,
            &mut last_journal_persist,
            file_index + 1 == changed.len(),
        )?;
    }
    journal.current_file.clear();
    complete_step(app, &mut journal, 2)?;

    journal.steps[3].name = "Commit transaction".to_string();
    journal.steps[3].detail =
        "Retain rollback backups until install metadata is committed".to_string();
    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Commit update transaction",
    )?;
    let obsolete_paths = obsolete_update_paths(
        &install_root,
        &source,
        &from_manifest,
        &target_manifest,
        &target_version,
    )?;
    session.prepare_obsolete_batch(&install_root, &obsolete_paths)?;
    for (index, relative_path) in obsolete_paths.iter().enumerate() {
        wait_for_control(app, &control, &mut journal, 3)?;
        session.backup_obsolete_file(&install_root, relative_path)?;
        journal.steps[3].progress = progress_fraction(index + 1, obsolete_paths.len());
    }
    journal.steps[3].progress = 1.0;
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize")?;
    journal.commit_state = "metadata".to_string();
    write_install_marker(
        app,
        &install_root,
        &target_manifest,
        &source,
        &target_version,
    )?;
    session.mark_marker_committed()?;
    finish_committed_transaction_cleanup(app, &mut journal, &mut session, &install_root, "Update")?;
    journal.status = JobStatus::Committed;
    journal.phase = "Committed".to_string();
    journal.commit_state = "committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    complete_step(app, &mut journal, 4)?;
    append_log(&mut journal, "info", "Sequential update committed");
    persist_and_emit(app, &journal)?;

    let to_version = journal.to_version.clone();
    try_apply_patch_fix(
        app,
        &mut journal,
        &source,
        &install_root,
        &to_version,
        &control,
        false,
    )?;
    Ok(journal)
}

fn sequential_batch_end(
    file: &FileEntry,
    start: usize,
    local_sources: &HashMap<String, LocalChunkSource>,
    cache_root: &Path,
    queue_budget: u64,
) -> Result<usize, JobError> {
    let mut end = start;
    let mut network_bytes = 0_u64;
    while end < file.chunks.len() {
        let chunk = &file.chunks[end];
        let cached =
            compressed_chunk_file_valid(&staged_chunk_path_from(cache_root, &chunk.hash), chunk)?;
        let cost = if local_sources.contains_key(&chunk.hash) || cached {
            0
        } else {
            chunk.compressed_size
        };
        if end > start && cost > 0 && network_bytes.saturating_add(cost) > queue_budget {
            break;
        }
        network_bytes = network_bytes.saturating_add(cost);
        end += 1;
        if network_bytes >= queue_budget {
            break;
        }
    }
    Ok(end.max(start.saturating_add(1)).min(file.chunks.len()))
}

fn sequential_prefetch_files(
    changed: &[FileEntry],
    file_index: usize,
    current_batch: FileEntry,
    local_sources: &HashMap<String, LocalChunkSource>,
    cache_root: &Path,
    session: &SequentialUpdateSession,
    queue_budget: u64,
) -> Result<(Vec<FileEntry>, usize), JobError> {
    let mut seen = HashSet::<String>::new();
    let mut network_bytes = 0_u64;
    for chunk in &current_batch.chunks {
        if !seen.insert(chunk.hash.clone()) || local_sources.contains_key(&chunk.hash) {
            continue;
        }
        let staged = staged_chunk_path_from(cache_root, &chunk.hash);
        if !compressed_chunk_file_valid(&staged, chunk)? {
            network_bytes = network_bytes.saturating_add(chunk.compressed_size);
        }
    }

    let mut window = vec![current_batch];
    let mut prefetched_through = file_index.saturating_add(1);
    if network_bytes >= queue_budget {
        return Ok((window, prefetched_through));
    }

    for (future_index, future) in changed
        .iter()
        .enumerate()
        .skip(file_index.saturating_add(1))
        .take(SEQUENTIAL_PREFETCH_MAX_FILES.saturating_sub(1))
    {
        let durable = session.durable_chunks(future);
        if durable >= future.chunks.len() {
            prefetched_through = future_index.saturating_add(1);
            continue;
        }

        let mut candidate_hashes = HashSet::<String>::new();
        let mut candidate_bytes = 0_u64;
        for chunk in future.chunks.iter().skip(durable) {
            if seen.contains(&chunk.hash)
                || !candidate_hashes.insert(chunk.hash.clone())
                || local_sources.contains_key(&chunk.hash)
            {
                continue;
            }
            let staged = staged_chunk_path_from(cache_root, &chunk.hash);
            if !compressed_chunk_file_valid(&staged, chunk)? {
                candidate_bytes = candidate_bytes.saturating_add(chunk.compressed_size);
            }
        }

        if candidate_bytes > 0 && network_bytes.saturating_add(candidate_bytes) > queue_budget {
            break;
        }

        network_bytes = network_bytes.saturating_add(candidate_bytes);
        seen.extend(candidate_hashes);
        let mut remaining = future.clone();
        remaining.chunks = future.chunks[durable..].to_vec();
        window.push(remaining);
        prefetched_through = future_index.saturating_add(1);
        if network_bytes >= queue_budget {
            break;
        }
    }

    Ok((window, prefetched_through))
}

fn commit_sequential_batch(
    session: &mut SequentialUpdateSession,
    install_root: &Path,
    files: &mut Vec<FileEntry>,
    local_sources: &mut HashMap<String, LocalChunkSource>,
) -> Result<usize, JobError> {
    if files.is_empty() {
        return Ok(0);
    }

    session.sync_staged_files(files)?;
    session.prepare_commit_batch(install_root, files)?;
    let committed = files.len();
    let mut path_remaps = HashMap::<String, PathBuf>::new();

    for file in files.drain(..) {
        let stage = session.stage_path(&file);
        let target = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
        if let Some((old_target, backup)) = session.commit_file(install_root, &file)? {
            path_remaps.insert(filesystem_path_key(&old_target), backup);
        }
        path_remaps.insert(filesystem_path_key(&stage), target.clone());
        for chunk in &file.chunks {
            local_sources.insert(
                chunk.hash.clone(),
                LocalChunkSource {
                    path: target.clone(),
                    offset: chunk.file_offset,
                    size: chunk.uncompressed_size,
                },
            );
        }
    }

    // Remap every source once per batch instead of scanning the full chunk map
    // once (or twice) for every file. This keeps 7k-file updates near O(chunks).
    for source in local_sources.values_mut() {
        if let Some(replacement) = path_remaps.get(&filesystem_path_key(&source.path)) {
            source.path = replacement.clone();
        }
    }

    Ok(committed)
}

fn merge_verified_writer_metrics(journal: &mut JobJournal, writer: &VerifiedFileWriter) {
    let metrics = writer.metrics();
    journal.metrics.disk_read_bytes = journal
        .metrics
        .disk_read_bytes
        .saturating_add(metrics.disk_read_bytes);
    journal.metrics.disk_write_bytes = journal
        .metrics
        .disk_write_bytes
        .saturating_add(metrics.disk_write_bytes);
    journal.metrics.resume_rehash_bytes = journal
        .metrics
        .resume_rehash_bytes
        .saturating_add(metrics.resume_rehash_bytes);
    journal.metrics.sync_wait_ms = journal
        .metrics
        .sync_wait_ms
        .saturating_add(metrics.sync_wait_ms);
    journal.metrics.allocation_reserved_bytes = journal
        .metrics
        .allocation_reserved_bytes
        .saturating_add(metrics.allocation_reserved_bytes);
    if journal.metrics.allocation_fallback_reason.is_none() {
        journal.metrics.allocation_fallback_reason = metrics.allocation_fallback_reason.clone();
    }
}

#[allow(clippy::too_many_arguments)]
fn stage_verified_manifest_files(
    app: &AppHandle,
    control: &Arc<JobControl>,
    journal: &mut JobJournal,
    source: &DepotSource,
    session: &mut VerifiedStageSession,
    files: &[FileEntry],
    local_sources: &mut HashMap<String, LocalChunkSource>,
    target_version: &str,
    operation: TransportOperation,
    step_index: usize,
) -> Result<(), JobError> {
    session.reconcile_chunk_references(files)?;
    let remaining_files = files
        .iter()
        .map(|file| {
            let mut remaining = file.clone();
            remaining.chunks = file.chunks[session.durable_chunks(file)..].to_vec();
            remaining
        })
        .collect::<Vec<_>>();
    let missing_chunks =
        plan_missing_chunks(local_sources, session.cache_root(), &remaining_files, None)?;
    let prepared_transports = if journal_uses_transport_v3(journal) {
        Some(source.prepare_pack_transports(
            session.cache_root(),
            &missing_chunks,
            Some(target_version),
            operation,
            &journal.transport_plans,
        )?)
    } else {
        None
    };
    let transfer_total = prepared_transports
        .as_ref()
        .map(|prepared| prepared.transfer_bytes)
        .unwrap_or_else(|| download_transfer_bytes(&missing_chunks));
    let resumed_in_flight = existing_partial_task_progress(session.cache_root(), &missing_chunks);
    let resumed_bytes = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(transfer_total);
    configure_transfer_plan(journal, transfer_total, resumed_bytes);
    configure_download_metrics(journal, &missing_chunks, false);
    journal.metrics.pipeline = if journal_uses_transport_v3(journal) {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "verified-stage-v2".to_string()
    };
    if journal.pipeline_version.is_empty() {
        journal.pipeline_version = "verified-stage-v2".to_string();
    }
    journal.apply_bytes_total = files.iter().map(|file| file.size).sum();
    journal.apply_bytes_done = session.total_durable_bytes(files);
    journal.durable_bytes = journal.apply_bytes_done;
    journal.commit_state = "staging".to_string();
    if let Some(prepared) = prepared_transports.as_ref() {
        journal.transport_plans = prepared.plans.clone();
        journal.metrics.overfetch_bytes = prepared
            .plans
            .iter()
            .map(|plan| plan.estimated_overfetch)
            .sum();
    }

    let queue_budget = crate::platform::current_settings()
        .download_queue_mb
        .max(8)
        .saturating_mul(1024 * 1024);
    let mut downloaded = 0_u64;
    let mut in_flight = resumed_in_flight;
    let mut last_ui_emit = Instant::now();
    let mut last_journal_persist = Instant::now();

    if let Some(prepared) = prepared_transports {
        persist_and_emit(app, journal)?;
        let mut fell_back_to_raw = false;
        for pack in prepared.xet_packs {
            wait_for_control(app, control, journal, step_index)?;
            let durable_snapshot = journal.apply_bytes_done;
            let mut progress_callback = |progress: DownloadProgress| {
                if progress.clear_in_flight {
                    in_flight.remove(&progress.task_id);
                } else {
                    in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
                }
                downloaded = downloaded.saturating_add(progress.committed_bytes);
                wait_for_control(app, control, journal, step_index)?;
                let active_bytes = in_flight.values().copied().sum::<u64>();
                observe_download_progress(journal, &progress, active_bytes);
                journal.bytes_done = downloaded
                    .saturating_add(active_bytes)
                    .min(journal.bytes_total);
                journal.steps[step_index].progress =
                    streamed_journal_progress(journal, durable_snapshot, journal.apply_bytes_total);
                journal.overall_progress =
                    overall_progress(step_index, journal.steps[step_index].progress);
                touch(journal);
                publish_job_progress(
                    app,
                    journal,
                    &mut last_ui_emit,
                    &mut last_journal_persist,
                    false,
                )
            };
            match source.download_xet_pack_to_store(
                session.cache_root(),
                &pack,
                control,
                &mut progress_callback,
            ) {
                Ok(()) => {}
                Err(JobError::Transient(error)) => {
                    fell_back_to_raw = true;
                    in_flight.remove(&format!("xet:{}", pack.plan.pack_id));
                    if let Some(plan) = journal
                        .transport_plans
                        .iter_mut()
                        .find(|plan| plan.pack_id == pack.plan.pack_id)
                    {
                        plan.selected_transport = DownloadTransportKind::HttpRange;
                    }
                    append_log(
                        journal,
                        "warning",
                        &format!(
                            "Xet unavailable for pack {}; falling back to exact verified ranges: {}",
                            pack.plan.pack_id, error
                        ),
                    );
                    persist_and_emit(app, journal)?;
                }
                Err(error) => return Err(error),
            }
        }

        if fell_back_to_raw {
            let remaining =
                plan_missing_chunks(local_sources, session.cache_root(), &remaining_files, None)?;
            let raw_remaining = download_transfer_bytes(&remaining);
            configure_transfer_plan(
                journal,
                downloaded.saturating_add(raw_remaining),
                downloaded,
            );
            journal.metrics.overfetch_bytes = journal
                .transport_plans
                .iter()
                .map(|plan| plan.estimated_overfetch)
                .sum();
        }
    }

    for (file_index, file) in files.iter().enumerate() {
        wait_for_control(app, control, journal, step_index)?;
        journal.current_file = file.path.clone();
        if let Some(step) = journal.steps.get_mut(step_index) {
            step.detail = format!(
                "Writing verified file {}/{}: {}",
                file_index.saturating_add(1),
                files.len(),
                file.path
            );
        }
        let mut writer = session.open_writer(file)?;
        let mut checkpointed_hashes = Vec::<String>::new();

        while writer.next_chunk() < file.chunks.len() {
            wait_for_control(app, control, journal, step_index)?;
            let batch_start = writer.next_chunk();
            let batch_end = sequential_batch_end(
                file,
                batch_start,
                local_sources,
                session.cache_root(),
                queue_budget,
            )?;
            let mut batch = file.clone();
            batch.chunks = file.chunks[batch_start..batch_end].to_vec();
            let batch_missing =
                plan_missing_chunks(local_sources, session.cache_root(), &[batch], None)?;

            if !batch_missing.is_empty() {
                let durable_snapshot = journal.apply_bytes_done;
                let mut progress_callback = |progress: DownloadProgress| {
                    if progress.clear_in_flight {
                        in_flight.remove(&progress.task_id);
                    } else {
                        in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
                    }
                    downloaded = downloaded.saturating_add(progress.committed_bytes);
                    wait_for_control(app, control, journal, step_index)?;
                    let active_bytes = in_flight.values().copied().sum::<u64>();
                    observe_download_progress(journal, &progress, active_bytes);
                    journal.bytes_done = downloaded
                        .saturating_add(active_bytes)
                        .min(journal.bytes_total);
                    journal.steps[step_index].progress = streamed_journal_progress(
                        journal,
                        durable_snapshot,
                        journal.apply_bytes_total,
                    );
                    journal.overall_progress =
                        overall_progress(step_index, journal.steps[step_index].progress);
                    journal.steps[step_index].retry_count = journal.steps[step_index]
                        .retry_count
                        .max(progress.retry_count);
                    touch(journal);
                    publish_job_progress(
                        app,
                        journal,
                        &mut last_ui_emit,
                        &mut last_journal_persist,
                        false,
                    )
                };
                source.download_chunks_to_store_parallel(
                    session.cache_root(),
                    &batch_missing,
                    Some(target_version),
                    Arc::clone(control),
                    &mut progress_callback,
                )?;
            }

            while writer.next_chunk() < batch_end {
                wait_for_control(app, control, journal, step_index)?;
                let chunk = &file.chunks[writer.next_chunk()];
                let data = read_chunk_bytes(chunk, local_sources, session.cache_root())?;
                journal.metrics.disk_read_bytes = journal
                    .metrics
                    .disk_read_bytes
                    .saturating_add(data.len() as u64);
                writer.append(chunk, &data)?;
                checkpointed_hashes.push(chunk.hash.clone());

                if writer.checkpoint_due() {
                    session.checkpoint_writer(&mut writer, false)?;
                    session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
                    journal.apply_bytes_done = session.total_durable_bytes(files);
                    journal.durable_bytes = journal.apply_bytes_done;
                }
                if last_ui_emit.elapsed() >= JOB_UI_EMIT_INTERVAL {
                    journal.steps[step_index].progress = streamed_journal_progress(
                        journal,
                        journal.apply_bytes_done,
                        journal.apply_bytes_total,
                    );
                    journal.overall_progress =
                        overall_progress(step_index, journal.steps[step_index].progress);
                    touch(journal);
                    publish_job_progress(
                        app,
                        journal,
                        &mut last_ui_emit,
                        &mut last_journal_persist,
                        false,
                    )?;
                }
            }
        }

        session.finish_writer(&mut writer, file)?;
        session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
        journal.apply_bytes_done = session.total_durable_bytes(files);
        journal.durable_bytes = journal.apply_bytes_done;
        merge_verified_writer_metrics(journal, &writer);
        drop(writer);

        let stage = session.stage_path(file);
        for chunk in &file.chunks {
            local_sources.insert(
                chunk.hash.clone(),
                LocalChunkSource {
                    path: stage.clone(),
                    offset: chunk.file_offset,
                    size: chunk.uncompressed_size,
                },
            );
        }
        journal.steps[step_index].progress =
            streamed_journal_progress(journal, journal.apply_bytes_done, journal.apply_bytes_total);
        journal.overall_progress = overall_progress(step_index, journal.steps[step_index].progress);
        touch(journal);
        publish_job_progress(
            app,
            journal,
            &mut last_ui_emit,
            &mut last_journal_persist,
            file_index.saturating_add(1) == files.len(),
        )?;
    }

    journal.current_file.clear();
    Ok(())
}

fn commit_verified_manifest_files(
    app: &AppHandle,
    journal: &mut JobJournal,
    session: &mut VerifiedStageSession,
    install_root: &Path,
    files: &[FileEntry],
    step_index: usize,
) -> Result<(), JobError> {
    journal.commit_state = "preparing".to_string();
    journal.phase = "Preparing safe transaction".to_string();
    persist_and_emit(app, journal)?;
    session.sync_staged_files(files)?;
    session.prepare_commit_batch(install_root, files)?;

    journal.commit_state = "committing".to_string();
    journal.phase = "Completing safe transaction".to_string();
    if let Some(step) = journal.steps.get_mut(step_index) {
        step.detail = "Installing verified files; pause and cancel are deferred".to_string();
    }
    persist_and_emit(app, journal)?;
    let commit_started = Instant::now();
    let mut last_emit = Instant::now();
    for (index, file) in files.iter().enumerate() {
        journal.current_file = file.path.clone();
        session.commit_file(install_root, file)?;
        if let Some(step) = journal.steps.get_mut(step_index) {
            step.progress = progress_fraction(index.saturating_add(1), files.len());
        }
        if last_emit.elapsed() >= JOB_UI_EMIT_INTERVAL || index.saturating_add(1) == files.len() {
            journal.overall_progress =
                overall_progress(step_index, journal.steps[step_index].progress);
            touch(journal);
            persist_and_emit(app, journal)?;
            last_emit = Instant::now();
        }
    }
    journal.metrics.commit_wait_ms = journal.metrics.commit_wait_ms.saturating_add(
        commit_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    );
    journal.current_file.clear();
    journal.commit_state = "filesInstalled".to_string();
    Ok(())
}

fn finish_committed_transaction_cleanup(
    app: &AppHandle,
    journal: &mut JobJournal,
    session: &mut VerifiedStageSession,
    install_root: &Path,
    label: &str,
) -> Result<(), JobError> {
    journal.commit_state = "cleanup".to_string();
    persist_and_emit(app, journal)?;
    if let Err(error) = session.cleanup_committed(install_root) {
        append_log(
            journal,
            "warning",
            &format!(
                "{label} files are committed, but transaction cleanup must resume before another file operation: {error}"
            ),
        );
        let _ = persist_and_emit(app, journal);
        return Err(error);
    }
    if let Err(error) = session.cleanup_session_files() {
        append_log(
            journal,
            "warning",
            &format!("{label} committed; inert session cleanup remains pending: {error}"),
        );
    }
    Ok(())
}

fn streamed_update_progress(
    network_done: u64,
    network_total: u64,
    assembled_done: u64,
    assembly_total: u64,
) -> f32 {
    let total = network_total.saturating_add(assembly_total);
    if total == 0 {
        return 1.0;
    }
    network_done.saturating_add(assembled_done).min(total) as f32 / total as f32
}

fn streamed_journal_progress(
    journal: &JobJournal,
    assembled_done: u64,
    assembly_total: u64,
) -> f32 {
    let logical_total = journal.logical_bytes_total.max(
        journal
            .session_base_bytes
            .saturating_add(journal.bytes_total),
    );
    let logical_done = current_logical_transfer_done(journal).min(logical_total);
    streamed_update_progress(logical_done, logical_total, assembled_done, assembly_total)
}

fn filesystem_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn same_filesystem_path(left: &Path, right: &Path) -> bool {
    filesystem_path_key(left) == filesystem_path_key(right)
}

fn obsolete_update_paths(
    install_root: &Path,
    source: &DepotSource,
    from_manifest: &VersionManifest,
    target_manifest: &VersionManifest,
    target_version: &str,
) -> Result<Vec<String>, JobError> {
    let target_patch = load_patch_manifest(source, target_version)?;
    let mut retained = target_manifest
        .files
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    if let Some(patch) = target_patch {
        retained.extend(
            patch
                .files
                .iter()
                .map(|file| file.path.to_ascii_lowercase()),
        );
    }

    let mut obsolete = Vec::new();
    let mut seen = HashSet::new();
    for path in from_manifest.files.iter().map(|file| file.path.clone()) {
        let normalized = path.to_ascii_lowercase();
        if !retained.contains(&normalized) && seen.insert(normalized) {
            obsolete.push(path);
        }
    }
    if let Some(old_patch) = read_applied_patch_manifest(install_root)? {
        for path in old_patch.files.into_iter().map(|file| file.path) {
            let normalized = path.to_ascii_lowercase();
            if !retained.contains(&normalized) && seen.insert(normalized) {
                obsolete.push(path);
            }
        }
    }
    Ok(obsolete)
}

#[allow(dead_code)]
fn run_legacy_update_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    // Steam-like layout: staging files go directly into dl/{appid}/, NOT dl/{appid}/files/
    let staging_root = downloading_root.to_path_buf();
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
    let mut changed = changed_target_files(&from_manifest, &target_manifest);
    changed = filter_already_assembled(app, &mut journal, &install_root, changed, &control)?;
    let (mut local_sources, reused_chunks, rejected_chunks) =
        build_verified_local_chunk_sources(&install_root, &from_manifest, &changed, &control)?;
    append_log(
        &mut journal,
        "info",
        &format!(
            "Validated {reused_chunks} reusable local chunks; {rejected_chunks} chunks not strictly valid at original offset"
        ),
    );

    // Attempt to discover remaining chunks in changed files
    let (discovered_sources, discovered_count) =
        discover_local_chunks(&install_root, &changed, &control)?;
    local_sources.extend(discovered_sources);
    if discovered_count > 0 {
        append_log(
            &mut journal,
            "info",
            &format!("Discovered {discovered_count} matching chunks shifted in existing files"),
        );
    }

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
        plan_missing_chunks(
            &local_sources,
            &staged_chunks_root,
            &changed,
            Some(&install_root),
        )?
    };
    let transfer_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    let resumed_bytes = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(transfer_total);
    configure_transfer_plan(&mut journal, transfer_total, resumed_bytes);
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
            Some(&target_version),
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Some(&target_version),
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

    // ==========================================
    // CLEANUP OBSOLETE FILES (BASE & PATCH)
    // ==========================================
    append_log(
        &mut journal,
        "info",
        "Cleaning up obsolete files from previous version",
    );

    // 1. Cleanup obsolete base game files
    let mut deleted_obsolete = 0;
    let target_map: HashSet<String> = target_manifest
        .files
        .iter()
        .map(|f| f.path.to_ascii_lowercase())
        .collect();

    for old_file in &from_manifest.files {
        if !target_map.contains(&old_file.path.to_ascii_lowercase()) {
            if let Some(path) = safe_join(&install_root, &old_file.path) {
                let lp = long_path(&path);
                if lp.exists() {
                    if let Err(e) = fs::remove_file(&lp) {
                        append_log(
                            &mut journal,
                            "warning",
                            &format!(
                                "Failed to delete obsolete base file {}: {}",
                                old_file.path, e
                            ),
                        );
                    } else {
                        deleted_obsolete += 1;
                    }
                }
            }
        }
    }

    // 2. Cleanup obsolete patch files
    if let Ok(Some(old_patch_manifest)) = read_applied_patch_manifest(&install_root) {
        let target_patch_manifest = load_patch_manifest(&source, &target_version).unwrap_or(None);
        let target_patch_map: HashSet<String> = target_patch_manifest
            .as_ref()
            .map(|m| {
                m.files
                    .iter()
                    .map(|f| f.path.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        for old_patch_file in &old_patch_manifest.files {
            if !target_patch_map.contains(&old_patch_file.path.to_ascii_lowercase()) {
                if let Some(path) = safe_join(&install_root, &old_patch_file.path) {
                    let lp = long_path(&path);
                    if lp.exists() {
                        if let Err(e) = fs::remove_file(&lp) {
                            append_log(
                                &mut journal,
                                "warning",
                                &format!(
                                    "Failed to delete obsolete patch file {}: {}",
                                    old_patch_file.path, e
                                ),
                            );
                        } else {
                            deleted_obsolete += 1;
                        }
                    }
                }
            }
        }
    }

    if deleted_obsolete > 0 {
        append_log(
            &mut journal,
            "info",
            &format!("Successfully deleted {} obsolete file(s)", deleted_obsolete),
        );
    }

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
        let mut chunk_usage: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for file in &changed {
            for chunk in &file.chunks {
                *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
            }
        }
        // Throttle: emit at most once every 200ms to avoid flooding the WebView
        // with hundreds of IPC round-trips per second during large assemblies.
        let mut last_assemble_emit = Instant::now();
        let assemble_emit_interval = Duration::from_millis(200);
        let total_files = changed.len();
        for (index, file) in changed.iter().enumerate() {
            wait_for_control(app, &control, &mut journal, 3)?;
            append_log(&mut journal, "info", &format!("Assembling {}", file.path));
            assemble_target_file(&install_root, &staged_chunks_root, file, &local_sources)?;

            // Free up disk space immediately by deleting chunks no longer needed
            for chunk in &file.chunks {
                if let Some(count) = chunk_usage.get_mut(&chunk.hash) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
                        let _ = fs::remove_file(&chunk_path);
                    }
                }
            }

            journal.steps[3].progress = progress_fraction(index + 1, total_files);
            journal.overall_progress = overall_progress(3, journal.steps[3].progress);
            touch(&mut journal);
            // Always emit the very last file so UI reaches 100%.
            let is_last = index + 1 == total_files;
            if is_last || last_assemble_emit.elapsed() >= assemble_emit_interval {
                persist_and_emit(app, &journal)?;
                last_assemble_emit = Instant::now();
            }
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

    // Cleanup chunks used by this game
    append_log(&mut journal, "info", "Cleaning up cached chunks");
    let mut cleaned_chunks = 0;
    for file in &changed {
        for chunk in &file.chunks {
            let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
            if chunk_path.exists() {
                match fs::remove_file(&chunk_path) {
                    Ok(_) => cleaned_chunks += 1,
                    Err(e) => {
                        eprintln!("[CLEANUP] Failed to remove chunk {}: {}", chunk.hash, e);
                    }
                }
            }
        }
    }
    append_log(
        &mut journal,
        "info",
        &format!("Cleaned {} chunk files", cleaned_chunks),
    );

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

    // After step 4 finishes, we do step 5 (Patch fix)
    let to_version = journal.to_version.clone();
    try_apply_patch_fix(
        app,
        &mut journal,
        &source,
        &install_root,
        &to_version,
        &control,
        false,
    )?;

    Ok(journal)
}

fn run_real_install_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
) -> Result<JobJournal, JobError> {
    if journal_uses_verified_stage(&journal) {
        run_verified_install_job(app, control, journal)
    } else {
        run_legacy_install_job(app, control, journal)
    }
}

fn run_verified_install_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    append_log(&mut journal, "info", "Verified streaming install started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Prepare store")?;
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&downloading_root)?;
    journal.install_path = install_root.display().to_string();
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
    let mut session = VerifiedStageSession::prepare_with_commit_proof(
        &downloading_root,
        &journal,
        &journal.from_version,
        &target_version,
        &install_root,
        &target_manifest.files,
        TransactionCommitProof::InstalledVersion,
    )?;
    if session.recover(&install_root, &target_version)? == RecoveryOutcome::AlreadyCommitted {
        for step in journal.steps.iter_mut().take(5) {
            step.status = StepStatus::Completed;
            step.progress = 1.0;
        }
        journal.status = JobStatus::Committed;
        journal.phase = "Committed".to_string();
        journal.commit_state = "committed".to_string();
        journal.overall_progress = 1.0;
        if let Err(error) = session.cleanup_session_files() {
            append_log(
                &mut journal,
                "warning",
                &format!("Committed install cleanup remains pending: {error}"),
            );
        }
        persist_and_emit(app, &journal)?;
        let version = journal.to_version.clone();
        try_apply_patch_fix(
            app,
            &mut journal,
            &source,
            &install_root,
            &version,
            &control,
            false,
        )?;
        return Ok(journal);
    }

    let mut changed = filter_already_assembled(
        app,
        &mut journal,
        &install_root,
        target_manifest.files.clone(),
        &control,
    )?;
    let (mut local_sources, discovered_count) =
        discover_local_chunks(&install_root, &changed, &control)?;
    if discovered_count > 0 {
        append_log(
            &mut journal,
            "info",
            &format!("Discovered {discovered_count} reusable verified chunks"),
        );
    }
    changed.sort_by(|left, right| left.path.cmp(&right.path));
    append_log(
        &mut journal,
        "info",
        &format!("{} file(s) require verified staging", changed.len()),
    );
    complete_step(app, &mut journal, 1)?;

    journal.steps[2].name = "Download and apply".to_string();
    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download and write verified chunks",
    )?;
    stage_verified_manifest_files(
        app,
        &control,
        &mut journal,
        &source,
        &mut session,
        &changed,
        &mut local_sources,
        &target_version,
        TransportOperation::FreshInstall,
        2,
    )?;
    complete_step(app, &mut journal, 2)?;

    journal.steps[3].name = "Commit transaction".to_string();
    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Commit verified install files",
    )?;
    commit_verified_manifest_files(app, &mut journal, &mut session, &install_root, &changed, 3)?;
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize install")?;
    journal.commit_state = "metadata".to_string();
    write_install_marker(
        app,
        &install_root,
        &target_manifest,
        &source,
        &target_version,
    )?;
    session.mark_marker_committed()?;
    finish_committed_transaction_cleanup(
        app,
        &mut journal,
        &mut session,
        &install_root,
        "Install",
    )?;
    journal.status = JobStatus::Committed;
    journal.phase = "Committed".to_string();
    journal.commit_state = "committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    journal.apply_bytes_done = journal.apply_bytes_total;
    journal.durable_bytes = journal.apply_bytes_done;
    complete_step(app, &mut journal, 4)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "committed",
        install_root.display().to_string(),
    )?;
    append_log(&mut journal, "info", "Verified streaming install committed");
    persist_and_emit(app, &journal)?;

    let version = journal.to_version.clone();
    try_apply_patch_fix(
        app,
        &mut journal,
        &source,
        &install_root,
        &version,
        &control,
        false,
    )?;
    Ok(journal)
}

fn run_legacy_install_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    let staged_chunks_root = staged_chunk_dir(&downloading_root);
    append_log(&mut journal, "info", "Real install job started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Prepare store")?;
    fs::create_dir_all(&install_root)?;
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
        &format!("Chunks cached in {}", staged_chunks_root.display()),
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

    let mut changed = target_manifest.files.clone();
    changed = filter_already_assembled(app, &mut journal, &install_root, changed, &control)?;
    let mut local_sources = HashMap::new();

    // Attempt to discover chunks in existing files in the install directory (if any)
    let (discovered_sources, discovered_count) =
        discover_local_chunks(&install_root, &changed, &control)?;
    local_sources.extend(discovered_sources);
    if discovered_count > 0 {
        append_log(
            &mut journal,
            "info",
            &format!("Discovered {discovered_count} matching chunks in existing files for install recovery")
        );
    }

    // We assemble directly to install folder now, no staging needed
    let direct_stage = None;

    let missing_chunks = plan_missing_chunks(
        &local_sources,
        &staged_chunks_root,
        &changed,
        Some(&install_root),
    )?;
    let transfer_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    let resumed_bytes = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(transfer_total);
    configure_transfer_plan(&mut journal, transfer_total, resumed_bytes);
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
            Some(&target_version),
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Some(&target_version),
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
        let mut chunk_usage: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for file in &changed {
            for chunk in &file.chunks {
                *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
            }
        }
        // Throttle: emit at most once every 200ms to avoid flooding the WebView
        // with hundreds of IPC round-trips per second during large assemblies.
        let mut last_assemble_emit = Instant::now();
        let assemble_emit_interval = Duration::from_millis(200);
        let total_files = changed.len();
        for (index, file) in changed.iter().enumerate() {
            wait_for_control(app, &control, &mut journal, 3)?;
            append_log(&mut journal, "info", &format!("Assembling {}", file.path));
            assemble_target_file(&install_root, &staged_chunks_root, file, &local_sources)?;

            // Free up disk space immediately by deleting chunks no longer needed
            for chunk in &file.chunks {
                if let Some(count) = chunk_usage.get_mut(&chunk.hash) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
                        let _ = fs::remove_file(&chunk_path);
                    }
                }
            }

            journal.steps[3].progress = progress_fraction(index + 1, total_files);
            journal.overall_progress = overall_progress(3, journal.steps[3].progress);
            touch(&mut journal);
            // Always emit the very last file so UI reaches 100%.
            let is_last = index + 1 == total_files;
            if is_last || last_assemble_emit.elapsed() >= assemble_emit_interval {
                persist_and_emit(app, &journal)?;
                last_assemble_emit = Instant::now();
            }
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

    // Cleanup chunks used by this game
    append_log(&mut journal, "info", "Cleaning up cached chunks");
    let mut cleaned_chunks = 0;
    for file in &changed {
        for chunk in &file.chunks {
            let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
            if chunk_path.exists() {
                match fs::remove_file(&chunk_path) {
                    Ok(_) => cleaned_chunks += 1,
                    Err(e) => {
                        eprintln!("[CLEANUP] Failed to remove chunk {}: {}", chunk.hash, e);
                    }
                }
            }
        }
    }
    append_log(
        &mut journal,
        "info",
        &format!("Cleaned {} chunk files", cleaned_chunks),
    );

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

    // After step 4 finishes, we do step 5 (Patch fix)
    let to_version = journal.to_version.clone();
    try_apply_patch_fix(
        app,
        &mut journal,
        &source,
        &install_root,
        &to_version,
        &control,
        false,
    )?;

    Ok(journal)
}

fn run_real_repair_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
    repair_files: Vec<FileEntry>,
    target_manifest: VersionManifest,
) -> Result<JobJournal, JobError> {
    if journal_uses_verified_stage(&journal) {
        run_verified_repair_job(app, control, journal, repair_files, target_manifest)
    } else {
        run_legacy_repair_job(app, control, journal, repair_files, target_manifest)
    }
}

fn run_verified_repair_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
    repair_files: Vec<FileEntry>,
    target_manifest: VersionManifest,
) -> Result<JobJournal, JobError> {
    let install_root = PathBuf::from(&journal.install_path);
    let source = DepotSource::for_game(&journal.game_id);
    let downloading_root = downloading_dir_for_install(&install_root, &source);
    append_log(&mut journal, "info", "Verified streaming repair started");

    set_step_running(app, &mut journal, 0, JobStatus::Running, "Prepare repair")?;
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&downloading_root)?;
    complete_step(app, &mut journal, 0)?;

    set_step_running(app, &mut journal, 1, JobStatus::Running, "Plan repair")?;
    let target_version = journal.to_version.clone();
    let mut session = VerifiedStageSession::prepare_with_commit_proof(
        &downloading_root,
        &journal,
        &journal.from_version,
        &target_version,
        &install_root,
        &repair_files,
        TransactionCommitProof::TransactionPhase,
    )?;
    if session.recover(&install_root, &target_version)? == RecoveryOutcome::AlreadyCommitted {
        for step in journal.steps.iter_mut().take(5) {
            step.status = StepStatus::Completed;
            step.progress = 1.0;
        }
        journal.status = JobStatus::Committed;
        journal.phase = "Repair committed".to_string();
        journal.commit_state = "committed".to_string();
        journal.overall_progress = 1.0;
        if let Err(error) = session.cleanup_session_files() {
            append_log(
                &mut journal,
                "warning",
                &format!("Committed repair cleanup remains pending: {error}"),
            );
        }
        persist_and_emit(app, &journal)?;
        return Ok(journal);
    }
    append_log(
        &mut journal,
        "info",
        &format!(
            "{} corrupt or missing file(s) require repair",
            repair_files.len()
        ),
    );
    complete_step(app, &mut journal, 1)?;

    journal.steps[2].name = "Download and repair".to_string();
    set_step_running(
        app,
        &mut journal,
        2,
        JobStatus::Downloading,
        "Download and write repaired files",
    )?;
    let (mut local_sources, discovered_count) =
        discover_local_chunks(&install_root, &repair_files, &control)?;
    if discovered_count > 0 {
        append_log(
            &mut journal,
            "info",
            &format!(
                "Reusing {discovered_count} verified chunk(s) from the existing damaged files"
            ),
        );
    }
    stage_verified_manifest_files(
        app,
        &control,
        &mut journal,
        &source,
        &mut session,
        &repair_files,
        &mut local_sources,
        &target_version,
        TransportOperation::Repair,
        2,
    )?;
    complete_step(app, &mut journal, 2)?;

    journal.steps[3].name = "Commit repair".to_string();
    set_step_running(
        app,
        &mut journal,
        3,
        JobStatus::Assembling,
        "Commit repaired files",
    )?;
    commit_verified_manifest_files(
        app,
        &mut journal,
        &mut session,
        &install_root,
        &repair_files,
        3,
    )?;
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize repair")?;
    journal.commit_state = "metadata".to_string();
    let existing_marker = read_install_marker(&install_root)?;
    session.backup_metadata_file(
        &install_root,
        &format!("{INSTALL_MARKER_DIR}/{INSTALL_MARKER_FILE}"),
    )?;
    if existing_marker.as_ref().is_some_and(|marker| {
        install_marker_matches_source(marker, &source) && marker.version == journal.to_version
    }) {
        write_install_marker_file(&install_root, existing_marker.as_ref().unwrap())?;
    } else {
        write_install_marker(
            app,
            &install_root,
            &target_manifest,
            &source,
            &journal.to_version,
        )?;
    }
    session.mark_marker_committed()?;
    finish_committed_transaction_cleanup(app, &mut journal, &mut session, &install_root, "Repair")?;
    journal.status = JobStatus::Committed;
    journal.phase = "Repair committed".to_string();
    journal.commit_state = "committed".to_string();
    journal.overall_progress = 1.0;
    journal.bytes_done = journal.bytes_total;
    journal.apply_bytes_done = journal.apply_bytes_total;
    journal.durable_bytes = journal.apply_bytes_done;
    complete_step(app, &mut journal, 4)?;
    write_download_session_marker(
        &downloading_root,
        &journal,
        "committed",
        install_root.display().to_string(),
    )?;
    append_log(&mut journal, "info", "Verified streaming repair committed");
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

fn run_legacy_repair_job(
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
    // Steam-like layout: staging files go directly into dl/{appid}/, NOT dl/{appid}/files/
    let staging_root = downloading_root.to_path_buf();
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
        plan_missing_chunks(
            &local_sources,
            &staged_chunks_root,
            &repair_files,
            Some(&install_root),
        )?
    };
    let transfer_total = download_transfer_bytes(&missing_chunks);
    configure_download_metrics(&mut journal, &missing_chunks, direct_stage.is_some());
    let resumed_in_flight = existing_partial_task_progress(&staged_chunks_root, &missing_chunks);
    let resumed_bytes = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(transfer_total);
    configure_transfer_plan(&mut journal, transfer_total, resumed_bytes);
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
    let target_version = journal.to_version.clone();
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
            Some(&target_version),
            Arc::clone(&control),
            &mut progress_callback,
        )?;
    } else {
        source.download_chunks_to_store_parallel(
            &staged_chunks_root,
            &missing_chunks,
            Some(&target_version),
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
        let mut chunk_usage: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for file in &repair_files {
            for chunk in &file.chunks {
                *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
            }
        }
        // Throttle: emit at most once every 200ms to avoid flooding the WebView.
        let mut last_emit = Instant::now();
        let emit_interval = Duration::from_millis(200);
        let total_files = repair_files.len();
        for (index, file) in repair_files.iter().enumerate() {
            wait_for_control(app, &control, &mut journal, 3)?;
            append_log(&mut journal, "info", &format!("Repairing {}", file.path));
            assemble_target_file(&install_root, &staged_chunks_root, file, &local_sources)?;

            // Free up disk space immediately by deleting chunks no longer needed
            for chunk in &file.chunks {
                if let Some(count) = chunk_usage.get_mut(&chunk.hash) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
                        let _ = fs::remove_file(&chunk_path);
                    }
                }
            }

            journal.steps[3].progress = progress_fraction(index + 1, total_files);
            journal.overall_progress = overall_progress(3, journal.steps[3].progress);
            touch(&mut journal);
            let is_last = index + 1 == total_files;
            if is_last || last_emit.elapsed() >= emit_interval {
                persist_and_emit(app, &journal)?;
                last_emit = Instant::now();
            }
        }
    }
    complete_step(app, &mut journal, 3)?;

    set_step_running(app, &mut journal, 4, JobStatus::Running, "Finalize repair")?;
    let existing_marker = read_install_marker(&install_root)?;
    if existing_marker.as_ref().is_some_and(|marker| {
        install_marker_matches_source(marker, &source) && marker.version == journal.to_version
    }) {
        // Repair does not change the installed version. Preserve the patch id
        // and its cached manifest so a repaired hotfix remains verified later.
        write_install_marker_file(&install_root, existing_marker.as_ref().unwrap())?;
    } else {
        write_install_marker(
            app,
            &install_root,
            &target_manifest,
            &source,
            &journal.to_version,
        )?;
    }
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
struct ResolvedPackUrl {
    signed_url: Option<String>,
    refresh_at: Instant,
    source_identity: Option<String>,
    total_pack_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
struct PlannedXetPack {
    plan: PackTransportPlan,
    base: DepotRemoteBase,
    location: HfRepoLocation,
    metadata: HfPackMetadata,
    relative_path: String,
    chunks: Vec<ChunkRef>,
}

#[derive(Debug, Default)]
struct PreparedPackTransports {
    plans: Vec<PackTransportPlan>,
    xet_packs: Vec<PlannedXetPack>,
    transfer_bytes: u64,
}

#[derive(Debug, Clone)]
struct DepotSource {
    game_id: String,
    game_dir_name: String,
    /// Steam AppID for shorter download paths (e.g., "1250410" instead of long game name)
    /// Fetched from Firestore steam_appids collection
    app_id: Option<u32>,
    base_urls: Vec<DepotRemoteBase>,
    active_base_url: Arc<Mutex<Option<String>>>,
    local_root: Option<PathBuf>,
    client: OnceLock<Client>,
    rate_coordinator: Arc<RateCoordinator>,
    governor: Arc<Mutex<AdaptiveGovernor>>,
    resolved_pack_urls: Arc<Mutex<HashMap<String, ResolvedPackUrl>>>,
    // Negative cache for optional JSON objects (notably patch manifests).
    // A missing patch must not be fetched from every mirror again later in
    // the same install/update job.
    missing_json_paths: Arc<Mutex<HashSet<String>>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    applied_patch_id: Option<String>,
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

/// Load Steam AppID mapping from local JSON file
/// Returns HashMap of game_id -> app_id
fn load_steam_appid_mapping() -> HashMap<String, u32> {
    use std::fs;

    // Try bundled resource first (production), then local file (development)
    let paths = [
        PathBuf::from("steam_appids_mapping.json"),
        PathBuf::from("../steam_appids_mapping.json"),
        PathBuf::from("../../steam_appids_mapping.json"),
    ];

    for mapping_path in &paths {
        if let Ok(content) = fs::read_to_string(mapping_path) {
            if let Ok(mapping) = serde_json::from_str::<HashMap<String, u32>>(&content) {
                eprintln!(
                    "âœ“ Loaded {} AppID mappings from {:?}",
                    mapping.len(),
                    mapping_path
                );
                return mapping;
            }
        }
    }

    // Fallback: return empty map if file not found or invalid
    eprintln!(
        "â  Warning: steam_appids_mapping.json not found - download paths will use long game IDs"
    );
    HashMap::new()
}

fn hf_environment_token() -> Option<String> {
    env::var("HF_TOKEN")
        .ok()
        .and_then(|value| sanitize_token(Some(value)))
}

fn all_remote_json_candidates_missing(attempted: usize, missing: usize) -> bool {
    attempted > 0 && attempted == missing
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

        // Fetch Steam AppID from local mapping file for shorter download paths
        let mapping = load_steam_appid_mapping();
        let app_id = mapping.get(&game_id).copied();

        // Debug log
        if let Some(appid) = app_id {
            eprintln!("âœ“ Found AppID {} for game '{}'", appid, game_id);
        } else {
            eprintln!(
                "â  No AppID found for game '{}' (will use long game ID)",
                game_id
            );
        }

        Self {
            game_dir_name: game_dir_name(&game_id).to_string(),
            game_id,
            app_id,
            base_urls,
            active_base_url: Arc::new(Mutex::new(None)),
            local_root,
            client: OnceLock::new(),
            rate_coordinator: Arc::new(RateCoordinator::default()),
            governor: Arc::new(Mutex::new(AdaptiveGovernor::new(
                crate::platform::current_settings().download_workers,
                if matches!(
                    crate::platform::current_settings().download_profile,
                    crate::platform::DownloadProfile::Auto
                ) {
                    4
                } else {
                    crate::platform::current_settings().download_workers
                },
                crate::platform::current_settings()
                    .pack_range_mb
                    .saturating_mul(1024 * 1024),
            ))),
            resolved_pack_urls: Arc::new(Mutex::new(HashMap::new())),
            missing_json_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn default_common_game_dir(&self) -> PathBuf {
        default_store_root()
            .join("common")
            .join(&self.game_dir_name)
    }

    fn default_downloading_game_dir(&self) -> PathBuf {
        default_store_root().join("dl").join(&self.game_dir_name)
    }

    fn status_label(&self) -> String {
        let has_token = self.base_urls.iter().any(|base| base.token.is_some());
        match (&self.local_root, has_token) {
            (Some(_), true) | (None, true) => "Content service ready".to_string(),
            (Some(_), false) => "Offline metadata ready".to_string(),
            (None, false) => "Remote content service ready".to_string(),
        }
    }

    fn effective_worker_count(&self) -> usize {
        download_worker_count()
    }

    fn effective_pack_range_task_bytes(&self) -> u64 {
        self.governor
            .lock()
            .map(|governor| governor.range_bytes())
            .unwrap_or_else(|_| pack_range_task_bytes())
    }

    fn active_connection_limit(&self) -> usize {
        self.governor
            .lock()
            .map(|governor| governor.active_connections())
            .unwrap_or(1)
    }

    fn observe_transport_window(
        &self,
        elapsed: Duration,
        throughput: u64,
        error_rate: f32,
        rate_limited: bool,
        ttfb_ms: u64,
        backpressured: bool,
    ) {
        if let Ok(mut governor) = self.governor.lock() {
            governor.observe_window(
                elapsed,
                throughput,
                error_rate,
                rate_limited,
                ttfb_ms,
                backpressured,
            );
        }
    }

    fn pack_relative_path(pack_id: &str, target_version: Option<&str>) -> String {
        if pack_id.starts_with("patch-") {
            target_version
                .map(|version| format!("patches/{version}/packs/{pack_id}.bin"))
                .unwrap_or_else(|| format!("packs/{pack_id}.bin"))
        } else {
            format!("packs/{pack_id}.bin")
        }
    }

    fn pack_relative_path_in(
        pack_id: &str,
        target_version: Option<&str>,
        pack_path_prefix: Option<&str>,
    ) -> String {
        let prefix = pack_path_prefix.map(str::trim).unwrap_or_default();
        if prefix.is_empty() {
            Self::pack_relative_path(pack_id, target_version)
        } else {
            format!("{}/{}.bin", prefix.trim_matches('/'), pack_id)
        }
    }

    fn pin_pack_source_metadata(
        &self,
        base_url: &str,
        relative_path: &str,
        metadata: &HfPackMetadata,
    ) -> Result<(), JobError> {
        let key = format!("{base_url}\n{relative_path}");
        let mut cache = self
            .resolved_pack_urls
            .lock()
            .map_err(|_| JobError::Depot("pack source cache is unavailable".to_string()))?;
        if let Some(previous) = cache.get(&key) {
            if previous.source_identity.as_deref().is_some_and(|identity| {
                !source_identities_match(identity, &metadata.source_identity)
            }) {
                return Err(JobError::Depot(
                    "source revision changed while planning pack transport".to_string(),
                ));
            }
            if previous.total_pack_bytes != Some(metadata.file_size) {
                return Err(JobError::Depot(
                    "source pack size changed while planning transport".to_string(),
                ));
            }
        }
        cache.entry(key).or_insert(ResolvedPackUrl {
            signed_url: None,
            refresh_at: Instant::now(),
            source_identity: Some(metadata.source_identity.clone()),
            total_pack_bytes: Some(metadata.file_size),
        });
        Ok(())
    }

    fn prepare_pack_transports(
        &self,
        staged_chunks_root: &Path,
        chunks: &[ChunkRef],
        target_version: Option<&str>,
        operation: TransportOperation,
        pinned_plans: &[PackTransportPlan],
    ) -> Result<PreparedPackTransports, JobError> {
        let mut by_pack = HashMap::<String, Vec<ChunkRef>>::new();
        for chunk in chunks {
            by_pack
                .entry(chunk.pack_id.clone())
                .or_default()
                .push(chunk.clone());
        }
        let mut pack_ids = by_pack.keys().cloned().collect::<Vec<_>>();
        pack_ids.sort();

        let cache_dir = staged_chunks_root.join("_transport").join("xet-cache");
        let mut prepared = PreparedPackTransports::default();
        for pack_id in pack_ids {
            let mut pack_chunks = by_pack.remove(&pack_id).unwrap_or_default();
            pack_chunks.sort_by_key(|chunk| chunk.pack_offset);
            let required_bytes = pack_chunks
                .iter()
                .map(|chunk| chunk.compressed_size)
                .sum::<u64>();
            let known_pack_extent = pack_chunks
                .iter()
                .map(|chunk| chunk.pack_offset.saturating_add(chunk.compressed_size))
                .max()
                .unwrap_or(0);
            let raw_transfer_bytes =
                build_pack_download_tasks(&pack_chunks, self.effective_pack_range_task_bytes())
                    .iter()
                    .map(|task| task.range_end.saturating_sub(task.range_start))
                    .sum::<u64>();
            let pinned = pinned_plans
                .iter()
                .find(|plan| plan.pack_id == pack_id)
                .cloned();
            let pinned_xet = pinned
                .as_ref()
                .is_some_and(|plan| plan.selected_transport == DownloadTransportKind::XetPack);
            let dense_enough_to_probe = required_bytes > 0
                && known_pack_extent > 0
                && u128::from(required_bytes) * 100 >= u128::from(known_pack_extent) * 90;
            let should_probe = pinned_xet
                || (pinned.is_none()
                    && operation == TransportOperation::FreshInstall
                    && dense_enough_to_probe);
            let relative_path = Self::pack_relative_path(&pack_id, target_version);
            let mut selected_xet = None::<PlannedXetPack>;

            if should_probe {
                for base in self.ordered_base_urls() {
                    let Some(mut location) = HfRepoLocation::parse_resolve_base(&base.url) else {
                        continue;
                    };
                    let metadata = match probe_hf_pack(
                        &location,
                        base.token.as_deref(),
                        &cache_dir,
                        &relative_path,
                    ) {
                        Ok(metadata) => metadata,
                        Err(_) => continue,
                    };
                    if let Some(pinned) = pinned.as_ref() {
                        if !pinned.source_identity.is_empty()
                            && !source_identities_match(
                                &pinned.source_identity,
                                &metadata.source_identity,
                            )
                        {
                            return Err(JobError::Depot(format!(
                                "source revision changed for pack {pack_id}"
                            )));
                        }
                    }
                    let transport = if pinned_xet {
                        DownloadTransportKind::XetPack
                    } else {
                        select_transport(
                            operation,
                            TransportCandidate {
                                required_bytes,
                                total_pack_bytes: metadata.file_size,
                                xet_metadata_available: metadata.xet_hash.is_some(),
                                public_read_available: true,
                                xet_estimated_bytes_per_second: None,
                                raw_estimated_bytes_per_second: None,
                            },
                        )
                    };
                    if transport != DownloadTransportKind::XetPack {
                        break;
                    }
                    location.revision = metadata.resolved_revision.clone();
                    let plan = PackTransportPlan {
                        pack_id: pack_id.clone(),
                        source_identity: metadata.source_identity.clone(),
                        required_bytes,
                        total_pack_bytes: metadata.file_size,
                        selected_transport: DownloadTransportKind::XetPack,
                        estimated_overfetch: metadata.file_size.saturating_sub(required_bytes),
                    };
                    selected_xet = Some(PlannedXetPack {
                        plan,
                        base,
                        location,
                        metadata,
                        relative_path: relative_path.clone(),
                        chunks: pack_chunks.clone(),
                    });
                    break;
                }
            }

            if let Some(xet) = selected_xet {
                prepared.transfer_bytes = prepared
                    .transfer_bytes
                    .saturating_add(xet.plan.total_pack_bytes);
                prepared.plans.push(xet.plan.clone());
                prepared.xet_packs.push(xet);
            } else {
                let plan = PackTransportPlan {
                    pack_id,
                    source_identity: pinned
                        .as_ref()
                        .map(|plan| plan.source_identity.clone())
                        .unwrap_or_default(),
                    required_bytes,
                    total_pack_bytes: pinned
                        .as_ref()
                        .map(|plan| plan.total_pack_bytes)
                        .filter(|bytes| *bytes > 0)
                        .unwrap_or(known_pack_extent),
                    selected_transport: DownloadTransportKind::HttpRange,
                    estimated_overfetch: raw_transfer_bytes.saturating_sub(required_bytes),
                };
                prepared.transfer_bytes =
                    prepared.transfer_bytes.saturating_add(raw_transfer_bytes);
                prepared.plans.push(plan);
            }
        }
        Ok(prepared)
    }

    fn download_xet_pack_to_store<F>(
        &self,
        staged_chunks_root: &Path,
        pack: &PlannedXetPack,
        control: &JobControl,
        on_progress: &mut F,
    ) -> Result<(), JobError>
    where
        F: FnMut(DownloadProgress) -> Result<(), JobError>,
    {
        self.pin_pack_source_metadata(&pack.base.url, &pack.relative_path, &pack.metadata)?;
        let safe_pack_id = pack
            .plan
            .pack_id
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect::<String>();
        let transport_root = staged_chunks_root.join("_transport");
        let cache_dir = transport_root.join("xet-cache");
        let spool_path = transport_root.join("xet-spool").join(format!(
            "{}.pack.part",
            if safe_pack_id.is_empty() {
                "pack"
            } else {
                &safe_pack_id
            }
        ));
        let task_id = format!("xet:{}", pack.plan.pack_id);
        let mut callback_error = None::<JobError>;
        let stream_result = stream_hf_xet_pack_to_spool(
            &pack.location,
            pack.base.token.as_deref(),
            &cache_dir,
            &pack.relative_path,
            &spool_path,
            pack.metadata.file_size,
            || control.is_canceled(),
            || control.is_paused(),
            |wire_delta, _written, durable| {
                let progress = DownloadProgress {
                    task_id: task_id.clone(),
                    committed_bytes: 0,
                    wire_bytes_delta: wire_delta,
                    in_flight_bytes: durable,
                    clear_in_flight: false,
                    retry_count: 0,
                    rate_bytes_per_second: 0,
                    retry_wait_ms: 0,
                    rate_limit_wait_ms: 0,
                    transport: DownloadTransportKind::XetPack,
                    stall_reason: "network".to_string(),
                    active_connections: 1,
                    queue_bytes: 0,
                };
                match on_progress(progress) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        callback_error = Some(error);
                        Err("download progress callback failed".to_string())
                    }
                }
            },
        );
        if let Some(error) = callback_error {
            return Err(error);
        }
        if let Err(error) = stream_result {
            if control.is_canceled() || error == "download canceled" {
                return Err(JobError::Canceled);
            }
            if error.contains("source size changed") || error.contains("source ended early") {
                cleanup_xet_spool(&spool_path);
                return Err(JobError::Depot(error));
            }
            cleanup_xet_spool(&spool_path);
            return Err(JobError::Transient(error));
        }

        let write_result = (|| -> Result<(), JobError> {
            let mut spool = File::open(long_path(&spool_path))?;
            for chunk in &pack.chunks {
                let chunk_path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
                if compressed_chunk_file_valid(&chunk_path, chunk)? {
                    continue;
                }
                let length = usize::try_from(chunk.compressed_size).map_err(|_| {
                    JobError::Depot(format!("chunk {} is too large to verify", chunk.hash))
                })?;
                let mut compressed = vec![0_u8; length];
                spool.seek(SeekFrom::Start(chunk.pack_offset))?;
                spool.read_exact(&mut compressed)?;
                verify_compressed_chunk_bytes(chunk, &compressed)?;
                write_chunk_file(&chunk_path, &compressed)?;
            }
            Ok(())
        })();
        cleanup_xet_spool(&spool_path);
        write_result?;
        on_progress(DownloadProgress {
            task_id,
            committed_bytes: pack.metadata.file_size,
            wire_bytes_delta: 0,
            in_flight_bytes: 0,
            clear_in_flight: true,
            retry_count: 0,
            rate_bytes_per_second: 0,
            retry_wait_ms: 0,
            rate_limit_wait_ms: 0,
            transport: DownloadTransportKind::XetPack,
            stall_reason: String::new(),
            active_connections: 0,
            queue_bytes: 0,
        })?;
        Ok(())
    }

    fn cached_signed_pack_url(&self, key: &str) -> Option<String> {
        self.resolved_pack_urls
            .lock()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
            .filter(|entry| entry.refresh_at > Instant::now())
            .and_then(|entry| entry.signed_url)
    }

    fn expire_signed_pack_url(&self, key: &str) {
        if let Ok(mut cache) = self.resolved_pack_urls.lock() {
            if let Some(entry) = cache.get_mut(key) {
                entry.refresh_at = Instant::now();
                entry.signed_url = None;
            }
        }
    }

    fn validate_pack_response(
        &self,
        key: &str,
        response: &reqwest::blocking::Response,
        resolved_from_origin: bool,
        origin_url: &str,
    ) -> Result<(), JobError> {
        let identity = pack_response_identity(response.headers());
        let total_pack_bytes = content_range_total(response.headers());
        let redirected_url = if resolved_from_origin && response.url().as_str() != origin_url {
            Some(response.url().as_str().to_string())
        } else {
            None
        };

        let mut cache = self
            .resolved_pack_urls
            .lock()
            .map_err(|_| JobError::Depot("pack source cache is unavailable".to_string()))?;
        if let Some(previous) = cache.get(key) {
            if let (Some(expected), Some(observed)) =
                (previous.source_identity.as_deref(), identity.as_deref())
            {
                if !source_identities_match(expected, observed) {
                    return Err(JobError::Depot(
                        "source revision changed while downloading pack".to_string(),
                    ));
                }
            }
            if previous.total_pack_bytes.is_some()
                && total_pack_bytes.is_some()
                && previous.total_pack_bytes != total_pack_bytes
            {
                return Err(JobError::Depot(
                    "source pack size changed while downloading".to_string(),
                ));
            }
        }

        let entry = cache.entry(key.to_string()).or_insert(ResolvedPackUrl {
            signed_url: None,
            refresh_at: Instant::now(),
            source_identity: identity.clone(),
            total_pack_bytes,
        });
        if entry.source_identity.is_none() {
            entry.source_identity = identity;
        }
        if entry.total_pack_bytes.is_none() {
            entry.total_pack_bytes = total_pack_bytes;
        }
        if let Some(url) = redirected_url {
            // Keep the signed URL only in process memory and refresh before its
            // server-provided expiry. No credentials or query parameters enter
            // the journal, telemetry, or diagnostic log.
            let refresh_at = signed_url_refresh_at(&url);
            entry.signed_url = Some(url);
            entry.refresh_at = refresh_at;
        }
        Ok(())
    }

    fn get_client(&self) -> &Client {
        self.client.get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(32)
                .tcp_keepalive(Duration::from_secs(30))
                .tcp_nodelay(true)
                .http2_adaptive_window(true)
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
            if let Some(base) = self
                .base_urls
                .iter()
                .find(|candidate| candidate.url == active)
            {
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

    fn json_path_is_known_missing(&self, relative_path: &str) -> bool {
        self.missing_json_paths
            .lock()
            .ok()
            .is_some_and(|paths| paths.contains(relative_path))
    }

    fn remember_missing_json_path(&self, relative_path: &str) {
        if let Ok(mut paths) = self.missing_json_paths.lock() {
            paths.insert(relative_path.to_string());
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
            JobError::Transient(format!("download failed: {}", error.without_url()))
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
                JobError::Transient(format!("anonymous retry failed ({})", error.without_url()))
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
                detail: format!("HTTP 429"),
                retry_after_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
            });
        }
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(JobError::Unauthorized(format!("HTTP {status}")));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(JobError::NotFound(format!("HTTP 404")));
        }
        if status == StatusCode::REQUEST_TIMEOUT || status.is_server_error() {
            self.rate_coordinator.record_transient_failure(base_url);
            return Err(JobError::Transient(format!("HTTP {status}")));
        }
        if !status.is_success() {
            return Err(JobError::Depot(format!("HTTP {status}")));
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
        let mut manifest: VersionManifest = self.load_json(path)?;
        validate_format_version(manifest.format_version, "manifest")?;

        // If the manifest itself carries no launch options (e.g. older depots
        // uploaded before the multi-launch feature), fall back to build-info.json
        // which is now the canonical location for launch config.
        if manifest.launch_options.is_empty() {
            let build_info_path = format!("versions/{}/build-info.json", version);
            // load_json is best-effort here; ignore errors silently.
            if let Ok(build_info) = self.load_json::<serde_json::Value>(&build_info_path) {
                if let Some(opts) = build_info.get("launchOptions").and_then(|v| v.as_array()) {
                    let parsed: Vec<crate::manifest::LaunchOption> = opts
                        .iter()
                        .filter_map(|o| serde_json::from_value(o.clone()).ok())
                        .collect();
                    if !parsed.is_empty() {
                        manifest.launch_options = parsed;
                    }
                }
                // Also pick up launchExecutable from build-info if the manifest lacks it.
                if manifest.launch_executable.is_none() {
                    if let Some(exe) = build_info.get("launchExecutable").and_then(|v| v.as_str()) {
                        if !exe.is_empty() {
                            manifest.launch_executable = Some(exe.to_string());
                        }
                    }
                }
            }
        }

        Ok(canonicalize_manifest_version(manifest, version))
    }

    fn load_json<T: for<'de> Deserialize<'de>>(&self, relative_path: &str) -> Result<T, JobError> {
        let mut local_candidate_missing = false;
        if let Some(root) = &self.local_root {
            let path = root.join(relative_to_path(relative_path));
            if path.exists() {
                let bytes = fs::read(path)?;
                return Ok(serde_json::from_slice(&bytes)?);
            }
            local_candidate_missing = true;
        }

        if self.json_path_is_known_missing(relative_path) {
            return Err(JobError::NotFound(relative_path.to_string()));
        }

        let encoded_relative_path = encode_hf_relative_path(relative_path);
        let mut failures = Vec::new();
        let mut attempted_remotes = 0_usize;
        let mut missing_remotes = 0_usize;
        for base in self.ordered_base_urls() {
            attempted_remotes = attempted_remotes.saturating_add(1);
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
                Err(JobError::NotFound(err)) => {
                    missing_remotes = missing_remotes.saturating_add(1);
                    failures.push(err);
                }
                Err(err) => failures.push(err.to_string()),
            }
        }

        // Preserve the semantic distinction between "optional object absent"
        // and "content service failed". load_patch_manifest relies on
        // JobError::NotFound to treat games without patches as a normal case.
        if all_remote_json_candidates_missing(attempted_remotes, missing_remotes)
            || (local_candidate_missing && attempted_remotes == 0)
        {
            self.remember_missing_json_path(relative_path);
            return Err(JobError::NotFound(relative_path.to_string()));
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

    fn ensure_pack_span_with_progress(
        &self,
        pack_id: &str,
        start: u64,
        end_exclusive: u64,
        relative_path: &str,
        task_id: &str,
        partial_path: &Path,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, JobError>>,
    ) -> Result<(), JobError> {
        let expected_len = end_exclusive.saturating_sub(start);
        if expected_len == 0 {
            return Ok(());
        }
        if let Some(parent) = partial_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                JobError::Depot(format!(
                    "failed to create download dir '{}': {e}",
                    parent.display()
                ))
            })?;
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
                    false,
                    control,
                    progress_tx,
                )?;
                return validate_completed_partial(partial_path, expected_len, pack_id);
            }
        }

        let encoded_relative_path = encode_hf_relative_path(relative_path);
        let mut failures = Vec::new();
        for base in self.ordered_base_urls() {
            // Check if already completed (don't normalize again - already done once above)
            let existing = durable_partial_len(partial_path).min(expected_len);
            if existing == expected_len {
                return validate_completed_partial(partial_path, expected_len, pack_id);
            }

            let request_start = start.saturating_add(existing);
            let end = end_exclusive - 1;
            let url = format!(
                "{}/{}",
                base.url.trim_end_matches('/'),
                encoded_relative_path
            );
            let cache_key = format!("{}\n{}", base.url, relative_path);
            let mut resolved_from_origin = false;
            let response_result = if let Some(signed_url) = self.cached_signed_pack_url(&cache_key)
            {
                let unsigned_base = DepotRemoteBase {
                    url: base.url.clone(),
                    token: None,
                };
                match self.send_remote_get(
                    &unsigned_base,
                    &signed_url,
                    Some((request_start, end)),
                    Some(control),
                ) {
                    Ok(response) => Ok(response),
                    Err(JobError::Unauthorized(_)) => {
                        self.expire_signed_pack_url(&cache_key);
                        resolved_from_origin = true;
                        self.send_remote_get(&base, &url, Some((request_start, end)), Some(control))
                    }
                    Err(JobError::NotFound(_)) => {
                        return Err(JobError::Depot(
                            "source revision changed while downloading pack".to_string(),
                        ));
                    }
                    Err(error) => Err(error),
                }
            } else {
                resolved_from_origin = true;
                self.send_remote_get(&base, &url, Some((request_start, end)), Some(control))
            };
            let mut response = match response_result {
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
            self.validate_pack_response(&cache_key, &response, resolved_from_origin, &url)?;

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
                true,
                control,
                progress_tx,
            ) {
                Ok(final_len) if final_len == expected_len => {
                    self.mark_active_base_url(&base.url);
                    return validate_completed_partial(partial_path, expected_len, pack_id);
                }
                Ok(final_len) => failures.push(format!(
                    "{url}: range size mismatch for {pack_id}; expected {expected_len}, got {final_len}"
                )),
                Err(JobError::Canceled) => return Err(JobError::Canceled),
                Err(err @ JobError::Transient(_)) | Err(err @ JobError::RateLimited { .. }) => {
                    return Err(err);
                }
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

    #[allow(clippy::too_many_arguments)]
    fn fetch_pack_span_with_progress(
        &self,
        pack_id: &str,
        start: u64,
        end_exclusive: u64,
        relative_path: &str,
        task_id: &str,
        partial_path: &Path,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, JobError>>,
    ) -> Result<Vec<u8>, JobError> {
        self.ensure_pack_span_with_progress(
            pack_id,
            start,
            end_exclusive,
            relative_path,
            task_id,
            partial_path,
            control,
            progress_tx,
        )?;
        read_completed_partial(partial_path, end_exclusive.saturating_sub(start), pack_id)
    }

    fn download_chunks_to_store_parallel<F>(
        &self,
        staged_chunks_root: &Path,
        chunks: &[ChunkRef],
        target_version: Option<&str>,
        control: Arc<JobControl>,
        on_progress: F,
    ) -> Result<(), JobError>
    where
        F: FnMut(DownloadProgress) -> Result<(), JobError>,
    {
        self.download_chunks_to_store_parallel_at_prefix(
            staged_chunks_root,
            chunks,
            target_version,
            None,
            control,
            on_progress,
        )
    }

    fn download_chunks_to_store_parallel_at_prefix<F>(
        &self,
        staged_chunks_root: &Path,
        chunks: &[ChunkRef],
        target_version: Option<&str>,
        pack_path_prefix: Option<&str>,
        control: Arc<JobControl>,
        mut on_progress: F,
    ) -> Result<(), JobError>
    where
        F: FnMut(DownloadProgress) -> Result<(), JobError>,
    {
        if chunks.is_empty() {
            return Ok(());
        }

        let tasks = build_pack_download_tasks(chunks, self.effective_pack_range_task_bytes());
        let settings = crate::platform::current_settings();
        let queue_budget = settings.download_queue_mb.saturating_mul(1024 * 1024);
        let workers_by_budget =
            (queue_budget / self.effective_pack_range_task_bytes()).max(1) as usize;
        let worker_count = self
            .effective_worker_count()
            .min(workers_by_budget)
            .min(tasks.len())
            .max(1);
        let decoder_count = thread::available_parallelism()
            .map(|count| (count.get() / 2).clamp(1, 4))
            .unwrap_or(1)
            .min(tasks.len())
            .max(1);
        let downloaded_queue_capacity =
            workers_by_budget.min(worker_count.saturating_mul(2)).max(1);
        let tasks = Arc::new(Mutex::new(VecDeque::from(tasks)));
        let abort = Arc::new(AtomicBool::new(false));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let queued_bytes = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::channel::<Result<DownloadProgress, JobError>>();
        let (downloaded_tx, downloaded_rx) =
            mpsc::sync_channel::<DownloadedPackTask>(downloaded_queue_capacity);
        let downloaded_rx = Arc::new(Mutex::new(downloaded_rx));
        let mut first_error: Option<JobError> = None;

        thread::scope(|scope| {
            for _ in 0..decoder_count {
                let downloaded_rx = Arc::clone(&downloaded_rx);
                let abort = Arc::clone(&abort);
                let tx = tx.clone();
                let staged_chunks_root = staged_chunks_root.to_path_buf();
                let queued_bytes = Arc::clone(&queued_bytes);
                scope.spawn(move || loop {
                    let downloaded = {
                        let receiver = match downloaded_rx.lock() {
                            Ok(receiver) => receiver,
                            Err(_) => {
                                abort.store(true, Ordering::SeqCst);
                                let _ = tx.send(Err(JobError::Depot(
                                    "download verification queue is unavailable".to_string(),
                                )));
                                return;
                            }
                        };
                        receiver.recv()
                    };
                    let downloaded = match downloaded {
                        Ok(downloaded) => downloaded,
                        Err(_) => return,
                    };
                    atomic_saturating_sub(
                        &queued_bytes,
                        downloaded
                            .task
                            .range_end
                            .saturating_sub(downloaded.task.range_start),
                    );
                    if abort.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Err(error) = DepotSource::verify_downloaded_pack_task_to_store(
                        &staged_chunks_root,
                        downloaded,
                        &tx,
                    ) {
                        abort.store(true, Ordering::SeqCst);
                        let _ = tx.send(Err(error));
                        return;
                    }
                });
            }
            for worker_index in 0..worker_count {
                let tasks = Arc::clone(&tasks);
                let abort = Arc::clone(&abort);
                let tx = tx.clone();
                let source = self.clone();
                let target_version = target_version.map(String::from);
                let pack_path_prefix = pack_path_prefix.map(String::from);
                let control = Arc::clone(&control);
                let staged_chunks_root = staged_chunks_root.to_path_buf();
                let downloaded_tx = downloaded_tx.clone();
                let active_connections = Arc::clone(&active_connections);
                let queued_bytes = Arc::clone(&queued_bytes);
                scope.spawn(move || loop {
                    if abort.load(Ordering::SeqCst) {
                        break;
                    }
                    if control.is_canceled() {
                        abort.store(true, Ordering::SeqCst);
                        let _ = tx.send(Err(JobError::Canceled));
                        break;
                    }
                    while control.is_paused() {
                        if control.is_canceled() {
                            abort.store(true, Ordering::SeqCst);
                            let _ = tx.send(Err(JobError::Canceled));
                            return;
                        }
                        thread::sleep(Duration::from_millis(150));
                    }

                    // Auto starts conservatively and wakes additional workers only
                    // after this source's governor observes useful throughput.
                    while worker_index >= source.active_connection_limit() {
                        if abort.load(Ordering::SeqCst) || control.is_canceled() {
                            return;
                        }
                        let queue_empty =
                            tasks.lock().map(|guard| guard.is_empty()).unwrap_or(true);
                        if queue_empty {
                            return;
                        }
                        thread::sleep(Duration::from_millis(50));
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
                        let attempt_started = Instant::now();
                        let attempt_result = {
                            let _active = ActiveConnectionGuard::new(&active_connections);
                            source.download_pack_task_to_spool(
                                &staged_chunks_root,
                                &task,
                                &task_id,
                                target_version.as_deref(),
                                pack_path_prefix.as_deref(),
                                &control,
                                &tx,
                            )
                        };
                        match attempt_result {
                            Ok(mut downloaded_task) => {
                                let elapsed = attempt_started.elapsed();
                                let bytes = task.range_end.saturating_sub(task.range_start);
                                let throughput = if elapsed.is_zero() {
                                    0
                                } else {
                                    (u128::from(bytes) * 1_000 / elapsed.as_millis().max(1))
                                        .min(u128::from(u64::MAX))
                                        as u64
                                };
                                source.observe_transport_window(
                                    elapsed, throughput, 0.0, false, 0, false,
                                );
                                let queue_wait_started = Instant::now();
                                loop {
                                    let task_bytes = downloaded_task
                                        .task
                                        .range_end
                                        .saturating_sub(downloaded_task.task.range_start);
                                    queued_bytes.fetch_add(task_bytes, Ordering::Relaxed);
                                    match downloaded_tx.try_send(downloaded_task) {
                                        Ok(()) => break,
                                        Err(mpsc::TrySendError::Full(task)) => {
                                            atomic_saturating_sub(&queued_bytes, task_bytes);
                                            downloaded_task = task;
                                            source.observe_transport_window(
                                                Duration::ZERO,
                                                0,
                                                0.0,
                                                false,
                                                0,
                                                true,
                                            );
                                            if abort.load(Ordering::SeqCst) || control.is_canceled()
                                            {
                                                return;
                                            }
                                            thread::sleep(Duration::from_millis(25));
                                        }
                                        Err(mpsc::TrySendError::Disconnected(_)) => {
                                            atomic_saturating_sub(&queued_bytes, task_bytes);
                                            abort.store(true, Ordering::SeqCst);
                                            let _ = tx.send(Err(JobError::Depot(
                                                "download verification queue stopped".to_string(),
                                            )));
                                            return;
                                        }
                                    }
                                }
                                if queue_wait_started.elapsed() >= Duration::from_millis(250) {
                                    let _ = tx.send(Ok(DownloadProgress {
                                        task_id: task_id.clone(),
                                        committed_bytes: 0,
                                        wire_bytes_delta: 0,
                                        in_flight_bytes: task
                                            .range_end
                                            .saturating_sub(task.range_start),
                                        clear_in_flight: false,
                                        retry_count,
                                        rate_bytes_per_second: 0,
                                        retry_wait_ms: 0,
                                        rate_limit_wait_ms: 0,
                                        transport: DownloadTransportKind::HttpRange,
                                        stall_reason: "verifying".to_string(),
                                        active_connections: 0,
                                        queue_bytes: 0,
                                    }));
                                }
                                break;
                            }
                            Err(err) if retry_count < max_retries && !control.is_canceled() => {
                                let next_retry = retry_count.saturating_add(1);
                                let Some(delay) = err.retry_delay(next_retry) else {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(err));
                                    break;
                                };
                                retry_count = next_retry;
                                source.observe_transport_window(
                                    attempt_started.elapsed(),
                                    0,
                                    1.0,
                                    matches!(err, JobError::RateLimited { .. }),
                                    0,
                                    false,
                                );
                                let _ = tx.send(Ok(DownloadProgress {
                                    task_id: task_id.clone(),
                                    committed_bytes: 0,
                                    wire_bytes_delta: 0,
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
                                    transport: DownloadTransportKind::HttpRange,
                                    stall_reason: if matches!(err, JobError::RateLimited { .. }) {
                                        "rate-limit".to_string()
                                    } else {
                                        "retry".to_string()
                                    },
                                    active_connections: 0,
                                    queue_bytes: 0,
                                }));
                                if let Err(err) = sleep_with_control(delay, &control) {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(err));
                                    break;
                                }
                            }
                            Err(err) => {
                                abort.store(true, Ordering::SeqCst);
                                let _ = tx.send(Err(err));
                                break;
                            }
                        }

                        if abort.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                });
            }
            drop(downloaded_tx);
            drop(tx);

            for message in rx {
                match message {
                    Ok(mut progress) => {
                        progress.active_connections = active_connections.load(Ordering::Relaxed);
                        progress.queue_bytes = queued_bytes.load(Ordering::Relaxed);
                        if let Err(err) = on_progress(progress) {
                            abort.store(true, Ordering::SeqCst);
                            first_error = Some(err);
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
            return Err(error);
        }
        Ok(())
    }

    fn download_pack_task_to_spool(
        &self,
        staged_chunks_root: &Path,
        task: &PackDownloadTask,
        task_id: &str,
        target_version: Option<&str>,
        pack_path_prefix: Option<&str>,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, JobError>>,
    ) -> Result<DownloadedPackTask, JobError> {
        let relative_path =
            Self::pack_relative_path_in(&task.pack_id, target_version, pack_path_prefix);
        let partial_path = partial_range_path_in(staged_chunks_root, task, pack_path_prefix);
        self.ensure_pack_span_with_progress(
            &task.pack_id,
            task.range_start,
            task.range_end,
            &relative_path,
            task_id,
            &partial_path,
            control,
            progress_tx,
        )?;
        Ok(DownloadedPackTask {
            task: task.clone(),
            task_id: task_id.to_string(),
            partial_path,
        })
    }

    fn verify_downloaded_pack_task_to_store(
        staged_chunks_root: &Path,
        downloaded: DownloadedPackTask,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, JobError>>,
    ) -> Result<(), JobError> {
        let task = &downloaded.task;
        let write_result = (|| -> Result<(), JobError> {
            validate_completed_partial(
                &downloaded.partial_path,
                task.range_end.saturating_sub(task.range_start),
                &task.pack_id,
            )?;
            let mut range = File::open(long_path(&downloaded.partial_path))?;
            for chunk in &task.chunks {
                let path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
                if compressed_chunk_file_valid(&path, chunk)? {
                    continue;
                }

                let start = chunk.pack_offset.saturating_sub(task.range_start);
                let length = usize::try_from(chunk.compressed_size).map_err(|_| {
                    JobError::Depot(format!("chunk {} is too large to verify", chunk.hash))
                })?;
                if start.saturating_add(chunk.compressed_size)
                    > task.range_end.saturating_sub(task.range_start)
                {
                    return Err(JobError::Depot(format!(
                        "pack range does not contain chunk {}",
                        chunk.hash
                    )));
                }
                let mut compressed = vec![0_u8; length];
                range.seek(SeekFrom::Start(start))?;
                range.read_exact(&mut compressed)?;
                verify_compressed_chunk_bytes(chunk, &compressed)?;
                write_chunk_file(&path, &compressed)?;
            }
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&downloaded.partial_path);
            let _ = fs::remove_file(partial_checkpoint_path(&downloaded.partial_path));
            return Err(err);
        }
        let _ = fs::remove_file(&downloaded.partial_path);
        let _ = fs::remove_file(partial_checkpoint_path(&downloaded.partial_path));
        progress_tx
            .send(Ok(DownloadProgress {
                task_id: downloaded.task_id,
                committed_bytes: task.range_end - task.range_start,
                wire_bytes_delta: 0,
                in_flight_bytes: 0,
                clear_in_flight: true,
                retry_count: 0,
                rate_bytes_per_second: 0,
                retry_wait_ms: 0,
                rate_limit_wait_ms: 0,
                transport: DownloadTransportKind::HttpRange,
                stall_reason: String::new(),
                active_connections: 0,
                queue_bytes: 0,
            }))
            .map_err(|_| JobError::Canceled)?;
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

#[derive(Debug)]
struct DownloadedPackTask {
    task: PackDownloadTask,
    task_id: String,
    partial_path: PathBuf,
}

struct ActiveConnectionGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> ActiveConnectionGuard<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for ActiveConnectionGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

fn atomic_saturating_sub(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(value))
    });
}

impl PackDownloadTask {
    fn id(&self) -> String {
        format!("{}:{}-{}", self.pack_id, self.range_start, self.range_end)
    }
}

fn partial_range_path(staged_chunks_root: &Path, task: &PackDownloadTask) -> PathBuf {
    staged_chunks_root.join(partial_range_file_name(task))
}

fn partial_range_path_in(
    staged_chunks_root: &Path,
    task: &PackDownloadTask,
    pack_path_prefix: Option<&str>,
) -> PathBuf {
    let prefix = pack_path_prefix.map(str::trim).unwrap_or_default();
    if prefix.is_empty() {
        return partial_range_path(staged_chunks_root, task);
    }
    let namespace = sha256_bytes(prefix.as_bytes());
    staged_chunks_root
        .join("_transport")
        .join("raw-spool")
        .join(&namespace[..16])
        .join(partial_range_file_name(task))
}

fn partial_range_file_name(task: &PackDownloadTask) -> String {
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
    format!(
        "{pack_file_id}-{}-{}.part",
        task.range_start, task.range_end
    )
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
    let lp = long_path(path);
    let actual = partial_file_len(&lp);

    if actual == 0 {
        return 0;
    }

    let lp_ckpt = long_path(&partial_checkpoint_path(path));
    let checkpoint = fs::read_to_string(lp_ckpt).ok().and_then(|value| {
        let trimmed = value.trim();
        // Validate checkpoint: must be numeric and not exceed actual file size
        trimmed.parse::<u64>().ok().filter(|&ckpt| ckpt <= actual)
    });

    checkpoint.unwrap_or(0).min(actual)
}

fn persist_partial_checkpoint(path: &Path, durable_len: u64) -> Result<(), JobError> {
    let checkpoint = partial_checkpoint_path(path);
    transport::persist_durable_counter(&checkpoint, durable_len).map_err(JobError::Depot)
}

fn normalize_partial_file(path: &Path, expected_len: u64) -> Result<(), JobError> {
    let lp = long_path(path);
    let lp_checkpoint = long_path(&partial_checkpoint_path(path));

    // Try to open the file first to avoid TOCTOU race
    let file_res = OpenOptions::new().write(true).open(&lp);

    let file = match file_res {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist - clean up orphaned checkpoint if any
            if lp_checkpoint.exists() {
                let _ = remove_file_with_retry(&lp_checkpoint, 2);
            }
            return Ok(());
        }
        Err(e) => {
            return Err(JobError::Depot(format!(
                "failed to open existing partial '{}' for normalization: {e}",
                path.display()
            )))
        }
    };

    let actual_len = file.metadata().map(|m| m.len()).unwrap_or(0);

    // If file is larger than expected, truncate it completely and remove checkpoint
    if actual_len > expected_len {
        drop(file); // Close file before removing
        let _ = remove_file_with_retry(&lp, 2);
        if lp_checkpoint.exists() {
            let _ = remove_file_with_retry(&lp_checkpoint, 2);
        }
        return Ok(());
    }

    // Determine durable length from checkpoint
    let durable = durable_partial_len(path).min(expected_len);

    // Truncate to durable checkpoint position if needed
    if actual_len != durable {
        file.set_len(durable)?;
        file.sync_all()?;
    }

    drop(file); // Release file handle before writing checkpoint
    persist_partial_checkpoint(path, durable)?;
    Ok(())
}

fn read_completed_partial(
    path: &Path,
    expected_len: u64,
    pack_id: &str,
) -> Result<Vec<u8>, JobError> {
    validate_completed_partial(path, expected_len, pack_id)?;
    Ok(fs::read(long_path(path))?)
}

fn validate_completed_partial(
    path: &Path,
    expected_len: u64,
    pack_id: &str,
) -> Result<(), JobError> {
    if durable_partial_len(path) != expected_len {
        return Err(JobError::Depot(format!(
            "partial range for {pack_id} is not durably checkpointed"
        )));
    }
    let actual_len = fs::metadata(long_path(path))?.len();
    if actual_len != expected_len {
        return Err(JobError::Depot(format!(
            "partial range size mismatch for {pack_id}; expected {expected_len}, got {actual_len}"
        )));
    }
    Ok(())
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

fn content_range_total(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit_once('/')?
        .1
        .parse::<u64>()
        .ok()
}

fn pack_response_identity(headers: &HeaderMap) -> Option<String> {
    ["x-xet-hash", "x-linked-etag", "etag", "x-repo-commit"]
        .into_iter()
        .find_map(|name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| format!("{name}:{}", value.trim()))
        })
}

fn signed_url_refresh_delay(value: &str, now_epoch_seconds: u64) -> Option<Duration> {
    const EXPIRY_MARGIN_SECONDS: u64 = 60;

    let parsed = url::Url::parse(value).ok()?;
    let expires = parsed.query_pairs().find_map(|(name, value)| {
        name.eq_ignore_ascii_case("Expires")
            .then(|| value.parse::<u64>().ok())
            .flatten()
    })?;
    Some(Duration::from_secs(
        expires
            .saturating_sub(now_epoch_seconds)
            .saturating_sub(EXPIRY_MARGIN_SECONDS),
    ))
}

fn signed_url_refresh_at(value: &str) -> Instant {
    let now_epoch_seconds = Utc::now().timestamp().max(0) as u64;
    let delay = signed_url_refresh_delay(value, now_epoch_seconds)
        .unwrap_or_else(|| Duration::from_secs(4 * 60));
    Instant::now() + delay
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
    count_wire_bytes: bool,
    control: &JobControl,
    progress_tx: &mpsc::Sender<Result<DownloadProgress, JobError>>,
) -> Result<u64, JobError> {
    let lp = long_path(partial_path);
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lp)
        .map_err(|e| {
            JobError::Depot(format!(
                "failed to open partial file '{}' (long: '{}'): {e}",
                partial_path.display(),
                lp.display()
            ))
        })?;
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
            wire_bytes_delta: 0,
            in_flight_bytes: durable,
            clear_in_flight: false,
            retry_count: 0,
            rate_bytes_per_second: 0,
            retry_wait_ms: 0,
            rate_limit_wait_ms: 0,
            transport: DownloadTransportKind::HttpRange,
            stall_reason: "network".to_string(),
            active_connections: 0,
            queue_bytes: 0,
        }))
        .map_err(|_| JobError::Canceled)?;

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
                return Err(JobError::Transient(format!(
                    "download stream interrupted for {task_id}: {error}"
                )));
            }
        };
        if read == 0 {
            let _ = checkpoint_partial(&mut output, partial_path, written);
            return Err(JobError::Transient(format!(
                "download stream ended early for {task_id}; expected {expected_len} bytes, got {written}"
            )));
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
                    wire_bytes_delta: if count_wire_bytes {
                        written.saturating_sub(last_progress_bytes)
                    } else {
                        0
                    },
                    in_flight_bytes: durable,
                    clear_in_flight: false,
                    retry_count: 0,
                    rate_bytes_per_second: rate,
                    retry_wait_ms: 0,
                    rate_limit_wait_ms: 0,
                    transport: DownloadTransportKind::HttpRange,
                    stall_reason: "network".to_string(),
                    active_connections: 0,
                    queue_bytes: 0,
                }))
                .map_err(|_| JobError::Canceled)?;
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
    existing_partial_task_progress_in(staged_chunks_root, chunks, None)
}

fn existing_partial_task_progress_in(
    staged_chunks_root: &Path,
    chunks: &[ChunkRef],
    pack_path_prefix: Option<&str>,
) -> HashMap<String, u64> {
    build_pack_download_tasks(chunks, pack_range_task_bytes())
        .into_iter()
        .filter_map(|task| {
            let expected = task.range_end.saturating_sub(task.range_start);
            let existing = durable_partial_len(&partial_range_path_in(
                staged_chunks_root,
                &task,
                pack_path_prefix,
            ))
            .min(expected);
            (existing > 0).then(|| (task.id(), existing))
        })
        .collect()
}

#[derive(Debug, Clone)]
struct DownloadProgress {
    task_id: String,
    committed_bytes: u64,
    wire_bytes_delta: u64,
    in_flight_bytes: u64,
    clear_in_flight: bool,
    retry_count: u32,
    rate_bytes_per_second: u64,
    retry_wait_ms: u64,
    rate_limit_wait_ms: u64,
    transport: DownloadTransportKind,
    stall_reason: String,
    active_connections: usize,
    queue_bytes: u64,
}

#[derive(Debug, Clone)]
struct InstalledUpdateBase {
    version: String,
    manifest: VersionManifest,
    source_label: String,
}

fn canonical_version_label(value: &str) -> String {
    let mut end = value.trim().len();
    let lower = value.trim().to_ascii_lowercase();
    for marker in [" - uploaded", " (build"] {
        if let Some(index) = lower.find(marker) {
            end = end.min(index);
        }
    }
    value.trim()[..end].trim().to_ascii_lowercase()
}

fn versions_equivalent(left: &str, right: &str) -> bool {
    let left = canonical_version_label(left);
    !left.is_empty() && left == canonical_version_label(right)
}

fn update_transport_operation(
    catalog: &Catalog,
    from_version: &str,
    target_version: &str,
) -> TransportOperation {
    let find_index = |value: &str| {
        catalog
            .versions
            .iter()
            .position(|entry| versions_equivalent(&entry.version, value))
    };
    match (find_index(from_version), find_index(target_version)) {
        (Some(from), Some(target)) if target < from => TransportOperation::Downgrade,
        _ => TransportOperation::Update,
    }
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
    // install/update. The mutable remote catalog is *not* install history: old
    // entries can be pruned while users still legitimately have that version
    // installed. In that case manifest.0xo is the exact local base manifest we
    // need to plan the update, so prefer it when it canonically matches state.0xo.
    if let Some(version) = marker_version {
        let local_manifest_matches_marker = manifest_version
            .as_deref()
            .map(|manifest_version| versions_equivalent(manifest_version, &version))
            .unwrap_or(false);

        if !catalog_has_version(catalog, &version) {
            if local_manifest_matches_marker {
                if let Some(manifest) = installed_manifest.as_ref() {
                    return Ok(Some(InstalledUpdateBase {
                        version: version.clone(),
                        manifest: canonicalize_manifest_version(manifest.clone(), &version),
                        source_label:
                            ".0xolemon/state.0xo + local manifest.0xo (catalog history pruned)"
                                .to_string(),
                    }));
                }
            }

            return Err(JobError::Depot(format!(
                "installed version '{}' from .0xolemon/state.0xo is not present in this game's catalog and no matching local manifest.0xo is available",
                version
            )));
        }

        let (manifest, source_label) = match installed_manifest {
            Some(manifest)
                if manifest_version
                    .as_deref()
                    .map(|manifest_version| versions_equivalent(manifest_version, &version))
                    .unwrap_or(false) =>
            {
                (
                    canonicalize_manifest_version(manifest, &version),
                    ".0xolemon/state.0xo + manifest.0xo".to_string(),
                )
            }
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
    // installed manifest is intact. A local installed manifest is itself the
    // update base and must remain usable even after its historical catalog entry
    // has been pruned. This works for every game and never invokes a title-specific
    // signature scanner.
    if let Some(manifest) = installed_manifest {
        if let Some(version) = manifest_version {
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
        let Ok(mut local_file) = File::open(long_path(&path)) else {
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

fn discover_local_chunks(
    install_root: &Path,
    changed: &[FileEntry],
    control: &JobControl,
) -> Result<(HashMap<String, LocalChunkSource>, usize), JobError> {
    let required_hashes = changed
        .iter()
        .flat_map(|file| file.chunks.iter().map(|chunk| chunk.hash.clone()))
        .collect::<HashSet<_>>();
    let mut out = HashMap::new();
    let mut discovered = 0_usize;

    for file in changed {
        let path = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;

        let Ok(local_file) = File::open(long_path(&path)) else {
            continue;
        };

        let chunker = StreamCDC::new(
            local_file,
            crate::manifest::CHUNK_MIN_SIZE,
            crate::manifest::CHUNK_TARGET_SIZE,
            crate::manifest::CHUNK_MAX_SIZE,
        );
        let mut offset = 0_u64;

        for result in chunker {
            if control.is_canceled() {
                return Err(JobError::Canceled);
            }

            let chunk = match result {
                Ok(c) => c,
                Err(_) => break,
            };

            let hash = blake3::hash(&chunk.data).to_hex().to_string();
            if required_hashes.contains(&hash) {
                if out
                    .insert(
                        hash,
                        LocalChunkSource {
                            path: path.clone(),
                            offset,
                            size: chunk.length as u64,
                        },
                    )
                    .is_none()
                {
                    discovered = discovered.saturating_add(1);
                }
            }

            offset += chunk.length as u64;
        }
    }

    Ok((out, discovered))
}

fn plan_missing_chunks(
    local_sources: &HashMap<String, LocalChunkSource>,
    staged_chunks_root: &Path,
    changed: &[FileEntry],
    _install_root: Option<&Path>,
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

fn build_pack_download_tasks(chunks: &[ChunkRef], max_task_bytes: u64) -> Vec<PackDownloadTask> {
    let mut by_pack: HashMap<String, Vec<ChunkRef>> = HashMap::new();
    for chunk in chunks {
        by_pack
            .entry(chunk.pack_id.clone())
            .or_default()
            .push(chunk.clone());
    }

    let mut tasks = Vec::new();
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

/// Cleanup temporary download files when download is cancelled
/// Removes .part, .checkpoint, .chunk, and state JSON files from dl/ root
fn cleanup_download_temp_files(dl_root: &Path) {
    eprintln!(
        "[CLEANUP] Cleaning up temporary download files in {:?}",
        dl_root
    );

    if !dl_root.exists() {
        eprintln!("[CLEANUP] Download root doesn't exist, nothing to cleanup");
        return;
    }

    let mut cleaned_count = 0;

    // Read all files in dl/ root (not recursive - we only want root level files)
    if let Ok(entries) = fs::read_dir(dl_root) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if !file_type.is_file() {
                    continue; // Skip directories
                }
            }

            let path = entry.path();
            let should_remove = if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                matches!(ext, "part" | "checkpoint" | "chunk")
            } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                // Remove state JSON files
                name.ends_with("-state.json") || name == "depot-session.json"
            } else {
                false
            };

            if should_remove {
                match fs::remove_file(&path) {
                    Ok(_) => {
                        eprintln!("[CLEANUP] Removed: {:?}", path);
                        cleaned_count += 1;
                    }
                    Err(e) => {
                        eprintln!("[CLEANUP] Failed to remove {:?}: {}", path, e);
                    }
                }
            }
        }
    }

    eprintln!("[CLEANUP] Cleaned up {} temporary files", cleaned_count);
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
    settings
        .pack_range_mb
        .saturating_mul(1024 * 1024)
        .clamp(MIN_PACK_RANGE_TASK_BYTES, MAX_PACK_RANGE_TASK_BYTES)
}

fn download_transfer_bytes(chunks: &[ChunkRef]) -> u64 {
    build_pack_download_tasks(chunks, pack_range_task_bytes())
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
    journal.current_transport = progress.transport;
    journal.stall_reason = progress.stall_reason.clone();
    journal.wire_bytes_done = journal
        .wire_bytes_done
        .saturating_add(progress.wire_bytes_delta);
    if journal_uses_transport_v3(journal) {
        if let Ok(mut runtime) = DOWNLOAD_RUNTIME_TELEMETRY
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            runtime.insert(
                journal.id.clone(),
                RuntimeTransportState {
                    active_connections: progress.active_connections,
                    queue_bytes: progress.queue_bytes,
                },
            );
        }
    }
    let metrics = &mut journal.metrics;
    metrics.network_bytes = metrics
        .network_bytes
        .saturating_add(progress.wire_bytes_delta);
    metrics.wire_bytes = metrics.wire_bytes.saturating_add(progress.wire_bytes_delta);
    match progress.transport {
        DownloadTransportKind::HttpRange => {
            metrics.raw_range_bytes = metrics
                .raw_range_bytes
                .saturating_add(progress.wire_bytes_delta);
        }
        DownloadTransportKind::XetPack => {
            metrics.xet_bytes = metrics.xet_bytes.saturating_add(progress.wire_bytes_delta);
        }
    }
    metrics.retry_wait_ms = metrics.retry_wait_ms.saturating_add(progress.retry_wait_ms);
    metrics.rate_limit_wait_ms = metrics
        .rate_limit_wait_ms
        .saturating_add(progress.rate_limit_wait_ms);
    metrics.peak_in_flight_bytes = metrics.peak_in_flight_bytes.max(in_flight_bytes);
    if progress.rate_bytes_per_second > 0 {
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
    staged_chunks_root: &Path,
    file: &FileEntry,
    local_sources: &HashMap<String, LocalChunkSource>,
) -> Result<(), JobError> {
    let target = safe_join(install_root, &file.path)
        .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;

    // Check if target already valid
    if target_file_valid(&target, file)? {
        return Ok(());
    }

    // Create parent dirs in install folder
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    // Assemble directly in install folder (no staging)
    let temp = sibling_path(&target, "007launcher.tmp")?;
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
    let path = long_path(path);
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::metadata(&path)?;
    if metadata.len() != file.size {
        return Ok(false);
    }
    Ok(sha256_file(&path)? == file.sha256)
}

fn filter_already_assembled(
    app: &AppHandle,
    journal: &mut JobJournal,
    install_root: &Path,
    changed: Vec<FileEntry>,
    control: &Arc<JobControl>,
) -> Result<Vec<FileEntry>, JobError> {
    let mut actual_changed = Vec::with_capacity(changed.len());
    let mut already_assembled = 0;

    for file in changed {
        if control.is_canceled() {
            return Err(JobError::Canceled);
        }
        let target = install_root.join(&file.path);
        let target_io = long_path(&target);

        // Fast path: only if file exists and size matches, we check hash
        let mut is_valid = false;
        if target_io.exists() {
            if file.preserve {
                is_valid = true;
            } else if let Ok(metadata) = fs::metadata(&target_io) {
                if metadata.len() == file.size {
                    // Only hash files that match the exact size
                    if let Ok(true) = target_file_valid(&target, &file) {
                        is_valid = true;
                    }
                }
            }
        }

        if is_valid {
            already_assembled += 1;
        } else {
            actual_changed.push(file);
        }
    }

    if already_assembled > 0 {
        append_log(
            journal,
            "info",
            &format!(
                "Skipped {} files that were already fully assembled from a previous run",
                already_assembled
            ),
        );
        let _ = persist_and_emit(app, journal);
    }

    Ok(actual_changed)
}

pub fn abort_and_clean_job(
    app: &AppHandle,
    _requested_game_id: Option<&str>,
) -> Result<(), JobError> {
    if current_journal_has_pending_transaction(app)? {
        return Err(JobError::Depot(
            "This journal owns rollback backups and cannot be cleared yet. Resume it to complete the safe transaction cleanup."
                .to_string(),
        ));
    }
    let active_journal = read_latest_journal(app).ok().flatten();
    let canceled_job_id = active_journal.as_ref().map(|journal| journal.id.clone());
    if let Some(job_id) = canceled_job_id.as_deref() {
        clear_current_journal_if_matches(app, job_id)?;
    } else {
        clear_current_journal(app)?;
    }
    Ok(())
}

fn planned_update_temporary_space(files: &[FileEntry], network_bytes: u64) -> u64 {
    let largest_target = files.iter().map(|file| file.size).max().unwrap_or(0);
    let queue_budget = crate::platform::current_settings()
        .download_queue_mb
        .max(8)
        .saturating_mul(1024 * 1024);
    largest_target
        .saturating_add(network_bytes.min(queue_budget))
        .saturating_add(queue_budget)
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
    let mut file = File::open(long_path(path))?;
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
        // The local source file may have been overwritten/renamed by a previous assemble
        // step (os error 2). If so, fall back to the staged chunk silently.
        match File::open(long_path(&source.path)) {
            Ok(mut file) => {
                if file.seek(SeekFrom::Start(source.offset)).is_ok() {
                    let mut buffer = vec![0_u8; source.size as usize];
                    if file.read_exact(&mut buffer).is_ok() {
                        if verify_chunk_bytes(chunk, &buffer).is_ok() {
                            return Ok(buffer);
                        }
                    }
                }
                // File exists but seek/read/verify failed â€” fall through to staged chunk
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Local source was removed by a previous assemble step â€” fall through
            }
            Err(e) => return Err(e.into()),
        }
    }

    let path = staged_chunk_path_from(staged_chunks_root, &chunk.hash);
    if !long_path(&path).exists() {
        return Err(JobError::StageMissing(format!(
            "verified chunk {} at {}",
            chunk.hash,
            path.display()
        )));
    }
    let transport = fs::read(long_path(&path))?;
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
    // Optimization: chunks are written atomically via .tmp and rename,
    // so if the file exists and the size matches, we can trust it.
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() == chunk.compressed_size {
            return Ok(true);
        }
    }
    fs::remove_file(path)?;
    Ok(false)
}

fn write_chunk_file(path: &Path, data: &[u8]) -> Result<(), JobError> {
    let lp = long_path(path);
    if let Some(parent) = lp.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            JobError::Depot(format!(
                "failed to create chunk dir '{}': {e}",
                parent.display()
            ))
        })?;
    }
    // Use .bin.tmp extension to avoid replacing .chunk extension
    let temp = lp.with_extension("tmp");
    fs::write(&temp, data).map_err(|e| {
        JobError::Depot(format!(
            "failed to write chunk temp '{}': {e}",
            path.display()
        ))
    })?;
    fs::rename(&temp, &lp).map_err(|e| {
        JobError::Depot(format!("failed to rename chunk '{}': {e}", path.display()))
    })?;
    Ok(())
}

// Path/default helper functions live in job/paths.rs

fn read_install_marker(install_root: &Path) -> Result<Option<InstallMarker>, JobError> {
    let path = install_marker_path(install_root);
    recover_state_file_if_needed(&path)?;

    // Auto-restore backup if missing
    if !path.exists() {
        if let Some(backup_root) = crate::job::paths::get_launcher_backup_root() {
            let backup_dir = crate::job::paths::state_backup_dir(&backup_root, install_root);
            if backup_dir.exists() && backup_dir.join(INSTALL_MARKER_FILE).exists() {
                let marker_dir = install_root.join(INSTALL_MARKER_DIR);
                let _ = fs::create_dir_all(&marker_dir);
                if let Ok(data) = fs::read(backup_dir.join(INSTALL_MARKER_FILE)) {
                    let _ = fs::write(marker_dir.join(INSTALL_MARKER_FILE), data);
                }
                if let Ok(data) = fs::read(backup_dir.join(INSTALLED_MANIFEST_FILE)) {
                    let _ = fs::write(marker_dir.join(INSTALLED_MANIFEST_FILE), data);
                }
                if let Ok(data) = fs::read(backup_dir.join(APPLIED_PATCH_MANIFEST_FILE)) {
                    let _ = fs::write(marker_dir.join(APPLIED_PATCH_MANIFEST_FILE), data);
                }
            }
        }
    }

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

pub(crate) fn inspect_discoverable_install(
    install_root: &Path,
    game_ids: &[String],
) -> Result<Option<DiscoverableInstallMarker>, String> {
    let marker = read_install_marker(install_root).map_err(|error| error.to_string())?;
    let Some(marker) = marker else {
        return Ok(None);
    };
    let marker_compact = compact_game_id(&marker.game_id);
    let game_id = game_ids.iter().find(|game_id| {
        marker_compact == compact_game_id(game_id)
            || marker_compact
                == compact_game_id(&crate::remote_paths::install_dir_name_for_game_id(game_id))
    });
    let Some(game_id) = game_id else {
        return Err(format!(
            "Install marker at {} belongs to an unknown game",
            install_root.display()
        ));
    };
    let version = usable_installed_version(&marker.version).ok_or_else(|| {
        format!(
            "Install marker at {} does not contain a usable version",
            install_root.display()
        )
    })?;
    let launch_executable = marker
        .launch_executable
        .clone()
        .unwrap_or_else(|| default_launch_executable(game_id));
    let executable = safe_join(install_root, &launch_executable)
        .ok_or_else(|| format!("Install executable path is unsafe: {launch_executable}"))?;
    if !executable.is_file() {
        return Err(format!(
            "Installed executable is missing: {}",
            executable.display()
        ));
    }
    Ok(Some(DiscoverableInstallMarker {
        game_id: game_id.clone(),
        version,
        launch_executable,
        applied_patch_id: marker.applied_patch_id,
    }))
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
        applied_patch_id: None,
    };
    crate::managed_game_runtime::ensure_after_install(
        app,
        &source.game_id,
        install_root,
        Path::new(&launch_executable),
        manifest,
    )
    .map_err(JobError::Depot)?;
    if manifest.version == installed_version {
        write_installed_manifest(install_root, manifest)?;
    } else {
        let canonical_manifest = canonicalize_manifest_version(manifest.clone(), installed_version);
        write_installed_manifest(install_root, &canonical_manifest)?;
    }
    clear_applied_patch_manifest(install_root);
    // state.0xo is the transaction commit point and must be written last.
    write_install_marker_file(install_root, &marker)?;

    // If the manifest has explicit launch options (e.g. Vanilla / Modded), write
    // a 0xo-launch.json so the picker opens automatically on Play.
    if !manifest.launch_options.is_empty() {
        use crate::launch::{GameLaunchConfig, GameLaunchOption, GameLaunchProcess};
        let options: Vec<GameLaunchOption> = manifest
            .launch_options
            .iter()
            .enumerate()
            .map(|(idx, opt)| {
                let id = opt
                    .name
                    .to_ascii_lowercase()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '-' })
                    .collect::<String>();
                let args: Vec<String> = if opt.arguments.trim().is_empty() {
                    vec![]
                } else {
                    opt.arguments.split_whitespace().map(String::from).collect()
                };
                GameLaunchOption {
                    id: id.clone(),
                    title: opt.name.clone(),
                    description: String::new(),
                    recommended: idx == 0,
                    processes: vec![GameLaunchProcess {
                        path: opt.executable.clone(),
                        args,
                        working_directory: String::new(),
                        environment: std::collections::HashMap::new(),
                        run_as_admin: false,
                        hidden: None,
                        wait_for_exit: false,
                        delay_before_ms: 0,
                        delay_after_ms: 0,
                        optional: false,
                        role: "main".to_string(),
                    }],
                }
            })
            .collect();
        let first_id = options.first().map(|o| o.id.clone()).unwrap_or_default();
        let launch_config = GameLaunchConfig {
            schema_version: 1,
            game_id: source.game_id.clone(),
            picker_mode: "auto".to_string(),
            default_option_id: first_id,
            options,
        };
        let launch_json_path = install_root.join("0xo-launch.json");
        if let Ok(json) = serde_json::to_string_pretty(&launch_config) {
            let _ = fs::write(&launch_json_path, json);
        }
    }

    if let Err(error) = crate::platform::register_install(
        app,
        &source.game_id,
        install_root,
        installed_version,
        &launch_executable,
    ) {
        eprintln!(
            "[UPDATE] Install metadata committed; platform registration will retry later: {error}"
        );
    }

    // Determine the best exe to use as the shortcut icon and target.
    // For multi-exe games, use the first launch option's exe so the shortcut
    // always gets created. The shortcut itself doesn't pin a specific option,
    // so clicking it will trigger the picker.
    let shortcut_exe_relative = if !manifest.launch_options.is_empty() {
        manifest.launch_options[0].executable.clone()
    } else {
        launch_executable.clone()
    };

    if let Some(executable) = safe_join(install_root, &shortcut_exe_relative) {
        if executable.exists() {
            // For multi-exe games, don't pass --launch-executable so the picker appears.
            let shortcut_result = if manifest.launch_options.len() >= 2 {
                create_game_shortcut_no_exe(app, source, install_root, &executable)
            } else {
                create_game_shortcut(
                    app,
                    source,
                    install_root,
                    &executable,
                    &shortcut_exe_relative,
                )
            };
            if let Err(error) = shortcut_result {
                eprintln!(
                    "[shortcut] Install committed, but the desktop shortcut for {} could not be created: {}",
                    source.game_id, error
                );
            }
            let _ = crate::steam_integration::ensure_game_shortcut(
                app,
                &source.game_id,
                &source.game_dir_name,
                install_root,
                &shortcut_exe_relative,
                Some(&executable),
            );
        }
    }
    backup_install_state(app, install_root);
    Ok(())
}

fn backup_install_state(_app: &AppHandle, install_root: &Path) {
    use crate::job::paths::{get_launcher_backup_root, state_backup_dir};

    if let Some(backup_root) = get_launcher_backup_root() {
        let backup_dir = state_backup_dir(&backup_root, install_root);
        let marker_dir = install_root.join(INSTALL_MARKER_DIR);

        if marker_dir.exists() {
            let _ = fs::create_dir_all(&backup_dir);

            if let Ok(data) = fs::read(marker_dir.join(INSTALL_MARKER_FILE)) {
                let _ = fs::write(backup_dir.join(INSTALL_MARKER_FILE), data);
            }

            if let Ok(data) = fs::read(marker_dir.join(INSTALLED_MANIFEST_FILE)) {
                let _ = fs::write(backup_dir.join(INSTALLED_MANIFEST_FILE), data);
            }

            if let Ok(data) = fs::read(marker_dir.join(APPLIED_PATCH_MANIFEST_FILE)) {
                let _ = fs::write(backup_dir.join(APPLIED_PATCH_MANIFEST_FILE), data);
            }
        }
    }
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
        applied_patch_id: marker.applied_patch_id.clone(),
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
    recover_state_file_if_needed(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_state_file(&path)?))
}

fn applied_patch_manifest_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(APPLIED_PATCH_MANIFEST_FILE)
}

fn write_applied_patch_manifest(
    install_root: &Path,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let path = applied_patch_manifest_path(install_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_state_file(&path, manifest)
}

fn read_applied_patch_manifest(install_root: &Path) -> Result<Option<VersionManifest>, JobError> {
    let path = applied_patch_manifest_path(install_root);
    recover_state_file_if_needed(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_state_file(&path)?))
}

fn clear_applied_patch_manifest(install_root: &Path) {
    let path = applied_patch_manifest_path(install_root);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn manifest_file_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn overlay_patch_manifest(
    mut base_manifest: VersionManifest,
    patch_manifest: &VersionManifest,
) -> Result<VersionManifest, JobError> {
    let mut patch_by_path = HashMap::<String, &FileEntry>::new();
    for file in &patch_manifest.files {
        let key = manifest_file_key(&file.path);
        if key.is_empty() || patch_by_path.insert(key.clone(), file).is_some() {
            return Err(JobError::Depot(format!(
                "patch manifest contains a duplicate or empty file path: {}",
                file.path
            )));
        }
    }

    let mut base_paths = HashSet::new();
    let mut merged_files =
        Vec::with_capacity(base_manifest.files.len() + patch_manifest.files.len());
    for file in base_manifest.files {
        let key = manifest_file_key(&file.path);
        base_paths.insert(key.clone());
        merged_files.push(patch_by_path.get(&key).copied().cloned().unwrap_or(file));
    }
    for file in &patch_manifest.files {
        if !base_paths.contains(&manifest_file_key(&file.path)) {
            merged_files.push(file.clone());
        }
    }

    base_manifest.total_size = merged_files.iter().map(|file| file.size).sum();
    base_manifest.files = merged_files;
    // The combined local manifest is intentionally not represented as a
    // depot-signed manifest. Each replacement still has its own SHA-256.
    base_manifest.signature = None;
    Ok(base_manifest)
}

fn applied_patch_manifest_for_marker(
    source: &DepotSource,
    install_root: &Path,
    marker: &InstallMarker,
) -> Result<Option<VersionManifest>, JobError> {
    let Some(applied_patch_id) = marker.applied_patch_id.as_deref() else {
        return Ok(None);
    };
    let version = usable_installed_version(&marker.version).ok_or_else(|| {
        JobError::Depot("cannot verify a patch for an unknown installed version".to_string())
    })?;
    let manifest = match read_applied_patch_manifest(install_root)? {
        Some(manifest) => {
            validate_patch_manifest(source, &version, &manifest)?;
            manifest
        }
        None => load_patch_manifest(source, &version)?.ok_or_else(|| {
            JobError::Depot(format!(
                "the applied patch '{}' no longer has a manifest to verify",
                applied_patch_id
            ))
        })?,
    };
    if manifest.created_at != applied_patch_id {
        return Err(JobError::Depot(format!(
            "applied patch marker '{}' does not match manifest '{}'",
            applied_patch_id, manifest.created_at
        )));
    }
    Ok(Some(manifest))
}

fn installed_manifest_for_version(
    source: &DepotSource,
    install_root: &Path,
    marker: Option<&InstallMarker>,
    version: &str,
) -> Result<VersionManifest, JobError> {
    let marker_matches_version = marker.is_some_and(|marker| {
        usable_installed_version(&marker.version) == usable_installed_version(version)
    });
    let base_manifest = if marker_matches_version {
        match read_installed_manifest(install_root)? {
            Some(manifest) => manifest,
            None => load_manifest_for_version(source, version)?,
        }
    } else {
        load_manifest_for_version(source, version)?
    };

    if marker_matches_version {
        if let Some(marker) = marker {
            if let Some(patch_manifest) =
                applied_patch_manifest_for_marker(source, install_root, marker)?
            {
                return overlay_patch_manifest(base_manifest, &patch_manifest);
            }
        }
    }
    Ok(base_manifest)
}

fn write_state_file<T: Serialize>(path: &Path, value: &T) -> Result<(), JobError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(&long_path(parent))?;
    }
    let mut payload = serde_json::to_vec(value)?;
    transform_state_payload(&mut payload);
    let mut bytes = STATE_MAGIC.to_vec();
    bytes.extend_from_slice(&payload);
    let temporary = sibling_path(path, "next")?;
    let recovery = sibling_path(path, "previous")?;
    {
        let mut file = File::create(long_path(&temporary))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    if long_path(&recovery).exists() {
        fs::remove_file(long_path(&recovery))?;
    }
    if long_path(path).exists() {
        fs::rename(long_path(path), long_path(&recovery))?;
    }
    if let Err(error) = fs::rename(long_path(&temporary), long_path(path)) {
        if long_path(&recovery).exists() {
            let _ = fs::rename(long_path(&recovery), long_path(path));
        }
        return Err(error.into());
    }
    // Keep the recovery copy until the state is durable under its final name.
    // The marker is the commit proof used after a crash or power loss.
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(long_path(path))?
        .sync_all()?;
    if long_path(&recovery).exists() {
        fs::remove_file(long_path(&recovery))?;
    }
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

fn recover_state_file_if_needed(path: &Path) -> Result<(), JobError> {
    let recovery = sibling_path(path, "previous")?;
    if long_path(path).exists() {
        if long_path(&recovery).exists() {
            fs::remove_file(long_path(&recovery))?;
        }
        return Ok(());
    }
    if long_path(&recovery).exists() {
        fs::rename(long_path(&recovery), long_path(path))?;
    }
    Ok(())
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

        // Resumed â€” restore step to running and emit immediately so UI updates
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
    ) || (journal.kind == "patch" && journal.status == JobStatus::Committed);
    let is_real = matches!(
        journal.kind.as_str(),
        "install" | "update" | "repair" | "patch"
    );
    is_active && is_real
}

fn current_logical_transfer_done(journal: &JobJournal) -> u64 {
    let derived = journal
        .session_base_bytes
        .saturating_add(journal.bytes_done);
    journal.logical_bytes_done.max(derived)
}

fn configure_transfer_plan(journal: &mut JobJournal, session_total: u64, resumed_done: u64) {
    let previous_total = journal.logical_bytes_total.max(journal.bytes_total);
    let previous_done = current_logical_transfer_done(journal)
        .max(journal.bytes_done)
        .min(previous_total.max(session_total));
    let resumed_done = resumed_done.min(session_total);

    // A smaller remaining-work plan means verified cache/local bytes already
    // satisfy the difference. Preserve those bytes as the visible base so a
    // resume never appears to fall from, for example, 3/8 GB to 0/5 GB.
    let derived_base = previous_total.saturating_sub(session_total);
    let preserved_base = previous_done.saturating_sub(resumed_done);
    let session_base = derived_base.max(preserved_base);
    let logical_total = previous_total
        .max(session_base.saturating_add(session_total))
        .max(session_total);
    let logical_done = previous_done
        .max(session_base.saturating_add(resumed_done))
        .min(logical_total);

    journal.bytes_total = session_total;
    journal.bytes_done = resumed_done;
    journal.session_base_bytes = session_base;
    journal.logical_bytes_total = logical_total;
    journal.logical_bytes_done = logical_done;
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
        logical_bytes_done: 0,
        logical_bytes_total: bytes_total,
        session_base_bytes: 0,
        apply_bytes_done: 0,
        apply_bytes_total: 0,
        durable_bytes: 0,
        wire_bytes_done: 0,
        current_file: String::new(),
        pipeline_version: String::new(),
        transport_plans: Vec::new(),
        current_transport: DownloadTransportKind::HttpRange,
        stall_reason: String::new(),
        commit_state: "idle".to_string(),
        planned_files: Vec::new(),
        retry_count: 0,
        resumable: true,
        updated_at: now.clone(),
        steps: vec![
            step("Scan", "Find local files and detect version"),
            step("Verify", "Hash manifest-owned files"),
            step("Download packs", "Resume missing byte ranges from proxy"),
            step("Assemble files", "Rebuild files into verified temp outputs"),
            step("Finalize", "Replace only after full-file hash match"),
            step("Patch fix", "Apply hotfixes and delta patches"),
        ],
        logs: vec![JobLog {
            at: now,
            level: "info".to_string(),
            message: "Ready to start resumable update".to_string(),
        }],
        metrics: DownloadMetrics::default(),
        applied_patch_id: None,
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
    // Keep the serialized user-facing counters current throughout the job,
    // while bytes_done/bytes_total remain scoped to the active transfer plan.
    let logical_total = journal.logical_bytes_total.max(
        journal
            .session_base_bytes
            .saturating_add(journal.bytes_total),
    );
    let logical_done = current_logical_transfer_done(journal).min(logical_total);
    journal.logical_bytes_total = logical_total;
    journal.logical_bytes_done = logical_done;
    journal.updated_at = Utc::now().to_rfc3339();
}

fn publish_job_progress(
    app: &AppHandle,
    journal: &JobJournal,
    last_ui_emit: &mut Instant,
    last_journal_persist: &mut Instant,
    force_persist: bool,
) -> Result<(), JobError> {
    let emitted = if force_persist || last_journal_persist.elapsed() >= JOB_JOURNAL_PERSIST_INTERVAL
    {
        persist_and_emit(app, journal)?;
        *last_journal_persist = Instant::now();
        *last_ui_emit = Instant::now();
        true
    } else if last_ui_emit.elapsed() >= JOB_UI_EMIT_INTERVAL {
        app.emit("launcher://job", journal)?;
        *last_ui_emit = Instant::now();
        true
    } else {
        false
    };
    if emitted {
        emit_download_telemetry(app, journal);
    }
    Ok(())
}

fn emit_download_telemetry(app: &AppHandle, journal: &JobJournal) {
    if !journal_uses_transport_v3(journal) {
        return;
    }

    let now = Instant::now();
    let terminal = matches!(
        journal.status,
        JobStatus::Committed | JobStatus::Canceled | JobStatus::Failed
    );
    let rates = DOWNLOAD_TELEMETRY_RATES.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut rates) = rates.lock() else {
        return;
    };

    let state = rates
        .entry(journal.id.clone())
        .or_insert_with(|| TelemetryRateState {
            sampled_at: now,
            wire_bytes: journal.wire_bytes_done,
            apply_bytes: journal.apply_bytes_done,
            wire_rate_ewma: 0.0,
            apply_rate_ewma: 0.0,
        });
    let elapsed = now.duration_since(state.sampled_at).as_secs_f64();
    if elapsed >= 0.1 {
        let wire_sample = journal.wire_bytes_done.saturating_sub(state.wire_bytes) as f64 / elapsed;
        let apply_sample =
            journal.apply_bytes_done.saturating_sub(state.apply_bytes) as f64 / elapsed;
        const EWMA_ALPHA: f64 = 0.25;
        state.wire_rate_ewma = if state.wire_rate_ewma == 0.0 {
            wire_sample
        } else {
            state.wire_rate_ewma * (1.0 - EWMA_ALPHA) + wire_sample * EWMA_ALPHA
        };
        state.apply_rate_ewma = if state.apply_rate_ewma == 0.0 {
            apply_sample
        } else {
            state.apply_rate_ewma * (1.0 - EWMA_ALPHA) + apply_sample * EWMA_ALPHA
        };
        state.sampled_at = now;
        state.wire_bytes = journal.wire_bytes_done;
        state.apply_bytes = journal.apply_bytes_done;
    }

    let runtime = DOWNLOAD_RUNTIME_TELEMETRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|runtime| runtime.get(&journal.id).copied())
        .unwrap_or_default();
    let payload = DownloadTelemetry {
        job_id: journal.id.clone(),
        wire_bytes_done: journal.wire_bytes_done,
        apply_bytes_done: journal.apply_bytes_done,
        durable_bytes_done: journal.durable_bytes,
        wire_bytes_per_second: state.wire_rate_ewma.max(0.0).round() as u64,
        apply_bytes_per_second: state.apply_rate_ewma.max(0.0).round() as u64,
        active_connections: runtime.active_connections,
        queue_bytes: runtime.queue_bytes,
        ttfb_ms: journal.metrics.ttfb_p50_ms,
        retry_wait_ms: journal.metrics.retry_wait_ms,
        rate_limit_wait_ms: journal.metrics.rate_limit_wait_ms,
        current_transport: journal.current_transport,
        stall_reason: journal.stall_reason.clone(),
    };
    if terminal {
        rates.remove(&journal.id);
        if let Ok(mut runtime) = DOWNLOAD_RUNTIME_TELEMETRY
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            runtime.remove(&journal.id);
        }
    }
    drop(rates);
    let _ = app.emit(DOWNLOAD_TELEMETRY_EVENT, payload);
}

fn persist_and_emit(app: &AppHandle, journal: &JobJournal) -> Result<(), JobError> {
    let path = journal_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec(journal)?;
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
    use std::io::{Cursor, Error, ErrorKind, Read};

    fn manifest_file(path: &str, file_hash: &str, chunk_hash: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            size: 8,
            sha256: file_hash.to_string(),
            chunks: vec![ChunkRef {
                hash: chunk_hash.to_string(),
                file_offset: 0,
                uncompressed_size: 8,
                pack_id: format!("pack-{chunk_hash}"),
                pack_offset: 0,
                compressed_size: 8,
                compressed_sha256: format!("compressed-{chunk_hash}"),
                codec: ChunkCodec::Raw,
                encryption: None,
            }],
            delta_patches: None,
            executable: false,
            preserve: false,
        }
    }

    fn target_manifest(version: &str, changing_chunk: &str) -> VersionManifest {
        VersionManifest {
            format_version: LEGACY_FORMAT_VERSION,
            game_id: "target-selection-test".to_string(),
            version: version.to_string(),
            created_at: String::new(),
            root_label: "Target Selection Test".to_string(),
            launch_executable: None,
            launch_options: Vec::new(),
            dependencies: None,
            total_size: 16,
            files: vec![
                manifest_file("stable.bin", "stable-file", "stable-chunk"),
                manifest_file(
                    "large.pak",
                    &format!("file-{changing_chunk}"),
                    changing_chunk,
                ),
            ],
            signature: None,
        }
    }

    #[test]
    fn patch_pack_ranges_use_the_patch_url_and_an_isolated_spool_namespace() {
        let task = PackDownloadTask {
            pack_id: "pack-00001".to_string(),
            range_start: 4096,
            range_end: 8192,
            chunks: Vec::new(),
        };
        let root = Path::new(r"E:\0xoLemon store\downloading\Example\dl");
        let prefix = "patches/v1.10/packs";

        assert_eq!(
            DepotSource::pack_relative_path_in(&task.pack_id, Some("v1.10"), Some(prefix)),
            "patches/v1.10/packs/pack-00001.bin"
        );
        let base_spool = partial_range_path(root, &task);
        let patch_spool = partial_range_path_in(root, &task, Some(prefix));
        let other_patch_spool = partial_range_path_in(root, &task, Some("patches/v1.09/packs"));

        assert_ne!(patch_spool, base_spool);
        assert_ne!(patch_spool, other_patch_spool);
        assert!(patch_spool.starts_with(root.join("_transport").join("raw-spool")));
        assert_eq!(patch_spool.file_name(), base_spool.file_name());
    }

    #[test]
    fn update_transport_direction_follows_the_selected_catalog_version() {
        let catalog = Catalog {
            format_version: LEGACY_FORMAT_VERSION,
            game_id: "direction-test".to_string(),
            latest_version: Some("v1.10".to_string()),
            versions: ["v1.0", "v1.5", "v1.10"]
                .into_iter()
                .map(|version| crate::manifest::CatalogVersion {
                    version: version.to_string(),
                    manifest_path: format!("versions/{version}/manifest.json"),
                    total_size: 0,
                    file_count: 0,
                    chunk_count: 0,
                    created_at: String::new(),
                })
                .collect(),
            packs: Vec::new(),
            signature: None,
        };

        assert_eq!(
            update_transport_operation(&catalog, "v1.5", "v1.10"),
            TransportOperation::Update
        );
        assert_eq!(
            update_transport_operation(&catalog, "v1.10", "v1.0"),
            TransportOperation::Downgrade
        );
    }

    #[test]
    fn transfer_replan_preserves_visible_progress_when_remaining_work_shrinks() {
        let gib = 1024_u64 * 1024 * 1024;
        let mut journal = default_journal(
            "hello-kitty-island-adventure",
            "install",
            r"E:\0xoLemon store\common\Hello Kitty Island Adventure".to_string(),
            "not installed",
            "v2.16.2",
            8 * gib,
        );
        journal.bytes_done = 3 * gib;

        configure_transfer_plan(&mut journal, 5 * gib, 0);

        assert_eq!(journal.bytes_total, 5 * gib);
        assert_eq!(journal.bytes_done, 0);
        assert_eq!(journal.session_base_bytes, 3 * gib);
        assert_eq!(journal.logical_bytes_done, 3 * gib);
        assert_eq!(journal.logical_bytes_total, 8 * gib);
    }

    #[test]
    fn repeated_resume_keeps_original_logical_total_and_monotonic_done_bytes() {
        let gib = 1024_u64 * 1024 * 1024;
        let mut journal = default_journal(
            "large-game",
            "install",
            r"E:\0xoLemon store\common\Large Game".to_string(),
            "not installed",
            "1.0.0",
            67 * gib,
        );
        journal.bytes_done = 34 * gib;

        configure_transfer_plan(&mut journal, 33 * gib, 0);
        assert_eq!(journal.bytes_done, 0);
        assert_eq!(journal.bytes_total, 33 * gib);
        assert_eq!(journal.session_base_bytes, 34 * gib);
        assert_eq!(journal.logical_bytes_done, 34 * gib);
        assert_eq!(journal.logical_bytes_total, 67 * gib);

        journal.bytes_done = 6 * gib;
        touch(&mut journal);
        assert_eq!(journal.logical_bytes_done, 40 * gib);

        configure_transfer_plan(&mut journal, 27 * gib, 0);
        assert_eq!(journal.bytes_done, 0);
        assert_eq!(journal.bytes_total, 27 * gib);
        assert_eq!(journal.session_base_bytes, 40 * gib);
        assert_eq!(journal.logical_bytes_done, 40 * gib);
        assert_eq!(journal.logical_bytes_total, 67 * gib);
    }

    #[test]
    fn install_update_and_downgrade_plan_only_the_selected_target_manifest() {
        let v1 = target_manifest("v1.0", "chunk-v1");
        let v5 = target_manifest("v1.5", "chunk-v5");
        let v10 = target_manifest("v1.10", "chunk-v10");

        let fresh = install_changed_files(&v10);
        assert_eq!(fresh.len(), v10.files.len());
        assert_eq!(
            v10.files
                .iter()
                .flat_map(|file| file.chunks.iter())
                .map(|chunk| chunk.hash.as_str())
                .collect::<Vec<_>>(),
            vec!["stable-chunk", "chunk-v10"]
        );

        let update = changed_target_files(&v1, &v10);
        assert_eq!(update.len(), 1);
        assert_eq!(update[0].path, "large.pak");
        assert_eq!(update[0].chunks[0].hash, "chunk-v10");

        let downgrade = changed_target_files(&v10, &v5);
        assert_eq!(downgrade.len(), 1);
        assert_eq!(downgrade[0].path, "large.pak");
        assert_eq!(downgrade[0].chunks[0].hash, "chunk-v5");

        let catalog = test_catalog(&["v1.0", "v1.5", "v1.10"]);
        assert_eq!(
            resolve_target_version(&catalog, Some("v1.0".to_string())).unwrap(),
            "v1.0"
        );
        assert_eq!(resolve_target_version(&catalog, None).unwrap(), "v1.10");
    }

    #[test]
    fn signed_hf_url_refreshes_before_server_expiry() {
        let delay = signed_url_refresh_delay(
            "https://cdn.example.test/pack.bin?user_id=public&Expires=1300&Signature=secret",
            1000,
        )
        .expect("CloudFront expiry should be parsed");
        assert_eq!(delay, Duration::from_secs(240));

        assert_eq!(
            signed_url_refresh_delay(
                "https://cdn.example.test/pack.bin?Expires=1040&Signature=secret",
                1000,
            ),
            Some(Duration::ZERO)
        );
        assert!(signed_url_refresh_delay("https://cdn.example.test/pack.bin", 1000).is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_long_path_converts_standard_unc_but_preserves_extended_paths() {
        assert_eq!(
            long_path(Path::new(r"\\server\share\Game\Content\file.pak")),
            PathBuf::from(r"\\?\UNC\server\share\Game\Content\file.pak")
        );
        assert_eq!(
            long_path(Path::new(r"\\?\E:\Store\Game\file.pak")),
            PathBuf::from(r"\\?\E:\Store\Game\file.pak")
        );
    }

    #[test]
    fn missing_chunk_planner_ignores_legacy_oxidelta_entries() {
        let root = env::temp_dir().join(format!(
            "0xolemon-chunk-plan-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&root).unwrap();
        let target_chunk = ChunkRef {
            hash: "target-fastcdc-hash".to_string(),
            file_offset: 0,
            uncompressed_size: 8,
            pack_id: "pack-target".to_string(),
            pack_offset: 0,
            compressed_size: 6,
            compressed_sha256: "target-compressed".to_string(),
            codec: ChunkCodec::Raw,
            encryption: None,
        };
        let file = FileEntry {
            path: "large.pak".to_string(),
            size: 8,
            sha256: "target-file".to_string(),
            chunks: vec![target_chunk.clone()],
            delta_patches: Some(vec![crate::manifest::DeltaPatch {
                from_sha256: "base-file".to_string(),
                pack_id: "legacy-delta-pack".to_string(),
                pack_offset: 0,
                uncompressed_size: 4,
                compressed_size: 4,
                compressed_sha256: "legacy-delta".to_string(),
                codec: ChunkCodec::Raw,
                encryption: None,
            }]),
            executable: false,
            preserve: false,
        };
        let missing = plan_missing_chunks(
            &HashMap::new(),
            &root,
            &[file],
            Some(Path::new("unused-install-root")),
        )
        .unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].hash, target_chunk.hash);
        fs::remove_dir(root).unwrap();
    }

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
    fn all_404_json_candidates_remain_not_found_for_optional_resources() {
        assert!(all_remote_json_candidates_missing(5, 5));
        assert!(all_remote_json_candidates_missing(1, 1));
        assert!(!all_remote_json_candidates_missing(5, 4));
        assert!(!all_remote_json_candidates_missing(0, 0));
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
    fn stream_interrupts_are_retryable() {
        let root = env::temp_dir().join(format!(
            "0xolemon-stream-retry-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&root).unwrap();
        let partial = root.join("range.part");

        struct FailingReader {
            cursor: Cursor<Vec<u8>>,
            fail_after_reads: usize,
            reads: usize,
        }

        impl Read for FailingReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.reads >= self.fail_after_reads {
                    return Err(Error::new(ErrorKind::ConnectionReset, "connection reset"));
                }
                self.reads += 1;
                self.cursor.read(buf)
            }
        }

        let (tx, _rx) = mpsc::channel();
        let control = JobControl::default();
        let mut reader = FailingReader {
            cursor: Cursor::new(vec![1, 2, 3, 4]),
            fail_after_reads: 1,
            reads: 0,
        };

        let err =
            append_stream_to_partial(&mut reader, &partial, 0, 8, "task-1", true, &control, &tx)
                .unwrap_err();

        assert!(matches!(err, JobError::Transient(_)));
        assert!(err.retry_delay(1).is_some());

        fs::remove_file(partial_checkpoint_path(&partial)).ok();
        fs::remove_file(&partial).ok();
        fs::remove_dir(&root).ok();
    }

    #[test]
    fn early_eof_is_retryable() {
        let root = env::temp_dir().join(format!(
            "0xolemon-eof-retry-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&root).unwrap();
        let partial = root.join("range.part");
        let (tx, _rx) = mpsc::channel();
        let control = JobControl::default();
        let mut reader = Cursor::new(vec![9, 9, 9]);

        let err =
            append_stream_to_partial(&mut reader, &partial, 0, 16, "task-2", true, &control, &tx)
                .unwrap_err();

        assert!(matches!(err, JobError::Transient(_)));
        assert!(err.retry_delay(1).is_some());

        fs::remove_file(partial_checkpoint_path(&partial)).ok();
        fs::remove_file(&partial).ok();
        fs::remove_dir(&root).ok();
    }

    #[test]
    fn canonical_version_comparison_ignores_display_metadata() {
        assert!(versions_equivalent(
            "1.01.1.0.3 (Build 22800422) - Uploaded 2026-07-15",
            "1.01.1.0.3",
        ));
        assert!(versions_equivalent("v2.16.2 (Build 23704290)", "v2.16.2"));
        assert!(!versions_equivalent("v2.16.1", "v2.16.2"));
    }

    #[test]
    fn automatic_updates_require_explicit_opt_in() {
        let mut settings = crate::platform::LauncherSettings::default();
        settings.game_update_mode = crate::platform::GameUpdateMode::Manual;
        assert!(!automatic_updates_allowed_now(&settings));

        settings.game_update_mode = crate::platform::GameUpdateMode::Automatic;
        assert!(automatic_updates_allowed_now(&settings));
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
    fn patch_retry_backoff_is_scoped_to_the_patch_identity() {
        let mut attempts = HashMap::new();
        attempts.insert(
            "geometry-dash".to_string(),
            ("patch-1".to_string(), Instant::now()),
        );

        assert!(patch_attempt_is_throttled(
            &attempts,
            "geometry-dash",
            "patch-1"
        ));
        assert!(!patch_attempt_is_throttled(
            &attempts,
            "geometry-dash",
            "patch-2"
        ));
        assert!(!patch_attempt_is_throttled(
            &attempts,
            "another-game",
            "patch-1"
        ));
    }

    #[test]
    fn patch_job_uses_the_committed_marker_version_without_catalog_lookup() {
        let source = DepotSource::for_game("geometry-dash");
        let marker = InstallMarker {
            game_id: "geometry-dash".to_string(),
            version: "2.2081-hotfixable".to_string(),
            installed_at: String::new(),
            launch_executable: None,
            applied_patch_id: None,
        };

        assert_eq!(
            patch_target_version_from_marker(&source, &marker, None).unwrap(),
            "2.2081-hotfixable"
        );
        assert_eq!(
            patch_target_version_from_marker(
                &source,
                &marker,
                Some("2.2081-hotfixable".to_string()),
            )
            .unwrap(),
            "2.2081-hotfixable"
        );
    }

    #[test]
    fn patch_job_rejects_a_version_that_does_not_match_the_marker() {
        let source = DepotSource::for_game("geometry-dash");
        let marker = InstallMarker {
            game_id: "geometry-dash".to_string(),
            version: "2.2081".to_string(),
            installed_at: String::new(),
            launch_executable: None,
            applied_patch_id: None,
        };

        assert!(
            patch_target_version_from_marker(&source, &marker, Some("2.2082".to_string()),)
                .is_err()
        );
    }

    #[test]
    fn active_patch_journals_are_restored_with_other_real_jobs() {
        let journal = default_journal(
            "geometry-dash",
            "patch",
            r"E:\0xoLemon store\common\Geometry Dash".to_string(),
            "2.2081",
            "2.2081",
            0,
        );
        assert!(is_active_real_journal(&journal));
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

    #[test]
    fn installed_update_base_uses_local_manifest_when_catalog_history_is_pruned() {
        let root = env::temp_dir().join(format!(
            "0xolemon-pruned-catalog-base-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();

        let source = DepotSource::for_game("among-us");
        let marker = InstallMarker {
            game_id: source.game_id.clone(),
            version: "1.0.4.0 (Build 24213434) - Uploaded 2026-07-17".to_string(),
            installed_at: "2026-07-17T00:00:00Z".to_string(),
            launch_executable: Some("Among Us.exe".to_string()),
            applied_patch_id: None,
        };
        write_install_marker_file(&root, &marker).unwrap();

        let installed_manifest = VersionManifest {
            format_version: LEGACY_FORMAT_VERSION,
            game_id: source.game_id.clone(),
            version: "1.0.4.0".to_string(),
            created_at: "2026-07-17T00:00:00Z".to_string(),
            root_label: "Among Us".to_string(),
            launch_executable: Some("Among Us.exe".to_string()),
            launch_options: Vec::new(),
            dependencies: None,
            total_size: 0,
            files: Vec::new(),
            signature: None,
        };
        write_installed_manifest(&root, &installed_manifest).unwrap();

        let catalog = test_catalog(&["1.0.5.0", "1.0.6.0"]);
        let base = load_installed_update_base(&root, &source, &catalog)
            .expect("a valid local manifest should survive catalog history pruning")
            .expect("installed base should be available");

        assert_eq!(
            base.version,
            "1.0.4.0 (Build 24213434) - Uploaded 2026-07-17"
        );
        assert_eq!(canonical_version_label(&base.manifest.version), "1.0.4.0");
        assert!(base.source_label.contains("local manifest.0xo"));

        fs::remove_dir_all(&root).ok();
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
fn load_patch_manifest(
    source: &DepotSource,
    version: &str,
) -> Result<Option<VersionManifest>, JobError> {
    let version = usable_installed_version(version).ok_or_else(|| {
        JobError::Depot("cannot check a patch for an unknown installed version".to_string())
    })?;
    let patch_path = format!("patches/{version}/manifest.json");
    match source.load_json::<VersionManifest>(&patch_path) {
        Ok(manifest) => {
            validate_patch_manifest(source, &version, &manifest)?;
            Ok(Some(manifest))
        }
        Err(JobError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn validate_patch_manifest(
    source: &DepotSource,
    version: &str,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let manifest_game_matches = sanitize_game_id(&manifest.game_id) == source.game_id
        || compact_game_id(&manifest.game_id) == compact_game_id(&source.game_dir_name);
    if !manifest_game_matches {
        return Err(JobError::Depot(format!(
            "patch manifest belongs to '{}', not '{}'",
            manifest.game_id, source.game_id
        )));
    }
    let manifest_version = usable_installed_version(&manifest.version)
        .ok_or_else(|| JobError::Depot("patch manifest is missing a usable version".to_string()))?;
    if manifest_version != version {
        return Err(JobError::Depot(format!(
            "patch manifest version '{}' does not match '{}'",
            manifest_version, version
        )));
    }
    if manifest.created_at.trim().is_empty() {
        return Err(JobError::Depot(
            "patch manifest is missing createdAt".to_string(),
        ));
    }
    Ok(())
}

pub fn check_patch_available(game_id: &str, version: &str) -> Result<Option<String>, String> {
    let source = DepotSource::for_game(game_id);
    load_patch_manifest(&source, version)
        .map(|manifest| manifest.map(|manifest| manifest.created_at))
        .map_err(|error| error.to_string())
}

fn patch_target_version_from_marker(
    source: &DepotSource,
    marker: &InstallMarker,
    requested_version: Option<String>,
) -> Result<String, JobError> {
    if !install_marker_matches_source(&marker, source) {
        return Err(JobError::Depot(format!(
            "install metadata belongs to '{}', not '{}'",
            marker.game_id, source.game_id
        )));
    }

    let installed_version = usable_installed_version(&marker.version).ok_or_else(|| {
        JobError::Depot("cannot apply a patch to an install with an unknown version".to_string())
    })?;
    if let Some(requested_version) = requested_version {
        let requested_version = usable_installed_version(&requested_version).ok_or_else(|| {
            JobError::Depot("cannot apply a patch for an unknown target version".to_string())
        })?;
        if requested_version != installed_version {
            return Err(JobError::Depot(format!(
                "patch target '{}' does not match installed version '{}'",
                requested_version, installed_version
            )));
        }
    }

    Ok(installed_version)
}

fn resolve_patch_target_version(
    source: &DepotSource,
    install_root: &Path,
    requested_version: Option<String>,
) -> Result<String, JobError> {
    if !install_root.is_dir() {
        return Err(JobError::Depot(format!(
            "{} is not installed at '{}'",
            source.game_dir_name,
            install_root.display()
        )));
    }

    let marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo",
            source.game_dir_name
        ))
    })?;
    patch_target_version_from_marker(source, &marker, requested_version)
}

fn default_patch_journal(
    source: &DepotSource,
    install_path: String,
    target_version: &str,
) -> JobJournal {
    let mut journal = default_journal(
        &source.game_id,
        "patch",
        install_path,
        "patching",
        target_version,
        0,
    );
    journal.steps = vec![
        step(
            "Check patch",
            "Confirm the installed version before patching",
        ),
        step("Download patch", "Download version-specific patch files"),
        step("Verify patch", "Validate downloaded patch files"),
        step("Apply patch", "Replace patched game files after validation"),
        step("Record patch", "Persist the applied patch metadata"),
        step("Patch complete", "Waiting for patch completion"),
    ];
    if let Some(log) = journal.logs.first_mut() {
        log.message = "Ready to download and apply a resumable patch".to_string();
    }
    journal.pipeline_version = if transport_pipeline_v3_enabled() {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "verified-stage-v2".to_string()
    };
    journal
}

pub fn spawn_patch_job(
    app: AppHandle,
    control: Arc<JobControl>,
    install_path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, JobError> {
    ensure_no_pending_file_transaction(&app)?;
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let install_root = PathBuf::from(&install_path);
    let target_version = resolve_patch_target_version(&source, &install_root, target_version)?;

    let journal = default_patch_journal(&source, install_path, &target_version);

    spawn_patch_journal(app, control, journal)
}

pub fn resume_patch_job(
    app: AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    if journal.kind != "patch" || journal.install_path.trim().is_empty() {
        return Err(JobError::Depot(
            "journal is not a resumable patch".to_string(),
        ));
    }
    journal.status = JobStatus::Planned;
    journal.phase = "Resuming".to_string();
    journal.commit_state = "idle".to_string();
    for step in &mut journal.steps {
        if matches!(
            step.status,
            StepStatus::Running | StepStatus::Paused | StepStatus::Failed
        ) {
            step.status = StepStatus::Waiting;
        }
    }
    append_log(
        &mut journal,
        "info",
        "Resuming the existing owned patch session",
    );
    control.reset();
    spawn_patch_journal(app, control, journal)
}

fn spawn_patch_journal(
    app: AppHandle,
    control: Arc<JobControl>,
    journal: JobJournal,
) -> Result<JobJournal, JobError> {
    persist_and_emit(&app, &journal)?;
    let app_for_thread = app.clone();
    let initial = journal.clone();
    let return_journal = journal.clone();

    control.set_running(true);
    let control_for_thread = control.clone();
    thread::spawn(move || {
        let canceled_job_id = initial.id.clone();
        // Small startup delay: give the WebView time to mount and subscribe to
        // `launcher://job` before the first status event fires. Without this,
        // very fast patch jobs can complete before the Downloads tab is ready
        // to display them.
        thread::sleep(Duration::from_millis(300));
        let result =
            run_real_patch_job(&app_for_thread, control_for_thread.clone(), initial.clone());
        let canceled = matches!(&result, Err(JobError::Canceled));
        control_for_thread.set_running(false);
        if canceled {
            let install_root = PathBuf::from(&initial.install_path);
            let source = DepotSource::for_game(&initial.game_id);
            let downloading_root = downloading_dir_for_install(&install_root, &source);
            let _ =
                VerifiedStageSession::cleanup_owned_session(&downloading_root, &canceled_job_id);
            let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            return;
        }
        match result {
            Ok(_) => {
                // Keep the terminal patch journal so the frontend can observe
                // `committed`, refresh GameInstallState, and read applied_patch_id.
            }
            Err(e) => {
                let mut errored = read_latest_journal(&app_for_thread)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| initial.clone());
                errored.status = JobStatus::Failed;
                errored.phase = "Failed".to_string();
                mark_running_step_failed(&mut errored);
                append_log(&mut errored, "error", &format!("Job failed: {}", e));
                let _ = persist_and_emit(&app_for_thread, &errored);
            }
        }
    });

    Ok(return_journal)
}

fn run_real_patch_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(&journal.game_id);
    let install_root = PathBuf::from(&journal.install_path);
    set_step_running(app, &mut journal, 0, JobStatus::Running, "Check patch")?;
    resolve_patch_target_version(&source, &install_root, Some(journal.to_version.clone()))?;
    complete_step(app, &mut journal, 0)?;

    let to_version = journal.to_version.clone();
    try_apply_patch_fix(
        app,
        &mut journal,
        &source,
        &install_root,
        &to_version,
        &control,
        true,
    )?;
    Ok(journal)
}

fn patch_transfer_bytes(manifest: &VersionManifest) -> u64 {
    manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter())
        .map(|chunk| chunk.compressed_size)
        .sum()
}

fn try_apply_patch_fix(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    install_root: &Path,
    version: &str,
    control: &Arc<JobControl>,
    manifest_errors_are_fatal: bool,
) -> Result<(), JobError> {
    if journal_uses_transport_v3(journal)
        || matches!(
            journal.pipeline_version.as_str(),
            "verified-stage-v2" | "sequential-stage-v1+verified-patch-v2"
        )
    {
        try_apply_patch_fix_verified(
            app,
            journal,
            source,
            install_root,
            version,
            control,
            manifest_errors_are_fatal,
        )
    } else {
        try_apply_patch_fix_legacy(
            app,
            journal,
            source,
            install_root,
            version,
            control,
            manifest_errors_are_fatal,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn try_apply_patch_fix_verified(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    install_root: &Path,
    version: &str,
    control: &Arc<JobControl>,
    manifest_errors_are_fatal: bool,
) -> Result<(), JobError> {
    let standalone_patch = journal.kind == "patch";
    let patch_step_index = if standalone_patch { 1 } else { 5 };
    let patch_manifest: VersionManifest = match load_patch_manifest(source, version) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            append_log(
                journal,
                "info",
                &format!("[Patch fix] No patch for {version} - skipping"),
            );
            for step in journal.steps.iter_mut().skip(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "No patch required for this version".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = if standalone_patch {
                "No patch required".to_string()
            } else {
                "Committed".to_string()
            };
            journal.status = JobStatus::Committed;
            journal.commit_state = "committed".to_string();
            persist_and_emit(app, journal)?;
            return Ok(());
        }
        Err(error) => {
            if manifest_errors_are_fatal {
                if let Some(step) = journal.steps.get_mut(patch_step_index) {
                    step.status = StepStatus::Failed;
                    step.detail = "Patch manifest unavailable".to_string();
                }
                journal.phase = "Failed".to_string();
                journal.status = JobStatus::Failed;
                append_log(
                    journal,
                    "error",
                    &format!("[Patch fix] Could not load patch manifest: {error}"),
                );
                persist_and_emit(app, journal)?;
                return Err(error);
            }
            append_log(
                journal,
                "warning",
                &format!("[Patch fix] Could not load patch manifest: {error} - skipping"),
            );
            if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "Skipped (patch manifest unavailable)".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = "Committed".to_string();
            journal.status = JobStatus::Committed;
            persist_and_emit(app, journal)?;
            return Ok(());
        }
    };

    let marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo before patching",
            source.game_dir_name
        ))
    })?;
    patch_target_version_from_marker(source, &marker, Some(version.to_string()))?;
    let patch_id = patch_manifest.created_at.clone();
    let downloading_root = downloading_dir_for_install(install_root, source);
    fs::create_dir_all(&downloading_root)?;
    let mut session = VerifiedStageSession::prepare_with_commit_proof(
        &downloading_root,
        journal,
        version,
        version,
        install_root,
        &patch_manifest.files,
        TransactionCommitProof::AppliedPatchId(patch_id.clone()),
    )?;
    if session.recover(install_root, version)? == RecoveryOutcome::AlreadyCommitted {
        for step in journal.steps.iter_mut().skip(patch_step_index) {
            step.status = StepStatus::Completed;
            step.progress = 1.0;
        }
        journal.applied_patch_id = Some(patch_id);
        journal.status = JobStatus::Committed;
        journal.phase = "Patch applied".to_string();
        journal.commit_state = "committed".to_string();
        journal.overall_progress = 1.0;
        if let Err(error) = session.cleanup_session_files() {
            append_log(
                journal,
                "warning",
                &format!("Committed patch cleanup remains pending: {error}"),
            );
        }
        persist_and_emit(app, journal)?;
        return Ok(());
    }

    let (mut local_sources, discovered_count) =
        discover_local_chunks(install_root, &patch_manifest.files, control)?;
    if discovered_count > 0 {
        append_log(
            journal,
            "info",
            &format!(
                "[Patch fix] Reusing {discovered_count} verified chunk(s) from installed files"
            ),
        );
    }
    let missing_chunks = plan_missing_chunks(
        &local_sources,
        session.cache_root(),
        &patch_manifest.files,
        None,
    )?;
    let use_bounded_patch_transport = journal_uses_transport_v3(journal);
    let patch_pack_prefix = format!("patches/{version}/packs");
    let prepared_transports = if use_bounded_patch_transport {
        Some(source.prepare_pack_transports(
            session.cache_root(),
            &missing_chunks,
            Some(version),
            TransportOperation::Patch,
            &[],
        )?)
    } else {
        None
    };
    let transfer_total = prepared_transports
        .as_ref()
        .map(|prepared| prepared.transfer_bytes)
        .unwrap_or_else(|| patch_transfer_bytes_for_chunks(&missing_chunks));
    let resumed_in_flight = if use_bounded_patch_transport {
        existing_partial_task_progress_in(
            session.cache_root(),
            &missing_chunks,
            Some(&patch_pack_prefix),
        )
    } else {
        HashMap::new()
    };
    let resumed_bytes = resumed_in_flight
        .values()
        .copied()
        .sum::<u64>()
        .min(transfer_total);
    configure_transfer_plan(journal, transfer_total, resumed_bytes);
    configure_download_metrics(journal, &missing_chunks, false);
    journal.metrics.pipeline = if journal_uses_transport_v3(journal) {
        TRANSPORT_PIPELINE_V3.to_string()
    } else {
        "verified-patch-stage-v2".to_string()
    };
    journal.apply_bytes_total = patch_manifest.files.iter().map(|file| file.size).sum();
    journal.apply_bytes_done = session.total_durable_bytes(&patch_manifest.files);
    journal.durable_bytes = journal.apply_bytes_done;
    journal.commit_state = "staging".to_string();
    if let Some(prepared) = prepared_transports.as_ref() {
        journal.transport_plans = prepared.plans.clone();
        journal.metrics.overfetch_bytes = prepared
            .plans
            .iter()
            .map(|plan| plan.estimated_overfetch)
            .sum();
    }
    session.reconcile_chunk_references(&patch_manifest.files)?;
    set_step_running(
        app,
        journal,
        patch_step_index,
        JobStatus::Downloading,
        "Download and stage patch",
    )?;

    let mut downloaded = 0_u64;
    let mut in_flight = resumed_in_flight;
    let queue_budget = crate::platform::current_settings()
        .download_queue_mb
        .max(8)
        .saturating_mul(1024 * 1024);
    let mut last_ui_emit = Instant::now();
    let mut last_journal_persist = Instant::now();
    for (file_index, file) in patch_manifest.files.iter().enumerate() {
        wait_for_control(app, control, journal, patch_step_index)?;
        journal.current_file = file.path.clone();
        if let Some(step) = journal.steps.get_mut(patch_step_index) {
            step.detail = format!(
                "Writing patch file {}/{}: {}",
                file_index.saturating_add(1),
                patch_manifest.files.len(),
                file.path
            );
        }
        let mut writer = session.open_writer(file)?;
        let mut checkpointed_hashes = Vec::<String>::new();
        while writer.next_chunk() < file.chunks.len() {
            wait_for_control(app, control, journal, patch_step_index)?;
            let batch_start = writer.next_chunk();
            let batch_end = if use_bounded_patch_transport {
                sequential_batch_end(
                    file,
                    batch_start,
                    &local_sources,
                    session.cache_root(),
                    queue_budget,
                )?
            } else {
                batch_start.saturating_add(1).min(file.chunks.len())
            };

            if use_bounded_patch_transport {
                let mut batch = file.clone();
                batch.chunks = file.chunks[batch_start..batch_end].to_vec();
                let batch_missing =
                    plan_missing_chunks(&local_sources, session.cache_root(), &[batch], None)?;
                if !batch_missing.is_empty() {
                    let durable_snapshot = journal.apply_bytes_done;
                    let mut progress_callback = |progress: DownloadProgress| {
                        if progress.clear_in_flight {
                            in_flight.remove(&progress.task_id);
                        } else {
                            in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
                        }
                        downloaded = downloaded.saturating_add(progress.committed_bytes);
                        wait_for_control(app, control, journal, patch_step_index)?;
                        let active_bytes = in_flight.values().copied().sum::<u64>();
                        observe_download_progress(journal, &progress, active_bytes);
                        journal.bytes_done = downloaded
                            .saturating_add(active_bytes)
                            .min(journal.bytes_total);
                        journal.steps[patch_step_index].progress = streamed_journal_progress(
                            journal,
                            durable_snapshot,
                            journal.apply_bytes_total,
                        );
                        journal.steps[patch_step_index].retry_count = journal.steps
                            [patch_step_index]
                            .retry_count
                            .max(progress.retry_count);
                        journal.steps[patch_step_index].detail =
                            format!("Downloading patch data for {}", file.path);
                        journal.overall_progress = overall_progress(
                            patch_step_index,
                            journal.steps[patch_step_index].progress,
                        );
                        touch(journal);
                        publish_job_progress(
                            app,
                            journal,
                            &mut last_ui_emit,
                            &mut last_journal_persist,
                            false,
                        )
                    };
                    source.download_chunks_to_store_parallel_at_prefix(
                        session.cache_root(),
                        &batch_missing,
                        Some(version),
                        Some(&patch_pack_prefix),
                        Arc::clone(control),
                        &mut progress_callback,
                    )?;
                }
            }

            while writer.next_chunk() < batch_end {
                let chunk = &file.chunks[writer.next_chunk()];
                let cache_path = staged_chunk_path_from(session.cache_root(), &chunk.hash);
                if !use_bounded_patch_transport
                    && !local_sources.contains_key(&chunk.hash)
                    && !compressed_chunk_file_valid(&cache_path, chunk)?
                {
                    let pack_relative = format!("patches/{version}/packs/{}.bin", chunk.pack_id);
                    let task_id = format!("patch-{version}-{}", chunk.hash);
                    let partial_path = session
                        .cache_root()
                        .join(format!("{}.range.partial", chunk.hash));
                    let transport = fetch_patch_pack_span_with_journal_progress(
                        app,
                        journal,
                        source,
                        chunk,
                        pack_relative,
                        task_id,
                        partial_path,
                        control,
                        patch_step_index,
                        &mut downloaded,
                        &mut in_flight,
                    )?;
                    verify_compressed_chunk_bytes(chunk, &transport)?;
                    write_chunk_file(&cache_path, &transport)?;
                }
                let plain = read_chunk_bytes(chunk, &local_sources, session.cache_root())?;
                journal.metrics.disk_read_bytes = journal
                    .metrics
                    .disk_read_bytes
                    .saturating_add(plain.len() as u64);
                writer.append(chunk, &plain)?;
                checkpointed_hashes.push(chunk.hash.clone());
                if writer.checkpoint_due() {
                    session.checkpoint_writer(&mut writer, false)?;
                    session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
                    journal.apply_bytes_done = session.total_durable_bytes(&patch_manifest.files);
                    journal.durable_bytes = journal.apply_bytes_done;
                    journal.steps[patch_step_index].progress = streamed_journal_progress(
                        journal,
                        journal.apply_bytes_done,
                        journal.apply_bytes_total,
                    );
                    journal.overall_progress = overall_progress(
                        patch_step_index,
                        journal.steps[patch_step_index].progress,
                    );
                    persist_and_emit(app, journal)?;
                }
            }

            if writer.next_chunk() < file.chunks.len() {
                session.checkpoint_writer(&mut writer, false)?;
                session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
                journal.apply_bytes_done = session.total_durable_bytes(&patch_manifest.files);
                journal.durable_bytes = journal.apply_bytes_done;
            }
        }
        session.finish_writer(&mut writer, file)?;
        session.release_checkpointed_chunks(&mut checkpointed_hashes)?;
        journal.apply_bytes_done = session.total_durable_bytes(&patch_manifest.files);
        journal.durable_bytes = journal.apply_bytes_done;
        merge_verified_writer_metrics(journal, &writer);
        drop(writer);
        let stage = session.stage_path(file);
        for chunk in &file.chunks {
            local_sources.insert(
                chunk.hash.clone(),
                LocalChunkSource {
                    path: stage.clone(),
                    offset: chunk.file_offset,
                    size: chunk.uncompressed_size,
                },
            );
        }
        journal.steps[patch_step_index].progress =
            streamed_journal_progress(journal, journal.apply_bytes_done, journal.apply_bytes_total);
        journal.overall_progress =
            overall_progress(patch_step_index, journal.steps[patch_step_index].progress);
        persist_and_emit(app, journal)?;
    }
    journal.current_file.clear();

    if standalone_patch {
        complete_step(app, journal, 1)?;
        set_step_running(app, journal, 2, JobStatus::Running, "Verify patch")?;
        complete_step(app, journal, 2)?;
        set_step_running(app, journal, 3, JobStatus::Assembling, "Commit patch")?;
    } else {
        journal.status = JobStatus::Assembling;
    }
    commit_verified_manifest_files(
        app,
        journal,
        &mut session,
        install_root,
        &patch_manifest.files,
        if standalone_patch {
            3
        } else {
            patch_step_index
        },
    )?;
    if standalone_patch {
        complete_step(app, journal, 3)?;
        set_step_running(app, journal, 4, JobStatus::Running, "Record patch")?;
    }

    journal.commit_state = "metadata".to_string();
    session.backup_metadata_file(
        install_root,
        &format!("{INSTALL_MARKER_DIR}/{APPLIED_PATCH_MANIFEST_FILE}"),
    )?;
    session.backup_metadata_file(
        install_root,
        &format!("{INSTALL_MARKER_DIR}/{INSTALL_MARKER_FILE}"),
    )?;
    write_applied_patch_manifest(install_root, &patch_manifest)?;
    let mut committed_marker = marker;
    committed_marker.applied_patch_id = Some(patch_id.clone());
    write_install_marker_file(install_root, &committed_marker)?;
    session.mark_marker_committed()?;
    finish_committed_transaction_cleanup(app, journal, &mut session, install_root, "Patch")?;
    if standalone_patch {
        complete_step(app, journal, 4)?;
        set_step_running(app, journal, 5, JobStatus::Running, "Finalize patch")?;
        complete_step(app, journal, 5)?;
    } else if let Some(step) = journal.steps.get_mut(patch_step_index) {
        step.status = StepStatus::Completed;
        step.progress = 1.0;
        step.detail = format!(
            "Applied {} verified patch file(s)",
            patch_manifest.files.len()
        );
    }
    journal.applied_patch_id = Some(patch_id);
    journal.apply_bytes_done = journal.apply_bytes_total;
    journal.durable_bytes = journal.apply_bytes_done;
    journal.bytes_done = journal.bytes_total;
    journal.overall_progress = 1.0;
    journal.phase = if standalone_patch {
        "Patch applied".to_string()
    } else {
        "Committed".to_string()
    };
    journal.status = JobStatus::Committed;
    journal.commit_state = "committed".to_string();
    append_log(
        journal,
        "info",
        &format!(
            "[Patch fix] Applied {} verified file(s)",
            patch_manifest.files.len()
        ),
    );
    persist_and_emit(app, journal)
}

fn patch_transfer_bytes_for_chunks(chunks: &[ChunkRef]) -> u64 {
    chunks.iter().map(|chunk| chunk.compressed_size).sum()
}

fn try_apply_patch_fix_legacy(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    install_root: &Path,
    version: &str,
    control: &Arc<JobControl>,
    manifest_errors_are_fatal: bool,
) -> Result<(), JobError> {
    let standalone_patch = journal.kind == "patch";
    let patch_step_index = if standalone_patch { 1 } else { 5 };
    let patch_manifest: VersionManifest = match load_patch_manifest(source, version) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            append_log(
                journal,
                "info",
                &format!("[Patch fix] No patch for {} - skipping", version),
            );
            if standalone_patch {
                for step in journal.steps.iter_mut().skip(patch_step_index) {
                    step.status = StepStatus::Completed;
                    step.detail = "No patch required for this version".to_string();
                    step.progress = 1.0;
                }
            } else if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "No patch required for this version".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = if standalone_patch {
                "No patch required".to_string()
            } else {
                "Committed".to_string()
            };
            journal.status = JobStatus::Committed;
            persist_and_emit(app, journal)?;
            return Ok(());
        }
        Err(err) => {
            if manifest_errors_are_fatal {
                append_log(
                    journal,
                    "error",
                    &format!("[Patch fix] Could not load patch manifest: {err}"),
                );
                if let Some(step) = journal.steps.get_mut(patch_step_index) {
                    step.status = StepStatus::Failed;
                    step.detail = "Patch manifest unavailable".to_string();
                }
                journal.phase = "Failed".to_string();
                journal.status = JobStatus::Failed;
                persist_and_emit(app, journal)?;
                return Err(err);
            }

            append_log(
                journal,
                "warning",
                &format!("[Patch fix] Could not load patch manifest: {err} - skipping"),
            );
            if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "Skipped (patch manifest unavailable)".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = "Committed".to_string();
            journal.status = JobStatus::Committed;
            persist_and_emit(app, journal)?;
            return Ok(());
        }
    };

    let file_count = patch_manifest.files.len();
    append_log(
        journal,
        "info",
        &format!(
            "[Patch fix] Found patch for {} ({} file(s))",
            version, file_count
        ),
    );

    let patch_chunks = patch_manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter().cloned())
        .collect::<Vec<_>>();
    if standalone_patch {
        let transfer_total = patch_transfer_bytes(&patch_manifest);
        configure_transfer_plan(journal, transfer_total, 0);
        configure_download_metrics(journal, &patch_chunks, true);
        journal.metrics.pipeline = "patch-direct-v1".to_string();
        set_step_running(
            app,
            journal,
            patch_step_index,
            JobStatus::Downloading,
            "Download patch",
        )?;
        if let Some(step) = journal.steps.get_mut(patch_step_index) {
            step.detail = format!("Downloading {} patch file(s)...", file_count);
        }
        persist_and_emit(app, journal)?;
    } else {
        journal.status = JobStatus::Running;
        journal.phase = "Patch fix".to_string();
        if let Some(step) = journal.steps.get_mut(patch_step_index) {
            step.status = StepStatus::Running;
            step.progress = 0.0;
            step.detail = format!("Applying {} fix file(s)...", file_count);
        }
        journal.overall_progress = overall_progress(patch_step_index, 0.0);
        persist_and_emit(app, journal)?;
    }

    let patch_stage = install_root.join(INSTALL_MARKER_DIR).join("patch_stage");
    fs::create_dir_all(&patch_stage)?;
    let (fallback_progress_tx, _fallback_progress_rx) =
        mpsc::channel::<Result<DownloadProgress, JobError>>();
    let mut downloaded = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();

    for (idx, file) in patch_manifest.files.iter().enumerate() {
        if standalone_patch {
            wait_for_control(app, control, journal, patch_step_index)?;
        } else if control.is_canceled() {
            return Err(JobError::Canceled);
        }
        let target_path = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe patch path: {}", file.path)))?;
        append_log(
            journal,
            "info",
            &format!("[Patch fix] ({}/{}) {}", idx + 1, file_count, file.path),
        );
        let mut assembled = Vec::with_capacity(file.size as usize);
        for chunk in &file.chunks {
            let pack_relative = format!("patches/{}/packs/{}.bin", version, chunk.pack_id);
            let task_id = format!("patch-{}-{}", version, chunk.hash);
            let partial_path = patch_stage.join(format!("{}.partial", chunk.hash));
            let transport = if standalone_patch {
                fetch_patch_pack_span_with_journal_progress(
                    app,
                    journal,
                    source,
                    chunk,
                    pack_relative,
                    task_id,
                    partial_path,
                    control,
                    patch_step_index,
                    &mut downloaded,
                    &mut in_flight,
                )?
            } else {
                source.fetch_pack_span_with_progress(
                    &chunk.pack_id,
                    chunk.pack_offset,
                    chunk.pack_offset + chunk.compressed_size,
                    &pack_relative,
                    &task_id,
                    &partial_path,
                    control,
                    &fallback_progress_tx,
                )?
            };
            verify_compressed_chunk_bytes(chunk, &transport)?;
            let compressed = decode_transport_chunk(chunk, &transport)?;
            let plain = decode_chunk_payload(chunk, &compressed)?;
            verify_chunk_bytes(chunk, &plain)?;
            assembled.extend_from_slice(&plain);
        }
        if !sha256_bytes(&assembled).eq_ignore_ascii_case(&file.sha256) {
            return Err(JobError::Depot(format!(
                "patch file hash mismatch: {}",
                file.path
            )));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let target_path = long_path(&target_path);
        let tmp_path = target_path.with_extension("patch_tmp");
        fs::write(&tmp_path, &assembled)?;
        fs::rename(&tmp_path, &target_path)?;
        if !standalone_patch {
            let step_progress = progress_fraction(idx + 1, file_count);
            if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.progress = step_progress;
            }
            journal.overall_progress = overall_progress(patch_step_index, step_progress);
            persist_and_emit(app, journal)?;
        }
    }
    for file in &patch_manifest.files {
        for chunk in &file.chunks {
            let partial_path = patch_stage.join(format!("{}.partial", chunk.hash));
            let _ = fs::remove_file(&partial_path);
        }
    }
    let _ = fs::remove_dir(&patch_stage);

    if standalone_patch {
        complete_step(app, journal, 1)?;
        set_step_running(app, journal, 2, JobStatus::Running, "Verify patch")?;
        if let Some(step) = journal.steps.get_mut(2) {
            step.detail = format!("Validated {} patch file(s)", file_count);
        }
        complete_step(app, journal, 2)?;
        set_step_running(app, journal, 3, JobStatus::Running, "Apply patch")?;
        if let Some(step) = journal.steps.get_mut(3) {
            step.detail = format!("Applied {} patch file(s)", file_count);
        }
        complete_step(app, journal, 3)?;
        set_step_running(app, journal, 4, JobStatus::Running, "Record patch")?;
    }

    write_applied_patch_manifest(install_root, &patch_manifest)?;
    let mut marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo after patching",
            source.game_dir_name
        ))
    })?;
    patch_target_version_from_marker(source, &marker, Some(version.to_string()))?;
    marker.applied_patch_id = Some(patch_manifest.created_at.clone());
    write_install_marker_file(install_root, &marker)?;

    append_log(
        journal,
        "info",
        &format!("[Patch fix] Applied {} file(s)", file_count),
    );
    if standalone_patch {
        complete_step(app, journal, 4)?;
        set_step_running(app, journal, 5, JobStatus::Running, "Finalize patch")?;
        if let Some(step) = journal.steps.get_mut(5) {
            step.detail = format!("Patch applied to {}", version);
        }
        complete_step(app, journal, 5)?;
        journal.bytes_done = journal.bytes_total;
        // Surface the applied patch ID on the journal so the frontend can
        // immediately clear the pending-patch badge without a round-trip.
        journal.applied_patch_id = Some(patch_manifest.created_at.clone());
    } else if let Some(step) = journal.steps.get_mut(patch_step_index) {
        step.status = StepStatus::Completed;
        step.progress = 1.0;
        step.detail = format!("Applied {} fix file(s)", file_count);
    }
    journal.overall_progress = 1.0;
    journal.phase = if standalone_patch {
        "Patch applied".to_string()
    } else {
        "Committed".to_string()
    };
    journal.status = JobStatus::Committed;
    persist_and_emit(app, journal)?;
    Ok(())
}

fn record_patch_download_progress(
    app: &AppHandle,
    journal: &mut JobJournal,
    control: &JobControl,
    step_index: usize,
    downloaded: &mut u64,
    in_flight: &mut HashMap<String, u64>,
    progress: DownloadProgress,
) -> Result<(), JobError> {
    wait_for_control(app, control, journal, step_index)?;
    if progress.clear_in_flight {
        in_flight.remove(&progress.task_id);
    } else {
        in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
    }
    *downloaded = downloaded.saturating_add(progress.committed_bytes);
    let in_flight_bytes = in_flight.values().copied().sum::<u64>();
    observe_download_progress(journal, &progress, in_flight_bytes);
    journal.bytes_done = downloaded
        .saturating_add(in_flight_bytes)
        .min(journal.bytes_total);
    journal.steps[step_index].progress = byte_progress(journal.bytes_done, journal.bytes_total);
    journal.steps[step_index].retry_count = journal.steps[step_index]
        .retry_count
        .max(progress.retry_count);
    journal.steps[step_index].detail = format!(
        "Downloading patch files ({} / {})",
        human_bytes(journal.bytes_done),
        human_bytes(journal.bytes_total)
    );
    journal.overall_progress = overall_progress(step_index, journal.steps[step_index].progress);
    touch(journal);
    persist_and_emit(app, journal)
}

#[allow(clippy::too_many_arguments)]
fn fetch_patch_pack_span_with_journal_progress(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    chunk: &ChunkRef,
    pack_relative: String,
    task_id: String,
    partial_path: PathBuf,
    control: &Arc<JobControl>,
    step_index: usize,
    downloaded: &mut u64,
    in_flight: &mut HashMap<String, u64>,
) -> Result<Vec<u8>, JobError> {
    let (progress_tx, progress_rx) = mpsc::channel::<Result<DownloadProgress, JobError>>();
    let source = source.clone();
    let pack_id = chunk.pack_id.clone();
    let start = chunk.pack_offset;
    let end_exclusive = chunk.pack_offset + chunk.compressed_size;
    let expected_size = chunk.compressed_size;
    let worker_task_id = task_id.clone();
    let worker_control = Arc::clone(control);
    let worker = thread::spawn(move || {
        source.fetch_pack_span_with_progress(
            &pack_id,
            start,
            end_exclusive,
            &pack_relative,
            &worker_task_id,
            &partial_path,
            &worker_control,
            &progress_tx,
        )
    });

    let mut interruption = None;
    loop {
        match progress_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(progress)) => record_patch_download_progress(
                app, journal, control, step_index, downloaded, in_flight, progress,
            )?,
            Ok(Err(error)) => {
                control.cancel();
                interruption = Some(error);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Err(error) = wait_for_control(app, control, journal, step_index) {
                    interruption = Some(error);
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let worker_result = worker
        .join()
        .map_err(|_| JobError::Depot("patch download worker panicked".to_string()))?;
    if let Some(error) = interruption {
        let _ = worker_result;
        return Err(error);
    }
    let transport = worker_result?;
    record_patch_download_progress(
        app,
        journal,
        control,
        step_index,
        downloaded,
        in_flight,
        DownloadProgress {
            task_id,
            committed_bytes: expected_size,
            wire_bytes_delta: expected_size,
            in_flight_bytes: 0,
            clear_in_flight: true,
            retry_count: 0,
            rate_bytes_per_second: 0,
            retry_wait_ms: 0,
            rate_limit_wait_ms: 0,
            transport: DownloadTransportKind::HttpRange,
            stall_reason: String::new(),
            active_connections: 0,
            queue_bytes: 0,
        },
    )?;
    Ok(transport)
}
