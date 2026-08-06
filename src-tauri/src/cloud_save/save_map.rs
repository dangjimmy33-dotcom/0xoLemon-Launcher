use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

use crate::security::verify_ed25519;

const BUILTIN_MAP_NAME: &str = "cloud-save-map.builtin.json";
const TRUSTED_ROOT_NAME: &str = "root.json";
const LKG_MAP_NAME: &str = "last-known-good.json";
const LKG_META_NAME: &str = "last-known-good.meta.json";
const MAP_STATE_DIR: &str = "cloud-save/maps";
const HARD_MAX_FILES: u64 = 50_000;
const HARD_MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const HARD_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const REMOTE_TIMEOUT_SECONDS: u64 = 20;
pub(super) const LEGACY_RETENTION_DAYS: u32 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSaveMap {
    pub schema_version: u32,
    #[serde(default)]
    pub map_version: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(default)]
    pub minimum_launcher_version: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub defaults: MapDefaults,
    #[serde(default)]
    pub games: HashMap<String, GameMap>,
}

fn default_platform() -> String {
    "windows".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapDefaults {
    #[serde(default = "default_sync_mode")]
    pub sync_mode: String,
    #[serde(default = "default_true")]
    pub sync_before_launch: bool,
    #[serde(default = "default_true")]
    pub sync_after_exit: bool,
    #[serde(default)]
    pub follow_reparse_points: bool,
    #[serde(default = "default_true")]
    pub exclude_wins: bool,
    #[serde(default)]
    pub limits: MapLimits,
    #[serde(default)]
    pub stability: StabilityPolicy,
    #[serde(default)]
    pub retention: RetentionPolicy,
    #[serde(default)]
    pub migration: MigrationPolicy,
}

impl Default for MapDefaults {
    fn default() -> Self {
        Self {
            sync_mode: default_sync_mode(),
            sync_before_launch: true,
            sync_after_exit: true,
            follow_reparse_points: false,
            exclude_wins: true,
            limits: MapLimits::default(),
            stability: StabilityPolicy::default(),
            retention: RetentionPolicy::default(),
            migration: MigrationPolicy::default(),
        }
    }
}

fn default_sync_mode() -> String {
    "automatic".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapLimits {
    #[serde(default = "default_max_files")]
    pub max_files: u64,
    #[serde(default = "default_max_total_bytes")]
    pub max_total_bytes: u64,
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: u64,
}

impl Default for MapLimits {
    fn default() -> Self {
        Self {
            max_files: default_max_files(),
            max_total_bytes: default_max_total_bytes(),
            max_file_bytes: default_max_file_bytes(),
        }
    }
}

fn default_max_files() -> u64 {
    10_000
}
fn default_max_total_bytes() -> u64 {
    5 * 1024 * 1024 * 1024
}
fn default_max_file_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StabilityPolicy {
    #[serde(default = "default_settle_time_ms")]
    pub settle_time_ms: u64,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
}

impl Default for StabilityPolicy {
    fn default() -> Self {
        Self {
            settle_time_ms: default_settle_time_ms(),
            poll_interval_ms: default_poll_interval_ms(),
            max_wait_ms: default_max_wait_ms(),
        }
    }
}

fn default_settle_time_ms() -> u64 {
    2_000
}
fn default_poll_interval_ms() -> u64 {
    500
}
fn default_max_wait_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicy {
    #[serde(default = "default_recent")]
    pub recent: usize,
    #[serde(default = "default_daily_days")]
    pub daily_days: u32,
    #[serde(default = "default_weekly_weeks")]
    pub weekly_weeks: u32,
    #[serde(default = "default_conflict_days")]
    pub conflict_days: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            recent: default_recent(),
            daily_days: default_daily_days(),
            weekly_weeks: default_weekly_weeks(),
            conflict_days: default_conflict_days(),
        }
    }
}

fn default_recent() -> usize {
    10
}
fn default_daily_days() -> u32 {
    7
}
fn default_weekly_weeks() -> u32 {
    4
}
fn default_conflict_days() -> u32 {
    90
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPolicy {
    #[serde(default = "default_legacy_retention_days")]
    pub legacy_retention_days: u32,
}

impl Default for MigrationPolicy {
    fn default() -> Self {
        Self {
            legacy_retention_days: default_legacy_retention_days(),
        }
    }
}

fn default_legacy_retention_days() -> u32 {
    LEGACY_RETENTION_DAYS
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMap {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "default_local_install_modes")]
    pub supported_install_modes: Vec<String>,
    #[serde(default)]
    pub roots: Vec<MapRoot>,
    #[serde(default)]
    pub limits: Option<MapLimits>,
    #[serde(default)]
    pub stability: Option<StabilityPolicy>,
    #[serde(default)]
    pub retention: Option<RetentionPolicy>,
    #[serde(default)]
    pub migration: Option<MigrationPolicy>,
}

fn default_local_install_modes() -> Vec<String> {
    vec!["local".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRoot {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_purpose")]
    pub purpose: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "default_match_policy")]
    pub match_policy: String,
    #[serde(default)]
    pub candidates: Vec<PathCandidate>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default)]
    pub follow_reparse_points: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub profile_discovery: ProfileDiscovery,
    #[serde(default)]
    pub limits: Option<MapLimits>,
    #[serde(default)]
    pub stability: Option<StabilityPolicy>,
    #[serde(default)]
    pub restore: RestorePolicy,
}

fn default_purpose() -> String {
    "save".to_string()
}

fn default_match_policy() -> String {
    "allExisting".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathCandidate {
    pub base: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub min_game_version: String,
    #[serde(default)]
    pub max_game_version: String,
    #[serde(default)]
    pub allow_absolute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDiscovery {
    #[serde(default = "default_profile_strategy")]
    pub strategy: String,
    #[serde(default = "default_max_profiles")]
    pub max_profiles: u32,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default)]
    pub names: Vec<String>,
}

impl Default for ProfileDiscovery {
    fn default() -> Self {
        Self {
            strategy: default_profile_strategy(),
            max_profiles: default_max_profiles(),
            max_depth: default_max_depth(),
            names: Vec::new(),
        }
    }
}

fn default_profile_strategy() -> String {
    "all".to_string()
}
fn default_max_profiles() -> u32 {
    32
}
fn default_max_depth() -> u32 {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePolicy {
    #[serde(default = "default_true")]
    pub create_missing_directories: bool,
    #[serde(default = "default_true")]
    pub backup_before_overwrite: bool,
    #[serde(default = "default_true")]
    pub atomic_replace: bool,
}

impl Default for RestorePolicy {
    fn default() -> Self {
        Self {
            create_missing_directories: true,
            backup_before_overwrite: true,
            atomic_replace: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSaveRoot {
    pub id: String,
    pub label: String,
    pub purpose: String,
    pub path: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub recursive: bool,
    pub required: bool,
    pub kind: String,
    pub limits: MapLimits,
    pub stability: StabilityPolicy,
    pub restore: RestorePolicy,
    pub fingerprint: String,
    #[serde(default)]
    pub legacy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedGameMap {
    pub game_id: String,
    pub display_name: String,
    pub map_version: String,
    pub source: String,
    pub roots: Vec<ResolvedSaveRoot>,
    pub limits: MapLimits,
    pub stability: StabilityPolicy,
    pub retention: RetentionPolicy,
    pub migration: MigrationPolicy,
    pub sync_before_launch: bool,
    pub sync_after_exit: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapUpdateReport {
    pub updated: bool,
    pub active_version: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LastKnownGoodMeta {
    version: u64,
    map_version: String,
    sha256: String,
    activated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SignedEnvelope {
    signed: Value,
    #[serde(default)]
    signatures: Vec<MetadataSignature>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataSignature {
    keyid: String,
    sig: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedRoot {
    signed: TrustedRootSigned,
    #[serde(default)]
    signatures: Vec<MetadataSignature>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedRootSigned {
    #[serde(default)]
    version: u64,
    #[serde(default)]
    expires: String,
    #[serde(default)]
    keys: HashMap<String, TrustedKey>,
    #[serde(default)]
    roles: HashMap<String, TrustedRole>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedKey {
    keyval: TrustedKeyValue,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedKeyValue {
    public: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedRole {
    #[serde(default)]
    keyids: Vec<String>,
    #[serde(default = "default_threshold")]
    threshold: usize,
}

fn default_threshold() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize)]
struct TimestampSigned {
    version: u64,
    expires: String,
    meta: HashMap<String, MetaDescription>,
}

#[derive(Debug, Clone, Deserialize)]
struct SnapshotSigned {
    version: u64,
    expires: String,
    meta: HashMap<String, MetaDescription>,
}

#[derive(Debug, Clone, Deserialize)]
struct TargetsSigned {
    version: u64,
    expires: String,
    targets: HashMap<String, TargetDescription>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetaDescription {
    version: u64,
    #[serde(default)]
    length: Option<u64>,
    #[serde(default)]
    hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TargetDescription {
    length: u64,
    hashes: HashMap<String, String>,
    #[serde(default)]
    custom: TargetCustom,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetCustom {
    #[serde(default)]
    map_version: String,
    #[serde(default)]
    rollout: Option<Rollout>,
}

#[derive(Debug, Clone, Deserialize)]
struct Rollout {
    percentage: u8,
    seed: String,
}

pub(super) fn load_for_game(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
    installed_version: &str,
) -> Result<ResolvedGameMap, String> {
    let (map, source) = load_active_map(app)?;
    validate_map_header(&map)?;
    let game = map
        .games
        .get(game_id)
        .ok_or_else(|| format!("Cloud Save chưa có cấu hình an toàn cho game {game_id}."))?;
    validate_game_map(game_id, game, &map.defaults)?;
    if !game.enabled {
        return Err("Cloud Save đang tắt trong save-path map cho game này.".to_string());
    }
    if !game
        .supported_install_modes
        .iter()
        .any(|mode| mode.eq_ignore_ascii_case("local"))
    {
        return Err("Cloud Save map này không áp dụng cho chế độ cài local.".to_string());
    }

    let limits = merge_limits(&map.defaults.limits, game.limits.as_ref());
    let stability = game
        .stability
        .clone()
        .unwrap_or_else(|| map.defaults.stability.clone());
    let retention = game
        .retention
        .clone()
        .unwrap_or_else(|| map.defaults.retention.clone());
    let migration = game
        .migration
        .clone()
        .unwrap_or_else(|| map.defaults.migration.clone());
    let mut warnings = Vec::new();
    let roots = resolve_game_roots(
        game_id,
        game,
        install_path,
        installed_version,
        &limits,
        &stability,
        &mut warnings,
    )?;
    if roots.is_empty() && game.roots.iter().any(|root| root.required) {
        return Err("Không tìm thấy đường dẫn Save bắt buộc trong map.".to_string());
    }

    Ok(ResolvedGameMap {
        game_id: game_id.to_string(),
        display_name: if game.display_name.trim().is_empty() {
            game_id.to_string()
        } else {
            game.display_name.clone()
        },
        map_version: map.map_version,
        source,
        roots,
        limits,
        stability,
        retention,
        migration,
        sync_before_launch: map.defaults.sync_before_launch,
        sync_after_exit: map.defaults.sync_after_exit,
        warnings,
    })
}

fn remote_repository_base_url() -> Option<String> {
    std::env::var("OXO_CLOUD_SAVE_MAP_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            option_env!("OXO_CLOUD_SAVE_MAP_BASE_URL")
                .map(str::to_string)
                .filter(|value| !value.trim().is_empty())
        })
}

pub(super) fn remote_repository_configured() -> bool {
    remote_repository_base_url().is_some()
}

pub(super) fn refresh_remote_map(app: &AppHandle, device_id: &str) -> Result<MapUpdateReport, String> {
    let base_url = remote_repository_base_url()
        .ok_or_else(|| "Remote save-map repository chưa được cấu hình.".to_string())?;
    let trusted_root = load_trusted_root(app)?;
    if trusted_root.signed.keys.is_empty() {
        return Err("Trusted root chưa chứa public key phát hành save map.".to_string());
    }
    validate_expiry(&trusted_root.signed.expires, "root.json")?;

    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(REMOTE_TIMEOUT_SECONDS))
        .user_agent("0xoLemon-CloudSaveMap/1")
        .build()
        .map_err(|error| error.to_string())?;

    let timestamp_bytes = fetch_bytes(&client, &format!("{}/timestamp.json", base_url.trim_end_matches('/')))?;
    let timestamp_env: SignedEnvelope = serde_json::from_slice(&timestamp_bytes).map_err(|error| error.to_string())?;
    verify_signed_metadata(&trusted_root, "timestamp", &timestamp_env)?;
    let timestamp: TimestampSigned = serde_json::from_value(timestamp_env.signed.clone()).map_err(|error| error.to_string())?;
    validate_expiry(&timestamp.expires, "timestamp.json")?;
    let snapshot_desc = timestamp
        .meta
        .get("snapshot.json")
        .ok_or_else(|| "timestamp.json thiếu snapshot.json".to_string())?;

    let snapshot_bytes = fetch_bytes(&client, &format!("{}/snapshot.json", base_url.trim_end_matches('/')))?;
    verify_description(&snapshot_bytes, snapshot_desc, "snapshot.json")?;
    let snapshot_env: SignedEnvelope = serde_json::from_slice(&snapshot_bytes).map_err(|error| error.to_string())?;
    verify_signed_metadata(&trusted_root, "snapshot", &snapshot_env)?;
    let snapshot: SnapshotSigned = serde_json::from_value(snapshot_env.signed.clone()).map_err(|error| error.to_string())?;
    validate_expiry(&snapshot.expires, "snapshot.json")?;
    if snapshot.version != snapshot_desc.version {
        return Err("snapshot.json version không khớp timestamp.json".to_string());
    }
    let targets_desc = snapshot
        .meta
        .get("targets.json")
        .ok_or_else(|| "snapshot.json thiếu targets.json".to_string())?;

    let targets_bytes = fetch_bytes(&client, &format!("{}/targets.json", base_url.trim_end_matches('/')))?;
    verify_description(&targets_bytes, targets_desc, "targets.json")?;
    let targets_env: SignedEnvelope = serde_json::from_slice(&targets_bytes).map_err(|error| error.to_string())?;
    verify_signed_metadata(&trusted_root, "targets", &targets_env)?;
    let targets: TargetsSigned = serde_json::from_value(targets_env.signed.clone()).map_err(|error| error.to_string())?;
    validate_expiry(&targets.expires, "targets.json")?;
    if targets.version != targets_desc.version {
        return Err("targets.json version không khớp snapshot.json".to_string());
    }

    let (target_name, target_desc) = targets
        .targets
        .iter()
        .filter(|(name, _)| name.starts_with("maps/cloud-save-map-") && name.ends_with(".json"))
        .max_by_key(|(_, description)| description.custom.map_version.clone())
        .ok_or_else(|| "targets.json không có Cloud Save map".to_string())?;

    if let Some(rollout) = &target_desc.custom.rollout {
        if !device_in_rollout(device_id, rollout) {
            let (_, source) = load_active_map(app)?;
            return Ok(MapUpdateReport {
                updated: false,
                active_version: current_lkg_version(app).unwrap_or_default(),
                source,
                message: "Thiết bị chưa nằm trong đợt rollout của save map mới.".to_string(),
            });
        }
    }

    let target_bytes = fetch_bytes(&client, &format!("{}/{}", base_url.trim_end_matches('/'), target_name))?;
    verify_target(&target_bytes, target_desc, target_name)?;
    let map: CloudSaveMap = serde_json::from_slice(&target_bytes).map_err(|error| error.to_string())?;
    validate_map_header(&map)?;
    for (game_id, game) in &map.games {
        validate_game_map(game_id, game, &map.defaults)?;
    }

    let current_meta = load_lkg_meta(app).ok();
    if current_meta
        .as_ref()
        .is_some_and(|current| timestamp.version < current.version)
    {
        return Err("Save map metadata bị rollback về phiên bản cũ.".to_string());
    }

    let map_hash = hex_sha256(&target_bytes);
    write_atomic(&lkg_map_path(app)?, &target_bytes)?;
    write_atomic(
        &lkg_meta_path(app)?,
        &serde_json::to_vec_pretty(&LastKnownGoodMeta {
            version: timestamp.version,
            map_version: map.map_version.clone(),
            sha256: map_hash,
            activated_at: Utc::now().to_rfc3339(),
        })
        .map_err(|error| error.to_string())?,
    )?;

    Ok(MapUpdateReport {
        updated: true,
        active_version: map.map_version,
        source: "remote-last-known-good".to_string(),
        message: "Cloud Save map đã được xác minh và kích hoạt an toàn.".to_string(),
    })
}

pub(super) fn validate_game_map(
    game_id: &str,
    game: &GameMap,
    defaults: &MapDefaults,
) -> Result<(), String> {
    if game_id.trim().is_empty() {
        return Err("gameId trong save map không được trống".to_string());
    }
    if game.roots.is_empty() {
        return Err(format!("{game_id}: save map chưa có root"));
    }
    let mut ids = HashSet::new();
    for root in &game.roots {
        if root.id.trim().is_empty() || !ids.insert(root.id.to_ascii_lowercase()) {
            return Err(format!("{game_id}: root id trống hoặc trùng"));
        }
        if root.candidates.is_empty() {
            return Err(format!("{game_id}/{}: chưa có path candidate", root.id));
        }
        if !matches!(root.match_policy.as_str(), "allExisting" | "firstExisting" | "highestPriority") {
            return Err(format!("{game_id}/{}: matchPolicy không hợp lệ", root.id));
        }
        if root.follow_reparse_points || defaults.follow_reparse_points {
            return Err(format!("{game_id}/{}: followReparsePoints không được phép", root.id));
        }
        for pattern in root.include.iter().chain(root.exclude.iter()) {
            validate_pattern(pattern)?;
        }
        let limits = merge_limits(&defaults.limits, root.limits.as_ref().or(game.limits.as_ref()));
        validate_limits(&limits)?;
        for candidate in &root.candidates {
            validate_candidate(game_id, candidate)?;
        }
    }
    validate_limits(&merge_limits(&defaults.limits, game.limits.as_ref()))
}

pub(super) fn resolve_game_roots(
    game_id: &str,
    game: &GameMap,
    install_path: &Path,
    installed_version: &str,
    game_limits: &MapLimits,
    game_stability: &StabilityPolicy,
    warnings: &mut Vec<String>,
) -> Result<Vec<ResolvedSaveRoot>, String> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    for root in &game.roots {
        let mut candidates = Vec::new();
        for candidate in root
            .candidates
            .iter()
            .filter(|candidate| candidate_applies(candidate, installed_version))
        {
            if let Some(path) = resolve_candidate(candidate, install_path)? {
                candidates.push((candidate.priority, path));
            }
        }
        candidates.sort_by(|left, right| right.0.cmp(&left.0));
        let existing = candidates
            .iter()
            .filter(|(_, path)| path.exists())
            .cloned()
            .collect::<Vec<_>>();
        let selected = if existing.is_empty() {
            // Keep one dormant candidate so the launcher can protect the first save
            // created by a newly installed game without asking the user to browse.
            candidates.into_iter().take(1).collect::<Vec<_>>()
        } else {
            match root.match_policy.as_str() {
                "firstExisting" | "highestPriority" => {
                    existing.into_iter().take(1).collect::<Vec<_>>()
                }
                _ => existing,
            }
        };
        if selected.is_empty() {
            if root.required {
                warnings.push(format!("Không tìm thấy root bắt buộc: {}", root.label_or_id()));
            }
            continue;
        }
        for (_, path) in selected {
            let canonical_key = path.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(canonical_key) {
                continue;
            }
            reject_broad_root(&path)?;
            if is_reparse_or_symlink(&path)? {
                return Err(format!("{} là junction/symlink và bị chặn", path.display()));
            }
            let limits = merge_limits(game_limits, root.limits.as_ref());
            let stability = root
                .stability
                .clone()
                .unwrap_or_else(|| game_stability.clone());
            if path.exists() {
                let report = dry_run_root(&path, root, &limits)?;
                if report.files > limits.max_files || report.bytes > limits.max_total_bytes {
                    return Err(format!(
                        "{} vượt giới hạn Cloud Save an toàn ({} file, {} byte)",
                        path.display(), report.files, report.bytes
                    ));
                }
                if report.suspicious_files > 0 {
                    warnings.push(format!(
                        "{} có {} file giống dữ liệu game/cache; hãy kiểm tra map.",
                        root.label_or_id(), report.suspicious_files
                    ));
                }
            } else {
                warnings.push(format!(
                    "Đang chờ game tạo thư mục Save: {}",
                    root.label_or_id()
                ));
            }
            let fingerprint = root_fingerprint(game_id, &root.id, &path, &root.include, &root.exclude);
            resolved.push(ResolvedSaveRoot {
                id: root.id.clone(),
                label: root.label_or_id(),
                purpose: root.purpose.clone(),
                path: path.display().to_string(),
                include: root.include.clone(),
                exclude: root.exclude.clone(),
                recursive: root.recursive,
                required: root.required,
                kind: if root.kind.trim().is_empty() {
                    if path.is_file() { "file" } else { "directory" }.to_string()
                } else {
                    root.kind.clone()
                },
                limits,
                stability,
                restore: root.restore.clone(),
                fingerprint,
                legacy: false,
            });
        }
    }
    Ok(resolved)
}

#[derive(Default)]
struct DryRunReport {
    files: u64,
    bytes: u64,
    suspicious_files: u64,
}

fn dry_run_root(path: &Path, root: &MapRoot, limits: &MapLimits) -> Result<DryRunReport, String> {
    let mut report = DryRunReport::default();
    if path.is_file() {
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.len() > limits.max_file_bytes {
            return Err(format!("{} vượt giới hạn kích thước file", path.display()));
        }
        report.files = 1;
        report.bytes = metadata.len();
        return Ok(report);
    }
    let walker = WalkDir::new(path)
        .follow_links(false)
        .max_depth(if root.recursive { root.profile_discovery.max_depth as usize + 32 } else { 1 });
    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(path)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if !selected(&relative, &root.include, &root.exclude) {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.len() > limits.max_file_bytes {
            return Err(format!("{} vượt giới hạn kích thước file", entry.path().display()));
        }
        report.files += 1;
        report.bytes = report.bytes.saturating_add(metadata.len());
        if is_suspicious_extension(entry.path()) {
            report.suspicious_files += 1;
        }
        if report.files > HARD_MAX_FILES || report.bytes > HARD_MAX_TOTAL_BYTES {
            return Err("Save path map rộng bất thường và đã bị chặn.".to_string());
        }
    }
    Ok(report)
}

fn load_active_map(app: &AppHandle) -> Result<(CloudSaveMap, String), String> {
    if let Ok(path) = std::env::var("OXO_CLOUD_SAVE_MAP_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return read_map(&path).map(|map| (map, "operator-map".to_string()));
        }
    }
    let lkg = lkg_map_path(app)?;
    if lkg.is_file() {
        if let Ok(map) = read_map(&lkg) {
            if validate_map_header(&map).is_ok() {
                return Ok((map, "remote-last-known-good".to_string()));
            }
        }
    }
    let builtin = builtin_resource_path(app, BUILTIN_MAP_NAME)?;
    read_map(&builtin).map(|map| (map, "built-in".to_string()))
}

fn read_map(path: &Path) -> Result<CloudSaveMap, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn validate_map_header(map: &CloudSaveMap) -> Result<(), String> {
    if map.schema_version != 1 {
        return Err(format!("Cloud Save map schema {} không được hỗ trợ", map.schema_version));
    }
    if !map.platform.eq_ignore_ascii_case("windows") {
        return Err("Cloud Save map không dành cho Windows.".to_string());
    }
    if !map.expires_at.trim().is_empty() {
        validate_expiry(&map.expires_at, "cloud-save-map")?;
    }
    validate_limits(&map.defaults.limits)
}

fn validate_limits(limits: &MapLimits) -> Result<(), String> {
    if limits.max_files == 0 || limits.max_files > HARD_MAX_FILES {
        return Err(format!("maxFiles phải nằm trong 1..={HARD_MAX_FILES}"));
    }
    if limits.max_total_bytes == 0 || limits.max_total_bytes > HARD_MAX_TOTAL_BYTES {
        return Err(format!("maxTotalBytes phải nằm trong 1..={HARD_MAX_TOTAL_BYTES}"));
    }
    if limits.max_file_bytes == 0 || limits.max_file_bytes > HARD_MAX_FILE_BYTES {
        return Err(format!("maxFileBytes phải nằm trong 1..={HARD_MAX_FILE_BYTES}"));
    }
    if limits.max_file_bytes > limits.max_total_bytes {
        return Err("maxFileBytes không được lớn hơn maxTotalBytes".to_string());
    }
    Ok(())
}

fn absolute_path_whitelisted(game_id: &str) -> bool {
    // Deliberately empty by default. Adding a game here requires a launcher
    // release and code review; a remotely signed map cannot grant itself
    // arbitrary absolute-path access.
    const ALLOWED_GAME_IDS: &[&str] = &[];
    ALLOWED_GAME_IDS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(game_id))
}

fn validate_candidate(game_id: &str, candidate: &PathCandidate) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "KnownFolder.LocalAppData",
        "KnownFolder.LocalAppDataLow",
        "KnownFolder.RoamingAppData",
        "KnownFolder.Documents",
        "KnownFolder.PublicDocuments",
        "KnownFolder.SavedGames",
        "KnownFolder.UserProfile",
        "GameInstall",
        "LauncherData",
        "AbsolutePath",
    ];
    if !ALLOWED.iter().any(|allowed| *allowed == candidate.base) {
        return Err(format!("base không được hỗ trợ: {}", candidate.base));
    }
    if candidate.base == "AbsolutePath"
        && (!candidate.allow_absolute || !absolute_path_whitelisted(game_id))
    {
        return Err(
            "AbsolutePath cần allowAbsolute=true và gameId phải được whitelist trong binary."
                .to_string(),
        );
    }
    let path = Path::new(&candidate.path);
    if candidate.base != "AbsolutePath" && path.is_absolute() {
        return Err("candidate path phải tương đối với base".to_string());
    }
    if path.components().any(|component| matches!(component, Component::ParentDir)) {
        return Err("candidate path không được chứa ..".to_string());
    }
    Ok(())
}

fn validate_pattern(pattern: &str) -> Result<(), String> {
    if pattern.contains("..") || pattern.starts_with('/') || pattern.starts_with('\\') {
        return Err(format!("glob không an toàn: {pattern}"));
    }
    if pattern.len() > 512 {
        return Err("glob quá dài".to_string());
    }
    Ok(())
}

fn candidate_applies(candidate: &PathCandidate, version: &str) -> bool {
    if !candidate.platform.trim().is_empty() && !candidate.platform.eq_ignore_ascii_case("windows") {
        return false;
    }
    if !candidate.min_game_version.trim().is_empty()
        && crate::save_paths::compare_versions(version, &candidate.min_game_version) < 0
    {
        return false;
    }
    if !candidate.max_game_version.trim().is_empty()
        && crate::save_paths::compare_versions(version, &candidate.max_game_version) > 0
    {
        return false;
    }
    true
}

fn resolve_candidate(candidate: &PathCandidate, install_path: &Path) -> Result<Option<PathBuf>, String> {
    let base = match candidate.base.as_str() {
        "KnownFolder.LocalAppData" => known_folder_path(KnownFolderKind::LocalAppData)?,
        "KnownFolder.LocalAppDataLow" => known_folder_path(KnownFolderKind::LocalAppDataLow)?,
        "KnownFolder.RoamingAppData" => known_folder_path(KnownFolderKind::RoamingAppData)?,
        "KnownFolder.Documents" => known_folder_path(KnownFolderKind::Documents)?,
        "KnownFolder.PublicDocuments" => known_folder_path(KnownFolderKind::PublicDocuments)?,
        "KnownFolder.SavedGames" => known_folder_path(KnownFolderKind::SavedGames)?,
        "KnownFolder.UserProfile" => known_folder_path(KnownFolderKind::UserProfile)?,
        "GameInstall" => install_path.to_path_buf(),
        "LauncherData" => known_folder_path(KnownFolderKind::LocalAppData)?.join("0xoLemon"),
        "AbsolutePath" if candidate.allow_absolute => PathBuf::from(&candidate.path),
        _ => return Ok(None),
    };
    let path = if candidate.base == "AbsolutePath" {
        base
    } else {
        safe_join(&base, &candidate.path)?
    };
    Ok(Some(path))
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err("Cloud Save map path escaped its base.".to_string());
    }
    Ok(base.join(relative))
}

fn reject_broad_root(path: &Path) -> Result<(), String> {
    let normalized = path.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    if normalized.len() <= 3 || normalized.ends_with(":") {
        return Err("Không được dùng toàn bộ ổ đĩa làm Save root.".to_string());
    }
    let protected = ["windows", "program files", "program files (x86)"];
    if protected.iter().any(|name| normalized.ends_with(name)) {
        return Err("Save root trỏ vào thư mục hệ thống và đã bị chặn.".to_string());
    }
    for kind in [KnownFolderKind::UserProfile, KnownFolderKind::LocalAppData, KnownFolderKind::RoamingAppData] {
        if let Ok(folder) = known_folder_path(kind) {
            if normalized == folder.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase() {
                return Err("Save root quá rộng và có thể đọc toàn bộ dữ liệu người dùng.".to_string());
            }
        }
    }
    Ok(())
}

fn merge_limits(base: &MapLimits, override_value: Option<&MapLimits>) -> MapLimits {
    override_value.cloned().unwrap_or_else(|| base.clone())
}

fn selected(path: &str, include: &[String], exclude: &[String]) -> bool {
    let included = include.is_empty() || include.iter().any(|pattern| wildcard_match(pattern, path));
    included && !exclude.iter().any(|pattern| wildcard_match(pattern, path))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.replace("**", "*");
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut p, mut v, mut star, mut matched) = (0usize, 0usize, None, 0usize);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&value[v])) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            matched = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            matched += 1;
            v = matched;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn is_suspicious_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("exe") | Some("dll") | Some("pak") | Some("ucas") | Some("utoc") | Some("bin")
    )
}

fn root_fingerprint(game_id: &str, root_id: &str, path: &Path, include: &[String], exclude: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(game_id.as_bytes());
    hasher.update([0]);
    hasher.update(root_id.as_bytes());
    hasher.update([0]);
    hasher.update(path.to_string_lossy().as_bytes());
    for pattern in include {
        hasher.update([1]);
        hasher.update(pattern.as_bytes());
    }
    for pattern in exclude {
        hasher.update([2]);
        hasher.update(pattern.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn verify_signed_metadata(root: &TrustedRoot, role_name: &str, envelope: &SignedEnvelope) -> Result<(), String> {
    let role = root
        .signed
        .roles
        .get(role_name)
        .ok_or_else(|| format!("trusted root thiếu role {role_name}"))?;
    let payload = canonical_json(&envelope.signed)?;
    let mut verified = HashSet::new();
    for signature in &envelope.signatures {
        if !role.keyids.iter().any(|keyid| keyid == &signature.keyid) {
            continue;
        }
        let Some(key) = root.signed.keys.get(&signature.keyid) else {
            continue;
        };
        if verify_ed25519(&key.keyval.public, &payload, &signature.sig)
            .map_err(|error| error.to_string())?
        {
            verified.insert(signature.keyid.clone());
        }
    }
    if verified.len() < role.threshold {
        return Err(format!("{role_name} metadata không đủ chữ ký hợp lệ"));
    }
    Ok(())
}

fn canonical_json(value: &Value) -> Result<Vec<u8>, String> {
    fn normalize(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted = map
                    .iter()
                    .map(|(key, value)| (key.clone(), normalize(value)))
                    .collect::<BTreeMap<_, _>>();
                let mut output = serde_json::Map::new();
                for (key, value) in sorted {
                    output.insert(key, value);
                }
                Value::Object(output)
            }
            Value::Array(values) => Value::Array(values.iter().map(normalize).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_vec(&normalize(value)).map_err(|error| error.to_string())
}

fn verify_description(bytes: &[u8], description: &MetaDescription, name: &str) -> Result<(), String> {
    if let Some(length) = description.length {
        if bytes.len() as u64 != length {
            return Err(format!("{name} length không khớp metadata"));
        }
    }
    if let Some(expected) = description.hashes.get("sha256") {
        if !hex_sha256(bytes).eq_ignore_ascii_case(expected) {
            return Err(format!("{name} SHA-256 không khớp metadata"));
        }
    }
    Ok(())
}

fn verify_target(bytes: &[u8], description: &TargetDescription, name: &str) -> Result<(), String> {
    if bytes.len() as u64 != description.length {
        return Err(format!("{name} length không khớp targets.json"));
    }
    let expected = description
        .hashes
        .get("sha256")
        .ok_or_else(|| format!("{name} thiếu SHA-256"))?;
    if !hex_sha256(bytes).eq_ignore_ascii_case(expected) {
        return Err(format!("{name} SHA-256 không khớp targets.json"));
    }
    Ok(())
}

fn validate_expiry(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let expiry = DateTime::parse_from_rfc3339(value)
        .map_err(|error| format!("{name} expires không hợp lệ: {error}"))?
        .with_timezone(&Utc);
    if expiry <= Utc::now() {
        return Err(format!("{name} đã hết hạn"));
    }
    Ok(())
}

fn fetch_bytes(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Save-map server HTTP {}", response.status()));
    }
    response.bytes().map(|bytes| bytes.to_vec()).map_err(|error| error.to_string())
}

fn device_in_rollout(device_id: &str, rollout: &Rollout) -> bool {
    if rollout.percentage >= 100 {
        return true;
    }
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update([0]);
    hasher.update(rollout.seed.as_bytes());
    let digest = hasher.finalize();
    let bucket = u16::from_be_bytes([digest[0], digest[1]]) % 100;
    bucket < rollout.percentage as u16
}

fn load_trusted_root(app: &AppHandle) -> Result<TrustedRoot, String> {
    let path = builtin_resource_path(app, TRUSTED_ROOT_NAME)?;
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn builtin_resource_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("cloud-save").join(name));
        candidates.push(resource_dir.join("resources").join("cloud-save").join(name));
        candidates.push(
            resource_dir
                .join("src")
                .join("resources")
                .join("cloud-save")
                .join(name),
        );
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join("resources").join("cloud-save").join(name));
    candidates.push(
        manifest_dir
            .join("src")
            .join("resources")
            .join("cloud-save")
            .join(name),
    );
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("Không tìm thấy Cloud Save resource {name}"))
}

fn map_state_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(MAP_STATE_DIR);
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn lkg_map_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(map_state_dir(app)?.join(LKG_MAP_NAME))
}

fn lkg_meta_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(map_state_dir(app)?.join(LKG_META_NAME))
}

fn load_lkg_meta(app: &AppHandle) -> Result<LastKnownGoodMeta, String> {
    serde_json::from_slice(&fs::read(lkg_meta_path(app)?).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn current_lkg_version(app: &AppHandle) -> Option<String> {
    load_lkg_meta(app).ok().map(|meta| meta.map_version)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("tmp");
    {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if path.exists() {
        let backup = path.with_extension("bak");
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
        match fs::rename(&temporary, path) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, path);
                Err(error.to_string())
            }
        }
    } else {
        fs::rename(temporary, path).map_err(|error| error.to_string())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl MapRoot {
    fn label_or_id(&self) -> String {
        if self.label.trim().is_empty() {
            self.id.clone()
        } else {
            self.label.clone()
        }
    }
}

#[derive(Clone, Copy)]
enum KnownFolderKind {
    LocalAppData,
    LocalAppDataLow,
    RoamingAppData,
    Documents,
    PublicDocuments,
    SavedGames,
    UserProfile,
}

#[cfg(target_os = "windows")]
fn known_folder_path(kind: KnownFolderKind) -> Result<PathBuf, String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHGetKnownFolderPath(
            rfid: *const Guid,
            flags: u32,
            token: *mut c_void,
            path: *mut *mut u16,
        ) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(memory: *mut c_void);
    }

    const LOCAL_APP_DATA: Guid = Guid { data1: 0xF1B32785, data2: 0x6FBA, data3: 0x4FCF, data4: [0x9D, 0x55, 0x7B, 0x8E, 0x7F, 0x15, 0x70, 0x91] };
    const LOCAL_APP_DATA_LOW: Guid = Guid { data1: 0xA520A1A4, data2: 0x1780, data3: 0x4FF6, data4: [0xBD, 0x18, 0x16, 0x73, 0x43, 0xC5, 0xAF, 0x16] };
    const ROAMING_APP_DATA: Guid = Guid { data1: 0x3EB685DB, data2: 0x65F9, data3: 0x4CF6, data4: [0xA0, 0x3A, 0xE3, 0xEF, 0x65, 0x72, 0x9F, 0x3D] };
    const DOCUMENTS: Guid = Guid { data1: 0xFDD39AD0, data2: 0x238F, data3: 0x46AF, data4: [0xAD, 0xB4, 0x6C, 0x85, 0x48, 0x03, 0x69, 0xC7] };
    const PUBLIC_DOCUMENTS: Guid = Guid { data1: 0xED4824AF, data2: 0xDCE4, data3: 0x45A8, data4: [0x81, 0xE2, 0xFC, 0x79, 0x65, 0x08, 0x36, 0x34] };
    const SAVED_GAMES: Guid = Guid { data1: 0x4C5C32FF, data2: 0xBB9D, data3: 0x43B0, data4: [0xBF, 0x9C, 0x4A, 0x39, 0xA0, 0xA5, 0xA1, 0xB0] };
    const PROFILE: Guid = Guid { data1: 0x5E6C858F, data2: 0x0E22, data3: 0x4760, data4: [0x9A, 0xFE, 0xEA, 0x33, 0x17, 0xB6, 0x71, 0x73] };

    let guid = match kind {
        KnownFolderKind::LocalAppData => &LOCAL_APP_DATA,
        KnownFolderKind::LocalAppDataLow => &LOCAL_APP_DATA_LOW,
        KnownFolderKind::RoamingAppData => &ROAMING_APP_DATA,
        KnownFolderKind::Documents => &DOCUMENTS,
        KnownFolderKind::PublicDocuments => &PUBLIC_DOCUMENTS,
        KnownFolderKind::SavedGames => &SAVED_GAMES,
        KnownFolderKind::UserProfile => &PROFILE,
    };
    let mut raw: *mut u16 = std::ptr::null_mut();
    let result = unsafe { SHGetKnownFolderPath(guid, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return Err(format!("SHGetKnownFolderPath failed: 0x{:08X}", result as u32));
    }
    let mut len = 0usize;
    unsafe {
        while *raw.add(len) != 0 {
            len += 1;
        }
    }
    let path = std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(raw, len) });
    unsafe { CoTaskMemFree(raw.cast()) };
    Ok(PathBuf::from(path))
}

#[cfg(not(target_os = "windows"))]
fn known_folder_path(kind: KnownFolderKind) -> Result<PathBuf, String> {
    let variable = match kind {
        KnownFolderKind::LocalAppData | KnownFolderKind::LocalAppDataLow => "LOCALAPPDATA",
        KnownFolderKind::RoamingAppData => "APPDATA",
        KnownFolderKind::UserProfile | KnownFolderKind::Documents | KnownFolderKind::SavedGames => "USERPROFILE",
        KnownFolderKind::PublicDocuments => "PUBLIC",
    };
    let base = PathBuf::from(std::env::var(variable).map_err(|_| format!("{variable} chưa được đặt"))?);
    Ok(match kind {
        KnownFolderKind::Documents => base.join("Documents"),
        KnownFolderKind::PublicDocuments => base.join("Documents"),
        KnownFolderKind::SavedGames => base.join("Saved Games"),
        _ => base,
    })
}

fn is_reparse_or_symlink(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return Ok(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0);
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal_and_broad_limits() {
        let candidate = PathCandidate {
            base: "KnownFolder.LocalAppData".to_string(),
            path: "../secret".to_string(),
            priority: 0,
            platform: String::new(),
            min_game_version: String::new(),
            max_game_version: String::new(),
            allow_absolute: false,
        };
        assert!(validate_candidate("unsafe-game", &candidate).is_err());
        assert!(validate_limits(&MapLimits {
            max_files: HARD_MAX_FILES + 1,
            max_total_bytes: 1,
            max_file_bytes: 1,
        })
        .is_err());
    }

    #[test]
    fn canonical_json_sorts_nested_keys() {
        let value: Value = serde_json::from_str(r#"{"z":1,"a":{"d":2,"b":1}}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            r#"{"a":{"b":1,"d":2},"z":1}"#
        );
    }

    #[test]
    fn rollout_is_stable_for_device_and_seed() {
        let rollout = Rollout { percentage: 50, seed: "map-42".to_string() };
        assert_eq!(device_in_rollout("device-a", &rollout), device_in_rollout("device-a", &rollout));
    }
}
