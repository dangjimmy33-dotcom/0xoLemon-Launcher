use std::collections::VecDeque;
use std::fs;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Error as WebSocketError, Message, WebSocket};

use crate::install_discovery::{self, LibraryRecoveryIndex};
use crate::job::{self, JobControl, JobJournal, JobStatus};
use crate::platform;
use crate::secret_store::{protect, unprotect};

const STORE_FILE: &str = "remote-web-access.json";
const STORE_SCHEMA: u32 = 1;
const BACKEND_API: &str = "https://zeroxolemon-launcher.onrender.com/api/0xolemon";
const CONNECT_RETRY_MAX: Duration = Duration::from_secs(60);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const SOCKET_POLL_INTERVAL: Duration = Duration::from_secs(1);
const OUTBOX_LIMIT: usize = 256;

static MANAGER: OnceLock<Arc<RemoteWebManager>> = OnceLock::new();

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredRemoteWebAccess {
    schema_version: u32,
    enabled: bool,
    device_id: String,
    encrypted_credential: String,
    device_name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWebAccessState {
    pub enabled: bool,
    pub paired: bool,
    pub connected: bool,
    pub device_id: String,
    pub device_name: String,
    pub last_error: Option<String>,
    pub last_connected_at: Option<String>,
    pub active_job_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeState {
    connected: bool,
    last_error: Option<String>,
    last_connected_at: Option<String>,
    active_job_id: Option<String>,
    active_cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceLibrary {
    id: String,
    label: String,
    free_bytes: u64,
    default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceState {
    name: String,
    os: String,
    launcher_version: String,
    libraries: Vec<DeviceLibrary>,
    installed_game_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationResponse {
    device_id: String,
    credential: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteJobRequest {
    id: String,
    request_id: String,
    action: String,
    game_id: String,
    #[serde(default)]
    version_id: String,
    device_id: String,
    #[serde(default)]
    library_id: String,
}

#[derive(Debug, Clone)]
struct LocalRemoteJob {
    request: RemoteJobRequest,
    install_path: PathBuf,
    launch_executable: Option<String>,
    canceled: Arc<AtomicBool>,
    attach_existing: bool,
}

struct RemoteWebManager {
    app: AppHandle,
    control: Arc<JobControl>,
    stored: Mutex<StoredRemoteWebAccess>,
    runtime: Mutex<RuntimeState>,
    outbox: Mutex<VecDeque<Value>>,
    worker_started: AtomicBool,
}

impl RemoteWebManager {
    fn new(app: AppHandle, control: Arc<JobControl>) -> Result<Self, String> {
        Ok(Self {
            stored: Mutex::new(load_store(&app)?),
            app,
            control,
            runtime: Mutex::new(RuntimeState::default()),
            outbox: Mutex::new(VecDeque::new()),
            worker_started: AtomicBool::new(false),
        })
    }

    fn public_state(&self) -> RemoteWebAccessState {
        let stored = lock(&self.stored).clone();
        let runtime = lock(&self.runtime).clone();
        RemoteWebAccessState {
            enabled: stored.enabled,
            paired: !stored.device_id.is_empty() && !stored.encrypted_credential.is_empty(),
            connected: runtime.connected,
            device_id: stored.device_id,
            device_name: stored.device_name,
            last_error: runtime.last_error,
            last_connected_at: runtime.last_connected_at,
            active_job_id: runtime.active_job_id,
        }
    }

    fn emit_state(&self) {
        let _ = self
            .app
            .emit("launcher://remote-web-state", self.public_state());
    }

    fn set_runtime<F>(&self, mutate: F)
    where
        F: FnOnce(&mut RuntimeState),
    {
        mutate(&mut lock(&self.runtime));
        self.emit_state();
    }

    fn save_stored<F>(&self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut StoredRemoteWebAccess),
    {
        let value = {
            let mut stored = lock(&self.stored);
            mutate(&mut stored);
            stored.schema_version = STORE_SCHEMA;
            stored.clone()
        };
        save_store(&self.app, &value)?;
        self.emit_state();
        Ok(())
    }

    fn start(self: &Arc<Self>) {
        if self.worker_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let manager = self.clone();
        thread::spawn(move || manager.supervisor());
    }

    fn supervisor(self: Arc<Self>) {
        let mut retry = Duration::from_secs(2);
        loop {
            if !lock(&self.stored).enabled {
                let should_emit = {
                    let mut runtime = lock(&self.runtime);
                    let changed = runtime.connected || runtime.last_error.is_some();
                    runtime.connected = false;
                    runtime.last_error = None;
                    changed
                };
                if should_emit {
                    self.emit_state();
                }
                thread::sleep(Duration::from_secs(2));
                retry = Duration::from_secs(2);
                continue;
            }

            let result = self.connect_once();
            self.set_runtime(|runtime| {
                runtime.connected = false;
                if let Err(error) = &result {
                    runtime.last_error = Some(safe_error(error));
                }
            });
            if lock(&self.stored).enabled {
                thread::sleep(retry);
                retry = (retry * 2).min(CONNECT_RETRY_MAX);
            }
        }
    }

    fn connect_once(self: &Arc<Self>) -> Result<(), String> {
        self.ensure_registration()?;
        let (device_id, credential) = self.credentials()?;
        let url = websocket_url();
        let (mut socket, _) =
            connect(url.as_str()).map_err(|error| format!("REMOTE_CONNECT_FAILED: {error}"))?;
        configure_socket_timeout(&mut socket)?;
        send_json(
            &mut socket,
            &json!({
                "type": "hello",
                "deviceId": device_id,
                "credential": credential,
                "state": self.device_state(),
            }),
        )?;

        let hello_deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if Instant::now() >= hello_deadline {
                return Err("REMOTE_HELLO_TIMEOUT".to_string());
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    let message: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "REMOTE_PROTOCOL_INVALID".to_string())?;
                    if message.get("type").and_then(Value::as_str) == Some("hello.accepted") {
                        break;
                    }
                    if message.get("type").and_then(Value::as_str) == Some("error") {
                        return Err(message
                            .get("code")
                            .and_then(Value::as_str)
                            .unwrap_or("REMOTE_HELLO_REJECTED")
                            .to_string());
                    }
                }
                Ok(Message::Close(_)) => return Err("REMOTE_SOCKET_CLOSED".to_string()),
                Ok(_) => {}
                Err(error) if websocket_would_block(&error) => {}
                Err(error) => return Err(format!("REMOTE_HELLO_FAILED: {error}")),
            }
        }

        self.set_runtime(|runtime| {
            runtime.connected = true;
            runtime.last_error = None;
            runtime.last_connected_at = Some(Utc::now().to_rfc3339());
        });

        let mut last_heartbeat = Instant::now();
        loop {
            if !lock(&self.stored).enabled {
                let _ = socket.close(None);
                return Ok(());
            }
            self.flush_outbox(&mut socket)?;
            if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
                send_json(
                    &mut socket,
                    &json!({ "type": "heartbeat", "state": self.device_state() }),
                )?;
                last_heartbeat = Instant::now();
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    let message: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "REMOTE_PROTOCOL_INVALID".to_string())?;
                    self.handle_message(message);
                }
                Ok(Message::Ping(payload)) => socket
                    .send(Message::Pong(payload))
                    .map_err(|error| format!("REMOTE_PONG_FAILED: {error}"))?,
                Ok(Message::Close(_)) => return Err("REMOTE_SOCKET_CLOSED".to_string()),
                Ok(_) => {}
                Err(error) if websocket_would_block(&error) => {}
                Err(error) => return Err(format!("REMOTE_SOCKET_FAILED: {error}")),
            }
        }
    }

    fn ensure_registration(&self) -> Result<(), String> {
        if self.credentials().is_ok() {
            return Ok(());
        }
        let token = crate::discord_auth::access_token_for_backend(&self.app)
            .map_err(|_| "DISCORD_AUTH_REQUIRED".to_string())?;
        let current = lock(&self.stored).clone();
        let device_id = (!current.device_id.is_empty()).then_some(current.device_id.clone());
        let device_name = if current.device_name.is_empty() {
            default_device_name()
        } else {
            current.device_name.clone()
        };
        let device_state = self.device_state();
        let body = json!({
            "deviceId": device_id,
            "name": device_name,
            "os": "Windows",
            "launcherVersion": env!("CARGO_PKG_VERSION"),
            "libraries": device_state.libraries,
            "installedGameIds": device_state.installed_game_ids,
        });
        let response = http_client()?
            .post(format!("{}/devices/register", backend_api_base()))
            .bearer_auth(token)
            .json(&body)
            .send()
            .map_err(|error| format!("REMOTE_REGISTRATION_FAILED: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "REMOTE_REGISTRATION_REJECTED_{}",
                response.status().as_u16()
            ));
        }
        let registration: RegistrationResponse = response
            .json()
            .map_err(|_| "REMOTE_REGISTRATION_INVALID".to_string())?;
        if !valid_identifier(&registration.device_id, 12, 96) || registration.credential.len() < 32
        {
            return Err("REMOTE_REGISTRATION_INVALID".to_string());
        }
        let encrypted = protect(registration.credential.as_bytes())?;
        self.save_stored(|stored| {
            stored.device_id = registration.device_id;
            stored.encrypted_credential = STANDARD.encode(encrypted);
            if stored.device_name.is_empty() {
                stored.device_name = default_device_name();
            }
        })
    }

    fn credentials(&self) -> Result<(String, String), String> {
        let stored = lock(&self.stored).clone();
        if !valid_identifier(&stored.device_id, 12, 96) || stored.encrypted_credential.is_empty() {
            return Err("REMOTE_DEVICE_NOT_REGISTERED".to_string());
        }
        let encrypted = STANDARD
            .decode(stored.encrypted_credential.as_bytes())
            .map_err(|_| "REMOTE_CREDENTIAL_INVALID".to_string())?;
        let credential = String::from_utf8(unprotect(&encrypted)?)
            .map_err(|_| "REMOTE_CREDENTIAL_INVALID".to_string())?;
        Ok((stored.device_id, credential))
    }

    fn device_state(&self) -> DeviceState {
        let default_root = platform::current_settings().default_library;
        let default_canonical = canonical_if_exists(Path::new(&default_root));
        let libraries = install_discovery::registered_library_roots()
            .into_iter()
            .filter_map(|(marker, root)| device_library(marker, root, default_canonical.as_deref()))
            .collect();
        let mut installed_game_ids = platform::install_records(&self.app)
            .unwrap_or_default()
            .into_iter()
            .filter(|record| Path::new(&record.install_path).exists())
            .map(|record| record.game_id)
            .collect::<Vec<_>>();
        installed_game_ids.sort();
        installed_game_ids.dedup();
        DeviceState {
            name: lock(&self.stored).device_name.clone(),
            os: "Windows".to_string(),
            launcher_version: env!("CARGO_PKG_VERSION").to_string(),
            libraries,
            installed_game_ids,
        }
    }

    fn flush_outbox(
        &self,
        socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    ) -> Result<(), String> {
        loop {
            let next = lock(&self.outbox).pop_front();
            let Some(message) = next else { return Ok(()) };
            if let Err(error) = send_json(socket, &message) {
                lock(&self.outbox).push_front(message);
                return Err(error);
            }
        }
    }

    fn queue_message(&self, message: Value) {
        let mut outbox = lock(&self.outbox);
        let kind = message.get("type").and_then(Value::as_str);
        let job_id = message.get("jobId").and_then(Value::as_str);
        if kind == Some("remoteJob.progress") {
            if let Some(existing) = outbox.iter_mut().rev().find(|queued| {
                queued.get("type").and_then(Value::as_str) == kind
                    && queued.get("jobId").and_then(Value::as_str) == job_id
            }) {
                *existing = message;
                return;
            }
        }
        while outbox.len() >= OUTBOX_LIMIT {
            if let Some(index) = outbox.iter().position(|queued| {
                queued.get("type").and_then(Value::as_str) == Some("remoteJob.progress")
            }) {
                outbox.remove(index);
            } else {
                outbox.pop_front();
            }
        }
        outbox.push_back(message);
    }

    fn handle_message(self: &Arc<Self>, message: Value) {
        match message.get("type").and_then(Value::as_str) {
            Some("remoteJob.dispatch") => {
                let dispatch_nonce = message
                    .get("dispatchNonce")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let fallback_job_id = message
                    .get("job")
                    .and_then(|job| job.get("id"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_default();
                let parsed = message
                    .get("job")
                    .cloned()
                    .ok_or_else(|| "REMOTE_JOB_INVALID".to_string())
                    .and_then(|value| {
                        serde_json::from_value::<RemoteJobRequest>(value)
                            .map_err(|_| "REMOTE_JOB_INVALID".to_string())
                    });
                match parsed {
                    Ok(request) => match self.accept_job(request) {
                        Ok(job_id) => self.queue_message(json!({
                            "type": "remoteJob.ack",
                            "jobId": job_id,
                            "dispatchNonce": dispatch_nonce,
                            "accepted": true
                        })),
                        Err((job_id, detail)) => self.queue_message(json!({
                            "type": "remoteJob.ack",
                            "jobId": job_id,
                            "dispatchNonce": dispatch_nonce,
                            "accepted": false,
                            "detail": detail
                        })),
                    },
                    Err(detail) => self.queue_message(json!({
                        "type": "remoteJob.ack",
                        "jobId": fallback_job_id,
                        "dispatchNonce": dispatch_nonce,
                        "accepted": false,
                        "detail": detail
                    })),
                }
            }
            Some("remoteJob.cancel") => {
                if let Some(job_id) = message.get("jobId").and_then(Value::as_str) {
                    self.cancel_job(job_id);
                }
            }
            _ => {}
        }
    }

    fn accept_job(self: &Arc<Self>, request: RemoteJobRequest) -> Result<String, (String, String)> {
        let job_id = request.id.clone();
        if !valid_remote_job(&request) {
            return Err((job_id, "REMOTE_JOB_INVALID".to_string()));
        }
        if request.device_id != lock(&self.stored).device_id {
            return Err((job_id, "REMOTE_DEVICE_MISMATCH".to_string()));
        }
        let matching_journal = job::read_latest_journal(&self.app)
            .ok()
            .flatten()
            .filter(|journal| journal_matches_request(journal, &request));
        let attach_existing = self.control.is_running() && matching_journal.is_some();
        {
            let runtime = lock(&self.runtime);
            if runtime.active_job_id.as_deref() == Some(request.id.as_str()) {
                return Ok(request.id);
            }
            if runtime.active_job_id.is_some() || (self.control.is_running() && !attach_existing) {
                return Err((job_id, "LAUNCHER_BUSY".to_string()));
            }
        }
        let mut local = self
            .resolve_local_job(request)
            .map_err(|error| (job_id.clone(), error))?;
        local.attach_existing = attach_existing;
        let cancel = local.canceled.clone();
        self.set_runtime(|runtime| {
            runtime.active_job_id = Some(job_id.clone());
            runtime.active_cancel = Some(cancel);
        });
        let manager = self.clone();
        thread::spawn(move || manager.execute_job(local));
        Ok(job_id)
    }

    fn resolve_local_job(&self, request: RemoteJobRequest) -> Result<LocalRemoteJob, String> {
        let action = request.action.as_str();
        let (install_path, launch_executable) = if action == "install" {
            let root = library_root_by_id(&request.library_id)
                .ok_or_else(|| "LIBRARY_NOT_REGISTERED".to_string())?;
            (job::install_path_for_library(&request.game_id, &root), None)
        } else {
            let record = platform::install_record(&self.app, &request.game_id)
                .map_err(|_| "INSTALL_STATE_UNAVAILABLE".to_string())?
                .ok_or_else(|| "GAME_NOT_INSTALLED".to_string())?;
            let install_path = PathBuf::from(&record.install_path);
            if !install_path.exists() {
                return Err("GAME_NOT_INSTALLED".to_string());
            }
            if action != "launch" && !install_belongs_to_library(&install_path, &request.library_id)
            {
                return Err("LIBRARY_MISMATCH".to_string());
            }
            let executable =
                (!record.launch_executable.trim().is_empty()).then_some(record.launch_executable);
            (install_path, executable)
        };
        Ok(LocalRemoteJob {
            request,
            install_path,
            launch_executable,
            canceled: Arc::new(AtomicBool::new(false)),
            attach_existing: false,
        })
    }

    fn execute_job(self: Arc<Self>, local: LocalRemoteJob) {
        let result = self.execute_job_inner(&local);
        if local.canceled.load(Ordering::Acquire) {
            self.queue_message(json!({ "type": "remoteJob.canceled", "jobId": local.request.id }));
        } else if let Err(error) = result {
            self.queue_message(json!({
                "type": "remoteJob.fail",
                "jobId": local.request.id,
                "errorCode": error_code(&error),
                "errorMessage": safe_error(&error),
            }));
        } else {
            self.queue_message(json!({ "type": "remoteJob.complete", "jobId": local.request.id }));
        }
        self.set_runtime(|runtime| {
            if runtime.active_job_id.as_deref() == Some(local.request.id.as_str()) {
                runtime.active_job_id = None;
                runtime.active_cancel = None;
            }
        });
    }

    fn execute_job_inner(&self, local: &LocalRemoteJob) -> Result<(), String> {
        match local.request.action.as_str() {
            "install" => {
                if self.require_installed_version(local).is_ok() {
                    return Ok(());
                }
                self.finish_existing_recovery(local)?;
                if !self.resume_matching_job(local)? {
                    self.control.reset();
                    job::spawn_install_job(
                        self.app.clone(),
                        self.control.clone(),
                        Some(local.request.version_id.clone()),
                        Some(local.install_path.display().to_string()),
                        Some(local.request.game_id.clone()),
                    )
                    .map_err(|error| error.to_string())?;
                }
                self.monitor_mutating_job(local, true)?;
                self.require_installed_version(local)
            }
            "update" | "downgrade" => {
                if self.require_installed_version(local).is_ok() {
                    return Ok(());
                }
                self.finish_existing_recovery(local)?;
                if !self.resume_matching_job(local)? {
                    self.control.reset();
                    job::spawn_update_job(
                        self.app.clone(),
                        self.control.clone(),
                        local.install_path.display().to_string(),
                        Some(local.request.version_id.clone()),
                        Some(local.request.game_id.clone()),
                    )
                    .map_err(|error| error.to_string())?;
                }
                self.monitor_mutating_job(local, true)?;
                self.require_installed_version(local)
            }
            "verify" => {
                self.queue_static_progress(&local.request.id, "Verifying", 0.01);
                let report = job::verify_install_integrity(
                    Some(&self.app),
                    &local.request.game_id,
                    &local.install_path,
                    Some(local.request.version_id.clone()),
                )
                .map_err(|error| error.to_string())?;
                if report.ok {
                    Ok(())
                } else {
                    Err("VERIFY_FAILED".to_string())
                }
            }
            "repair" => {
                self.finish_existing_recovery(local)?;
                let report = job::verify_install_integrity(
                    Some(&self.app),
                    &local.request.game_id,
                    &local.install_path,
                    Some(local.request.version_id.clone()),
                )
                .map_err(|error| error.to_string())?;
                if report.ok {
                    return Ok(());
                }
                if self.resume_matching_job(local)? {
                    self.monitor_mutating_job(local, true)?;
                    let verified = job::verify_install_integrity(
                        Some(&self.app),
                        &local.request.game_id,
                        &local.install_path,
                        Some(local.request.version_id.clone()),
                    )
                    .map_err(|error| error.to_string())?;
                    return verified
                        .ok
                        .then_some(())
                        .ok_or_else(|| "REPAIR_VERIFY_FAILED".to_string());
                }
                let mut files = report.missing_files;
                files.extend(report.mismatched_files);
                files.sort();
                files.dedup();
                self.control.reset();
                job::spawn_repair_job(
                    self.app.clone(),
                    self.control.clone(),
                    &local.request.game_id,
                    local.install_path.display().to_string(),
                    Some(local.request.version_id.clone()),
                    files,
                )
                .map_err(|error| error.to_string())?;
                self.monitor_mutating_job(local, true)?;
                let verified = job::verify_install_integrity(
                    Some(&self.app),
                    &local.request.game_id,
                    &local.install_path,
                    Some(local.request.version_id.clone()),
                )
                .map_err(|error| error.to_string())?;
                verified
                    .ok
                    .then_some(())
                    .ok_or_else(|| "REPAIR_VERIFY_FAILED".to_string())
            }
            "launch" => job::launch_game(
                &self.app,
                &local.request.game_id,
                &local.install_path,
                local.launch_executable.clone(),
                None,
                false,
            )
            .map(|_| ())
            .map_err(|error| error.to_string()),
            _ => Err("REMOTE_ACTION_INVALID".to_string()),
        }
    }

    fn finish_existing_recovery(&self, local: &LocalRemoteJob) -> Result<(), String> {
        if local.attach_existing {
            self.monitor_mutating_job(local, false)?;
        }
        Ok(())
    }

    fn resume_matching_job(&self, local: &LocalRemoteJob) -> Result<bool, String> {
        let Some(journal) = job::read_latest_journal(&self.app)
            .map_err(|error| error.to_string())?
            .filter(|journal| journal_matches_request(journal, &local.request))
            .filter(|journal| {
                journal.resumable
                    && !matches!(journal.status, JobStatus::Committed | JobStatus::Canceled)
            })
        else {
            return Ok(false);
        };
        match journal.kind.as_str() {
            "install" => job::resume_install_job(self.app.clone(), self.control.clone(), journal),
            "update" => job::resume_update_job(self.app.clone(), self.control.clone(), journal),
            "repair" => job::resume_repair_job(self.app.clone(), self.control.clone(), journal),
            _ => return Ok(false),
        }
        .map_err(|error| error.to_string())?;
        Ok(true)
    }

    fn monitor_mutating_job(
        &self,
        local: &LocalRemoteJob,
        fail_on_failed_journal: bool,
    ) -> Result<(), String> {
        let mut last_bytes = 0_u64;
        let mut last_sample = Instant::now();
        let mut last_progress = 0_f64;
        while self.control.is_running() {
            if local.canceled.load(Ordering::Acquire) {
                self.control.cancel();
            }
            if let Ok(Some(journal)) = job::read_latest_journal(&self.app) {
                if journal.game_id == local.request.game_id {
                    if fail_on_failed_journal && journal.status == JobStatus::Failed {
                        return Err(journal
                            .logs
                            .last()
                            .map(|entry| entry.message.clone())
                            .unwrap_or_else(|| "REMOTE_JOB_FAILED".to_string()));
                    }
                    let bytes_done = journal.logical_bytes_done.max(journal.bytes_done);
                    let elapsed = last_sample.elapsed().as_secs_f64().max(0.001);
                    let speed = bytes_done.saturating_sub(last_bytes) as f64 / elapsed;
                    last_bytes = bytes_done;
                    last_sample = Instant::now();
                    last_progress = last_progress
                        .max(journal.overall_progress as f64)
                        .clamp(0.0, 0.999);
                    self.queue_message(json!({
                        "type": "remoteJob.progress",
                        "jobId": local.request.id,
                        "progress": {
                            "overallProgress": last_progress,
                            "phase": journal.phase,
                            "bytesDone": bytes_done,
                            "bytesTotal": journal.logical_bytes_total.max(journal.bytes_total),
                            "speedBytesPerSecond": speed.max(0.0) as u64,
                        }
                    }));
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
        thread::sleep(Duration::from_millis(150));
        if local.canceled.load(Ordering::Acquire) {
            return Err("REMOTE_JOB_CANCELED".to_string());
        }
        if fail_on_failed_journal {
            if let Ok(Some(journal)) = job::read_latest_journal(&self.app) {
                if journal.game_id == local.request.game_id && journal.status == JobStatus::Failed {
                    return Err(journal
                        .logs
                        .last()
                        .map(|entry| entry.message.clone())
                        .unwrap_or_else(|| "REMOTE_JOB_FAILED".to_string()));
                }
            }
        }
        Ok(())
    }

    fn require_installed_version(&self, local: &LocalRemoteJob) -> Result<(), String> {
        let state = job::game_install_state(&self.app, &local.request.game_id)
            .map_err(|error| error.to_string())?;
        if state.installed && state.current_version == local.request.version_id {
            Ok(())
        } else {
            Err("REMOTE_TARGET_VERSION_NOT_COMMITTED".to_string())
        }
    }

    fn queue_static_progress(&self, job_id: &str, phase: &str, progress: f64) {
        self.queue_message(json!({
            "type": "remoteJob.progress",
            "jobId": job_id,
            "progress": {
                "overallProgress": progress,
                "phase": phase,
                "bytesDone": 0,
                "bytesTotal": 0,
                "speedBytesPerSecond": 0,
            }
        }));
    }

    fn cancel_job(&self, job_id: &str) {
        let cancel = {
            let runtime = lock(&self.runtime);
            (runtime.active_job_id.as_deref() == Some(job_id))
                .then(|| runtime.active_cancel.clone())
                .flatten()
        };
        let Some(cancel) = cancel else {
            return;
        };
        cancel.store(true, Ordering::Release);
        self.control.cancel();
    }

    fn revoke(&self) -> Result<(), String> {
        let (device_id, _) = self.credentials()?;
        let token = crate::discord_auth::access_token_for_backend(&self.app)
            .map_err(|_| "DISCORD_AUTH_REQUIRED".to_string())?;
        let response = http_client()?
            .delete(format!(
                "{}/devices/{}/registration",
                backend_api_base(),
                device_id
            ))
            .bearer_auth(token)
            .send()
            .map_err(|error| format!("REMOTE_REVOKE_FAILED: {error}"))?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(format!(
                "REMOTE_REVOKE_REJECTED_{}",
                response.status().as_u16()
            ));
        }
        self.save_stored(|stored| {
            stored.enabled = false;
            stored.device_id.clear();
            stored.encrypted_credential.clear();
        })?;
        self.set_runtime(|runtime| {
            runtime.connected = false;
            runtime.last_error = None;
        });
        Ok(())
    }
}

pub fn initialize(app: AppHandle, control: Arc<JobControl>) -> Result<(), String> {
    if MANAGER.get().is_none() {
        let manager = Arc::new(RemoteWebManager::new(app, control)?);
        let _ = MANAGER.set(manager.clone());
        manager.start();
    }
    Ok(())
}

#[tauri::command]
pub fn get_remote_web_access_state() -> Result<RemoteWebAccessState, String> {
    Ok(manager()?.public_state())
}

#[tauri::command]
pub fn enable_remote_web_access(
    device_name: Option<String>,
) -> Result<RemoteWebAccessState, String> {
    let manager = manager()?;
    let name = sanitize_device_name(device_name.as_deref().unwrap_or(""));
    manager.save_stored(|stored| {
        stored.enabled = true;
        if !name.is_empty() {
            stored.device_name = name;
        } else if stored.device_name.is_empty() {
            stored.device_name = default_device_name();
        }
    })?;
    manager.start();
    Ok(manager.public_state())
}

#[tauri::command]
pub fn disable_remote_web_access() -> Result<RemoteWebAccessState, String> {
    let manager = manager()?;
    manager.save_stored(|stored| stored.enabled = false)?;
    manager.set_runtime(|runtime| runtime.connected = false);
    Ok(manager.public_state())
}

#[tauri::command]
pub fn revoke_remote_web_access() -> Result<RemoteWebAccessState, String> {
    let manager = manager()?;
    manager.revoke()?;
    Ok(manager.public_state())
}

fn manager() -> Result<&'static Arc<RemoteWebManager>, String> {
    MANAGER
        .get()
        .ok_or_else(|| "REMOTE_WEB_NOT_INITIALIZED".to_string())
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(STORE_FILE))
        .map_err(|error| format!("REMOTE_STORAGE_UNAVAILABLE: {error}"))
}

fn load_store(app: &AppHandle) -> Result<StoredRemoteWebAccess, String> {
    let path = store_path(app)?;
    if !path.exists() {
        return Ok(StoredRemoteWebAccess {
            schema_version: STORE_SCHEMA,
            device_name: default_device_name(),
            ..StoredRemoteWebAccess::default()
        });
    }
    let bytes = fs::read(path).map_err(|error| format!("REMOTE_STORAGE_READ_FAILED: {error}"))?;
    let mut stored: StoredRemoteWebAccess =
        serde_json::from_slice(&bytes).map_err(|_| "REMOTE_STORAGE_INVALID".to_string())?;
    if stored.device_name.is_empty() {
        stored.device_name = default_device_name();
    }
    Ok(stored)
}

fn save_store(app: &AppHandle, stored: &StoredRemoteWebAccess) -> Result<(), String> {
    let path = store_path(app)?;
    let bytes = serde_json::to_vec_pretty(stored)
        .map_err(|error| format!("REMOTE_STORAGE_SERIALIZE_FAILED: {error}"))?;
    crate::lua_live::atomic_write_path(&path, &bytes)
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!(
            "0xoLemon/",
            env!("CARGO_PKG_VERSION"),
            " RemoteWeb"
        ))
        .build()
        .map_err(|error| format!("REMOTE_HTTP_CLIENT_FAILED: {error}"))
}

fn backend_api_base() -> String {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("OXO_REMOTE_API_BASE") {
        let value = value.trim().trim_end_matches('/');
        if value.starts_with("http://127.0.0.1:") || value.starts_with("http://localhost:") {
            return value.to_string();
        }
    }
    BACKEND_API.to_string()
}

fn websocket_url() -> String {
    let base = backend_api_base();
    if let Some(value) = base.strip_prefix("https://") {
        format!("wss://{value}/devices/connect")
    } else if let Some(value) = base.strip_prefix("http://") {
        format!("ws://{value}/devices/connect")
    } else {
        format!("{base}/devices/connect")
    }
}

fn configure_socket_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<(), String> {
    let timeout = Some(SOCKET_POLL_INTERVAL);
    let result = match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(timeout),
        MaybeTlsStream::Rustls(stream) => stream.sock.set_read_timeout(timeout),
        _ => Ok(()),
    };
    result.map_err(|error| format!("REMOTE_SOCKET_CONFIG_FAILED: {error}"))
}

fn websocket_would_block(error: &WebSocketError) -> bool {
    matches!(
        error,
        WebSocketError::Io(io_error)
            if matches!(io_error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
    )
}

fn send_json(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    value: &Value,
) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|_| "REMOTE_PROTOCOL_INVALID".to_string())?;
    socket
        .send(Message::text(text))
        .map_err(|error| format!("REMOTE_SEND_FAILED: {error}"))
}

fn device_library(
    marker: LibraryRecoveryIndex,
    root: PathBuf,
    default_root: Option<&Path>,
) -> Option<DeviceLibrary> {
    let canonical = canonical_if_exists(&root)?;
    let free_bytes = fs2::free_space(&canonical).unwrap_or(0);
    let drive = canonical
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default();
    let folder = canonical
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "0xoLemon Library".to_string());
    Some(DeviceLibrary {
        id: marker.library_id,
        label: sanitize_device_name(&format!("{drive} · {folder}")),
        free_bytes,
        default: default_root.is_some_and(|value| value == canonical),
    })
}

fn library_root_by_id(library_id: &str) -> Option<PathBuf> {
    install_discovery::registered_library_roots()
        .into_iter()
        .find_map(|(marker, root)| (marker.library_id == library_id).then_some(root))
}

fn install_belongs_to_library(install_path: &Path, library_id: &str) -> bool {
    let Some(root) = library_root_by_id(library_id) else {
        return false;
    };
    let Some(install) = canonical_if_exists(install_path) else {
        return false;
    };
    let Some(common) = canonical_if_exists(&root.join("common")) else {
        return false;
    };
    install.starts_with(common)
}

fn canonical_if_exists(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok()
}

fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .map(|value| sanitize_device_name(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Windows PC".to_string())
}

fn sanitize_device_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

fn valid_identifier(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_game_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_version(value: &str) -> bool {
    !value.is_empty() && value.len() <= 160 && !value.chars().any(char::is_control)
}

fn valid_remote_job(job: &RemoteJobRequest) -> bool {
    valid_identifier(&job.id, 40, 40)
        && job.id.bytes().all(|byte| byte.is_ascii_hexdigit())
        && valid_identifier(&job.request_id, 16, 128)
        && matches!(
            job.action.as_str(),
            "install" | "update" | "downgrade" | "repair" | "verify" | "launch"
        )
        && valid_game_id(&job.game_id)
        && valid_identifier(&job.device_id, 12, 96)
        && (job.action == "launch"
            || (valid_identifier(&job.library_id, 1, 96) && valid_version(&job.version_id)))
}

fn journal_matches_request(journal: &JobJournal, request: &RemoteJobRequest) -> bool {
    let expected_kind = match request.action.as_str() {
        "install" => "install",
        "update" | "downgrade" => "update",
        "repair" => "repair",
        _ => return false,
    };
    journal.kind == expected_kind
        && journal.game_id == request.game_id
        && journal.to_version == request.version_id
}

fn safe_error(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(300)
        .collect::<String>()
}

fn error_code(error: &str) -> String {
    let value = error
        .split(':')
        .next()
        .unwrap_or("REMOTE_JOB_FAILED")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    if value.len() >= 3 {
        value
    } else {
        "REMOTE_JOB_FAILED".to_string()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{error_code, valid_game_id, valid_identifier, valid_version};

    #[test]
    fn remote_ids_reject_paths_and_urls() {
        assert!(valid_game_id("assassins-creed-black-flag"));
        assert!(!valid_game_id("..\\game"));
        assert!(!valid_game_id("https://example.com"));
        assert!(valid_identifier("library_01", 1, 96));
        assert!(!valid_identifier("E:\\Games", 1, 96));
        assert!(valid_version("1.2.0 (Build 1234)"));
        assert!(!valid_version("1.2\nmalformed"));
    }

    #[test]
    fn remote_errors_are_stable_codes() {
        assert_eq!(
            error_code("device unavailable: timed out"),
            "DEVICE_UNAVAILABLE"
        );
    }
}
