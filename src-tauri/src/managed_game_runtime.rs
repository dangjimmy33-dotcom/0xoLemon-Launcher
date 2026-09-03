use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::managed_file_transaction::{
    self, ManagedDeleteSpec, ManagedFileChange, ManagedFileSpec,
};
use crate::manifest::VersionManifest;
use crate::save_paths::SaveProviderKind;

const RUNTIME_CATALOG_BYTES: &[u8] = include_bytes!("../resources/managed-runtime/catalog.json");
const RUNTIME_CATALOG_SHA256: &str =
    "4b3a6f8a4cc4371cae5b3079676a39f587869fa449262b49e802051180b97b80";
const ARTIFACT_MANIFEST_BYTES: &[u8] = include_bytes!("../resources/emu/manifest.json");
const ADAPTER_PATCH_PROVENANCE_BYTES: &[u8] =
    include_bytes!("../resources/emu/OXO_PATCH_PROVENANCE.json");
const ARTIFACT_MANIFEST_SHA256: &str =
    "486bbdb4c53f6f8e96a5b3c1682de7bcd81a189ffd68e2c31f40e3e576de0487";
const RUNTIME_CATALOG_SCHEMA: u32 = 1;
const ARTIFACT_MANIFEST_SCHEMA: u32 = 2;
const ADAPTER_PATCH_PROVENANCE_SCHEMA: u32 = 1;
const ACHIEVEMENT_PIPE_PROTOCOL_VERSION: u32 = 1;
const LEGACY_MANAGED_STATE_SCHEMA: u32 = 1;
const MANAGED_STATE_SCHEMA: u32 = 2;
const STATE_DIRECTORY: &str = ".0xolemon";
const STATE_FILE: &str = "managed-gse-state.json";
const ORIGINAL_DIRECTORY: &str = "managed-runtime/original";
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const CANONICAL_GSE_UPSTREAM: &str = "https://github.com/Detanup01/gbe_fork";
const CANONICAL_GSE_COMMIT: &str = "e4035e085a028a195c2e3f13e3207e0dfdadce57";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SteamRuntimeMode {
    #[default]
    None,
    ManagedGse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveProvider {
    pub provider: SaveProviderKind,
    pub save_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalRuntimeIntegration {
    #[serde(default)]
    pub steam_runtime: SteamRuntimeMode,
    #[serde(default)]
    pub achievements_enabled: bool,
    #[serde(default)]
    pub save_providers: Vec<SaveProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCatalog {
    schema_version: u32,
    revision: String,
    games: Vec<RuntimeCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCatalogEntry {
    game_id: String,
    #[serde(default)]
    app_id: Option<u32>,
    #[serde(flatten)]
    integration: LocalRuntimeIntegration,
    #[serde(default)]
    allowed_original_sha256: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactManifest {
    schema_version: u32,
    runtime_version: String,
    source: ArtifactSource,
    artifacts: Vec<RuntimeArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactSource {
    upstream: String,
    source_manifest_sha256: String,
    adapter_patch_sha256: String,
    license: String,
    license_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdapterPatchProvenance {
    schema_version: u32,
    patch_version: String,
    base_source_manifest_sha256: String,
    protocol_version: u32,
    files: Vec<AdapterSourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdapterSourceFile {
    relative_path: String,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeArtifact {
    architecture: PeArchitecture,
    relative_path: String,
    file_name: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PeArchitecture {
    X86,
    X64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ManagedGseStatus {
    NotManaged,
    Missing,
    Installed,
    Restored,
    RepairRequired,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedGseReceiptV1 {
    schema_version: u32,
    game_id: String,
    #[serde(default)]
    app_id: Option<u32>,
    catalog_revision: String,
    runtime_version: String,
    architecture: PeArchitecture,
    dll_relative_path: String,
    managed_sha256: String,
    #[serde(default)]
    original_sha256: Option<String>,
    #[serde(default)]
    original_backup_relative_path: Option<String>,
    installed_at: String,
    #[serde(default)]
    restored_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeComponentV2 {
    SteamApi,
    SteamSettings,
    SteamInterfaces,
    RuntimeConfig,
    AchievementSchema,
    StatsSchema,
    ItemsSchema,
    OverlayAsset,
    OverlaySound,
    OverlayFont,
    ReshadeBridge,
    ReshadeAddon,
    ColdClient,
    RuneRuntime,
    UcRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum OriginalFileStateV2 {
    Absent,
    Present {
        sha256: String,
        backup_relative_path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOwnedFileReceiptV2 {
    pub component: RuntimeComponentV2,
    pub architecture: PeArchitecture,
    pub target_relative_path: String,
    pub artifact_id: String,
    pub managed_sha256: String,
    pub original: OriginalFileStateV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeReceiptV2 {
    pub schema_version: u32,
    pub transaction_id: String,
    pub profile_id: String,
    pub game_id: String,
    #[serde(default)]
    pub app_id: Option<u32>,
    pub catalog_revision: String,
    pub runtime_version: String,
    pub installed_at: String,
    #[serde(default)]
    pub restored_at: Option<String>,
    pub files: Vec<ManagedOwnedFileReceiptV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeFileStateV2 {
    pub component: RuntimeComponentV2,
    pub architecture: PeArchitecture,
    pub target_path: String,
    pub managed_sha256: String,
    pub original_sha256: Option<String>,
    pub original_backup_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OverlayRendererV2 {
    GseNative,
    ReshadeCompatibility,
    DesktopFallback,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AntiCheatPolicyV2 {
    BlockProtectedRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeTargetSpecV2 {
    pub relative_path: String,
    pub architecture: PeArchitecture,
    pub component: RuntimeComponentV2,
    pub allowed_original_sha256: Vec<String>,
    pub managed_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeProfileV2 {
    pub schema_version: u32,
    pub profile_id: String,
    pub game_id: String,
    pub app_id: u32,
    pub canonical_upstream: String,
    pub immutable_commit: String,
    pub build_id: String,
    pub patch_set_hash: String,
    pub license: String,
    pub provenance_verified: bool,
    pub executable_allowlist: Vec<String>,
    pub runtime_process_allowlist: Vec<String>,
    pub anti_cheat_policy: AntiCheatPolicyV2,
    pub targets: Vec<ManagedRuntimeTargetSpecV2>,
    pub generated_settings: Vec<String>,
    pub generated_interfaces: Vec<String>,
    pub generated_assets: Vec<String>,
    pub renderer: OverlayRendererV2,
    pub hotkey: String,
    pub achievement_protocol_version: u32,
    pub save_provider: Option<SaveProviderKind>,
    pub save_root: Option<String>,
    pub cloud_policy: String,
    pub required_components: Vec<String>,
    pub on_demand_components: Vec<String>,
    pub restore_constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeSemanticChangeV2 {
    pub target_relative_path: String,
    pub action: String,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimePlanV2 {
    pub schema_version: u32,
    pub profile: ManagedRuntimeProfileV2,
    pub state: ManagedGseState,
    pub can_apply: bool,
    pub blocked_reason: Option<String>,
    pub changes: Vec<ManagedRuntimeSemanticChangeV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedGseState {
    pub schema_version: u32,
    pub game_id: String,
    pub app_id: Option<u32>,
    pub steam_runtime: SteamRuntimeMode,
    pub achievements_enabled: bool,
    pub status: ManagedGseStatus,
    pub catalog_revision: String,
    pub runtime_version: Option<String>,
    pub architecture: Option<PeArchitecture>,
    pub dll_path: Option<String>,
    pub managed_sha256: Option<String>,
    pub original_sha256: Option<String>,
    pub original_backup_path: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub transaction_id: Option<String>,
    #[serde(default)]
    pub owned_files: Vec<ManagedRuntimeFileStateV2>,
    pub message: String,
}

#[derive(Debug)]
struct RuntimeTarget {
    install_root: PathBuf,
    launch_executable_path: PathBuf,
    launch_executable_relative_path: String,
    architecture: PeArchitecture,
    dll_path: PathBuf,
    dll_relative_path: String,
    state_path: PathBuf,
    backup_path: PathBuf,
    backup_relative_path: String,
}

static CATALOG: OnceLock<Result<RuntimeCatalog, String>> = OnceLock::new();
static ARTIFACT_MANIFEST: OnceLock<Result<ArtifactManifest, String>> = OnceLock::new();

fn runtime_catalog() -> Result<&'static RuntimeCatalog, String> {
    CATALOG
        .get_or_init(|| {
            verify_embedded_hash(
                "managed runtime catalog",
                RUNTIME_CATALOG_BYTES,
                RUNTIME_CATALOG_SHA256,
            )?;
            let catalog: RuntimeCatalog = serde_json::from_slice(RUNTIME_CATALOG_BYTES)
                .map_err(|error| format!("invalid managed runtime catalog: {error}"))?;
            if catalog.schema_version != RUNTIME_CATALOG_SCHEMA {
                return Err(format!(
                    "unsupported managed runtime catalog schema {}",
                    catalog.schema_version
                ));
            }
            validate_runtime_catalog(catalog)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn artifact_manifest() -> Result<&'static ArtifactManifest, String> {
    ARTIFACT_MANIFEST
        .get_or_init(|| {
            verify_embedded_hash(
                "GSE artifact manifest",
                ARTIFACT_MANIFEST_BYTES,
                ARTIFACT_MANIFEST_SHA256,
            )?;
            let manifest: ArtifactManifest = serde_json::from_slice(ARTIFACT_MANIFEST_BYTES)
                .map_err(|error| format!("invalid GSE artifact manifest: {error}"))?;
            if manifest.schema_version != ARTIFACT_MANIFEST_SCHEMA {
                return Err(format!(
                    "unsupported GSE artifact manifest schema {}",
                    manifest.schema_version
                ));
            }
            let manifest = validate_artifact_manifest(manifest)?;
            verify_embedded_hash(
                "GSE adapter patch provenance",
                ADAPTER_PATCH_PROVENANCE_BYTES,
                &manifest.source.adapter_patch_sha256,
            )?;
            let provenance: AdapterPatchProvenance =
                serde_json::from_slice(ADAPTER_PATCH_PROVENANCE_BYTES)
                    .map_err(|error| format!("invalid GSE adapter patch provenance: {error}"))?;
            validate_adapter_patch_provenance(provenance, &manifest)?;
            Ok(manifest)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn verify_embedded_hash(label: &str, bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        return Err(format!(
            "{label} integrity check failed: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

fn validate_runtime_catalog(mut catalog: RuntimeCatalog) -> Result<RuntimeCatalog, String> {
    if catalog.revision.trim().is_empty() {
        return Err("managed runtime catalog revision is empty".to_string());
    }
    let mut game_ids = HashSet::new();
    for game in &mut catalog.games {
        game.game_id = normalize_game_id(&game.game_id)?;
        if !game_ids.insert(game.game_id.clone()) {
            return Err(format!(
                "managed runtime catalog contains duplicate game {}",
                game.game_id
            ));
        }
        if game.integration.steam_runtime == SteamRuntimeMode::ManagedGse && game.app_id.is_none() {
            return Err(format!(
                "managed GSE game {} must declare an appId",
                game.game_id
            ));
        }
        if game.integration.achievements_enabled
            && game.integration.steam_runtime != SteamRuntimeMode::ManagedGse
        {
            return Err(format!(
                "achievements require managedGse for {}",
                game.game_id
            ));
        }
        let mut providers = HashSet::new();
        for provider in &mut game.integration.save_providers {
            provider.save_id = provider.save_id.trim().to_string();
            if provider.save_id.is_empty()
                || !provider
                    .save_id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            {
                return Err(format!("invalid save provider id for {}", game.game_id));
            }
            if !providers.insert((provider.provider.clone(), provider.save_id.clone())) {
                return Err(format!("duplicate save provider for {}", game.game_id));
            }
        }
        for hash in &mut game.allowed_original_sha256 {
            *hash = normalize_sha256(hash)?;
        }
    }
    Ok(catalog)
}

fn validate_artifact_manifest(mut manifest: ArtifactManifest) -> Result<ArtifactManifest, String> {
    if manifest.runtime_version.trim().is_empty() {
        return Err("GSE runtime version is empty".to_string());
    }
    if manifest.source.license != "LGPL-3.0" {
        return Err("GSE artifact manifest must retain the LGPL-3.0 license".to_string());
    }
    normalize_sha256(&manifest.source.source_manifest_sha256)?;
    manifest.source.adapter_patch_sha256 = normalize_sha256(&manifest.source.adapter_patch_sha256)?;
    normalize_sha256(&manifest.source.license_sha256)?;
    let mut architectures = HashSet::new();
    for artifact in &mut manifest.artifacts {
        if !architectures.insert(artifact.architecture) {
            return Err("GSE artifact manifest contains a duplicate architecture".to_string());
        }
        validate_relative_path(&artifact.relative_path)?;
        let expected_name = match artifact.architecture {
            PeArchitecture::X86 => "steam_api.dll",
            PeArchitecture::X64 => "steam_api64.dll",
        };
        if artifact.file_name != expected_name
            || Path::new(&artifact.relative_path)
                .file_name()
                .and_then(|value| value.to_str())
                != Some(expected_name)
        {
            return Err(format!(
                "invalid GSE artifact filename for {:?}",
                artifact.architecture
            ));
        }
        artifact.sha256 = normalize_sha256(&artifact.sha256)?;
        if artifact.size_bytes == 0 {
            return Err("GSE artifact size must be non-zero".to_string());
        }
    }
    if architectures.len() != 2 {
        return Err("GSE artifact manifest must contain x86 and x64 builds".to_string());
    }
    Ok(manifest)
}

fn validate_adapter_patch_provenance(
    mut provenance: AdapterPatchProvenance,
    manifest: &ArtifactManifest,
) -> Result<AdapterPatchProvenance, String> {
    if provenance.schema_version != ADAPTER_PATCH_PROVENANCE_SCHEMA {
        return Err(format!(
            "unsupported GSE adapter patch provenance schema {}",
            provenance.schema_version
        ));
    }
    if provenance.patch_version.trim().is_empty() {
        return Err("GSE adapter patch version is empty".to_string());
    }
    if provenance.protocol_version != ACHIEVEMENT_PIPE_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported GSE achievement protocol {}",
            provenance.protocol_version
        ));
    }
    provenance.base_source_manifest_sha256 =
        normalize_sha256(&provenance.base_source_manifest_sha256)?;
    if provenance.base_source_manifest_sha256 != manifest.source.source_manifest_sha256 {
        return Err("GSE adapter base source does not match the artifact manifest".to_string());
    }

    let required_paths = HashSet::from([
        "dll/dll/oxo_achievement_pipe_adapter.h".to_string(),
        "dll/dll/steam_user_stats.h".to_string(),
        "dll/steam_user_stats.cpp".to_string(),
        "dll/steam_user_stats_achievements.cpp".to_string(),
        "dll/steam_user_stats_stats.cpp".to_string(),
    ]);
    let mut paths = HashSet::new();
    for file in &mut provenance.files {
        validate_relative_path(&file.relative_path)?;
        file.relative_path = file.relative_path.replace('\\', "/");
        file.sha256 = normalize_sha256(&file.sha256)?;
        if !paths.insert(file.relative_path.clone()) {
            return Err(format!(
                "duplicate GSE adapter source file {}",
                file.relative_path
            ));
        }
    }
    if paths != required_paths {
        return Err("GSE adapter patch provenance has an incomplete source set".to_string());
    }
    Ok(provenance)
}

pub fn local_runtime_integration_for_game(game_id: &str) -> LocalRuntimeIntegration {
    catalog_entry(game_id)
        .map(|entry| entry.integration.clone())
        .unwrap_or_default()
}

pub fn runtime_catalog_revision() -> Result<String, String> {
    Ok(runtime_catalog()?.revision.clone())
}

pub fn managed_runtime_version() -> Result<String, String> {
    Ok(artifact_manifest()?.runtime_version.clone())
}

pub fn save_providers_for_game(game_id: &str) -> Vec<SaveProvider> {
    local_runtime_integration_for_game(game_id).save_providers
}

pub fn catalog_app_id_for_game(game_id: &str) -> Option<u32> {
    catalog_entry(game_id).and_then(|entry| entry.app_id)
}

pub fn supports_managed_achievements(game_id: &str) -> bool {
    let integration = local_runtime_integration_for_game(game_id);
    integration.steam_runtime == SteamRuntimeMode::ManagedGse
        && integration.achievements_enabled
        && catalog_app_id_for_game(game_id).is_some()
}

fn catalog_entry(game_id: &str) -> Option<&'static RuntimeCatalogEntry> {
    let normalized = normalize_game_id(game_id).ok()?;
    runtime_catalog()
        .ok()?
        .games
        .iter()
        .find(|entry| entry.game_id == normalized)
}

#[tauri::command]
pub fn get_managed_gse_state(
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    inspect_state(
        &game_id,
        Path::new(&install_dir),
        Path::new(&launch_executable),
    )
}

#[tauri::command]
pub fn repair_managed_gse(
    app: AppHandle,
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    repair_with_expected_original(
        &app,
        &game_id,
        Path::new(&install_dir),
        Path::new(&launch_executable),
        None,
    )
}

#[tauri::command]
pub fn restore_managed_gse_original(
    app: AppHandle,
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    restore_original(
        &app,
        &game_id,
        Path::new(&install_dir),
        Path::new(&launch_executable),
    )
}

fn normalized_profile_v2(
    entry: &RuntimeCatalogEntry,
    target: &RuntimeTarget,
) -> Result<ManagedRuntimeProfileV2, String> {
    let artifact = artifact_for(target.architecture)?;
    let manifest = artifact_manifest()?;
    let save_provider = entry.integration.save_providers.first().map(|value| value.provider.clone());
    Ok(ManagedRuntimeProfileV2 {
        schema_version: MANAGED_STATE_SCHEMA,
        profile_id: format!("managed-gse-{}", entry.game_id),
        game_id: entry.game_id.clone(),
        app_id: entry
            .app_id
            .ok_or_else(|| "managed runtime profile requires an appId".to_string())?,
        canonical_upstream: CANONICAL_GSE_UPSTREAM.to_string(),
        immutable_commit: CANONICAL_GSE_COMMIT.to_string(),
        build_id: manifest.runtime_version.clone(),
        patch_set_hash: manifest.source.adapter_patch_sha256.clone(),
        license: manifest.source.license.clone(),
        // The current imported snapshot does not itself record the immutable git commit.
        // The profile pins the intended upstream, but release UI must keep this false
        // until a clean-source manifest proves that exact commit and its submodules.
        provenance_verified: false,
        executable_allowlist: vec![target.launch_executable_relative_path.clone()],
        runtime_process_allowlist: vec![target
            .launch_executable_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "launch executable name is invalid".to_string())?
            .to_string()],
        anti_cheat_policy: AntiCheatPolicyV2::BlockProtectedRuntime,
        targets: vec![ManagedRuntimeTargetSpecV2 {
            relative_path: target.dll_relative_path.clone(),
            architecture: target.architecture,
            component: RuntimeComponentV2::SteamApi,
            allowed_original_sha256: entry.allowed_original_sha256.clone(),
            managed_sha256: artifact.sha256.clone(),
        }],
        generated_settings: vec!["steam_settings".to_string()],
        generated_interfaces: vec!["steam_interfaces.txt".to_string()],
        generated_assets: Vec::new(),
        renderer: OverlayRendererV2::Disabled,
        hotkey: "Shift+Tab".to_string(),
        achievement_protocol_version: ACHIEVEMENT_PIPE_PROTOCOL_VERSION,
        save_provider,
        save_root: None,
        cloud_policy: "localWinsOnceOnRestore".to_string(),
        required_components: vec![format!(
            "gseRegular:{}",
            architecture_label(target.architecture)
        )],
        on_demand_components: vec!["gseExperimental".to_string()],
        restore_constraints: vec![
            "gameClosed".to_string(),
            "currentHashMatchesReceipt".to_string(),
            "exactOwnedFilesOnly".to_string(),
        ],
    })
}

#[tauri::command]
pub fn plan_managed_runtime(
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedRuntimePlanV2, String> {
    let normalized_game_id = normalize_game_id(&game_id)?;
    let entry = catalog_entry(&normalized_game_id)
        .ok_or_else(|| "This game is not authorized by the managed runtime catalog".to_string())?;
    if entry.integration.steam_runtime != SteamRuntimeMode::ManagedGse {
        return Err("This game does not opt in to managedGse".to_string());
    }
    let target = resolve_target(Path::new(&install_dir), Path::new(&launch_executable))?;
    let profile = normalized_profile_v2(entry, &target)?;
    let state = inspect_state(&normalized_game_id, Path::new(&install_dir), Path::new(&launch_executable))?;
    let before_sha256 = optional_file_hash(&target.dll_path)?;
    let action = match state.status {
        ManagedGseStatus::Installed => "verify",
        ManagedGseStatus::Missing => "create",
        ManagedGseStatus::Restored | ManagedGseStatus::RepairRequired => "replace",
        ManagedGseStatus::Conflict => "blocked",
        ManagedGseStatus::NotManaged => "none",
    }
    .to_string();
    let blocked_reason = if !profile.provenance_verified {
        Some(
            "RUNTIME_PROVENANCE_UNVERIFIED: the managed runtime does not yet prove the pinned source commit, toolchain, patch set, binary hashes, and license bundle"
                .to_string(),
        )
    } else if state.status == ManagedGseStatus::Conflict {
        Some(state.message.clone())
    } else {
        None
    };
    Ok(ManagedRuntimePlanV2 {
        schema_version: MANAGED_STATE_SCHEMA,
        changes: vec![ManagedRuntimeSemanticChangeV2 {
            target_relative_path: target.dll_relative_path,
            action,
            before_sha256,
            after_sha256: profile.targets[0].managed_sha256.clone(),
        }],
        can_apply: blocked_reason.is_none(),
        blocked_reason,
        profile,
        state,
    })
}

#[tauri::command]
pub fn apply_managed_runtime(
    app: AppHandle,
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    repair_with_expected_original(
        &app,
        &game_id,
        Path::new(&install_dir),
        Path::new(&launch_executable),
        None,
    )
}

#[tauri::command]
pub fn verify_managed_runtime(
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    inspect_state(&game_id, Path::new(&install_dir), Path::new(&launch_executable))
}

#[tauri::command]
pub fn repair_managed_runtime(
    app: AppHandle,
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    apply_managed_runtime(app, game_id, install_dir, launch_executable)
}

#[tauri::command]
pub fn restore_managed_runtime(
    app: AppHandle,
    game_id: String,
    install_dir: String,
    launch_executable: String,
) -> Result<ManagedGseState, String> {
    restore_original(
        &app,
        &game_id,
        Path::new(&install_dir),
        Path::new(&launch_executable),
    )
}

pub fn ensure_after_install(
    app: &AppHandle,
    game_id: &str,
    install_root: &Path,
    launch_executable: &Path,
    manifest: &VersionManifest,
) -> Result<Option<ManagedGseState>, String> {
    let Some(entry) = catalog_entry(game_id) else {
        return Ok(None);
    };
    if entry.integration.steam_runtime != SteamRuntimeMode::ManagedGse {
        return Ok(None);
    }
    let target = resolve_target(install_root, launch_executable)?;
    let expected_original = manifest
        .files
        .iter()
        .find(|file| normalized_relative_key(&file.path) == target.dll_relative_path)
        .map(|file| normalize_sha256(&file.sha256))
        .transpose()?;
    repair_with_expected_original(
        app,
        game_id,
        install_root,
        launch_executable,
        expected_original.as_deref(),
    )
    .map(Some)
}

pub fn verify_before_launch(
    game_id: &str,
    install_root: &Path,
    launch_executable: &Path,
) -> Result<Option<ManagedGseState>, String> {
    let Some(entry) = catalog_entry(game_id) else {
        return Ok(None);
    };
    if entry.integration.steam_runtime != SteamRuntimeMode::ManagedGse {
        return Ok(None);
    }
    let state = inspect_state(game_id, install_root, launch_executable)?;
    if state.status != ManagedGseStatus::Installed {
        return Err(format!(
            "Managed GSE verification failed: {}. Close the game and use Repair Integration.",
            state.message
        ));
    }
    Ok(Some(state))
}

fn inspect_state(
    game_id: &str,
    install_root: &Path,
    launch_executable: &Path,
) -> Result<ManagedGseState, String> {
    let normalized_game_id = normalize_game_id(game_id)?;
    let catalog = runtime_catalog()?;
    let entry = catalog
        .games
        .iter()
        .find(|entry| entry.game_id == normalized_game_id);
    let Some(entry) = entry else {
        return Ok(unmanaged_state(&normalized_game_id, &catalog.revision));
    };
    if entry.integration.steam_runtime != SteamRuntimeMode::ManagedGse {
        return Ok(ManagedGseState {
            schema_version: MANAGED_STATE_SCHEMA,
            game_id: normalized_game_id,
            app_id: entry.app_id,
            steam_runtime: entry.integration.steam_runtime,
            achievements_enabled: entry.integration.achievements_enabled,
            status: ManagedGseStatus::NotManaged,
            catalog_revision: catalog.revision.clone(),
            runtime_version: None,
            architecture: None,
            dll_path: None,
            managed_sha256: None,
            original_sha256: None,
            original_backup_path: None,
            profile_id: None,
            transaction_id: None,
            owned_files: Vec::new(),
            message: "This catalog entry does not use a managed Steam runtime.".to_string(),
        });
    }

    let target = resolve_target(install_root, launch_executable)?;
    let artifact = artifact_for(target.architecture)?;
    let profile = normalized_profile_v2(entry, &target)?;
    let current_hash = optional_file_hash(&target.dll_path)?;
    let receipt = read_receipt(&target.state_path)?;

    let (status, message) = match (&receipt, current_hash.as_deref()) {
        (_, None) => (
            ManagedGseStatus::Missing,
            format!("{} is missing", artifact.file_name),
        ),
        (Some(receipt), Some(hash)) => {
            validate_receipt(receipt, &normalized_game_id, &target)?;
            let file = primary_receipt_file(receipt)?;
            let (original_sha256, _) = original_parts(file);
            if hash == file.managed_sha256 {
                if receipt.runtime_version == artifact_manifest()?.runtime_version
                    && hash == artifact.sha256
                    && receipt.restored_at.is_none()
                {
                    if profile.provenance_verified {
                        (
                            ManagedGseStatus::Installed,
                            "Managed GSE is installed and verified.".to_string(),
                        )
                    } else {
                        (
                            ManagedGseStatus::RepairRequired,
                            "Managed GSE bytes match the imported artifact, but immutable source/build provenance is not verified; launch is blocked."
                                .to_string(),
                        )
                    }
                } else {
                    (
                        ManagedGseStatus::RepairRequired,
                        "Managed GSE is an older or restored revision and must be repaired."
                            .to_string(),
                    )
                }
            } else if original_sha256 == Some(hash) && receipt.restored_at.is_some() {
                (
                    ManagedGseStatus::Restored,
                    "The original Steam runtime is restored.".to_string(),
                )
            } else {
                (
                    ManagedGseStatus::Conflict,
                    "The Steam runtime DLL changed outside 0xoLemon; no file was modified."
                        .to_string(),
                )
            }
        }
        (None, Some(hash)) if hash == artifact.sha256 => (
            ManagedGseStatus::RepairRequired,
            "The bundled GSE DLL is present but has no durable managed-state receipt.".to_string(),
        ),
        (None, Some(_)) => (
            ManagedGseStatus::Conflict,
            "An unrecognized Steam runtime DLL is present; no file was modified.".to_string(),
        ),
    };

    Ok(state_from_parts(
        entry,
        &catalog.revision,
        &target,
        receipt.as_ref(),
        status,
        message,
    ))
}

fn repair_with_expected_original(
    app: &AppHandle,
    game_id: &str,
    install_root: &Path,
    launch_executable: &Path,
    expected_original_sha256: Option<&str>,
) -> Result<ManagedGseState, String> {
    ensure_game_closed(game_id, launch_executable)?;
    managed_file_transaction::recover_pending(app)?;
    let normalized_game_id = normalize_game_id(game_id)?;
    let catalog = runtime_catalog()?;
    let entry = catalog
        .games
        .iter()
        .find(|entry| entry.game_id == normalized_game_id)
        .ok_or_else(|| "This game is not authorized by the managed runtime catalog".to_string())?;
    if entry.integration.steam_runtime != SteamRuntimeMode::ManagedGse {
        return Err("This game does not opt in to managedGse".to_string());
    }

    let target = resolve_target(install_root, launch_executable)?;
    let profile = normalized_profile_v2(entry, &target)?;
    if !profile.provenance_verified {
        return Err(
            "RUNTIME_PROVENANCE_UNVERIFIED: refusing to mutate the game until the pinned source checkout, toolchain, patch set, binary hashes, and license bundle are verified"
                .to_string(),
        );
    }
    let artifact = verified_artifact(app, target.architecture)?;
    let current_hash = optional_file_hash(&target.dll_path)?;
    let existing = read_receipt(&target.state_path)?;
    if let Some(receipt) = &existing {
        validate_receipt(receipt, &normalized_game_id, &target)?;
    }
    let existing_file = existing
        .as_ref()
        .map(primary_receipt_file)
        .transpose()?;

    let expected_original = expected_original_sha256.map(normalize_sha256).transpose()?;
    let allowed_original = current_hash.as_ref().is_some_and(|hash| {
        expected_original.as_deref() == Some(hash.as_str())
            || entry
                .allowed_original_sha256
                .iter()
                .any(|allowed| allowed == hash)
    });

    let (mut original_sha256, mut backup_relative) = existing_file
        .map(original_parts)
        .map(|(sha256, backup)| (sha256.map(str::to_string), backup.map(str::to_string)))
        .unwrap_or((None, None));
    let current_is_managed = current_hash.as_ref().is_some_and(|hash| {
        hash == &artifact.sha256
            || existing_file.is_some_and(|file| hash == &file.managed_sha256)
    });
    let current_is_restored = current_hash.as_ref().is_some_and(|hash| {
        existing_file
            .and_then(|file| original_parts(file).0)
            == Some(hash.as_str())
    });

    if is_unknown_runtime(
        current_hash.as_deref(),
        current_is_managed,
        current_is_restored,
        allowed_original,
    ) {
        return Err(
            "MANAGED_GSE_UNKNOWN_DLL: The existing Steam runtime hash is not authorized; no file was modified."
                .to_string(),
        );
    }

    let mut changes = Vec::new();
    if current_hash.is_some() && !current_is_managed && original_sha256.is_none() {
        if target.backup_path.exists() {
            return Err(format!(
                "MANAGED_GSE_BACKUP_CONFLICT: {} already exists without a matching receipt",
                target.backup_path.display()
            ));
        }
        let hash = current_hash.clone().expect("checked above");
        original_sha256 = Some(hash.clone());
        backup_relative = Some(target.backup_relative_path.clone());
        if let Some(parent) = target.backup_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create managed runtime backup directory: {error}")
            })?;
        }
        changes.push(ManagedFileChange::Replace(ManagedFileSpec {
            source: target.dll_path.clone(),
            target: target.backup_path.clone(),
            allowed_source_root: target.install_root.clone(),
            allowed_target_root: target.install_root.clone(),
            expected_sha256: hash,
        }));
    } else if let (Some(original_hash), Some(relative)) =
        (original_sha256.as_deref(), backup_relative.as_deref())
    {
        let backup = safe_join_existing_or_target(&target.install_root, relative)?;
        let actual = optional_file_hash(&backup)?.ok_or_else(|| {
            format!(
                "Managed GSE original backup is missing: {}",
                backup.display()
            )
        })?;
        if actual != original_hash {
            return Err("Managed GSE original backup hash mismatch".to_string());
        }
    }

    let transaction_id = Uuid::new_v4();
    let original = match (original_sha256, backup_relative) {
        (Some(sha256), Some(backup_relative_path)) => OriginalFileStateV2::Present {
            sha256,
            backup_relative_path,
        },
        (None, None) => OriginalFileStateV2::Absent,
        _ => return Err("Managed GSE original backup metadata is incomplete".to_string()),
    };
    let receipt = ManagedRuntimeReceiptV2 {
        schema_version: MANAGED_STATE_SCHEMA,
        transaction_id: transaction_id.to_string(),
        profile_id: format!("managed-gse-{}", normalized_game_id),
        game_id: normalized_game_id.clone(),
        app_id: entry.app_id,
        catalog_revision: catalog.revision.clone(),
        runtime_version: artifact_manifest()?.runtime_version.clone(),
        installed_at: existing
            .as_ref()
            .map(|state| state.installed_at.clone())
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        restored_at: None,
        files: vec![ManagedOwnedFileReceiptV2 {
            component: RuntimeComponentV2::SteamApi,
            architecture: target.architecture,
            target_relative_path: target.dll_relative_path.clone(),
            artifact_id: format!("gse-regular-{}", architecture_label(target.architecture)),
            managed_sha256: artifact.sha256.clone(),
            original,
        }],
    };
    let receipt_file = primary_receipt_file(&receipt)?;

    if current_hash.as_deref() == Some(artifact.sha256.as_str())
        && existing.as_ref().is_some_and(|state| {
            state.runtime_version == receipt.runtime_version
                && state
                    .files
                    .first()
                    .is_some_and(|file| file.managed_sha256 == receipt_file.managed_sha256)
                && state.restored_at.is_none()
        })
    {
        return inspect_state(game_id, install_root, launch_executable);
    }

    let state_source = persist_state_source(app, &receipt)?;
    changes.push(ManagedFileChange::Replace(ManagedFileSpec {
        source: artifact.path.clone(),
        target: target.dll_path.clone(),
        allowed_source_root: artifact.root.clone(),
        allowed_target_root: target.install_root.clone(),
        expected_sha256: artifact.sha256.clone(),
    }));
    changes.push(ManagedFileChange::Replace(ManagedFileSpec {
        source: state_source.path.clone(),
        target: target.state_path.clone(),
        allowed_source_root: state_source.root.clone(),
        allowed_target_root: target.install_root.clone(),
        expected_sha256: state_source.sha256.clone(),
    }));

    let result = managed_file_transaction::apply_changes_with_id(
        app,
        transaction_id,
        "managed-gse-repair",
        changes,
    );
    remove_exact_file(&state_source.path).ok();
    result?;
    inspect_state(game_id, install_root, launch_executable)
}

fn is_unknown_runtime(
    current_hash: Option<&str>,
    current_is_managed: bool,
    current_is_restored: bool,
    allowed_original: bool,
) -> bool {
    current_hash.is_some() && !current_is_managed && !current_is_restored && !allowed_original
}

fn restore_original(
    app: &AppHandle,
    game_id: &str,
    install_root: &Path,
    launch_executable: &Path,
) -> Result<ManagedGseState, String> {
    ensure_game_closed(game_id, launch_executable)?;
    managed_file_transaction::recover_pending(app)?;
    let target = resolve_target(install_root, launch_executable)?;
    let mut receipt = read_receipt(&target.state_path)?
        .ok_or_else(|| "Managed GSE has no receipt to restore".to_string())?;
    let normalized_game_id = normalize_game_id(game_id)?;
    validate_receipt(&receipt, &normalized_game_id, &target)?;
    let receipt_file = primary_receipt_file(&receipt)?;
    let managed_sha256 = receipt_file.managed_sha256.clone();
    let (original_sha256, original_backup_relative_path) = original_parts(receipt_file);
    let original_sha256 = original_sha256.map(str::to_string);
    let original_backup_relative_path = original_backup_relative_path.map(str::to_string);

    let current = optional_file_hash(&target.dll_path)?;
    if receipt.restored_at.is_some() {
        let already_restored = match original_sha256.as_deref() {
            Some(original) => current.as_deref() == Some(original),
            None => current.is_none(),
        };
        if already_restored {
            return inspect_state(game_id, install_root, launch_executable);
        }
    }
    if current.as_deref() != Some(managed_sha256.as_str()) {
        return Err(
            "MANAGED_GSE_RESTORE_CONFLICT: The managed DLL changed; refusing to overwrite it."
                .to_string(),
        );
    }

    let transaction_id = Uuid::new_v4();
    receipt.transaction_id = transaction_id.to_string();
    receipt.restored_at = Some(Utc::now().to_rfc3339());
    let state_source = persist_state_source(app, &receipt)?;
    let mut changes = Vec::new();
    match (
        original_sha256.as_deref(),
        original_backup_relative_path.as_deref(),
    ) {
        (Some(expected), Some(relative)) => {
            let backup = safe_join_existing_or_target(&target.install_root, relative)?;
            let actual = optional_file_hash(&backup)?
                .ok_or_else(|| "Managed GSE original backup is missing".to_string())?;
            if actual != expected {
                remove_exact_file(&state_source.path).ok();
                return Err("Managed GSE original backup hash mismatch".to_string());
            }
            changes.push(ManagedFileChange::Replace(ManagedFileSpec {
                source: backup,
                target: target.dll_path.clone(),
                allowed_source_root: target.install_root.clone(),
                allowed_target_root: target.install_root.clone(),
                expected_sha256: expected.to_string(),
            }));
        }
        (None, None) => changes.push(ManagedFileChange::Delete(ManagedDeleteSpec {
            target: target.dll_path.clone(),
            allowed_target_root: target.install_root.clone(),
        })),
        _ => {
            remove_exact_file(&state_source.path).ok();
            return Err("Managed GSE receipt has incomplete original backup metadata".to_string());
        }
    }
    changes.push(ManagedFileChange::Replace(ManagedFileSpec {
        source: state_source.path.clone(),
        target: target.state_path.clone(),
        allowed_source_root: state_source.root.clone(),
        allowed_target_root: target.install_root.clone(),
        expected_sha256: state_source.sha256.clone(),
    }));

    let result = managed_file_transaction::apply_changes_with_id(
        app,
        transaction_id,
        "managed-gse-restore",
        changes,
    );
    remove_exact_file(&state_source.path).ok();
    result?;
    inspect_state(game_id, install_root, launch_executable)
}

#[derive(Debug)]
struct VerifiedArtifact {
    root: PathBuf,
    path: PathBuf,
    sha256: String,
}

fn verified_artifact(
    app: &AppHandle,
    architecture: PeArchitecture,
) -> Result<VerifiedArtifact, String> {
    let artifact = artifact_for(architecture)?;
    let root = resolve_emu_resource_root(app)?;
    let path = safe_join_existing_or_target(&root, &artifact.relative_path)?;
    if !path.is_file() {
        return Err(format!(
            "Bundled GSE artifact is missing: {}",
            path.display()
        ));
    }
    let metadata =
        fs::metadata(&path).map_err(|error| format!("failed to inspect GSE artifact: {error}"))?;
    if metadata.len() != artifact.size_bytes {
        return Err(format!(
            "Bundled GSE artifact size mismatch for {}",
            path.display()
        ));
    }
    let actual = sha256_file(&path)?;
    if actual != artifact.sha256 {
        return Err(format!(
            "Bundled GSE artifact hash mismatch for {}",
            path.display()
        ));
    }
    if read_pe_architecture(&path)? != architecture {
        return Err(format!(
            "Bundled GSE artifact architecture mismatch for {}",
            path.display()
        ));
    }
    Ok(VerifiedArtifact {
        root,
        path,
        sha256: artifact.sha256.clone(),
    })
}

fn artifact_for(architecture: PeArchitecture) -> Result<&'static RuntimeArtifact, String> {
    artifact_manifest()?
        .artifacts
        .iter()
        .find(|artifact| artifact.architecture == architecture)
        .ok_or_else(|| format!("No GSE artifact is available for {architecture:?}"))
}

fn resolve_emu_resource_root(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("resources").join("emu"));
        candidates.push(resource_dir.join("emu"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/emu"));
    for candidate in candidates {
        if candidate.join("manifest.json").is_file() {
            return candidate
                .canonicalize()
                .map_err(|error| format!("failed to resolve GSE resource directory: {error}"));
        }
    }
    Err("Bundled GSE resource directory is unavailable".to_string())
}

fn resolve_target(install_root: &Path, launch_executable: &Path) -> Result<RuntimeTarget, String> {
    let install_root = validate_install_root(install_root)?;
    let executable = if launch_executable.is_absolute() {
        launch_executable.to_path_buf()
    } else {
        safe_join_existing_or_target(&install_root, &launch_executable.to_string_lossy())?
    };
    let executable = executable.canonicalize().map_err(|error| {
        format!(
            "failed to resolve launch executable {}: {error}",
            executable.display()
        )
    })?;
    if !executable.starts_with(&install_root) || !executable.is_file() {
        return Err(
            "Launch executable must be a file inside the selected install directory".to_string(),
        );
    }
    reject_reparse_chain(&install_root, &executable)?;
    let launch_executable_relative_path = normalized_relative_path(&install_root, &executable)?;
    let architecture = read_pe_architecture(&executable)?;
    let file_name = artifact_for(architecture)?.file_name.clone();
    let parent = executable
        .parent()
        .ok_or_else(|| "Launch executable has no parent directory".to_string())?;
    let dll_path = parent.join(file_name);
    let dll_relative_path = normalized_relative_path(&install_root, &dll_path)?;
    let state_path = install_root.join(STATE_DIRECTORY).join(STATE_FILE);
    let backup_path = install_root
        .join(STATE_DIRECTORY)
        .join(ORIGINAL_DIRECTORY)
        .join(architecture_label(architecture))
        .join(artifact_for(architecture)?.file_name.as_str());
    let backup_relative_path = normalized_relative_path(&install_root, &backup_path)?;
    Ok(RuntimeTarget {
        install_root,
        launch_executable_path: executable,
        launch_executable_relative_path,
        architecture,
        dll_path,
        dll_relative_path,
        state_path,
        backup_path,
        backup_relative_path,
    })
}

fn validate_install_root(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("Install directory must be an existing absolute directory".to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve install directory: {error}"))?;
    if canonical.parent().is_none() || canonical.components().count() < 2 {
        return Err(
            "A drive or filesystem root cannot be used as a game install directory".to_string(),
        );
    }
    reject_reparse_point(&canonical)?;

    if let Some(windows) = std::env::var_os("WINDIR").map(PathBuf::from) {
        if let Ok(windows) = windows.canonicalize() {
            if canonical == windows || canonical.starts_with(windows.join("System32")) {
                return Err(
                    "Windows system directories cannot be managed as game installs".to_string(),
                );
            }
        }
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(launcher_dir) = current_exe
            .parent()
            .and_then(|path| path.canonicalize().ok())
        {
            if canonical == launcher_dir {
                return Err(
                    "The launcher directory cannot be managed as a game install".to_string()
                );
            }
        }
    }
    Ok(canonical)
}

pub fn read_pe_architecture(path: &Path) -> Result<PeArchitecture, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open PE file {}: {error}", path.display()))?;
    let length = file
        .metadata()
        .map_err(|error| format!("failed to inspect PE file {}: {error}", path.display()))?
        .len();
    if length < 70 {
        return Err(format!("PE file is too small: {}", path.display()));
    }
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|error| format!("failed to read DOS header {}: {error}", path.display()))?;
    if &dos[0..2] != b"MZ" {
        return Err(format!("invalid DOS signature in {}", path.display()));
    }
    let pe_offset = u32::from_le_bytes(
        dos[0x3c..0x40]
            .try_into()
            .map_err(|_| "invalid DOS header offset".to_string())?,
    ) as u64;
    if pe_offset > length.saturating_sub(6) {
        return Err(format!(
            "PE header offset is out of bounds in {}",
            path.display()
        ));
    }
    file.seek(SeekFrom::Start(pe_offset))
        .map_err(|error| format!("failed to seek PE header {}: {error}", path.display()))?;
    let mut header = [0u8; 6];
    file.read_exact(&mut header)
        .map_err(|error| format!("failed to read PE header {}: {error}", path.display()))?;
    if &header[0..4] != b"PE\0\0" {
        return Err(format!("invalid PE signature in {}", path.display()));
    }
    match u16::from_le_bytes([header[4], header[5]]) {
        0x014c => Ok(PeArchitecture::X86),
        0x8664 => Ok(PeArchitecture::X64),
        machine => Err(format!(
            "unsupported PE machine 0x{machine:04x} in {}",
            path.display()
        )),
    }
}

fn ensure_game_closed(game_id: &str, launch_executable: &Path) -> Result<(), String> {
    if crate::cloud_save::is_game_running(game_id) {
        return Err("Close the game before changing its managed runtime".to_string());
    }
    let name = launch_executable
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Launch executable name is invalid".to_string())?;
    if is_process_name_running(name)? {
        return Err(format!("Close {name} before changing its managed runtime"));
    }
    Ok(())
}

#[cfg(windows)]
fn is_process_name_running(name: &str) -> Result<bool, String> {
    use std::mem::{size_of, zeroed};
    use winapi::shared::minwindef::FALSE;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(format!(
                "failed to enumerate processes: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut has_entry = Process32FirstW(snapshot, &mut entry);
        while has_entry != FALSE {
            let end = entry
                .szExeFile
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(entry.szExeFile.len());
            let process_name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if process_name.eq_ignore_ascii_case(name) {
                CloseHandle(snapshot);
                return Ok(true);
            }
            has_entry = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    Ok(false)
}

#[cfg(not(windows))]
fn is_process_name_running(_name: &str) -> Result<bool, String> {
    Ok(false)
}

fn legacy_transaction_id(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x50;
    id[8] = (id[8] & 0x3f) | 0x80;
    Uuid::from_bytes(id).to_string()
}

fn normalize_v1_receipt(receipt: ManagedGseReceiptV1, bytes: &[u8]) -> ManagedRuntimeReceiptV2 {
    let original = match (
        receipt.original_sha256,
        receipt.original_backup_relative_path,
    ) {
        (Some(sha256), Some(backup_relative_path)) => OriginalFileStateV2::Present {
            sha256,
            backup_relative_path,
        },
        _ => OriginalFileStateV2::Absent,
    };
    ManagedRuntimeReceiptV2 {
        schema_version: MANAGED_STATE_SCHEMA,
        transaction_id: legacy_transaction_id(bytes),
        profile_id: format!("legacy-managed-gse-{}", receipt.game_id),
        game_id: receipt.game_id,
        app_id: receipt.app_id,
        catalog_revision: receipt.catalog_revision,
        runtime_version: receipt.runtime_version,
        installed_at: receipt.installed_at,
        restored_at: receipt.restored_at,
        files: vec![ManagedOwnedFileReceiptV2 {
            component: RuntimeComponentV2::SteamApi,
            architecture: receipt.architecture,
            artifact_id: format!("gse-regular-{}", architecture_label(receipt.architecture)),
            target_relative_path: receipt.dll_relative_path,
            managed_sha256: receipt.managed_sha256,
            original,
        }],
    }
}

fn read_receipt(path: &Path) -> Result<Option<ManagedRuntimeReceiptV2>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read managed GSE receipt: {error}"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid managed GSE receipt: {error}"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "managed GSE receipt is missing schemaVersion".to_string())? as u32;
    let receipt = match schema_version {
        LEGACY_MANAGED_STATE_SCHEMA => {
            let v1: ManagedGseReceiptV1 = serde_json::from_value(value)
                .map_err(|error| format!("invalid managed GSE v1 receipt: {error}"))?;
            if v1.original_sha256.is_some() != v1.original_backup_relative_path.is_some() {
                return Err("Managed GSE v1 receipt has incomplete original backup metadata".to_string());
            }
            normalize_v1_receipt(v1, &bytes)
        }
        MANAGED_STATE_SCHEMA => serde_json::from_value(value)
            .map_err(|error| format!("invalid managed runtime v2 receipt: {error}"))?,
        unsupported => {
            return Err(format!(
                "unsupported managed GSE receipt schema {unsupported}"
            ))
        }
    };
    Ok(Some(receipt))
}

fn validate_receipt(
    receipt: &ManagedRuntimeReceiptV2,
    game_id: &str,
    target: &RuntimeTarget,
) -> Result<(), String> {
    if receipt.schema_version != MANAGED_STATE_SCHEMA
        || receipt.game_id != game_id
        || receipt.profile_id.trim().is_empty()
        || Uuid::parse_str(&receipt.transaction_id).is_err()
    {
        return Err("Managed GSE receipt does not match this game installation".to_string());
    }
    let mut targets = HashSet::new();
    let mut backups = HashSet::new();
    for file in &receipt.files {
        validate_relative_path(&file.target_relative_path)?;
        let target_key = normalized_relative_key(&file.target_relative_path);
        if !targets.insert(target_key.clone()) {
            return Err("Managed runtime receipt contains duplicate target paths".to_string());
        }
        normalize_sha256(&file.managed_sha256)?;
        match &file.original {
            OriginalFileStateV2::Absent => {}
            OriginalFileStateV2::Present {
                sha256,
                backup_relative_path,
            } => {
                normalize_sha256(sha256)?;
                validate_relative_path(backup_relative_path)?;
                if !backups.insert(normalized_relative_key(backup_relative_path)) {
                    return Err("Managed runtime receipt contains duplicate backup paths".to_string());
                }
            }
        }
        if file.component != RuntimeComponentV2::SteamApi
            || file.architecture != target.architecture
            || target_key != target.dll_relative_path
        {
            return Err("Managed GSE receipt file set does not match this profile".to_string());
        }
    }
    if receipt.files.len() != 1 {
        return Err("Managed GSE receipt file set does not match this profile".to_string());
    }
    Ok(())
}

fn primary_receipt_file(
    receipt: &ManagedRuntimeReceiptV2,
) -> Result<&ManagedOwnedFileReceiptV2, String> {
    receipt
        .files
        .first()
        .filter(|_| receipt.files.len() == 1)
        .ok_or_else(|| "Managed GSE receipt does not contain one exact runtime target".to_string())
}

fn original_parts(file: &ManagedOwnedFileReceiptV2) -> (Option<&str>, Option<&str>) {
    match &file.original {
        OriginalFileStateV2::Absent => (None, None),
        OriginalFileStateV2::Present {
            sha256,
            backup_relative_path,
        } => (Some(sha256.as_str()), Some(backup_relative_path.as_str())),
    }
}

struct StateSource {
    root: PathBuf,
    path: PathBuf,
    sha256: String,
}

fn persist_state_source(
    app: &AppHandle,
    receipt: &ManagedRuntimeReceiptV2,
) -> Result<StateSource, String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("managed runtime state directory unavailable: {error}"))?
        .join("managed-runtime-state-sources");
    fs::create_dir_all(&root)
        .map_err(|error| format!("failed to create managed runtime state directory: {error}"))?;
    let path = root.join(format!("{}.json", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("failed to serialize managed GSE receipt: {error}"))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("failed to create managed GSE state source: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("failed to write managed GSE state source: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("failed to flush managed GSE state source: {error}"))?;
    Ok(StateSource {
        root,
        path,
        sha256: hex::encode(Sha256::digest(&bytes)),
    })
}

fn state_from_parts(
    entry: &RuntimeCatalogEntry,
    catalog_revision: &str,
    target: &RuntimeTarget,
    receipt: Option<&ManagedRuntimeReceiptV2>,
    status: ManagedGseStatus,
    message: String,
) -> ManagedGseState {
    let primary = receipt.and_then(|value| value.files.first());
    let (original_sha256, original_backup_relative) = primary
        .map(original_parts)
        .unwrap_or((None, None));
    ManagedGseState {
        schema_version: MANAGED_STATE_SCHEMA,
        game_id: entry.game_id.clone(),
        app_id: entry.app_id,
        steam_runtime: entry.integration.steam_runtime,
        achievements_enabled: entry.integration.achievements_enabled,
        status,
        catalog_revision: catalog_revision.to_string(),
        runtime_version: receipt.map(|value| value.runtime_version.clone()),
        architecture: Some(target.architecture),
        dll_path: Some(target.dll_path.display().to_string()),
        managed_sha256: primary.map(|value| value.managed_sha256.clone()),
        original_sha256: original_sha256.map(str::to_string),
        original_backup_path: original_backup_relative
            .and_then(|relative| safe_join_existing_or_target(&target.install_root, relative).ok())
            .map(|path| path.display().to_string()),
        profile_id: receipt.map(|value| value.profile_id.clone()),
        transaction_id: receipt.map(|value| value.transaction_id.clone()),
        owned_files: primary
            .map(|file| vec![ManagedRuntimeFileStateV2 {
                component: file.component,
                architecture: file.architecture,
                target_path: target.dll_path.display().to_string(),
                managed_sha256: file.managed_sha256.clone(),
                original_sha256: original_sha256.map(str::to_string),
                original_backup_path: original_backup_relative
                    .and_then(|relative| safe_join_existing_or_target(&target.install_root, relative).ok())
                    .map(|path| path.display().to_string()),
            }])
            .unwrap_or_default(),
        message,
    }
}

fn unmanaged_state(game_id: &str, revision: &str) -> ManagedGseState {
    ManagedGseState {
        schema_version: MANAGED_STATE_SCHEMA,
        game_id: game_id.to_string(),
        app_id: None,
        steam_runtime: SteamRuntimeMode::None,
        achievements_enabled: false,
        status: ManagedGseStatus::NotManaged,
        catalog_revision: revision.to_string(),
        runtime_version: None,
        architecture: None,
        dll_path: None,
        managed_sha256: None,
        original_sha256: None,
        original_backup_path: None,
        profile_id: None,
        transaction_id: None,
        owned_files: Vec::new(),
        message: "This game has no managed runtime metadata; the launcher will not inject a DLL."
            .to_string(),
    }
}

fn normalize_game_id(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err("Invalid game id".to_string());
    }
    Ok(normalized)
}

fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("SHA-256 must contain exactly 64 hexadecimal characters".to_string());
    }
    Ok(normalized)
}

fn validate_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe relative path: {value}"));
    }
    Ok(())
}

fn safe_join_existing_or_target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative_path(relative)?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("failed to resolve allowed root {}: {error}", root.display()))?;
    let path = root.join(relative.replace('/', "\\"));
    if !path.starts_with(&root) {
        return Err(format!("path escapes allowed root: {relative}"));
    }
    Ok(path)
}

fn normalized_relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("{} escapes {}", path.display(), root.display()))?;
    let value = relative.to_string_lossy().replace('\\', "/");
    validate_relative_path(&value)?;
    Ok(value.to_ascii_lowercase())
}

fn normalized_relative_key(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn architecture_label(value: PeArchitecture) -> &'static str {
    match value {
        PeArchitecture::X86 => "x86",
        PeArchitecture::X64 => "x64",
    }
}

fn optional_file_hash(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "Managed runtime target is not a file: {}",
                    path.display()
                ));
            }
            Ok(Some(sha256_file(path)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn remove_exact_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|error| {
                format!(
                    "failed to remove temporary file {}: {error}",
                    path.display()
                )
            })
        }
        Ok(_) => Err(format!(
            "refusing to remove non-file path: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to inspect temporary file {}: {error}",
            path.display()
        )),
    }
}

#[cfg(windows)]
fn reject_reparse_point(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!(
            "reparse points are not allowed: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn reject_reparse_point(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("symlinks are not allowed: {}", path.display()));
    }
    Ok(())
}

fn reject_reparse_chain(root: &Path, target: &Path) -> Result<(), String> {
    reject_reparse_point(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "Path escapes the selected install directory".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if current.exists() {
            reject_reparse_point(&current)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_minimal_pe(path: &Path, machine: u16) {
        let mut bytes = vec![0u8; 0x90];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("0xo-runtime-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn old_catalog_metadata_defaults_to_no_runtime() {
        let metadata: LocalRuntimeIntegration = serde_json::from_str("{}").unwrap();
        assert_eq!(metadata.steam_runtime, SteamRuntimeMode::None);
        assert!(!metadata.achievements_enabled);
        assert!(metadata.save_providers.is_empty());
    }

    #[test]
    fn catalog_enables_only_explicit_managed_game() {
        let managed = local_runtime_integration_for_game("007-first-light");
        assert_eq!(managed.steam_runtime, SteamRuntimeMode::ManagedGse);
        assert!(managed.achievements_enabled);

        let unknown = local_runtime_integration_for_game("unknown-game");
        assert_eq!(unknown.steam_runtime, SteamRuntimeMode::None);
        assert!(!unknown.achievements_enabled);
    }

    #[test]
    fn pe_parser_returns_result_for_x86_x64_and_invalid_files() {
        let root = test_dir("pe");
        let x86 = root.join("x86.exe");
        let x64 = root.join("x64.exe");
        let invalid = root.join("invalid.exe");
        write_minimal_pe(&x86, 0x014c);
        write_minimal_pe(&x64, 0x8664);
        fs::write(&invalid, b"not-a-pe").unwrap();

        assert_eq!(read_pe_architecture(&x86).unwrap(), PeArchitecture::X86);
        assert_eq!(read_pe_architecture(&x64).unwrap(), PeArchitecture::X64);
        assert!(read_pe_architecture(&invalid).is_err());

        fs::remove_file(x86).ok();
        fs::remove_file(x64).ok();
        fs::remove_file(invalid).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn runtime_manifests_have_pinned_integrity() {
        assert!(runtime_catalog().is_ok());
        assert!(artifact_manifest().is_ok());
        assert_eq!(artifact_manifest().unwrap().artifacts.len(), 2);
    }

    #[test]
    fn imported_runtime_profile_remains_fail_closed_without_commit_proof() {
        let root = test_dir("profile-provenance");
        let target = receipt_target(&root);
        let entry = catalog_entry("007-first-light").unwrap();
        let profile = normalized_profile_v2(entry, &target).unwrap();

        assert_eq!(profile.immutable_commit, CANONICAL_GSE_COMMIT);
        assert!(!profile.provenance_verified);
        assert_eq!(profile.renderer, OverlayRendererV2::Disabled);

        fs::remove_dir(root).ok();
    }

    #[test]
    fn gse_oracle_matrix_rejects_foreign_dlls_and_noncanonical_artifacts() {
        assert!(!is_unknown_runtime(None, false, false, false));
        assert!(!is_unknown_runtime(Some("managed"), true, false, false));
        assert!(!is_unknown_runtime(Some("restored"), false, true, false));
        assert!(!is_unknown_runtime(Some("allowed"), false, false, true));
        assert!(is_unknown_runtime(Some("foreign"), false, false, false));

        let canonical = artifact_manifest().unwrap().clone();
        let names = canonical
            .artifacts
            .iter()
            .map(|artifact| (artifact.architecture, artifact.file_name.as_str()))
            .collect::<HashSet<_>>();
        assert_eq!(
            names,
            HashSet::from([
                (PeArchitecture::X86, "steam_api.dll"),
                (PeArchitecture::X64, "steam_api64.dll"),
            ])
        );

        let mut duplicate_architecture = canonical.clone();
        duplicate_architecture.artifacts[1].architecture = PeArchitecture::X86;
        assert!(validate_artifact_manifest(duplicate_architecture)
            .unwrap_err()
            .contains("duplicate architecture"));

        let mut custom_bridge = canonical;
        custom_bridge.artifacts[1].file_name = "dinput8.dll".to_string();
        custom_bridge.artifacts[1].relative_path = "x64/dinput8.dll".to_string();
        assert!(validate_artifact_manifest(custom_bridge)
            .unwrap_err()
            .contains("invalid GSE artifact filename"));
    }

    fn receipt_target(root: &Path) -> RuntimeTarget {
        RuntimeTarget {
            install_root: root.to_path_buf(),
            launch_executable_path: root.join("bin/game.exe"),
            launch_executable_relative_path: "bin/game.exe".to_string(),
            architecture: PeArchitecture::X64,
            dll_path: root.join("bin/steam_api64.dll"),
            dll_relative_path: "bin/steam_api64.dll".to_string(),
            state_path: root.join(STATE_DIRECTORY).join(STATE_FILE),
            backup_path: root.join(".0xolemon/managed-runtime/original/bin/steam_api64.dll"),
            backup_relative_path: ".0xolemon/managed-runtime/original/bin/steam_api64.dll"
                .to_string(),
        }
    }

    fn receipt_file(target_path: &str) -> ManagedOwnedFileReceiptV2 {
        ManagedOwnedFileReceiptV2 {
            component: RuntimeComponentV2::SteamApi,
            architecture: PeArchitecture::X64,
            target_relative_path: target_path.to_string(),
            artifact_id: "gse-regular-x64".to_string(),
            managed_sha256: "a".repeat(64),
            original: OriginalFileStateV2::Absent,
        }
    }

    fn v2_receipt(files: Vec<ManagedOwnedFileReceiptV2>) -> ManagedRuntimeReceiptV2 {
        ManagedRuntimeReceiptV2 {
            schema_version: MANAGED_STATE_SCHEMA,
            transaction_id: Uuid::new_v4().to_string(),
            profile_id: "managed-gse-test-game".to_string(),
            game_id: "test-game".to_string(),
            app_id: Some(480),
            catalog_revision: "test".to_string(),
            runtime_version: "test".to_string(),
            installed_at: Utc::now().to_rfc3339(),
            restored_at: None,
            files,
        }
    }

    #[test]
    fn v1_receipt_normalizes_without_rewriting() {
        let root = test_dir("receipt-v1");
        let path = root.join(STATE_FILE);
        let v1 = ManagedGseReceiptV1 {
            schema_version: LEGACY_MANAGED_STATE_SCHEMA,
            game_id: "test-game".to_string(),
            app_id: Some(480),
            catalog_revision: "legacy".to_string(),
            runtime_version: "legacy-runtime".to_string(),
            architecture: PeArchitecture::X64,
            dll_relative_path: "bin/steam_api64.dll".to_string(),
            managed_sha256: "a".repeat(64),
            original_sha256: Some("b".repeat(64)),
            original_backup_relative_path: Some(
                ".0xolemon/managed-runtime/original/x64/steam_api64.dll".to_string(),
            ),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            restored_at: None,
        };
        let bytes = serde_json::to_vec_pretty(&v1).unwrap();
        fs::write(&path, &bytes).unwrap();

        let normalized = read_receipt(&path).unwrap().unwrap();
        assert_eq!(normalized.schema_version, MANAGED_STATE_SCHEMA);
        assert_eq!(normalized.files.len(), 1);
        assert_eq!(normalized.transaction_id, legacy_transaction_id(&bytes));
        assert!(matches!(
            normalized.files[0].original,
            OriginalFileStateV2::Present { .. }
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);

        fs::remove_file(path).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn v2_receipt_rejects_duplicate_target_paths_case_insensitively() {
        let root = test_dir("receipt-duplicate");
        let target = receipt_target(&root);
        let receipt = v2_receipt(vec![
            receipt_file("bin/steam_api64.dll"),
            receipt_file("BIN/STEAM_API64.DLL"),
        ]);
        let error = validate_receipt(&receipt, "test-game", &target).unwrap_err();
        assert!(error.contains("duplicate target"));
        fs::remove_dir(root).ok();
    }

    #[test]
    fn v2_receipt_rejects_backup_path_traversal() {
        let root = test_dir("receipt-traversal");
        let target = receipt_target(&root);
        let mut file = receipt_file("bin/steam_api64.dll");
        file.original = OriginalFileStateV2::Present {
            sha256: "b".repeat(64),
            backup_relative_path: "../outside.dll".to_string(),
        };
        let error = validate_receipt(&v2_receipt(vec![file]), "test-game", &target).unwrap_err();
        assert!(error.contains("unsafe relative path"));
        fs::remove_dir(root).ok();
    }
}
