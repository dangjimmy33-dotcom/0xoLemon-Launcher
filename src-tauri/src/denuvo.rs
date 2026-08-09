use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hex::encode as hex_encode;
use once_cell::sync::Lazy;
use rand_core::{OsRng, RngCore};
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{discord_auth, job};

const GAME_ID: &str = "ea-sports-fc-26";
const GAME_EXECUTABLE: &str = "FC26.exe";
const GAME_CONFIG: &str = "anadius.cfg";
const BACKEND_BASE_URL: &str = "https://zeroxolemon-launcher.onrender.com";
const BACKEND_TENANT: &str = "0xolemon";
const ACTIVATION_EVENT: &str = "launcher://offline-activation";
const TICKET_TIMEOUT: Duration = Duration::from_secs(120);
const HTTP_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const PACKAGE_HTTP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PACKAGE_FILES: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineActivationStep {
    pub id: String,
    pub status: String,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineActivationState {
    pub game_id: String,
    pub status: String,
    pub phase: String,
    pub progress: f32,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub cancellable: bool,
    pub can_resume: bool,
    pub message_code: Option<String>,
    pub error_code: Option<String>,
    pub request_id: Option<String>,
    pub capacity: u32,
    pub available: u32,
    pub in_use: u32,
    pub reservations: u32,
    pub next_available_at: Option<String>,
    pub server_time: Option<String>,
    pub account_eligible: Option<bool>,
    pub next_eligible_at: Option<String>,
    pub backend_ready: bool,
    pub backend_missing_configuration: Vec<String>,
    pub package_version: Option<String>,
    pub package_sha256: Option<String>,
    pub steps: Vec<OfflineActivationStep>,
}

impl Default for OfflineActivationState {
    fn default() -> Self {
        Self {
            game_id: GAME_ID.to_string(),
            status: "idle".to_string(),
            phase: "idle".to_string(),
            progress: 0.0,
            bytes_downloaded: 0,
            total_bytes: 0,
            cancellable: false,
            can_resume: false,
            message_code: None,
            error_code: None,
            request_id: None,
            capacity: 5,
            available: 0,
            in_use: 0,
            reservations: 0,
            next_available_at: None,
            server_time: None,
            account_eligible: None,
            next_eligible_at: None,
            backend_ready: false,
            backend_missing_configuration: Vec::new(),
            package_version: None,
            package_sha256: None,
            steps: default_steps(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationJournal {
    schema_version: u32,
    request_id: String,
    game_id: String,
    game_dir: String,
    status: String,
    phase: String,
    package_version: Option<String>,
    ticket_hash: Option<String>,
    ticket_file: Option<String>,
    started_at_ms: u64,
    updated_at_ms: u64,
    error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageFile {
    path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageMetadata {
    version: Option<String>,
    url: Option<String>,
    size_bytes: u64,
    sha256: Option<String>,
    #[serde(default)]
    files: Vec<PackageFile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ServiceReadiness {
    ready: bool,
    #[serde(default)]
    missing_configuration: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendStatus {
    capacity: u32,
    available: u32,
    in_use: u32,
    reservations: u32,
    next_available_at: Option<String>,
    server_time: String,
    readiness: ServiceReadiness,
    package: PackageMetadata,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageAccess {
    #[serde(default)]
    archive_password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountEligibility {
    eligible: bool,
    next_eligible_at: Option<String>,
    server_time: String,
    package_access: PackageAccess,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendErrorBody {
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivationRequest<'a> {
    ticket: &'a str,
    request_id: &'a str,
    launcher_version: &'a str,
}

#[derive(Debug, Clone)]
struct ActivationFailure {
    code: String,
    message: String,
}

impl ActivationFailure {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

type ActivationResult<T> = Result<T, ActivationFailure>;

struct ActivationRuntime {
    running: AtomicBool,
    cancel_requested: AtomicBool,
    state: Mutex<OfflineActivationState>,
}

static RUNTIME: Lazy<ActivationRuntime> = Lazy::new(|| ActivationRuntime {
    running: AtomicBool::new(false),
    cancel_requested: AtomicBool::new(false),
    state: Mutex::new(OfflineActivationState::default()),
});

fn default_steps() -> Vec<OfflineActivationStep> {
    [
        "validate", "package", "ticket", "request", "apply", "launch",
    ]
    .into_iter()
    .map(|id| OfflineActivationStep {
        id: id.to_string(),
        status: "pending".to_string(),
        progress: 0.0,
    })
    .collect()
}

fn phase_step(phase: &str) -> usize {
    match phase {
        "validating" => 0,
        "downloadingPackage" | "verifyingPackage" | "extractingPackage" | "installingPackage" => 1,
        "launchingTicketHelper" | "waitingForTicket" => 2,
        "requestingActivation" => 3,
        "applyingToken" => 4,
        "launchingGame" | "completed" => 5,
        _ => 0,
    }
}

fn phase_progress(phase: &str) -> f32 {
    match phase {
        "validating" => 0.03,
        "downloadingPackage" => 0.10,
        "verifyingPackage" => 0.42,
        "extractingPackage" => 0.48,
        "installingPackage" => 0.62,
        "launchingTicketHelper" => 0.68,
        "waitingForTicket" => 0.72,
        "requestingActivation" => 0.82,
        "applyingToken" => 0.93,
        "launchingGame" => 0.98,
        "completed" => 1.0,
        _ => 0.0,
    }
}

fn failure_can_resume(code: Option<&str>) -> bool {
    !matches!(
        code,
        Some(
            "CANCELED"
                | "EA_REJECTED"
                | "GAME_FILES_INVALID"
                | "GAME_FILES_MISSING"
                | "GAME_NOT_INSTALLED"
                | "GAME_PATH_CHANGED"
                | "GAME_PATH_INVALID"
                | "INVALID_PATH"
                | "UNSUPPORTED_GAME"
        )
    )
}

fn update_state(
    app: &AppHandle,
    update: impl FnOnce(&mut OfflineActivationState),
) -> OfflineActivationState {
    let state = {
        let mut guard = RUNTIME
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut guard);
        guard.clone()
    };
    let _ = app.emit(ACTIVATION_EVENT, &state);
    state
}

fn set_phase(app: &AppHandle, phase: &str, progress: f32, message_code: &str, cancellable: bool) {
    let step_index = phase_step(phase);
    update_state(app, |state| {
        state.status = "running".to_string();
        state.phase = phase.to_string();
        state.progress = progress.clamp(0.0, 1.0);
        state.message_code = Some(message_code.to_string());
        state.error_code = None;
        state.cancellable = cancellable;
        for (index, step) in state.steps.iter_mut().enumerate() {
            if index < step_index {
                step.status = "completed".to_string();
                step.progress = 1.0;
            } else if index == step_index {
                step.status = "running".to_string();
                step.progress = progress.clamp(0.0, 1.0);
            }
        }
    });
}

fn mark_failed_state(app: &AppHandle, failure: &ActivationFailure) {
    update_state(app, |state| {
        state.status = if failure.code == "CANCELED" {
            "canceled".to_string()
        } else {
            "failed".to_string()
        };
        state.cancellable = false;
        state.can_resume = failure_can_resume(Some(&failure.code));
        state.error_code = Some(failure.code.clone());
        state.message_code = Some(failure.message.clone());
        let index = phase_step(&state.phase);
        if let Some(step) = state.steps.get_mut(index) {
            step.status = "failed".to_string();
        }
    });
}

fn initialize_running_state(
    state: &mut OfflineActivationState,
    request_id: &str,
    message_code: &str,
) {
    state.status = "running".to_string();
    state.phase = "validating".to_string();
    state.progress = 0.01;
    state.bytes_downloaded = 0;
    state.total_bytes = 0;
    state.cancellable = true;
    state.can_resume = false;
    state.message_code = Some(message_code.to_string());
    state.error_code = None;
    state.request_id = Some(request_id.to_string());
    state.steps = default_steps();
    state.steps[0].status = "running".to_string();
}

fn check_canceled() -> ActivationResult<()> {
    if RUNTIME.cancel_requested.load(Ordering::Acquire) {
        Err(ActivationFailure::new("CANCELED", "activation.canceled"))
    } else {
        Ok(())
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn request_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

fn activation_root(app: &AppHandle) -> ActivationResult<PathBuf> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| ActivationFailure::new("APP_DATA_UNAVAILABLE", error.to_string()))?
        .join("offline-activation");
    fs::create_dir_all(&root)
        .map_err(|error| ActivationFailure::new("APP_DATA_UNAVAILABLE", error.to_string()))?;
    Ok(root)
}

fn journal_path(app: &AppHandle) -> ActivationResult<PathBuf> {
    Ok(activation_root(app)?.join("activation-journal.json"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> ActivationResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ActivationFailure::new("INVALID_PATH", "activation.invalidPath"))?;
    fs::create_dir_all(parent)
        .map_err(|error| ActivationFailure::new("WRITE_FAILED", error.to_string()))?;
    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("previous");
    {
        let mut file = File::create(&temporary)
            .map_err(|error| ActivationFailure::new("WRITE_FAILED", error.to_string()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| ActivationFailure::new("WRITE_FAILED", error.to_string()))?;
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| ActivationFailure::new("WRITE_FAILED", error.to_string()))?;
    }
    if path.exists() {
        fs::rename(path, &backup)
            .map_err(|error| ActivationFailure::new("WRITE_FAILED", error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(ActivationFailure::new("WRITE_FAILED", error.to_string()));
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn persist_journal(app: &AppHandle, journal: &ActivationJournal) -> ActivationResult<()> {
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| ActivationFailure::new("JOURNAL_FAILED", error.to_string()))?;
    write_atomic(&journal_path(app)?, &bytes)
}

fn read_journal(app: &AppHandle) -> ActivationResult<Option<ActivationJournal>> {
    let path = journal_path(app)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .map_err(|error| ActivationFailure::new("JOURNAL_FAILED", error.to_string()))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| ActivationFailure::new("JOURNAL_FAILED", error.to_string()))
}

fn validate_registered_game(app: &AppHandle, game_id: &str) -> ActivationResult<PathBuf> {
    if game_id != GAME_ID {
        return Err(ActivationFailure::new(
            "UNSUPPORTED_GAME",
            "activation.unsupportedGame",
        ));
    }
    let install = job::game_install_state(app, GAME_ID)
        .map_err(|error| ActivationFailure::new("INSTALL_STATE_FAILED", error.to_string()))?;
    if !install.installed || install.game_id != GAME_ID {
        return Err(ActivationFailure::new(
            "GAME_NOT_INSTALLED",
            "activation.gameNotInstalled",
        ));
    }
    let root = fs::canonicalize(&install.install_path)
        .map_err(|_| ActivationFailure::new("GAME_PATH_INVALID", "activation.gamePathInvalid"))?;
    if !root.is_dir() {
        return Err(ActivationFailure::new(
            "GAME_PATH_INVALID",
            "activation.gamePathInvalid",
        ));
    }
    for file_name in [GAME_EXECUTABLE, GAME_CONFIG] {
        let file = root.join(file_name);
        let metadata = fs::symlink_metadata(&file).map_err(|_| {
            ActivationFailure::new("GAME_FILES_MISSING", "activation.gameFilesMissing")
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ActivationFailure::new(
                "GAME_FILES_INVALID",
                "activation.gameFilesInvalid",
            ));
        }
        let canonical = fs::canonicalize(&file).map_err(|_| {
            ActivationFailure::new("GAME_FILES_INVALID", "activation.gameFilesInvalid")
        })?;
        if canonical.parent() != Some(root.as_path()) {
            return Err(ActivationFailure::new(
                "GAME_FILES_INVALID",
                "activation.gameFilesInvalid",
            ));
        }
    }
    Ok(root)
}

fn http_client() -> ActivationResult<Client> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("0xoLemon/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| ActivationFailure::new("NETWORK_CLIENT_FAILED", error.to_string()))
}

fn package_http_client() -> ActivationResult<Client> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(PACKAGE_HTTP_TIMEOUT)
        .user_agent(concat!("0xoLemon/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| ActivationFailure::new("NETWORK_CLIENT_FAILED", error.to_string()))
}

fn endpoint(game_id: &str, suffix: &str) -> String {
    format!("{BACKEND_BASE_URL}/api/{BACKEND_TENANT}/offline-activation/{game_id}/{suffix}")
}

fn backend_error(response: Response) -> ActivationFailure {
    let status = response.status();
    let body = response.json::<BackendErrorBody>().ok();
    ActivationFailure::new(
        body.as_ref()
            .and_then(|value| value.code.clone())
            .unwrap_or_else(|| format!("BACKEND_HTTP_{}", status.as_u16())),
        body.and_then(|value| value.message)
            .unwrap_or_else(|| "activation.backendError".to_string()),
    )
}

fn get_backend_status(client: &Client) -> ActivationResult<BackendStatus> {
    let response = client
        .get(endpoint(GAME_ID, "status"))
        .send()
        .map_err(|_| {
            ActivationFailure::new("BACKEND_UNAVAILABLE", "activation.backendUnavailable")
        })?;
    if !response.status().is_success() {
        return Err(backend_error(response));
    }
    response.json().map_err(|_| {
        ActivationFailure::new(
            "BACKEND_RESPONSE_INVALID",
            "activation.backendResponseInvalid",
        )
    })
}

fn get_account_eligibility(
    client: &Client,
    discord_token: &str,
) -> ActivationResult<AccountEligibility> {
    let response = client
        .get(endpoint(GAME_ID, "me"))
        .bearer_auth(discord_token)
        .send()
        .map_err(|_| {
            ActivationFailure::new("BACKEND_UNAVAILABLE", "activation.backendUnavailable")
        })?;
    if !response.status().is_success() {
        return Err(backend_error(response));
    }
    response.json().map_err(|_| {
        ActivationFailure::new(
            "BACKEND_RESPONSE_INVALID",
            "activation.backendResponseInvalid",
        )
    })
}

fn merge_remote_state(
    app: &AppHandle,
    status: &BackendStatus,
    account: Option<&AccountEligibility>,
) {
    update_state(app, |state| {
        state.capacity = status.capacity;
        state.available = status.available;
        state.in_use = status.in_use;
        state.reservations = status.reservations;
        state.next_available_at = status.next_available_at.clone();
        state.server_time = Some(
            account
                .map(|value| value.server_time.clone())
                .unwrap_or_else(|| status.server_time.clone()),
        );
        state.backend_ready = status.readiness.ready;
        state.backend_missing_configuration = status.readiness.missing_configuration.clone();
        state.package_version = status.package.version.clone();
        state.package_sha256 = status.package.sha256.clone();
        state.account_eligible = account.map(|value| value.eligible);
        state.next_eligible_at = account.and_then(|value| value.next_eligible_at.clone());
        if state.status == "idle" {
            state.error_code = None;
            state.message_code = None;
        }
    });
}

fn validated_package(status: &BackendStatus) -> ActivationResult<PackageMetadata> {
    if !status.readiness.ready {
        return Err(ActivationFailure::new(
            "SERVICE_UNAVAILABLE",
            "activation.serviceUnavailable",
        ));
    }
    if status.available == 0 {
        return Err(ActivationFailure::new(
            "NO_GLOBAL_SLOT",
            "activation.noGlobalSlot",
        ));
    }
    let package = status.package.clone();
    let version = package
        .version
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ActivationFailure::new(
                "PACKAGE_METADATA_INVALID",
                "activation.packageMetadataInvalid",
            )
        })?;
    let url = package
        .url
        .clone()
        .filter(|value| value.starts_with("https://"))
        .ok_or_else(|| {
            ActivationFailure::new(
                "PACKAGE_METADATA_INVALID",
                "activation.packageMetadataInvalid",
            )
        })?;
    let hash = package
        .sha256
        .as_deref()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            ActivationFailure::new(
                "PACKAGE_METADATA_INVALID",
                "activation.packageMetadataInvalid",
            )
        })?;
    if package.size_bytes == 0
        || package.size_bytes > MAX_PACKAGE_BYTES
        || package.files.is_empty()
        || package.files.len() > MAX_PACKAGE_FILES
    {
        return Err(ActivationFailure::new(
            "PACKAGE_METADATA_INVALID",
            "activation.packageMetadataInvalid",
        ));
    }
    let mut paths = HashSet::new();
    for file in &package.files {
        let normalized = safe_relative_path(&file.path)?;
        let normalized_text = normalized.to_string_lossy().replace('\\', "/");
        if normalized_text.eq_ignore_ascii_case(GAME_EXECUTABLE)
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !paths.insert(normalized_text.to_ascii_lowercase())
        {
            return Err(ActivationFailure::new(
                "PACKAGE_METADATA_INVALID",
                "activation.packageMetadataInvalid",
            ));
        }
    }
    let _ = (version, url, hash);
    Ok(package)
}

fn safe_relative_path(value: &str) -> ActivationResult<PathBuf> {
    if value.is_empty() || value.len() > 240 || value.contains(':') {
        return Err(ActivationFailure::new(
            "PACKAGE_PATH_INVALID",
            "activation.packagePathInvalid",
        ));
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => result.push(part),
            _ => {
                return Err(ActivationFailure::new(
                    "PACKAGE_PATH_INVALID",
                    "activation.packagePathInvalid",
                ))
            }
        }
    }
    if result.as_os_str().is_empty() {
        return Err(ActivationFailure::new(
            "PACKAGE_PATH_INVALID",
            "activation.packagePathInvalid",
        ));
    }
    Ok(result)
}

fn sha256_file(path: &Path) -> ActivationResult<(String, u64)> {
    let mut file = File::open(path)
        .map_err(|error| ActivationFailure::new("FILE_READ_FAILED", error.to_string()))?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ActivationFailure::new("FILE_READ_FAILED", error.to_string()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size = size.saturating_add(count as u64);
    }
    Ok((hex_encode(hash.finalize()), size))
}

fn file_matches(path: &Path, size: u64, sha256: &str) -> bool {
    sha256_file(path)
        .map(|(actual_hash, actual_size)| {
            actual_size == size && actual_hash.eq_ignore_ascii_case(sha256)
        })
        .unwrap_or(false)
}

fn ensure_package(
    app: &AppHandle,
    client: &Client,
    package: &PackageMetadata,
) -> ActivationResult<PathBuf> {
    let expected_hash = package.sha256.as_deref().unwrap_or_default();
    let cache_root = activation_root(app)?.join("cache");
    fs::create_dir_all(&cache_root)
        .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    let cache_path = cache_root.join(format!("{}.7z", expected_hash.to_ascii_lowercase()));
    let part_path = cache_path.with_extension("7z.part");
    if cache_path.is_file() {
        if file_matches(&cache_path, package.size_bytes, expected_hash) {
            update_state(app, |state| {
                state.bytes_downloaded = package.size_bytes;
                state.total_bytes = package.size_bytes;
            });
            return Ok(cache_path);
        }
        fs::remove_file(&cache_path)
            .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    }

    set_phase(
        app,
        "downloadingPackage",
        0.1,
        "activation.downloadingPackage",
        true,
    );
    let existing = fs::metadata(&part_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        .min(package.size_bytes);
    if existing == package.size_bytes && existing > 0 {
        if file_matches(&part_path, package.size_bytes, expected_hash) {
            fs::rename(&part_path, &cache_path).map_err(|error| {
                ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string())
            })?;
            return Ok(cache_path);
        }
        fs::remove_file(&part_path)
            .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    }

    let resume_from = fs::metadata(&part_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        .min(package.size_bytes);
    let mut request = client.get(package.url.as_deref().unwrap_or_default());
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }
    let mut response = request.send().map_err(|_| {
        ActivationFailure::new(
            "PACKAGE_DOWNLOAD_FAILED",
            "activation.packageDownloadFailed",
        )
    })?;
    let append = resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !response.status().is_success() {
        return Err(ActivationFailure::new(
            "PACKAGE_DOWNLOAD_FAILED",
            "activation.packageDownloadFailed",
        ));
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&part_path)
        .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    let mut downloaded = if append { resume_from } else { 0 };
    let mut buffer = [0u8; 128 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);
    loop {
        check_canceled()?;
        let count = response.read(&mut buffer).map_err(|_| {
            ActivationFailure::new(
                "PACKAGE_DOWNLOAD_FAILED",
                "activation.packageDownloadFailed",
            )
        })?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
        downloaded = downloaded.saturating_add(count as u64);
        if downloaded > package.size_bytes {
            return Err(ActivationFailure::new(
                "PACKAGE_SIZE_MISMATCH",
                "activation.packageIntegrityFailed",
            ));
        }
        if last_update.elapsed() >= Duration::from_millis(100) {
            let ratio = downloaded as f32 / package.size_bytes as f32;
            update_state(app, |state| {
                state.bytes_downloaded = downloaded;
                state.total_bytes = package.size_bytes;
                state.progress = 0.1 + ratio * 0.3;
                if let Some(step) = state.steps.get_mut(1) {
                    step.progress = ratio * 0.6;
                }
            });
            last_update = Instant::now();
        }
    }
    file.sync_all()
        .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    drop(file);

    set_phase(
        app,
        "verifyingPackage",
        0.42,
        "activation.verifyingPackage",
        true,
    );
    if !file_matches(&part_path, package.size_bytes, expected_hash) {
        return Err(ActivationFailure::new(
            "PACKAGE_INTEGRITY_FAILED",
            "activation.packageIntegrityFailed",
        ));
    }
    fs::rename(&part_path, &cache_path)
        .map_err(|error| ActivationFailure::new("PACKAGE_CACHE_FAILED", error.to_string()))?;
    Ok(cache_path)
}

fn extract_package(
    app: &AppHandle,
    archive: &Path,
    package: &PackageMetadata,
    password: &str,
    request_id: &str,
) -> ActivationResult<PathBuf> {
    set_phase(
        app,
        "extractingPackage",
        0.48,
        "activation.extractingPackage",
        true,
    );
    check_canceled()?;
    let staging = activation_root(app)?.join("staging").join(request_id);
    if staging.exists() {
        cleanup_staging_files(&staging, package);
        if staging.exists() {
            return Err(ActivationFailure::new(
                "PACKAGE_STAGING_NOT_EMPTY",
                "activation.packageExtractFailed",
            ));
        }
    }
    fs::create_dir_all(&staging)
        .map_err(|error| ActivationFailure::new("PACKAGE_EXTRACT_FAILED", error.to_string()))?;

    let mut allowlist = HashMap::new();
    for file in &package.files {
        let relative = safe_relative_path(&file.path)?;
        allowlist.insert(
            relative
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
            relative,
        );
    }
    let mut extracted = HashSet::new();
    let file = File::open(archive)
        .map_err(|error| ActivationFailure::new("PACKAGE_EXTRACT_FAILED", error.to_string()))?;
    let extraction_result = sevenz_rust::decompress_with_extract_fn_and_password(
        file,
        &staging,
        password.into(),
        |entry, reader, _| {
            let relative = safe_relative_path(entry.name())
                .map_err(|error| sevenz_rust::Error::other(error.message))?;
            let key = relative
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            if entry.is_directory() {
                std::io::copy(reader, &mut std::io::sink()).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            }
            let Some(allowed_relative) = allowlist.get(&key) else {
                std::io::copy(reader, &mut std::io::sink()).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            };
            if !extracted.insert(key) {
                return Err(sevenz_rust::Error::other(
                    "duplicate allowlisted archive entry",
                ));
            }
            let destination = staging.join(allowed_relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
            }
            let mut output = File::create(destination).map_err(sevenz_rust::Error::io)?;
            let copied = std::io::copy(reader, &mut output).map_err(sevenz_rust::Error::io)?;
            output.sync_all().map_err(sevenz_rust::Error::io)?;
            if copied != entry.size() {
                return Err(sevenz_rust::Error::other("archive entry size mismatch"));
            }
            Ok(true)
        },
    );
    if extraction_result.is_err() {
        cleanup_staging_files(&staging, package);
        return Err(ActivationFailure::new(
            "PACKAGE_EXTRACT_FAILED",
            "activation.packageExtractFailed",
        ));
    }

    for expected in &package.files {
        let path = staging.join(safe_relative_path(&expected.path)?);
        if !file_matches(&path, expected.size_bytes, &expected.sha256) {
            cleanup_staging_files(&staging, package);
            return Err(ActivationFailure::new(
                "PACKAGE_FILE_INTEGRITY_FAILED",
                "activation.packageIntegrityFailed",
            ));
        }
    }
    Ok(staging)
}

#[derive(Debug)]
struct PreparedFile {
    target: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
    had_target: bool,
}

fn cleanup_prepared_temporaries(prepared: &[PreparedFile]) {
    for item in prepared {
        if item.temporary.exists() {
            let _ = fs::remove_file(&item.temporary);
        }
    }
}

fn rollback_committed_files(prepared: &[PreparedFile]) {
    for item in prepared.iter().rev() {
        if item.target.exists() {
            let _ = fs::remove_file(&item.target);
        }
        if item.had_target && item.backup.exists() {
            let _ = fs::rename(&item.backup, &item.target);
        }
    }
}

fn prepare_package_file(
    game_dir: &Path,
    staging: &Path,
    expected: &PackageFile,
    suffix: &str,
) -> ActivationResult<PreparedFile> {
    let relative = safe_relative_path(&expected.path)?;
    let source = staging.join(&relative);
    let target = ensure_safe_parent(game_dir, &relative)?;
    if target.exists() {
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| ActivationFailure::new("PACKAGE_INSTALL_FAILED", error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ActivationFailure::new(
                "PACKAGE_PATH_INVALID",
                "activation.packagePathInvalid",
            ));
        }
    }
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            ActivationFailure::new("PACKAGE_PATH_INVALID", "activation.packagePathInvalid")
        })?;
    let temporary = target.with_file_name(format!(".{file_name}.activation-{suffix}.tmp"));
    let backup = target.with_file_name(format!(".{file_name}.activation-{suffix}.bak"));
    for stale in [&temporary, &backup] {
        if stale.exists() {
            fs::remove_file(stale).map_err(|error| {
                ActivationFailure::new("PACKAGE_INSTALL_FAILED", error.to_string())
            })?;
        }
    }
    if let Err(error) = fs::copy(&source, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(ActivationFailure::new(
            "PACKAGE_INSTALL_FAILED",
            error.to_string(),
        ));
    }
    if !file_matches(&temporary, expected.size_bytes, &expected.sha256) {
        let _ = fs::remove_file(&temporary);
        return Err(ActivationFailure::new(
            "PACKAGE_FILE_INTEGRITY_FAILED",
            "activation.packageIntegrityFailed",
        ));
    }
    Ok(PreparedFile {
        had_target: target.exists(),
        target,
        temporary,
        backup,
    })
}

fn ensure_safe_parent(root: &Path, relative: &Path) -> ActivationResult<PathBuf> {
    let target = root.join(relative);
    let parent = target.parent().ok_or_else(|| {
        ActivationFailure::new("PACKAGE_PATH_INVALID", "activation.packagePathInvalid")
    })?;
    let mut current = root.to_path_buf();
    for component in relative.parent().into_iter().flat_map(Path::components) {
        if let Component::Normal(part) = component {
            current.push(part);
            if current.exists() {
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    ActivationFailure::new("PACKAGE_PATH_INVALID", error.to_string())
                })?;
                if metadata.file_type().is_symlink() {
                    return Err(ActivationFailure::new(
                        "PACKAGE_PATH_INVALID",
                        "activation.packagePathInvalid",
                    ));
                }
            }
        }
    }
    fs::create_dir_all(parent)
        .map_err(|error| ActivationFailure::new("PACKAGE_INSTALL_FAILED", error.to_string()))?;
    Ok(target)
}

fn install_package_files(
    app: &AppHandle,
    game_dir: &Path,
    staging: &Path,
    package: &PackageMetadata,
    request_id: &str,
) -> ActivationResult<()> {
    set_phase(
        app,
        "installingPackage",
        0.62,
        "activation.installingPackage",
        true,
    );
    check_canceled()?;
    let suffix = &request_id[..8.min(request_id.len())];
    let mut prepared = Vec::new();
    for expected in &package.files {
        match prepare_package_file(game_dir, staging, expected, suffix) {
            Ok(item) => prepared.push(item),
            Err(error) => {
                cleanup_prepared_temporaries(&prepared);
                return Err(error);
            }
        }
    }

    let mut committed = 0usize;
    for item in &prepared {
        if item.had_target {
            if let Err(error) = fs::rename(&item.target, &item.backup) {
                rollback_committed_files(&prepared[..committed]);
                cleanup_prepared_temporaries(&prepared);
                return Err(ActivationFailure::new(
                    "PACKAGE_INSTALL_FAILED",
                    error.to_string(),
                ));
            }
        }
        if let Err(error) = fs::rename(&item.temporary, &item.target) {
            if item.had_target && item.backup.exists() {
                let _ = fs::rename(&item.backup, &item.target);
            }
            rollback_committed_files(&prepared[..committed]);
            cleanup_prepared_temporaries(&prepared);
            return Err(ActivationFailure::new(
                "PACKAGE_INSTALL_FAILED",
                error.to_string(),
            ));
        }
        committed += 1;
    }
    for item in &prepared {
        if item.backup.exists() {
            let _ = fs::remove_file(&item.backup);
        }
    }
    Ok(())
}

fn cleanup_staging_files(staging: &Path, package: &PackageMetadata) {
    let mut directories = HashSet::new();
    for expected in &package.files {
        let Ok(relative) = safe_relative_path(&expected.path) else {
            continue;
        };
        let file = staging.join(&relative);
        let _ = fs::remove_file(file);
        let mut parent = relative.parent();
        while let Some(relative_parent) = parent {
            directories.insert(staging.join(relative_parent));
            parent = relative_parent.parent();
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        let _ = fs::remove_dir(directory);
    }
    let _ = fs::remove_dir(staging);
}

fn is_ticket_name(name: &str) -> bool {
    name.starts_with("Denuvo_ticket_")
        && name.ends_with(".txt")
        && !name.contains('/')
        && !name.contains('\\')
}

fn delete_old_tickets(game_dir: &Path) -> ActivationResult<()> {
    for entry in fs::read_dir(game_dir)
        .map_err(|error| ActivationFailure::new("TICKET_SCAN_FAILED", error.to_string()))?
    {
        let entry = entry
            .map_err(|error| ActivationFailure::new("TICKET_SCAN_FAILED", error.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ticket_name(&name) && entry.path().is_file() {
            fs::remove_file(entry.path()).map_err(|error| {
                ActivationFailure::new("TICKET_CLEAN_FAILED", error.to_string())
            })?;
        }
    }
    Ok(())
}

fn ticket_from_file(path: &Path) -> ActivationResult<String> {
    let content = fs::read_to_string(path)
        .map_err(|error| ActivationFailure::new("TICKET_READ_FAILED", error.to_string()))?;
    content
        .split_whitespace()
        .find(|value| {
            value.len() >= 500 && value.len() <= 24_000 && value.matches('|').count() == 2
        })
        .map(str::to_string)
        .ok_or_else(|| ActivationFailure::new("TICKET_INVALID", "activation.ticketInvalid"))
}

fn newest_ticket_after(
    game_dir: &Path,
    started_at: SystemTime,
) -> ActivationResult<Option<(PathBuf, String)>> {
    let threshold = started_at
        .checked_sub(Duration::from_secs(2))
        .unwrap_or(UNIX_EPOCH);
    let mut candidates = Vec::new();
    for entry in fs::read_dir(game_dir)
        .map_err(|error| ActivationFailure::new("TICKET_SCAN_FAILED", error.to_string()))?
    {
        let entry = entry
            .map_err(|error| ActivationFailure::new("TICKET_SCAN_FAILED", error.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_ticket_name(&name) || !entry.path().is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        if modified >= threshold {
            candidates.push((modified, entry.path()));
        }
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in candidates {
        if let Ok(ticket) = ticket_from_file(&path) {
            return Ok(Some((path, ticket)));
        }
    }
    Ok(None)
}

fn launch_ticket_helper(game_dir: &Path) -> ActivationResult<Child> {
    let mut command = Command::new(game_dir.join(GAME_EXECUTABLE));
    command.current_dir(game_dir).args(["-silent", "-quiet"]);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
        .spawn()
        .map_err(|error| ActivationFailure::new("GAME_LAUNCH_FAILED", error.to_string()))
}

fn wait_for_ticket(app: &AppHandle, game_dir: &Path) -> ActivationResult<(PathBuf, String)> {
    delete_old_tickets(game_dir)?;
    let started_at = SystemTime::now();
    let mut child = launch_ticket_helper(game_dir)?;
    set_phase(
        app,
        "waitingForTicket",
        0.72,
        "activation.waitingForTicket",
        true,
    );
    let wait_started = Instant::now();
    loop {
        if let Err(error) = check_canceled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if let Some(ticket) = newest_ticket_after(game_dir, started_at)? {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ticket);
        }
        if wait_started.elapsed() >= TICKET_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ActivationFailure::new(
                "TICKET_TIMEOUT",
                "activation.ticketTimeout",
            ));
        }
        if child
            .try_wait()
            .map_err(|error| ActivationFailure::new("GAME_LAUNCH_FAILED", error.to_string()))?
            .is_some()
            && wait_started.elapsed() > Duration::from_secs(5)
        {
            if let Some(ticket) = newest_ticket_after(game_dir, started_at)? {
                return Ok(ticket);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn resume_ticket(
    game_dir: &Path,
    journal: &ActivationJournal,
) -> ActivationResult<Option<(PathBuf, String)>> {
    let (Some(expected_hash), Some(file_name)) = (&journal.ticket_hash, &journal.ticket_file)
    else {
        return Ok(None);
    };
    if !is_ticket_name(file_name) {
        return Err(ActivationFailure::new(
            "RESUME_TICKET_INVALID",
            "activation.resumeTicketMissing",
        ));
    }
    let path = game_dir.join(file_name);
    if !path.is_file() {
        return Err(ActivationFailure::new(
            "RESUME_TICKET_MISSING",
            "activation.resumeTicketMissing",
        ));
    }
    let ticket = ticket_from_file(&path)?;
    let actual_hash = hex_encode(Sha256::digest(ticket.as_bytes()));
    if !actual_hash.eq_ignore_ascii_case(expected_hash) {
        return Err(ActivationFailure::new(
            "RESUME_TICKET_INVALID",
            "activation.resumeTicketMissing",
        ));
    }
    Ok(Some((path, ticket)))
}

fn request_activation_token(
    client: &Client,
    discord_token: &str,
    request_id: &str,
    ticket: &str,
    launcher_version: &str,
) -> ActivationResult<String> {
    let response = client
        .post(endpoint(GAME_ID, "activate"))
        .bearer_auth(discord_token)
        .json(&ActivationRequest {
            ticket,
            request_id,
            launcher_version,
        })
        .send()
        .map_err(|_| ActivationFailure::new("OUTCOME_UNCERTAIN", "activation.outcomeUncertain"))?;
    if !response.status().is_success() {
        return Err(backend_error(response));
    }
    let response: ActivationResponse = response.json().map_err(|_| {
        ActivationFailure::new(
            "BACKEND_RESPONSE_INVALID",
            "activation.backendResponseInvalid",
        )
    })?;
    if response.token.len() < 16
        || response.token.len() > 64 * 1024
        || response
            .token
            .chars()
            .any(|character| character.is_control() || character == '"')
    {
        return Err(ActivationFailure::new(
            "TOKEN_INVALID",
            "activation.tokenInvalid",
        ));
    }
    Ok(response.token)
}

fn apply_token_atomic(game_dir: &Path, token: &str, request_id: &str) -> ActivationResult<()> {
    let path = game_dir.join(GAME_CONFIG);
    let content = fs::read_to_string(&path)
        .map_err(|error| ActivationFailure::new("CONFIG_READ_FAILED", error.to_string()))?;
    let expression = Regex::new(r#"(?im)^(\s*"DenuvoToken"\s+)"[^"\r\n]*""#)
        .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
    if !expression.is_match(&content) {
        return Err(ActivationFailure::new(
            "CONFIG_TOKEN_FIELD_MISSING",
            "activation.configTokenFieldMissing",
        ));
    }
    let updated = expression
        .replace(&content, |captures: &regex::Captures<'_>| {
            format!("{}\"{}\"", &captures[1], token)
        })
        .to_string();
    let suffix = &request_id[..8.min(request_id.len())];
    let temporary = path.with_file_name(format!(".{GAME_CONFIG}.activation-{suffix}.tmp"));
    {
        let mut file = File::create(&temporary)
            .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
        file.write_all(updated.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
    }
    let backup = path.with_file_name(format!(".{GAME_CONFIG}.activation-{suffix}.bak"));
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
    }
    fs::rename(&path, &backup)
        .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::rename(&backup, &path);
        return Err(ActivationFailure::new(
            "CONFIG_UPDATE_FAILED",
            error.to_string(),
        ));
    }
    let verified = fs::read_to_string(&path)
        .map_err(|error| ActivationFailure::new("CONFIG_UPDATE_FAILED", error.to_string()))?;
    let expected = format!("\"{token}\"");
    if !verified.contains(&expected) {
        let _ = fs::remove_file(&path);
        let _ = fs::rename(&backup, &path);
        return Err(ActivationFailure::new(
            "CONFIG_VERIFY_FAILED",
            "activation.configVerifyFailed",
        ));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn launch_game(game_dir: &Path) -> ActivationResult<()> {
    let mut command = Command::new(game_dir.join(GAME_EXECUTABLE));
    command.current_dir(game_dir);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| ActivationFailure::new("GAME_LAUNCH_FAILED", error.to_string()))
}

fn update_journal_phase(
    app: &AppHandle,
    journal: &mut ActivationJournal,
    phase: &str,
) -> ActivationResult<()> {
    journal.phase = phase.to_string();
    journal.status = "running".to_string();
    journal.updated_at_ms = unix_millis();
    persist_journal(app, journal)
}

fn run_activation(
    app: &AppHandle,
    mut journal: ActivationJournal,
    resume: bool,
) -> ActivationResult<()> {
    let game_dir = validate_registered_game(app, &journal.game_id)?;
    let journal_dir = fs::canonicalize(&journal.game_dir)
        .map_err(|_| ActivationFailure::new("GAME_PATH_INVALID", "activation.gamePathInvalid"))?;
    if journal_dir != game_dir {
        return Err(ActivationFailure::new(
            "GAME_PATH_CHANGED",
            "activation.gamePathChanged",
        ));
    }
    set_phase(app, "validating", 0.03, "activation.validating", true);
    update_journal_phase(app, &mut journal, "validating")?;
    check_canceled()?;

    let client = http_client()?;
    let discord_token = discord_auth::access_token_for_backend(app)
        .map_err(|_| ActivationFailure::new("AUTH_REQUIRED", "activation.authRequired"))?;
    let backend_status = get_backend_status(&client)?;
    let account = get_account_eligibility(&client, &discord_token)?;
    merge_remote_state(app, &backend_status, Some(&account));
    let existing_ticket = if resume {
        resume_ticket(&game_dir, &journal)?
    } else {
        None
    };
    let (ticket_path, ticket) = if let Some(ticket) = existing_ticket {
        ticket
    } else {
        if !account.eligible {
            return Err(ActivationFailure::new(
                "ACCOUNT_COOLDOWN",
                "activation.accountCooldown",
            ));
        }
        let package = validated_package(&backend_status)?;
        journal.package_version = package.version.clone();
        persist_journal(app, &journal)?;
        let package_client = package_http_client()?;
        let archive = ensure_package(app, &package_client, &package)?;
        check_canceled()?;
        let staging = extract_package(
            app,
            &archive,
            &package,
            &account.package_access.archive_password,
            &journal.request_id,
        )?;
        let install_result =
            install_package_files(app, &game_dir, &staging, &package, &journal.request_id);
        cleanup_staging_files(&staging, &package);
        install_result?;
        update_journal_phase(app, &mut journal, "waitingForTicket")?;
        set_phase(
            app,
            "launchingTicketHelper",
            0.68,
            "activation.launchingTicketHelper",
            true,
        );
        wait_for_ticket(app, &game_dir)?
    };

    let ticket_hash = hex_encode(Sha256::digest(ticket.as_bytes()));
    let ticket_file = ticket_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| is_ticket_name(value))
        .ok_or_else(|| ActivationFailure::new("TICKET_INVALID", "activation.ticketInvalid"))?
        .to_string();
    journal.ticket_hash = Some(ticket_hash);
    journal.ticket_file = Some(ticket_file);
    update_journal_phase(app, &mut journal, "requestingActivation")?;
    check_canceled()?;
    set_phase(
        app,
        "requestingActivation",
        0.82,
        "activation.requestingActivation",
        false,
    );
    RUNTIME.cancel_requested.store(false, Ordering::Release);

    let launcher_version = app.package_info().version.to_string();
    let token = request_activation_token(
        &client,
        &discord_token,
        &journal.request_id,
        &ticket,
        &launcher_version,
    )?;
    set_phase(
        app,
        "applyingToken",
        0.93,
        "activation.applyingToken",
        false,
    );
    update_journal_phase(app, &mut journal, "applyingToken")?;
    apply_token_atomic(&game_dir, &token, &journal.request_id)?;
    drop(token);

    set_phase(
        app,
        "launchingGame",
        0.98,
        "activation.launchingGame",
        false,
    );
    launch_game(&game_dir)?;
    journal.status = "committed".to_string();
    journal.phase = "completed".to_string();
    journal.error_code = None;
    journal.updated_at_ms = unix_millis();
    persist_journal(app, &journal)?;
    let latest_status = get_backend_status(&client).ok();
    let latest_account = get_account_eligibility(&client, &discord_token).ok();
    if let Some(status) = latest_status.as_ref() {
        merge_remote_state(app, status, latest_account.as_ref());
    }
    update_state(app, |state| {
        state.status = "completed".to_string();
        state.phase = "completed".to_string();
        state.progress = 1.0;
        state.cancellable = false;
        state.can_resume = false;
        state.error_code = None;
        state.message_code = Some("activation.completed".to_string());
        state.bytes_downloaded = state.total_bytes;
        for step in &mut state.steps {
            step.status = "completed".to_string();
            step.progress = 1.0;
        }
    });
    Ok(())
}

fn spawn_activation(app: AppHandle, journal: ActivationJournal, resume: bool) {
    tauri::async_runtime::spawn(async move {
        let worker_app = app.clone();
        let journal_for_worker = journal.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            run_activation(&worker_app, journal_for_worker, resume)
        })
        .await
        .unwrap_or_else(|_| {
            Err(ActivationFailure::new(
                "ACTIVATION_WORKER_FAILED",
                "activation.workerFailed",
            ))
        });

        if let Err(failure) = result {
            let mut failed_journal = read_journal(&app)
                .ok()
                .flatten()
                .filter(|current| current.request_id == journal.request_id)
                .unwrap_or(journal);
            failed_journal.status = if failure.code == "CANCELED" {
                "canceled".to_string()
            } else {
                "failed".to_string()
            };
            failed_journal.error_code = Some(failure.code.clone());
            failed_journal.updated_at_ms = unix_millis();
            let _ = persist_journal(&app, &failed_journal);
            mark_failed_state(&app, &failure);
        }
        RUNTIME.cancel_requested.store(false, Ordering::Release);
        RUNTIME.running.store(false, Ordering::Release);
    });
}

fn acquire_runtime() -> Result<(), String> {
    RUNTIME
        .running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| "An offline activation is already running.".to_string())
}

fn state_from_journal(state: &mut OfflineActivationState, journal: &ActivationJournal) {
    state.request_id = Some(journal.request_id.clone());
    state.phase = journal.phase.clone();
    state.status = match journal.status.as_str() {
        "committed" => "completed".to_string(),
        "running" => "paused".to_string(),
        _ => journal.status.clone(),
    };
    state.progress = phase_progress(&journal.phase);
    state.package_version = journal.package_version.clone();
    state.error_code = journal.error_code.clone();
    state.message_code = Some(match journal.status.as_str() {
        "committed" => "activation.completed".to_string(),
        "canceled" => "activation.canceled".to_string(),
        "running" => "activation.interrupted".to_string(),
        _ => format!("activation.{}", journal.phase),
    });
    state.can_resume = journal.status == "running"
        || (journal.status == "failed" && failure_can_resume(journal.error_code.as_deref()));
    state.cancellable = false;
    state.steps = default_steps();
    let current_step = phase_step(&journal.phase);
    for (index, step) in state.steps.iter_mut().enumerate() {
        if journal.status == "committed" || index < current_step {
            step.status = "completed".to_string();
            step.progress = 1.0;
        } else if index == current_step && journal.status != "canceled" {
            step.status = match journal.status.as_str() {
                "failed" => "failed".to_string(),
                "running" => "pending".to_string(),
                _ => "running".to_string(),
            };
            step.progress = state.progress;
        }
    }
}

#[tauri::command]
pub async fn get_offline_activation_state(
    app: AppHandle,
    game_id: String,
) -> Result<OfflineActivationState, String> {
    if game_id != GAME_ID {
        return Err("Offline activation is only available for EA SPORTS FC 26.".to_string());
    }
    let worker_app = app.clone();
    let remote = tauri::async_runtime::spawn_blocking(move || {
        let client = http_client()?;
        let status = get_backend_status(&client)?;
        let account = discord_auth::access_token_for_backend(&worker_app)
            .ok()
            .and_then(|token| get_account_eligibility(&client, &token).ok());
        Ok::<_, ActivationFailure>((status, account))
    })
    .await
    .map_err(|_| "Offline activation state worker failed.".to_string())?;

    match remote {
        Ok((status, account)) => merge_remote_state(&app, &status, account.as_ref()),
        Err(failure) => {
            update_state(&app, |state| {
                state.backend_ready = false;
                state.account_eligible = None;
                if state.status == "idle" {
                    state.error_code = Some(failure.code);
                    state.message_code = Some(failure.message);
                }
            });
        }
    }
    if !RUNTIME.running.load(Ordering::Acquire) {
        if let Ok(Some(journal)) = read_journal(&app) {
            update_state(&app, |state| state_from_journal(state, &journal));
        }
    }
    Ok(RUNTIME
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone())
}

#[tauri::command]
pub async fn start_offline_activation(
    app: AppHandle,
    game_id: String,
) -> Result<OfflineActivationState, String> {
    let game_dir = validate_registered_game(&app, &game_id).map_err(|error| error.message)?;
    acquire_runtime()?;
    RUNTIME.cancel_requested.store(false, Ordering::Release);
    let now = unix_millis();
    let journal = ActivationJournal {
        schema_version: 1,
        request_id: request_id(),
        game_id: GAME_ID.to_string(),
        game_dir: game_dir.display().to_string(),
        status: "running".to_string(),
        phase: "validating".to_string(),
        package_version: None,
        ticket_hash: None,
        ticket_file: None,
        started_at_ms: now,
        updated_at_ms: now,
        error_code: None,
    };
    if let Err(error) = persist_journal(&app, &journal) {
        RUNTIME.running.store(false, Ordering::Release);
        return Err(error.message);
    }
    let initial = update_state(&app, |state| {
        initialize_running_state(state, &journal.request_id, "activation.validating");
    });
    spawn_activation(app, journal, false);
    Ok(initial)
}

#[tauri::command]
pub async fn resume_offline_activation(app: AppHandle) -> Result<OfflineActivationState, String> {
    let journal = read_journal(&app)
        .map_err(|error| error.message)?
        .ok_or_else(|| "No offline activation journal is available.".to_string())?;
    if !matches!(journal.status.as_str(), "running" | "failed") {
        return Err("This offline activation cannot be resumed.".to_string());
    }
    let game_dir =
        validate_registered_game(&app, &journal.game_id).map_err(|error| error.message)?;
    let journal_dir = fs::canonicalize(&journal.game_dir)
        .map_err(|_| "The recorded game folder no longer exists.".to_string())?;
    if game_dir != journal_dir {
        return Err("The registered game folder changed after activation started.".to_string());
    }
    acquire_runtime()?;
    RUNTIME.cancel_requested.store(false, Ordering::Release);
    let initial = update_state(&app, |state| {
        initialize_running_state(state, &journal.request_id, "activation.resuming");
    });
    spawn_activation(app, journal, true);
    Ok(initial)
}

#[tauri::command]
pub fn cancel_offline_activation(app: AppHandle) -> Result<OfflineActivationState, String> {
    if !RUNTIME.running.load(Ordering::Acquire) {
        return Err("No offline activation is running.".to_string());
    }
    let state = RUNTIME
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if !state.cancellable {
        return Err(
            "Activation can no longer be canceled because the EA request has started.".to_string(),
        );
    }
    RUNTIME.cancel_requested.store(true, Ordering::Release);
    Ok(update_state(&app, |state| {
        state.message_code = Some("activation.canceling".to_string());
        state.cancellable = false;
    }))
}

#[cfg(test)]
mod tests {
    use super::{failure_can_resume, safe_relative_path, GAME_EXECUTABLE};

    #[test]
    fn activation_package_paths_cannot_escape_the_game_directory() {
        assert!(safe_relative_path("plugins/activation.dll").is_ok());
        assert!(safe_relative_path("../activation.dll").is_err());
        assert!(safe_relative_path("C:\\activation.dll").is_err());
        assert!(safe_relative_path("/activation.dll").is_err());
    }

    #[test]
    fn game_executable_name_is_fixed() {
        assert_eq!(GAME_EXECUTABLE, "FC26.exe");
    }

    #[test]
    fn resume_policy_keeps_uncertain_requests_and_rejects_invalid_installs() {
        assert!(failure_can_resume(Some("OUTCOME_UNCERTAIN")));
        assert!(failure_can_resume(Some("CONFIG_VERIFY_FAILED")));
        assert!(!failure_can_resume(Some("EA_REJECTED")));
        assert!(!failure_can_resume(Some("GAME_PATH_CHANGED")));
        assert!(!failure_can_resume(Some("CANCELED")));
    }
}
