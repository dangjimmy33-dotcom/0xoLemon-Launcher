use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use zip::ZipArchive;

const BYPASS_CATALOG: &str = include_str!("../resources/lightning/data.json");
const ONLINE_FIX_CATALOG: &str = include_str!("../resources/lightning/data-fix.json");
const PARTNER_STORE_CATALOG: &str = include_str!("../resources/lightning/shop.json");
const MAX_PACKAGE_FILES: usize = 4_096;
const MAX_PACKAGE_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PACKAGE_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_GITHUB_DEPTH: usize = 10;

static APPLY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningCatalogItem {
    pub kind: String,
    pub app_id: u32,
    pub category: Option<String>,
    pub name: String,
    pub package_name: Option<String>,
    pub image_url: Option<String>,
    pub background_url: Option<String>,
    pub logo_url: Option<String>,
    pub dependencies: Vec<String>,
    pub instructions: Vec<String>,
    pub note: Option<String>,
    pub launch_with_steam: bool,
    pub launch_executable: bool,
    pub active: bool,
    pub regular_price: Option<String>,
    pub supporter_price: Option<String>,
    pub discount: Option<String>,
    pub source_repository: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningCatalogResponse {
    pub kind: String,
    pub revision: String,
    pub categories: Vec<String>,
    pub items: Vec<LightningCatalogItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningIntegrationStatus {
    pub reference_version: String,
    pub bypass_count: usize,
    pub online_fix_count: usize,
    pub store_count: usize,
    pub package_apply_mode: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningPackageRequest {
    pub kind: String,
    pub app_id: u32,
    pub request_id: String,
    pub install_dir: String,
    pub revision: String,
    pub package_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningSourceIdentity {
    pub repository: String,
    pub revision: String,
    pub package_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningPackageProgress {
    pub request_id: String,
    pub app_id: u32,
    pub phase: String,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LightningAppliedFile {
    relative_path: String,
    backup_path: Option<String>,
    #[serde(default)]
    backup_sha256: Option<String>,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LightningTransactionReceipt {
    schema: u32,
    request_id: String,
    kind: String,
    app_id: u32,
    source_repository: String,
    source_reference: String,
    installed_at: String,
    install_dir: String,
    committed: bool,
    restored_at: Option<String>,
    files: Vec<LightningAppliedFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningPackageResult {
    pub request_id: String,
    pub kind: String,
    pub app_id: u32,
    pub game_name: String,
    pub install_dir: String,
    pub source_repository: String,
    pub source_reference: String,
    pub downloaded_bytes: u64,
    pub applied_files: usize,
    pub backup_files: usize,
    pub receipt_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningAppliedPackageStatus {
    pub request_id: String,
    pub kind: String,
    pub app_id: u32,
    pub source_repository: String,
    pub source_reference: String,
    pub installed_at: String,
    pub committed: bool,
    pub restored_at: Option<String>,
    pub applied_files: usize,
    pub backup_files: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningGameStatus {
    pub app_id: u32,
    pub installed: bool,
    pub install_dir: Option<String>,
    pub latest_package: Option<LightningAppliedPackageStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningExecutable {
    pub relative_path: String,
    pub size: u64,
    pub patched: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    sha: Option<String>,
}

#[derive(Debug, Clone)]
struct GithubFile {
    relative_path: PathBuf,
    download_url: String,
    size: u64,
    blob_sha: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubBranch {
    commit: GithubCommit,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubCommit {
    sha: String,
}

#[derive(Debug, Clone)]
struct PackageSource {
    repository: &'static str,
    reference: String,
    content_path: String,
}

#[tauri::command]
pub fn get_lightning_catalog(kind: String) -> Result<LightningCatalogResponse, String> {
    catalog_for_kind(&kind)
}

#[tauri::command]
pub fn get_lightning_integration_status() -> Result<LightningIntegrationStatus, String> {
    Ok(LightningIntegrationStatus {
        reference_version: "project-lightning-v5.0.8-snapshot".to_string(),
        bypass_count: catalog_for_kind("bypass")?.items.len(),
        online_fix_count: catalog_for_kind("onlineFix")?.items.len(),
        store_count: catalog_for_kind("store")?.items.len(),
        package_apply_mode: "same-volume-staging-with-rollback".to_string(),
        capabilities: vec![
            "Compatibility catalog".to_string(),
            "OnlineFix catalog".to_string(),
            "Offer catalog".to_string(),
            "Native Steamless".to_string(),
            "Managed feature packages".to_string(),
            "Steam library and runtime integration".to_string(),
            "Quick guide, legal, changelog and donate".to_string(),
        ],
    })
}

#[tauri::command]
pub async fn apply_lightning_package(
    app: AppHandle,
    request: LightningPackageRequest,
) -> Result<LightningPackageResult, String> {
    tauri::async_runtime::spawn_blocking(move || apply_package_blocking(&app, request))
        .await
        .map_err(|error| format!("LIGHTNING_TASK_FAILED: {error}"))?
}

pub async fn get_lightning_package_identity(
    kind: String,
    app_id: u32,
) -> Result<LightningSourceIdentity, String> {
    tauri::async_runtime::spawn_blocking(move || package_identity_blocking(&kind, app_id))
        .await
        .map_err(|error| format!("GAME_TOOLS_TASK_FAILED: {error}"))?
}

pub async fn pick_lightning_install_dir(
    app: AppHandle,
    kind: String,
    app_id: u32,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let kind = normalize_kind(&kind)?;
        if kind == "store" {
            return Err("GAME_TOOLS_STORE_ITEM_HAS_NO_PACKAGE".to_string());
        }
        let item = catalog_for_kind(kind)?
            .items
            .into_iter()
            .find(|item| item.app_id == app_id)
            .ok_or_else(|| "GAME_TOOLS_ITEM_NOT_FOUND".to_string())?;

        let mut picker = app.dialog().file().set_title(format!(
            "Select the folder where {} is installed",
            item.name
        ));
        if let Some(default) = crate::steam_integration::get_steam_game_install_dir(app_id) {
            let default = PathBuf::from(default);
            if default.is_dir() {
                picker = picker.set_directory(default);
            }
        }
        let Some(selected) = picker.blocking_pick_folder() else {
            return Ok(None);
        };
        let selected = selected
            .into_path()
            .map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_INVALID: {error}"))?;
        let canonical = canonical_install_dir(&selected)?;
        verify_install_identity(&canonical, app_id, &item)?;
        Ok(Some(canonical.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| format!("GAME_TOOLS_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn restore_latest_lightning_package(
    app: AppHandle,
    app_id: u32,
) -> Result<LightningPackageResult, String> {
    tauri::async_runtime::spawn_blocking(move || restore_latest_blocking(&app, app_id))
        .await
        .map_err(|error| format!("LIGHTNING_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub fn get_lightning_game_status(app_id: u32) -> Result<LightningGameStatus, String> {
    let Some(install_dir) =
        crate::steam_integration::get_steam_game_install_dir(app_id).map(PathBuf::from)
    else {
        return Ok(LightningGameStatus {
            app_id,
            installed: false,
            install_dir: None,
            latest_package: None,
        });
    };
    validate_install_dir(&install_dir)?;
    Ok(LightningGameStatus {
        app_id,
        installed: true,
        install_dir: Some(install_dir.to_string_lossy().to_string()),
        latest_package: read_latest_package_status(app_id, &install_dir)?,
    })
}

#[tauri::command]
pub async fn list_lightning_game_executables(
    app_id: u32,
) -> Result<Vec<LightningExecutable>, String> {
    tauri::async_runtime::spawn_blocking(move || list_game_executables_blocking(app_id))
        .await
        .map_err(|error| format!("LIGHTNING_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn apply_lightning_steamless(
    app_id: u32,
    relative_executable: String,
) -> Result<crate::steamless::SteamlessResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let executable = resolve_game_relative_executable(app_id, &relative_executable)?;
        Ok(crate::steamless::steamless_apply(
            executable.to_string_lossy().to_string(),
            Some(".org.exe".to_string()),
        ))
    })
    .await
    .map_err(|error| format!("LIGHTNING_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn restore_lightning_steamless(
    app_id: u32,
    relative_executable: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let executable = resolve_game_relative_executable(app_id, &relative_executable)?;
        crate::steamless::steamless_restore(
            executable.to_string_lossy().to_string(),
            Some(".org.exe".to_string()),
        )
    })
    .await
    .map_err(|error| format!("LIGHTNING_TASK_FAILED: {error}"))?
}

fn catalog_for_kind(kind: &str) -> Result<LightningCatalogResponse, String> {
    match normalize_kind(kind)? {
        "bypass" => parse_bypass_catalog(),
        "onlineFix" => parse_online_fix_catalog(),
        "store" => parse_store_catalog(),
        _ => unreachable!(),
    }
}

fn normalize_kind(kind: &str) -> Result<&'static str, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "bypass" => Ok("bypass"),
        "onlinefix" | "online-fix" | "fix" => Ok("onlineFix"),
        "store" | "partnerstore" | "partner-store" => Ok("store"),
        _ => Err("LIGHTNING_CATALOG_KIND_INVALID".to_string()),
    }
}

fn parse_partner_json(source: &str, error_code: &str) -> Result<Value, String> {
    match serde_json::from_str(source) {
        Ok(value) => Ok(value),
        Err(_) => {
            let sanitized = strip_trailing_json_commas(source);
            serde_json::from_str(&sanitized).map_err(|error| format!("{error_code}: {error}"))
        }
    }
}

fn strip_trailing_json_commas(source: &str) -> String {
    let characters = source.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(source.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push('"');
            index += 1;
            continue;
        }
        if character == ',' {
            let mut next = index + 1;
            while next < characters.len() && characters[next].is_ascii_whitespace() {
                next += 1;
            }
            if next < characters.len() && matches!(characters[next], '}' | ']') {
                index += 1;
                continue;
            }
        }
        output.push(character);
        index += 1;
    }
    output
}

fn parse_bypass_catalog() -> Result<LightningCatalogResponse, String> {
    let root = parse_partner_json(BYPASS_CATALOG, "LIGHTNING_BYPASS_CATALOG_INVALID")?;
    let categories = root
        .as_object()
        .ok_or_else(|| "LIGHTNING_BYPASS_CATALOG_INVALID".to_string())?;
    let mut items = Vec::new();
    for (category, games) in categories {
        let games = games
            .as_object()
            .ok_or_else(|| "LIGHTNING_BYPASS_CATEGORY_INVALID".to_string())?;
        for (app_id, game) in games {
            let app_id = parse_app_id(app_id)?;
            let object = game
                .as_object()
                .ok_or_else(|| "LIGHTNING_BYPASS_ITEM_INVALID".to_string())?;
            let custom = object.get("custom_images").and_then(Value::as_object);
            items.push(LightningCatalogItem {
                kind: "bypass".to_string(),
                app_id,
                category: Some(category.clone()),
                name: required_string(object.get("name"), "LIGHTNING_BYPASS_NAME_INVALID")?,
                package_name: optional_string(object.get("nombre_fix")),
                image_url: custom.and_then(|images| optional_string(images.get("hero_image"))),
                background_url: custom.and_then(|images| optional_string(images.get("background"))),
                logo_url: None,
                dependencies: string_array(object.get("programas_necesarios")),
                instructions: string_array(object.get("errores")),
                note: optional_string(object.get("comentarios")),
                launch_with_steam: object
                    .get("launch_steam")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                launch_executable: object
                    .get("launch_exe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                active: true,
                regular_price: None,
                supporter_price: None,
                discount: None,
                source_repository: "LightnigFast/gamesFixes".to_string(),
            });
        }
    }
    items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    let mut category_names = categories.keys().cloned().collect::<Vec<_>>();
    category_names.sort();
    Ok(LightningCatalogResponse {
        kind: "bypass".to_string(),
        revision: content_sha256(BYPASS_CATALOG.as_bytes()),
        categories: category_names,
        items,
    })
}

fn parse_online_fix_catalog() -> Result<LightningCatalogResponse, String> {
    let root = parse_partner_json(ONLINE_FIX_CATALOG, "LIGHTNING_ONLINE_FIX_CATALOG_INVALID")?;
    let games = root
        .as_object()
        .ok_or_else(|| "LIGHTNING_ONLINE_FIX_CATALOG_INVALID".to_string())?;
    let mut items = Vec::with_capacity(games.len());
    for (app_id, game) in games {
        let object = game
            .as_object()
            .ok_or_else(|| "LIGHTNING_ONLINE_FIX_ITEM_INVALID".to_string())?;
        items.push(LightningCatalogItem {
            kind: "onlineFix".to_string(),
            app_id: parse_app_id(app_id)?,
            category: None,
            name: required_string(object.get("name"), "LIGHTNING_ONLINE_FIX_NAME_INVALID")?,
            package_name: optional_string(object.get("nombre_fix")),
            image_url: optional_string(object.get("custom_images")),
            background_url: None,
            logo_url: None,
            dependencies: Vec::new(),
            instructions: Vec::new(),
            note: None,
            launch_with_steam: true,
            launch_executable: false,
            active: true,
            regular_price: None,
            supporter_price: None,
            discount: None,
            source_repository: "LightnigFast/onlineFixes".to_string(),
        });
    }
    items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(LightningCatalogResponse {
        kind: "onlineFix".to_string(),
        revision: content_sha256(ONLINE_FIX_CATALOG.as_bytes()),
        categories: Vec::new(),
        items,
    })
}

fn parse_store_catalog() -> Result<LightningCatalogResponse, String> {
    let root = parse_partner_json(PARTNER_STORE_CATALOG, "LIGHTNING_STORE_CATALOG_INVALID")?;
    let games = root
        .as_object()
        .ok_or_else(|| "LIGHTNING_STORE_CATALOG_INVALID".to_string())?;
    let mut items = Vec::with_capacity(games.len());
    for (app_id, game) in games {
        let object = game
            .as_object()
            .ok_or_else(|| "LIGHTNING_STORE_ITEM_INVALID".to_string())?;
        items.push(LightningCatalogItem {
            kind: "store".to_string(),
            app_id: parse_app_id(app_id)?,
            category: None,
            name: required_string(object.get("name"), "LIGHTNING_STORE_NAME_INVALID")?,
            package_name: None,
            image_url: optional_string(object.get("imgVertical")),
            background_url: optional_string(object.get("imgCabecera")),
            logo_url: optional_string(object.get("imgLogo")),
            dependencies: Vec::new(),
            instructions: Vec::new(),
            note: None,
            launch_with_steam: false,
            launch_executable: false,
            active: object
                .get("activo")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("true")),
            regular_price: optional_string(object.get("precioNormal")),
            supporter_price: optional_string(object.get("precioDonadores")),
            discount: optional_string(object.get("descuento")),
            source_repository: "LightnigFast/Project-Lightning".to_string(),
        });
    }
    items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(LightningCatalogResponse {
        kind: "store".to_string(),
        revision: content_sha256(PARTNER_STORE_CATALOG.as_bytes()),
        categories: Vec::new(),
        items,
    })
}

fn apply_package_blocking(
    app: &AppHandle,
    request: LightningPackageRequest,
) -> Result<LightningPackageResult, String> {
    let kind = normalize_kind(&request.kind)?;
    if kind == "store" {
        return Err("LIGHTNING_STORE_ITEM_HAS_NO_PACKAGE".to_string());
    }
    let request_id = Uuid::parse_str(request.request_id.trim())
        .map_err(|_| "LIGHTNING_REQUEST_ID_INVALID".to_string())?
        .to_string();
    let item = catalog_for_kind(kind)?
        .items
        .into_iter()
        .find(|item| item.app_id == request.app_id)
        .ok_or_else(|| "LIGHTNING_ITEM_NOT_FOUND".to_string())?;
    let install_dir = canonical_install_dir(Path::new(request.install_dir.trim()))?;
    verify_install_identity(&install_dir, request.app_id, &item)?;

    let lock = APPLY_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "LIGHTNING_APPLY_LOCK_POISONED".to_string())?;
    let transaction_root = install_dir
        .join(".0xolemon")
        .join("lightning-transactions")
        .join(&request_id);
    let download_root = transaction_root.join("download");
    let payload_root = transaction_root.join("payload");
    let backup_root = transaction_root.join("backup");
    fs::create_dir_all(&download_root)
        .and_then(|_| fs::create_dir_all(&payload_root))
        .and_then(|_| fs::create_dir_all(&backup_root))
        .map_err(|error| format!("LIGHTNING_STAGE_CREATE_FAILED: {error}"))?;

    let client = github_client()?;
    let source = resolve_package_source(&client, kind, request.app_id)?;
    let files = list_package_files(&client, &source)?;
    if files.is_empty() {
        return Err("LIGHTNING_PACKAGE_EMPTY".to_string());
    }
    if files.len() > MAX_PACKAGE_FILES {
        return Err("LIGHTNING_PACKAGE_TOO_MANY_FILES".to_string());
    }
    let package_sha256 = package_descriptor_sha256(&source, &files)?;
    if request.revision.trim() != source.reference {
        return Err("GAME_TOOLS_SOURCE_REVISION_CHANGED".to_string());
    }
    if normalize_package_sha256(&request.package_sha256)? != package_sha256 {
        return Err("GAME_TOOLS_PACKAGE_IDENTITY_CHANGED".to_string());
    }
    let total_bytes = files.iter().try_fold(0u64, |total, file| {
        total
            .checked_add(file.size)
            .ok_or_else(|| "LIGHTNING_PACKAGE_SIZE_OVERFLOW".to_string())
    })?;
    if total_bytes == 0 || total_bytes > MAX_PACKAGE_DOWNLOAD_BYTES {
        return Err("LIGHTNING_PACKAGE_DOWNLOAD_SIZE_INVALID".to_string());
    }
    let download_headroom = total_bytes.saturating_add(512 * 1024 * 1024);
    let available = fs2::free_space(&transaction_root)
        .map_err(|error| format!("LIGHTNING_DISK_SPACE_CHECK_FAILED: {error}"))?;
    if available < download_headroom {
        return Err(format!(
            "LIGHTNING_DISK_SPACE_INSUFFICIENT: required {download_headroom}, available {available}"
        ));
    }

    emit_progress(
        app,
        &request_id,
        request.app_id,
        "downloading",
        0,
        files.len(),
        0,
        total_bytes,
        None,
    );
    let mut downloaded_bytes = 0u64;
    for (index, remote) in files.iter().enumerate() {
        let destination = safe_join(&download_root, &remote.relative_path)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("LIGHTNING_STAGE_CREATE_FAILED: {error}"))?;
        }
        download_file(&client, remote, &destination)?;
        downloaded_bytes = downloaded_bytes.saturating_add(remote.size);
        emit_progress(
            app,
            &request_id,
            request.app_id,
            "downloading",
            index + 1,
            files.len(),
            downloaded_bytes,
            total_bytes,
            Some(remote.relative_path.to_string_lossy().to_string()),
        );
    }

    materialize_payload(&download_root, &payload_root, &files)?;
    let payload_files = collect_regular_files(&payload_root)?;
    if payload_files.is_empty() {
        return Err("LIGHTNING_PACKAGE_PAYLOAD_EMPTY".to_string());
    }
    emit_progress(
        app,
        &request_id,
        request.app_id,
        "committing",
        0,
        payload_files.len(),
        downloaded_bytes,
        total_bytes,
        None,
    );

    let receipt_path = transaction_root.join("receipt.json");
    let mut receipt = LightningTransactionReceipt {
        schema: 1,
        request_id: request_id.clone(),
        kind: kind.to_string(),
        app_id: request.app_id,
        source_repository: source.repository.to_string(),
        source_reference: source.reference.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        install_dir: install_dir.to_string_lossy().to_string(),
        committed: false,
        restored_at: None,
        files: Vec::new(),
    };
    write_json_durable(&receipt_path, &receipt)?;

    let commit_result = commit_payload(
        app,
        &request_id,
        request.app_id,
        &install_dir,
        &payload_root,
        &backup_root,
        &payload_files,
        &mut receipt,
        downloaded_bytes,
        total_bytes,
    );
    if let Err(error) = commit_result {
        return Err(error);
    }
    receipt.committed = true;
    write_json_durable(&receipt_path, &receipt)?;
    write_json_durable(
        &install_dir
            .join(".0xolemon")
            .join("lightning-transactions")
            .join("latest.json"),
        &serde_json::json!({
            "schema": 1,
            "requestId": request_id,
            "receiptPath": receipt_path.to_string_lossy(),
            "appId": request.app_id,
        }),
    )?;
    emit_progress(
        app,
        &receipt.request_id,
        request.app_id,
        "committed",
        receipt.files.len(),
        receipt.files.len(),
        downloaded_bytes,
        total_bytes,
        None,
    );

    Ok(LightningPackageResult {
        request_id: receipt.request_id,
        kind: receipt.kind,
        app_id: receipt.app_id,
        game_name: item.name,
        install_dir: receipt.install_dir,
        source_repository: receipt.source_repository,
        source_reference: receipt.source_reference,
        downloaded_bytes,
        applied_files: receipt.files.len(),
        backup_files: receipt
            .files
            .iter()
            .filter(|file| file.backup_path.is_some())
            .count(),
        receipt_path: receipt_path.to_string_lossy().to_string(),
    })
}

fn package_identity_blocking(kind: &str, app_id: u32) -> Result<LightningSourceIdentity, String> {
    let kind = normalize_kind(kind)?;
    if kind == "store" {
        return Err("GAME_TOOLS_STORE_ITEM_HAS_NO_PACKAGE".to_string());
    }
    if !catalog_for_kind(kind)?
        .items
        .iter()
        .any(|item| item.app_id == app_id)
    {
        return Err("GAME_TOOLS_ITEM_NOT_FOUND".to_string());
    }
    let client = github_client()?;
    let source = resolve_package_source(&client, kind, app_id)?;
    let files = list_package_files(&client, &source)?;
    if files.is_empty() || files.len() > MAX_PACKAGE_FILES {
        return Err("GAME_TOOLS_PACKAGE_FILE_SET_INVALID".to_string());
    }
    Ok(LightningSourceIdentity {
        repository: source.repository.to_string(),
        revision: source.reference.clone(),
        package_sha256: package_descriptor_sha256(&source, &files)?,
    })
}

fn restore_latest_blocking(app: &AppHandle, app_id: u32) -> Result<LightningPackageResult, String> {
    let install_dir = crate::steam_integration::get_steam_game_install_dir(app_id)
        .map(PathBuf::from)
        .ok_or_else(|| "LIGHTNING_STEAM_GAME_NOT_INSTALLED".to_string())?;
    validate_install_dir(&install_dir)?;
    let latest_path = install_dir
        .join(".0xolemon")
        .join("lightning-transactions")
        .join("latest.json");
    let latest: Value = serde_json::from_slice(
        &fs::read(&latest_path).map_err(|error| format!("LIGHTNING_RECEIPT_NOT_FOUND: {error}"))?,
    )
    .map_err(|error| format!("LIGHTNING_RECEIPT_INVALID: {error}"))?;
    let receipt_path = latest
        .get("receiptPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "LIGHTNING_RECEIPT_INVALID".to_string())?;
    if !receipt_path.starts_with(install_dir.join(".0xolemon").join("lightning-transactions")) {
        return Err("LIGHTNING_RECEIPT_PATH_INVALID".to_string());
    }
    let mut receipt: LightningTransactionReceipt = serde_json::from_slice(
        &fs::read(&receipt_path)
            .map_err(|error| format!("LIGHTNING_RECEIPT_NOT_FOUND: {error}"))?,
    )
    .map_err(|error| format!("LIGHTNING_RECEIPT_INVALID: {error}"))?;
    if receipt.app_id != app_id || !receipt.committed || receipt.restored_at.is_some() {
        return Err("LIGHTNING_RECEIPT_NOT_RESTORABLE".to_string());
    }
    let transaction_root = receipt_path
        .parent()
        .ok_or_else(|| "LIGHTNING_RECEIPT_PATH_INVALID".to_string())?;
    let backup_root = transaction_root.join("backup");
    let mut changes = Vec::with_capacity(receipt.files.len());
    for file in &receipt.files {
        let relative = safe_relative_path(Path::new(&file.relative_path))?;
        let target = safe_join(&install_dir, &relative)?;
        if file.backup_path.is_some() {
            let source = safe_join(&backup_root, &relative)?;
            let expected_sha256 = match file.backup_sha256.as_deref() {
                Some(value) => normalize_package_sha256(value)?,
                None => sha256_file(&source)?,
            };
            changes.push(crate::managed_file_transaction::ManagedFileChange::Replace(
                crate::managed_file_transaction::ManagedFileSpec {
                    source,
                    target,
                    allowed_source_root: backup_root.clone(),
                    allowed_target_root: install_dir.clone(),
                    expected_sha256,
                },
            ));
        } else {
            changes.push(crate::managed_file_transaction::ManagedFileChange::Delete(
                crate::managed_file_transaction::ManagedDeleteSpec {
                    target,
                    allowed_target_root: install_dir.clone(),
                },
            ));
        }
    }
    crate::managed_file_transaction::apply_changes(
        app,
        &format!("game-tools-restore-{app_id}"),
        changes,
    )?;
    receipt.restored_at = Some(chrono::Utc::now().to_rfc3339());
    write_json_durable(&receipt_path, &receipt)?;
    Ok(LightningPackageResult {
        request_id: receipt.request_id,
        kind: receipt.kind,
        app_id,
        game_name: format!("Steam App {app_id}"),
        install_dir: receipt.install_dir,
        source_repository: receipt.source_repository,
        source_reference: receipt.source_reference,
        downloaded_bytes: 0,
        applied_files: receipt.files.len(),
        backup_files: receipt
            .files
            .iter()
            .filter(|file| file.backup_path.is_some())
            .count(),
        receipt_path: receipt_path.to_string_lossy().to_string(),
    })
}

fn read_latest_package_status(
    app_id: u32,
    install_dir: &Path,
) -> Result<Option<LightningAppliedPackageStatus>, String> {
    let transaction_root = install_dir.join(".0xolemon").join("lightning-transactions");
    let latest_path = transaction_root.join("latest.json");
    if !latest_path.is_file() {
        return Ok(None);
    }

    let latest: Value = serde_json::from_slice(
        &fs::read(&latest_path).map_err(|error| format!("LIGHTNING_RECEIPT_NOT_FOUND: {error}"))?,
    )
    .map_err(|error| format!("LIGHTNING_RECEIPT_INVALID: {error}"))?;
    let receipt_path = latest
        .get("receiptPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "LIGHTNING_RECEIPT_INVALID".to_string())?;
    let canonical_root = fs::canonicalize(&transaction_root)
        .map_err(|error| format!("LIGHTNING_RECEIPT_PATH_INVALID: {error}"))?;
    let canonical_receipt = fs::canonicalize(&receipt_path)
        .map_err(|error| format!("LIGHTNING_RECEIPT_NOT_FOUND: {error}"))?;
    if !canonical_receipt.starts_with(&canonical_root) || !canonical_receipt.is_file() {
        return Err("LIGHTNING_RECEIPT_PATH_INVALID".to_string());
    }

    let receipt: LightningTransactionReceipt = serde_json::from_slice(
        &fs::read(&canonical_receipt)
            .map_err(|error| format!("LIGHTNING_RECEIPT_NOT_FOUND: {error}"))?,
    )
    .map_err(|error| format!("LIGHTNING_RECEIPT_INVALID: {error}"))?;
    if receipt.app_id != app_id {
        return Err("LIGHTNING_RECEIPT_APP_ID_MISMATCH".to_string());
    }

    Ok(Some(LightningAppliedPackageStatus {
        request_id: receipt.request_id,
        kind: receipt.kind,
        app_id: receipt.app_id,
        source_repository: receipt.source_repository,
        source_reference: receipt.source_reference,
        installed_at: receipt.installed_at,
        committed: receipt.committed,
        restored_at: receipt.restored_at,
        applied_files: receipt.files.len(),
        backup_files: receipt
            .files
            .iter()
            .filter(|file| file.backup_path.is_some())
            .count(),
    }))
}

fn list_game_executables_blocking(app_id: u32) -> Result<Vec<LightningExecutable>, String> {
    const MAX_SCAN_DEPTH: usize = 6;
    const MAX_SCAN_ENTRIES: usize = 20_000;
    const MAX_EXECUTABLES: usize = 256;

    let install_dir = crate::steam_integration::get_steam_game_install_dir(app_id)
        .map(PathBuf::from)
        .ok_or_else(|| "LIGHTNING_STEAM_GAME_NOT_INSTALLED".to_string())?;
    validate_install_dir(&install_dir)?;
    let canonical_root = fs::canonicalize(&install_dir)
        .map_err(|error| format!("LIGHTNING_INSTALL_DIR_INVALID: {error}"))?;
    let mut pending = vec![(canonical_root.clone(), 0usize)];
    let mut scanned_entries = 0usize;
    let mut output = Vec::new();

    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("LIGHTNING_EXECUTABLE_SCAN_FAILED: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("LIGHTNING_EXECUTABLE_SCAN_FAILED: {error}"))?;
            scanned_entries += 1;
            if scanned_entries > MAX_SCAN_ENTRIES {
                return Err("LIGHTNING_EXECUTABLE_SCAN_LIMIT_EXCEEDED".to_string());
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("LIGHTNING_EXECUTABLE_SCAN_FAILED: {error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if depth < MAX_SCAN_DEPTH
                    && !entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(".0xolemon")
                {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                continue;
            }
            let relative = path
                .strip_prefix(&canonical_root)
                .map_err(|_| "LIGHTNING_EXECUTABLE_PATH_INVALID".to_string())?;
            let relative = safe_relative_path(relative)?;
            let size = entry
                .metadata()
                .map_err(|error| format!("LIGHTNING_EXECUTABLE_SCAN_FAILED: {error}"))?
                .len();
            output.push(LightningExecutable {
                relative_path: relative.to_string_lossy().replace('\\', "/"),
                size,
                patched: crate::steamless::steamless_status(
                    path.to_string_lossy().to_string(),
                    Some(".org.exe".to_string()),
                ),
            });
            if output.len() >= MAX_EXECUTABLES {
                break;
            }
        }
        if output.len() >= MAX_EXECUTABLES {
            break;
        }
    }

    output.sort_by(|left, right| {
        left.relative_path
            .to_ascii_lowercase()
            .cmp(&right.relative_path.to_ascii_lowercase())
    });
    Ok(output)
}

fn resolve_game_relative_executable(
    app_id: u32,
    relative_executable: &str,
) -> Result<PathBuf, String> {
    let install_dir = crate::steam_integration::get_steam_game_install_dir(app_id)
        .map(PathBuf::from)
        .ok_or_else(|| "LIGHTNING_STEAM_GAME_NOT_INSTALLED".to_string())?;
    validate_install_dir(&install_dir)?;
    let relative = safe_relative_path(Path::new(relative_executable))?;
    if !relative
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("LIGHTNING_EXECUTABLE_EXTENSION_INVALID".to_string());
    }
    let candidate = install_dir.join(relative);
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|error| format!("LIGHTNING_EXECUTABLE_NOT_FOUND: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("LIGHTNING_EXECUTABLE_PATH_INVALID".to_string());
    }
    let canonical_root = fs::canonicalize(&install_dir)
        .map_err(|error| format!("LIGHTNING_INSTALL_DIR_INVALID: {error}"))?;
    let canonical_candidate = fs::canonicalize(&candidate)
        .map_err(|error| format!("LIGHTNING_EXECUTABLE_NOT_FOUND: {error}"))?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err("LIGHTNING_EXECUTABLE_PATH_INVALID".to_string());
    }
    Ok(canonical_candidate)
}

fn github_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30 * 60))
        .user_agent("0xoLemon-Launcher/2 Project-Lightning-partner-integration")
        .build()
        .map_err(|error| format!("LIGHTNING_HTTP_CLIENT_FAILED: {error}"))
}

fn github_request(client: &Client, url: String) -> RequestBuilder {
    let request = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    match std::env::var("OXO_LIGHTNING_GITHUB_TOKEN") {
        Ok(token) if !token.trim().is_empty() => request.bearer_auth(token),
        _ => request,
    }
}

fn resolve_package_source(
    client: &Client,
    kind: &str,
    app_id: u32,
) -> Result<PackageSource, String> {
    if kind == "onlineFix" {
        return Ok(PackageSource {
            repository: "LightnigFast/onlineFixes",
            reference: resolve_branch_commit(client, "LightnigFast/onlineFixes", "main")?,
            content_path: app_id.to_string(),
        });
    }
    let repository = "LightnigFast/gamesFixes";
    let branch_url = format!("https://api.github.com/repos/{repository}/branches/{app_id}");
    let branch_response = github_request(client, branch_url)
        .send()
        .map_err(|error| format!("LIGHTNING_SOURCE_LOOKUP_FAILED: {error}"))?;
    if branch_response.status().is_success() {
        let branch: GithubBranch = branch_response
            .json()
            .map_err(|error| format!("LIGHTNING_SOURCE_LOOKUP_INVALID: {error}"))?;
        Ok(PackageSource {
            repository,
            reference: normalize_git_commit(&branch.commit.sha)?,
            content_path: String::new(),
        })
    } else if branch_response.status().as_u16() == 404 {
        Ok(PackageSource {
            repository,
            reference: resolve_branch_commit(client, repository, "main")?,
            content_path: app_id.to_string(),
        })
    } else if matches!(branch_response.status().as_u16(), 401 | 403) {
        Err("LIGHTNING_SOURCE_AUTH_REQUIRED".to_string())
    } else {
        Err(format!(
            "LIGHTNING_SOURCE_LOOKUP_FAILED: HTTP {}",
            branch_response.status()
        ))
    }
}

fn resolve_branch_commit(
    client: &Client,
    repository: &str,
    branch: &str,
) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{repository}/branches/{}",
        urlencoding::encode(branch)
    );
    let response = github_request(client, url)
        .send()
        .map_err(|error| format!("LIGHTNING_SOURCE_LOOKUP_FAILED: {error}"))?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err("LIGHTNING_SOURCE_AUTH_REQUIRED".to_string());
    }
    let branch: GithubBranch = response
        .error_for_status()
        .map_err(|error| format!("LIGHTNING_SOURCE_LOOKUP_FAILED: {error}"))?
        .json()
        .map_err(|error| format!("LIGHTNING_SOURCE_LOOKUP_INVALID: {error}"))?;
    normalize_git_commit(&branch.commit.sha)
}

fn normalize_git_commit(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("LIGHTNING_SOURCE_COMMIT_INVALID".to_string());
    }
    Ok(value)
}

fn list_package_files(client: &Client, source: &PackageSource) -> Result<Vec<GithubFile>, String> {
    let mut files = Vec::new();
    collect_github_files(
        client,
        source,
        &source.content_path,
        Path::new(""),
        0,
        &mut files,
    )?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn collect_github_files(
    client: &Client,
    source: &PackageSource,
    api_path: &str,
    relative_root: &Path,
    depth: usize,
    output: &mut Vec<GithubFile>,
) -> Result<(), String> {
    if depth > MAX_GITHUB_DEPTH || output.len() > MAX_PACKAGE_FILES {
        return Err("LIGHTNING_PACKAGE_TREE_TOO_LARGE".to_string());
    }
    let encoded_path = api_path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/");
    let url = if encoded_path.is_empty() {
        format!(
            "https://api.github.com/repos/{}/contents?ref={}",
            source.repository,
            urlencoding::encode(&source.reference)
        )
    } else {
        format!(
            "https://api.github.com/repos/{}/contents/{}?ref={}",
            source.repository,
            encoded_path,
            urlencoding::encode(&source.reference)
        )
    };
    let response = github_request(client, url)
        .send()
        .map_err(|error| format!("LIGHTNING_PACKAGE_LIST_FAILED: {error}"))?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err("LIGHTNING_SOURCE_AUTH_REQUIRED".to_string());
    }
    let response = response
        .error_for_status()
        .map_err(|error| format!("LIGHTNING_PACKAGE_LIST_FAILED: {error}"))?;
    let entries: Vec<GithubEntry> = response
        .json()
        .map_err(|error| format!("LIGHTNING_PACKAGE_LIST_INVALID: {error}"))?;
    for entry in entries {
        let name = safe_relative_path(Path::new(&entry.name))?;
        let relative_path = relative_root.join(name);
        match entry.entry_type.as_str() {
            "file" => {
                let download_url = entry
                    .download_url
                    .ok_or_else(|| "LIGHTNING_PACKAGE_DOWNLOAD_URL_MISSING".to_string())?;
                validate_partner_download_url(&download_url)?;
                let blob_sha = entry
                    .sha
                    .as_deref()
                    .ok_or_else(|| "LIGHTNING_PACKAGE_BLOB_SHA_MISSING".to_string())
                    .and_then(normalize_git_commit)?;
                output.push(GithubFile {
                    relative_path,
                    download_url,
                    size: entry.size,
                    blob_sha,
                });
            }
            "dir" => collect_github_files(
                client,
                source,
                &entry.path,
                &relative_path,
                depth + 1,
                output,
            )?,
            _ => return Err("LIGHTNING_PACKAGE_ENTRY_UNSUPPORTED".to_string()),
        }
    }
    Ok(())
}

fn download_file(client: &Client, remote: &GithubFile, destination: &Path) -> Result<(), String> {
    let partial = destination.with_extension(format!(
        "{}.part",
        destination
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
    ));
    let mut response = client
        .get(&remote.download_url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("LIGHTNING_PACKAGE_DOWNLOAD_FAILED: {error}"))?;
    if let Some(length) = response.content_length() {
        if length != remote.size || length > MAX_PACKAGE_DOWNLOAD_BYTES {
            return Err("LIGHTNING_PACKAGE_FILE_SIZE_INVALID".to_string());
        }
    }
    let mut file = File::create(&partial)
        .map_err(|error| format!("LIGHTNING_PACKAGE_STAGE_FAILED: {error}"))?;
    let copied = std::io::copy(&mut response, &mut file)
        .map_err(|error| format!("LIGHTNING_PACKAGE_DOWNLOAD_FAILED: {error}"))?;
    if copied != remote.size {
        return Err("LIGHTNING_PACKAGE_FILE_SIZE_MISMATCH".to_string());
    }
    file.sync_data()
        .map_err(|error| format!("LIGHTNING_PACKAGE_STAGE_FAILED: {error}"))?;
    drop(file);
    fs::rename(&partial, destination)
        .map_err(|error| format!("LIGHTNING_PACKAGE_STAGE_COMMIT_FAILED: {error}"))
}

fn materialize_payload(
    download_root: &Path,
    payload_root: &Path,
    files: &[GithubFile],
) -> Result<(), String> {
    for fragment in files
        .iter()
        .filter(|file| is_split_zip_fragment(&file.relative_path))
    {
        if !files.iter().any(|candidate| {
            is_zip_archive(&candidate.relative_path)
                && same_archive_stem(&candidate.relative_path, &fragment.relative_path)
        }) {
            return Err("LIGHTNING_SPLIT_ARCHIVE_PRIMARY_MISSING".to_string());
        }
    }

    let mut payload_paths = HashSet::new();
    let mut expanded_bytes = 0u64;
    let mut seven_zip: Option<PathBuf> = None;
    for remote in files {
        if is_split_zip_fragment(&remote.relative_path) {
            continue;
        }
        let source = safe_join(download_root, &remote.relative_path)?;
        let split_zip = is_zip_archive(&remote.relative_path)
            && files.iter().any(|candidate| {
                is_split_zip_fragment(&candidate.relative_path)
                    && same_archive_stem(&remote.relative_path, &candidate.relative_path)
            });
        if split_zip || is_external_archive(&remote.relative_path) {
            let executable = match &seven_zip {
                Some(path) => path.clone(),
                None => {
                    let path = crate::sff_packages::ensure_feature_package("seven-zip-cli")?;
                    seven_zip = Some(path.clone());
                    path
                }
            };
            extract_external_archive(
                &executable,
                &source,
                payload_root,
                &mut payload_paths,
                &mut expanded_bytes,
            )?;
            continue;
        }
        if is_zip_archive(&remote.relative_path) {
            extract_zip(
                &source,
                payload_root,
                &mut payload_paths,
                &mut expanded_bytes,
            )?;
            continue;
        }
        let key = normalized_relative_key(&remote.relative_path)?;
        if !payload_paths.insert(key) {
            return Err("LIGHTNING_PACKAGE_DUPLICATE_PATH".to_string());
        }
        expanded_bytes = expanded_bytes
            .checked_add(remote.size)
            .ok_or_else(|| "LIGHTNING_PACKAGE_EXPANDED_SIZE_OVERFLOW".to_string())?;
        if expanded_bytes > MAX_PACKAGE_EXPANDED_BYTES {
            return Err("LIGHTNING_PACKAGE_EXPANDED_SIZE_INVALID".to_string());
        }
        let destination = safe_join(payload_root, &remote.relative_path)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("LIGHTNING_PAYLOAD_CREATE_FAILED: {error}"))?;
        }
        fs::copy(&source, &destination)
            .map_err(|error| format!("LIGHTNING_PAYLOAD_COPY_FAILED: {error}"))?;
    }
    Ok(())
}

fn is_zip_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn is_external_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("7z") || extension.eq_ignore_ascii_case("rar")
        })
}

fn is_split_zip_fragment(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            let extension = extension.to_ascii_lowercase();
            extension.len() == 3
                && extension.starts_with('z')
                && extension[1..].chars().all(|value| value.is_ascii_digit())
        })
}

fn same_archive_stem(primary: &Path, fragment: &Path) -> bool {
    primary.parent() == fragment.parent()
        && primary
            .file_stem()
            .and_then(|value| value.to_str())
            .zip(fragment.file_stem().and_then(|value| value.to_str()))
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
}

#[derive(Debug)]
struct ExternalArchiveEntry {
    relative_path: PathBuf,
    size: u64,
}

fn extract_external_archive(
    seven_zip: &Path,
    archive_path: &Path,
    payload_root: &Path,
    payload_paths: &mut HashSet<String>,
    expanded_bytes: &mut u64,
) -> Result<(), String> {
    if !seven_zip.is_file() {
        return Err("LIGHTNING_7ZIP_ENTRYPOINT_MISSING".to_string());
    }
    let output = Command::new(seven_zip)
        .args(["l", "-slt", "-ba", "-sccUTF-8"])
        .arg(archive_path)
        .output()
        .map_err(|error| format!("LIGHTNING_ARCHIVE_LIST_FAILED: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "LIGHTNING_ARCHIVE_LIST_FAILED: {}",
            bounded_command_error(&output.stderr)
        ));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| "LIGHTNING_ARCHIVE_LIST_INVALID_UTF8".to_string())?;
    let entries = parse_external_archive_listing(&listing)?;
    if entries.is_empty() {
        return Err("LIGHTNING_ARCHIVE_EMPTY".to_string());
    }
    if payload_paths.len().saturating_add(entries.len()) > MAX_PACKAGE_FILES {
        return Err("LIGHTNING_ARCHIVE_TOO_MANY_FILES".to_string());
    }

    let mut archive_expanded = 0u64;
    for entry in &entries {
        let key = normalized_relative_key(&entry.relative_path)?;
        if !payload_paths.insert(key) {
            return Err("LIGHTNING_PACKAGE_DUPLICATE_PATH".to_string());
        }
        archive_expanded = archive_expanded
            .checked_add(entry.size)
            .ok_or_else(|| "LIGHTNING_PACKAGE_EXPANDED_SIZE_OVERFLOW".to_string())?;
    }
    *expanded_bytes = expanded_bytes
        .checked_add(archive_expanded)
        .ok_or_else(|| "LIGHTNING_PACKAGE_EXPANDED_SIZE_OVERFLOW".to_string())?;
    if *expanded_bytes > MAX_PACKAGE_EXPANDED_BYTES {
        return Err("LIGHTNING_PACKAGE_EXPANDED_SIZE_INVALID".to_string());
    }
    ensure_archive_capacity(payload_root, archive_expanded)?;

    let output_directory = format!("-o{}", payload_root.display());
    let output = Command::new(seven_zip)
        .arg("x")
        .arg(archive_path)
        .arg(output_directory)
        .args(["-y", "-bb0", "-bd", "-bso0", "-bsp0", "-sccUTF-8"])
        .current_dir(
            archive_path
                .parent()
                .ok_or_else(|| "LIGHTNING_ARCHIVE_PARENT_INVALID".to_string())?,
        )
        .output()
        .map_err(|error| format!("LIGHTNING_ARCHIVE_EXTRACT_FAILED: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "LIGHTNING_ARCHIVE_EXTRACT_FAILED: {}",
            bounded_command_error(&output.stderr)
        ));
    }

    for entry in entries {
        let extracted = safe_join(payload_root, &entry.relative_path)?;
        let metadata = fs::metadata(&extracted)
            .map_err(|error| format!("LIGHTNING_ARCHIVE_OUTPUT_MISSING: {error}"))?;
        if !metadata.is_file() || metadata.len() != entry.size {
            return Err("LIGHTNING_ARCHIVE_OUTPUT_INVALID".to_string());
        }
    }
    Ok(())
}

fn parse_external_archive_listing(listing: &str) -> Result<Vec<ExternalArchiveEntry>, String> {
    let normalized = listing.replace("\r\n", "\n");
    let mut entries = Vec::new();
    for record in normalized.split("\n\n") {
        let mut path = None;
        let mut size = None;
        let mut folder = false;
        let mut link = false;
        for line in record.lines() {
            let Some((key, value)) = line.split_once(" = ") else {
                continue;
            };
            match key.trim() {
                "Path" => path = Some(value.trim()),
                "Size" => {
                    size = Some(
                        value
                            .trim()
                            .parse::<u64>()
                            .map_err(|_| "LIGHTNING_ARCHIVE_LIST_SIZE_INVALID".to_string())?,
                    )
                }
                "Folder" => folder = value.trim() == "+",
                "Symbolic Link" | "Hard Link" => link = !value.trim().is_empty(),
                _ => {}
            }
        }
        if link {
            return Err("LIGHTNING_ARCHIVE_LINK_REJECTED".to_string());
        }
        let (Some(path), Some(size)) = (path, size) else {
            continue;
        };
        if folder {
            continue;
        }
        entries.push(ExternalArchiveEntry {
            relative_path: safe_relative_path(Path::new(path))?,
            size,
        });
    }
    Ok(entries)
}

fn ensure_archive_capacity(payload_root: &Path, expanded_bytes: u64) -> Result<(), String> {
    let required = expanded_bytes
        .saturating_mul(2)
        .saturating_add(256 * 1024 * 1024);
    let available = fs2::free_space(payload_root)
        .map_err(|error| format!("LIGHTNING_DISK_SPACE_CHECK_FAILED: {error}"))?;
    if available < required {
        return Err(format!(
            "LIGHTNING_DISK_SPACE_INSUFFICIENT: required {required}, available {available}"
        ));
    }
    Ok(())
}

fn bounded_command_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.chars().take(2_048).collect::<String>()
}

fn extract_zip(
    archive_path: &Path,
    payload_root: &Path,
    payload_paths: &mut HashSet<String>,
    expanded_bytes: &mut u64,
) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("LIGHTNING_ARCHIVE_OPEN_FAILED: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("LIGHTNING_ARCHIVE_INVALID: {error}"))?;
    if archive.len() > MAX_PACKAGE_FILES {
        return Err("LIGHTNING_ARCHIVE_TOO_MANY_FILES".to_string());
    }
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("LIGHTNING_ARCHIVE_INVALID: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("LIGHTNING_ARCHIVE_SYMLINK_REJECTED".to_string());
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "LIGHTNING_ARCHIVE_PATH_INVALID".to_string())?;
        let relative = safe_relative_path(&relative)?;
        let key = normalized_relative_key(&relative)?;
        if !payload_paths.insert(key) {
            return Err("LIGHTNING_PACKAGE_DUPLICATE_PATH".to_string());
        }
        *expanded_bytes = expanded_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "LIGHTNING_PACKAGE_EXPANDED_SIZE_OVERFLOW".to_string())?;
        if *expanded_bytes > MAX_PACKAGE_EXPANDED_BYTES {
            return Err("LIGHTNING_PACKAGE_EXPANDED_SIZE_INVALID".to_string());
        }
        let destination = safe_join(payload_root, &relative)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("LIGHTNING_PAYLOAD_CREATE_FAILED: {error}"))?;
        }
        let mut output = File::create(&destination)
            .map_err(|error| format!("LIGHTNING_PAYLOAD_CREATE_FAILED: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("LIGHTNING_ARCHIVE_EXTRACT_FAILED: {error}"))?;
        output
            .sync_data()
            .map_err(|error| format!("LIGHTNING_PAYLOAD_SYNC_FAILED: {error}"))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_payload(
    app: &AppHandle,
    request_id: &str,
    app_id: u32,
    install_dir: &Path,
    payload_root: &Path,
    backup_root: &Path,
    payload_files: &[PathBuf],
    receipt: &mut LightningTransactionReceipt,
    downloaded_bytes: u64,
    total_bytes: u64,
) -> Result<(), String> {
    let mut specs = Vec::with_capacity(payload_files.len());
    for (index, source) in payload_files.iter().enumerate() {
        let relative = source
            .strip_prefix(payload_root)
            .map_err(|_| "LIGHTNING_PAYLOAD_PATH_INVALID".to_string())?;
        let relative = safe_relative_path(relative)?;
        let target = safe_join(install_dir, &relative)?;
        let backup = safe_join(backup_root, &relative)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("LIGHTNING_TARGET_CREATE_FAILED: {error}"))?;
        }
        let (backup_path, backup_sha256) = if target.is_file() {
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("LIGHTNING_BACKUP_CREATE_FAILED: {error}"))?;
            }
            let expected = sha256_file(&target)?;
            fs::copy(&target, &backup)
                .map_err(|error| format!("LIGHTNING_BACKUP_FAILED: {error}"))?;
            if sha256_file(&backup)? != expected || sha256_file(&target)? != expected {
                return Err("LIGHTNING_BACKUP_HASH_MISMATCH".to_string());
            }
            (Some(relative.to_string_lossy().to_string()), Some(expected))
        } else if target.exists() {
            return Err("LIGHTNING_TARGET_TYPE_CONFLICT".to_string());
        } else {
            (None, None)
        };
        let metadata = fs::metadata(source)
            .map_err(|error| format!("LIGHTNING_PAYLOAD_METADATA_FAILED: {error}"))?;
        let sha256 = sha256_file(source)?;
        receipt.files.push(LightningAppliedFile {
            relative_path: relative.to_string_lossy().to_string(),
            backup_path,
            backup_sha256,
            sha256,
            size: metadata.len(),
        });
        write_json_durable(
            &install_dir
                .join(".0xolemon")
                .join("lightning-transactions")
                .join(request_id)
                .join("receipt.json"),
            receipt,
        )?;
        specs.push(crate::managed_file_transaction::ManagedFileSpec {
            source: source.clone(),
            target,
            allowed_source_root: payload_root.to_path_buf(),
            allowed_target_root: install_dir.to_path_buf(),
            expected_sha256: receipt
                .files
                .last()
                .map(|file| file.sha256.clone())
                .ok_or_else(|| "LIGHTNING_RECEIPT_FILE_MISSING".to_string())?,
        });
        emit_progress(
            app,
            request_id,
            app_id,
            "committing",
            index + 1,
            payload_files.len(),
            downloaded_bytes,
            total_bytes,
            Some(relative.to_string_lossy().to_string()),
        );
    }
    crate::managed_file_transaction::apply_files(
        app,
        &format!("game-tools-apply-{app_id}-{request_id}"),
        specs,
    )?;
    Ok(())
}

fn collect_regular_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut output = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("LIGHTNING_PAYLOAD_READ_FAILED: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("LIGHTNING_PAYLOAD_READ_FAILED: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("LIGHTNING_PAYLOAD_READ_FAILED: {error}"))?;
            if file_type.is_symlink() {
                return Err("LIGHTNING_PAYLOAD_SYMLINK_REJECTED".to_string());
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                output.push(entry.path());
            } else {
                return Err("LIGHTNING_PAYLOAD_ENTRY_UNSUPPORTED".to_string());
            }
            if output.len() > MAX_PACKAGE_FILES {
                return Err("LIGHTNING_PACKAGE_TOO_MANY_FILES".to_string());
            }
        }
    }
    output.sort();
    Ok(output)
}

fn package_descriptor_sha256(
    source: &PackageSource,
    files: &[GithubFile],
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"0xolemon-game-tools-package-v1\0");
    hasher.update(source.repository.as_bytes());
    hasher.update(b"\0");
    hasher.update(source.reference.as_bytes());
    hasher.update(b"\0");
    hasher.update(source.content_path.as_bytes());
    hasher.update(b"\0");
    for file in files {
        let relative = normalized_relative_key(&file.relative_path)?;
        hasher.update(relative.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.size.to_le_bytes());
        hasher.update(b"\0");
        hasher.update(file.blob_sha.as_bytes());
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

fn normalize_package_sha256(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("GAME_TOOLS_PACKAGE_SHA256_INVALID".to_string());
    }
    Ok(value)
}

fn validate_install_dir(path: &Path) -> Result<(), String> {
    canonical_install_dir(path).map(|_| ())
}

fn canonical_install_dir(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("GAME_TOOLS_INSTALL_DIR_MUST_BE_ABSOLUTE".to_string());
    }
    reject_install_reparse_chain(path)?;
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_INVALID: {error}"))?;
    if !canonical.is_dir() || canonical.file_name().is_none() {
        return Err("GAME_TOOLS_INSTALL_DIR_INVALID".to_string());
    }

    if let Some(windows) = std::env::var_os("WINDIR").map(PathBuf::from) {
        if let Ok(windows) = windows.canonicalize() {
            if canonical == windows || canonical.starts_with(windows.join("System32")) {
                return Err("GAME_TOOLS_SYSTEM_DIR_REJECTED".to_string());
            }
        }
    }
    if let Ok(launcher) = std::env::current_exe() {
        if let Some(launcher_dir) = launcher
            .parent()
            .and_then(|value| value.canonicalize().ok())
        {
            if canonical == launcher_dir {
                return Err("GAME_TOOLS_LAUNCHER_DIR_REJECTED".to_string());
            }
        }
    }
    Ok(canonical)
}

fn reject_install_reparse_chain(path: &Path) -> Result<(), String> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        if !cursor.exists() {
            return Err("GAME_TOOLS_INSTALL_DIR_INVALID".to_string());
        }
        let metadata = fs::symlink_metadata(&cursor)
            .map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_INVALID: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("GAME_TOOLS_INSTALL_DIR_REPARSE_REJECTED".to_string());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("GAME_TOOLS_INSTALL_DIR_REPARSE_REJECTED".to_string());
            }
        }
    }
    Ok(())
}

fn verify_install_identity(
    install_dir: &Path,
    app_id: u32,
    item: &LightningCatalogItem,
) -> Result<(), String> {
    if item.app_id != app_id {
        return Err("GAME_TOOLS_APP_ID_MISMATCH".to_string());
    }
    let canonical = canonical_install_dir(install_dir)?;
    if let Some(steam_dir) = crate::steam_integration::get_steam_game_install_dir(app_id) {
        if PathBuf::from(steam_dir)
            .canonicalize()
            .is_ok_and(|steam| steam == canonical)
        {
            return Ok(());
        }
    }

    let app_id_file = canonical.join("steam_appid.txt");
    if app_id_file.is_file() {
        let value = fs::read_to_string(&app_id_file)
            .map_err(|error| format!("GAME_TOOLS_APP_ID_READ_FAILED: {error}"))?;
        let parsed = value
            .trim()
            .parse::<u32>()
            .map_err(|_| "GAME_TOOLS_APP_ID_FILE_INVALID".to_string())?;
        if parsed != app_id {
            return Err("GAME_TOOLS_APP_ID_MISMATCH".to_string());
        }
        return Ok(());
    }

    if item.launch_executable && has_executable_hint(&canonical)? {
        return Ok(());
    }
    Err("GAME_TOOLS_INSTALL_IDENTITY_UNVERIFIED".to_string())
}

fn has_executable_hint(install_dir: &Path) -> Result<bool, String> {
    const HINT_DIRS: &[&str] = &["", "bin", "Binaries/Win32", "Binaries/Win64"];
    let mut inspected = 0usize;
    for hint in HINT_DIRS {
        let directory = if hint.is_empty() {
            install_dir.to_path_buf()
        } else {
            install_dir.join(hint)
        };
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_READ_FAILED: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_READ_FAILED: {error}"))?;
            inspected += 1;
            if inspected > 512 {
                return Err("GAME_TOOLS_INSTALL_DIR_TOO_LARGE_TO_VERIFY".to_string());
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("GAME_TOOLS_INSTALL_DIR_READ_FAILED: {error}"))?;
            if file_type.is_file()
                && !file_type.is_symlink()
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn validate_partner_download_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| "LIGHTNING_PACKAGE_DOWNLOAD_URL_INVALID".to_string())?;
    if parsed.scheme() != "https" {
        return Err("LIGHTNING_PACKAGE_DOWNLOAD_URL_INVALID".to_string());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(
        host.as_str(),
        "raw.githubusercontent.com"
            | "github.com"
            | "objects.githubusercontent.com"
            | "github-releases.githubusercontent.com"
    ) {
        return Err("LIGHTNING_PACKAGE_DOWNLOAD_HOST_REJECTED".to_string());
    }
    Ok(())
}

fn safe_relative_path(path: &Path) -> Result<PathBuf, String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized.contains('\0')
    {
        return Err("LIGHTNING_RELATIVE_PATH_INVALID".to_string());
    }
    let mut output = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.ends_with([' ', '.'])
            || is_windows_device_name(segment)
        {
            return Err("LIGHTNING_RELATIVE_PATH_INVALID".to_string());
        }
        output.push(segment);
    }
    if output.as_os_str().is_empty()
        || output
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("LIGHTNING_RELATIVE_PATH_INVALID".to_string());
    }
    Ok(output)
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

fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    Ok(root.join(safe_relative_path(relative)?))
}

fn normalized_relative_key(path: &Path) -> Result<String, String> {
    Ok(safe_relative_path(path)?
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase())
}

fn parse_app_id(value: &str) -> Result<u32, String> {
    let app_id = value
        .parse::<u32>()
        .map_err(|_| "LIGHTNING_APP_ID_INVALID".to_string())?;
    if app_id == 0 {
        return Err("LIGHTNING_APP_ID_INVALID".to_string());
    }
    Ok(app_id)
}

fn required_string(value: Option<&Value>, code: &str) -> Result<String, String> {
    optional_string(value).ok_or_else(|| code.to_string())
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| optional_string(Some(item)))
                .collect()
        })
        .unwrap_or_default()
}

fn content_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("LIGHTNING_PAYLOAD_HASH_FAILED: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("LIGHTNING_PAYLOAD_HASH_FAILED: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn write_json_durable(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("LIGHTNING_JOURNAL_CREATE_FAILED: {error}"))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("LIGHTNING_JOURNAL_SERIALIZE_FAILED: {error}"))?;
    let mut file = File::create(&temporary)
        .map_err(|error| format!("LIGHTNING_JOURNAL_WRITE_FAILED: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("LIGHTNING_JOURNAL_WRITE_FAILED: {error}"))?;
    drop(file);
    let previous = path.with_extension("previous");
    let had_current = path.is_file();
    if had_current {
        if previous.exists() {
            if !previous.is_file() {
                return Err("LIGHTNING_JOURNAL_PREVIOUS_INVALID".to_string());
            }
            fs::remove_file(&previous)
                .map_err(|error| format!("LIGHTNING_JOURNAL_REPLACE_FAILED: {error}"))?;
        }
        fs::rename(path, &previous)
            .map_err(|error| format!("LIGHTNING_JOURNAL_REPLACE_FAILED: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if had_current && previous.is_file() {
            let _ = fs::rename(&previous, path);
        }
        return Err(format!("LIGHTNING_JOURNAL_REPLACE_FAILED: {error}"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app: &AppHandle,
    request_id: &str,
    app_id: u32,
    phase: &str,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    current_file: Option<String>,
) {
    let progress = LightningPackageProgress {
        request_id: request_id.to_string(),
        app_id,
        phase: phase.to_string(),
        files_done,
        files_total,
        bytes_done,
        bytes_total,
        current_file,
    };
    let _ = app.emit("launcher://game-tools-package-progress", progress.clone());
    let _ = app.emit("launcher://lightning-package-progress", progress);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalogs_are_normalized() {
        let bypass = parse_bypass_catalog().expect("bypass catalog");
        let online = parse_online_fix_catalog().expect("online fix catalog");
        let store = parse_store_catalog().expect("store catalog");
        assert!(bypass.items.len() >= 100);
        assert!(online.items.len() >= 100);
        assert_eq!(store.items.len(), 8);
        assert!(bypass.items.iter().all(|item| item.app_id > 0));
        assert!(online.items.iter().all(|item| item.app_id > 0));
    }

    #[test]
    fn partner_json_tolerates_only_structural_trailing_commas() {
        let source = r#"{"text":"keep,}","items":["đối tác",],}"#;
        let value = parse_partner_json(source, "INVALID").unwrap();
        assert_eq!(value["text"], "keep,}");
        assert_eq!(value["items"][0], "đối tác");
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        assert!(safe_relative_path(Path::new("../payload.dll")).is_err());
        assert!(safe_relative_path(Path::new("C:\\payload.dll")).is_err());
        assert!(safe_relative_path(Path::new("bin/NUL.dll")).is_err());
        assert!(safe_relative_path(Path::new("bin/payload.dll.")).is_err());
        assert!(safe_relative_path(Path::new("bin/payload.dll")).is_ok());
    }

    #[test]
    fn split_zip_fragments_match_only_their_primary_archive() {
        assert!(is_split_zip_fragment(Path::new("game.z01")));
        assert!(is_split_zip_fragment(Path::new("game.Z99")));
        assert!(!is_split_zip_fragment(Path::new("game.zip")));
        assert!(same_archive_stem(
            Path::new("payload/game.zip"),
            Path::new("payload/game.z01")
        ));
        assert!(!same_archive_stem(
            Path::new("payload/game.zip"),
            Path::new("payload/other.z01")
        ));
    }

    #[test]
    fn external_archive_listing_rejects_links_and_parses_files() {
        let listing = "Path = bin\\payload.dll\nSize = 42\nFolder = -\n\nPath = empty.txt\nSize = 0\nFolder = -\n";
        let entries = parse_external_archive_listing(listing).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].relative_path, PathBuf::from("bin/payload.dll"));
        assert_eq!(entries[0].size, 42);

        let link = "Path = escape.dll\nSize = 4\nFolder = -\nSymbolic Link = ..\\escape.dll\n";
        assert!(parse_external_archive_listing(link).is_err());
    }

    #[test]
    fn download_hosts_are_allowlisted() {
        assert!(validate_partner_download_url(
            "https://raw.githubusercontent.com/LightnigFast/onlineFixes/main/1/a.zip"
        )
        .is_ok());
        assert!(validate_partner_download_url("http://example.com/payload.zip").is_err());
        assert!(validate_partner_download_url("https://example.com/payload.zip").is_err());
    }
}
