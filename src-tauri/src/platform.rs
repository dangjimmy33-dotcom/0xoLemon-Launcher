use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const PLATFORM_STATE_FILE: &str = "platform-state.json";
const PLATFORM_STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadProfile {
    Eco,
    Balanced,
    Turbo,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GameUpdateMode {
    Automatic,
    Scheduled,
    Manual,
}

impl Default for GameUpdateMode {
    fn default() -> Self {
        Self::Automatic
    }
}

impl Default for DownloadProfile {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LauncherSettings {
    pub default_library: String,
    pub download_workers: usize,
    pub download_retries: u32,
    pub pack_range_mb: u64,
    pub keep_chunk_cache: bool,
    pub notifications_enabled: bool,
    pub auto_verify_after_install: bool,
    pub download_profile: DownloadProfile,
    pub download_queue_mb: u64,
    pub direct_to_staging: bool,
    pub cloud_save_root: String,
    pub game_update_mode: GameUpdateMode,
    pub game_update_schedule_start: String,
    pub game_update_schedule_end: String,
    /// HuggingFace repo ID hosting depot manifests and keys.
    /// Format: "owner/repo-name"  e.g. "dangjimmy33-dotcom/depots"
    #[serde(default)]
    pub depot_hf_repo_id: String,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            default_library: r"E:\0xoLemon store".to_string(),
            download_workers: 8,
            download_retries: 5,
            pack_range_mb: 16,
            keep_chunk_cache: true,
            notifications_enabled: true,
            auto_verify_after_install: false,
            download_profile: DownloadProfile::Balanced,
            download_queue_mb: 128,
            direct_to_staging: true,
            cloud_save_root: String::new(),
            game_update_mode: GameUpdateMode::Automatic,
            game_update_schedule_start: "02:00".to_string(),
            game_update_schedule_end: "06:00".to_string(),
            depot_hf_repo_id: String::new(),
        }
    }
}

impl LauncherSettings {
    fn sanitized(mut self) -> Self {
        self.default_library = self.default_library.trim().to_string();
        if self.default_library.is_empty() {
            self.default_library = LauncherSettings::default().default_library;
        }
        let (workers, queue_mb) = match self.download_profile {
            DownloadProfile::Eco => (4, 64),
            DownloadProfile::Balanced => (8, 128),
            DownloadProfile::Turbo => (12, 256),
        };
        self.download_workers = workers;
        self.download_queue_mb = queue_mb;
        self.download_retries = self.download_retries.clamp(0, 12);
        self.pack_range_mb = self.pack_range_mb.clamp(8, 64);
        self.cloud_save_root = self.cloud_save_root.trim().to_string();
        if !valid_clock_time(&self.game_update_schedule_start) {
            self.game_update_schedule_start = "02:00".to_string();
        }
        if !valid_clock_time(&self.game_update_schedule_end) {
            self.game_update_schedule_end = "06:00".to_string();
        }
        self
    }
}

fn valid_clock_time(value: &str) -> bool {
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    hour.parse::<u8>().is_ok_and(|value| value < 24)
        && minute.parse::<u8>().is_ok_and(|value| value < 60)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub game_id: String,
    pub install_path: String,
    pub version: String,
    pub launch_executable: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRuntimeState {
    pub game_id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub total_playtime_seconds: u64,
    pub current_session_started_at: Option<String>,
    pub last_played_at: Option<String>,
    pub launch_count: u64,
}

impl GameRuntimeState {
    fn new(game_id: &str) -> Self {
        Self {
            game_id: game_id.to_string(),
            running: false,
            pid: None,
            total_playtime_seconds: 0,
            current_session_started_at: None,
            last_played_at: None,
            launch_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformAchievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unlocked: bool,
    pub unlocked_at: Option<String>,
    pub progress: u64,
    pub target: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlatformState {
    pub game_id: String,
    pub runtime: GameRuntimeState,
    pub achievements: Vec<PlatformAchievement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementUnlockedEvent {
    pub game_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub unlocked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStartedEvent {
    pub game_id: String,
    pub pid: u32,
    pub executable: String,
    pub install_path: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameExitedEvent {
    pub game_id: String,
    pub pid: u32,
    pub exit_code: Option<i32>,
    pub session_seconds: u64,
    pub total_playtime_seconds: u64,
    pub exited_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredAchievement {
    unlocked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformStateFile {
    schema_version: u32,
    #[serde(default)]
    settings: LauncherSettings,
    #[serde(default)]
    installs: HashMap<String, InstallRecord>,
    #[serde(default)]
    runtime: HashMap<String, GameRuntimeState>,
    #[serde(default)]
    achievements: HashMap<String, HashMap<String, StoredAchievement>>,
}

impl Default for PlatformStateFile {
    fn default() -> Self {
        Self {
            schema_version: PLATFORM_STATE_SCHEMA,
            settings: LauncherSettings::default(),
            installs: HashMap::new(),
            runtime: HashMap::new(),
            achievements: HashMap::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct AchievementDefinition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    target: u64,
    progress: fn(&GameRuntimeState) -> u64,
}

fn zero_progress(_: &GameRuntimeState) -> u64 {
    0
}

fn launch_progress(runtime: &GameRuntimeState) -> u64 {
    runtime.launch_count
}

fn playtime_progress(runtime: &GameRuntimeState) -> u64 {
    runtime.total_playtime_seconds
}

const ACHIEVEMENT_DEFINITIONS: &[AchievementDefinition] = &[
    AchievementDefinition {
        id: "FIRST_INSTALL",
        name: "Ready to play",
        description: "Install this game through the launcher.",
        target: 1,
        progress: zero_progress,
    },
    AchievementDefinition {
        id: "FIRST_LAUNCH",
        name: "First launch",
        description: "Start this game for the first time.",
        target: 1,
        progress: launch_progress,
    },
    AchievementDefinition {
        id: "PLAY_1_HOUR",
        name: "Settling in",
        description: "Play this game for one hour.",
        target: 60 * 60,
        progress: playtime_progress,
    },
    AchievementDefinition {
        id: "PLAY_10_HOURS",
        name: "Dedicated player",
        description: "Play this game for ten hours.",
        target: 10 * 60 * 60,
        progress: playtime_progress,
    },
    AchievementDefinition {
        id: "FIRST_VERIFY",
        name: "Integrity checked",
        description: "Verify this game's files successfully.",
        target: 1,
        progress: zero_progress,
    },
    AchievementDefinition {
        id: "FIRST_REPAIR",
        name: "Back in shape",
        description: "Repair this game's files successfully.",
        target: 1,
        progress: zero_progress,
    },
    AchievementDefinition {
        id: "FIRST_UPDATE",
        name: "Up to date",
        description: "Complete the first game update.",
        target: 1,
        progress: zero_progress,
    },
    AchievementDefinition {
        id: "FIRST_ROLLBACK",
        name: "Time traveller",
        description: "Roll this game back to an older version.",
        target: 1,
        progress: zero_progress,
    },
];

static LIVE_SETTINGS: OnceLock<RwLock<LauncherSettings>> = OnceLock::new();
static STATE_LOCK: OnceLock<RwLock<()>> = OnceLock::new();

fn state_lock() -> &'static RwLock<()> {
    STATE_LOCK.get_or_init(|| RwLock::new(()))
}

fn live_settings() -> &'static RwLock<LauncherSettings> {
    LIVE_SETTINGS.get_or_init(|| RwLock::new(LauncherSettings::default()))
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    let mut state = load_state(app).unwrap_or_default();
    for runtime in state.runtime.values_mut() {
        runtime.running = false;
        runtime.pid = None;
        runtime.current_session_started_at = None;
    }
    write_state_unlocked(app, &state)?;
    if let Ok(mut settings) = live_settings().write() {
        *settings = state.settings.clone().sanitized();
    }
    Ok(())
}

pub fn current_settings() -> LauncherSettings {
    live_settings()
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default()
}

pub fn get_settings(app: &AppHandle) -> Result<LauncherSettings, String> {
    let settings = load_state(app)?.settings.sanitized();
    if let Ok(mut live) = live_settings().write() {
        *live = settings.clone();
    }
    Ok(settings)
}

pub fn set_settings(
    app: &AppHandle,
    settings: LauncherSettings,
) -> Result<LauncherSettings, String> {
    let settings = settings.sanitized();
    update_state(app, |state| {
        state.settings = settings.clone();
    })?;
    if let Ok(mut live) = live_settings().write() {
        *live = settings.clone();
    }
    Ok(settings)
}

pub fn register_install(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    version: &str,
    launch_executable: &str,
) -> Result<(), String> {
    let game_id = normalize_game_id(game_id);
    let record = InstallRecord {
        game_id: game_id.clone(),
        install_path: install_path.display().to_string(),
        version: version.to_string(),
        launch_executable: launch_executable.to_string(),
        updated_at: Utc::now().to_rfc3339(),
    };
    update_state(app, |state| {
        state.installs.insert(game_id, record);
    })
}

pub fn unregister_install(app: &AppHandle, game_id: &str) -> Result<(), String> {
    let game_id = normalize_game_id(game_id);
    update_state(app, |state| {
        state.installs.remove(&game_id);
    })
}

pub fn install_record(app: &AppHandle, game_id: &str) -> Result<Option<InstallRecord>, String> {
    let game_id = normalize_game_id(game_id);
    Ok(load_state(app)?.installs.get(&game_id).cloned())
}

pub fn install_records(app: &AppHandle) -> Result<Vec<InstallRecord>, String> {
    Ok(load_state(app)?.installs.into_values().collect())
}

pub fn registered_install_path(app: &AppHandle, game_id: &str) -> Result<Option<PathBuf>, String> {
    Ok(install_record(app, game_id)?
        .map(|record| PathBuf::from(record.install_path))
        .filter(|path| path.exists()))
}

pub fn get_game_platform_state(
    app: &AppHandle,
    game_id: &str,
) -> Result<GamePlatformState, String> {
    let game_id = normalize_game_id(game_id);
    let state = load_state(app)?;
    Ok(build_game_platform_state(&state, &game_id))
}

pub fn get_runtime_states(app: &AppHandle) -> Result<Vec<GameRuntimeState>, String> {
    let state = load_state(app)?;
    let mut values = state.runtime.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| a.game_id.cmp(&b.game_id));
    Ok(values)
}

pub fn begin_game_session(
    app: &AppHandle,
    game_id: &str,
    pid: u32,
    install_path: &Path,
    executable: &Path,
) -> Result<(GameStartedEvent, Vec<AchievementUnlockedEvent>), String> {
    let game_id = normalize_game_id(game_id);
    let now = Utc::now().to_rfc3339();
    let mut unlocked = Vec::new();
    update_state(app, |state| {
        {
            let runtime = state
                .runtime
                .entry(game_id.clone())
                .or_insert_with(|| GameRuntimeState::new(&game_id));
            runtime.running = true;
            runtime.pid = Some(pid);
            runtime.current_session_started_at = Some(now.clone());
            runtime.last_played_at = Some(now.clone());
            runtime.launch_count = runtime.launch_count.saturating_add(1);
        }
        if let Some(event) = unlock_in_state(state, &game_id, "FIRST_LAUNCH") {
            unlocked.push(event);
        }
    })?;

    Ok((
        GameStartedEvent {
            game_id,
            pid,
            executable: executable.display().to_string(),
            install_path: install_path.display().to_string(),
            started_at: now,
        },
        unlocked,
    ))
}

pub fn end_game_session(
    app: &AppHandle,
    game_id: &str,
    pid: u32,
    session_seconds: u64,
    exit_code: Option<i32>,
) -> Result<(GameExitedEvent, Vec<AchievementUnlockedEvent>), String> {
    let game_id = normalize_game_id(game_id);
    let now = Utc::now().to_rfc3339();
    let mut unlocked = Vec::new();
    let mut total_playtime_seconds = session_seconds;
    update_state(app, |state| {
        let (unlock_one_hour, unlock_ten_hours) = {
            let runtime = state
                .runtime
                .entry(game_id.clone())
                .or_insert_with(|| GameRuntimeState::new(&game_id));
            runtime.running = false;
            runtime.pid = None;
            runtime.current_session_started_at = None;
            runtime.last_played_at = Some(now.clone());
            runtime.total_playtime_seconds = runtime
                .total_playtime_seconds
                .saturating_add(session_seconds);
            total_playtime_seconds = runtime.total_playtime_seconds;
            (
                runtime.total_playtime_seconds >= 60 * 60,
                runtime.total_playtime_seconds >= 10 * 60 * 60,
            )
        };
        if unlock_one_hour {
            if let Some(event) = unlock_in_state(state, &game_id, "PLAY_1_HOUR") {
                unlocked.push(event);
            }
        }
        if unlock_ten_hours {
            if let Some(event) = unlock_in_state(state, &game_id, "PLAY_10_HOURS") {
                unlocked.push(event);
            }
        }
    })?;

    Ok((
        GameExitedEvent {
            game_id,
            pid,
            exit_code,
            session_seconds,
            total_playtime_seconds,
            exited_at: now,
        },
        unlocked,
    ))
}

pub fn record_activity(
    app: &AppHandle,
    game_id: &str,
    achievement_id: &str,
) -> Result<Vec<AchievementUnlockedEvent>, String> {
    let game_id = normalize_game_id(game_id);
    let mut unlocked = Vec::new();
    update_state(app, |state| {
        if let Some(event) = unlock_in_state(state, &game_id, achievement_id) {
            unlocked.push(event);
        }
    })?;
    Ok(unlocked)
}

pub fn emit_achievement_events(app: &AppHandle, events: &[AchievementUnlockedEvent]) {
    if !current_settings().notifications_enabled {
        return;
    }
    for event in events {
        let _ = app.emit("launcher://achievement-unlocked", event.clone());
    }
}

fn build_game_platform_state(state: &PlatformStateFile, game_id: &str) -> GamePlatformState {
    let runtime = state
        .runtime
        .get(game_id)
        .cloned()
        .unwrap_or_else(|| GameRuntimeState::new(game_id));
    let stored = state.achievements.get(game_id);
    let mut achievements: Vec<PlatformAchievement> = ACHIEVEMENT_DEFINITIONS
        .iter()
        .map(|definition| {
            let unlocked_at = stored
                .and_then(|achievements| achievements.get(definition.id))
                .and_then(|achievement| achievement.unlocked_at.clone());
            let progress = if unlocked_at.is_some() {
                definition.target
            } else {
                (definition.progress)(&runtime).min(definition.target)
            };
            PlatformAchievement {
                id: definition.id.to_string(),
                name: definition.name.to_string(),
                description: definition.description.to_string(),
                unlocked: unlocked_at.is_some(),
                unlocked_at,
                progress,
                target: definition.target,
            }
        })
        .collect();

    if let Some(stored_achievements) = stored {
        for (ach_id, ach_state) in stored_achievements {
            if !ACHIEVEMENT_DEFINITIONS.iter().any(|d| d.id == ach_id) {
                achievements.push(PlatformAchievement {
                    id: ach_id.clone(),
                    name: ach_id.clone(),
                    description: "".to_string(),
                    unlocked: ach_state.unlocked_at.is_some(),
                    unlocked_at: ach_state.unlocked_at.clone(),
                    progress: if ach_state.unlocked_at.is_some() { 1 } else { 0 },
                    target: 1,
                });
            }
        }
    }

    GamePlatformState {
        game_id: game_id.to_string(),
        runtime,
        achievements,
    }
}

fn unlock_in_state(
    state: &mut PlatformStateFile,
    game_id: &str,
    achievement_id: &str,
) -> Option<AchievementUnlockedEvent> {
    let definition = ACHIEVEMENT_DEFINITIONS
        .iter()
        .find(|definition| definition.id == achievement_id);
    let achievements = state.achievements.entry(game_id.to_string()).or_default();
    let stored = achievements.entry(achievement_id.to_string()).or_default();
    if stored.unlocked_at.is_some() {
        return None;
    }
    let unlocked_at = Utc::now().to_rfc3339();
    stored.unlocked_at = Some(unlocked_at.clone());
    Some(AchievementUnlockedEvent {
        game_id: game_id.to_string(),
        id: achievement_id.to_string(),
        name: definition.map(|d| d.name.to_string()).unwrap_or_else(|| achievement_id.to_string()),
        description: definition.map(|d| d.description.to_string()).unwrap_or_else(|| "".to_string()),
        unlocked_at,
    })
}

fn normalize_game_id(game_id: &str) -> String {
    let normalized = game_id
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>();
    if normalized.is_empty() {
        "unknown-game".to_string()
    } else {
        normalized
    }
}

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(PLATFORM_STATE_FILE))
}

fn load_state(app: &AppHandle) -> Result<PlatformStateFile, String> {
    let _guard = state_lock()
        .read()
        .map_err(|_| "platform state lock poisoned".to_string())?;
    load_state_unlocked(app)
}

fn load_state_unlocked(app: &AppHandle) -> Result<PlatformStateFile, String> {
    let path = state_path(app)?;
    if !path.exists() {
        return Ok(PlatformStateFile::default());
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    match serde_json::from_slice::<PlatformStateFile>(&bytes) {
        Ok(mut state) => {
            state.settings = state.settings.sanitized();
            Ok(state)
        }
        Err(error) => {
            let backup = path.with_extension(format!("corrupt-{}.json", Utc::now().timestamp()));
            let _ = fs::rename(&path, backup);
            Err(format!(
                "platform state was corrupt and has been reset: {error}"
            ))
        }
    }
}

fn update_state<F>(app: &AppHandle, update: F) -> Result<(), String>
where
    F: FnOnce(&mut PlatformStateFile),
{
    let _guard = state_lock()
        .write()
        .map_err(|_| "platform state lock poisoned".to_string())?;
    let mut state = load_state_unlocked(app).unwrap_or_default();
    update(&mut state);
    state.schema_version = PLATFORM_STATE_SCHEMA;
    write_state_unlocked(app, &state)
}

fn write_state_unlocked(app: &AppHandle, state: &PlatformStateFile) -> Result<(), String> {
    let path = state_path(app)?;
    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    let bytes = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    {
        let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if path.exists() {
        fs::copy(&path, &backup).map_err(|error| error.to_string())?;
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        if backup.exists() {
            let _ = fs::copy(&backup, &path);
        }
        return Err(error.to_string());
    }
    Ok(())
}
