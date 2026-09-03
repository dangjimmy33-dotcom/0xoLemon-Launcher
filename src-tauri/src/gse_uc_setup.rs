use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::lua_runtime_profiles::{
    self, LuaAccountMode, LuaNetworkMode, LuaOverlayRenderer, LuaRuntimeComponentHealth,
    LuaRuntimeComponentStatus, LuaRuntimePackage, LuaRuntimeSettings, LuaRuntimeTargetScan,
    LuaSaveMode, LuaSteamStubMode,
};
use crate::managed_file_transaction::{
    self, ManagedDeleteSpec, ManagedFileChange, ManagedFileSpec,
};
use crate::managed_game_runtime::PeArchitecture;

const MANIFEST_SCHEMA: u32 = 1;
const RECEIPT_SCHEMA: u32 = 1;
const RESOURCE_DIR_NAME: &str = "gse-uc";
const RECEIPT_DIR: &str = "lua-runtime/gse-uc-receipts";
const BACKUP_DIR: &str = "lua-runtime/gse-uc-originals";
const STAGE_DIR: &str = "lua-runtime/gse-uc-stage";
const UPDATE_DIR: &str = "lua-runtime/gse-uc-updates/active";
const MAX_ACTIONS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GseUcManifestFile {
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GseUcManifestComponent {
    id: String,
    label: String,
    source: Option<String>,
    tag: Option<String>,
    declared_sha256: Option<String>,
    license: Option<String>,
    required_files: Vec<String>,
    file_count: usize,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    missing_required_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GseUcManifest {
    schema_version: u32,
    package_version: String,
    source_snapshot: String,
    integrity_model: String,
    provenance_model: String,
    components: Vec<GseUcManifestComponent>,
    files: Vec<GseUcManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GseUcComponentHealth {
    pub id: String,
    pub label: String,
    pub status: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub integrity_verified: bool,
    pub provenance_verified: bool,
    pub file_count: usize,
    pub checked_files: usize,
    pub missing_files: Vec<String>,
    pub corrupt_files: Vec<String>,
    pub license: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GseUcActionKind {
    Create,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GseUcFileAction {
    pub kind: GseUcActionKind,
    pub component_id: String,
    pub architecture: Option<PeArchitecture>,
    pub target_relative_path: String,
    pub source_relative_path: String,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseUcPlan {
    pub schema_version: u32,
    pub app_id: u32,
    pub package: LuaRuntimePackage,
    pub install_root: String,
    pub approval_fingerprint: String,
    pub locally_approved: bool,
    pub can_apply: bool,
    pub blocked_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub actions: Vec<GseUcFileAction>,
    pub component_health: Vec<GseUcComponentHealth>,
    pub steam_stub_mode: LuaSteamStubMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseUcOwnedFileReceipt {
    pub target_relative_path: String,
    pub managed_sha256: String,
    pub original_sha256: Option<String>,
    pub original_backup_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GseUcReceipt {
    schema_version: u32,
    app_id: u32,
    package: LuaRuntimePackage,
    transaction_id: String,
    approval_fingerprint: String,
    install_root: String,
    active: bool,
    owned_files: Vec<GseUcOwnedFileReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseUcReceiptState {
    pub schema_version: u32,
    pub app_id: u32,
    pub package: Option<LuaRuntimePackage>,
    pub status: String,
    pub transaction_id: Option<String>,
    pub approval_fingerprint: Option<String>,
    pub owned_files: Vec<GseUcOwnedFileReceipt>,
    pub message: String,
}

static HEALTH_CACHE: OnceLock<Mutex<Option<(String, Vec<GseUcComponentHealth>)>>> = OnceLock::new();

fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("invalid SHA-256: {value}"));
    }
    Ok(normalized)
}

fn validate_relative_path(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.is_absolute() || raw.trim().is_empty() {
        return Err(format!("unsafe relative path: {raw}"));
    }
    for part in path.components() {
        match part {
            Component::Normal(_) => {}
            _ => return Err(format!("unsafe relative path: {raw}")),
        }
    }
    Ok(path.to_path_buf())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_manifest(root: &Path) -> Result<(GseUcManifest, String), String> {
    let path = root.join("manifest.json");
    let bytes = fs::read(&path)
        .map_err(|error| format!("Could not read GSE/UC manifest {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let manifest_hash = format!("{:x}", hasher.finalize());
    let manifest: GseUcManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid GSE/UC manifest {}: {error}", path.display()))?;
    if manifest.schema_version != MANIFEST_SCHEMA {
        return Err(format!(
            "Unsupported GSE/UC manifest schema {}",
            manifest.schema_version
        ));
    }
    if manifest.integrity_model != "sha256-per-file" {
        return Err("Unsupported GSE/UC integrity model".to_string());
    }
    let mut seen = BTreeSet::new();
    for row in &manifest.files {
        validate_relative_path(&row.relative_path)?;
        normalize_sha256(&row.sha256)?;
        if !seen.insert(row.relative_path.replace('\\', "/").to_ascii_lowercase()) {
            return Err(format!("Duplicate GSE/UC manifest path {}", row.relative_path));
        }
    }
    Ok((manifest, manifest_hash))
}

fn resource_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(local) = app.path().app_local_data_dir() {
        candidates.push(local.join(UPDATE_DIR));
    }
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("resources").join(RESOURCE_DIR_NAME));
        candidates.push(resources.join(RESOURCE_DIR_NAME));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/gse-uc"));
    candidates
}

fn resource_root(app: &AppHandle) -> Result<PathBuf, String> {
    for candidate in resource_candidates(app) {
        if candidate.join("manifest.json").is_file() {
            return Ok(candidate);
        }
    }
    Err("GSE/UC bundled resource root is unavailable".to_string())
}

fn manifest_file_index(manifest: &GseUcManifest) -> BTreeMap<&str, &GseUcManifestFile> {
    manifest
        .files
        .iter()
        .map(|row| (row.relative_path.as_str(), row))
        .collect()
}

fn verify_component_at(
    root: &Path,
    manifest: &GseUcManifest,
    component: &GseUcManifestComponent,
) -> GseUcComponentHealth {
    let index = manifest_file_index(manifest);
    let files: Vec<String> = if component.files.is_empty() {
        component.required_files.clone()
    } else {
        component.files.clone()
    };
    let mut missing = Vec::new();
    let mut corrupt = Vec::new();
    let mut checked = 0usize;

    for relative in &files {
        let Some(expected) = index.get(relative.as_str()) else {
            missing.push(format!("{relative} (not in manifest)"));
            continue;
        };
        let Ok(relative_path) = validate_relative_path(relative) else {
            corrupt.push(format!("{relative} (unsafe path)"));
            continue;
        };
        let path = root.join(relative_path);
        if !path.is_file() {
            missing.push(relative.clone());
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            missing.push(relative.clone());
            continue;
        };
        if metadata.len() != expected.size_bytes {
            corrupt.push(format!("{relative} (size mismatch)"));
            continue;
        }
        match sha256_file(&path) {
            Ok(actual) if actual == expected.sha256 => checked += 1,
            Ok(_) => corrupt.push(format!("{relative} (SHA-256 mismatch)")),
            Err(error) => corrupt.push(format!("{relative} ({error})")),
        }
    }

    for required in &component.required_files {
        if !root.join(required).is_file() && !missing.contains(required) {
            missing.push(required.clone());
        }
    }
    missing.sort();
    missing.dedup();
    corrupt.sort();
    corrupt.dedup();
    let integrity_verified = missing.is_empty() && corrupt.is_empty() && checked == files.len();
    let status = if integrity_verified {
        "available"
    } else if !missing.is_empty() {
        "unavailable"
    } else {
        "blocked"
    };
    let detail = if integrity_verified {
        format!(
            "Integrity verified against the bundled SHA-256 manifest ({} files). Build provenance remains unverified for this user-supplied snapshot.",
            checked
        )
    } else {
        "RUNTIME_INTEGRITY_FAILED: one or more bundled files are missing or do not match the manifest"
            .to_string()
    };

    GseUcComponentHealth {
        id: component.id.clone(),
        label: component.label.clone(),
        status: status.to_string(),
        version: component.tag.clone().or_else(|| Some(manifest.package_version.clone())),
        source: component.source.clone(),
        integrity_verified,
        // Hash equality to a user-supplied snapshot is deliberately not equivalent to
        // a clean reproducible source/build provenance claim.
        provenance_verified: false,
        file_count: files.len(),
        checked_files: checked,
        missing_files: missing,
        corrupt_files: corrupt,
        license: component.license.clone(),
        detail,
    }
}

fn compute_health(app: &AppHandle, use_cache: bool) -> Result<Vec<GseUcComponentHealth>, String> {
    let root = resource_root(app)?;
    let (manifest, manifest_hash) = read_manifest(&root)?;
    let cache_key = format!("{}:{manifest_hash}", root.display());
    let cache = HEALTH_CACHE.get_or_init(|| Mutex::new(None));
    if use_cache {
        if let Ok(guard) = cache.lock() {
            if let Some((key, health)) = guard.as_ref() {
                if key == &cache_key {
                    return Ok(health.clone());
                }
            }
        }
    }

    let health: Vec<_> = manifest
        .components
        .iter()
        .map(|component| verify_component_at(&root, &manifest, component))
        .collect();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((cache_key, health.clone()));
    }
    Ok(health)
}

fn health_for_component(
    app: &AppHandle,
    component_id: &str,
    force_verify: bool,
) -> Result<GseUcComponentHealth, String> {
    if force_verify {
        let root = resource_root(app)?;
        let (manifest, _) = read_manifest(&root)?;
        let component = manifest
            .components
            .iter()
            .find(|component| component.id == component_id)
            .ok_or_else(|| format!("GSE/UC component {component_id} is absent from the manifest"))?;
        return Ok(verify_component_at(&root, &manifest, component));
    }
    compute_health(app, true)?
        .into_iter()
        .find(|component| component.id == component_id)
        .ok_or_else(|| format!("GSE/UC component {component_id} is absent from the manifest"))
}

#[tauri::command]
pub async fn get_gse_uc_resource_health(app: AppHandle) -> Result<Vec<GseUcComponentHealth>, String> {
    tauri::async_runtime::spawn_blocking(move || compute_health(&app, true))
        .await
        .map_err(|error| format!("GSE/UC health worker failed: {error}"))?
}

/// Lightweight component metadata for launcher Settings/startup surfaces.
///
/// This intentionally reads only the bundle manifest. It must not open/hash payload
/// files because the full GSE/UC bundle can contain hundreds of megabytes and many
/// files. Integrity remains false until an explicit Dry Run/Verify/Apply path performs
/// strict component verification.
pub fn lua_component_summary(app: &AppHandle) -> Vec<LuaRuntimeComponentHealth> {
    let result = (|| -> Result<Vec<LuaRuntimeComponentHealth>, String> {
        let root = resource_root(app)?;
        let (manifest, _) = read_manifest(&root)?;
        let package_version = manifest.package_version.clone();

        Ok(manifest
            .components
            .into_iter()
            .map(|component| {
                let file_count = if component.files.is_empty() {
                    component.required_files.len()
                } else {
                    component.files.len()
                };
                LuaRuntimeComponentHealth {
                    id: component.id,
                    label: component.label,
                    status: LuaRuntimeComponentStatus::OnDemand,
                    version: component.tag.or_else(|| Some(package_version.clone())),
                    canonical_source: component.source,
                    immutable_commit: None,
                    integrity_verified: false,
                    provenance_verified: false,
                    artifact_sha256: component.declared_sha256,
                    license: component.license,
                    detail: format!(
                        "Bundled metadata available ({file_count} files). Payload hashing is deferred until Dry Run/Verify."
                    ),
                }
            })
            .collect())
    })();

    match result {
        Ok(rows) => rows,
        Err(error) => vec![LuaRuntimeComponentHealth {
            id: "gseUcBundle".to_string(),
            label: "GSE / UC resource bundle".to_string(),
            status: LuaRuntimeComponentStatus::Unavailable,
            version: None,
            canonical_source: None,
            immutable_commit: None,
            integrity_verified: false,
            provenance_verified: false,
            artifact_sha256: None,
            license: None,
            detail: error,
        }],
    }
}

pub fn lua_component_health(app: &AppHandle) -> Vec<LuaRuntimeComponentHealth> {
    match compute_health(app, true) {
        Ok(rows) => rows
            .into_iter()
            .map(|row| LuaRuntimeComponentHealth {
                id: row.id,
                label: row.label,
                status: match row.status.as_str() {
                    "available" => LuaRuntimeComponentStatus::Available,
                    "blocked" => LuaRuntimeComponentStatus::Blocked,
                    _ => LuaRuntimeComponentStatus::Unavailable,
                },
                version: row.version,
                canonical_source: row.source,
                immutable_commit: None,
                integrity_verified: row.integrity_verified,
                provenance_verified: row.provenance_verified,
                artifact_sha256: None,
                license: row.license,
                detail: row.detail,
            })
            .collect(),
        Err(error) => vec![LuaRuntimeComponentHealth {
            id: "gseUcBundle".to_string(),
            label: "GSE / UC resource bundle".to_string(),
            status: LuaRuntimeComponentStatus::Unavailable,
            version: None,
            canonical_source: None,
            immutable_commit: None,
            integrity_verified: false,
            provenance_verified: false,
            artifact_sha256: None,
            license: None,
            detail: error,
        }],
    }
}

fn package_id(package: LuaRuntimePackage) -> &'static str {
    match package {
        LuaRuntimePackage::GseRegular => "gseRegular",
        LuaRuntimePackage::GseExperimental => "gseExperimental",
        LuaRuntimePackage::GseColdClient => "gseColdClient",
        LuaRuntimePackage::GseColdClientV1 => "gseColdClientV1",
        LuaRuntimePackage::UcOnline2 => "ucOnline2",
        LuaRuntimePackage::RuneRegular => "runeRegular",
        LuaRuntimePackage::RuneSteakClient => "runeSteakClient",
        LuaRuntimePackage::RuneSteamClient => "runeSteamClient",
    }
}

fn runtime_source_for(package: LuaRuntimePackage, arch: PeArchitecture) -> Option<&'static str> {
    match (package, arch) {
        (LuaRuntimePackage::GseRegular, PeArchitecture::X86) => {
            Some("embedded/gse/regular/x86/steam_api.dll")
        }
        (LuaRuntimePackage::GseRegular, PeArchitecture::X64) => {
            Some("embedded/gse/regular/x64/steam_api64.dll")
        }
        (LuaRuntimePackage::GseExperimental, PeArchitecture::X86) => {
            Some("embedded/gse/experimental/x86/steam_api.dll")
        }
        (LuaRuntimePackage::GseExperimental, PeArchitecture::X64) => {
            Some("embedded/gse/experimental/x64/steam_api64.dll")
        }
        (LuaRuntimePackage::UcOnline2, PeArchitecture::X86) => Some(
            "embedded/uc_online/uc-online2-v1.7.0a-release/x86/steam_api.dll",
        ),
        (LuaRuntimePackage::UcOnline2, PeArchitecture::X64) => Some(
            "embedded/uc_online/uc-online2-v1.7.0a-release/x64/steam_api64.dll",
        ),
        (LuaRuntimePackage::RuneRegular, PeArchitecture::X86) => {
            Some("embedded/rune/emu/steam_api.dll")
        }
        (LuaRuntimePackage::RuneRegular, PeArchitecture::X64) => {
            Some("embedded/rune/emu/steam_api64.dll")
        }
        _ => None,
    }
}

fn manifest_file<'a>(manifest: &'a GseUcManifest, relative: &str) -> Result<&'a GseUcManifestFile, String> {
    manifest
        .files
        .iter()
        .find(|row| row.relative_path == relative)
        .ok_or_else(|| format!("Manifest file is missing: {relative}"))
}

fn file_hash_if_exists(path: &Path) -> Result<Option<String>, String> {
    if path.is_file() {
        sha256_file(path).map(Some)
    } else {
        Ok(None)
    }
}

fn generated_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn add_action(
    actions: &mut Vec<GseUcFileAction>,
    component_id: &str,
    architecture: Option<PeArchitecture>,
    install_root: &Path,
    target_relative_path: String,
    source_relative_path: String,
    after_sha256: String,
    generated: bool,
) -> Result<(), String> {
    validate_relative_path(&target_relative_path)?;
    if !generated {
        validate_relative_path(&source_relative_path)?;
    }
    if actions.len() >= MAX_ACTIONS {
        return Err(format!("Runtime plan exceeds {MAX_ACTIONS} file actions"));
    }
    if actions.iter().any(|row| row.target_relative_path.eq_ignore_ascii_case(&target_relative_path)) {
        return Err(format!("Runtime plan contains a duplicate target: {target_relative_path}"));
    }
    let target = install_root.join(&target_relative_path);
    let before = file_hash_if_exists(&target)?;
    let kind = if before.is_some() {
        GseUcActionKind::Replace
    } else {
        GseUcActionKind::Create
    };
    actions.push(GseUcFileAction {
        kind,
        component_id: component_id.to_string(),
        architecture,
        target_relative_path,
        source_relative_path,
        before_sha256: before,
        after_sha256,
        generated,
    });
    Ok(())
}

fn target_parent_relative(relative: &str) -> Result<PathBuf, String> {
    let path = validate_relative_path(relative)?;
    Ok(path.parent().unwrap_or_else(|| Path::new("")).to_path_buf())
}

fn generated_marker(kind: &str, target_relative: &str) -> String {
    format!("@generated/{kind}/{}", target_relative.replace('\\', "/"))
}

fn steamless_generated_bytes(source_exe: &Path, scratch_root: &Path, label: &str) -> Result<Vec<u8>, String> {
    if !source_exe.is_file() {
        return Err(format!("Steamless source executable does not exist: {}", source_exe.display()));
    }
    let work = scratch_root.join(format!("steamless-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&work)
        .map_err(|error| format!("Could not create Steamless scratch directory: {error}"))?;
    let file_name = source_exe
        .file_name()
        .ok_or_else(|| format!("Steamless source has no filename: {}", source_exe.display()))?;
    let staged_exe = work.join(file_name);
    let result = (|| -> Result<Vec<u8>, String> {
        fs::copy(source_exe, &staged_exe).map_err(|error| {
            format!(
                "Could not stage executable for Steamless {} -> {}: {error}",
                source_exe.display(),
                staged_exe.display()
            )
        })?;
        File::open(&staged_exe)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("Could not flush Steamless staged executable: {error}"))?;
        let patched = crate::steamless::unpack_exe(&staged_exe, ".0xo-stage-original.exe");
        if !patched.success {
            return Err(format!("STEAMLESS_NOT_APPLICABLE: {}", patched.message));
        }
        fs::read(&staged_exe)
            .map_err(|error| format!("Could not read staged Steamless output: {error}"))
    })();
    if let Err(error) = fs::remove_dir_all(&work) {
        if result.is_ok() {
            return Err(format!("Could not clean Steamless scratch directory {}: {error}", work.display()));
        }
    }
    result
}

fn gse_generated_files(
    settings: &LuaRuntimeSettings,
    app_id: u32,
    base: &Path,
) -> Vec<(PathBuf, Vec<u8>)> {
    let save_path = match settings.save_mode {
        LuaSaveMode::Global => "".to_string(),
        LuaSaveMode::Portable => "./steam_settings/saves".to_string(),
        LuaSaveMode::Custom => settings.custom_save_root.clone().unwrap_or_default(),
    };
    let (offline, disable_networking) = match settings.network_mode {
        LuaNetworkMode::Lan => ("0", "0"),
        _ => ("0", "1"),
    };
    let overlay_enabled = matches!(settings.renderer, LuaOverlayRenderer::GseNative);
    let steam_settings = base.join("steam_settings");
    vec![
        (steam_settings.join("steam_appid.txt"), format!("{app_id}\n").into_bytes()),
        (
            steam_settings.join("configs.app.ini"),
            b"[app::general]\nbranch_name=public\n".to_vec(),
        ),
        (
            steam_settings.join("configs.user.ini"),
            format!(
                "[user::general]\naccount_name=0xoLemon\nlanguage=english\n\n[user::saves]\nlocal_save_path={}\nsaves_folder_name=GSE Saves\n",
                save_path
            )
            .into_bytes(),
        ),
        (
            steam_settings.join("configs.main.ini"),
            format!(
                "[main::connectivity]\noffline={}\ndisable_networking={}\ndisable_lan_only=0\n",
                offline, disable_networking
            )
            .into_bytes(),
        ),
        (
            steam_settings.join("configs.overlay.ini"),
            format!(
                "[overlay::general]\nenable_experimental_overlay={}\ndisable_achievement_notification=0\ndisable_friend_notification=0\ndisable_warning_any=0\n\n[overlay::hotkeys]\nkey_combo=shift + tab\n\n[overlay::appearance]\nNotification_Animation={}\nNotification_Duration_Achievement={}\n",
                if overlay_enabled { 1 } else { 0 },
                if settings.reduced_motion { 0.0 } else { 0.35 },
                (settings.notification_duration_ms as f32 / 1000.0)
            )
            .into_bytes(),
        ),
    ]
}

fn uc_ini(settings: &LuaRuntimeSettings, app_id: u32) -> Vec<u8> {
    let spoof = if matches!(settings.account_mode, LuaAccountMode::SteamClientSpacewar) {
        480
    } else {
        app_id
    };
    let runtime_stub = matches!(settings.steam_stub_mode, LuaSteamStubMode::UcRuntime);
    format!(
        "[Steam]\nAppId={app_id}\nSpoofAppId={spoof}\nRuntimeSteamStub={}\nPlugins=0\n",
        if runtime_stub { 1 } else { 0 }
    )
    .into_bytes()
}

fn render_rune_ini(template: &str, app_id: u32, settings: &LuaRuntimeSettings) -> String {
    let mut value = template.replace("SteamID", &app_id.to_string());
    value = value.replace("UserName=RUNE", "UserName=0xoLemon");
    value = value.replace("Language=english", "Language=english");
    value = value.replace(
        "Offline=0",
        if matches!(settings.network_mode, LuaNetworkMode::Offline) {
            "Offline=1"
        } else {
            "Offline=0"
        },
    );
    value = value.replace("RUNE_Interfaces", "");
    value = value.replace("RUNE_DLC", "");
    value
}

fn sole_executable_dir(scan: &LuaRuntimeTargetScan) -> Result<PathBuf, String> {
    if scan.executables.len() != 1 {
        return Err(format!(
            "This runtime mode requires an unambiguous main executable, but the scan found {} executables",
            scan.executables.len()
        ));
    }
    let relative = validate_relative_path(&scan.executables[0].relative_path)?;
    Ok(relative.parent().unwrap_or_else(|| Path::new("")).to_path_buf())
}

fn plan_internal(app: &AppHandle, app_id: u32) -> Result<GseUcPlan, String> {
    let settings_state = lua_runtime_profiles::get_lua_runtime_settings(app.clone())?;
    let settings = settings_state.settings;
    let scan = lua_runtime_profiles::scan_lua_runtime_target_sync(app, app_id)?;
    let install_root = PathBuf::from(&scan.install_root);
    if !install_root.is_absolute() {
        return Err("Steam install root is not absolute".to_string());
    }

    let root = resource_root(app)?;
    let (manifest, _) = read_manifest(&root)?;
    let component_id = package_id(settings.default_package);
    let selected_health = health_for_component(app, component_id, true)?;
    let mut component_health = vec![selected_health.clone()];
    let mut blocked = scan.blocked_reasons.clone();
    let mut warnings = scan.warnings.clone();
    if !scan.locally_approved {
        blocked.push("RUNTIME_TARGET_NOT_APPROVED: review and approve the current fingerprint before applying runtime files".to_string());
    }
    if !selected_health.integrity_verified {
        blocked.push(format!(
            "RUNTIME_INTEGRITY_FAILED: component {} failed bundled file verification",
            component_id
        ));
    }

    let mut actions = Vec::new();
    let package = settings.default_package;
    let runtime_supported = matches!(
        package,
        LuaRuntimePackage::GseRegular
            | LuaRuntimePackage::GseExperimental
            | LuaRuntimePackage::UcOnline2
            | LuaRuntimePackage::RuneRegular
    );

    if runtime_supported {
        for target in &scan.runtime_targets {
            let Some(architecture) = target.architecture else {
                blocked.push(format!(
                    "Target architecture is unavailable for {}",
                    target.relative_path
                ));
                continue;
            };
            let Some(source_relative) = runtime_source_for(package, architecture) else {
                blocked.push(format!("No runtime source exists for {:?}", architecture));
                continue;
            };
            let expected = manifest_file(&manifest, source_relative)?;
            add_action(
                &mut actions,
                component_id,
                Some(architecture),
                &install_root,
                target.relative_path.clone(),
                source_relative.to_string(),
                expected.sha256.clone(),
                false,
            )?;
        }
    }

    match package {
        LuaRuntimePackage::GseRegular | LuaRuntimePackage::GseExperimental => {
            let mut parents = BTreeSet::new();
            for target in &scan.runtime_targets {
                parents.insert(target_parent_relative(&target.relative_path)?);
            }
            for parent in parents {
                for (relative, bytes) in gse_generated_files(&settings, app_id, &parent) {
                    let relative_string = relative.to_string_lossy().replace('\\', "/");
                    add_action(
                        &mut actions,
                        component_id,
                        None,
                        &install_root,
                        relative_string.clone(),
                        generated_marker("gse", &relative_string),
                        generated_sha256(&bytes),
                        true,
                    )?;
                }
            }
            if matches!(package, LuaRuntimePackage::GseExperimental)
                && matches!(settings.renderer, LuaOverlayRenderer::GseNative)
            {
                let cold = health_for_component(app, "gseColdClient", true)?;
                component_health.push(cold.clone());
                if !cold.integrity_verified {
                    blocked.push("Native GSE overlay assets failed integrity verification".to_string());
                } else {
                    warnings.push("GSE native overlay is enabled; launcher desktop Shift+F1 remains a separate diagnostic fallback".to_string());
                }
            }
        }
        LuaRuntimePackage::UcOnline2 => {
            let exe_dir = match sole_executable_dir(&scan) {
                Ok(value) => value,
                Err(error) => {
                    blocked.push(error);
                    PathBuf::new()
                }
            };
            if blocked.iter().all(|row| !row.contains("unambiguous main executable")) {
                let target = exe_dir.join("union-crax.ini");
                let target_string = target.to_string_lossy().replace('\\', "/");
                let bytes = uc_ini(&settings, app_id);
                add_action(
                    &mut actions,
                    component_id,
                    None,
                    &install_root,
                    target_string.clone(),
                    generated_marker("uc", &target_string),
                    generated_sha256(&bytes),
                    true,
                )?;
            }
        }
        LuaRuntimePackage::RuneRegular => {
            let template_relative = "embedded/rune/emu/steam_emu.ini";
            let template = fs::read_to_string(root.join(template_relative)).map_err(|error| {
                format!("Could not read RUNE template {template_relative}: {error}")
            })?;
            let rendered = render_rune_ini(&template, app_id, &settings).into_bytes();
            let mut parents = BTreeSet::new();
            for target in &scan.runtime_targets {
                parents.insert(target_parent_relative(&target.relative_path)?);
            }
            for parent in parents {
                let target = parent.join("steam_emu.ini");
                let target_string = target.to_string_lossy().replace('\\', "/");
                add_action(
                    &mut actions,
                    component_id,
                    None,
                    &install_root,
                    target_string.clone(),
                    generated_marker("rune", &target_string),
                    generated_sha256(&rendered),
                    true,
                )?;
            }
            for target in [PathBuf::from("steam_appid.txt")] {
                let target_string = target.to_string_lossy().replace('\\', "/");
                let bytes = format!("{app_id}\n").into_bytes();
                add_action(
                    &mut actions,
                    component_id,
                    None,
                    &install_root,
                    target_string.clone(),
                    generated_marker("rune-appid", &target_string),
                    generated_sha256(&bytes),
                    true,
                )?;
            }
        }
        LuaRuntimePackage::GseColdClient | LuaRuntimePackage::GseColdClientV1 => {
            if let Err(error) = sole_executable_dir(&scan) {
                blocked.push(error);
            }
            blocked.push("ColdClient planning is available through resource health, but direct mutation stays fail-closed until an exact loader/executable mapping is stored for this game".to_string());
        }
        LuaRuntimePackage::RuneSteakClient | LuaRuntimePackage::RuneSteamClient => {
            if let Err(error) = sole_executable_dir(&scan) {
                blocked.push(error);
            }
            blocked.push("RUNE client-loader mode requires a per-game proxy/loader mapping; the launcher will not guess or silently patch imports".to_string());
        }
    }

    if !matches!(settings.steam_stub_mode, LuaSteamStubMode::Disabled) {
        if scan.executables.len() != 1 {
            blocked.push("SteamStub handling requires exactly one reviewed executable".to_string());
        } else {
            match settings.steam_stub_mode {
                LuaSteamStubMode::AutoSteamless | LuaSteamStubMode::Steamless => {
                    let relative = validate_relative_path(&scan.executables[0].relative_path)?;
                    let source_exe = install_root.join(&relative);
                    let scratch = app_local_root(app)?.join(STAGE_DIR).join("dry-run");
                    match steamless_generated_bytes(&source_exe, &scratch, "plan") {
                        Ok(bytes) => {
                            let target_string = relative.to_string_lossy().replace('\\', "/");
                            add_action(
                                &mut actions,
                                "steamless",
                                scan.executables[0].architecture,
                                &install_root,
                                target_string.clone(),
                                generated_marker("steamless", &target_string),
                                generated_sha256(&bytes),
                                true,
                            )?;
                            warnings.push("Steamless output was generated against a scratch copy during dry-run; the live executable will only change inside the managed transaction".to_string());
                        }
                        Err(error) if matches!(settings.steam_stub_mode, LuaSteamStubMode::AutoSteamless) => {
                            warnings.push(format!("Auto Steamless skipped this executable: {error}"));
                        }
                        Err(error) => blocked.push(error),
                    }
                }
                LuaSteamStubMode::RuneProxy => {
                    let health = health_for_component(app, "runeSteamStub", true)?;
                    component_health.push(health.clone());
                    if !health.integrity_verified {
                        blocked.push("RUNE SteamStub component failed integrity verification".to_string());
                    }
                }
                LuaSteamStubMode::UcRuntime => {
                    if !matches!(package, LuaRuntimePackage::UcOnline2) {
                        blocked.push("UC runtime SteamStub requires UC Online2 package".to_string());
                    }
                }
                LuaSteamStubMode::Disabled => {}
            }
        }
    }

    if actions.is_empty() && runtime_supported {
        blocked.push("Runtime plan contains no file actions".to_string());
    }
    blocked.sort();
    blocked.dedup();
    warnings.sort();
    warnings.dedup();

    Ok(GseUcPlan {
        schema_version: 1,
        app_id,
        package,
        install_root: install_root.display().to_string(),
        approval_fingerprint: scan.approval_fingerprint,
        locally_approved: scan.locally_approved,
        can_apply: blocked.is_empty(),
        blocked_reasons: blocked,
        warnings,
        actions,
        component_health,
        steam_stub_mode: settings.steam_stub_mode,
    })
}

#[tauri::command]
pub async fn plan_gse_uc_setup(app: AppHandle, app_id: u32) -> Result<GseUcPlan, String> {
    tauri::async_runtime::spawn_blocking(move || plan_internal(&app, app_id))
        .await
        .map_err(|error| format!("GSE/UC plan worker failed: {error}"))?
}

fn app_local_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve launcher local-data directory: {error}"))
}

fn receipt_path(app: &AppHandle, app_id: u32) -> Result<PathBuf, String> {
    Ok(app_local_root(app)?.join(RECEIPT_DIR).join(format!("{app_id}.json")))
}

fn backup_root(app: &AppHandle, app_id: u32) -> Result<PathBuf, String> {
    Ok(app_local_root(app)?.join(BACKUP_DIR).join(app_id.to_string()))
}

fn load_receipt(app: &AppHandle, app_id: u32) -> Result<Option<GseUcReceipt>, String> {
    let path = receipt_path(app, app_id)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("Could not read GSE/UC receipt {}: {error}", path.display()))?;
    let receipt: GseUcReceipt = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid GSE/UC receipt {}: {error}", path.display()))?;
    if receipt.schema_version != RECEIPT_SCHEMA || receipt.app_id != app_id {
        return Err("Stored GSE/UC receipt has an incompatible schema or AppID".to_string());
    }
    Ok(Some(receipt))
}

fn write_receipt(app: &AppHandle, receipt: &GseUcReceipt) -> Result<(), String> {
    let path = receipt_path(app, receipt.app_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create receipt directory: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("Could not serialize GSE/UC receipt: {error}"))?;
    crate::lua_live::atomic_write_path(&path, &bytes)
}

fn backup_relative_for(target_relative: &str) -> Result<PathBuf, String> {
    let rel = validate_relative_path(target_relative)?;
    Ok(PathBuf::from("files").join(rel))
}

fn copy_backup(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create backup directory {}: {error}", parent.display()))?;
    }
    fs::copy(source, target).map_err(|error| {
        format!(
            "Could not preserve original {} -> {}: {error}",
            source.display(),
            target.display()
        )
    })?;
    let file = File::open(target)
        .map_err(|error| format!("Could not reopen backup {}: {error}", target.display()))?;
    file.sync_all()
        .map_err(|error| format!("Could not flush backup {}: {error}", target.display()))?;
    Ok(())
}

fn generated_bytes(
    action: &GseUcFileAction,
    settings: &LuaRuntimeSettings,
    app_id: u32,
    resource_root: &Path,
) -> Result<Vec<u8>, String> {
    let marker = action.source_relative_path.as_str();
    if marker.starts_with("@generated/gse/") {
        let target = PathBuf::from(&action.target_relative_path);
        let parent = target
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new(""));
        for (relative, bytes) in gse_generated_files(settings, app_id, parent) {
            if relative.to_string_lossy().replace('\\', "/") == action.target_relative_path {
                return Ok(bytes);
            }
        }
    } else if marker.starts_with("@generated/uc/") {
        return Ok(uc_ini(settings, app_id));
    } else if marker.starts_with("@generated/rune-appid/") {
        return Ok(format!("{app_id}\n").into_bytes());
    } else if marker.starts_with("@generated/rune/") {
        let template = fs::read_to_string(resource_root.join("embedded/rune/emu/steam_emu.ini"))
            .map_err(|error| format!("Could not read RUNE template: {error}"))?;
        return Ok(render_rune_ini(&template, app_id, settings).into_bytes());
    }
    Err(format!(
        "Unknown generated file marker {} for {}",
        marker, action.target_relative_path
    ))
}

fn existing_receipt_original(
    receipt: Option<&GseUcReceipt>,
    target_relative: &str,
    current_hash: Option<&str>,
) -> Result<Option<GseUcOwnedFileReceipt>, String> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    let Some(owned) = receipt
        .owned_files
        .iter()
        .find(|row| row.target_relative_path == target_relative)
    else {
        return Ok(None);
    };
    if receipt.active {
        let matches_managed = current_hash == Some(owned.managed_sha256.as_str());
        let matches_original = owned.original_sha256.as_deref() == current_hash;
        let safely_missing = current_hash.is_none();
        if !matches_managed && !matches_original && !safely_missing {
            return Err(format!(
                "RUNTIME_EXTERNAL_MODIFICATION: managed file changed outside the launcher: {}",
                target_relative
            ));
        }
    }
    Ok(Some(owned.clone()))
}

fn execute_plan(
    app: &AppHandle,
    plan: &GseUcPlan,
    expected_fingerprint: &str,
) -> Result<GseUcReceiptState, String> {
    if !plan.can_apply {
        return Err(format!("Runtime plan is blocked: {}", plan.blocked_reasons.join("; ")));
    }
    if plan.approval_fingerprint != expected_fingerprint.trim() {
        return Err("RUNTIME_TARGET_CHANGED: scan fingerprint changed before apply".to_string());
    }
    let fresh = lua_runtime_profiles::scan_lua_runtime_target_sync(app, plan.app_id)?;
    if !fresh.locally_approved || fresh.approval_fingerprint != plan.approval_fingerprint {
        return Err("RUNTIME_TARGET_NOT_APPROVED: target changed or approval is missing".to_string());
    }

    let settings = lua_runtime_profiles::get_lua_runtime_settings(app.clone())?.settings;
    if settings.default_package != plan.package {
        return Err("Runtime settings changed after dry-run; create a new plan".to_string());
    }
    let root = resource_root(app)?;
    let selected = health_for_component(app, package_id(plan.package), true)?;
    if !selected.integrity_verified {
        return Err("RUNTIME_INTEGRITY_FAILED: selected runtime component changed after dry-run".to_string());
    }

    let install_root = PathBuf::from(&plan.install_root);
    let existing = load_receipt(app, plan.app_id)?;
    if let Some(receipt) = existing.as_ref() {
        if receipt.install_root != plan.install_root {
            return Err("Stored runtime receipt belongs to a different install root".to_string());
        }
    }
    let backup_root = backup_root(app, plan.app_id)?;
    fs::create_dir_all(&backup_root)
        .map_err(|error| format!("Could not create runtime backup root: {error}"))?;
    let stage_root = app_local_root(app)?.join(STAGE_DIR).join(Uuid::new_v4().to_string());
    fs::create_dir_all(&stage_root)
        .map_err(|error| format!("Could not create runtime staging root: {error}"))?;

    let mut owned_files = Vec::new();
    let mut changes = Vec::new();
    for (index, action) in plan.actions.iter().enumerate() {
        let target_relative = validate_relative_path(&action.target_relative_path)?;
        let target = install_root.join(&target_relative);
        let current_hash = file_hash_if_exists(&target)?;
        if current_hash != action.before_sha256 {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(format!(
                "RUNTIME_TARGET_CHANGED: {} changed after dry-run",
                action.target_relative_path
            ));
        }

        let previous_owned = existing_receipt_original(
            existing.as_ref(),
            &action.target_relative_path,
            current_hash.as_deref(),
        )?;
        let (original_sha256, original_backup_relative_path) = if let Some(previous) = previous_owned {
            (
                previous.original_sha256,
                previous.original_backup_relative_path,
            )
        } else if target.is_file() {
            let original_hash = current_hash.clone().ok_or_else(|| {
                format!("Could not hash original {}", action.target_relative_path)
            })?;
            let relative_backup = backup_relative_for(&action.target_relative_path)?;
            let backup = backup_root.join(&relative_backup);
            if backup.is_file() {
                let existing_hash = sha256_file(&backup)?;
                if existing_hash != original_hash {
                    let _ = fs::remove_dir_all(&stage_root);
                    return Err(format!(
                        "Original backup collision for {}",
                        action.target_relative_path
                    ));
                }
            } else {
                copy_backup(&target, &backup)?;
            }
            (
                Some(original_hash),
                Some(relative_backup.to_string_lossy().replace('\\', "/")),
            )
        } else {
            (None, None)
        };

        let (source, allowed_source_root) = if action.generated {
            let bytes = if action.source_relative_path.starts_with("@generated/steamless/") {
                steamless_generated_bytes(&target, &stage_root, &format!("apply-{index}"))?
            } else {
                generated_bytes(action, &settings, plan.app_id, &root)?
            };
            let actual = generated_sha256(&bytes);
            if actual != action.after_sha256 {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(format!(
                    "Generated runtime file hash changed for {}",
                    action.target_relative_path
                ));
            }
            let staged = stage_root.join(format!("{index}.bin"));
            let mut file = File::create(&staged)
                .map_err(|error| format!("Could not create staged file {}: {error}", staged.display()))?;
            file.write_all(&bytes)
                .map_err(|error| format!("Could not write staged file {}: {error}", staged.display()))?;
            file.sync_all()
                .map_err(|error| format!("Could not sync staged file {}: {error}", staged.display()))?;
            (staged, stage_root.clone())
        } else {
            let source_relative = validate_relative_path(&action.source_relative_path)?;
            let source = root.join(&source_relative);
            let actual = sha256_file(&source)?;
            if actual != action.after_sha256 {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(format!(
                    "RUNTIME_INTEGRITY_FAILED: {} changed after dry-run",
                    action.source_relative_path
                ));
            }
            (source, root.clone())
        };

        changes.push(ManagedFileChange::Replace(ManagedFileSpec {
            source,
            target,
            allowed_source_root,
            allowed_target_root: install_root.clone(),
            expected_sha256: action.after_sha256.clone(),
        }));
        owned_files.push(GseUcOwnedFileReceipt {
            target_relative_path: action.target_relative_path.clone(),
            managed_sha256: action.after_sha256.clone(),
            original_sha256,
            original_backup_relative_path,
        });
    }

    let transaction = managed_file_transaction::apply_changes(
        app,
        &format!("gse-uc-apply-{}", plan.app_id),
        changes,
    )?;
    let _ = fs::remove_dir_all(&stage_root);


    let receipt = GseUcReceipt {
        schema_version: RECEIPT_SCHEMA,
        app_id: plan.app_id,
        package: plan.package,
        transaction_id: transaction.transaction_id.clone(),
        approval_fingerprint: plan.approval_fingerprint.clone(),
        install_root: plan.install_root.clone(),
        active: true,
        owned_files: owned_files.clone(),
    };
    write_receipt(app, &receipt)?;
    Ok(GseUcReceiptState {
        schema_version: RECEIPT_SCHEMA,
        app_id: plan.app_id,
        package: Some(plan.package),
        status: "installed".to_string(),
        transaction_id: Some(transaction.transaction_id),
        approval_fingerprint: Some(plan.approval_fingerprint.clone()),
        owned_files,
        message: "GSE / UC runtime applied transactionally and receipt stored".to_string(),
    })
}

#[tauri::command]
pub fn apply_gse_uc_setup(
    app: AppHandle,
    app_id: u32,
    expected_fingerprint: String,
) -> Result<GseUcReceiptState, String> {
    let plan = plan_internal(&app, app_id)?;
    execute_plan(&app, &plan, &expected_fingerprint)
}

fn verify_receipt(app: &AppHandle, app_id: u32) -> Result<GseUcReceiptState, String> {
    let Some(receipt) = load_receipt(app, app_id)? else {
        return Ok(GseUcReceiptState {
            schema_version: RECEIPT_SCHEMA,
            app_id,
            package: None,
            status: "notInstalled".to_string(),
            transaction_id: None,
            approval_fingerprint: None,
            owned_files: Vec::new(),
            message: "No GSE / UC runtime receipt exists for this game".to_string(),
        });
    };
    let install_root = PathBuf::from(&receipt.install_root);
    let mut drift = Vec::new();
    if receipt.active {
        for owned in &receipt.owned_files {
            let target = install_root.join(validate_relative_path(&owned.target_relative_path)?);
            let current = file_hash_if_exists(&target)?;
            if current.as_deref() != Some(owned.managed_sha256.as_str()) {
                drift.push(owned.target_relative_path.clone());
            }
        }
    }
    let status = if !receipt.active {
        "restored"
    } else if drift.is_empty() {
        "installed"
    } else {
        "repairRequired"
    };
    Ok(GseUcReceiptState {
        schema_version: RECEIPT_SCHEMA,
        app_id,
        package: Some(receipt.package),
        status: status.to_string(),
        transaction_id: Some(receipt.transaction_id),
        approval_fingerprint: Some(receipt.approval_fingerprint),
        owned_files: receipt.owned_files,
        message: if drift.is_empty() {
            if receipt.active {
                "All launcher-owned runtime files match the receipt".to_string()
            } else {
                "Runtime was restored to its pre-apply state".to_string()
            }
        } else {
            format!("Runtime drift detected: {}", drift.join(", "))
        },
    })
}

#[tauri::command]
pub fn verify_gse_uc_setup(app: AppHandle, app_id: u32) -> Result<GseUcReceiptState, String> {
    verify_receipt(&app, app_id)
}

#[tauri::command]
pub fn repair_gse_uc_setup(
    app: AppHandle,
    app_id: u32,
    expected_fingerprint: String,
) -> Result<GseUcReceiptState, String> {
    let state = verify_receipt(&app, app_id)?;
    if state.status == "installed" {
        return Ok(state);
    }
    if state.status == "restored" || state.status == "notInstalled" {
        return apply_gse_uc_setup(app, app_id, expected_fingerprint);
    }
    // Re-plan from the current fingerprint. execute_plan accepts a missing managed file or
    // the exact backed-up original, but rejects every unknown external hash.
    apply_gse_uc_setup(app, app_id, expected_fingerprint)
}

#[tauri::command]
pub fn restore_gse_uc_setup(app: AppHandle, app_id: u32) -> Result<GseUcReceiptState, String> {
    let Some(mut receipt) = load_receipt(&app, app_id)? else {
        return verify_receipt(&app, app_id);
    };
    if !receipt.active {
        return verify_receipt(&app, app_id);
    }
    let install_root = PathBuf::from(&receipt.install_root);
    let backup_root = backup_root(&app, app_id)?;
    let mut changes = Vec::new();
    for owned in &receipt.owned_files {
        let target_relative = validate_relative_path(&owned.target_relative_path)?;
        let target = install_root.join(&target_relative);
        let current = file_hash_if_exists(&target)?;
        if current.as_deref() != Some(owned.managed_sha256.as_str()) {
            return Err(format!(
                "Restore blocked because {} no longer matches the managed receipt",
                owned.target_relative_path
            ));
        }
        if let Some(relative_backup) = owned.original_backup_relative_path.as_deref() {
            let backup_relative = validate_relative_path(relative_backup)?;
            let source = backup_root.join(&backup_relative);
            let expected = owned.original_sha256.clone().ok_or_else(|| {
                format!("Receipt lacks original hash for {}", owned.target_relative_path)
            })?;
            if sha256_file(&source)? != expected {
                return Err(format!(
                    "Original backup failed hash verification for {}",
                    owned.target_relative_path
                ));
            }
            changes.push(ManagedFileChange::Replace(ManagedFileSpec {
                source,
                target,
                allowed_source_root: backup_root.clone(),
                allowed_target_root: install_root.clone(),
                expected_sha256: expected,
            }));
        } else {
            changes.push(ManagedFileChange::Delete(ManagedDeleteSpec {
                target,
                allowed_target_root: install_root.clone(),
            }));
        }
    }
    if changes.is_empty() {
        return Err("Runtime receipt contains no owned files".to_string());
    }
    let transaction = managed_file_transaction::apply_changes(
        &app,
        &format!("gse-uc-restore-{app_id}"),
        changes,
    )?;
    receipt.active = false;
    receipt.transaction_id = transaction.transaction_id.clone();
    write_receipt(&app, &receipt)?;
    verify_receipt(&app, app_id)
}

#[tauri::command]
pub fn launch_migrate_gse(app: AppHandle) -> Result<u32, String> {
    let health = health_for_component(&app, "migrateGse", true)?;
    if !health.integrity_verified {
        return Err("RUNTIME_INTEGRITY_FAILED: migrate_gse is not verified".to_string());
    }
    let root = resource_root(&app)?;
    let exe = root.join("embedded/migrate_gse/migrate_gse.exe");
    let cwd = exe
        .parent()
        .ok_or_else(|| "migrate_gse resource path has no parent".to_string())?;
    let child = Command::new(&exe)
        .current_dir(cwd)
        .spawn()
        .map_err(|error| format!("Could not launch {}: {error}", exe.display()))?;
    Ok(child.id())
}
