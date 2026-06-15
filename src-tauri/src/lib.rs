pub mod asset_pack;
pub mod builder;
pub mod job;
pub mod manifest;
pub mod scanner;
pub mod security;
pub mod updater;

use std::path::PathBuf;
use std::sync::Arc;

use asset_pack::{AssetBlob, GameCatalog, GameDetail};
use job::{
    GameInstallState, JobControl, JobJournal, LaunchReport, LauncherSnapshot, UninstallReport,
    VerifyInstallReport,
};
use scanner::ScanReport;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
fn get_disk_free_space(path: String) -> Result<u64, String> {
    fs2::free_space(PathBuf::from(path)).map_err(|err| err.to_string())
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
fn check_launcher_update() -> Result<Option<updater::LauncherUpdateInfo>, String> {
    Ok(updater::check_update())
}

#[tauri::command]
fn apply_launcher_update(app: AppHandle, download_url: String) -> Result<(), String> {
    updater::download_and_apply(&app, download_url)
}

#[tauri::command]
fn get_launcher_snapshot(app: AppHandle) -> Result<LauncherSnapshot, String> {
    job::snapshot(&app).map_err(|err| err.to_string())
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
fn get_game_install_state(game_id: String) -> Result<GameInstallState, String> {
    job::game_install_state(&game_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn launch_game(
    app: AppHandle,
    game_id: String,
    install_path: String,
    launch_executable: Option<String>,
) -> Result<LaunchReport, String> {
    job::launch_game(
        &app,
        &game_id,
        PathBuf::from(install_path).as_path(),
        launch_executable,
    )
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
fn uninstall_game(game_id: String, install_path: String) -> Result<UninstallReport, String> {
    job::uninstall_game(&game_id, PathBuf::from(install_path).as_path())
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
fn resume_job(state: State<'_, LauncherState>) {
    state.job_control.resume();
}

#[tauri::command]
fn cancel_job(state: State<'_, LauncherState>) {
    state.job_control.cancel();
}

#[tauri::command]
fn abort_and_clean_job(state: State<'_, LauncherState>, game_id: String) -> Result<(), String> {
    state.job_control.cancel();
    job::abort_and_clean_job(&game_id).map_err(|e| e.to_string())
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

pub fn run() {
    tauri::Builder::default()
        .manage(LauncherState {
            job_control: Arc::new(JobControl::default()),
        })
        .manage(asset_pack::AssetPackCache::default())
        .invoke_handler(tauri::generate_handler![
            get_disk_free_space,
            list_system_drives,
            check_launcher_update,
            apply_launcher_update,
            get_launcher_snapshot,
            plan_install_update,
            plan_fresh_install,
            scan_install,
            read_job_journal,
            get_game_catalog,
            get_game_detail,
            get_game_asset,
            get_game_install_state,
            launch_game,
            verify_install_integrity,
            uninstall_game,
            start_update_job,
            start_install_job,
            start_repair_job,
            pause_job,
            resume_job,
            cancel_job,
            abort_and_clean_job
        ])
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(app_dir.join("journals"))?;
            std::fs::create_dir_all(app_dir.join("cache"))?;
            if let Some(request) = parse_shortcut_launch_request() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let _ = handle.emit("launcher://shortcut-launch", request.clone());
                    std::thread::sleep(std::time::Duration::from_millis(1800));
                    let install_path = PathBuf::from(&request.install_path);
                    if let Err(err) = job::launch_game(
                        &handle,
                        &request.game_id,
                        install_path.as_path(),
                        request.launch_executable.clone(),
                    ) {
                        let _ = handle.emit("launcher://shortcut-launch-error", err.to_string());
                    }
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
