use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

use crate::managed_game_runtime::{self, PeArchitecture};

const SETTINGS_SCHEMA_VERSION: u32 = 1;
const APPROVAL_SCHEMA_VERSION: u32 = 1;
const CONTRACT_VERSION: &str = "gse-uc-1.8.2-contract-v1";
const EXPECTED_GSE_COMMIT: &str = "e4035e085a028a195c2e3f13e3207e0dfdadce57";
const MAX_SCAN_ENTRIES: usize = 20_000;
const MAX_RUNTIME_TARGETS: usize = 64;
const MAX_EXECUTABLES: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaRuntimePackage {
    GseRegular,
    GseExperimental,
    GseColdClient,
    GseColdClientV1,
    UcOnline2,
    RuneRegular,
    RuneSteakClient,
    RuneSteamClient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaOverlayRenderer {
    GseNative,
    ReshadeCompatibility,
    DesktopFallback,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaSteamStubMode {
    Disabled,
    AutoSteamless,
    Steamless,
    RuneProxy,
    UcRuntime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaSaveMode {
    Global,
    Portable,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaNetworkMode {
    Offline,
    Lan,
    UcOnline2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaAccountMode {
    LocalEmulated,
    SteamClientSpacewar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaRuntimeSettings {
    pub schema_version: u32,
    pub default_package: LuaRuntimePackage,
    pub renderer: LuaOverlayRenderer,
    pub overlay_hotkey: String,
    pub desktop_fallback_hotkey: String,
    pub theme: String,
    pub scale: f32,
    pub opacity: f32,
    pub sound_enabled: bool,
    pub notification_duration_ms: u32,
    pub telemetry_enabled: bool,
    pub reduced_motion: bool,
    pub save_mode: LuaSaveMode,
    pub custom_save_root: Option<String>,
    pub network_mode: LuaNetworkMode,
    pub account_mode: LuaAccountMode,
    pub steam_stub_mode: LuaSteamStubMode,
    pub resource_channel: String,
    pub advanced_features: bool,
}

impl Default for LuaRuntimeSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            default_package: LuaRuntimePackage::GseRegular,
            // A renderer is never enabled merely because a preferred package was selected.
            renderer: LuaOverlayRenderer::Disabled,
            overlay_hotkey: "Shift+Tab".to_string(),
            desktop_fallback_hotkey: "Shift+F1".to_string(),
            theme: "launcher".to_string(),
            scale: 1.0,
            opacity: 0.94,
            sound_enabled: true,
            notification_duration_ms: 5_000,
            telemetry_enabled: false,
            reduced_motion: false,
            save_mode: LuaSaveMode::Global,
            custom_save_root: None,
            network_mode: LuaNetworkMode::Offline,
            account_mode: LuaAccountMode::LocalEmulated,
            steam_stub_mode: LuaSteamStubMode::Disabled,
            resource_channel: "stable".to_string(),
            advanced_features: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaRuntimeComponentStatus {
    Available,
    OnDemand,
    Blocked,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuaRuntimeComponentHealth {
    pub id: String,
    pub label: String,
    pub status: LuaRuntimeComponentStatus,
    pub version: Option<String>,
    pub canonical_source: Option<String>,
    pub immutable_commit: Option<String>,
    pub integrity_verified: bool,
    pub provenance_verified: bool,
    pub artifact_sha256: Option<String>,
    pub license: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaRuntimeSettingsState {
    pub contract_version: String,
    pub settings: LuaRuntimeSettings,
    pub components: Vec<LuaRuntimeComponentHealth>,
    pub activation_allowed: bool,
    pub activation_blocked_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuaRuntimeScannedFile {
    pub relative_path: String,
    pub architecture: Option<PeArchitecture>,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuaRuntimeTargetScan {
    pub schema_version: u32,
    pub app_id: u32,
    pub install_root: String,
    pub runtime_targets: Vec<LuaRuntimeScannedFile>,
    pub executables: Vec<LuaRuntimeScannedFile>,
    pub anti_cheat_signals: Vec<String>,
    pub reparse_points: Vec<String>,
    pub warnings: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub approval_fingerprint: String,
    pub approval_eligible: bool,
    pub locally_approved: bool,
    pub approved_at: Option<String>,
    pub can_apply: bool,
    pub apply_blocked_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaRuntimeApproval {
    app_id: u32,
    fingerprint: String,
    approved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaRuntimeApprovalStore {
    schema_version: u32,
    approvals: BTreeMap<String, LuaRuntimeApproval>,
}

impl Default for LuaRuntimeApprovalStore {
    fn default() -> Self {
        Self {
            schema_version: APPROVAL_SCHEMA_VERSION,
            approvals: BTreeMap::new(),
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("lua-runtime").join("settings-v1.json"))
        .map_err(|error| format!("Could not resolve Lua runtime settings path: {error}"))
}

fn approvals_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("lua-runtime").join("local-approvals-v1.json"))
        .map_err(|error| format!("Could not resolve Lua runtime approvals path: {error}"))
}

fn load_json_or_default<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.is_file() {
        return Ok(T::default());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Stored file {} is invalid: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not serialize {}: {error}", path.display()))?;
    crate::lua_live::atomic_write_path(path, &bytes)
}

fn validate_settings(settings: &mut LuaRuntimeSettings) -> Result<(), String> {
    settings.schema_version = SETTINGS_SCHEMA_VERSION;
    settings.overlay_hotkey = settings.overlay_hotkey.trim().to_string();
    settings.desktop_fallback_hotkey = settings.desktop_fallback_hotkey.trim().to_string();
    settings.theme = settings.theme.trim().to_ascii_lowercase();
    settings.resource_channel = settings.resource_channel.trim().to_ascii_lowercase();

    if settings.overlay_hotkey != "Shift+Tab" {
        return Err("The canonical in-game overlay hotkey must be Shift+Tab".to_string());
    }
    if settings.desktop_fallback_hotkey != "Shift+F1" {
        return Err("The desktop diagnostic overlay hotkey must be Shift+F1".to_string());
    }
    if !matches!(
        settings.theme.as_str(),
        "launcher" | "dark" | "high-contrast"
    ) {
        return Err("Overlay theme must be launcher, dark, or high-contrast".to_string());
    }
    if !(0.75..=2.0).contains(&settings.scale) || !settings.scale.is_finite() {
        return Err("Overlay scale must be between 0.75 and 2.0".to_string());
    }
    if !(0.35..=1.0).contains(&settings.opacity) || !settings.opacity.is_finite() {
        return Err("Overlay opacity must be between 0.35 and 1.0".to_string());
    }
    if !(1_000..=15_000).contains(&settings.notification_duration_ms) {
        return Err(
            "Achievement notification duration must be between 1000 and 15000 ms".to_string(),
        );
    }
    if !matches!(settings.resource_channel.as_str(), "stable" | "pinned") {
        return Err("Resource channel must be stable or pinned".to_string());
    }

    match settings.save_mode {
        LuaSaveMode::Custom => {
            let raw = settings
                .custom_save_root
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Custom save mode requires an absolute save root".to_string())?;
            let path = Path::new(raw);
            if !path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(
                    "Custom save root must be an absolute path without parent traversal"
                        .to_string(),
                );
            }
            settings.custom_save_root = Some(raw.to_string());
        }
        _ => settings.custom_save_root = None,
    }

    let requires_advanced = !matches!(settings.default_package, LuaRuntimePackage::GseRegular)
        || matches!(settings.renderer, LuaOverlayRenderer::ReshadeCompatibility)
        || !matches!(
            settings.network_mode,
            LuaNetworkMode::Offline | LuaNetworkMode::Lan
        )
        || !matches!(settings.account_mode, LuaAccountMode::LocalEmulated)
        || !matches!(settings.steam_stub_mode, LuaSteamStubMode::Disabled);
    if requires_advanced && !settings.advanced_features {
        return Err("UC, RUNE, ReShade, SteamStub and Steam-client identity modes require Advanced features".to_string());
    }

    if matches!(settings.renderer, LuaOverlayRenderer::GseNative)
        && !matches!(settings.default_package, LuaRuntimePackage::GseExperimental)
    {
        return Err("The native Shift+Tab renderer requires GSE Experimental".to_string());
    }
    if matches!(settings.renderer, LuaOverlayRenderer::ReshadeCompatibility)
        && matches!(
            settings.default_package,
            LuaRuntimePackage::GseColdClient | LuaRuntimePackage::GseColdClientV1
        )
    {
        return Err("ReShade compatibility is not valid for a ColdClient profile".to_string());
    }
    if matches!(settings.network_mode, LuaNetworkMode::UcOnline2)
        || matches!(settings.account_mode, LuaAccountMode::SteamClientSpacewar)
        || matches!(settings.steam_stub_mode, LuaSteamStubMode::UcRuntime)
    {
        if !matches!(settings.default_package, LuaRuntimePackage::UcOnline2) {
            return Err("UC Online2, Steam client/Spacewar identity, and UC runtime require the UC Online2 package".to_string());
        }
    }
    if matches!(settings.steam_stub_mode, LuaSteamStubMode::RuneProxy)
        && !matches!(
            settings.default_package,
            LuaRuntimePackage::RuneRegular
                | LuaRuntimePackage::RuneSteakClient
                | LuaRuntimePackage::RuneSteamClient
        )
    {
        return Err("RUNE proxy mode requires a RUNE package".to_string());
    }
    Ok(())
}

fn component_health(app: &AppHandle) -> Vec<LuaRuntimeComponentHealth> {
    let mut rows = crate::gse_uc_setup::lua_component_summary(app);
    rows.push(LuaRuntimeComponentHealth {
        id: "luaVariantVault".to_string(),
        label: "Lua Variant Vault".to_string(),
        status: LuaRuntimeComponentStatus::Available,
        version: Some("1".to_string()),
        canonical_source: None,
        immutable_commit: None,
        integrity_verified: true,
        provenance_verified: true,
        artifact_sha256: None,
        license: None,
        detail: "Exact-byte capture, drift detection, export and transactional restore are available.".to_string(),
    });
    rows.push(LuaRuntimeComponentHealth {
        id: "saveCloud".to_string(),
        label: "Save manager + launcher cloud vault".to_string(),
        status: LuaRuntimeComponentStatus::Available,
        version: Some("1".to_string()),
        canonical_source: None,
        immutable_commit: None,
        integrity_verified: true,
        provenance_verified: true,
        artifact_sha256: None,
        license: None,
        detail: "Uses the launcher's existing credential vault; no Python token store is imported.".to_string(),
    });
    rows
}

fn make_settings_state(app: &AppHandle, settings: LuaRuntimeSettings) -> LuaRuntimeSettingsState {
    let components = component_health(app);
    let selected_id = match settings.default_package {
        LuaRuntimePackage::GseRegular => "gseRegular",
        LuaRuntimePackage::GseExperimental => "gseExperimental",
        LuaRuntimePackage::GseColdClient => "gseColdClient",
        LuaRuntimePackage::GseColdClientV1 => "gseColdClientV1",
        LuaRuntimePackage::UcOnline2 => "ucOnline2",
        LuaRuntimePackage::RuneRegular => "runeRegular",
        LuaRuntimePackage::RuneSteakClient => "runeSteakClient",
        LuaRuntimePackage::RuneSteamClient => "runeSteamClient",
    };
    let selected = components.iter().find(|row| row.id == selected_id);
    let activation_allowed = selected.is_some_and(|row| row.integrity_verified);
    let activation_blocked_reason = if activation_allowed {
        "Bundled runtime integrity is verified. Per-game scan, approval and compatibility gates still apply before mutation.".to_string()
    } else {
        format!("Selected runtime component {selected_id} is unavailable or failed integrity verification")
    };
    LuaRuntimeSettingsState {
        contract_version: CONTRACT_VERSION.to_string(),
        settings,
        components,
        activation_allowed,
        activation_blocked_reason,
    }
}

#[tauri::command]
pub fn get_lua_runtime_settings(app: AppHandle) -> Result<LuaRuntimeSettingsState, String> {
    let mut settings: LuaRuntimeSettings = load_json_or_default(&settings_path(&app)?)?;
    validate_settings(&mut settings)?;
    Ok(make_settings_state(&app, settings))
}

#[tauri::command]
pub fn save_lua_runtime_settings(
    app: AppHandle,
    mut settings: LuaRuntimeSettings,
) -> Result<LuaRuntimeSettingsState, String> {
    validate_settings(&mut settings)?;
    write_json(&settings_path(&app)?, &settings)?;
    Ok(make_settings_state(&app, settings))
}

#[tauri::command]
pub fn get_lua_runtime_component_health(app: AppHandle) -> Vec<LuaRuntimeComponentHealth> {
    component_health(&app)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(target_os = "windows")]
fn is_reparse_point(path: &Path) -> Result<bool, String> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .map_err(|error| {
            format!(
                "Could not inspect reparse attributes for {}: {error}",
                path.display()
            )
        })
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(path: &Path) -> Result<bool, String> {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .map_err(|error| {
            format!(
                "Could not inspect symlink attributes for {}: {error}",
                path.display()
            )
        })
}

fn has_anti_cheat_signal(relative_lower: &str) -> bool {
    [
        "easyanticheat",
        "easyanticheat_eos",
        "battleye",
        "beclient",
        "bedaisy",
        "xigncode",
        "equ8",
        "faceit",
        "mhyprot",
        "gameguard",
        "nprotect",
        "vgk.sys",
    ]
    .iter()
    .any(|needle| relative_lower.contains(needle))
}

fn scanned_file(root: &Path, path: &Path) -> Result<LuaRuntimeScannedFile, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    Ok(LuaRuntimeScannedFile {
        relative_path: relative_display(root, path),
        architecture: managed_game_runtime::read_pe_architecture(path).ok(),
        sha256: sha256_file(path)?,
        size_bytes: metadata.len(),
    })
}

fn scan_fingerprint(
    app_id: u32,
    root: &Path,
    targets: &[LuaRuntimeScannedFile],
    executables: &[LuaRuntimeScannedFile],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("lua-runtime-approval-v1\0{app_id}\0{}\0", root.display()).as_bytes());
    for file in targets.iter().chain(executables) {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update([0]);
        hasher.update(file.size_bytes.to_le_bytes());
        hasher.update([0]);
        hasher.update(match file.architecture {
            Some(PeArchitecture::X86) => b"x86".as_slice(),
            Some(PeArchitecture::X64) => b"x64".as_slice(),
            None => b"unknown".as_slice(),
        });
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn scan_install_root(app_id: u32, root: &Path) -> Result<LuaRuntimeTargetScan, String> {
    if !root.is_dir() {
        return Err(format!(
            "Steam install directory does not exist: {}",
            root.display()
        ));
    }

    let mut runtime_paths = Vec::new();
    let mut executable_paths = Vec::new();
    let mut anti_cheat_signals = Vec::new();
    let mut reparse_points = Vec::new();
    let mut warnings = Vec::new();
    let mut entries_seen = 0usize;
    let mut walker = WalkDir::new(root)
        .follow_links(false)
        .max_depth(12)
        .into_iter();

    while let Some(next) = walker.next() {
        let entry = next.map_err(|error| format!("Could not scan Steam install: {error}"))?;
        entries_seen += 1;
        if entries_seen > MAX_SCAN_ENTRIES {
            return Err(format!(
                "Read-only scan exceeded the {MAX_SCAN_ENTRIES} entry safety limit"
            ));
        }
        let relative = relative_display(root, entry.path());
        if is_reparse_point(entry.path())? {
            reparse_points.push(relative);
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let lower = relative.to_ascii_lowercase();
        if has_anti_cheat_signal(&lower) && anti_cheat_signals.len() < 32 {
            anti_cheat_signals.push(relative.clone());
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if matches!(name.as_str(), "steam_api.dll" | "steam_api64.dll") {
            runtime_paths.push(entry.path().to_path_buf());
        } else if name.ends_with(".exe") {
            executable_paths.push(entry.path().to_path_buf());
        }
    }

    runtime_paths.sort_by_key(|path| relative_display(root, path).to_ascii_lowercase());
    executable_paths.sort_by_key(|path| relative_display(root, path).to_ascii_lowercase());
    anti_cheat_signals.sort();
    reparse_points.sort();

    let mut blocked_reasons = Vec::new();
    if runtime_paths.is_empty() {
        blocked_reasons.push("No steam_api.dll or steam_api64.dll target was found".to_string());
    }
    if executable_paths.is_empty() {
        blocked_reasons.push("No executable was found in the Steam install".to_string());
    }
    if runtime_paths.len() > MAX_RUNTIME_TARGETS {
        blocked_reasons.push(format!("Found more than {MAX_RUNTIME_TARGETS} Steam API targets; narrow the profile before approval"));
        runtime_paths.truncate(MAX_RUNTIME_TARGETS);
    }
    if executable_paths.len() > MAX_EXECUTABLES {
        blocked_reasons.push(format!(
            "Found more than {MAX_EXECUTABLES} executables; narrow the profile before approval"
        ));
        executable_paths.truncate(MAX_EXECUTABLES);
    }
    if !anti_cheat_signals.is_empty() {
        blocked_reasons.push(
            "Anti-cheat or protected-runtime signals were found; runtime mutation is blocked"
                .to_string(),
        );
    }
    if !reparse_points.is_empty() {
        blocked_reasons.push(
            "Reparse points were found inside the target; runtime mutation is blocked".to_string(),
        );
    }

    let mut runtime_targets = Vec::with_capacity(runtime_paths.len());
    for path in runtime_paths {
        let file = scanned_file(root, &path)?;
        let name = Path::new(&file.relative_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let architecture_matches_name = matches!(
            (name.as_str(), file.architecture),
            ("steam_api.dll", Some(PeArchitecture::X86))
                | ("steam_api64.dll", Some(PeArchitecture::X64))
        );
        if !architecture_matches_name {
            blocked_reasons.push(format!(
                "Architecture could not be validated for {}",
                file.relative_path
            ));
        }
        runtime_targets.push(file);
    }

    let mut executables = Vec::with_capacity(executable_paths.len());
    for path in executable_paths {
        let file = scanned_file(root, &path)?;
        if file.architecture.is_none() {
            blocked_reasons.push(format!(
                "Executable architecture is unsupported or unreadable: {}",
                file.relative_path
            ));
        }
        executables.push(file);
    }
    blocked_reasons.sort();
    blocked_reasons.dedup();
    warnings.sort();
    warnings.dedup();

    let fingerprint = scan_fingerprint(app_id, root, &runtime_targets, &executables);
    Ok(LuaRuntimeTargetScan {
        schema_version: 1,
        app_id,
        install_root: root.display().to_string(),
        runtime_targets,
        executables,
        anti_cheat_signals,
        reparse_points,
        warnings,
        approval_fingerprint: fingerprint,
        approval_eligible: blocked_reasons.is_empty(),
        blocked_reasons,
        locally_approved: false,
        approved_at: None,
        can_apply: false,
        apply_blocked_reason: "Read-only profile approval does not bypass the missing provenance-verified runtime package gate.".to_string(),
    })
}

fn load_approvals(app: &AppHandle) -> Result<LuaRuntimeApprovalStore, String> {
    let store: LuaRuntimeApprovalStore = load_json_or_default(&approvals_path(app)?)?;
    if store.schema_version != APPROVAL_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Lua runtime approval schema {}",
            store.schema_version
        ));
    }
    Ok(store)
}

fn attach_approval(app: &AppHandle, scan: &mut LuaRuntimeTargetScan) -> Result<(), String> {
    let approvals = load_approvals(app)?;
    if let Some(approval) = approvals.approvals.get(&scan.app_id.to_string()) {
        if approval.fingerprint == scan.approval_fingerprint {
            scan.locally_approved = true;
            scan.approved_at = Some(approval.approved_at.clone());
        } else {
            scan.warnings.push("The executable or Steam API target set changed; previous local approval is no longer valid".to_string());
        }
    }
    Ok(())
}

fn scan_installed_game(app: &AppHandle, app_id: u32) -> Result<LuaRuntimeTargetScan, String> {
    if app_id == 0 {
        return Err("Steam AppID must be positive".to_string());
    }
    let root = crate::steam_integration::get_steam_game_install_dir(app_id)
        .ok_or_else(|| format!("Steam AppID {app_id} is not installed in a detected library"))?;
    let mut scan = scan_install_root(app_id, Path::new(&root))?;
    attach_approval(app, &mut scan)?;
    Ok(scan)
}

pub(crate) fn scan_lua_runtime_target_sync(
    app: &AppHandle,
    app_id: u32,
) -> Result<LuaRuntimeTargetScan, String> {
    scan_installed_game(app, app_id)
}

#[tauri::command]
pub async fn scan_lua_runtime_target(
    app: AppHandle,
    app_id: u32,
) -> Result<LuaRuntimeTargetScan, String> {
    tauri::async_runtime::spawn_blocking(move || scan_installed_game(&app, app_id))
        .await
        .map_err(|error| format!("Lua runtime scan worker failed: {error}"))?
}

#[tauri::command]
pub fn approve_lua_runtime_target(
    app: AppHandle,
    app_id: u32,
    expected_fingerprint: String,
) -> Result<LuaRuntimeTargetScan, String> {
    let mut scan = scan_installed_game(&app, app_id)?;
    if !scan.approval_eligible {
        return Err(format!(
            "Target cannot be approved: {}",
            scan.blocked_reasons.join("; ")
        ));
    }
    if expected_fingerprint.trim() != scan.approval_fingerprint {
        return Err("Target changed after review; run the read-only scan again".to_string());
    }
    let mut store = load_approvals(&app)?;
    let approved_at = Utc::now().to_rfc3339();
    store.approvals.insert(
        app_id.to_string(),
        LuaRuntimeApproval {
            app_id,
            fingerprint: scan.approval_fingerprint.clone(),
            approved_at: approved_at.clone(),
        },
    );
    write_json(&approvals_path(&app)?, &store)?;
    scan.locally_approved = true;
    scan.approved_at = Some(approved_at);
    Ok(scan)
}

#[tauri::command]
pub fn revoke_lua_runtime_target_approval(app: AppHandle, app_id: u32) -> Result<bool, String> {
    let mut store = load_approvals(&app)?;
    let removed = store.approvals.remove(&app_id.to_string()).is_some();
    if removed {
        write_json(&approvals_path(&app)?, &store)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn write_minimal_pe(path: &Path, machine: u16) {
        let mut bytes = vec![0u8; 80];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&(64u32).to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&machine.to_le_bytes());
        let mut file = File::create(path).unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn settings_default_to_fail_closed_runtime_policy() {
        let mut settings = LuaRuntimeSettings::default();
        validate_settings(&mut settings).unwrap();
        assert_eq!(settings.renderer, LuaOverlayRenderer::Disabled);
        assert_eq!(settings.steam_stub_mode, LuaSteamStubMode::Disabled);
        assert_eq!(settings.network_mode, LuaNetworkMode::Offline);
        assert!(!settings.advanced_features);
    }

    #[test]
    fn uc_and_proxy_combinations_require_explicit_advanced_profile() {
        let mut settings = LuaRuntimeSettings {
            default_package: LuaRuntimePackage::UcOnline2,
            network_mode: LuaNetworkMode::UcOnline2,
            account_mode: LuaAccountMode::SteamClientSpacewar,
            steam_stub_mode: LuaSteamStubMode::UcRuntime,
            ..LuaRuntimeSettings::default()
        };
        assert!(validate_settings(&mut settings).is_err());
        settings.advanced_features = true;
        validate_settings(&mut settings).unwrap();

        settings.default_package = LuaRuntimePackage::GseRegular;
        assert!(validate_settings(&mut settings).is_err());
    }

    #[test]
    fn read_only_scan_hashes_every_target_and_executable() {
        let root = std::env::temp_dir().join(format!("oxo-lua-runtime-scan-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let exe = root.join("game.exe");
        let dll = root.join("steam_api64.dll");
        write_minimal_pe(&exe, 0x8664);
        write_minimal_pe(&dll, 0x8664);

        let first = scan_install_root(480, &root).unwrap();
        assert!(first.approval_eligible);
        assert_eq!(first.runtime_targets.len(), 1);
        assert_eq!(first.executables.len(), 1);
        assert!(!first.can_apply);

        let mut changed = fs::OpenOptions::new().append(true).open(&exe).unwrap();
        changed.write_all(b"changed").unwrap();
        changed.sync_all().unwrap();
        let second = scan_install_root(480, &root).unwrap();
        assert_ne!(first.approval_fingerprint, second.approval_fingerprint);

        fs::remove_file(&exe).unwrap();
        fs::remove_file(&dll).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn anti_cheat_signal_blocks_local_approval_without_mutating_files() {
        let root = std::env::temp_dir().join(format!("oxo-lua-runtime-block-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let exe = root.join("game.exe");
        let dll = root.join("steam_api.dll");
        let protected = root.join("EasyAntiCheat_EOS_Setup.exe");
        write_minimal_pe(&exe, 0x014c);
        write_minimal_pe(&dll, 0x014c);
        write_minimal_pe(&protected, 0x014c);
        let dll_before = fs::read(&dll).unwrap();

        let scan = scan_install_root(480, &root).unwrap();
        assert!(!scan.approval_eligible);
        assert!(!scan.anti_cheat_signals.is_empty());
        assert_eq!(fs::read(&dll).unwrap(), dll_before);

        fs::remove_file(&exe).unwrap();
        fs::remove_file(&dll).unwrap();
        fs::remove_file(&protected).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
