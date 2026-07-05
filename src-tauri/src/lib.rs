pub mod steam;
pub mod asset_cache;
pub mod chat;
pub mod asset_pack;
pub mod builder;
pub mod cloud_redirect;
pub mod cloud_redirect_v2;
pub mod cloud_save;
pub mod depot_crypto;
pub mod discord_auth;
pub mod game_tags;
pub mod job;
pub mod launch;
pub mod manifest;
pub mod notifications;
pub mod platform;
pub mod remote_paths;
pub mod scanner;
pub mod secret_store;
pub mod security;
pub mod steam_integration;
pub mod storage;
pub mod translations;
pub mod updater;
pub mod overlay_injector;
pub mod steamless;

use std::path::PathBuf;
use std::sync::Arc;

use asset_pack::{AssetBlob, GameCatalog, GameDetail};
use job::{
    GameInstallState, JobControl, JobJournal, LaunchReport, LauncherSnapshot, UninstallReport,
    VerifyInstallReport,
};
use launch::ResolvedGameLaunchConfig;
use scanner::ScanReport;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_disk_free_space(path: String) -> Result<u64, String> {
    fs2::free_space(PathBuf::from(path)).map_err(|err| err.to_string())
}

#[tauri::command]
fn check_spacewar_installed() -> bool {
    steam_integration::is_spacewar_installed()
}

#[tauri::command]
fn install_spacewar() -> Result<(), String> {
    steam_integration::install_spacewar()
}

#[tauri::command]
async fn get_discord_auth_status(
    app: AppHandle,
) -> Result<discord_auth::DiscordAuthStatus, String> {
    tauri::async_runtime::spawn_blocking(move || discord_auth::get_status(&app))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn login_discord(app: AppHandle) -> Result<discord_auth::DiscordAuthStatus, String> {
    tauri::async_runtime::spawn_blocking(move || discord_auth::login(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn logout_discord(app: AppHandle) -> Result<discord_auth::DiscordAuthStatus, String> {
    discord_auth::logout(&app)
}

#[tauri::command]
fn is_steam_running() -> bool {
    steam_integration::is_steam_running()
}

#[tauri::command]
fn open_steam() -> Result<(), String> {
    steam_integration::open_steam()
}

#[tauri::command]
fn open_steam_big_picture() -> Result<(), String> {
    steam_integration::open_big_picture()
}

#[tauri::command]
fn restart_steam() -> Result<steam_integration::RestartSteamReport, String> {
    steam_integration::restart_steam()
}

#[tauri::command]
fn is_lua_game_mode_enabled() -> bool {
    steam_integration::is_lua_game_mode_enabled()
}

#[tauri::command]
fn enable_lua_game_mode() -> Result<(), String> {
    steam_integration::enable_lua_game_mode()
}

#[tauri::command]
fn disable_lua_game_mode() -> Result<(), String> {
    steam_integration::disable_lua_game_mode()
}

#[tauri::command]
fn get_steam_environment(app: AppHandle) -> steam_integration::SteamEnvironmentInfo {
    steam_integration::environment_info(&app)
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        // For URLs, we use PowerShell to safely handle special characters like '&' without cmd's argument parsing issues.
        let escaped_url = url.replace("'", "''");
        std::process::Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-WindowStyle", "Hidden",
                "-Command",
                &format!("Start-Process '{}'", escaped_url)
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct DriveInfo {
    letter: String,
    label: String,
    free_bytes: u64,
    total_bytes: u64,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

#[tauri::command]
fn list_system_drives() -> Vec<DriveInfo> {
    let mut drives = Vec::new();
    #[cfg(windows)]
    {
        const DRIVE_REMOVABLE: u32 = 2;
        const DRIVE_FIXED: u32 = 3;

        let mask = unsafe { GetLogicalDrives() };
        for index in 0..26 {
            if mask & (1u32 << index) == 0 {
                continue;
            }

            let letter = (b'A' + index as u8) as char;
            let root = format!("{}:\\", letter);
            let root_wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
            if drive_type != DRIVE_FIXED && drive_type != DRIVE_REMOVABLE {
                continue;
            }

            let mut free_available = 0u64;
            let mut total = 0u64;
            let mut total_free = 0u64;
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    root_wide.as_ptr(),
                    &mut free_available,
                    &mut total,
                    &mut total_free,
                )
            };

            if ok != 0 && total > 0 {
                drives.push(DriveInfo {
                    letter: format!("{}:", letter),
                    label: format!("Local Disk ({}:)", letter),
                    free_bytes: free_available,
                    total_bytes: total,
                });
            }
        }
    }
    drives
}

#[tauri::command]
async fn check_launcher_update(
    app: AppHandle,
) -> Result<Option<updater::LauncherUpdateInfo>, String> {
    updater::check_update(&app).await
}

#[tauri::command]
async fn apply_launcher_update(app: AppHandle) -> Result<(), String> {
    updater::download_and_apply(&app).await
}

#[tauri::command]
fn list_notifications(app: AppHandle) -> Result<Vec<notifications::NotificationRecord>, String> {
    notifications::list(&app)
}

#[tauri::command]
fn push_notification(
    app: AppHandle,
    notification: notifications::NewNotification,
) -> Result<notifications::PushNotificationResult, String> {
    notifications::push(&app, notification)
}

#[tauri::command]
fn mark_notification_read(
    app: AppHandle,
    notification_id: String,
) -> Result<Vec<notifications::NotificationRecord>, String> {
    notifications::mark_read(&app, &notification_id)
}

#[tauri::command]
fn mark_all_notifications_read(
    app: AppHandle,
) -> Result<Vec<notifications::NotificationRecord>, String> {
    notifications::mark_all_read(&app)
}

#[tauri::command]
fn clear_notifications(app: AppHandle) -> Result<Vec<notifications::NotificationRecord>, String> {
    notifications::clear(&app)
}

#[tauri::command]
fn open_notification_action(app: AppHandle, notification_id: String) -> Result<(), String> {
    notifications::open_action(&app, &notification_id)
}

#[tauri::command]
fn get_game_runtime_states(app: AppHandle) -> Result<Vec<platform::GameRuntimeState>, String> {
    platform::get_runtime_states(&app)
}

#[tauri::command]
fn clear_chunk_cache(
    state: State<'_, LauncherState>,
    cache_path: String,
) -> Result<storage::ClearCacheReport, String> {
    if state.job_control.is_running() {
        return Err("pause or finish the active download before clearing cache".to_string());
    }
    storage::clear_chunk_cache(PathBuf::from(cache_path).as_path())
}

#[tauri::command]
fn get_launcher_snapshot(app: AppHandle) -> Result<LauncherSnapshot, String> {
    job::snapshot(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_launcher_settings(app: AppHandle) -> Result<platform::LauncherSettings, String> {
    platform::get_settings(&app)
}

#[tauri::command]
fn set_launcher_settings(
    app: AppHandle,
    settings: platform::LauncherSettings,
) -> Result<platform::LauncherSettings, String> {
    platform::set_settings(&app, settings)
}

#[tauri::command]
fn get_cloud_save_status(
    app: AppHandle,
    game_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    cloud_save::get_status(&app, &game_id)
}

#[tauri::command]
fn set_cloud_save_config(
    app: AppHandle,
    game_id: String,
    enabled: bool,
    save_roots: Vec<cloud_save::CloudSaveRoot>,
    include: Vec<String>,
    exclude: Vec<String>,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    cloud_save::set_config(&app, &game_id, enabled, save_roots, include, exclude)
}

#[tauri::command]
fn sync_cloud_save(
    app: AppHandle,
    game_id: String,
    direction: Option<String>,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    cloud_save::sync_manual(&app, &game_id, direction.as_deref())
}

#[tauri::command]
fn resolve_cloud_save_conflict(
    app: AppHandle,
    game_id: String,
    conflict_id: String,
    resolution: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    cloud_save::resolve_conflict(&app, &game_id, &conflict_id, &resolution)
}

#[tauri::command]
fn restore_cloud_save_snapshot(
    app: AppHandle,
    game_id: String,
    snapshot_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    cloud_save::restore_snapshot(&app, &game_id, &snapshot_id)
}

#[tauri::command]
async fn global_connect_google_drive(
    app: AppHandle,
) -> Result<(), String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || cloud_save::global_connect_google_drive(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn global_disconnect_google_drive(
    app: AppHandle,
) -> Result<(), String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || cloud_save::global_disconnect_google_drive(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn global_is_google_drive_connected(
    app: AppHandle,
) -> Result<bool, String> {
    Ok(cloud_save::global_is_google_drive_connected(&app))
}

#[tauri::command]
async fn connect_google_drive(
    app: AppHandle,
    game_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || cloud_save::connect_google_drive(&app, &game_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn disconnect_google_drive(
    app: AppHandle,
    game_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || {
        cloud_save::disconnect_google_drive(&app, &game_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn backup_save_game_to_google_drive(
    app: AppHandle,
    game_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || cloud_save::backup_to_google_drive(&app, &game_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn restore_missing_save_files(
    app: AppHandle,
    game_id: String,
) -> Result<cloud_save::CloudSaveStatus, String> {
    discord_auth::require_authorized_session()?;
    tauri::async_runtime::spawn_blocking(move || {
        cloud_save::restore_missing_from_google_drive(&app, &game_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn plan_install_update(
    app: AppHandle,
    path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<LauncherSnapshot, String> {
    job::snapshot_for_install(&app, PathBuf::from(path).as_path(), target_version, game_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn plan_fresh_install(
    app: AppHandle,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<LauncherSnapshot, String> {
    job::snapshot_for_fresh_install(&app, target_version, game_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn scan_install(path: String) -> Result<ScanReport, String> {
    scanner::scan_install(PathBuf::from(path).as_path()).map_err(|err| err.to_string())
}

#[tauri::command]
fn read_job_journal(app: AppHandle) -> Result<Option<JobJournal>, String> {
    job::read_latest_journal(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_catalog(app: AppHandle) -> Result<GameCatalog, String> {
    asset_pack::get_game_catalog(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_detail(
    app: AppHandle,
    game_id: String,
    locale: Option<String>,
) -> Result<GameDetail, String> {
    asset_pack::get_game_detail(&app, &game_id, locale).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_asset(app: AppHandle, game_id: String, asset_id: String) -> Result<AssetBlob, String> {
    asset_pack::get_game_asset(&app, &game_id, &asset_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_install_state(app: AppHandle, game_id: String) -> Result<GameInstallState, String> {
    job::game_install_state(&app, &game_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_install_states(
    app: AppHandle,
    game_ids: Vec<String>,
) -> Result<Vec<GameInstallState>, String> {
    job::game_install_states_quick(&app, &game_ids).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_game_launch_config(
    app: AppHandle,
    game_id: String,
    install_path: String,
    launch_executable: Option<String>,
) -> Result<ResolvedGameLaunchConfig, String> {
    job::game_launch_config(
        &app,
        &game_id,
        PathBuf::from(install_path).as_path(),
        launch_executable,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn launch_game(
    app: AppHandle,
    game_id: String,
    install_path: String,
    launch_executable: Option<String>,
    launch_option_id: Option<String>,
    skip_cloud_sync: Option<bool>,
) -> Result<LaunchReport, String> {
    discord_auth::require_authorized_session()?;
    job::launch_game(
        &app,
        &game_id,
        PathBuf::from(install_path).as_path(),
        launch_executable,
        launch_option_id,
        skip_cloud_sync.unwrap_or(false),
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn kill_game(_app: AppHandle, game_id: String) -> Result<(), String> {
    job::kill_game(&game_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn verify_install_integrity(
    app: AppHandle,
    game_id: String,
    install_path: String,
    target_version: Option<String>,
) -> Result<VerifyInstallReport, String> {
    job::verify_install_integrity(
        Some(&app),
        &game_id,
        PathBuf::from(install_path).as_path(),
        target_version,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn uninstall_game(
    app: AppHandle,
    game_id: String,
    install_path: String,
) -> Result<UninstallReport, String> {
    discord_auth::require_authorized_session()?;
    job::uninstall_game(&app, &game_id, PathBuf::from(install_path).as_path())
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn start_update_job(
    app: AppHandle,
    state: State<'_, LauncherState>,
    install_path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, String> {
    discord_auth::require_authorized_session()?;
    state.job_control.reset();
    job::spawn_update_job(
        app,
        state.job_control.clone(),
        install_path,
        target_version,
        game_id,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn start_install_job(
    app: AppHandle,
    state: State<'_, LauncherState>,
    game_id: Option<String>,
    target_version: Option<String>,
    install_path: Option<String>,
) -> Result<JobJournal, String> {
    discord_auth::require_authorized_session()?;
    state.job_control.reset();
    job::spawn_install_job(
        app,
        state.job_control.clone(),
        target_version,
        install_path,
        game_id,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn start_repair_job(
    app: AppHandle,
    state: State<'_, LauncherState>,
    game_id: String,
    install_path: String,
    target_version: Option<String>,
    file_paths: Vec<String>,
) -> Result<JobJournal, String> {
    discord_auth::require_authorized_session()?;
    state.job_control.reset();
    job::spawn_repair_job(
        app,
        state.job_control.clone(),
        &game_id,
        install_path,
        target_version,
        file_paths,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn pause_job(state: State<'_, LauncherState>) {
    state.job_control.pause();
}

#[tauri::command]
fn resume_job(app: AppHandle, state: State<'_, LauncherState>) -> Result<(), String> {
    state.job_control.resume();
    if !state.job_control.is_running() {
        if let Ok(Some(journal)) = job::read_latest_journal(&app) {
            match journal.kind.as_str() {
                "install" => {
                    job::spawn_install_job(
                        app,
                        state.job_control.clone(),
                        Some(journal.to_version),
                        Some(journal.install_path),
                        Some(journal.game_id),
                    )
                    .map_err(|e| e.to_string())?;
                }
                "update" => {
                    job::spawn_update_job(
                        app,
                        state.job_control.clone(),
                        journal.install_path,
                        Some(journal.to_version),
                        Some(journal.game_id),
                    )
                    .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn cancel_job(app: AppHandle, state: State<'_, LauncherState>) -> Result<(), String> {
    state.job_control.cancel();
    job::abort_and_clean_job(&app, None).map_err(|e| e.to_string())
}

#[tauri::command]
fn abort_and_clean_job(
    app: AppHandle,
    state: State<'_, LauncherState>,
    game_id: String,
) -> Result<(), String> {
    state.job_control.cancel();
    job::abort_and_clean_job(&app, Some(&game_id)).map_err(|e| e.to_string())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutLaunchRequest {
    game_id: String,
    install_path: String,
    launch_executable: Option<String>,
}

struct LauncherState {
    job_control: Arc<JobControl>,
}

#[tauri::command]
fn clear_launcher_config(app: tauri::AppHandle) -> Result<(), String> {
    if let Ok(app_dir) = app.path().app_data_dir() {
        if app_dir.exists() {
            let _ = std::fs::remove_dir_all(&app_dir);
            let _ = std::fs::create_dir_all(&app_dir);
        }
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(LauncherState {
            job_control: Arc::new(JobControl::default()),
        })
        .manage(asset_pack::AssetPackCache::default())
        .invoke_handler(tauri::generate_handler![
            steam::check_steam_status,
            steam::check_steam_update,
            steam::remove_from_steam,
            steam::force_restart_steam,
            steam::add_to_steam,
            steam::get_installed_steam_apps,
            chat::load_chat_history,
            chat::save_chat_message,
            chat::delete_chat_message,
            chat::edit_chat_message,
            chat::clear_chat_history,
            chat::download_from_huggingface,
            chat::upload_chat_media,
            chat::upload_chat_media_from_path,
            chat::delete_chat_media,
            chat::get_chat_media_base64,
            chat::download_chat_media_to_disk,
            chat::sync_to_huggingface,
            chat::read_file_base64,
            get_disk_free_space,
            list_system_drives,
            check_launcher_update,
            apply_launcher_update,
            list_notifications,
            push_notification,
            mark_notification_read,
            mark_all_notifications_read,
            clear_notifications,
            open_notification_action,
            get_game_runtime_states,
            clear_chunk_cache,
            get_launcher_snapshot,
            get_launcher_settings,
            set_launcher_settings,
            get_cloud_save_status,
            set_cloud_save_config,
            sync_cloud_save,
            resolve_cloud_save_conflict,
            restore_cloud_save_snapshot,
            connect_google_drive,
            disconnect_google_drive,
            global_connect_google_drive,
            global_disconnect_google_drive,
            global_is_google_drive_connected,
            backup_save_game_to_google_drive,
            restore_missing_save_files,
            plan_install_update,
            plan_fresh_install,
            scan_install,
            read_job_journal,
            get_game_catalog,
            get_game_detail,
            get_game_asset,
            asset_cache::fetch_asset_cache,
            asset_cache::clear_game_cache,
            get_game_install_state,
            get_game_install_states,
            get_game_launch_config,
            launch_game,
            kill_game,
            verify_install_integrity,
            uninstall_game,
            start_update_job,
            translations::get_available_translations,
            translations::install_translation,
            translations::uninstall_translation,
            start_install_job,
            start_repair_job,
            pause_job,
            resume_job,
            cancel_job,
            abort_and_clean_job,
            open_folder,
            open_url,
            check_spacewar_installed,
            install_spacewar,
            get_discord_auth_status,
            login_discord,
            logout_discord,
            is_steam_running,
            open_steam,
            restart_steam,
            open_steam_big_picture,
            get_steam_environment,
            is_lua_game_mode_enabled,
            enable_lua_game_mode,
            disable_lua_game_mode,
            steam_integration::get_steam_game_install_dir,
            exit_app,
            clear_launcher_config,
            cloud_redirect::cloud_redirect_get_status,
            cloud_redirect::cloud_redirect_run_stfixer,
            cloud_redirect::cloud_redirect_get_provider_config,
            cloud_redirect::cloud_redirect_save_provider_config,
            cloud_redirect::cloud_redirect_connect_google,
            // CloudRedirect V2 (new features)
            cloud_redirect_v2::cloud_redirect_v2_get_status,
            cloud_redirect_v2::cloud_redirect_enable,
            cloud_redirect_v2::cloud_redirect_disable,
            cloud_redirect_v2::cloud_redirect_set_local_path,
            cloud_redirect_v2::cloud_redirect_start_oauth,
            cloud_redirect_v2::cloud_redirect_complete_oauth,
            cloud_redirect_v2::cloud_redirect_trigger_sync,
            cloud_redirect_v2::cloud_redirect_get_sync_status,
            cloud_redirect_v2::cloud_redirect_poll_oauth_code,
            cloud_redirect_v2::cloud_redirect_list_game_saves,
            cloud_redirect_v2::cloud_redirect_backup_save,
            cloud_redirect_v2::cloud_redirect_reset_game,
            cloud_redirect_v2::cloud_redirect_list_backups,
            cloud_redirect_v2::cloud_redirect_restore_backup,
            // Steamless — native DRM remover (Error 54 Fix)
            steamless::steamless_apply,
            steamless::steamless_restore,
            steamless::steamless_status,
        ])
        .setup(|app| {
            // Check and update DLLs on startup if Lua-Game Mode is enabled
            std::thread::spawn(|| {
                if let Err(e) = steam_integration::check_and_update_dlls() {
                    eprintln!("Failed to check/update DLLs: {}", e);
                }
            });

            asset_cache::perform_ttl_cleanup(app.handle());
            let quit_i =
                tauri::menu::MenuItem::with_id(app, "quit", "Quit 0xoLemon", true, None::<&str>)?;
            let store_i =
                tauri::menu::MenuItem::with_id(app, "store", "Store", true, None::<&str>)?;
            let library_i =
                tauri::menu::MenuItem::with_id(app, "library", "Library", true, None::<&str>)?;
            let community_i =
                tauri::menu::MenuItem::with_id(app, "community", "Community", true, None::<&str>)?;
            let settings_i =
                tauri::menu::MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let tray_menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &store_i,
                    &library_i,
                    &community_i,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &settings_i,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &quit_i,
                ],
            )?;

            let _tray = tauri::tray::TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    id => {
                        let tab = match id {
                            "store" => "Home",
                            "library" => "Library",
                            "community" => "Community",
                            "settings" => "Settings",
                            _ => return,
                        };
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.emit("navigate", tab);
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click: toggle main window
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let is_visible = win.is_visible().unwrap_or(false);
                            if is_visible {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        window_clone.hide().unwrap();
                        api.prevent_close();
                    }
                    _ => {}
                });
            }

            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(app_dir.join("journals"))?;
            platform::initialize(app.handle())
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
            steam_integration::start_pending_worker(app.handle().clone());
            let job_control = app.state::<LauncherState>().job_control.clone();
            job::start_auto_update_scheduler(app.handle().clone(), job_control);
            cloud_save::start_google_drive_restore_monitor(app.handle().clone());
            // Initialize overlay window: pass-through mouse events by default so the
            // transparent overlay window never accidentally blocks game input.
            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.set_ignore_cursor_events(true);
            }
            let shortcut_request = parse_shortcut_launch_request();
            // When the main launcher starts, migrate existing desktop shortcuts away
            // from the legacy AppData bootstrap and point them at the game directory.
            // Do not run this from a per-game bootstrap, otherwise an old bootstrap
            // could copy itself into every registered game folder.
            if shortcut_request.is_none() {
                let _ = job::refresh_registered_game_shortcuts(app.handle());
            }
            if let Some(request) = shortcut_request {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let auth = discord_auth::get_status(&handle);
                    if auth.state != "authorized" {
                        let _ = handle.emit(
                            "launcher://shortcut-launch-error",
                            "Discord authorization is required before launching a game.",
                        );
                        return;
                    }
                    let _ = handle.emit("launcher://shortcut-launch", request.clone());
                    std::thread::sleep(std::time::Duration::from_millis(1800));
                    if game_tags::game_has_tag(&request.game_id, "online") {
                        if !steam_integration::is_spacewar_installed() {
                            let _ = handle.emit("launcher://spacewar-required", request.clone());
                            return;
                        }
                        if !steam_integration::is_steam_running() {
                            let _ = handle
                                .emit("launcher://steam-recommendation-required", request.clone());
                            return;
                        }
                    }
                    let install_path = PathBuf::from(&request.install_path);
                    if let Err(err) = job::launch_game(
                        &handle,
                        &request.game_id,
                        install_path.as_path(),
                        request.launch_executable.clone(),
                        None,
                        false,
                    ) {
                        let _ = handle.emit("launcher://shortcut-launch-error", err.to_string());
                    }
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut("Shift+F1")
                .unwrap()
                .with_handler(|app, shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if shortcut.matches(tauri_plugin_global_shortcut::Modifiers::SHIFT, tauri_plugin_global_shortcut::Code::F1) {
                            if let Some(window) = app.get_webview_window("overlay") {
                                if window.is_visible().unwrap_or(false) {
                                    // Hide overlay: let game receive all mouse input
                                    let _ = window.set_ignore_cursor_events(true);
                                    let _ = window.hide();
                                } else {
                                    // Show overlay: re-assert topmost so game can't cover it,
                                    // then enable mouse interaction on the overlay
                                    let _ = window.set_always_on_top(true);
                                    let _ = window.set_ignore_cursor_events(false);
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .run(tauri::generate_context!())
        .expect("failed to run 007 First Light launcher");
}

fn parse_shortcut_launch_request() -> Option<ShortcutLaunchRequest> {
    let args = std::env::args().collect::<Vec<_>>();
    let game_id = flag_value(&args, "--launch-game")?;
    let install_path = flag_value(&args, "--install-path")?;
    Some(ShortcutLaunchRequest {
        game_id,
        install_path,
        launch_executable: flag_value(&args, "--launch-executable"),
    })
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .filter(|value| !value.trim().is_empty())
}
