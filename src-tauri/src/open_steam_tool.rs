use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use toml_edit::{value, DocumentMut};

pub(crate) const STEAM_HOOK_DLLS: [&str; 4] = [
    "0xoCore.dll",
    "0xoPayload.dll",
    "dwmapi.dll",
    "xinput1_4.dll",
];
pub(crate) const LUA_GAME_MODE_MARKER: &str = ".0xo-lua-game-mode-enabled";

static HOOK_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OSTConfig {
    pub log_level: String,
    pub manifest_url: String,
    pub enable_stats_api: bool,
    pub enable_inject: bool,
}

impl Default for OSTConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            manifest_url: "opensteamtool".to_string(),
            enable_stats_api: true,
            enable_inject: true,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCoreSettingsState {
    pub stats_api_enabled: bool,
    pub config_exists: bool,
}

const NATIVE_CORE_CONFIG: &str = "_0xolemoncore.toml";

fn native_core_config_path() -> Result<PathBuf, String> {
    Ok(get_steam_root()?.join(NATIVE_CORE_CONFIG))
}

fn parse_native_core_config(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|error| format!("Native core settings are invalid: {error}"))
}

fn read_native_core_stats_setting(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(true);
    }
    let text = fs::read_to_string(path).map_err(|error| io_error("read", path, error))?;
    let document = parse_native_core_config(&text)?;
    Ok(document["stats"]["enable_api"].as_bool().unwrap_or(true))
}

fn write_native_core_stats_setting(path: &Path, enabled: bool) -> Result<(), String> {
    let mut document = if path.is_file() {
        parse_native_core_config(
            &fs::read_to_string(path).map_err(|error| io_error("read", path, error))?,
        )?
    } else {
        DocumentMut::new()
    };
    document["stats"]["enable_api"] = value(enabled);
    crate::lua_live::atomic_write_path(path, document.to_string().as_bytes())
}

#[tauri::command]
pub async fn get_native_core_settings() -> Result<NativeCoreSettingsState, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = native_core_config_path()?;
        Ok(NativeCoreSettingsState {
            stats_api_enabled: read_native_core_stats_setting(&path)?,
            config_exists: path.is_file(),
        })
    })
    .await
    .map_err(|error| format!("Native core settings task failed: {error}"))?
}

#[tauri::command]
pub async fn set_native_core_stats_api(enabled: bool) -> Result<NativeCoreSettingsState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = native_core_config_path()?;
        write_native_core_stats_setting(&path, enabled)?;
        Ok(NativeCoreSettingsState {
            stats_api_enabled: enabled,
            config_exists: true,
        })
    })
    .await
    .map_err(|error| format!("Native core settings task failed: {error}"))?
}

pub fn get_steam_root() -> Result<PathBuf, String> {
    crate::steam::get_steam_path()
        .filter(|path| path.is_dir() && path.join("steam.exe").is_file())
        .ok_or_else(|| "Steam installation not found".to_string())
}

pub(crate) fn resolve_hook_resource_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|_| "Engine resources are unavailable".to_string())?;

    let mut candidates = vec![
        // Tauri v2 preserves the configured resource path when resources are
        // listed as strings, so resources/steam_hooks/* lands here.
        resource_root.join("resources").join("steam_hooks"),
        // Development/portable fallback used by older launcher builds.
        resource_root.join("steam_hooks"),
    ];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("resources").join("steam_hooks"));
        }
    }

    candidates
        .into_iter()
        .find(|dir| STEAM_HOOK_DLLS.iter().all(|file| dir.join(file).is_file()))
        .ok_or_else(|| "Engine resources are unavailable".to_string())
}

pub(crate) fn ensure_steam_closed() -> Result<(), String> {
    let process_ids = crate::cloud_redirect::steam_detector::try_steam_process_ids()?;
    if process_ids.is_empty() {
        return Ok(());
    }

    Err(format!(
        "Steam is still running in the background (PID{} {}). Exit Steam from the tray and retry.",
        if process_ids.len() == 1 { "" } else { "s" },
        process_ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

pub(crate) fn hook_files_present(steam_root: &Path) -> bool {
    STEAM_HOOK_DLLS
        .iter()
        .all(|dll| steam_root.join(dll).is_file())
}

pub(crate) fn hook_files_match_sources(app: &AppHandle, steam_root: &Path) -> bool {
    let Ok(resource_dir) = resolve_hook_resource_dir(app) else {
        return false;
    };

    STEAM_HOOK_DLLS.iter().all(|dll| {
        let Ok(source) = fs::read(resource_dir.join(dll)) else {
            return false;
        };
        let Ok(installed) = fs::read(steam_root.join(dll)) else {
            return false;
        };
        source == installed
    })
}

pub(crate) fn install_hook_files(app: &AppHandle, steam_root: &Path) -> Result<(), String> {
    let resource_dir = resolve_hook_resource_dir(app)?;
    install_hook_files_from(&resource_dir, steam_root)
}

fn install_hook_files_from(resource_dir: &Path, steam_root: &Path) -> Result<(), String> {
    let _guard = HOOK_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Steam hook operation lock is poisoned".to_string())?;
    let operation_id = hook_operation_id();
    let mut staged = Vec::with_capacity(STEAM_HOOK_DLLS.len());

    // Stage every source before touching an installed hook. A missing or
    // quarantined resource therefore cannot leave a partial installation.
    for dll in STEAM_HOOK_DLLS {
        let source = resource_dir.join(dll);
        let staged_path = steam_root.join(format!(".0xolemon-{operation_id}-new-{dll}"));
        let expected_size = match fs::metadata(&source) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                cleanup_files(staged.iter().map(|(_, path)| path));
                return Err(io_error("read", &source, error));
            }
        };
        match fs::copy(&source, &staged_path) {
            Ok(copied) if copied == expected_size => staged.push((dll, staged_path)),
            Ok(copied) => {
                cleanup_files(staged.iter().map(|(_, path)| path));
                let _ = fs::remove_file(&staged_path);
                return Err(format!(
                    "Steam hook staging was incomplete for {dll}: copied {copied} of {expected_size} bytes"
                ));
            }
            Err(error) => {
                cleanup_files(staged.iter().map(|(_, path)| path));
                let _ = fs::remove_file(&staged_path);
                return Err(io_error("stage", &staged_path, error));
            }
        }
    }

    let mut committed: Vec<(&str, PathBuf, Option<PathBuf>)> = Vec::new();
    for (dll, staged_path) in &staged {
        let destination = steam_root.join(dll);
        let previous = if destination.is_file() {
            let previous = steam_root.join(format!(".0xolemon-{operation_id}-previous-{dll}"));
            if let Err(error) = fs::rename(&destination, &previous) {
                rollback_hook_commit(&committed);
                cleanup_files(staged.iter().map(|(_, path)| path));
                return Err(io_error("prepare replacement for", &destination, error));
            }
            Some(previous)
        } else {
            None
        };

        if let Err(error) = fs::rename(staged_path, &destination) {
            if let Some(previous) = &previous {
                let _ = fs::rename(previous, &destination);
            }
            rollback_hook_commit(&committed);
            cleanup_files(staged.iter().map(|(_, path)| path));
            return Err(io_error("install", &destination, error));
        }
        committed.push((dll, destination, previous));
    }

    for (dll, _, previous) in committed {
        if let Some(previous) = previous {
            let backup = steam_root.join(format!("{dll}.backup"));
            let _ = fs::remove_file(&backup);
            if fs::rename(&previous, &backup).is_err() {
                let _ = fs::remove_file(&previous);
            }
        }
    }
    cleanup_files(staged.iter().map(|(_, path)| path));
    Ok(())
}

pub(crate) fn remove_hook_files(steam_root: &Path) -> Result<(), String> {
    let _guard = HOOK_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Steam hook operation lock is poisoned".to_string())?;
    let operation_id = hook_operation_id();
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();

    for file_name in STEAM_HOOK_DLLS
        .into_iter()
        .chain(["0xoLemon.dll", LUA_GAME_MODE_MARKER])
    {
        let installed = steam_root.join(file_name);
        if !installed.exists() {
            continue;
        }
        let quarantined = steam_root.join(format!(
            ".0xolemon-{operation_id}-remove-{}",
            file_name.trim_start_matches('.')
        ));
        if let Err(error) = fs::rename(&installed, &quarantined) {
            for (original, moved_path) in moved.iter().rev() {
                let _ = fs::rename(moved_path, original);
            }
            return Err(io_error("remove", &installed, error));
        }
        moved.push((installed, quarantined));
    }

    // Once all active filenames have moved successfully the hook is disabled.
    // Cleanup failures cannot reactivate it and are safe to retry later.
    cleanup_files(moved.iter().map(|(_, path)| path));
    for dll in STEAM_HOOK_DLLS.into_iter().chain(["0xoLemon.dll"]) {
        let _ = fs::remove_file(steam_root.join(format!("{dll}.backup")));
    }
    Ok(())
}

fn rollback_hook_commit(committed: &[(&str, PathBuf, Option<PathBuf>)]) {
    for (_, destination, previous) in committed.iter().rev() {
        let _ = fs::remove_file(destination);
        if let Some(previous) = previous {
            let _ = fs::rename(previous, destination);
        }
    }
}

fn cleanup_files<'a>(paths: impl Iterator<Item = &'a PathBuf>) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn hook_operation_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{timestamp}", std::process::id())
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> String {
    let guidance = if error.kind() == std::io::ErrorKind::PermissionDenied {
        " Windows denied access; close Steam and run the launcher as administrator once."
    } else {
        ""
    };
    format!("Failed to {action} {}: {error}.{guidance}", path.display())
}

#[cfg(test)]
mod tests {
    use super::{
        hook_files_present, install_hook_files_from, remove_hook_files, LUA_GAME_MODE_MARKER,
        STEAM_HOOK_DLLS,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn hook_install_and_remove_use_the_complete_four_file_package() {
        let (root, resources, steam) = test_directories("complete");
        for dll in STEAM_HOOK_DLLS {
            fs::write(resources.join(dll), format!("new-{dll}")).expect("write source hook");
            fs::write(steam.join(dll), format!("old-{dll}")).expect("write installed hook");
        }
        fs::write(steam.join(LUA_GAME_MODE_MARKER), "enabled").expect("write mode marker");

        install_hook_files_from(&resources, &steam).expect("install complete hook package");
        assert!(hook_files_present(&steam));
        for dll in STEAM_HOOK_DLLS {
            assert_eq!(
                fs::read(steam.join(dll)).expect("read installed hook"),
                fs::read(resources.join(dll)).expect("read source hook")
            );
        }

        remove_hook_files(&steam).expect("remove complete hook package");
        assert!(!hook_files_present(&steam));
        assert!(!steam.join(LUA_GAME_MODE_MARKER).exists());
        cleanup_test_directories(&root, &resources, &steam);
    }

    #[test]
    fn missing_hook_resource_does_not_modify_existing_installation() {
        let (root, resources, steam) = test_directories("missing");
        for dll in STEAM_HOOK_DLLS {
            if dll != "0xoPayload.dll" {
                fs::write(resources.join(dll), format!("new-{dll}")).expect("write source hook");
            }
            fs::write(steam.join(dll), format!("old-{dll}")).expect("write installed hook");
        }

        let error = install_hook_files_from(&resources, &steam)
            .expect_err("incomplete hook package must be rejected");
        assert!(error.contains("0xoPayload.dll"));
        for dll in STEAM_HOOK_DLLS {
            assert_eq!(
                fs::read(steam.join(dll)).expect("read unchanged hook"),
                format!("old-{dll}").as_bytes()
            );
        }

        cleanup_test_directories(&root, &resources, &steam);
    }

    fn test_directories(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "0xolemon-hook-test-{label}-{}-{nonce}",
            std::process::id()
        ));
        let resources = root.join("resources");
        let steam = root.join("steam");
        fs::create_dir_all(&resources).expect("create resource test directory");
        fs::create_dir_all(&steam).expect("create Steam test directory");
        (root, resources, steam)
    }

    fn cleanup_test_directories(root: &Path, resources: &Path, steam: &Path) {
        for dll in STEAM_HOOK_DLLS {
            let _ = fs::remove_file(resources.join(dll));
            let _ = fs::remove_file(steam.join(dll));
            let _ = fs::remove_file(steam.join(format!("{dll}.backup")));
        }
        let _ = fs::remove_file(steam.join("0xoLemon.dll"));
        let _ = fs::remove_file(steam.join("0xoLemon.dll.backup"));
        let _ = fs::remove_file(steam.join(LUA_GAME_MODE_MARKER));
        fs::remove_dir(resources).expect("remove empty resource test directory");
        fs::remove_dir(steam).expect("remove empty Steam test directory");
        fs::remove_dir(root).expect("remove empty test root");
    }
}

#[tauri::command]
pub async fn ost_save_lua(app: AppHandle, appid: String, content: String) -> Result<(), String> {
    let steam_root = get_steam_root()?;
    let lua_dir = steam_root.join("config").join("lua");
    if !lua_dir.exists() {
        fs::create_dir_all(&lua_dir).map_err(|e| format!("Failed to create lua dir: {}", e))?;
    }

    let file_path = lua_dir.join(format!("{}.lua", appid));
    fs::write(&file_path, content).map_err(|e| format!("Failed to write lua script: {}", e))?;
    Ok(())
}
