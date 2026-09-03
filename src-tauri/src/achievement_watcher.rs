use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::game_session_state::{
    AchievementEventInput, AchievementEventKind, AchievementEventSource, AchievementTransport,
};
use crate::managed_game_runtime;
use crate::notifications::{self, NewNotification};
use crate::platform;

const PROTOCOL_VERSION: u32 = 1;
const STATE_SCHEMA_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_STATE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SCHEMA_FILE_BYTES: u64 = 8 * 1024 * 1024;
const COMMAND_QUEUE_CAPACITY: usize = 256;
const RECEIPT_CACHE_CAPACITY: usize = 256;
const REPLAY_WINDOW_CAPACITY: usize = 512;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const LOOP_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AchievementCommand {
    QueryState,
    Unlock {
        achievement_id: String,
    },
    Clear {
        achievement_id: String,
    },
    SetProgress {
        achievement_id: String,
        current: u64,
        target: u64,
    },
    SetStat {
        stat_id: String,
        value: f64,
        #[serde(default)]
        integer: bool,
    },
    Flush,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AchievementCommandStatus {
    Queued,
    Acknowledged,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementCommandReceipt {
    pub session_id: String,
    pub game_id: String,
    pub request_id: String,
    pub command_id: u64,
    pub status: AchievementCommandStatus,
    pub accepted_at: String,
    pub acknowledged_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AchievementSchemaEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub hidden: bool,
    pub target: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AchievementRecord {
    pub id: String,
    pub unlocked: bool,
    pub unlocked_at: Option<String>,
    pub progress: u64,
    pub target: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementState {
    pub schema_version: u32,
    pub game_id: String,
    pub app_id: u32,
    pub session_id: Option<String>,
    pub connected: bool,
    pub transport: String,
    pub schema: Vec<AchievementSchemaEntry>,
    pub achievements: BTreeMap<String, AchievementRecord>,
    pub stats: BTreeMap<String, f64>,
    pub last_event_id: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementEvent {
    pub protocol_version: u32,
    pub session_id: String,
    pub game_id: String,
    pub app_id: u32,
    pub pid: u32,
    pub message_id: u64,
    pub source: String,
    pub event_type: String,
    pub achievement_id: Option<String>,
    pub stat_id: Option<String>,
    pub payload: Value,
    pub occurred_at: String,
}

#[derive(Debug, Clone)]
struct PendingCommand {
    message_id: u64,
    request_id: String,
    command: AchievementCommand,
    sent: bool,
}

#[derive(Debug)]
struct ProtocolState {
    next_server_command_id: u64,
    next_server_ack_id: u64,
    replay: ReplayWindow,
    pending_commands: VecDeque<PendingCommand>,
    receipts: HashMap<String, AchievementCommandReceipt>,
    receipt_order: VecDeque<String>,
}

impl Default for ProtocolState {
    fn default() -> Self {
        Self {
            next_server_command_id: 1,
            next_server_ack_id: 1,
            replay: ReplayWindow::default(),
            pending_commands: VecDeque::new(),
            receipts: HashMap::new(),
            receipt_order: VecDeque::new(),
        }
    }
}

#[derive(Debug, Default)]
struct ReplayWindow {
    highest: u64,
    order: VecDeque<u64>,
    seen: HashSet<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayDisposition {
    New,
    Duplicate,
    Stale,
}

impl ReplayWindow {
    fn observe(&mut self, message_id: u64) -> ReplayDisposition {
        if message_id == 0 {
            return ReplayDisposition::Stale;
        }
        if self.seen.contains(&message_id) {
            return ReplayDisposition::Duplicate;
        }
        if message_id <= self.highest {
            return ReplayDisposition::Stale;
        }
        self.highest = message_id;
        self.seen.insert(message_id);
        self.order.push_back(message_id);
        while self.order.len() > REPLAY_WINDOW_CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.seen.remove(&expired);
            }
        }
        ReplayDisposition::New
    }
}

struct SessionShared {
    app: AppHandle,
    game_id: String,
    app_id: u32,
    install_path: PathBuf,
    session_id: String,
    pipe_name: String,
    secret: String,
    expected_pid: AtomicU32,
    stop: AtomicBool,
    state: Mutex<AchievementState>,
    protocol: Mutex<ProtocolState>,
}

pub struct AchievementWatcher {
    shared: Arc<SessionShared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for AchievementWatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AchievementWatcher")
            .field("game_id", &self.shared.game_id)
            .field("app_id", &self.shared.app_id)
            .field("session_id", &self.shared.session_id)
            .field(
                "expected_pid",
                &self.shared.expected_pid.load(Ordering::Acquire),
            )
            .finish()
    }
}

impl AchievementWatcher {
    pub fn session_id(&self) -> &str {
        &self.shared.session_id
    }

    pub fn app_id(&self) -> u32 {
        self.shared.app_id
    }

    pub fn environment(&self) -> Vec<(String, String)> {
        vec![
            (
                "OXO_ACHIEVEMENT_PROTOCOL".to_string(),
                PROTOCOL_VERSION.to_string(),
            ),
            (
                "OXO_ACHIEVEMENT_PIPE".to_string(),
                self.shared.pipe_name.clone(),
            ),
            (
                "OXO_ACHIEVEMENT_SECRET".to_string(),
                self.shared.secret.clone(),
            ),
            (
                "OXO_ACHIEVEMENT_SESSION_ID".to_string(),
                self.shared.session_id.clone(),
            ),
            ("OXO_GAME_ID".to_string(), self.shared.game_id.clone()),
            (
                "OXO_STEAM_APP_ID".to_string(),
                self.shared.app_id.to_string(),
            ),
        ]
    }

    pub fn bind_pid(&self, pid: u32) -> Result<(), String> {
        if pid == 0 {
            return Err("Achievement runtime cannot bind process id 0".to_string());
        }
        match self
            .shared
            .expected_pid
            .compare_exchange(0, pid, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(existing) if existing == pid => Ok(()),
            Err(existing) => Err(format!(
                "Achievement runtime is already bound to process {existing}"
            )),
        }
    }
}

impl Drop for AchievementWatcher {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
        if let Ok(mut sessions) = active_sessions().lock() {
            if sessions
                .get(&self.shared.game_id)
                .and_then(Weak::upgrade)
                .is_some_and(|session| Arc::ptr_eq(&session, &self.shared))
            {
                sessions.remove(&self.shared.game_id);
            }
        }
    }
}

static ACTIVE_SESSIONS: OnceLock<Mutex<HashMap<String, Weak<SessionShared>>>> = OnceLock::new();

fn active_sessions() -> &'static Mutex<HashMap<String, Weak<SessionShared>>> {
    ACTIVE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn start_session(
    app: AppHandle,
    game_id: &str,
    app_id: Option<u32>,
    install_path: &Path,
    pid: u32,
) -> Result<AchievementWatcher, String> {
    let game_id = normalize_game_id(game_id)?;
    if !managed_game_runtime::supports_managed_achievements(&game_id) {
        return Err(format!(
            "Achievements are not enabled by the pinned runtime catalog for {game_id}"
        ));
    }
    let catalog_app_id = managed_game_runtime::catalog_app_id_for_game(&game_id)
        .ok_or_else(|| format!("The pinned runtime catalog has no AppID for {game_id}"))?;
    if app_id.is_some_and(|value| value != catalog_app_id) {
        return Err(format!(
            "Achievement AppID mismatch for {game_id}: expected {catalog_app_id}, got {}",
            app_id.unwrap_or_default()
        ));
    }
    let install_path = install_path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve achievement install directory: {error}"))?;
    if !install_path.is_dir() {
        return Err("Achievement install directory is not a directory".to_string());
    }

    let session_id = Uuid::new_v4().to_string();
    let pipe_name = format!(r"\\.\pipe\0xo-ach-{}", Uuid::new_v4().simple());
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let pipe = NativePipe::create(&pipe_name)?;
    let schema = load_achievement_schema(&install_path).unwrap_or_default();
    let shared = Arc::new(SessionShared {
        app,
        game_id: game_id.clone(),
        app_id: catalog_app_id,
        install_path,
        session_id: session_id.clone(),
        pipe_name,
        secret,
        expected_pid: AtomicU32::new(pid),
        stop: AtomicBool::new(false),
        state: Mutex::new(AchievementState {
            schema_version: STATE_SCHEMA_VERSION,
            game_id: game_id.clone(),
            app_id: catalog_app_id,
            session_id: Some(session_id),
            connected: false,
            transport: "polling".to_string(),
            schema,
            achievements: BTreeMap::new(),
            stats: BTreeMap::new(),
            last_event_id: 0,
            updated_at: Utc::now().to_rfc3339(),
        }),
        protocol: Mutex::new(ProtocolState::default()),
    });

    {
        let mut sessions = active_sessions()
            .lock()
            .map_err(|_| "Achievement session registry is unavailable".to_string())?;
        if sessions
            .get(&game_id)
            .and_then(Weak::upgrade)
            .is_some_and(|session| !session.stop.load(Ordering::Acquire))
        {
            return Err(format!(
                "An achievement session is already active for {game_id}"
            ));
        }
        sessions.insert(game_id, Arc::downgrade(&shared));
    }

    let worker_shared = Arc::clone(&shared);
    let worker = thread::Builder::new()
        .name(format!("0xo-achievement-{}", worker_shared.app_id))
        .spawn(move || run_session(worker_shared, pipe))
        .map_err(|error| format!("Could not start achievement runtime: {error}"))?;

    Ok(AchievementWatcher {
        shared,
        worker: Mutex::new(Some(worker)),
    })
}

#[tauri::command]
pub fn get_achievement_state(
    game_id: String,
    install_path: Option<String>,
) -> Result<AchievementState, String> {
    let game_id = normalize_game_id(&game_id)?;
    if let Some(session) = active_session(&game_id) {
        return session
            .state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| "Achievement state is unavailable".to_string());
    }

    let app_id = managed_game_runtime::catalog_app_id_for_game(&game_id)
        .ok_or_else(|| format!("No achievement runtime is cataloged for {game_id}"))?;
    if !managed_game_runtime::supports_managed_achievements(&game_id) {
        return Err(format!("Managed achievements are disabled for {game_id}"));
    }
    let install_path = install_path.map(PathBuf::from);
    let schema = install_path
        .as_deref()
        .and_then(|path| path.canonicalize().ok())
        .and_then(|path| load_achievement_schema(&path).ok())
        .unwrap_or_default();
    let mut state = empty_state(&game_id, app_id, schema);
    hydrate_offline_state(&mut state, install_path.as_deref())?;
    Ok(state)
}

#[tauri::command]
pub fn send_achievement_command(
    game_id: String,
    request_id: String,
    command: AchievementCommand,
) -> Result<AchievementCommandReceipt, String> {
    let game_id = normalize_game_id(&game_id)?;
    validate_request_id(&request_id)?;
    validate_command(&command)?;
    if !managed_game_runtime::supports_managed_achievements(&game_id) {
        return Err("Achievement writes are allowed only for managedGse games".to_string());
    }
    let session = active_session(&game_id)
        .ok_or_else(|| "Start the managed game before sending achievement commands".to_string())?;
    if session.expected_pid.load(Ordering::Acquire) == 0 {
        return Err("The achievement session is not bound to a game process yet".to_string());
    }

    let mut protocol = session
        .protocol
        .lock()
        .map_err(|_| "Achievement command queue is unavailable".to_string())?;
    if let Some(receipt) = protocol.receipts.get(&request_id) {
        return Ok(receipt.clone());
    }
    if protocol.pending_commands.len() >= COMMAND_QUEUE_CAPACITY {
        return Err(
            "Achievement command queue is full; retry after pending ACKs arrive".to_string(),
        );
    }
    let command_id = allocate_server_command_id(&mut protocol);
    let receipt = AchievementCommandReceipt {
        session_id: session.session_id.clone(),
        game_id: game_id.clone(),
        request_id: request_id.clone(),
        command_id,
        status: AchievementCommandStatus::Queued,
        accepted_at: Utc::now().to_rfc3339(),
        acknowledged_at: None,
        error: None,
    };
    protocol.pending_commands.push_back(PendingCommand {
        message_id: command_id,
        request_id: request_id.clone(),
        command,
        sent: false,
    });
    protocol
        .receipts
        .insert(request_id.clone(), receipt.clone());
    protocol.receipt_order.push_back(request_id);
    trim_receipt_cache(&mut protocol);
    Ok(receipt)
}

fn run_session(shared: Arc<SessionShared>, pipe: NativePipe) {
    let mut connected = false;
    let mut authenticated = false;
    let mut peer_pid = 0u32;
    let mut fallback = FallbackPoller::new(&shared);

    while !shared.stop.load(Ordering::Acquire) {
        if !connected {
            match pipe.try_connect() {
                Ok(true) => match pipe.client_pid() {
                    Ok(pid) => {
                        connected = true;
                        authenticated = false;
                        peer_pid = pid;
                        reset_pending_for_reconnect(&shared);
                    }
                    Err(error) => {
                        emit_transport_error(&shared, &error);
                        pipe.disconnect();
                    }
                },
                Ok(false) => {}
                Err(error) => {
                    emit_transport_error(&shared, &error);
                    pipe.disconnect();
                }
            }
        } else {
            match pipe.try_read() {
                Ok(PipeRead::None) => {}
                Ok(PipeRead::Disconnected) => {
                    mark_disconnected(&shared);
                    pipe.disconnect();
                    connected = false;
                    authenticated = false;
                    peer_pid = 0;
                }
                Ok(PipeRead::Message(bytes)) => {
                    match process_client_message(&shared, peer_pid, authenticated, &bytes) {
                        Ok(result) => {
                            authenticated |= result.authenticated;
                            if let Some(ack) = result.ack {
                                if let Err(error) = pipe.write_json(&ack) {
                                    emit_transport_error(&shared, &error);
                                    mark_disconnected(&shared);
                                    pipe.disconnect();
                                    connected = false;
                                    authenticated = false;
                                    peer_pid = 0;
                                }
                            }
                        }
                        Err(error) => {
                            emit_transport_error(&shared, &error);
                            mark_disconnected(&shared);
                            pipe.disconnect();
                            connected = false;
                            authenticated = false;
                            peer_pid = 0;
                        }
                    }
                }
                Err(error) => {
                    emit_transport_error(&shared, &error);
                    mark_disconnected(&shared);
                    pipe.disconnect();
                    connected = false;
                    authenticated = false;
                    peer_pid = 0;
                }
            }

            if connected && authenticated {
                if let Some(command) = next_pending_command(&shared) {
                    if let Err(error) = pipe.write_json(&command) {
                        emit_transport_error(&shared, &error);
                        mark_disconnected(&shared);
                        pipe.disconnect();
                        connected = false;
                        authenticated = false;
                        peer_pid = 0;
                    }
                }
            }
        }

        // The file provider is a scoped continuity fallback, never a second
        // achievement source while the authenticated named pipe is healthy.
        if (!connected || !authenticated) && fallback.should_poll() {
            if let Err(error) = fallback.poll(&shared) {
                eprintln!(
                    "[achievement] Fallback poll failed for {}: {error}",
                    shared.game_id
                );
            }
        }
        thread::sleep(LOOP_INTERVAL);
    }

    mark_disconnected(&shared);
    pipe.disconnect();
}

struct MessageResult {
    authenticated: bool,
    ack: Option<Value>,
}

fn process_client_message(
    shared: &Arc<SessionShared>,
    peer_pid: u32,
    authenticated: bool,
    bytes: &[u8],
) -> Result<MessageResult, String> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Achievement message size is invalid".to_string());
    }
    let envelope: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid achievement protocol JSON: {error}"))?;
    validate_envelope_identity(shared, peer_pid, &envelope)?;
    let kind = required_string(&envelope, "kind")?;
    let message_id = required_u64(&envelope, "messageId")?;

    if kind == "hello" {
        mark_connected(shared, peer_pid);
        return Ok(MessageResult {
            authenticated: true,
            ack: Some(build_ack(shared, message_id, true, None, None)?),
        });
    }
    if !authenticated {
        return Err("Achievement client sent data before an authenticated hello".to_string());
    }
    match kind {
        "event" => {
            let disposition = {
                let mut protocol = shared
                    .protocol
                    .lock()
                    .map_err(|_| "Achievement replay state is unavailable".to_string())?;
                protocol.replay.observe(message_id)
            };
            if disposition == ReplayDisposition::Stale {
                return Err(format!("Rejected stale achievement event {message_id}"));
            }
            if disposition == ReplayDisposition::Duplicate {
                return Ok(MessageResult {
                    authenticated: false,
                    ack: Some(build_ack(shared, message_id, true, None, None)?),
                });
            }
            let event = envelope
                .get("event")
                .cloned()
                .filter(Value::is_object)
                .ok_or_else(|| "Achievement event payload is missing".to_string())?;
            handle_runtime_event(shared, peer_pid, message_id, "namedPipe", event)?;
            Ok(MessageResult {
                authenticated: false,
                ack: Some(build_ack(shared, message_id, true, None, None)?),
            })
        }
        "metrics" => {
            let disposition = {
                let mut protocol = shared
                    .protocol
                    .lock()
                    .map_err(|_| "Achievement replay state is unavailable".to_string())?;
                protocol.replay.observe(message_id)
            };
            if disposition == ReplayDisposition::Stale {
                return Err(format!("Rejected stale overlay metric {message_id}"));
            }
            if disposition == ReplayDisposition::Duplicate {
                return Ok(MessageResult {
                    authenticated: false,
                    ack: Some(build_ack(shared, message_id, true, None, None)?),
                });
            }
            let metrics = envelope
                .get("metrics")
                .cloned()
                .filter(Value::is_object)
                .ok_or_else(|| "Overlay metric payload is missing".to_string())?;
            let sample: crate::game_session_state::OverlayMetricSample =
                serde_json::from_value(metrics)
                    .map_err(|error| format!("Invalid overlay metric payload: {error}"))?;
            crate::game_session_state::record_overlay_metric(
                &shared.app,
                &shared.game_id,
                &shared.session_id,
                sample,
            )?;
            Ok(MessageResult {
                authenticated: false,
                ack: Some(build_ack(shared, message_id, true, None, None)?),
            })
        }
        "ack" => {
            let ack_id = required_u64(&envelope, "ackId")?;
            let ok = envelope.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let error = envelope
                .get("error")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            acknowledge_command(shared, ack_id, ok, error)?;
            Ok(MessageResult {
                authenticated: false,
                ack: None,
            })
        }
        other => Err(format!("Unsupported achievement message kind: {other}")),
    }
}

fn validate_envelope_identity(
    shared: &SessionShared,
    peer_pid: u32,
    envelope: &Value,
) -> Result<(), String> {
    if required_u64(envelope, "protocolVersion")? != u64::from(PROTOCOL_VERSION) {
        return Err("Achievement protocol version mismatch".to_string());
    }
    if required_string(envelope, "secret")? != shared.secret {
        return Err("Achievement session secret mismatch".to_string());
    }
    if required_string(envelope, "sessionId")? != shared.session_id {
        return Err("Achievement session id mismatch".to_string());
    }
    if required_string(envelope, "gameId")? != shared.game_id {
        return Err("Achievement game id mismatch".to_string());
    }
    if required_u64(envelope, "appId")? != u64::from(shared.app_id) {
        return Err("Achievement AppID mismatch".to_string());
    }
    let claimed_pid = u32::try_from(required_u64(envelope, "pid")?)
        .map_err(|_| "Achievement PID is out of range".to_string())?;
    let expected_pid = shared.expected_pid.load(Ordering::Acquire);
    if expected_pid == 0 {
        return Err("Achievement process identity has not been bound".to_string());
    }
    if claimed_pid != expected_pid || peer_pid != expected_pid {
        return Err(format!(
            "Achievement process identity mismatch: expected {expected_pid}, claimed {claimed_pid}, connected {peer_pid}"
        ));
    }
    Ok(())
}

fn build_ack(
    shared: &SessionShared,
    ack_id: u64,
    ok: bool,
    result: Option<Value>,
    error: Option<String>,
) -> Result<Value, String> {
    let mut protocol = shared
        .protocol
        .lock()
        .map_err(|_| "Achievement protocol state is unavailable".to_string())?;
    let message_id = allocate_server_ack_id(&mut protocol);
    Ok(json!({
        "protocolVersion": PROTOCOL_VERSION,
        "kind": "ack",
        "secret": shared.secret,
        "sessionId": shared.session_id,
        "gameId": shared.game_id,
        "appId": shared.app_id,
        "pid": shared.expected_pid.load(Ordering::Acquire),
        "messageId": message_id,
        "ackId": ack_id,
        "ok": ok,
        "result": result,
        "error": error,
    }))
}

fn next_pending_command(shared: &SessionShared) -> Option<Value> {
    let mut protocol = shared.protocol.lock().ok()?;
    let pending = protocol.pending_commands.front_mut()?;
    if pending.sent {
        return None;
    }
    pending.sent = true;
    Some(json!({
        "protocolVersion": PROTOCOL_VERSION,
        "kind": "command",
        "secret": shared.secret,
        "sessionId": shared.session_id,
        "gameId": shared.game_id,
        "appId": shared.app_id,
        "pid": shared.expected_pid.load(Ordering::Acquire),
        "messageId": pending.message_id,
        "requestId": pending.request_id,
        "command": pending.command,
    }))
}

fn reset_pending_for_reconnect(shared: &SessionShared) {
    if let Ok(mut protocol) = shared.protocol.lock() {
        for pending in &mut protocol.pending_commands {
            pending.sent = false;
        }
    }
}

fn acknowledge_command(
    shared: &SessionShared,
    command_id: u64,
    ok: bool,
    error: Option<String>,
) -> Result<(), String> {
    let mut protocol = shared
        .protocol
        .lock()
        .map_err(|_| "Achievement command queue is unavailable".to_string())?;
    let Some(index) = protocol
        .pending_commands
        .iter()
        .position(|pending| pending.message_id == command_id)
    else {
        return Ok(());
    };
    let pending = protocol
        .pending_commands
        .remove(index)
        .ok_or_else(|| "Achievement command queue changed unexpectedly".to_string())?;
    let receipt = protocol
        .receipts
        .get_mut(&pending.request_id)
        .ok_or_else(|| "Achievement command receipt is missing".to_string())?;
    receipt.status = if ok {
        AchievementCommandStatus::Acknowledged
    } else {
        AchievementCommandStatus::Rejected
    };
    receipt.acknowledged_at = Some(Utc::now().to_rfc3339());
    receipt.error = error;
    let receipt = receipt.clone();
    drop(protocol);
    let _ = shared
        .app
        .emit("launcher://achievement-command-ack", receipt);
    Ok(())
}

fn allocate_server_command_id(protocol: &mut ProtocolState) -> u64 {
    let current = protocol.next_server_command_id.max(1);
    protocol.next_server_command_id = current.saturating_add(1).max(1);
    current
}

fn allocate_server_ack_id(protocol: &mut ProtocolState) -> u64 {
    let current = protocol.next_server_ack_id.max(1);
    protocol.next_server_ack_id = current.saturating_add(1).max(1);
    current
}

fn trim_receipt_cache(protocol: &mut ProtocolState) {
    while protocol.receipt_order.len() > RECEIPT_CACHE_CAPACITY {
        let Some(oldest) = protocol.receipt_order.pop_front() else {
            break;
        };
        let is_pending = protocol
            .pending_commands
            .iter()
            .any(|pending| pending.request_id == oldest);
        if is_pending {
            protocol.receipt_order.push_back(oldest);
            break;
        }
        protocol.receipts.remove(&oldest);
    }
}

fn handle_runtime_event(
    shared: &SessionShared,
    pid: u32,
    message_id: u64,
    source: &str,
    payload: Value,
) -> Result<(), String> {
    let event_type = required_string(&payload, "type")?.to_string();
    let achievement_id = payload
        .get("achievementId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let stat_id = payload
        .get("statId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let mut newly_unlocked = None;
    {
        let mut state = shared
            .state
            .lock()
            .map_err(|_| "Achievement state is unavailable".to_string())?;
        state.last_event_id = state.last_event_id.max(message_id);
        state.updated_at = Utc::now().to_rfc3339();
        match event_type.as_str() {
            "schemaReady" => apply_schema_ready(&mut state, &payload),
            "unlocked" => {
                if let Some(id) = achievement_id.as_deref() {
                    let record = state.achievements.entry(id.to_string()).or_insert_with(|| {
                        AchievementRecord {
                            id: id.to_string(),
                            ..AchievementRecord::default()
                        }
                    });
                    if !record.unlocked {
                        newly_unlocked = Some(id.to_string());
                    }
                    record.unlocked = true;
                    record.unlocked_at = payload
                        .get("unlockedAt")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .or_else(|| Some(Utc::now().to_rfc3339()));
                }
            }
            "cleared" => {
                if let Some(id) = achievement_id.as_deref() {
                    let record = state.achievements.entry(id.to_string()).or_insert_with(|| {
                        AchievementRecord {
                            id: id.to_string(),
                            ..AchievementRecord::default()
                        }
                    });
                    record.unlocked = false;
                    record.unlocked_at = None;
                }
            }
            "progress" => {
                if let Some(id) = achievement_id.as_deref() {
                    let record = state.achievements.entry(id.to_string()).or_insert_with(|| {
                        AchievementRecord {
                            id: id.to_string(),
                            ..AchievementRecord::default()
                        }
                    });
                    record.progress = payload.get("current").and_then(Value::as_u64).unwrap_or(0);
                    record.target = payload.get("target").and_then(Value::as_u64).unwrap_or(0);
                }
            }
            "statChanged" => {
                if let (Some(id), Some(value)) = (
                    stat_id.as_deref(),
                    payload.get("value").and_then(Value::as_f64),
                ) {
                    state.stats.insert(id.to_string(), value);
                }
            }
            "runtimeStopped" => {
                state.connected = false;
                state.transport = "scopedFallback".to_string();
            }
            "flushed" => {}
            _ => return Err(format!("Unsupported achievement event type: {event_type}")),
        }
    }

    let source_v2 = if source == "namedPipe" {
        AchievementEventSource::NamedPipe
    } else {
        AchievementEventSource::ScopedFallback
    };
    let kind_v2 = match event_type.as_str() {
        "schemaReady" => AchievementEventKind::Schema,
        "unlocked" => AchievementEventKind::Unlock,
        "progress" => AchievementEventKind::Progress,
        "cleared" => AchievementEventKind::Clear,
        "statChanged" => AchievementEventKind::Stat,
        "flushed" => AchievementEventKind::Flush,
        "runtimeStopped" => AchievementEventKind::RuntimeStopped,
        _ => return Err(format!("Unsupported achievement event type: {event_type}")),
    };
    let schema_metadata = achievement_id.as_deref().and_then(|id| {
        shared.state.lock().ok().and_then(|state| {
            state
                .schema
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| (entry.name.clone(), entry.description.clone()))
        })
    });
    let event_name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| schema_metadata.as_ref().map(|value| value.0.clone()));
    let event_description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| schema_metadata.as_ref().map(|value| value.1.clone()));
    let occurred_at_ms = payload
        .get("unlockedAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis());
    let v2_input = AchievementEventInput {
        source_event_id: message_id.to_string(),
        session_id: shared.session_id.clone(),
        game_id: shared.game_id.clone(),
        app_id: shared.app_id,
        achievement_id: achievement_id
            .clone()
            .or_else(|| stat_id.clone())
            .unwrap_or_default(),
        kind: kind_v2,
        name: event_name,
        description: event_description,
        icon_path: payload
            .get("iconPath")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        current: payload
            .get("current")
            .and_then(Value::as_f64)
            .or_else(|| payload.get("value").and_then(Value::as_f64)),
        maximum: payload
            .get("maximum")
            .and_then(Value::as_f64)
            .or_else(|| payload.get("target").and_then(Value::as_f64)),
        occurred_at: occurred_at_ms,
        source: source_v2,
    };
    if let Err(error) = crate::game_session_state::accept_achievement_event(&shared.app, v2_input)
    {
        eprintln!(
            "[achievement] Could not fan out event through GameSessionState for {}: {error}",
            shared.game_id
        );
    }

    let event = AchievementEvent {
        protocol_version: PROTOCOL_VERSION,
        session_id: shared.session_id.clone(),
        game_id: shared.game_id.clone(),
        app_id: shared.app_id,
        pid,
        message_id,
        source: source.to_string(),
        event_type,
        achievement_id,
        stat_id,
        payload,
        occurred_at: Utc::now().to_rfc3339(),
    };
    let _ = shared.app.emit("launcher://achievement-event", event);
    if let Some(achievement_id) = newly_unlocked {
        emit_achievement(&shared.app, &shared.game_id, &achievement_id);
    }
    Ok(())
}

fn apply_schema_ready(state: &mut AchievementState, payload: &Value) {
    if let Some(schema) = payload.get("schema").and_then(Value::as_array) {
        let parsed = schema
            .iter()
            .filter_map(|entry| {
                serde_json::from_value::<AchievementSchemaEntry>(entry.clone()).ok()
            })
            .filter(|entry| !entry.id.trim().is_empty())
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            state.schema = parsed;
        }
    }
    if let Some(achievements) = payload.get("achievements").and_then(Value::as_object) {
        for (id, value) in achievements {
            let record =
                state
                    .achievements
                    .entry(id.clone())
                    .or_insert_with(|| AchievementRecord {
                        id: id.clone(),
                        ..AchievementRecord::default()
                    });
            record.unlocked = value
                .get("earned")
                .and_then(Value::as_bool)
                .or_else(|| value.get("unlocked").and_then(Value::as_bool))
                .unwrap_or(false);
            record.progress = value.get("progress").and_then(Value::as_u64).unwrap_or(0);
            record.target = value
                .get("max_progress")
                .and_then(Value::as_u64)
                .or_else(|| value.get("target").and_then(Value::as_u64))
                .unwrap_or(0);
            record.unlocked_at = unix_timestamp_to_rfc3339(
                value
                    .get("earned_time")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
        }
    }
    if let Some(stats) = payload.get("stats").and_then(Value::as_object) {
        for (id, value) in stats {
            if let Some(value) = value.as_f64() {
                state.stats.insert(id.clone(), value);
            }
        }
    }
}

fn mark_connected(shared: &SessionShared, pid: u32) {
    if let Ok(mut state) = shared.state.lock() {
        state.connected = true;
        state.transport = "namedPipe".to_string();
        state.updated_at = Utc::now().to_rfc3339();
    }
    if let Err(error) = crate::game_session_state::mark_transport(
        &shared.app,
        &shared.game_id,
        &shared.session_id,
        AchievementTransport::NamedPipe,
        Some(pid),
    ) {
        eprintln!(
            "[achievement] Could not mark named-pipe transport for {}: {error}",
            shared.game_id
        );
    }
    let _ = shared.app.emit(
        "launcher://achievement-transport",
        json!({
            "gameId": shared.game_id,
            "appId": shared.app_id,
            "sessionId": shared.session_id,
            "pid": pid,
            "connected": true,
            "transport": "namedPipe"
        }),
    );
}

fn mark_disconnected(shared: &SessionShared) {
    let mut changed = false;
    if let Ok(mut state) = shared.state.lock() {
        changed = state.connected;
        state.connected = false;
        state.transport = "scopedFallback".to_string();
        state.updated_at = Utc::now().to_rfc3339();
    }
    if let Err(error) = crate::game_session_state::mark_transport(
        &shared.app,
        &shared.game_id,
        &shared.session_id,
        AchievementTransport::ScopedFallback,
        None,
    ) {
        eprintln!(
            "[achievement] Could not mark scoped fallback for {}: {error}",
            shared.game_id
        );
    }
    if changed {
        let _ = shared.app.emit(
            "launcher://achievement-transport",
            json!({
                "gameId": shared.game_id,
                "appId": shared.app_id,
                "sessionId": shared.session_id,
                "connected": false,
                "transport": "scopedFallback"
            }),
        );
    }
}

fn emit_transport_error(shared: &SessionShared, error: &str) {
    let _ = shared.app.emit(
        "launcher://achievement-runtime-error",
        json!({
            "gameId": shared.game_id,
            "sessionId": shared.session_id,
            "message": error
        }),
    );
}

fn active_session(game_id: &str) -> Option<Arc<SessionShared>> {
    let mut sessions = active_sessions().lock().ok()?;
    let session = sessions.get(game_id).and_then(Weak::upgrade);
    if session.is_none() {
        sessions.remove(game_id);
    }
    session.filter(|session| !session.stop.load(Ordering::Acquire))
}

fn empty_state(
    game_id: &str,
    app_id: u32,
    schema: Vec<AchievementSchemaEntry>,
) -> AchievementState {
    AchievementState {
        schema_version: STATE_SCHEMA_VERSION,
        game_id: game_id.to_string(),
        app_id,
        session_id: None,
        connected: false,
        transport: "polling".to_string(),
        schema,
        achievements: BTreeMap::new(),
        stats: BTreeMap::new(),
        last_event_id: 0,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn validate_command(command: &AchievementCommand) -> Result<(), String> {
    match command {
        AchievementCommand::QueryState | AchievementCommand::Flush => Ok(()),
        AchievementCommand::Unlock { achievement_id }
        | AchievementCommand::Clear { achievement_id } => {
            validate_entity_id("achievement", achievement_id)
        }
        AchievementCommand::SetProgress {
            achievement_id,
            current,
            target,
        } => {
            validate_entity_id("achievement", achievement_id)?;
            if *target == 0 || current > target {
                return Err(
                    "Achievement progress must satisfy 0 <= current <= target and target > 0"
                        .to_string(),
                );
            }
            if *target > u64::from(u32::MAX) {
                return Err("Achievement progress exceeds the GSE u32 range".to_string());
            }
            Ok(())
        }
        AchievementCommand::SetStat {
            stat_id,
            value,
            integer,
        } => {
            validate_entity_id("stat", stat_id)?;
            if !value.is_finite() {
                return Err("Stat value must be finite".to_string());
            }
            if *integer
                && (value.fract() != 0.0
                    || *value < f64::from(i32::MIN)
                    || *value > f64::from(i32::MAX))
            {
                return Err("Integer stat value is outside the i32 range".to_string());
            }
            Ok(())
        }
    }
}

fn validate_entity_id(kind: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("Invalid {kind} id"));
    }
    Ok(())
}

fn validate_request_id(value: &str) -> Result<(), String> {
    let parsed =
        Uuid::parse_str(value).map_err(|_| "Achievement requestId must be a UUID".to_string())?;
    if parsed.is_nil() {
        return Err("Achievement requestId cannot be nil".to_string());
    }
    Ok(())
}

fn normalize_game_id(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Invalid game id".to_string());
    }
    Ok(value)
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Achievement message field {key} is missing"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Achievement message field {key} is missing"))
}

fn emit_achievement(app: &AppHandle, game_id: &str, achievement_id: &str) {
    if let Ok(events) = platform::record_activity(app, game_id, achievement_id) {
        platform::emit_achievement_events(app, &events);
        for event in events {
            let _ = notifications::push(
                app,
                NewNotification {
                    category: "achievements".to_string(),
                    severity: "success".to_string(),
                    title: "Achievement Unlocked".to_string(),
                    message: event.name.clone(),
                    dedupe_key: format!("{}-{}", game_id, event.id),
                    entity: None,
                    action: None,
                },
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    size: u64,
    modified: Option<SystemTime>,
}

struct FallbackPoller {
    last_poll: Instant,
    initialized: bool,
    fingerprints: HashMap<PathBuf, FileFingerprint>,
    schema_map: Vec<UgsSchemaEntry>,
}

impl FallbackPoller {
    fn new(shared: &SessionShared) -> Self {
        Self {
            last_poll: Instant::now()
                .checked_sub(POLL_INTERVAL)
                .unwrap_or_else(Instant::now),
            initialized: false,
            fingerprints: HashMap::new(),
            schema_map: load_ugs_schema(&shared.install_path).unwrap_or_default(),
        }
    }

    fn should_poll(&mut self) -> bool {
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return false;
        }
        self.last_poll = Instant::now();
        true
    }

    fn poll(&mut self, shared: &SessionShared) -> Result<(), String> {
        let files = discover_state_files(shared.app_id)?;
        for path in files {
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
            if metadata.len() > MAX_STATE_FILE_BYTES {
                continue;
            }
            let fingerprint = FileFingerprint {
                size: metadata.len(),
                modified: metadata.modified().ok(),
            };
            if self.fingerprints.get(&path) == Some(&fingerprint) {
                continue;
            }
            self.fingerprints.insert(path.clone(), fingerprint);
            let records = if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("json"))
            {
                parse_achievement_json_file(&path)?
            } else if self.schema_map.is_empty() {
                BTreeMap::new()
            } else {
                let bytes = fs::read(&path)
                    .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
                parse_ugs_records(&bytes, &self.schema_map)?
            };
            apply_polled_records(shared, records, self.initialized)?;
        }
        self.initialized = true;
        Ok(())
    }
}

fn hydrate_offline_state(
    state: &mut AchievementState,
    install_path: Option<&Path>,
) -> Result<(), String> {
    let schema_map = install_path
        .and_then(|path| path.canonicalize().ok())
        .and_then(|path| load_ugs_schema(&path).ok())
        .unwrap_or_default();
    for path in discover_state_files(state.app_id)? {
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() <= MAX_STATE_FILE_BYTES => {}
            _ => continue,
        }
        let records = if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("json"))
        {
            parse_achievement_json_file(&path)?
        } else if schema_map.is_empty() {
            BTreeMap::new()
        } else {
            parse_ugs_records(
                &fs::read(&path).map_err(|error| error.to_string())?,
                &schema_map,
            )?
        };
        for (id, record) in records {
            state.achievements.insert(id, record);
        }
    }
    state.updated_at = Utc::now().to_rfc3339();
    Ok(())
}

fn apply_polled_records(
    shared: &SessionShared,
    records: BTreeMap<String, AchievementRecord>,
    emit_changes: bool,
) -> Result<(), String> {
    let mut changes = Vec::new();
    {
        let mut state = shared
            .state
            .lock()
            .map_err(|_| "Achievement state is unavailable".to_string())?;
        for (id, incoming) in records {
            let previous = state.achievements.get(&id).cloned();
            if emit_changes && previous.as_ref() != Some(&incoming) {
                state.last_event_id = state.last_event_id.saturating_add(1).max(1);
                let event_type = if incoming.unlocked
                    && !previous.as_ref().is_some_and(|value| value.unlocked)
                {
                    "unlocked"
                } else if !incoming.unlocked
                    && previous.as_ref().is_some_and(|value| value.unlocked)
                {
                    "cleared"
                } else {
                    "progress"
                };
                changes.push((
                    state.last_event_id,
                    event_type.to_string(),
                    incoming.clone(),
                ));
            }
            state.achievements.insert(id, incoming);
        }
        state.updated_at = Utc::now().to_rfc3339();
    }

    for (message_id, event_type, record) in changes {
        let payload = json!({
            "type": event_type,
            "achievementId": record.id,
            "unlockedAt": record.unlocked_at,
            "current": record.progress,
            "target": record.target
        });
        handle_runtime_event(
            shared,
            shared.expected_pid.load(Ordering::Acquire),
            message_id,
            "scopedFallback",
            payload,
        )?;
    }
    Ok(())
}

fn discover_state_files(app_id: u32) -> Result<Vec<PathBuf>, String> {
    let Some(app_data) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return Ok(Vec::new());
    };
    if app_data.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    let roots = [
        app_data.join("GSE Saves").join(app_id.to_string()),
        app_data
            .join("Goldberg SteamEmu Saves")
            .join(app_id.to_string()),
    ];
    let mut files = Vec::new();
    for root in roots {
        if !safe_state_root(&root) {
            continue;
        }
        for candidate in [
            root.join("achievements.json"),
            root.join("stats").join("achievements.json"),
        ] {
            if safe_regular_file(&candidate) {
                files.push(candidate);
            }
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("UserGameStats_")
                && name.ends_with(&format!("_{app_id}.bin"))
                && safe_regular_file(&entry.path())
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn safe_state_root(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    metadata.is_dir() && !metadata.file_type().is_symlink() && !metadata_is_reparse(&metadata)
}

fn safe_regular_file(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && !metadata_is_reparse(&metadata)
        && metadata.len() <= MAX_STATE_FILE_BYTES
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

fn parse_achievement_json_file(path: &Path) -> Result<BTreeMap<String, AchievementRecord>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
    if metadata.len() > MAX_STATE_FILE_BYTES {
        return Err(format!(
            "Achievement state file is too large: {}",
            path.display()
        ));
    }
    let value: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("Invalid achievement state {}: {error}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("Achievement state is not an object: {}", path.display()))?;
    let mut records = BTreeMap::new();
    for (id, value) in object {
        if validate_entity_id("achievement", id).is_err() {
            continue;
        }
        let unlocked = match value {
            Value::Bool(value) => *value,
            Value::Number(value) => value.as_i64().unwrap_or(0) > 0,
            Value::Object(value) => value
                .get("earned")
                .and_then(Value::as_bool)
                .or_else(|| value.get("unlocked").and_then(Value::as_bool))
                .unwrap_or(false),
            _ => false,
        };
        let (timestamp, progress, target) = value
            .as_object()
            .map(|value| {
                (
                    value
                        .get("earned_time")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    value.get("progress").and_then(Value::as_u64).unwrap_or(0),
                    value
                        .get("max_progress")
                        .and_then(Value::as_u64)
                        .or_else(|| value.get("target").and_then(Value::as_u64))
                        .unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0));
        records.insert(
            id.clone(),
            AchievementRecord {
                id: id.clone(),
                unlocked,
                unlocked_at: unix_timestamp_to_rfc3339(timestamp),
                progress,
                target,
            },
        );
    }
    Ok(records)
}

fn load_achievement_schema(install_path: &Path) -> Result<Vec<AchievementSchemaEntry>, String> {
    let path = install_path
        .join("steam_settings")
        .join("achievements.json");
    if !safe_regular_file(&path) {
        return Ok(Vec::new());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SCHEMA_FILE_BYTES {
        return Err("Achievement schema file is too large".to_string());
    }
    let value: Value = serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid achievement schema: {error}"))?;
    let entries = value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("name")?.as_str()?.trim().to_string();
            if validate_entity_id("achievement", &id).is_err() {
                return None;
            }
            let name = localized_schema_string(&entry, "displayName").unwrap_or_else(|| id.clone());
            let description = localized_schema_string(&entry, "description").unwrap_or_default();
            let hidden = entry
                .get("hidden")
                .and_then(|value| {
                    value
                        .as_bool()
                        .or_else(|| value.as_u64().map(|value| value != 0))
                        .or_else(|| value.as_str().map(|value| value == "1"))
                })
                .unwrap_or(false);
            let target = entry
                .pointer("/progress/max_val")
                .and_then(json_u64)
                .unwrap_or(0);
            Some(AchievementSchemaEntry {
                id,
                name,
                description,
                hidden,
                target,
            })
        })
        .collect();
    Ok(entries)
}

fn localized_schema_string(entry: &Value, key: &str) -> Option<String> {
    let value = entry.get(key)?;
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    let object = value.as_object()?;
    object
        .get("english")
        .and_then(Value::as_str)
        .or_else(|| object.values().find_map(Value::as_str))
        .map(ToOwned::to_owned)
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

#[derive(Debug, Clone)]
struct UgsSchemaEntry {
    id: String,
    group: String,
    bit: u32,
}

fn load_ugs_schema(install_path: &Path) -> Result<Vec<UgsSchemaEntry>, String> {
    let path = install_path
        .join("steam_settings")
        .join("achievements.json");
    if !safe_regular_file(&path) {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid UGS achievement schema: {error}"))?;
    Ok(value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("name")?.as_str()?.to_string();
            let group = entry.get("_group").and_then(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| value.as_u64().map(|value| value.to_string()))
            })?;
            let bit = entry.get("_bit").and_then(json_u64)?;
            let bit = u32::try_from(bit).ok()?;
            (bit < 32).then_some(UgsSchemaEntry { id, group, bit })
        })
        .collect())
}

#[derive(Debug, Clone)]
enum VdfValue {
    Object(Vec<(String, VdfValue)>),
    String,
    Int32(u32),
    Float32,
    UInt64,
    Int64,
}

struct VdfReader<'a> {
    bytes: &'a [u8],
    position: usize,
    entries: usize,
}

impl<'a> VdfReader<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let value = *self
            .bytes
            .get(self.position)
            .ok_or_else(|| "Unexpected end of binary VDF".to_string())?;
        self.position += 1;
        Ok(value)
    }

    fn cstring(&mut self) -> Result<String, String> {
        let start = self.position;
        let relative_end = self.bytes[start..]
            .iter()
            .position(|value| *value == 0)
            .ok_or_else(|| "Unterminated binary VDF string".to_string())?;
        let end = start + relative_end;
        self.position = end + 1;
        Ok(String::from_utf8_lossy(&self.bytes[start..end]).to_string())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "Binary VDF value exceeds file bounds".to_string())?;
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }

    fn wide_string(&mut self) -> Result<(), String> {
        loop {
            let pair = self.take(2)?;
            if pair == [0, 0] {
                return Ok(());
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Vec<(String, VdfValue)>, String> {
        if depth > 32 {
            return Err("Binary VDF nesting is too deep".to_string());
        }
        let mut entries = Vec::new();
        loop {
            let value_type = self.byte()?;
            if matches!(value_type, 0x08 | 0x0b) {
                return Ok(entries);
            }
            self.entries += 1;
            if self.entries > 100_000 {
                return Err("Binary VDF has too many entries".to_string());
            }
            let key = self.cstring()?;
            let value = match value_type {
                0x00 => VdfValue::Object(self.object(depth + 1)?),
                0x01 => {
                    let _ = self.cstring()?;
                    VdfValue::String
                }
                0x02 | 0x04 | 0x06 => {
                    let bytes: [u8; 4] = self
                        .take(4)?
                        .try_into()
                        .map_err(|_| "Invalid binary VDF int32".to_string())?;
                    VdfValue::Int32(u32::from_le_bytes(bytes))
                }
                0x03 => {
                    let _ = self.take(4)?;
                    VdfValue::Float32
                }
                0x05 => {
                    self.wide_string()?;
                    VdfValue::String
                }
                0x07 => {
                    let _ = self.take(8)?;
                    VdfValue::UInt64
                }
                0x0a => {
                    let _ = self.take(8)?;
                    VdfValue::Int64
                }
                other => return Err(format!("Unsupported binary VDF type 0x{other:02x}")),
            };
            entries.push((key, value));
        }
    }
}

fn parse_binary_vdf(bytes: &[u8]) -> Result<VdfValue, String> {
    if bytes.is_empty() || bytes.len() > MAX_STATE_FILE_BYTES as usize {
        return Err("Binary VDF size is invalid".to_string());
    }
    let mut reader = VdfReader {
        bytes,
        position: 0,
        entries: 0,
    };
    if reader.byte()? != 0x00 {
        return Err("Binary VDF root is not an object".to_string());
    }
    let _root_key = reader.cstring()?;
    Ok(VdfValue::Object(reader.object(0)?))
}

fn parse_ugs_records(
    bytes: &[u8],
    schema: &[UgsSchemaEntry],
) -> Result<BTreeMap<String, AchievementRecord>, String> {
    let root = parse_binary_vdf(bytes)?;
    let VdfValue::Object(root) = root else {
        return Err("UGS root is invalid".to_string());
    };
    let mut groups = HashMap::<String, (u32, HashMap<u32, u32>)>::new();
    for (group_id, value) in root {
        let VdfValue::Object(entries) = value else {
            continue;
        };
        let mut data = 0u32;
        let mut times = HashMap::new();
        for (key, value) in entries {
            match (key.as_str(), value) {
                ("data", VdfValue::Int32(value)) => data = value,
                ("AchievementTimes", VdfValue::Object(entries)) => {
                    for (bit, value) in entries {
                        if let (Ok(bit), VdfValue::Int32(timestamp)) = (bit.parse::<u32>(), value) {
                            times.insert(bit, timestamp);
                        }
                    }
                }
                _ => {}
            }
        }
        groups.insert(group_id, (data, times));
    }
    let mut records = BTreeMap::new();
    for entry in schema {
        let (data, times) = groups
            .get(&entry.group)
            .cloned()
            .unwrap_or_else(|| (0, HashMap::new()));
        let unlocked = data & (1u32 << entry.bit) != 0;
        let timestamp = times.get(&entry.bit).copied().unwrap_or(0);
        records.insert(
            entry.id.clone(),
            AchievementRecord {
                id: entry.id.clone(),
                unlocked,
                unlocked_at: unix_timestamp_to_rfc3339(u64::from(timestamp)),
                progress: 0,
                target: 0,
            },
        );
    }
    Ok(records)
}

fn unix_timestamp_to_rfc3339(timestamp: u64) -> Option<String> {
    if timestamp == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(i64::try_from(timestamp).ok()?, 0)
        .map(|value| value.to_rfc3339())
}

enum PipeRead {
    None,
    Message(Vec<u8>),
    Disconnected,
}

#[cfg(windows)]
struct NativePipe {
    handle: winapi::um::winnt::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for NativePipe {}

#[cfg(windows)]
impl NativePipe {
    fn create(name: &str) -> Result<Self, String> {
        use std::ffi::c_void;
        use std::mem::{size_of, zeroed};
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use winapi::shared::minwindef::{DWORD, FALSE};
        use winapi::shared::sddl::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        };
        use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
        use winapi::um::minwinbase::SECURITY_ATTRIBUTES;
        use winapi::um::namedpipeapi::CreateNamedPipeW;
        use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
        use winapi::um::securitybaseapi::GetTokenInformation;
        use winapi::um::winbase::{
            LocalFree, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, PIPE_NOWAIT,
            PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE,
        };
        use winapi::um::winnt::{TokenUser, HANDLE, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER};

        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == FALSE {
                return Err(format!(
                    "Cannot open the launcher user token: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut required: DWORD = 0;
            let _ = GetTokenInformation(token, TokenUser, null_mut(), 0, &mut required);
            if required == 0 {
                CloseHandle(token);
                return Err("Cannot size the launcher user token".to_string());
            }
            let word_count = (required as usize + size_of::<usize>() - 1) / size_of::<usize>();
            let mut token_buffer = vec![0usize; word_count];
            if GetTokenInformation(
                token,
                TokenUser,
                token_buffer.as_mut_ptr().cast::<c_void>(),
                required,
                &mut required,
            ) == FALSE
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(token);
                return Err(format!("Cannot read the launcher user token: {error}"));
            }
            let token_user = &*(token_buffer.as_ptr().cast::<TOKEN_USER>());
            let mut sid_string = null_mut();
            if ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string) == FALSE {
                let error = std::io::Error::last_os_error();
                CloseHandle(token);
                return Err(format!("Cannot encode the launcher user SID: {error}"));
            }
            let mut sid_length = 0usize;
            while *sid_string.add(sid_length) != 0 {
                sid_length += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(sid_string, sid_length));
            LocalFree(sid_string.cast::<c_void>());
            CloseHandle(token);

            let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})");
            let mut sddl_wide: Vec<u16> = std::ffi::OsStr::new(&sddl).encode_wide().collect();
            sddl_wide.push(0);
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                u32::from(SDDL_REVISION_1),
                &mut descriptor,
                null_mut(),
            ) == FALSE
            {
                return Err(format!(
                    "Cannot create the achievement pipe ACL: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut attributes: SECURITY_ATTRIBUTES = zeroed();
            attributes.nLength = size_of::<SECURITY_ATTRIBUTES>() as DWORD;
            attributes.lpSecurityDescriptor = descriptor;
            attributes.bInheritHandle = FALSE;
            let mut wide_name: Vec<u16> = std::ffi::OsStr::new(name).encode_wide().collect();
            wide_name.push(0);
            let handle = CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE
                    | PIPE_READMODE_MESSAGE
                    | PIPE_NOWAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE_BYTES as DWORD,
                MAX_MESSAGE_BYTES as DWORD,
                0,
                &mut attributes,
            );
            LocalFree(descriptor.cast::<c_void>());
            if handle == INVALID_HANDLE_VALUE {
                return Err(format!(
                    "Cannot create the authenticated achievement pipe: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self { handle })
        }
    }

    fn try_connect(&self) -> Result<bool, String> {
        use std::ptr::null_mut;
        use winapi::shared::minwindef::FALSE;
        use winapi::shared::winerror::{ERROR_NO_DATA, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING};
        use winapi::um::namedpipeapi::{ConnectNamedPipe, DisconnectNamedPipe};

        unsafe {
            if ConnectNamedPipe(self.handle, null_mut()) != FALSE {
                return Ok(true);
            }
            match std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default() as u32
            {
                ERROR_PIPE_CONNECTED => Ok(true),
                ERROR_PIPE_LISTENING => Ok(false),
                ERROR_NO_DATA => {
                    DisconnectNamedPipe(self.handle);
                    Ok(false)
                }
                _ => Err(format!(
                    "Achievement pipe connect failed: {}",
                    std::io::Error::last_os_error()
                )),
            }
        }
    }

    fn client_pid(&self) -> Result<u32, String> {
        use winapi::shared::minwindef::FALSE;
        use winapi::um::winbase::GetNamedPipeClientProcessId;
        let mut pid = 0u32;
        unsafe {
            if GetNamedPipeClientProcessId(self.handle, &mut pid) == FALSE || pid == 0 {
                return Err(format!(
                    "Cannot verify the achievement pipe client PID: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(pid)
    }

    fn try_read(&self) -> Result<PipeRead, String> {
        use std::ptr::null_mut;
        use winapi::shared::minwindef::{DWORD, FALSE};
        use winapi::shared::winerror::{
            ERROR_BROKEN_PIPE, ERROR_MORE_DATA, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED,
        };
        use winapi::um::fileapi::ReadFile;
        use winapi::um::namedpipeapi::PeekNamedPipe;

        unsafe {
            let mut available: DWORD = 0;
            if PeekNamedPipe(
                self.handle,
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            ) == FALSE
            {
                let error = std::io::Error::last_os_error();
                return match error.raw_os_error().unwrap_or_default() as u32 {
                    ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA => {
                        Ok(PipeRead::Disconnected)
                    }
                    _ => Err(format!("Achievement pipe peek failed: {error}")),
                };
            }
            if available == 0 {
                return Ok(PipeRead::None);
            }
            if available as usize > MAX_MESSAGE_BYTES {
                return Err("Achievement pipe message exceeds 64 KiB".to_string());
            }
            let mut buffer = vec![0u8; available as usize];
            let mut read: DWORD = 0;
            if ReadFile(
                self.handle,
                buffer.as_mut_ptr().cast(),
                available,
                &mut read,
                null_mut(),
            ) == FALSE
            {
                let error = std::io::Error::last_os_error();
                return match error.raw_os_error().unwrap_or_default() as u32 {
                    ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA => {
                        Ok(PipeRead::Disconnected)
                    }
                    ERROR_MORE_DATA => Err("Achievement pipe message exceeds 64 KiB".to_string()),
                    _ => Err(format!("Achievement pipe read failed: {error}")),
                };
            }
            buffer.truncate(read as usize);
            Ok(PipeRead::Message(buffer))
        }
    }

    fn write_json(&self, value: &Value) -> Result<(), String> {
        use std::ptr::null_mut;
        use winapi::shared::minwindef::{DWORD, FALSE};
        use winapi::um::fileapi::WriteFile;
        let bytes = serde_json::to_vec(value)
            .map_err(|error| format!("Cannot encode achievement message: {error}"))?;
        if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
            return Err("Achievement response size is invalid".to_string());
        }
        let mut written: DWORD = 0;
        unsafe {
            if WriteFile(
                self.handle,
                bytes.as_ptr().cast(),
                bytes.len() as DWORD,
                &mut written,
                null_mut(),
            ) == FALSE
                || written as usize != bytes.len()
            {
                return Err(format!(
                    "Achievement pipe write failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(())
    }

    fn disconnect(&self) {
        unsafe {
            winapi::um::namedpipeapi::DisconnectNamedPipe(self.handle);
        }
    }
}

#[cfg(windows)]
impl Drop for NativePipe {
    fn drop(&mut self) {
        unsafe {
            winapi::um::handleapi::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
struct NativePipe;

#[cfg(not(windows))]
impl NativePipe {
    fn create(_name: &str) -> Result<Self, String> {
        Err("Managed achievement pipes are supported only on Windows".to_string())
    }

    fn try_connect(&self) -> Result<bool, String> {
        Ok(false)
    }

    fn client_pid(&self) -> Result<u32, String> {
        Err("Managed achievement pipes are supported only on Windows".to_string())
    }

    fn try_read(&self) -> Result<PipeRead, String> {
        Ok(PipeRead::None)
    }

    fn write_json(&self, _value: &Value) -> Result<(), String> {
        Err("Managed achievement pipes are supported only on Windows".to_string())
    }

    fn disconnect(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_window_accepts_new_replays_duplicates_and_rejects_stale_ids() {
        let mut replay = ReplayWindow::default();
        assert_eq!(replay.observe(1), ReplayDisposition::New);
        assert_eq!(replay.observe(1), ReplayDisposition::Duplicate);
        assert_eq!(replay.observe(3), ReplayDisposition::New);
        assert_eq!(replay.observe(2), ReplayDisposition::Stale);
        assert_eq!(replay.observe(0), ReplayDisposition::Stale);
    }

    #[test]
    fn commands_are_strictly_validated() {
        assert!(validate_command(&AchievementCommand::SetProgress {
            achievement_id: "ACH_ONE".to_string(),
            current: 2,
            target: 1,
        })
        .is_err());
        assert!(validate_command(&AchievementCommand::SetStat {
            stat_id: "score".to_string(),
            value: f64::NAN,
            integer: false,
        })
        .is_err());
        assert!(validate_command(&AchievementCommand::Unlock {
            achievement_id: "ACH_ONE".to_string(),
        })
        .is_ok());
    }

    #[test]
    fn binary_vdf_parser_maps_ugs_bitmask_and_timestamp() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x00]);
        bytes.extend_from_slice(b"cache\0");
        bytes.extend_from_slice(&[0x00]);
        bytes.extend_from_slice(b"7\0");
        bytes.extend_from_slice(&[0x02]);
        bytes.extend_from_slice(b"data\0");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0x00]);
        bytes.extend_from_slice(b"AchievementTimes\0");
        bytes.extend_from_slice(&[0x02]);
        bytes.extend_from_slice(b"1\0");
        bytes.extend_from_slice(&1_700_000_000u32.to_le_bytes());
        bytes.extend_from_slice(&[0x08, 0x08, 0x08]);

        let records = parse_ugs_records(
            &bytes,
            &[UgsSchemaEntry {
                id: "ACH_ONE".to_string(),
                group: "7".to_string(),
                bit: 1,
            }],
        )
        .unwrap();
        let record = records.get("ACH_ONE").unwrap();
        assert!(record.unlocked);
        assert!(record.unlocked_at.is_some());
    }
}
