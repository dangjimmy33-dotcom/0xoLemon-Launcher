use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const GAME_SESSION_SCHEMA_VERSION: u32 = 1;
const ACHIEVEMENT_EVENT_SCHEMA_VERSION: u32 = 2;
const EVENT_DEDUPE_CAPACITY: usize = 2_048;
const METRIC_SAMPLE_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OverlayRenderer {
    GseNative,
    ReshadeCompatibility,
    DesktopFallback,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GameSessionLifecycle {
    Starting,
    Running,
    Exiting,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AchievementTransport {
    Connecting,
    NamedPipe,
    ScopedFallback,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AchievementEventKind {
    Schema,
    Unlock,
    Progress,
    Clear,
    Stat,
    Flush,
    RuntimeStopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AchievementEventSource {
    NamedPipe,
    ScopedFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMetricsSummary {
    pub sample_count: u64,
    pub frame_interval_p95_ms: Option<f64>,
    pub overlay_callback_p95_ms: Option<f64>,
    pub private_bytes: Option<u64>,
    pub commit_bytes: Option<u64>,
    pub vram_bytes: Option<u64>,
    pub handle_count: Option<u64>,
    pub queue_depth: u64,
    pub sampled_at: Option<i64>,
}

impl Default for OverlayMetricsSummary {
    fn default() -> Self {
        Self {
            sample_count: 0,
            frame_interval_p95_ms: None,
            overlay_callback_p95_ms: None,
            private_bytes: None,
            commit_bytes: None,
            vram_bytes: None,
            handle_count: None,
            queue_depth: 0,
            sampled_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMetricSample {
    pub frame_interval_ms: Option<f64>,
    pub overlay_callback_ms: Option<f64>,
    pub private_bytes: Option<u64>,
    pub commit_bytes: Option<u64>,
    pub vram_bytes: Option<u64>,
    pub handle_count: Option<u64>,
    pub queue_depth: Option<u64>,
    pub sampled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameSessionStateV1 {
    pub schema_version: u32,
    pub session_id: String,
    pub game_id: String,
    pub app_id: u32,
    pub lifecycle: GameSessionLifecycle,
    pub root_pid: u32,
    pub runtime_pid: Option<u32>,
    pub renderer: OverlayRenderer,
    pub achievement_transport: AchievementTransport,
    pub dropped_event_count: u64,
    pub latest_achievement_sequence: u64,
    pub metrics: OverlayMetricsSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AchievementEventV2 {
    pub schema_version: u32,
    pub event_id: String,
    pub sequence: u64,
    pub session_id: String,
    pub game_id: String,
    pub app_id: u32,
    pub achievement_id: String,
    pub kind: AchievementEventKind,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_path: Option<String>,
    pub current: Option<f64>,
    pub maximum: Option<f64>,
    pub occurred_at: i64,
    pub source: AchievementEventSource,
}

#[derive(Debug, Clone)]
pub struct AchievementEventInput {
    pub source_event_id: String,
    pub session_id: String,
    pub game_id: String,
    pub app_id: u32,
    pub achievement_id: String,
    pub kind: AchievementEventKind,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_path: Option<String>,
    pub current: Option<f64>,
    pub maximum: Option<f64>,
    pub occurred_at: Option<i64>,
    pub source: AchievementEventSource,
}

#[derive(Debug)]
struct SessionRecord {
    state: GameSessionStateV1,
    seen_event_ids: HashSet<String>,
    event_order: VecDeque<String>,
    metrics: MetricAccumulator,
}

#[derive(Debug, Default)]
struct MetricAccumulator {
    frame_intervals: VecDeque<f64>,
    overlay_callbacks: VecDeque<f64>,
}

impl MetricAccumulator {
    fn accept(&mut self, state: &mut GameSessionStateV1, sample: OverlayMetricSample) -> Result<(), String> {
        validate_duration_sample(sample.frame_interval_ms, "frameIntervalMs")?;
        validate_duration_sample(sample.overlay_callback_ms, "overlayCallbackMs")?;
        if let Some(value) = sample.frame_interval_ms {
            push_bounded(&mut self.frame_intervals, value);
        }
        if let Some(value) = sample.overlay_callback_ms {
            push_bounded(&mut self.overlay_callbacks, value);
        }
        state.metrics.sample_count = state.metrics.sample_count.saturating_add(1);
        state.metrics.frame_interval_p95_ms = percentile_95(&self.frame_intervals);
        state.metrics.overlay_callback_p95_ms = percentile_95(&self.overlay_callbacks);
        if sample.private_bytes.is_some() {
            state.metrics.private_bytes = sample.private_bytes;
        }
        if sample.commit_bytes.is_some() {
            state.metrics.commit_bytes = sample.commit_bytes;
        }
        if sample.vram_bytes.is_some() {
            state.metrics.vram_bytes = sample.vram_bytes;
        }
        if sample.handle_count.is_some() {
            state.metrics.handle_count = sample.handle_count;
        }
        if let Some(queue_depth) = sample.queue_depth {
            state.metrics.queue_depth = queue_depth;
        }
        state.metrics.sampled_at = Some(sample.sampled_at.unwrap_or_else(|| Utc::now().timestamp_millis()));
        Ok(())
    }
}

fn validate_duration_sample(value: Option<f64>, field: &str) -> Result<(), String> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0 || value > 60_000.0) {
        return Err(format!("Invalid overlay metric {field}"));
    }
    Ok(())
}

fn push_bounded(samples: &mut VecDeque<f64>, value: f64) {
    samples.push_back(value);
    while samples.len() > METRIC_SAMPLE_CAPACITY {
        samples.pop_front();
    }
}

fn percentile_95(samples: &VecDeque<f64>) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.iter().copied().collect::<Vec<_>>();
    ordered.sort_by(f64::total_cmp);
    let rank = ((ordered.len() as f64) * 0.95).ceil() as usize;
    ordered.get(rank.saturating_sub(1)).copied()
}

impl SessionRecord {
    fn accept_event(&mut self, input: AchievementEventInput) -> Result<Option<AchievementEventV2>, String> {
        if input.session_id != self.state.session_id
            || input.game_id != self.state.game_id
            || input.app_id != self.state.app_id
        {
            self.state.dropped_event_count = self.state.dropped_event_count.saturating_add(1);
            return Err("Achievement event identity does not match the active game session".to_string());
        }
        let event_id = format!(
            "{}:{}:{}",
            input.session_id,
            source_label(input.source),
            input.source_event_id
        );
        if self.seen_event_ids.contains(&event_id) {
            return Ok(None);
        }
        self.seen_event_ids.insert(event_id.clone());
        self.event_order.push_back(event_id.clone());
        while self.event_order.len() > EVENT_DEDUPE_CAPACITY {
            if let Some(expired) = self.event_order.pop_front() {
                self.seen_event_ids.remove(&expired);
            }
        }

        self.state.latest_achievement_sequence =
            self.state.latest_achievement_sequence.saturating_add(1).max(1);
        if input.kind == AchievementEventKind::RuntimeStopped {
            self.state.achievement_transport = AchievementTransport::Closed;
            if self.state.lifecycle == GameSessionLifecycle::Running {
                self.state.lifecycle = GameSessionLifecycle::Exiting;
            }
        }
        Ok(Some(AchievementEventV2 {
            schema_version: ACHIEVEMENT_EVENT_SCHEMA_VERSION,
            event_id,
            sequence: self.state.latest_achievement_sequence,
            session_id: input.session_id,
            game_id: input.game_id,
            app_id: input.app_id,
            achievement_id: input.achievement_id,
            kind: input.kind,
            name: input.name,
            description: input.description,
            icon_path: input.icon_path,
            current: input.current,
            maximum: input.maximum,
            occurred_at: input.occurred_at.unwrap_or_else(|| Utc::now().timestamp_millis()),
            source: input.source,
        }))
    }
}

static SESSIONS: OnceLock<Mutex<HashMap<String, SessionRecord>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, SessionRecord>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn normalize_game_id(game_id: &str) -> Result<String, String> {
    let normalized = game_id.trim().to_lowercase();
    if normalized.is_empty()
        || normalized.len() > 128
        || normalized
            .chars()
            .any(|value| value.is_control() || matches!(value, '/' | '\\'))
    {
        return Err("Invalid game id for the game session registry".to_string());
    }
    Ok(normalized)
}

fn source_label(source: AchievementEventSource) -> &'static str {
    match source {
        AchievementEventSource::NamedPipe => "namedPipe",
        AchievementEventSource::ScopedFallback => "scopedFallback",
    }
}

fn emit_state(app: &AppHandle, state: &GameSessionStateV1) {
    let _ = app.emit("launcher://game-session-state", state.clone());
}

pub fn begin_session(
    app: &AppHandle,
    session_id: &str,
    game_id: &str,
    app_id: u32,
    renderer: OverlayRenderer,
) -> Result<GameSessionStateV1, String> {
    let game_id = normalize_game_id(game_id)?;
    if session_id.trim().is_empty() {
        return Err("Game session id cannot be empty".to_string());
    }
    let state = GameSessionStateV1 {
        schema_version: GAME_SESSION_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        game_id: game_id.clone(),
        app_id,
        lifecycle: GameSessionLifecycle::Starting,
        root_pid: 0,
        runtime_pid: None,
        renderer,
        achievement_transport: AchievementTransport::Connecting,
        dropped_event_count: 0,
        latest_achievement_sequence: 0,
        metrics: OverlayMetricsSummary::default(),
    };
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?;
    if registry.get(&game_id).is_some_and(|record| {
        matches!(
            record.state.lifecycle,
            GameSessionLifecycle::Starting
                | GameSessionLifecycle::Running
                | GameSessionLifecycle::Exiting
        ) && record.state.session_id != session_id
    }) {
        return Err(format!("A game session is already active for {game_id}"));
    }
    registry.insert(
        game_id,
        SessionRecord {
            state: state.clone(),
            seen_event_ids: HashSet::new(),
            event_order: VecDeque::new(),
            metrics: MetricAccumulator::default(),
        },
    );
    drop(registry);
    emit_state(app, &state);
    Ok(state)
}

pub fn mark_running(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
    root_pid: u32,
) -> Result<GameSessionStateV1, String> {
    if root_pid == 0 {
        return Err("Game session root PID cannot be zero".to_string());
    }
    update_session(app, game_id, session_id, |state| {
        state.root_pid = root_pid;
        state.lifecycle = GameSessionLifecycle::Running;
    })
}

pub fn mark_transport(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
    transport: AchievementTransport,
    runtime_pid: Option<u32>,
) -> Result<GameSessionStateV1, String> {
    if runtime_pid == Some(0) {
        return Err("Achievement runtime PID cannot be zero".to_string());
    }
    update_session(app, game_id, session_id, |state| {
        state.achievement_transport = transport;
        if runtime_pid.is_some() {
            state.runtime_pid = runtime_pid;
        }
    })
}

pub fn close_session(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
    failed: bool,
) -> Result<GameSessionStateV1, String> {
    update_session(app, game_id, session_id, |state| {
        state.lifecycle = if failed {
            GameSessionLifecycle::Failed
        } else {
            GameSessionLifecycle::Closed
        };
        state.achievement_transport = AchievementTransport::Closed;
        state.metrics.queue_depth = 0;
    })
}

pub fn mark_exiting(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
) -> Result<GameSessionStateV1, String> {
    update_session(app, game_id, session_id, |state| {
        if state.lifecycle == GameSessionLifecycle::Running {
            state.lifecycle = GameSessionLifecycle::Exiting;
        }
    })
}

fn update_session(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
    update: impl FnOnce(&mut GameSessionStateV1),
) -> Result<GameSessionStateV1, String> {
    let game_id = normalize_game_id(game_id)?;
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?;
    let record = registry
        .get_mut(&game_id)
        .ok_or_else(|| format!("No game session is registered for {game_id}"))?;
    if record.state.session_id != session_id {
        return Err("Game session id does not match the active session".to_string());
    }
    update(&mut record.state);
    let state = record.state.clone();
    drop(registry);
    emit_state(app, &state);
    Ok(state)
}

pub fn accept_achievement_event(
    app: &AppHandle,
    input: AchievementEventInput,
) -> Result<Option<AchievementEventV2>, String> {
    let game_id = normalize_game_id(&input.game_id)?;
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?;
    let record = registry
        .get_mut(&game_id)
        .ok_or_else(|| format!("No game session is registered for {game_id}"))?;
    let event = record.accept_event(input)?;
    let state = record.state.clone();
    drop(registry);
    if let Some(event) = event.as_ref() {
        let _ = app.emit("launcher://achievement-event-v2", event.clone());
        emit_state(app, &state);
    }
    Ok(event)
}

pub fn record_overlay_metric(
    app: &AppHandle,
    game_id: &str,
    session_id: &str,
    sample: OverlayMetricSample,
) -> Result<GameSessionStateV1, String> {
    let game_id = normalize_game_id(game_id)?;
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?;
    let record = registry
        .get_mut(&game_id)
        .ok_or_else(|| format!("No game session is registered for {game_id}"))?;
    if record.state.session_id != session_id {
        return Err("Metric session id does not match the active session".to_string());
    }
    record.metrics.accept(&mut record.state, sample)?;
    let state = record.state.clone();
    drop(registry);
    emit_state(app, &state);
    Ok(state)
}

#[tauri::command]
pub fn get_game_session_state(game_id: String) -> Result<GameSessionStateV1, String> {
    let game_id = normalize_game_id(&game_id)?;
    sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?
        .get(&game_id)
        .map(|record| record.state.clone())
        .ok_or_else(|| format!("No game session is registered for {game_id}"))
}

#[tauri::command]
pub fn list_game_session_states() -> Result<Vec<GameSessionStateV1>, String> {
    let registry = sessions()
        .lock()
        .map_err(|_| "Game session registry is unavailable".to_string())?;
    let mut states = registry
        .values()
        .map(|record| record.state.clone())
        .collect::<Vec<_>>();
    states.sort_by(|left, right| {
        lifecycle_priority(left.lifecycle)
            .cmp(&lifecycle_priority(right.lifecycle))
            .then_with(|| left.game_id.cmp(&right.game_id))
    });
    Ok(states)
}

pub fn has_active_session() -> bool {
    sessions().lock().is_ok_and(|registry| {
        registry.values().any(|record| {
            matches!(
                record.state.lifecycle,
                GameSessionLifecycle::Starting
                    | GameSessionLifecycle::Running
                    | GameSessionLifecycle::Exiting
            )
        })
    })
}

fn lifecycle_priority(lifecycle: GameSessionLifecycle) -> u8 {
    match lifecycle {
        GameSessionLifecycle::Running => 0,
        GameSessionLifecycle::Starting => 1,
        GameSessionLifecycle::Exiting => 2,
        GameSessionLifecycle::Failed => 3,
        GameSessionLifecycle::Closed => 4,
    }
}

#[tauri::command]
pub fn get_overlay_metrics(game_id: String) -> Result<OverlayMetricsSummary, String> {
    Ok(get_game_session_state(game_id)?.metrics)
}

#[tauri::command]
pub fn set_overlay_profile(
    app: AppHandle,
    game_id: String,
    renderer: OverlayRenderer,
) -> Result<GameSessionStateV1, String> {
    let game_id = normalize_game_id(&game_id)?;
    if matches!(
        renderer,
        OverlayRenderer::GseNative | OverlayRenderer::ReshadeCompatibility
    ) {
        return Err(
            "This renderer requires a provenance-verified, per-game compatibility profile; no approved native artifact is installed"
                .to_string(),
        );
    }
    let session_id = get_game_session_state(game_id.clone())?.session_id;
    update_session(&app, &game_id, &session_id, |state| {
        state.renderer = renderer;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record() -> SessionRecord {
        SessionRecord {
            state: GameSessionStateV1 {
                schema_version: GAME_SESSION_SCHEMA_VERSION,
                session_id: "session-a".to_string(),
                game_id: "game-a".to_string(),
                app_id: 480,
                lifecycle: GameSessionLifecycle::Running,
                root_pid: 42,
                runtime_pid: Some(42),
                renderer: OverlayRenderer::Disabled,
                achievement_transport: AchievementTransport::NamedPipe,
                dropped_event_count: 0,
                latest_achievement_sequence: 0,
                metrics: OverlayMetricsSummary::default(),
            },
            seen_event_ids: HashSet::new(),
            event_order: VecDeque::new(),
            metrics: MetricAccumulator::default(),
        }
    }

    fn event(source_event_id: &str, source: AchievementEventSource) -> AchievementEventInput {
        AchievementEventInput {
            source_event_id: source_event_id.to_string(),
            session_id: "session-a".to_string(),
            game_id: "game-a".to_string(),
            app_id: 480,
            achievement_id: "ACH_WIN_ONE_GAME".to_string(),
            kind: AchievementEventKind::Unlock,
            name: Some("Winner".to_string()),
            description: None,
            icon_path: None,
            current: Some(1.0),
            maximum: Some(1.0),
            occurred_at: Some(123),
            source,
        }
    }

    #[test]
    fn assigns_one_monotonic_sequence_and_dedupes_replay() {
        let mut record = test_record();
        let first = record
            .accept_event(event("7", AchievementEventSource::NamedPipe))
            .expect("first event")
            .expect("new event");
        let replay = record
            .accept_event(event("7", AchievementEventSource::NamedPipe))
            .expect("replay");
        let fallback = record
            .accept_event(event("7", AchievementEventSource::ScopedFallback))
            .expect("fallback")
            .expect("distinct source event");

        assert_eq!(first.sequence, 1);
        assert!(replay.is_none());
        assert_eq!(fallback.sequence, 2);
        assert_eq!(record.state.latest_achievement_sequence, 2);
    }

    #[test]
    fn rejects_wrong_session_identity_and_counts_the_drop() {
        let mut record = test_record();
        let mut wrong = event("8", AchievementEventSource::NamedPipe);
        wrong.session_id = "session-b".to_string();
        assert!(record.accept_event(wrong).is_err());
        assert_eq!(record.state.dropped_event_count, 1);
        assert_eq!(record.state.latest_achievement_sequence, 0);
    }

    #[test]
    fn metric_percentiles_are_real_and_sample_storage_is_bounded() {
        let mut record = test_record();
        for value in 1..=(METRIC_SAMPLE_CAPACITY + 100) {
            record
                .metrics
                .accept(
                    &mut record.state,
                    OverlayMetricSample {
                        frame_interval_ms: Some(value as f64 / 10.0),
                        overlay_callback_ms: Some(value as f64 / 100.0),
                        private_bytes: Some(value as u64),
                        queue_depth: Some((value % 7) as u64),
                        sampled_at: Some(value as i64),
                        ..OverlayMetricSample::default()
                    },
                )
                .unwrap();
        }

        assert_eq!(record.metrics.frame_intervals.len(), METRIC_SAMPLE_CAPACITY);
        assert_eq!(record.metrics.overlay_callbacks.len(), METRIC_SAMPLE_CAPACITY);
        assert_eq!(record.state.metrics.sample_count, (METRIC_SAMPLE_CAPACITY + 100) as u64);
        assert_eq!(record.state.metrics.private_bytes, Some((METRIC_SAMPLE_CAPACITY + 100) as u64));
        assert!(record.state.metrics.frame_interval_p95_ms.unwrap() > 390.0);
        assert!(record.state.metrics.overlay_callback_p95_ms.unwrap() > 39.0);
    }

    #[test]
    fn rejects_non_finite_or_negative_duration_metrics() {
        let mut record = test_record();
        for invalid in [f64::NAN, f64::INFINITY, -0.01, 60_000.01] {
            assert!(record
                .metrics
                .accept(
                    &mut record.state,
                    OverlayMetricSample {
                        frame_interval_ms: Some(invalid),
                        ..OverlayMetricSample::default()
                    },
                )
                .is_err());
        }
        assert_eq!(record.state.metrics.sample_count, 0);
    }

    #[test]
    fn runtime_stopped_closes_transport_without_faking_metrics() {
        let mut record = test_record();
        let mut stopped = event("9", AchievementEventSource::NamedPipe);
        stopped.kind = AchievementEventKind::RuntimeStopped;
        stopped.achievement_id.clear();
        record
            .accept_event(stopped)
            .expect("runtime stopped")
            .expect("new event");

        assert_eq!(record.state.lifecycle, GameSessionLifecycle::Exiting);
        assert_eq!(record.state.achievement_transport, AchievementTransport::Closed);
        assert_eq!(record.state.metrics.sample_count, 0);
        assert!(record.state.metrics.frame_interval_p95_ms.is_none());
    }
}
