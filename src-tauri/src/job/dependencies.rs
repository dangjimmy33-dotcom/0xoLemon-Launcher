use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::launch::{
    expand_placeholders, is_script_path, process_path, process_working_directory, GameLaunchOption,
};

use super::{hidden_command, DepotSource, JobError};

const DEPENDENCY_BUNDLE_JSON: &str = include_str!("../../dependency-bundle.json");
const MAX_DEPENDENCY_DOWNLOAD_BYTES: u64 = 600 * 1024 * 1024;
const DOTNET_FRAMEWORK_481_RELEASE: u64 = 533_320;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DependencyBundle {
    schema: u32,
    #[serde(default)]
    default_dependencies: Vec<String>,
    #[serde(default)]
    game_profiles: HashMap<String, Vec<String>>,
    packages: Vec<DependencyBundlePackage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DependencyBundlePackage {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    display_name: String,
    url: String,
    download_file_name: String,
    bundled_path: String,
    download_sha256: String,
    payload_sha256: String,
    minimum_download_bytes: u64,
    minimum_payload_bytes: u64,
    #[serde(default)]
    signature_publisher: Option<String>,
    #[serde(default)]
    archive_kind: Option<String>,
    #[serde(default)]
    extracted_installer_path: Option<String>,
    #[serde(default)]
    extracted_installer_sha256: Option<String>,
    #[serde(default)]
    minimum_extracted_installer_bytes: Option<u64>,
    #[serde(default)]
    extracted_signature_publisher: Option<String>,
    installer_kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerKind {
    VcRedist,
    VcRedistLegacy,
    DirectXRedist,
    DotNetFramework,
    Msi,
    PhysXRedist,
    OpenAlRedist,
    EaApp,
}

impl InstallerKind {
    fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::VcRedist => &["/install", "/quiet", "/norestart"],
            Self::VcRedistLegacy => &["/q", "/norestart"],
            Self::DirectXRedist => &["/silent"],
            Self::DotNetFramework => &["/q", "/norestart"],
            Self::Msi => &[],
            Self::PhysXRedist => &["/quiet"],
            Self::OpenAlRedist => &["/S"],
            Self::EaApp => &["/install", "/quiet"],
        }
    }

    fn accepted_exit_codes(self) -> &'static [i32] {
        match self {
            Self::VcRedist | Self::VcRedistLegacy => &[0, 1638, 3010],
            Self::DotNetFramework => &[0, 1641, 3010],
            Self::Msi => &[0, 1641, 3010],
            Self::DirectXRedist | Self::PhysXRedist | Self::OpenAlRedist | Self::EaApp => &[0],
        }
    }
}

#[derive(Debug, Clone)]
struct DependencySpec {
    id: String,
    display_name: String,
    url: String,
    file_name: String,
    bundled_path: Option<String>,
    download_sha256: Option<String>,
    payload_sha256: Option<String>,
    minimum_download_bytes: u64,
    minimum_payload_bytes: u64,
    archive_kind: Option<String>,
    extracted_installer_path: Option<String>,
    extracted_installer_sha256: Option<String>,
    minimum_extracted_installer_bytes: Option<u64>,
    installer_kind: InstallerKind,
}

static DEPENDENCY_BUNDLE: OnceLock<Result<DependencyBundle, String>> = OnceLock::new();

fn dependency_bundle() -> Result<&'static DependencyBundle, JobError> {
    match DEPENDENCY_BUNDLE.get_or_init(|| {
        let bundle: DependencyBundle = serde_json::from_str(DEPENDENCY_BUNDLE_JSON)
            .map_err(|error| format!("invalid dependency-bundle.json: {error}"))?;
        validate_dependency_bundle(&bundle)?;
        Ok(bundle)
    }) {
        Ok(bundle) => Ok(bundle),
        Err(error) => Err(JobError::Depot(error.clone())),
    }
}

fn validate_dependency_bundle(bundle: &DependencyBundle) -> Result<(), String> {
    if bundle.schema != 1
        || bundle.packages.is_empty()
        || bundle.default_dependencies.is_empty()
        || bundle.game_profiles.is_empty()
    {
        return Err("dependency bundle schema is unsupported or empty".to_string());
    }

    let mut names = HashSet::new();
    for package in &bundle.packages {
        let id = normalize_dependency_id(&package.id);
        if id.is_empty() || !names.insert(id.clone()) {
            return Err(format!("duplicate or empty dependency id: {}", package.id));
        }
        for alias in &package.aliases {
            let alias = normalize_dependency_id(alias);
            if alias.is_empty() || !names.insert(alias.clone()) {
                return Err(format!("duplicate or empty dependency alias: {alias}"));
            }
        }

        let parsed_url = url::Url::parse(&package.url)
            .map_err(|error| format!("invalid dependency URL for {id}: {error}"))?;
        let trusted_host = matches!(
            parsed_url.host_str(),
            Some(
                "download.microsoft.com"
                    | "download.visualstudio.microsoft.com"
                    | "us.download.nvidia.com"
                    | "www.openal.org"
            )
        );
        if parsed_url.scheme() != "https" || !trusted_host {
            return Err(format!("untrusted dependency URL for {id}"));
        }
        validate_relative_bundle_path(&package.download_file_name)?;
        validate_relative_bundle_path(&package.bundled_path)?;
        if let Some(path) = package.extracted_installer_path.as_deref() {
            validate_relative_bundle_path(path)?;
        }
        validate_sha256(&package.download_sha256, &id)?;
        validate_sha256(&package.payload_sha256, &id)?;
        if let Some(hash) = package.extracted_installer_sha256.as_deref() {
            validate_sha256(hash, &id)?;
        }
        if package.display_name.trim().is_empty()
            || package.minimum_download_bytes == 0
            || package.minimum_payload_bytes == 0
        {
            return Err(format!("incomplete dependency metadata for {id}"));
        }
        match package.installer_kind.as_str() {
            "vc-redist" | "vc-redist-legacy" | "directx-redist" | "dotnet-framework" | "msi"
            | "physx-redist" | "openal-redist" => {}
            other => return Err(format!("unsupported installer kind for {id}: {other}")),
        }
        if matches!(
            package.installer_kind.as_str(),
            "directx-redist" | "openal-redist"
        ) && (package.extracted_installer_sha256.is_none()
            || package.extracted_installer_path.is_none()
            || package.minimum_extracted_installer_bytes.unwrap_or(0) == 0)
        {
            return Err(format!("extracted installer metadata is missing for {id}"));
        }
        if let Some(archive_kind) = package.archive_kind.as_deref() {
            if archive_kind != "zip" || package.installer_kind != "openal-redist" {
                return Err(format!("unsupported archive kind for {id}: {archive_kind}"));
            }
        }
        if package.archive_kind.is_some() && package.extracted_signature_publisher.is_none() {
            return Err(format!("archive signer metadata is missing for {id}"));
        }
        if package.archive_kind.is_none() && package.signature_publisher.is_none() {
            return Err(format!("payload signer metadata is missing for {id}"));
        }
    }

    for id in bundle.default_dependencies.iter().chain(
        bundle
            .game_profiles
            .values()
            .flat_map(|dependencies| dependencies.iter()),
    ) {
        let normalized = normalize_dependency_id(id);
        if !names.contains(&normalized) && normalized != "ea-app" {
            return Err(format!("profile references unknown dependency: {id}"));
        }
    }
    for (game_id, dependencies) in &bundle.game_profiles {
        if normalize_game_id(game_id).is_empty() || dependencies.is_empty() {
            return Err(format!("empty dependency profile for game: {game_id}"));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, dependency_id: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid SHA-256 for dependency {dependency_id}"))
    }
}

fn validate_relative_bundle_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("unsafe dependency bundle path: {value}"));
    }
    Ok(path.to_path_buf())
}

fn normalize_dependency_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

fn normalize_game_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

fn installer_kind(package: &DependencyBundlePackage) -> Result<InstallerKind, JobError> {
    match package.installer_kind.as_str() {
        "vc-redist" => Ok(InstallerKind::VcRedist),
        "vc-redist-legacy" => Ok(InstallerKind::VcRedistLegacy),
        "directx-redist" => Ok(InstallerKind::DirectXRedist),
        "dotnet-framework" => Ok(InstallerKind::DotNetFramework),
        "msi" => Ok(InstallerKind::Msi),
        "physx-redist" => Ok(InstallerKind::PhysXRedist),
        "openal-redist" => Ok(InstallerKind::OpenAlRedist),
        other => Err(JobError::Depot(format!(
            "unsupported installer kind for {}: {other}",
            package.id
        ))),
    }
}

fn dependency_spec_by_id(id: &str) -> Result<Option<DependencySpec>, JobError> {
    let requested = normalize_dependency_id(id);
    if requested == "ea-app" {
        return Ok(Some(DependencySpec {
            id: "ea-app".to_string(),
            display_name: "EA App".to_string(),
            url: "https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe".to_string(),
            file_name: "EAappInstaller.exe".to_string(),
            bundled_path: None,
            download_sha256: None,
            payload_sha256: None,
            minimum_download_bytes: 512 * 1024,
            minimum_payload_bytes: 512 * 1024,
            archive_kind: None,
            extracted_installer_path: None,
            extracted_installer_sha256: None,
            minimum_extracted_installer_bytes: None,
            installer_kind: InstallerKind::EaApp,
        }));
    }

    let package = dependency_bundle()?.packages.iter().find(|package| {
        normalize_dependency_id(&package.id) == requested
            || package
                .aliases
                .iter()
                .any(|alias| normalize_dependency_id(alias) == requested)
    });
    package
        .map(|package| {
            Ok(DependencySpec {
                id: normalize_dependency_id(&package.id),
                display_name: package.display_name.clone(),
                url: package.url.clone(),
                file_name: package.download_file_name.clone(),
                bundled_path: Some(package.bundled_path.clone()),
                download_sha256: Some(package.download_sha256.clone()),
                payload_sha256: Some(package.payload_sha256.clone()),
                minimum_download_bytes: package.minimum_download_bytes,
                minimum_payload_bytes: package.minimum_payload_bytes,
                archive_kind: package.archive_kind.clone(),
                extracted_installer_path: package.extracted_installer_path.clone(),
                extracted_installer_sha256: package.extracted_installer_sha256.clone(),
                minimum_extracted_installer_bytes: package.minimum_extracted_installer_bytes,
                installer_kind: installer_kind(package)?,
            })
        })
        .transpose()
}

fn dependency_specs(ids: &[&str]) -> Result<Vec<DependencySpec>, JobError> {
    let mut specs = Vec::new();
    for id in ids {
        specs.push(dependency_spec_by_id(id)?.ok_or_else(|| {
            JobError::Depot(format!("launcher dependency catalog is missing {id}"))
        })?);
    }
    deduplicate_dependency_specs(&mut specs);
    Ok(specs)
}

fn dependency_specs_for_game(game_id: &str) -> Result<Vec<DependencySpec>, JobError> {
    let bundle = dependency_bundle()?;
    let normalized = normalize_game_id(game_id);
    let ids = bundle
        .game_profiles
        .iter()
        .find(|(profile_game_id, _)| normalize_game_id(profile_game_id) == normalized)
        .map(|(_, dependencies)| dependencies)
        .unwrap_or(&bundle.default_dependencies);
    let id_refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
    dependency_specs(&id_refs)
}

fn deduplicate_dependency_specs(specs: &mut Vec<DependencySpec>) {
    let mut seen = HashSet::new();
    specs.retain(|spec| seen.insert(spec.id.clone()));
}

fn split_manifest_dependencies(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| entry.split([',', ';', '\n', '\r']))
        .map(normalize_dependency_id)
        .filter(|id| !id.is_empty())
        .collect()
}

fn resolve_manifest_dependencies(entries: &[String]) -> Result<Vec<DependencySpec>, JobError> {
    let dependency_ids = split_manifest_dependencies(entries);
    let mut specs = Vec::new();
    let mut unsupported = Vec::new();
    for id in dependency_ids {
        match dependency_spec_by_id(&id)? {
            Some(spec) => specs.push(spec),
            None => unsupported.push(id),
        }
    }
    if !unsupported.is_empty() {
        unsupported.sort();
        unsupported.dedup();
        return Err(JobError::Depot(format!(
            "unsupported game dependency IDs: {}",
            unsupported.join(", ")
        )));
    }
    deduplicate_dependency_specs(&mut specs);
    Ok(specs)
}

#[cfg(target_os = "windows")]
fn registry_dword(paths: &[&str], value: &str) -> Option<u64> {
    paths.iter().find_map(|path| {
        let output = hidden_command("reg.exe")
            .args(["query", path, "/v", value])
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        let expected = value.to_ascii_lowercase();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find(|line| line.to_ascii_lowercase().contains(&expected))
            .and_then(|line| {
                line.split_whitespace().rev().find_map(|token| {
                    token
                        .strip_prefix("0x")
                        .or_else(|| token.strip_prefix("0X"))
                        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                        .or_else(|| token.parse::<u64>().ok())
                })
            })
    })
}

fn windows_directory() -> PathBuf {
    env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

#[cfg(target_os = "windows")]
fn directory_has_entry_prefix(directory: &Path, prefix: &str) -> bool {
    let prefix = prefix.to_ascii_lowercase();
    fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(&prefix)
        })
}

#[cfg(target_os = "windows")]
fn vc_runtime_installed(version: &str, architecture: &str) -> bool {
    let native =
        format!(r"HKLM\SOFTWARE\Microsoft\VisualStudio\{version}\VC\Runtimes\{architecture}");
    let wow = format!(
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\{version}\VC\Runtimes\{architecture}"
    );
    registry_dword(&[native.as_str(), wow.as_str()], "Installed") == Some(1)
}

fn dependency_installed(spec: &DependencySpec) -> bool {
    #[cfg(target_os = "windows")]
    {
        const VC_RUNTIME_X64: &[&str] = &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
        ];
        const VC_RUNTIME_X86: &[&str] = &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
        ];
        const DOTNET_FULL: &[&str] = &[
            r"HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\NET Framework Setup\NDP\v4\Full",
        ];

        let windows = windows_directory();
        let winsxs = windows.join("WinSxS");
        match spec.id.as_str() {
            "vc-redist-v14-x64" => registry_dword(VC_RUNTIME_X64, "Installed") == Some(1),
            "vc-redist-v14-x86" => registry_dword(VC_RUNTIME_X86, "Installed") == Some(1),
            "vc-redist-2008-x64" => {
                directory_has_entry_prefix(&winsxs, "amd64_microsoft.vc90.crt_")
            }
            "vc-redist-2008-x86" => directory_has_entry_prefix(&winsxs, "x86_microsoft.vc90.crt_"),
            "vc-redist-2010-x64" => windows.join("System32/msvcp100.dll").is_file(),
            "vc-redist-2010-x86" => {
                let syswow64 = windows.join("SysWOW64");
                if syswow64.is_dir() {
                    syswow64.join("msvcp100.dll").is_file()
                } else {
                    windows.join("System32/msvcp100.dll").is_file()
                }
            }
            "vc-redist-2012-x64" => {
                vc_runtime_installed("11.0", "x64")
                    || windows.join("System32/msvcp110.dll").is_file()
            }
            "vc-redist-2012-x86" => {
                vc_runtime_installed("11.0", "x86")
                    || windows.join("SysWOW64/msvcp110.dll").is_file()
            }
            "vc-redist-2013-x64" => {
                vc_runtime_installed("12.0", "x64")
                    || windows.join("System32/msvcp120.dll").is_file()
            }
            "vc-redist-2013-x86" => {
                vc_runtime_installed("12.0", "x86")
                    || windows.join("SysWOW64/msvcp120.dll").is_file()
            }
            "directx-jun2010" => {
                let x64_component = windows.join("System32/d3dx9_43.dll").is_file();
                let syswow64 = windows.join("SysWOW64");
                x64_component && (!syswow64.is_dir() || syswow64.join("d3dx9_43.dll").is_file())
            }
            "dotnet-framework-481" => registry_dword(DOTNET_FULL, "Release")
                .map(|release| release >= DOTNET_FRAMEWORK_481_RELEASE)
                .unwrap_or(false),
            "xna-framework-4-refresh" => {
                let assembly_root =
                    windows.join("Microsoft.NET/assembly/GAC_32/Microsoft.Xna.Framework");
                assembly_root.is_dir()
                    && fs::read_dir(assembly_root)
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter_map(Result::ok)
                        .any(|entry| entry.path().is_dir())
            }
            "nvidia-physx" => env::var_os("ProgramFiles(x86)")
                .map(PathBuf::from)
                .into_iter()
                .chain(env::var_os("ProgramFiles").map(PathBuf::from))
                .any(|root| {
                    let physx = root.join("NVIDIA Corporation/PhysX");
                    physx.join("Common/PhysXLoader.dll").is_file() || physx.join("Engine").is_dir()
                }),
            "openal-1.1" => {
                windows.join("System32/OpenAL32.dll").is_file()
                    || windows.join("SysWOW64/OpenAL32.dll").is_file()
            }
            "ea-app" => [
                env::var_os("ProgramFiles"),
                env::var_os("ProgramFiles(x86)"),
            ]
            .into_iter()
            .flatten()
            .map(PathBuf::from)
            .any(|root| {
                [
                    "Electronic Arts/EA Desktop/EA Desktop/EADesktop.exe",
                    "Electronic Arts/EA Desktop/EADesktop.exe",
                    "Electronic Arts/EA Desktop/EA Desktop.exe",
                ]
                .iter()
                .any(|relative| root.join(relative).is_file())
            }),
            _ => false,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = spec;
        true
    }
}

pub(super) fn ensure_game_dependencies(
    app: &AppHandle,
    source: &DepotSource,
    install_path: &Path,
) -> Result<Vec<String>, JobError> {
    let mut specs = if let Ok(Some(manifest)) = super::read_installed_manifest(install_path) {
        match manifest.dependencies {
            Some(entries) if !split_manifest_dependencies(&entries).is_empty() => {
                resolve_manifest_dependencies(&entries)?
            }
            _ => dependency_specs_for_game(&source.game_id)?,
        }
    } else {
        dependency_specs_for_game(&source.game_id)?
    };
    deduplicate_dependency_specs(&mut specs);

    let mut installed = Vec::new();
    for spec in specs {
        if dependency_installed(&spec) {
            continue;
        }
        let installer = prepare_dependency_installer(app, &spec)?;
        run_dependency_installer(&spec, &installer)?;
        if !dependency_installed(&spec) {
            return Err(JobError::Depot(format!(
                "{} finished but the required runtime was not detected",
                spec.display_name
            )));
        }
        installed.push(spec.display_name);
    }
    Ok(installed)
}

fn prepare_dependency_installer(
    app: &AppHandle,
    spec: &DependencySpec,
) -> Result<PathBuf, JobError> {
    let redist_dir = app.path().app_data_dir()?.join("redist");
    fs::create_dir_all(&redist_dir)?;
    let source = match locate_bundled_installer(app, spec)? {
        Some(installer) => installer,
        None => download_dependency_installer(&redist_dir, spec)?,
    };
    match spec.installer_kind {
        InstallerKind::DirectXRedist => prepare_directx_installer(&redist_dir, spec, &source),
        InstallerKind::OpenAlRedist => prepare_zip_dependency_installer(&redist_dir, spec, &source),
        _ => Ok(source),
    }
}

fn prepare_directx_installer(
    redist_dir: &Path,
    spec: &DependencySpec,
    source: &Path,
) -> Result<PathBuf, JobError> {
    let hash_prefix = spec
        .payload_sha256
        .as_deref()
        .unwrap_or("unverified")
        .chars()
        .take(12)
        .collect::<String>();
    let extraction_dir = redist_dir.join(format!("directx-jun2010-{hash_prefix}"));
    fs::create_dir_all(&extraction_dir)?;
    let extracted_path = spec.extracted_installer_path.as_deref().ok_or_else(|| {
        JobError::Depot("DirectX extracted installer path is not configured".to_string())
    })?;
    let payload = extraction_dir
        .join(validate_relative_bundle_path(extracted_path).map_err(JobError::Depot)?);
    let extracted_minimum = spec.minimum_extracted_installer_bytes.ok_or_else(|| {
        JobError::Depot("DirectX extracted installer size is not configured".to_string())
    })?;
    let extracted_hash = spec.extracted_installer_sha256.as_deref().ok_or_else(|| {
        JobError::Depot("DirectX extracted installer hash is not configured".to_string())
    })?;
    if payload.is_file() {
        verify_dependency_file(&payload, extracted_minimum, Some(extracted_hash), &spec.id)?;
        verify_directx_payload_directory(&extraction_dir)?;
        return Ok(payload);
    }

    let extract_target = format!("/T:{}", extraction_dir.display());
    let status = hidden_command(&source)
        .args(["/Q", &extract_target])
        .status()?;
    if !status.success() {
        return Err(JobError::Depot(format!(
            "failed to extract {}",
            spec.display_name
        )));
    }
    verify_dependency_file(&payload, extracted_minimum, Some(extracted_hash), &spec.id)?;
    verify_directx_payload_directory(&extraction_dir)?;
    Ok(payload)
}

fn prepare_zip_dependency_installer(
    redist_dir: &Path,
    spec: &DependencySpec,
    source: &Path,
) -> Result<PathBuf, JobError> {
    if spec.archive_kind.as_deref() != Some("zip") {
        return Err(JobError::Depot(format!(
            "unsupported dependency archive for {}",
            spec.id
        )));
    }
    let extracted_path = spec.extracted_installer_path.as_deref().ok_or_else(|| {
        JobError::Depot(format!("archive installer path is missing for {}", spec.id))
    })?;
    let relative = validate_relative_bundle_path(extracted_path).map_err(JobError::Depot)?;
    let expected_hash = spec.extracted_installer_sha256.as_deref().ok_or_else(|| {
        JobError::Depot(format!("archive installer hash is missing for {}", spec.id))
    })?;
    let minimum_bytes = spec.minimum_extracted_installer_bytes.ok_or_else(|| {
        JobError::Depot(format!("archive installer size is missing for {}", spec.id))
    })?;
    let hash_prefix = spec
        .payload_sha256
        .as_deref()
        .unwrap_or("unverified")
        .chars()
        .take(12)
        .collect::<String>();
    let extraction_dir = redist_dir.join(format!("{}-{hash_prefix}", spec.id));
    fs::create_dir_all(&extraction_dir)?;
    let payload = extraction_dir.join(&relative);
    if payload.is_file() {
        verify_dependency_file(&payload, minimum_bytes, Some(expected_hash), &spec.id)?;
        return Ok(payload);
    }

    let archive_file = File::open(source)?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|error| JobError::Depot(format!("invalid {} archive: {error}", spec.id)))?;
    let entry_name = relative.to_string_lossy().replace('\\', "/");
    let mut entry = archive.by_name(&entry_name).map_err(|_| {
        JobError::Depot(format!(
            "{} archive does not contain {}",
            spec.id, entry_name
        ))
    })?;
    if entry.is_dir() || entry.enclosed_name().as_deref() != Some(relative.as_path()) {
        return Err(JobError::Depot(format!(
            "unsafe installer entry in {} archive",
            spec.id
        )));
    }
    if entry.size() > MAX_DEPENDENCY_DOWNLOAD_BYTES {
        return Err(JobError::Depot(format!(
            "extracted dependency is too large: {}",
            spec.id
        )));
    }
    let temporary = extraction_dir.join(format!("{}.download", uuid::Uuid::new_v4()));
    {
        let mut output = File::create(&temporary)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        output.sync_all()?;
    }
    verify_dependency_file(&temporary, minimum_bytes, Some(expected_hash), &spec.id)?;
    if payload.exists() {
        fs::remove_file(&payload)?;
    }
    fs::rename(&temporary, &payload)?;
    Ok(payload)
}

fn run_dependency_installer(spec: &DependencySpec, installer: &Path) -> Result<(), JobError> {
    if spec.installer_kind == InstallerKind::Msi {
        let installer_arg = installer.to_string_lossy().to_string();
        let args = ["/i", installer_arg.as_str(), "/quiet", "/norestart"];
        return run_elevated(
            Path::new("msiexec.exe"),
            &args,
            installer.parent(),
            true,
            spec.installer_kind.accepted_exit_codes(),
        );
    }
    run_elevated(
        installer,
        spec.installer_kind.arguments(),
        installer.parent(),
        true,
        spec.installer_kind.accepted_exit_codes(),
    )
}

fn locate_bundled_installer(
    app: &AppHandle,
    spec: &DependencySpec,
) -> Result<Option<PathBuf>, JobError> {
    let Some(relative) = spec.bundled_path.as_deref() else {
        return Ok(None);
    };
    let relative = validate_relative_bundle_path(relative).map_err(JobError::Depot)?;
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("redist").join(&relative));
        candidates.push(resource_dir.join("resources/redist").join(&relative));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/redist")
            .join(&relative),
    );

    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        verify_dependency_file(
            &candidate,
            spec.minimum_payload_bytes,
            spec.payload_sha256.as_deref(),
            &spec.id,
        )?;
        return Ok(Some(candidate));
    }
    Ok(None)
}

fn verify_directx_payload_directory(directory: &Path) -> Result<(), JobError> {
    let file_count = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .count();
    if file_count < 150 {
        return Err(JobError::Depot(format!(
            "DirectX offline bundle is incomplete ({file_count} files)"
        )));
    }
    Ok(())
}

fn download_dependency_installer(
    redist_dir: &Path,
    spec: &DependencySpec,
) -> Result<PathBuf, JobError> {
    let destination = redist_dir.join(&spec.file_name);
    if destination.is_file()
        && verify_dependency_file(
            &destination,
            spec.minimum_download_bytes,
            spec.download_sha256.as_deref(),
            &spec.id,
        )
        .is_ok()
    {
        return Ok(destination);
    }

    let temporary = redist_dir.join(format!(
        "{}.{}.download",
        spec.file_name,
        uuid::Uuid::new_v4()
    ));
    let response = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .unwrap_or_else(|_| Client::new())
        .get(&spec.url)
        .header(USER_AGENT, "0xoLemon-launcher-redist/1.0")
        .send()?
        .error_for_status()?;
    if response
        .content_length()
        .map(|length| length > MAX_DEPENDENCY_DOWNLOAD_BYTES)
        .unwrap_or(false)
    {
        return Err(JobError::Depot(format!(
            "dependency download is too large: {}",
            spec.id
        )));
    }

    let mut file = File::create(&temporary)?;
    let copied = std::io::copy(
        &mut response.take(MAX_DEPENDENCY_DOWNLOAD_BYTES + 1),
        &mut file,
    )?;
    file.sync_all()?;
    if copied > MAX_DEPENDENCY_DOWNLOAD_BYTES {
        let _ = fs::remove_file(&temporary);
        return Err(JobError::Depot(format!(
            "dependency download exceeded the size limit: {}",
            spec.id
        )));
    }
    if let Err(error) = verify_dependency_file(
        &temporary,
        spec.minimum_download_bytes,
        spec.download_sha256.as_deref(),
        &spec.id,
    ) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if destination.exists() {
        fs::remove_file(&destination)?;
    }
    fs::rename(&temporary, &destination)?;
    Ok(destination)
}

fn verify_dependency_file(
    path: &Path,
    minimum_bytes: u64,
    expected_sha256: Option<&str>,
    dependency_id: &str,
) -> Result<(), JobError> {
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.len() < minimum_bytes {
        return Err(JobError::Depot(format!(
            "dependency payload is missing or too small: {dependency_id}"
        )));
    }
    if let Some(expected) = expected_sha256 {
        let actual = sha256_file(path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(JobError::Depot(format!(
                "dependency SHA-256 mismatch for {dependency_id}"
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, JobError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode_upper(hasher.finalize()))
}

// ══════════════════════════════════════════════════════════════
//  Desktop shortcuts
// ══════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
fn desktop_directory(app: &AppHandle, fallback: &Path) -> PathBuf {
    let registry = hidden_command("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders",
            "/v",
            "Desktop",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().find_map(|line| {
                ["REG_EXPAND_SZ", "REG_SZ"].iter().find_map(|marker| {
                    line.find(*marker).and_then(|index| {
                        let raw = line[index + marker.len()..].trim();
                        (!raw.is_empty()).then(|| raw.to_string())
                    })
                })
            })
        })
        .map(|path| {
            let mut expanded = path;
            for (key, value) in env::vars() {
                expanded = expanded.replace(&format!("%{key}%"), &value);
            }
            PathBuf::from(expanded)
        });

    registry
        .or_else(|| {
            env::var("OneDrive")
                .ok()
                .map(PathBuf::from)
                .map(|home| home.join("Desktop"))
                .filter(|path| path.exists())
        })
        .or_else(|| {
            env::var("USERPROFILE")
                .ok()
                .map(PathBuf::from)
                .map(|home| home.join("Desktop"))
        })
        .unwrap_or_else(|| {
            app.path()
                .app_data_dir()
                .unwrap_or_else(|_| fallback.to_path_buf())
        })
}

#[cfg(not(target_os = "windows"))]
fn desktop_directory(app: &AppHandle, fallback: &Path) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| fallback.to_path_buf())
}

#[cfg(target_os = "windows")]
const GAME_SHORTCUT_BOOTSTRAP_FILE: &str = "0xoLemon Launcher.exe";

#[cfg(target_os = "windows")]
fn game_shortcut_bootstrap_path(install_root: &Path) -> PathBuf {
    install_root.join(GAME_SHORTCUT_BOOTSTRAP_FILE)
}

#[cfg(target_os = "windows")]
fn same_file_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn files_have_same_contents(left: &Path, right: &Path) -> bool {
    let left_len = left.metadata().map(|metadata| metadata.len()).ok();
    let right_len = right.metadata().map(|metadata| metadata.len()).ok();
    if left_len.is_none() || left_len != right_len {
        return false;
    }

    let (Ok(mut left_file), Ok(mut right_file)) = (File::open(left), File::open(right)) else {
        return false;
    };
    let mut left_buffer = [0_u8; 128 * 1024];
    let mut right_buffer = [0_u8; 128 * 1024];
    loop {
        let Ok(left_read) = left_file.read(&mut left_buffer) else {
            return false;
        };
        let Ok(right_read) = right_file.read(&mut right_buffer) else {
            return false;
        };
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return false;
        }
        if left_read == 0 {
            return true;
        }
    }
}

#[cfg(target_os = "windows")]
fn refresh_game_shortcut_bootstrap(
    launcher_exe: &Path,
    bootstrap_exe: &Path,
) -> Result<(), JobError> {
    if same_file_path(launcher_exe, bootstrap_exe)
        || files_have_same_contents(launcher_exe, bootstrap_exe)
    {
        return Ok(());
    }

    if let Some(parent) = bootstrap_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = bootstrap_exe.with_extension("exe.new");
    let _ = fs::remove_file(&temporary);
    fs::copy(launcher_exe, &temporary)?;
    if bootstrap_exe.exists() {
        if let Err(error) = fs::remove_file(bootstrap_exe) {
            let _ = fs::remove_file(&temporary);
            // A shortcut bootstrap that is currently running is locked on Windows.
            // Keep it for this session; the main launcher will refresh it later.
            if bootstrap_exe.is_file() {
                return Ok(());
            }
            return Err(error.into());
        }
    }
    fs::rename(temporary, bootstrap_exe)?;
    Ok(())
}

pub(super) fn remove_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
) -> Result<Vec<PathBuf>, JobError> {
    let mut removed = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let desktop = desktop_directory(app, install_root);
        // Remove both old .lnk and new .url shortcut types
        let mut candidates = vec![
            desktop.join(format!("{}.lnk", source.game_dir_name)),
            desktop.join(format!("{}.url", source.game_dir_name)),
        ];
        if let Ok(profile) = env::var("USERPROFILE") {
            candidates.push(
                PathBuf::from(&profile)
                    .join("Desktop")
                    .join(format!("{}.lnk", source.game_dir_name)),
            );
            candidates.push(
                PathBuf::from(&profile)
                    .join("Desktop")
                    .join(format!("{}.url", source.game_dir_name)),
            );
        }
        if let Ok(one_drive) = env::var("OneDrive") {
            candidates.push(
                PathBuf::from(&one_drive)
                    .join("Desktop")
                    .join(format!("{}.lnk", source.game_dir_name)),
            );
            candidates.push(
                PathBuf::from(&one_drive)
                    .join("Desktop")
                    .join(format!("{}.url", source.game_dir_name)),
            );
        }
        candidates.sort();
        candidates.dedup();
        for path in candidates {
            if path.is_file() {
                fs::remove_file(&path)?;
                removed.push(path);
            }
        }

        let bootstrap = game_shortcut_bootstrap_path(install_root);
        if bootstrap.is_file() {
            fs::remove_file(&bootstrap)?;
            removed.push(bootstrap);
        }

        // Remove the legacy AppData bootstrap created by older launcher builds.
        if let Ok(app_data) = app.path().app_data_dir() {
            let legacy_bootstrap = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
            if legacy_bootstrap.is_file() {
                fs::remove_file(&legacy_bootstrap)?;
                removed.push(legacy_bootstrap);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root);
    }

    Ok(removed)
}

#[cfg(target_os = "windows")]
fn create_windows_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    icon_executable: &Path,
    relative_executable: Option<&str>,
) -> Result<Option<PathBuf>, JobError> {
    let launcher_exe = env::current_exe().map_err(|error| {
        JobError::Depot(format!("unable to locate the launcher executable: {error}"))
    })?;
    if !launcher_exe.is_file() || !icon_executable.is_file() {
        return Ok(None);
    }

    let desktop = desktop_directory(app, install_root);
    fs::create_dir_all(&desktop)?;
    let shortcut_path = desktop.join(format!("{}.lnk", source.game_dir_name));
    let legacy_url = desktop.join(format!("{}.url", source.game_dir_name));
    let _ = fs::remove_file(&legacy_url);
    let _ = fs::remove_file(&shortcut_path);

    // Remove the bootstrap copies used by older builds. The shell link now points
    // straight at the installed launcher and passes the game request as CLI flags.
    if let Ok(app_data) = app.path().app_data_dir() {
        let legacy = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
        let _ = fs::remove_file(legacy);
    }
    let _ = fs::remove_file(game_shortcut_bootstrap_path(install_root));

    let install_path = install_root.display().to_string();
    let arguments = match relative_executable {
        Some(relative) => shortcut_argument_line(&[
            ("--launch-game", source.game_id.as_str()),
            ("--install-path", install_path.as_str()),
            ("--launch-executable", relative),
        ]),
        None => shortcut_argument_line(&[
            ("--launch-game", source.game_id.as_str()),
            ("--install-path", install_path.as_str()),
        ]),
    };
    let launcher_directory = launcher_exe.parent().unwrap_or(install_root);
    let icon_location = format!("{},0", icon_executable.display());
    let description = format!("Launch {} with 0xoLemon", source.game_dir_name);

    // WScript.Shell creates a native Windows Shell Link (.lnk), so the shortcut
    // does not depend on URL-protocol registration and supports quoted paths.
    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell; \
         $shortcut = $shell.CreateShortcut({}); \
         $shortcut.TargetPath = {}; \
         $shortcut.Arguments = {}; \
         $shortcut.WorkingDirectory = {}; \
         $shortcut.IconLocation = {}; \
         $shortcut.Description = {}; \
         $shortcut.Save()",
        ps_quote_os(shortcut_path.as_os_str()),
        ps_quote_os(launcher_exe.as_os_str()),
        ps_quote(&arguments),
        ps_quote_os(launcher_directory.as_os_str()),
        ps_quote(&icon_location),
        ps_quote(&description),
    );
    let output = hidden_command("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()?;
    if !output.status.success() || !shortcut_path.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(JobError::Depot(if stderr.is_empty() {
            format!(
                "unable to create desktop shortcut: {}",
                shortcut_path.display()
            )
        } else {
            format!("unable to create desktop shortcut: {stderr}")
        }));
    }

    Ok(Some(shortcut_path))
}

pub(super) fn create_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    executable: &Path,
    relative_executable: &str,
) -> Result<Option<PathBuf>, JobError> {
    if !executable.exists() {
        return Ok(None);
    }
    #[cfg(target_os = "windows")]
    {
        create_windows_game_shortcut(
            app,
            source,
            install_root,
            executable,
            Some(relative_executable),
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root, executable, relative_executable);
        Ok(None)
    }
}

/// Like `create_game_shortcut` but does not pin a specific executable.
/// Multi-executable games can therefore reopen the launch-option picker.
pub(super) fn create_game_shortcut_no_exe(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    icon_executable: &Path,
) -> Result<Option<PathBuf>, JobError> {
    if !icon_executable.exists() {
        return Ok(None);
    }
    #[cfg(target_os = "windows")]
    {
        create_windows_game_shortcut(app, source, install_root, icon_executable, None)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root, icon_executable);
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn shortcut_argument_line(args: &[(&str, &str)]) -> String {
    args.iter()
        .flat_map(|(flag, value)| [(*flag).to_string(), win_arg_quote(value)])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn win_arg_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '\\' | '/')
        })
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

// ══════════════════════════════════════════════════════════════
//  Process launching
// ══════════════════════════════════════════════════════════════

pub(super) fn launch_option_processes(
    game_id: &str,
    install_root: &Path,
    option: &GameLaunchOption,
    runtime_environment: &[(String, String)],
) -> Result<LaunchedProcessSet, JobError> {
    let mut launched = Vec::new();
    let mut main_process = None;
    let main_index = option
        .processes
        .iter()
        .position(|process| process.role.eq_ignore_ascii_case("main"))
        .or_else(|| option.processes.len().checked_sub(1));

    for (index, process) in option.processes.iter().enumerate() {
        if process.delay_before_ms > 0 {
            std::thread::sleep(Duration::from_millis(process.delay_before_ms));
        }

        let Some(executable) = process_path(install_root, process, game_id) else {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "unsafe launch process path: {}",
                process.path
            )));
        };
        if !executable.exists() {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "launch process is missing: {}",
                executable.display()
            )));
        }

        let working_dir = process_working_directory(install_root, &executable, process, game_id)
            .ok_or_else(|| {
                JobError::Depot(format!("unsafe working directory for {}", process.path))
            })?;
        if !working_dir.is_dir() {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "launch working directory is missing: {}",
                working_dir.display()
            )));
        }

        let args = process
            .args
            .iter()
            .map(|arg| expand_placeholders(arg, install_root, game_id))
            .collect::<Vec<_>>();
        let mut environment = process
            .environment
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    expand_placeholders(value, install_root, game_id),
                )
            })
            .collect::<Vec<_>>();
        if main_index == Some(index) {
            for (key, value) in runtime_environment {
                environment.retain(|(existing, _)| !existing.eq_ignore_ascii_case(key));
                environment.push((key.clone(), value.clone()));
            }
        }
        let hidden = process
            .hidden
            .unwrap_or_else(|| is_script_path(&executable));

        if process.run_as_admin {
            let pid = run_configured_process_elevated(
                &executable,
                &args,
                &working_dir,
                &environment,
                hidden,
                process.wait_for_exit,
            )?;
            if main_index == Some(index) {
                main_process = pid.map(TrackedMainProcess::Pid);
            }
        } else {
            let child = run_configured_process(
                &executable,
                &args,
                &working_dir,
                &environment,
                hidden,
                process.wait_for_exit,
            )?;
            if main_index == Some(index) {
                main_process = child.map(TrackedMainProcess::Child);
            }
        }

        launched.push(executable);
        if process.delay_after_ms > 0 {
            std::thread::sleep(Duration::from_millis(process.delay_after_ms));
        }
    }

    if launched.is_empty() {
        return Err(JobError::Depot(
            "the selected launch option did not start any process".to_string(),
        ));
    }
    Ok(LaunchedProcessSet {
        paths: launched,
        main_process,
    })
}

pub(super) struct LaunchedProcessSet {
    pub(super) paths: Vec<PathBuf>,
    pub(super) main_process: Option<TrackedMainProcess>,
}

pub(super) enum TrackedMainProcess {
    Child(Child),
    /// PID returned by PowerShell Start-Process -Verb RunAs. The elevated
    /// process is not a std::process::Child of the launcher, so it is monitored
    /// by PID until Windows reports that it has exited.
    Pid(u32),
}

impl TrackedMainProcess {
    pub(super) fn id(&self) -> u32 {
        match self {
            Self::Child(child) => child.id(),
            Self::Pid(pid) => *pid,
        }
    }

    pub(super) fn terminate(&mut self) {
        match self {
            Self::Child(child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Self::Pid(pid) => {
                let _ = hidden_command("taskkill.exe")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .status();
            }
        }
    }

    pub(super) fn wait(&mut self) -> Result<Option<i32>, JobError> {
        match self {
            Self::Child(child) => Ok(child.wait()?.code()),
            Self::Pid(pid) => wait_for_elevated_process(*pid),
        }
    }
}

fn wait_for_elevated_process(pid: u32) -> Result<Option<i32>, JobError> {
    #[cfg(target_os = "windows")]
    {
        let script = format!("Wait-Process -Id {} -ErrorAction SilentlyContinue", pid);
        let status = hidden_command("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()?;
        if status.success() {
            Ok(None)
        } else {
            Err(JobError::Depot(format!(
                "unable to monitor elevated game process {pid}"
            )))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        Ok(None)
    }
}

fn run_configured_process(
    executable: &Path,
    args: &[String],
    working_dir: &Path,
    environment: &[(String, String)],
    hidden: bool,
    wait: bool,
) -> Result<Option<Child>, JobError> {
    let mut command = if is_script_path(executable) {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/S", "/C"]);
        command.arg(batch_command_line(executable, args));
        command
    } else {
        let mut command = Command::new(executable);
        command.args(args);
        command
    };

    command.current_dir(working_dir);
    for (key, value) in environment {
        validate_environment_key(key)?;
        command.env(key, value);
    }

    if hidden {
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(super::CREATE_NO_WINDOW);
        }
    }

    if wait {
        let status = command.status()?;
        if !status.success() {
            return Err(JobError::Depot(format!(
                "launch process exited with code {}: {}",
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                executable.display()
            )));
        }
        Ok(None)
    } else {
        Ok(Some(command.spawn()?))
    }
}

fn run_configured_process_elevated(
    executable: &Path,
    args: &[String],
    working_dir: &Path,
    environment: &[(String, String)],
    hidden: bool,
    wait: bool,
) -> Result<Option<u32>, JobError> {
    #[cfg(target_os = "windows")]
    {
        let (file_path, process_args) = if is_script_path(executable) {
            (
                PathBuf::from("cmd.exe"),
                vec![
                    "/D".to_string(),
                    "/S".to_string(),
                    "/C".to_string(),
                    batch_command_line(executable, args),
                ],
            )
        } else {
            (executable.to_path_buf(), args.to_vec())
        };

        let mut script = String::new();
        for (key, value) in environment {
            validate_environment_key(key)?;
            script.push_str(&format!("$env:{} = {}; ", key, ps_quote(value)));
        }
        script.push_str(&format!(
            "$p = Start-Process -FilePath {} -Verb RunAs -WindowStyle {} -WorkingDirectory {} -PassThru",
            ps_quote_os(file_path.as_os_str()),
            if hidden { "Hidden" } else { "Normal" },
            ps_quote_os(working_dir.as_os_str()),
        ));
        if !process_args.is_empty() {
            let quoted_args = process_args
                .iter()
                .map(|arg| ps_quote(arg))
                .collect::<Vec<_>>()
                .join(", ");
            script.push_str(&format!(" -ArgumentList @({quoted_args})"));
        }
        if wait {
            script.push_str("; $p.WaitForExit(); if ($p.ExitCode -ne 0) { exit $p.ExitCode }");
        } else {
            script.push_str("; [Console]::Out.Write($p.Id)");
        }

        let output = hidden_command("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(JobError::Depot(if stderr.is_empty() {
                format!(
                    "admin launch was canceled or failed: {}",
                    executable.display()
                )
            } else {
                format!("admin launch failed: {stderr}")
            }));
        }
        if wait {
            return Ok(None);
        }
        let pid_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let pid = pid_text.parse::<u32>().map_err(|_| {
            JobError::Depot(format!(
                "admin launch did not return a process id for {}",
                executable.display()
            ))
        })?;
        Ok(Some(pid))
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_configured_process(executable, args, working_dir, environment, hidden, wait)
            .map(|child| child.map(|child| child.id()))
    }
}

fn validate_environment_key(key: &str) -> Result<(), JobError> {
    if !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(())
    } else {
        Err(JobError::Depot(format!(
            "invalid environment variable name in launch config: {key}"
        )))
    }
}

fn batch_command_line(executable: &Path, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 2);
    parts.push("call".to_string());
    parts.push(cmd_quote(&executable.display().to_string()));
    parts.extend(args.iter().map(|arg| cmd_quote(arg)));
    parts.join(" ")
}

fn cmd_quote(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn run_elevated(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
    accepted_exit_codes: &[i32],
) -> Result<(), JobError> {
    #[cfg(target_os = "windows")]
    {
        run_elevated_windows(executable, args, working_dir, wait, accepted_exit_codes)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new(executable);
        command.args(args);
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        if wait {
            let status = command.status()?;
            let exit_code = status.code().unwrap_or(-1);
            if !accepted_exit_codes.contains(&exit_code) {
                return Err(JobError::Depot(format!(
                    "dependency installer failed with exit code {exit_code}"
                )));
            }
        } else {
            command.spawn()?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn run_elevated_windows(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
    accepted_exit_codes: &[i32],
) -> Result<(), JobError> {
    let mut script = format!(
        "$p = Start-Process -FilePath {} -Verb RunAs -WindowStyle Normal",
        ps_quote_os(executable.as_os_str())
    );
    if let Some(dir) = working_dir {
        script.push_str(&format!(
            " -WorkingDirectory {}",
            ps_quote_os(dir.as_os_str())
        ));
    }
    if !args.is_empty() {
        let quoted_args = args
            .iter()
            .map(|arg| ps_quote(arg))
            .collect::<Vec<_>>()
            .join(", ");
        script.push_str(&format!(" -ArgumentList @({quoted_args})"));
    }
    if wait {
        let accepted = accepted_exit_codes
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        script.push_str(&format!(
            " -Wait -PassThru; if (@({accepted}) -notcontains $p.ExitCode) {{ exit 1 }}"
        ));
    }

    let status = hidden_command("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(JobError::Depot(
            "admin launch was canceled or failed".to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
fn ps_quote_os(value: &OsStr) -> String {
    ps_quote(&value.to_string_lossy())
}

#[cfg(test)]
mod dependency_tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid_and_aliases_are_unique() {
        let bundle: DependencyBundle = serde_json::from_str(DEPENDENCY_BUNDLE_JSON).unwrap();
        validate_dependency_bundle(&bundle).unwrap();

        let mut names = HashSet::new();
        for package in bundle.packages {
            assert!(names.insert(normalize_dependency_id(&package.id)));
            for alias in package.aliases {
                assert!(names.insert(normalize_dependency_id(&alias)));
            }
        }
    }

    #[test]
    fn manifest_csv_is_split_and_cumulative_runtimes_are_deduplicated() {
        let entries =
            vec!["vc-redist-x64-2019,vc-redist-x64-2022,directx-jun2010,net4.6-redist".to_string()];
        let specs = resolve_manifest_dependencies(&entries).unwrap();
        let ids = specs
            .iter()
            .map(|spec| spec.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "vc-redist-v14-x64",
                "directx-jun2010",
                "dotnet-framework-481"
            ]
        );
    }

    #[test]
    fn net46_alias_resolves_to_supported_dotnet_runtime() {
        let spec = dependency_spec_by_id("net4.6-redist").unwrap().unwrap();
        assert_eq!(spec.id, "dotnet-framework-481");
        assert_eq!(spec.installer_kind, InstallerKind::DotNetFramework);
    }

    #[test]
    fn legacy_and_optional_runtime_aliases_resolve_to_pinned_packages() {
        let cases = [
            ("vc-redist-x86-2008", "vc-redist-2008-x86"),
            ("vc-redist-x64-2012", "vc-redist-2012-x64"),
            ("vc-redist-x86-2013", "vc-redist-2013-x86"),
            ("xna-4.0", "xna-framework-4-refresh"),
            ("physx", "nvidia-physx"),
            ("openal", "openal-1.1"),
        ];
        for (alias, expected) in cases {
            let spec = dependency_spec_by_id(alias).unwrap().unwrap();
            assert_eq!(spec.id, expected);
        }
    }

    #[test]
    fn all_production_games_have_explicit_dependency_profiles() {
        const PRODUCTION_GAME_IDS: &[&str] = &[
            "007-first-light",
            "alan-wake",
            "among-us",
            "assassins-creed-black-flag-resynced",
            "assassins-creed-black-flag-resynced-steam",
            "assassins-creed-mirage",
            "atomic-heart",
            "avatar-frontiers-of-pandora",
            "blackmythwukong",
            "call-of-duty-vanguard",
            "crimson-desert",
            "dead-space-2023",
            "dragon-quest-vii-reimagined",
            "ea-sports-fc-26",
            "ea-sports-fifa-23",
            "elden-ring",
            "fifa-13",
            "fifa-14",
            "geometry-dash",
            "grand-theft-auto-san-andreas-2005",
            "heavy-rain",
            "hello-kitty-island-adventure",
            "hogwarts-legacy",
            "judgment",
            "meccha-chameleon",
            "microsoft-flight-simulator-2020-40th-anniversary-edition",
            "octopath-traveler-0",
            "paper-bride",
            "persona-3-reload",
            "persona-5-royal",
            "pragmata",
            "red-dead-redemption",
            "red-dead-redemption-2",
            "resident-evil-requiem",
            "rogue-genesia",
            "sekiro-shadows-die-twice---goty-edition",
            "soulstone-survivors",
            "stellar-blade",
            "tom-clancy-s-splinter-cell-blacklist",
            "total-war-three-kingdoms",
            "yakuza-0",
            "yakuza-like-a-dragon",
        ];
        let bundle: DependencyBundle = serde_json::from_str(DEPENDENCY_BUNDLE_JSON).unwrap();
        for game_id in PRODUCTION_GAME_IDS {
            assert!(
                bundle.game_profiles.contains_key(*game_id),
                "missing dependency profile for {game_id}"
            );
            assert!(!dependency_specs_for_game(game_id).unwrap().is_empty());
        }
    }

    #[test]
    fn unknown_future_games_receive_the_cross_architecture_safe_default() {
        let specs = dependency_specs_for_game("future-production-game").unwrap();
        let ids = specs
            .iter()
            .map(|spec| spec.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec!["vc-redist-v14-x64", "vc-redist-v14-x86", "directx-jun2010"]
        );
    }

    #[test]
    fn older_game_profiles_include_their_non_vc_runtime_requirements() {
        let gta = dependency_specs_for_game("grand-theft-auto-san-andreas-2005").unwrap();
        assert!(gta.iter().any(|spec| spec.id == "openal-1.1"));

        let fifa = dependency_specs_for_game("fifa-14").unwrap();
        assert!(fifa.iter().any(|spec| spec.id == "nvidia-physx"));
        assert!(fifa.iter().any(|spec| spec.id == "vc-redist-2008-x86"));
    }

    #[test]
    fn openal_archive_metadata_is_safe_and_hash_pinned() {
        let spec = dependency_spec_by_id("openal").unwrap().unwrap();
        assert_eq!(spec.archive_kind.as_deref(), Some("zip"));
        assert_eq!(
            spec.extracted_installer_path.as_deref(),
            Some("oalinst.exe")
        );
        assert!(spec.extracted_installer_sha256.is_some());
        assert!(spec.minimum_extracted_installer_bytes.unwrap_or(0) >= 800_000);
    }

    #[test]
    fn unsupported_manifest_dependency_is_not_silently_ignored() {
        let error =
            resolve_manifest_dependencies(&["vc-redist-x64-2022,unknown-sdk".into()]).unwrap_err();
        assert!(error.to_string().contains("unknown-sdk"));
    }

    #[test]
    fn dependency_bundle_paths_reject_absolute_and_parent_paths() {
        assert!(validate_relative_bundle_path("VC_redist.x64.exe").is_ok());
        assert!(validate_relative_bundle_path("directx/DXSETUP.exe").is_ok());
        assert!(validate_relative_bundle_path("../outside.exe").is_err());
        assert!(validate_relative_bundle_path(r"C:\outside.exe").is_err());
    }

    #[test]
    fn payload_hash_verification_detects_tampering() {
        let path = std::env::temp_dir().join(format!(
            "0xolemon-dependency-test-{}.bin",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, b"verified dependency payload").unwrap();
        let expected = sha256_file(&path).unwrap();
        verify_dependency_file(&path, 4, Some(&expected), "test-runtime").unwrap();

        fs::write(&path, b"tampered dependency payload").unwrap();
        let error = verify_dependency_file(&path, 4, Some(&expected), "test-runtime").unwrap_err();
        assert!(error.to_string().contains("SHA-256 mismatch"));
        fs::remove_file(path).unwrap();
    }
}
