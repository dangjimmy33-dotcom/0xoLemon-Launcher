use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use uuid::Uuid;
use zip::ZipArchive;

const MAX_PACKAGE_DOWNLOAD_BYTES: u64 = 600 * 1024 * 1024;
const MAX_PACKAGE_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const GITHUB_API: &str = "https://api.github.com/repos";
const DOTNET_9_RELEASES: &str =
    "https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/9.0/releases.json";

static INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
}

#[derive(Debug, Clone, Copy)]
struct GitHubPackage {
    id: &'static str,
    display_name: &'static str,
    capability: &'static str,
    repository: &'static str,
    asset_pattern: &'static str,
    entrypoint_file: &'static str,
    archive: ArchiveKind,
    dependency: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
struct StaticPackage {
    id: &'static str,
    display_name: &'static str,
    capability: &'static str,
    source: &'static str,
    version: &'static str,
    url: &'static str,
    asset_name: &'static str,
    asset_size: u64,
    archive_sha256: &'static str,
    entrypoint_file: &'static str,
    archive: ArchiveKind,
    integration: &'static str,
    used_by: &'static str,
}

const GITHUB_PACKAGES: &[GitHubPackage] = &[
    GitHubPackage {
        id: "depot-downloader-mod",
        display_name: "DepotDownloaderMod",
        capability: "Download or switch an exact Steam build",
        repository: "SteamAutoCracks/DepotDownloaderMod",
        asset_pattern: r"(?i)^Release\.rar$",
        entrypoint_file: "DepotDownloaderMod.exe",
        archive: ArchiveKind::Rar,
        dependency: Some("dotnet-runtime-9"),
    },
    GitHubPackage {
        id: "smoke-api",
        display_name: "SmokeAPI",
        capability: "Steam DLC unlocker payload",
        repository: "acidicoala/SmokeAPI",
        asset_pattern: r"(?i)^SmokeAPI-.+\.zip$",
        entrypoint_file: "smoke_api64.dll",
        archive: ArchiveKind::Zip,
        dependency: None,
    },
    GitHubPackage {
        id: "uplay-r1-unlocker",
        display_name: "Uplay R1 Unlocker",
        capability: "Legacy Ubisoft DLC unlocker payload",
        repository: "acidicoala/UplayR1Unlocker",
        asset_pattern: r"(?i)^UplayR1Unlocker-.+\.zip$",
        entrypoint_file: "uplay_r1_loader64.dll",
        archive: ArchiveKind::Zip,
        dependency: None,
    },
    GitHubPackage {
        id: "uplay-r2-unlocker",
        display_name: "Uplay R2 Unlocker",
        capability: "Ubisoft Connect DLC unlocker payload",
        repository: "acidicoala/UplayR2Unlocker",
        asset_pattern: r"(?i)^UplayR2Unlocker-.+\.zip$",
        entrypoint_file: "upc_r2_loader64.dll",
        archive: ArchiveKind::Zip,
        dependency: None,
    },
    GitHubPackage {
        id: "gbe-fork",
        display_name: "gbe_fork",
        capability: "Goldberg Steam emulator tools",
        repository: "Detanup01/gbe_fork",
        asset_pattern: r"(?i)^emu-win-release-vs22\.7z$",
        entrypoint_file: "steam_api64.dll",
        archive: ArchiveKind::SevenZip,
        dependency: None,
    },
    GitHubPackage {
        id: "steam-auto-crack",
        display_name: "SteamAutoCrack",
        capability: "Steamless and Goldberg desktop automation",
        repository: "SteamAutoCracks/Steam-auto-crack",
        asset_pattern: r"(?i)^SteamAutoCrack\.zip$",
        entrypoint_file: "SteamAutoCrack.exe",
        archive: ArchiveKind::Zip,
        dependency: None,
    },
];

const STATIC_PACKAGES: &[StaticPackage] = &[StaticPackage {
    id: "seven-zip-cli",
    display_name: "7-Zip Console",
    capability: "Extract split ZIP, 7z and RAR compatibility packages",
    source: "ip7z/7zip",
    version: "26.02",
    url: "https://github.com/ip7z/7zip/releases/download/26.02/7z2602-extra.7z",
    asset_name: "7z2602-extra.7z",
    asset_size: 1_758_916,
    archive_sha256: "081df9e9311dfd9c9e0e98c1c80180b99bb51e4cb24156b5f3057fe3c259d70a",
    entrypoint_file: "x64/7za.exe",
    archive: ArchiveKind::SevenZip,
    integration: "dependency",
    used_by: "Project Lightning compatibility packages",
}];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePackageStatus {
    pub id: String,
    pub display_name: String,
    pub capability: String,
    pub source: String,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub entrypoint: Option<String>,
    pub built_in: bool,
    pub integration: String,
    pub used_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledPackageState {
    schema: u32,
    id: String,
    version: String,
    source: String,
    asset_name: String,
    archive_sha256: String,
    entrypoint: String,
    installed_at: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DotNetReleaseIndex {
    #[serde(rename = "latest-runtime")]
    latest_runtime: String,
    releases: Vec<DotNetRelease>,
}

#[derive(Debug, Deserialize)]
struct DotNetRelease {
    #[serde(rename = "release-version")]
    release_version: String,
    runtime: DotNetRuntime,
}

#[derive(Debug, Deserialize)]
struct DotNetRuntime {
    files: Vec<DotNetFile>,
}

#[derive(Debug, Deserialize)]
struct DotNetFile {
    name: String,
    rid: String,
    url: String,
    hash: String,
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(15 * 60))
        .user_agent("0xoLemon-Launcher/2 feature-package-manager")
        .build()
        .map_err(|error| format!("PACKAGE_HTTP_CLIENT_FAILED: {error}"))
}

fn launcher_package_root() -> Result<PathBuf, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("PACKAGE_ROOT_UNAVAILABLE: {error}"))?;
    let install_dir = executable
        .parent()
        .ok_or_else(|| "PACKAGE_ROOT_UNAVAILABLE: launcher has no parent directory".to_string())?;
    let preferred = install_dir.join("feature-packages");
    match verify_package_root_writable(&preferred) {
        Ok(()) => Ok(preferred),
        Err(preferred_error) => {
            let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
                format!(
                    "PACKAGE_ROOT_NOT_WRITABLE: {} ({preferred_error}); LOCALAPPDATA is unavailable",
                    preferred.display()
                )
            })?;
            let fallback = PathBuf::from(local_app_data)
                .join("com.oxolemon.launcher")
                .join("feature-packages");
            verify_package_root_writable(&fallback).map_err(|fallback_error| {
                format!(
                    "PACKAGE_ROOT_NOT_WRITABLE: {} ({preferred_error}); fallback {} ({fallback_error})",
                    preferred.display(),
                    fallback.display()
                )
            })?;
            Ok(fallback)
        }
    }
}

fn verify_package_root_writable(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let probe = root.join(format!(".write-probe-{}", Uuid::new_v4()));
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| error.to_string())?;
    let write_result = file
        .write_all(b"0xoLemon feature package write probe")
        .and_then(|_| file.sync_data())
        .map_err(|error| error.to_string());
    drop(file);
    let cleanup_result = fs::remove_file(&probe).map_err(|error| error.to_string());
    write_result?;
    cleanup_result
}

fn package_descriptor(id: &str) -> Option<&'static GitHubPackage> {
    GITHUB_PACKAGES.iter().find(|package| package.id == id)
}

fn static_package_descriptor(id: &str) -> Option<&'static StaticPackage> {
    STATIC_PACKAGES.iter().find(|package| package.id == id)
}

fn package_integration(id: &str) -> &'static str {
    if id == "depot-downloader-mod" {
        "automatic"
    } else {
        "component"
    }
}

fn package_current_state(root: &Path, id: &str) -> Option<(InstalledPackageState, PathBuf)> {
    let state_path = root.join(id).join("current.json");
    let state: InstalledPackageState = serde_json::from_slice(&fs::read(state_path).ok()?).ok()?;
    if state.schema != 1 || state.id != id {
        return None;
    }
    let version_dir = root.join(id).join(&state.version);
    let relative = safe_relative_path(&state.entrypoint).ok()?;
    let entrypoint = version_dir.join(relative);
    if !entrypoint.is_file() {
        return None;
    }
    Some((state, entrypoint))
}

pub fn resolve_installed_entrypoint(id: &str) -> Option<PathBuf> {
    let root = launcher_package_root().ok()?;
    package_current_state(&root, id).map(|(_, entrypoint)| entrypoint)
}

pub fn resolve_dotnet_root() -> Option<PathBuf> {
    resolve_installed_entrypoint("dotnet-runtime-9")
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

#[tauri::command]
pub async fn list_sff_feature_packages() -> Result<Vec<FeaturePackageStatus>, String> {
    tauri::async_runtime::spawn_blocking(list_sff_feature_packages_blocking)
        .await
        .map_err(|error| format!("PACKAGE_TASK_FAILED: {error}"))?
}

fn list_sff_feature_packages_blocking() -> Result<Vec<FeaturePackageStatus>, String> {
    let root = launcher_package_root()?;
    let mut result = vec![
        FeaturePackageStatus {
            id: "cloud-redirect".to_string(),
            display_name: "CloudRedirect".to_string(),
            capability: "Steam cloud save redirection and synchronization".to_string(),
            source: "dangjimmy33-dotcom/CloudRedirect".to_string(),
            installed: true,
            installed_version: Some(crate::cloud_redirect_v2::ENGINE_VERSION.to_string()),
            entrypoint: None,
            built_in: true,
            integration: "builtIn".to_string(),
            used_by: Some("Cloud Saves".to_string()),
        },
        FeaturePackageStatus {
            id: "steamless".to_string(),
            display_name: "Steamless".to_string(),
            capability: "SteamStub analysis and removal".to_string(),
            source: "0xoLemon native implementation".to_string(),
            installed: true,
            installed_version: None,
            entrypoint: None,
            built_in: true,
            integration: "builtIn".to_string(),
            used_by: Some("Library > game tools".to_string()),
        },
        FeaturePackageStatus {
            id: "native-steam-core".to_string(),
            display_name: "0xoLemon Core Native".to_string(),
            capability: "Lua ownership, manifest pinning, hot reload, OnlineFix routing and Steam integration".to_string(),
            source: "0xoLemon native core, maintained against LumaCore and BetterSteamTools".to_string(),
            installed: true,
            installed_version: None,
            entrypoint: None,
            built_in: true,
            integration: "builtIn".to_string(),
            used_by: Some("Lua Shop and Steam runtime".to_string()),
        },
        FeaturePackageStatus {
            id: "lua-source-manager".to_string(),
            display_name: "Lua and manifest source manager".to_string(),
            capability: "Steam catalog, Live/Locked channels and provider-scoped Lua updates".to_string(),
            source: "Steam catalog with Hubcap, Hugging Face, Sushi and opt-in Ryuu providers".to_string(),
            installed: true,
            installed_version: None,
            entrypoint: None,
            built_in: true,
            integration: "builtIn".to_string(),
            used_by: Some("Lua Shop".to_string()),
        },
        FeaturePackageStatus {
            id: "dotnet-runtime-9".to_string(),
            display_name: ".NET 9 Runtime".to_string(),
            capability: "Portable runtime for DepotDownloaderMod".to_string(),
            source: "Microsoft".to_string(),
            installed: false,
            installed_version: None,
            entrypoint: None,
            built_in: false,
            integration: "dependency".to_string(),
            used_by: Some("DepotDownloaderMod".to_string()),
        },
    ];

    for package in GITHUB_PACKAGES {
        let current = package_current_state(&root, package.id);
        result.push(FeaturePackageStatus {
            id: package.id.to_string(),
            display_name: package.display_name.to_string(),
            capability: package.capability.to_string(),
            source: package.repository.to_string(),
            installed: current.is_some(),
            installed_version: current.as_ref().map(|(state, _)| state.version.clone()),
            entrypoint: current
                .as_ref()
                .map(|(_, path)| path.to_string_lossy().to_string()),
            built_in: false,
            integration: package_integration(package.id).to_string(),
            used_by: Some(
                match package.id {
                    "depot-downloader-mod" => "Version locked / exact BuildID",
                    "smoke-api" | "uplay-r1-unlocker" | "uplay-r2-unlocker" => {
                        "Optional per-game DLC tooling"
                    }
                    "gbe-fork" | "steam-auto-crack" => "Optional per-game compatibility tooling",
                    _ => "Optional feature",
                }
                .to_string(),
            ),
        });
    }

    for package in STATIC_PACKAGES {
        let current = package_current_state(&root, package.id);
        result.push(FeaturePackageStatus {
            id: package.id.to_string(),
            display_name: package.display_name.to_string(),
            capability: package.capability.to_string(),
            source: package.source.to_string(),
            installed: current.is_some(),
            installed_version: current.as_ref().map(|(state, _)| state.version.clone()),
            entrypoint: current
                .as_ref()
                .map(|(_, path)| path.to_string_lossy().to_string()),
            built_in: false,
            integration: package.integration.to_string(),
            used_by: Some(package.used_by.to_string()),
        });
    }

    if let Some(status) = result
        .iter_mut()
        .find(|status| status.id == "dotnet-runtime-9")
    {
        if let Some((state, entrypoint)) = package_current_state(&root, "dotnet-runtime-9") {
            status.installed = true;
            status.installed_version = Some(state.version);
            status.entrypoint = Some(entrypoint.to_string_lossy().to_string());
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn install_sff_feature_package(
    package_id: String,
    force_update: Option<bool>,
) -> Result<FeaturePackageStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        install_feature_package_blocking(&package_id, force_update.unwrap_or(false))?;
        list_sff_feature_packages_blocking()?
            .into_iter()
            .find(|status| status.id == package_id)
            .ok_or_else(|| "PACKAGE_UNKNOWN".to_string())
    })
    .await
    .map_err(|error| format!("PACKAGE_TASK_FAILED: {error}"))?
}

pub fn ensure_feature_package(id: &str) -> Result<PathBuf, String> {
    if let Some(path) = resolve_installed_entrypoint(id) {
        return Ok(path);
    }
    install_feature_package_blocking(id, false)
}

fn install_feature_package_blocking(id: &str, force_update: bool) -> Result<PathBuf, String> {
    let lock = INSTALL_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "PACKAGE_INSTALL_LOCK_POISONED".to_string())?;
    let root = launcher_package_root()?;
    if !force_update {
        if let Some((_, entrypoint)) = package_current_state(&root, id) {
            return Ok(entrypoint);
        }
    }

    if id == "dotnet-runtime-9" {
        return install_dotnet_runtime(&root);
    }
    if let Some(package) = static_package_descriptor(id) {
        return install_static_package(&root, package);
    }
    let package = package_descriptor(id).ok_or_else(|| "PACKAGE_UNKNOWN".to_string())?;
    if let Some(dependency) = package.dependency {
        if package_current_state(&root, dependency).is_none() {
            install_dotnet_runtime(&root)?;
        }
    }
    install_github_package(&root, package)
}

fn install_static_package(root: &Path, package: &StaticPackage) -> Result<PathBuf, String> {
    if package.asset_size == 0 || package.asset_size > MAX_PACKAGE_DOWNLOAD_BYTES {
        return Err("PACKAGE_ASSET_SIZE_INVALID".to_string());
    }
    let version = safe_version_component(package.version)?;
    let package_dir = root.join(package.id);
    fs::create_dir_all(&package_dir)
        .map_err(|error| format!("PACKAGE_DIRECTORY_FAILED: {error}"))?;
    let final_dir = package_dir.join(&version);
    let expected_relative = safe_relative_path(package.entrypoint_file)?;
    let existing_entrypoint = final_dir.join(&expected_relative);
    if existing_entrypoint.is_file() {
        let entrypoint = existing_entrypoint;
        let state = load_installed_package_state(&final_dir).unwrap_or_else(|| {
            package_state_for_existing_version(
                package.id,
                &version,
                package.source,
                package.asset_name,
                &entrypoint,
                &final_dir,
            )
        });
        if state
            .archive_sha256
            .eq_ignore_ascii_case(package.archive_sha256)
        {
            write_json_durable(&package_dir.join("current.json"), &state)?;
            return Ok(entrypoint);
        }
        return Err("PACKAGE_VERSION_HASH_CONFLICT".to_string());
    }

    let download_dir = root.join(".downloads");
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("PACKAGE_DOWNLOAD_DIRECTORY_FAILED: {error}"))?;
    let archive_path = download_dir.join(format!("{}.part", Uuid::new_v4()));
    let client = http_client()?;
    let (actual_sha256, _) = download_url(
        &client,
        package.url,
        &archive_path,
        Some(package.asset_size),
    )?;
    if !actual_sha256.eq_ignore_ascii_case(package.archive_sha256) {
        let _ = fs::remove_file(&archive_path);
        return Err("PACKAGE_ARCHIVE_HASH_MISMATCH".to_string());
    }

    let staging = package_dir.join(format!(".staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|error| format!("PACKAGE_STAGING_FAILED: {error}"))?;
    extract_archive(&archive_path, &staging, package.archive)?;
    validate_extracted_tree(&staging)?;
    let staged_entrypoint = staging.join(&expected_relative);
    if !staged_entrypoint.is_file() {
        return Err("PACKAGE_ENTRYPOINT_NOT_FOUND".to_string());
    }
    let relative_entrypoint = staged_entrypoint
        .strip_prefix(&staging)
        .map_err(|_| "PACKAGE_ENTRYPOINT_OUTSIDE_STAGING".to_string())?
        .to_path_buf();
    let state = InstalledPackageState {
        schema: 1,
        id: package.id.to_string(),
        version: version.clone(),
        source: package.source.to_string(),
        asset_name: package.asset_name.to_string(),
        archive_sha256: actual_sha256,
        entrypoint: path_to_portable_string(&relative_entrypoint),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    write_json_durable(&staging.join("package.json"), &state)?;
    if final_dir.exists() {
        let entrypoint = final_dir.join(&expected_relative);
        return entrypoint
            .is_file()
            .then_some(entrypoint)
            .ok_or_else(|| "PACKAGE_VERSION_CONFLICT".to_string());
    }
    fs::rename(&staging, &final_dir).map_err(|error| format!("PACKAGE_COMMIT_FAILED: {error}"))?;
    let entrypoint = final_dir.join(&relative_entrypoint);
    write_json_durable(&package_dir.join("current.json"), &state)?;
    let _ = fs::remove_file(&archive_path);
    Ok(entrypoint)
}

fn install_github_package(root: &Path, package: &GitHubPackage) -> Result<PathBuf, String> {
    let client = http_client()?;
    let release: GitHubRelease = client
        .get(format!(
            "{GITHUB_API}/{}/releases/latest",
            package.repository
        ))
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("PACKAGE_RELEASE_LOOKUP_FAILED: {error}"))?
        .json()
        .map_err(|error| format!("PACKAGE_RELEASE_INVALID: {error}"))?;
    let selector = regex::Regex::new(package.asset_pattern)
        .map_err(|error| format!("PACKAGE_CATALOG_INVALID: {error}"))?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| selector.is_match(&asset.name))
        .ok_or_else(|| "PACKAGE_ASSET_NOT_FOUND".to_string())?;
    if asset.size == 0 || asset.size > MAX_PACKAGE_DOWNLOAD_BYTES {
        return Err("PACKAGE_ASSET_SIZE_INVALID".to_string());
    }
    let version = safe_version_component(&release.tag_name)?;
    let package_dir = root.join(package.id);
    fs::create_dir_all(&package_dir)
        .map_err(|error| format!("PACKAGE_DIRECTORY_FAILED: {error}"))?;
    let final_dir = package_dir.join(&version);
    if let Some(entrypoint) = valid_version_entrypoint(&final_dir, package.entrypoint_file) {
        let state = load_installed_package_state(&final_dir).unwrap_or_else(|| {
            package_state_for_existing_version(
                package.id,
                &version,
                package.repository,
                &asset.name,
                &entrypoint,
                &final_dir,
            )
        });
        write_json_durable(&package_dir.join("current.json"), &state)?;
        return Ok(entrypoint);
    }

    let download_dir = root.join(".downloads");
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("PACKAGE_DOWNLOAD_DIRECTORY_FAILED: {error}"))?;
    let archive_path = download_dir.join(format!("{}.part", Uuid::new_v4()));
    let actual_sha256 = download_asset(&client, &asset, &archive_path)?;
    if let Some(expected) = asset
        .digest
        .as_deref()
        .and_then(|value| value.strip_prefix("sha256:"))
    {
        if !actual_sha256.eq_ignore_ascii_case(expected) {
            return Err("PACKAGE_ARCHIVE_HASH_MISMATCH".to_string());
        }
    }

    let staging = package_dir.join(format!(".staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|error| format!("PACKAGE_STAGING_FAILED: {error}"))?;
    extract_archive(&archive_path, &staging, package.archive)?;
    validate_extracted_tree(&staging)?;
    let staged_entrypoint = find_entrypoint(&staging, package.entrypoint_file)
        .ok_or_else(|| "PACKAGE_ENTRYPOINT_NOT_FOUND".to_string())?;
    let relative_entrypoint = staged_entrypoint
        .strip_prefix(&staging)
        .map_err(|_| "PACKAGE_ENTRYPOINT_OUTSIDE_STAGING".to_string())?
        .to_path_buf();
    let state = InstalledPackageState {
        schema: 1,
        id: package.id.to_string(),
        version: version.clone(),
        source: package.repository.to_string(),
        asset_name: asset.name.clone(),
        archive_sha256: actual_sha256.clone(),
        entrypoint: path_to_portable_string(&relative_entrypoint),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    write_json_durable(&staging.join("package.json"), &state)?;
    if final_dir.exists() {
        return valid_version_entrypoint(&final_dir, package.entrypoint_file)
            .ok_or_else(|| "PACKAGE_VERSION_CONFLICT".to_string());
    }
    fs::rename(&staging, &final_dir).map_err(|error| format!("PACKAGE_COMMIT_FAILED: {error}"))?;
    let entrypoint = final_dir.join(&relative_entrypoint);
    write_json_durable(&package_dir.join("current.json"), &state)?;
    let _ = fs::remove_file(&archive_path);
    Ok(entrypoint)
}

fn install_dotnet_runtime(root: &Path) -> Result<PathBuf, String> {
    if let Some((_, entrypoint)) = package_current_state(root, "dotnet-runtime-9") {
        return Ok(entrypoint);
    }
    let client = http_client()?;
    let index: DotNetReleaseIndex = client
        .get(DOTNET_9_RELEASES)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("DOTNET_RELEASE_LOOKUP_FAILED: {error}"))?
        .json()
        .map_err(|error| format!("DOTNET_RELEASE_INVALID: {error}"))?;
    let release = index
        .releases
        .into_iter()
        .find(|release| release.release_version == index.latest_runtime)
        .ok_or_else(|| "DOTNET_RELEASE_NOT_FOUND".to_string())?;
    let asset = release
        .runtime
        .files
        .into_iter()
        .find(|file| file.rid == "win-x64" && file.name == "dotnet-runtime-win-x64.zip")
        .ok_or_else(|| "DOTNET_ASSET_NOT_FOUND".to_string())?;
    let version = safe_version_component(&release.release_version)?;
    let package_dir = root.join("dotnet-runtime-9");
    fs::create_dir_all(&package_dir)
        .map_err(|error| format!("PACKAGE_DIRECTORY_FAILED: {error}"))?;
    let final_dir = package_dir.join(&version);
    if final_dir.join("dotnet.exe").is_file() {
        let entrypoint = final_dir.join("dotnet.exe");
        let state = load_installed_package_state(&final_dir).unwrap_or_else(|| {
            package_state_for_existing_version(
                "dotnet-runtime-9",
                &version,
                "Microsoft .NET release metadata",
                &asset.name,
                &entrypoint,
                &final_dir,
            )
        });
        write_json_durable(&package_dir.join("current.json"), &state)?;
        return Ok(entrypoint);
    }
    let download_dir = root.join(".downloads");
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("PACKAGE_DOWNLOAD_DIRECTORY_FAILED: {error}"))?;
    let archive_path = download_dir.join(format!("{}.part", Uuid::new_v4()));
    let (sha256, sha512) = download_url(&client, &asset.url, &archive_path, None)?;
    if !sha512.eq_ignore_ascii_case(&asset.hash) {
        return Err("DOTNET_ARCHIVE_HASH_MISMATCH".to_string());
    }
    let staging = package_dir.join(format!(".staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|error| format!("PACKAGE_STAGING_FAILED: {error}"))?;
    extract_zip(&archive_path, &staging)?;
    validate_extracted_tree(&staging)?;
    if !staging.join("dotnet.exe").is_file() {
        return Err("DOTNET_ENTRYPOINT_NOT_FOUND".to_string());
    }
    let state = InstalledPackageState {
        schema: 1,
        id: "dotnet-runtime-9".to_string(),
        version: version.clone(),
        source: "Microsoft .NET release metadata".to_string(),
        asset_name: asset.name,
        archive_sha256: sha256,
        entrypoint: "dotnet.exe".to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    write_json_durable(&staging.join("package.json"), &state)?;
    if final_dir.exists() {
        return if final_dir.join("dotnet.exe").is_file() {
            Ok(final_dir.join("dotnet.exe"))
        } else {
            Err("PACKAGE_VERSION_CONFLICT".to_string())
        };
    }
    fs::rename(&staging, &final_dir).map_err(|error| format!("PACKAGE_COMMIT_FAILED: {error}"))?;
    write_json_durable(&package_dir.join("current.json"), &state)?;
    let _ = fs::remove_file(&archive_path);
    Ok(final_dir.join("dotnet.exe"))
}

fn download_asset(
    client: &Client,
    asset: &GitHubAsset,
    destination: &Path,
) -> Result<String, String> {
    let (sha256, _) = download_url(
        client,
        &asset.browser_download_url,
        destination,
        Some(asset.size),
    )?;
    Ok(sha256)
}

fn download_url(
    client: &Client,
    url: &str,
    destination: &Path,
    expected_size: Option<u64>,
) -> Result<(String, String), String> {
    let parsed = url::Url::parse(url).map_err(|error| format!("PACKAGE_URL_INVALID: {error}"))?;
    let trusted = matches!(
        parsed.host_str(),
        Some(
            "github.com"
                | "objects.githubusercontent.com"
                | "release-assets.githubusercontent.com"
                | "builds.dotnet.microsoft.com"
                | "dotnetcli.blob.core.windows.net"
        )
    );
    if parsed.scheme() != "https" || !trusted {
        return Err("PACKAGE_URL_UNTRUSTED".to_string());
    }
    let mut response = client
        .get(parsed)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("PACKAGE_DOWNLOAD_FAILED: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PACKAGE_DOWNLOAD_BYTES)
    {
        return Err("PACKAGE_DOWNLOAD_TOO_LARGE".to_string());
    }
    let mut output = File::create(destination)
        .map_err(|error| format!("PACKAGE_DOWNLOAD_CREATE_FAILED: {error}"))?;
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("PACKAGE_DOWNLOAD_READ_FAILED: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_PACKAGE_DOWNLOAD_BYTES {
            return Err("PACKAGE_DOWNLOAD_TOO_LARGE".to_string());
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("PACKAGE_DOWNLOAD_WRITE_FAILED: {error}"))?;
        sha256.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    output
        .sync_all()
        .map_err(|error| format!("PACKAGE_DOWNLOAD_FLUSH_FAILED: {error}"))?;
    if expected_size.is_some_and(|expected| expected != total) || total == 0 {
        return Err("PACKAGE_DOWNLOAD_SIZE_MISMATCH".to_string());
    }
    Ok((
        hex::encode(sha256.finalize()),
        hex::encode(sha512.finalize()),
    ))
}

fn extract_archive(archive: &Path, destination: &Path, kind: ArchiveKind) -> Result<(), String> {
    match kind {
        ArchiveKind::Zip => extract_zip(archive, destination),
        ArchiveKind::SevenZip => extract_seven_zip(archive, destination),
        ArchiveKind::Rar => extract_rar_with_system_tar(archive, destination),
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("PACKAGE_ARCHIVE_OPEN_FAILED: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("PACKAGE_ARCHIVE_INVALID: {error}"))?;
    let mut names = HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("PACKAGE_ARCHIVE_ENTRY_FAILED: {error}"))?;
        let relative = safe_relative_path(entry.name())?;
        let key = path_to_portable_string(&relative).to_ascii_lowercase();
        if !names.insert(key) {
            return Err("PACKAGE_ARCHIVE_DUPLICATE_PATH".to_string());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("PACKAGE_ARCHIVE_SYMLINK_REJECTED".to_string());
        }
        if entry.is_dir() {
            fs::create_dir_all(destination.join(relative))
                .map_err(|error| format!("PACKAGE_EXTRACT_DIRECTORY_FAILED: {error}"))?;
            continue;
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_PACKAGE_EXPANDED_BYTES {
            return Err("PACKAGE_EXPANDED_SIZE_EXCEEDED".to_string());
        }
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("PACKAGE_EXTRACT_DIRECTORY_FAILED: {error}"))?;
        }
        let mut output = File::create(&target)
            .map_err(|error| format!("PACKAGE_EXTRACT_FILE_FAILED: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("PACKAGE_EXTRACT_WRITE_FAILED: {error}"))?;
    }
    Ok(())
}

fn extract_seven_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("PACKAGE_ARCHIVE_OPEN_FAILED: {error}"))?;
    let mut names = HashSet::new();
    let mut expanded = 0_u64;
    sevenz_rust::decompress_with_extract_fn_and_password(
        file,
        destination,
        sevenz_rust::Password::empty(),
        |entry, reader, _| {
            let relative = safe_relative_path(entry.name()).map_err(sevenz_rust::Error::other)?;
            let key = path_to_portable_string(&relative).to_ascii_lowercase();
            if !names.insert(key) {
                return Err(sevenz_rust::Error::other("duplicate archive path"));
            }
            if entry.is_directory() {
                fs::create_dir_all(destination.join(relative)).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            }
            expanded = expanded.saturating_add(entry.size());
            if expanded > MAX_PACKAGE_EXPANDED_BYTES {
                return Err(sevenz_rust::Error::other("expanded size exceeded"));
            }
            let target = destination.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
            }
            let mut output = File::create(target).map_err(sevenz_rust::Error::io)?;
            std::io::copy(reader, &mut output).map_err(sevenz_rust::Error::io)?;
            Ok(true)
        },
    )
    .map_err(|error| format!("PACKAGE_ARCHIVE_INVALID: {error}"))?;
    Ok(())
}

fn extract_rar_with_system_tar(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let listing = Command::new("tar.exe")
        .arg("-tvf")
        .arg(archive_path)
        .output()
        .map_err(|error| format!("PACKAGE_RAR_READER_UNAVAILABLE: {error}"))?;
    if !listing.status.success() {
        return Err("PACKAGE_RAR_LIST_FAILED".to_string());
    }
    let listing = String::from_utf8(listing.stdout)
        .map_err(|_| "PACKAGE_RAR_LIST_INVALID_UTF8".to_string())?;
    let mut names = HashSet::new();
    let mut expanded = 0_u64;
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let permissions = line.as_bytes().first().copied().unwrap_or_default();
        if permissions == b'l' || permissions == b'h' || line.contains(" -> ") {
            return Err("PACKAGE_ARCHIVE_LINK_REJECTED".to_string());
        }
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() < 9 {
            return Err("PACKAGE_RAR_LIST_INVALID".to_string());
        }
        let size = columns[4]
            .parse::<u64>()
            .map_err(|_| "PACKAGE_RAR_SIZE_INVALID".to_string())?;
        expanded = expanded.saturating_add(size);
        if expanded > MAX_PACKAGE_EXPANDED_BYTES {
            return Err("PACKAGE_EXPANDED_SIZE_EXCEEDED".to_string());
        }
        let path = columns[8..].join(" ");
        let relative = safe_relative_path(&path)?;
        if !names.insert(path_to_portable_string(&relative).to_ascii_lowercase()) {
            return Err("PACKAGE_ARCHIVE_DUPLICATE_PATH".to_string());
        }
    }
    let status = Command::new("tar.exe")
        .arg("-xf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination)
        .status()
        .map_err(|error| format!("PACKAGE_RAR_EXTRACT_FAILED: {error}"))?;
    if !status.success() {
        return Err("PACKAGE_RAR_EXTRACT_FAILED".to_string());
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains(':') {
        return Err("PACKAGE_ARCHIVE_UNSAFE_PATH".to_string());
    }
    let mut result = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || is_windows_device_name(segment)
        {
            return Err("PACKAGE_ARCHIVE_UNSAFE_PATH".to_string());
        }
        result.push(segment);
    }
    if result
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("PACKAGE_ARCHIVE_UNSAFE_PATH".to_string());
    }
    Ok(result)
}

fn is_windows_device_name(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn validate_extracted_tree(root: &Path) -> Result<(), String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("PACKAGE_STAGING_CANONICALIZE_FAILED: {error}"))?;
    let mut total = 0_u64;
    for item in walkdir::WalkDir::new(root).follow_links(false) {
        let item = item.map_err(|error| format!("PACKAGE_EXTRACTED_TREE_INVALID: {error}"))?;
        let metadata = fs::symlink_metadata(item.path())
            .map_err(|error| format!("PACKAGE_EXTRACTED_METADATA_FAILED: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("PACKAGE_EXTRACTED_SYMLINK_REJECTED".to_string());
        }
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
            if total > MAX_PACKAGE_EXPANDED_BYTES {
                return Err("PACKAGE_EXPANDED_SIZE_EXCEEDED".to_string());
            }
        }
        let canonical = fs::canonicalize(item.path())
            .map_err(|error| format!("PACKAGE_EXTRACTED_CANONICALIZE_FAILED: {error}"))?;
        if !canonical.starts_with(&canonical_root) {
            return Err("PACKAGE_EXTRACTED_PATH_ESCAPE".to_string());
        }
    }
    Ok(())
}

fn find_entrypoint(root: &Path, file_name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .find(|item| {
            item.file_type().is_file()
                && item
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(file_name)
        })
        .map(|item| item.into_path())
}

fn valid_version_entrypoint(version_dir: &Path, file_name: &str) -> Option<PathBuf> {
    if !version_dir.is_dir() {
        return None;
    }
    find_entrypoint(version_dir, file_name)
}

fn safe_version_component(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("PACKAGE_VERSION_INVALID".to_string());
    }
    Ok(value.to_string())
}

fn path_to_portable_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn package_state_for_existing_version(
    id: &str,
    version: &str,
    source: &str,
    asset_name: &str,
    entrypoint: &Path,
    version_dir: &Path,
) -> InstalledPackageState {
    let relative = entrypoint.strip_prefix(version_dir).unwrap_or(entrypoint);
    InstalledPackageState {
        schema: 1,
        id: id.to_string(),
        version: version.to_string(),
        source: source.to_string(),
        asset_name: asset_name.to_string(),
        archive_sha256: String::new(),
        entrypoint: path_to_portable_string(relative),
        installed_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn load_installed_package_state(version_dir: &Path) -> Option<InstalledPackageState> {
    let bytes = fs::read(version_dir.join("package.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json_durable<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("PACKAGE_STATE_DIRECTORY_FAILED: {error}"))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("PACKAGE_STATE_SERIALIZE_FAILED: {error}"))?;
    let mut file = File::create(&temporary)
        .map_err(|error| format!("PACKAGE_STATE_CREATE_FAILED: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("PACKAGE_STATE_WRITE_FAILED: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("PACKAGE_STATE_FLUSH_FAILED: {error}"))?;
    drop(file);

    if !path.exists() {
        return fs::rename(&temporary, path)
            .map_err(|error| format!("PACKAGE_STATE_COMMIT_FAILED: {error}"));
    }

    let backup = path.with_extension(format!("backup-{}", Uuid::new_v4()));
    fs::rename(path, &backup).map_err(|error| format!("PACKAGE_STATE_BACKUP_FAILED: {error}"))?;
    match fs::rename(&temporary, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup);
            Ok(())
        }
        Err(error) => {
            let restore_result = fs::rename(&backup, path);
            let _ = fs::remove_file(&temporary);
            match restore_result {
                Ok(()) => Err(format!("PACKAGE_STATE_COMMIT_FAILED: {error}")),
                Err(restore_error) => Err(format!(
                    "PACKAGE_STATE_COMMIT_FAILED: {error}; PACKAGE_STATE_RESTORE_FAILED: {restore_error}"
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_archive_paths() {
        for value in [
            "../escape",
            "/absolute",
            "C:/escape",
            "safe/../escape",
            "safe/NUL.txt",
        ] {
            assert!(safe_relative_path(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn accepts_nested_package_paths() {
        assert_eq!(
            safe_relative_path("Release/net9.0/DepotDownloaderMod.exe").unwrap(),
            PathBuf::from("Release/net9.0/DepotDownloaderMod.exe")
        );
    }

    #[test]
    fn durable_json_replaces_existing_state() {
        let root = std::env::temp_dir().join(format!("sff-state-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let state_path = root.join("current.json");

        write_json_durable(&state_path, &serde_json::json!({ "version": "1" })).unwrap();
        write_json_durable(&state_path, &serde_json::json!({ "version": "2" })).unwrap();

        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(state["version"], "2");
        fs::remove_file(&state_path).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn writable_package_root_probe_cleans_up_after_itself() {
        let root = std::env::temp_dir().join(format!("sff-root-test-{}", Uuid::new_v4()));
        verify_package_root_writable(&root).unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn package_ids_are_unique_and_selectors_compile() {
        let mut ids = HashSet::new();
        for package in GITHUB_PACKAGES {
            assert!(ids.insert(package.id));
            assert!(regex::Regex::new(package.asset_pattern).is_ok());
        }
        for package in STATIC_PACKAGES {
            assert!(ids.insert(package.id));
            assert_eq!(package.archive_sha256.len(), 64);
            assert!(package
                .archive_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()));
            assert!(safe_relative_path(package.entrypoint_file).is_ok());
        }
    }

    #[test]
    fn lightning_split_archive_dependency_is_pinned() {
        let package = static_package_descriptor("seven-zip-cli").unwrap();
        assert_eq!(package.version, "26.02");
        assert_eq!(package.asset_size, 1_758_916);
        assert_eq!(package.entrypoint_file, "x64/7za.exe");
        assert_eq!(package.integration, "dependency");
    }

    #[test]
    fn only_exact_build_downloader_is_automatic() {
        for package in GITHUB_PACKAGES {
            let expected = if package.id == "depot-downloader-mod" {
                "automatic"
            } else {
                "component"
            };
            assert_eq!(package_integration(package.id), expected, "{}", package.id);
        }
    }

    #[test]
    fn version_component_rejects_path_syntax() {
        assert!(safe_version_component("3.4.0").is_ok());
        assert!(safe_version_component("../3.4.0").is_err());
        assert!(safe_version_component("3/4").is_err());
    }
}
