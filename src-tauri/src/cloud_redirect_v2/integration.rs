use crate::cloud_redirect::steam_detector::{
    find_steam_path, get_steam_version, is_steam_running, is_supported_steam_version,
    shutdown_steam, steam_process_ids, SUPPORTED_STEAM_VERSIONS,
};
use chrono::Utc;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{command, AppHandle};

use super::{backup, engine, manifest_pinning};
use super::models::{
    DiagnosticItem, DiagnosticsReport, EngineStatus, LocalBackupInfo, ManifestPinConfig,
    SteamRuntimeState,
    ManifestPinConfigInput, MigrationEvent, MigrationRequest, OperationResult, ProviderConfigInput,
    ProviderConfigView, RemoteAppInfo, RemoteFileInfo, StatsEntry, ENGINE_SOURCE_COMMIT,
    ENGINE_VERSION,
};
use super::upstream_config;

#[command]
pub fn cloud_redirect_engine_get_status(app: AppHandle) -> Result<EngineStatus, String> {
    let steam = find_steam_path();
    let provider_view = upstream_config::get_provider_view().unwrap_or_default();
    let runtime = engine::ensure_runtime(&app);
    let authenticated = match provider_view.provider.as_str() {
        "local" => true,
        "folder" => provider_view.authenticated,
        provider if runtime.is_ok() => engine::run_cli_value(
            &app,
            &["auth-status".to_string(), provider.to_string()],
        )
        .ok()
        .and_then(|value| value.get("authenticated").and_then(Value::as_bool))
        .unwrap_or(false),
        _ => false,
    };
    let steam_path = steam.as_ref().map(|path| path.to_string_lossy().into_owned());
    let steam_process_ids = steam_process_ids();
    let steam_version = steam.as_ref().and_then(|path| get_steam_version(path));
    let steam_version_supported = steam_version.map(is_supported_steam_version).unwrap_or(false);
    let dll_installed = steam
        .as_ref()
        .is_some_and(|path| path.join("cloud_redirect.dll").is_file());
    let account_ids = steam
        .as_ref()
        .map(|path| discover_account_ids(path))
        .unwrap_or_default();

    Ok(EngineStatus {
        version: ENGINE_VERSION.to_string(),
        source_commit: ENGINE_SOURCE_COMMIT.to_string(),
        engine_ready: runtime.is_ok(),
        engine_dir: runtime.ok().map(|path| path.to_string_lossy().into_owned()),
        steam_path,
        steam_running: !steam_process_ids.is_empty(),
        steam_process_ids,
        steam_version,
        steam_version_supported,
        supported_steam_versions: SUPPORTED_STEAM_VERSIONS.to_vec(),
        dll_installed,
        mode: upstream_config::mode_from_settings(),
        provider: Some(provider_view.provider.clone()),
        provider_display_name: Some(upstream_config::provider_display_name(&provider_view.provider)),
        authenticated,
        token_path: provider_view.token_path,
        sync_path: provider_view.sync_path,
        account_ids,
        last_error: None,
        supported_providers: upstream_config::PROVIDERS
            .iter()
            .map(|provider| provider.to_string())
            .collect(),
    })
}

#[command]
pub fn cloud_redirect_engine_get_steam_state() -> SteamRuntimeState {
    let steam = find_steam_path();
    let process_ids = steam_process_ids();
    let version = steam.as_ref().and_then(|path| get_steam_version(path));
    SteamRuntimeState {
        running: !process_ids.is_empty(),
        process_ids,
        version,
        version_supported: version.map(is_supported_steam_version).unwrap_or(false),
        supported_versions: SUPPORTED_STEAM_VERSIONS.to_vec(),
    }
}

#[command]
pub fn cloud_redirect_engine_close_steam() -> Result<OperationResult, String> {
    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    if !is_steam_running() {
        return Ok(result_message("Steam is already closed"));
    }
    shutdown_steam(&steam);
    let remaining = steam_process_ids();
    if !remaining.is_empty() {
        return Err(format!(
            "Steam is still running in the background (PID{} {}). Exit Steam from the tray or end the remaining process and retry.",
            if remaining.len() == 1 { "" } else { "s" },
            remaining.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(result_message("Steam closed"))
}

#[command]
pub fn cloud_redirect_engine_install(app: AppHandle) -> Result<OperationResult, String> {
    let path = engine::install_dll(&app)?;
    Ok(result_message(format!("CloudRedirect {} installed at {}", ENGINE_VERSION, path.display())))
}

#[command]
pub fn cloud_redirect_engine_remove() -> Result<OperationResult, String> {
    engine::uninstall_dll()?;
    Ok(result_message("CloudRedirect DLL removed"))
}

#[command]
pub fn cloud_redirect_engine_run_required_patches(
    app: AppHandle,
    mode: String,
    install_core_if_missing: bool,
) -> Result<OperationResult, String> {
    upstream_config::save_mode(&mode)?;
    if let Some(steam) = find_steam_path() {
        if let Some(version) = get_steam_version(&steam) {
            if !is_supported_steam_version(version) {
                return Err(format!(
                    "Steam client {version} is newer than the bundled CloudRedirect engine can safely patch. \
                     Patching is blocked to protect the Steam installation. Bundled engine {ENGINE_VERSION} supports: {}. \
                     Install a CloudRedirect update that explicitly supports this Steam build before running patches.",
                    SUPPORTED_STEAM_VERSIONS.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    let result = crate::cloud_redirect::cloud_redirect_run_stfixer(install_core_if_missing);
    if !result.succeeded {
        return Err(result
            .error
            .unwrap_or_else(|| result.log.join("\n")));
    }
    let installed = engine::install_dll(&app)?;
    Ok(OperationResult {
        success: true,
        message: format!(
            "Required patches applied and CloudRedirect {} deployed to {}\n{}",
            ENGINE_VERSION,
            installed.display(),
            result.log.join("\n")
        ),
        ..OperationResult::default()
    })
}

#[command]
pub fn cloud_redirect_engine_get_provider() -> Result<ProviderConfigView, String> {
    upstream_config::get_provider_view()
}

#[command]
pub fn cloud_redirect_engine_save_provider(
    app: AppHandle,
    input: ProviderConfigInput,
) -> Result<ProviderConfigView, String> {
    let view = upstream_config::save_provider(&input)?;
    if matches!(view.provider.as_str(), "gdrive" | "onedrive" | "r2" | "s3") {
        let authenticated = engine::run_cli_value(
            &app,
            &["auth-status".to_string(), view.provider.clone()],
        )
        .ok()
        .and_then(|value| value.get("authenticated").and_then(Value::as_bool))
        .unwrap_or(false);
        let mut view = view;
        view.authenticated = authenticated;
        return Ok(view);
    }
    Ok(view)
}

#[command]
pub fn cloud_redirect_engine_set_mode(mode: String) -> Result<OperationResult, String> {
    upstream_config::save_mode(&mode)?;
    Ok(result_message(format!("CloudRedirect mode set to {mode}")))
}

#[command]
pub fn cloud_redirect_engine_test_provider(app: AppHandle, provider: String) -> Result<OperationResult, String> {
    if provider == "local" {
        return Ok(result_message("Local-only provider is ready"));
    }
    if provider == "folder" {
        let view = upstream_config::get_provider_view()?;
        let path = view.sync_path.ok_or_else(|| "Sync folder is not configured".to_string())?;
        if !Path::new(&path).is_dir() {
            return Err("Configured sync folder does not exist".to_string());
        }
        return Ok(result_message("Folder provider is ready"));
    }
    let value = engine::run_cli_value(&app, &["auth-status".to_string(), provider.clone()])?;
    let authenticated = value
        .get("authenticated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !authenticated {
        return Err(format!("{provider} is not authenticated"));
    }
    let mut raw = value;
    if let Some(steam) = find_steam_path() {
        if let Some(account_id) = discover_account_ids(&steam).into_iter().next() {
            raw = engine::run_cli_value(
                &app,
                &[
                    "list-remote-apps".to_string(),
                    provider.clone(),
                    account_id,
                ],
            )?;
        }
    }
    Ok(OperationResult {
        success: true,
        message: format!("{} connection verified", upstream_config::provider_display_name(&provider)),
        raw: Some(raw),
        ..OperationResult::default()
    })
}

#[command]
pub fn cloud_redirect_engine_list_apps(
    app: AppHandle,
    provider: Option<String>,
) -> Result<Vec<RemoteAppInfo>, String> {
    let provider = active_provider(provider)?;
    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    let account_ids = discover_account_ids(&steam);
    if account_ids.is_empty() {
        return Err("No Steam account folders were found in userdata".to_string());
    }

    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for account_id in account_ids {
        match engine::run_cli_value(
            &app,
            &[
                "list-remote-apps".to_string(),
                provider.clone(),
                account_id.clone(),
            ],
        ) {
            Ok(detail) => {
                if let Some(apps) = detail.get("apps").and_then(Value::as_array) {
                    for value in apps {
                        let app_id = string_field(value, "app_id");
                        if app_id.is_empty() {
                            continue;
                        }
                        entries.push(RemoteAppInfo {
                            account_id: account_id.clone(),
                            app_id,
                            file_count: u64_field(value, "file_count"),
                            total_size: u64_field(value, "total_size"),
                        });
                    }
                }
            }
            Err(error) => errors.push(format!("{account_id}: {error}")),
        }
    }
    if entries.is_empty() && !errors.is_empty() {
        return Err(errors.join(" | "));
    }
    entries.sort_by(|left, right| {
        left.account_id
            .cmp(&right.account_id)
            .then_with(|| left.app_id.cmp(&right.app_id))
    });
    Ok(entries)
}

#[command]
pub fn cloud_redirect_engine_list_files(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
) -> Result<Vec<RemoteFileInfo>, String> {
    let provider = active_provider(provider)?;
    let value = engine::run_cli_value(
        &app,
        &[
            "list-remote-app-files".to_string(),
            provider,
            account_id,
            app_id,
        ],
    )?;
    Ok(value
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .map(|file| RemoteFileInfo {
                    path: string_field(file, "path"),
                    size: u64_field(file, "size"),
                    modified_time: file
                        .get("modified_time")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default())
}

#[command]
pub fn cloud_redirect_engine_sync_app(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
) -> Result<OperationResult, String> {
    let provider = active_provider(provider)?;
    let cloud_root = cloud_root()?;
    let value = engine::run_cli_value(
        &app,
        &[
            "sync-remote-app".to_string(),
            provider,
            account_id,
            app_id,
            cloud_root.to_string_lossy().into_owned(),
        ],
    )?;
    Ok(OperationResult {
        success: true,
        message: "Remote app synchronized".to_string(),
        raw: Some(value),
        ..OperationResult::default()
    })
}

#[command]
pub fn cloud_redirect_engine_sync_all(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
) -> Result<OperationResult, String> {
    let provider = active_provider(provider)?;
    let cloud_root = cloud_root()?;
    let value = engine::run_cli_value(
        &app,
        &[
            "sync-all-remote-apps".to_string(),
            provider,
            account_id,
            cloud_root.to_string_lossy().into_owned(),
        ],
    )?;
    Ok(OperationResult {
        success: true,
        message: "All remote apps synchronized".to_string(),
        raw: Some(value),
        ..OperationResult::default()
    })
}

#[command]
pub fn cloud_redirect_engine_delete_app(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
    confirmation: String,
) -> Result<OperationResult, String> {
    if confirmation != format!("DELETE {app_id}") {
        return Err(format!("Type DELETE {app_id} to confirm"));
    }
    if is_steam_running() {
        return Err(
            "Close Steam before deleting CloudRedirect data so a rollback backup can be created."
                .to_string(),
        );
    }
    let provider = active_provider(provider)?;
    let cloud_root = cloud_root()?;
    let (value, safety_backup) = engine::with_operation_lock(|| {
        // Always materialize and verify the current remote state locally first.
        // A destructive operation is not allowed to proceed without a restorable
        // snapshot owned by the launcher.
        engine::run_cli_value(
            &app,
            &[
                "sync-remote-app".to_string(),
                provider.clone(),
                account_id.clone(),
                app_id.clone(),
                cloud_root.to_string_lossy().into_owned(),
            ],
        )?;
        let safety_backup = backup::create_safety(
            &app,
            &account_id,
            &app_id,
            "before-remote-delete",
        )?;
        let value = engine::run_cli_value(
            &app,
            &[
                "delete-remote-app".to_string(),
                provider,
                account_id,
                app_id,
            ],
        )?;
        Ok((value, safety_backup))
    })?;
    let mut raw = value.clone();
    if let Some(object) = raw.as_object_mut() {
        object.insert(
            "safetyBackup".to_string(),
            serde_json::to_value(&safety_backup)
                .map_err(|error| format!("Cannot serialize safety backup: {error}"))?,
        );
    }
    Ok(OperationResult {
        success: value.get("success").and_then(Value::as_bool).unwrap_or(false),
        message: format!(
            "Remote app data deleted. Safety backup {} was retained locally.",
            safety_backup.id
        ),
        deleted: value.get("deleted").and_then(Value::as_u64),
        failed: value.get("failed").and_then(Value::as_u64),
        raw: Some(raw),
    })
}

#[command]
pub fn cloud_redirect_engine_get_manifest_pins() -> Result<ManifestPinConfig, String> {
    manifest_pinning::load()
}

#[command]
pub fn cloud_redirect_engine_save_manifest_pins(
    input: ManifestPinConfigInput,
) -> Result<ManifestPinConfig, String> {
    manifest_pinning::save(input)
}

#[command]
pub fn cloud_redirect_engine_create_backup(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
) -> Result<LocalBackupInfo, String> {
    if is_steam_running() {
        return Err("Close Steam before creating a consistent CloudRedirect backup".to_string());
    }
    let provider = active_provider(provider)?;
    let root = cloud_root()?;
    engine::with_operation_lock(|| {
        engine::run_cli_value(
            &app,
            &[
                "sync-remote-app".to_string(),
                provider,
                account_id.clone(),
                app_id.clone(),
                root.to_string_lossy().into_owned(),
            ],
        )?;
        backup::create(&app, &account_id, &app_id)
    })
}

#[command]
pub fn cloud_redirect_engine_list_backups(
    app: AppHandle,
    app_id: Option<String>,
) -> Result<Vec<LocalBackupInfo>, String> {
    backup::list(&app, app_id.as_deref())
}

#[command]
pub fn cloud_redirect_engine_restore_backup(
    app: AppHandle,
    provider: Option<String>,
    backup_id: String,
    confirmation: String,
    publish_after_restore: bool,
) -> Result<OperationResult, String> {
    engine::with_operation_lock(|| {
        let restored = backup::restore(&app, &backup_id, &confirmation)?;
        let mut raw = serde_json::to_value(&restored)
            .map_err(|error| format!("Cannot serialize restored backup: {error}"))?;
        if publish_after_restore {
            let provider = active_provider(provider)?;
            let root = cloud_root()?;
            let publish = engine::run_cli_value(
                &app,
                &[
                    "publish-full-manifest".to_string(),
                    provider,
                    restored.account_id.clone(),
                    restored.app_id.clone(),
                    root.to_string_lossy().into_owned(),
                ],
            )?;
            if let Value::Object(object) = &mut raw {
                object.insert("publish".to_string(), publish);
            }
        }
        Ok(OperationResult {
            success: true,
            message: format!("Backup {} restored", restored.id),
            raw: Some(raw),
            ..OperationResult::default()
        })
    })
}

#[command]
pub fn cloud_redirect_engine_list_stats(app: AppHandle, provider: Option<String>) -> Result<Vec<StatsEntry>, String> {
    let provider = active_provider(provider)?;
    let value = engine::run_cli_value(
        &app,
        &["list-all-stats".to_string(), provider],
    )?;
    Ok(value
        .get("apps")
        .and_then(Value::as_array)
        .map(|apps| {
            apps
                .iter()
                .map(|entry| StatsEntry {
                    account_id: string_field(entry, "account_id"),
                    app_id: string_field(entry, "app_id"),
                    content: string_field(entry, "content"),
                })
                .collect()
        })
        .unwrap_or_default())
}

#[command]
pub fn cloud_redirect_engine_migrate(app: AppHandle, request: MigrationRequest) -> Result<MigrationEvent, String> {
    if request.source_provider == request.destination_provider {
        return Err("Source and destination providers must be different".to_string());
    }
    let event = engine::run_migration(
        &app,
        &request.source_provider,
        &request.destination_provider,
    )?;
    if request.switch_after_verify && event.failed.unwrap_or(0) == 0 {
        upstream_config::activate_provider(&request.destination_provider)?;
    }
    Ok(event)
}

#[command]
pub fn cloud_redirect_engine_gc_blobs(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
) -> Result<OperationResult, String> {
    let provider = active_provider(provider)?;
    let root = cloud_root()?;
    run_simple_cli(
        &app,
        vec![
            "gc-blobs".to_string(),
            provider,
            account_id,
            app_id,
            root.to_string_lossy().into_owned(),
        ],
        "Unreferenced cloud blobs cleaned",
    )
}

#[command]
pub fn cloud_redirect_engine_publish_manifest(
    app: AppHandle,
    provider: Option<String>,
    account_id: String,
    app_id: String,
) -> Result<OperationResult, String> {
    let provider = active_provider(provider)?;
    let root = cloud_root()?;
    run_simple_cli(
        &app,
        vec![
            "publish-full-manifest".to_string(),
            provider,
            account_id,
            app_id,
            root.to_string_lossy().into_owned(),
        ],
        "Full manifest published",
    )
}

#[command]
pub fn cloud_redirect_engine_prune_legacy(app: AppHandle) -> Result<OperationResult, String> {
    let root = cloud_root()?;
    run_simple_cli(
        &app,
        vec![
            "prune-local-legacy-metadata".to_string(),
            root.to_string_lossy().into_owned(),
        ],
        "Legacy local metadata pruned",
    )
}

#[command]
pub fn cloud_redirect_engine_run_cloud760(
    app: AppHandle,
    app_id: Option<String>,
    action: Option<String>,
    files: Option<Vec<String>>,
    confirmation: Option<String>,
) -> Result<OperationResult, String> {
    if !is_steam_running() {
        return Err("Steam must be running and logged in for the Cloud760 tool".to_string());
    }
    let runtime = engine::ensure_runtime(&app)?;
    let tool = runtime.join("cloud760_tool.exe");
    let steam_api = runtime.join("steam_api.dll");
    if !steam_api.is_file() {
        return Err(
            "Cloud760 requires the upstream 32-bit steam_api.dll beside cloud760_tool.exe. The source-only upstream archive did not include that release asset."
                .to_string(),
        );
    }

    let app_id = app_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "760".to_string());
    let action = action
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "list".to_string());
    if !matches!(action.as_str(), "list" | "quota" | "delete" | "delete-all") {
        return Err(format!("Unsupported Cloud760 action: {action}"));
    }
    if matches!(action.as_str(), "delete" | "delete-all")
        && confirmation.as_deref() != Some(&format!("DELETE {app_id}"))
    {
        return Err(format!("Type DELETE {app_id} to confirm"));
    }

    let mut command = Command::new(tool);
    command
        .current_dir(&runtime)
        .arg(&action)
        .arg(&app_id)
        .arg("--porcelain")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if action == "delete" {
        let files = files.unwrap_or_default();
        if files.is_empty() {
            return Err("Select at least one Cloud760 file to delete".to_string());
        }
        command.args(files);
    } else if action == "delete-all" {
        command.arg("--yes");
    }
    engine::hide_console(&mut command);
    let output = command.output().map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }

    let mut cloud_enabled_account = None;
    let mut cloud_enabled_app = None;
    let mut quota_total = None;
    let mut quota_used = None;
    let mut remote_files = Vec::new();
    let mut deleted = 0_u64;
    let mut failed = 0_u64;
    for line in stdout.lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        match columns.as_slice() {
            ["CLOUD", account, app] => {
                cloud_enabled_account = Some(*account == "1");
                cloud_enabled_app = Some(*app == "1");
            }
            ["QUOTA", total, used] => {
                quota_total = total.parse::<u64>().ok();
                quota_used = used.parse::<u64>().ok();
            }
            ["FILE", name, size, persisted] => remote_files.push(serde_json::json!({
                "name": name,
                "size": size.parse::<u64>().unwrap_or_default(),
                "persisted": *persisted == "1",
            })),
            ["DEL", _, result] => {
                if *result == "OK" { deleted += 1 } else { failed += 1 }
            }
            _ => {}
        }
    }
    Ok(OperationResult {
        success: true,
        message: if action == "list" {
            format!("Cloud760 inventory loaded for AppID {app_id}")
        } else {
            format!("Cloud760 {action} completed for AppID {app_id}")
        },
        deleted: Some(deleted),
        failed: Some(failed),
        raw: Some(serde_json::json!({
            "appId": app_id,
            "cloudEnabledForAccount": cloud_enabled_account,
            "cloudEnabledForApp": cloud_enabled_app,
            "quotaTotal": quota_total,
            "quotaUsed": quota_used,
            "files": remote_files,
            "stdout": stdout,
        })),
    })
}

#[command]
pub fn cloud_redirect_engine_diagnostics(app: AppHandle) -> Result<DiagnosticsReport, String> {
    let mut items = Vec::new();
    let runtime = engine::ensure_runtime(&app);
    match &runtime {
        Ok(path) => items.push(diagnostic("engine", "ok", "Engine ready", &path.display().to_string(), None)),
        Err(error) => items.push(diagnostic("engine", "error", "Engine unavailable", error, Some("buildEngine"))),
    }
    let steam = find_steam_path();
    match &steam {
        Some(path) => items.push(diagnostic("steam", "ok", "Steam detected", &path.display().to_string(), None)),
        None => items.push(diagnostic("steam", "error", "Steam not detected", "The Steam installation could not be resolved.", None)),
    }
    if is_steam_running() {
        items.push(diagnostic("steam-running", "warning", "Steam is running", "Close Steam before applying patches or replacing the DLL.", Some("closeSteam")));
    }
    let provider = upstream_config::get_provider_view()?;
    items.push(diagnostic(
        "provider",
        if provider.authenticated { "ok" } else { "warning" },
        "Cloud provider",
        &format!("{} ({})", upstream_config::provider_display_name(&provider.provider), if provider.authenticated { "configured" } else { "not verified" }),
        if provider.authenticated { None } else { Some("configureProvider") },
    ));
    if let Some(path) = steam.as_ref().map(|steam| steam.join("cloud_redirect.dll")) {
        items.push(diagnostic(
            "dll",
            if path.is_file() { "ok" } else { "warning" },
            "Steam DLL deployment",
            if path.is_file() { "cloud_redirect.dll is installed." } else { "cloud_redirect.dll is not installed." },
            if path.is_file() { None } else { Some("installDll") },
        ));
    }
    let log_tail = steam
        .as_ref()
        .map(|path| tail_lines(&path.join("cloud_redirect.log"), 100))
        .unwrap_or_default();
    Ok(DiagnosticsReport {
        generated_at: Utc::now().to_rfc3339(),
        items,
        log_tail,
    })
}

fn active_provider(provider: Option<String>) -> Result<String, String> {
    let provider = provider
        .filter(|value| !value.trim().is_empty())
        .or_else(upstream_config::provider_from_config)
        .ok_or_else(|| "CloudRedirect provider is not configured".to_string())?;
    if provider == "local" {
        return Err("This operation requires a configured cloud or folder provider".to_string());
    }
    Ok(provider)
}

fn cloud_root() -> Result<PathBuf, String> {
    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    let root = steam.join("cloud_redirect");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Cannot create {}: {error}", root.display()))?;
    Ok(root)
}

fn discover_account_ids(steam: &Path) -> Vec<String> {
    let userdata = steam.join("userdata");
    let mut ids: Vec<String> = fs::read_dir(userdata)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(ToString::to_string))
        .filter(|value| value.parse::<u64>().is_ok())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}


fn run_simple_cli(app: &AppHandle, args: Vec<String>, message: &str) -> Result<OperationResult, String> {
    let value = engine::with_operation_lock(|| engine::run_cli_value(app, &args))?;
    Ok(OperationResult {
        success: true,
        message: message.to_string(),
        raw: Some(value),
        ..OperationResult::default()
    })
}

fn result_message(message: impl Into<String>) -> OperationResult {
    OperationResult {
        success: true,
        message: message.into(),
        ..OperationResult::default()
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn diagnostic(
    id: &str,
    severity: &str,
    title: &str,
    detail: &str,
    fix_action: Option<&str>,
) -> DiagnosticItem {
    DiagnosticItem {
        id: id.to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        fix_action: fix_action.map(ToString::to_string),
    }
}

fn tail_lines(path: &Path, limit: usize) -> Vec<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| {
            let lines: Vec<&str> = content.lines().collect();
            lines[lines.len().saturating_sub(limit)..]
                .iter()
                .map(|line| line.to_string())
                .collect()
        })
        .unwrap_or_default()
}
