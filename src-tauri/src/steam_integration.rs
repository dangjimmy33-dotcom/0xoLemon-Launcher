use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::steam::get_steam_path;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
const PENDING_FILE: &str = "steam-shortcuts-pending.json";
const STEAM_SHORTCUT_TAG: &str = "0xoLemon";
static PENDING_WORKER_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingSteamAction {
    action: String,
    game_id: String,
    title: String,
    install_path: String,
    launch_executable: String,
    icon_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamShortcutOutcome {
    pub changed: bool,
    pub queued: bool,
    pub shortcuts_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamEnvironmentInfo {
    pub installed: bool,
    pub running: bool,
    pub root_path: Option<String>,
    pub ui_language: Option<String>,
    pub active_account_id: Option<String>,
    pub library_paths: Vec<String>,
    pub shortcuts_path: Option<String>,
    pub spacewar_installed: bool,
    pub pending_shortcut_actions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartSteamReport {
    pub was_running: bool,
    pub forced: bool,
    pub running: bool,
    pub message: String,
}

pub fn environment_info(app: &AppHandle) -> SteamEnvironmentInfo {
    let root = find_steam_root();
    let account_id = root.as_deref().and_then(|steam_root| {
        most_recent_account_id(steam_root)
            .or_else(|| most_recent_userdata_account(&steam_root.join("userdata")))
    });
    let shortcuts_path = root.as_ref().and_then(|steam_root| {
        account_id.as_ref().map(|account_id| {
            steam_root
                .join("userdata")
                .join(account_id)
                .join("config")
                .join("shortcuts.vdf")
                .display()
                .to_string()
        })
    });

    SteamEnvironmentInfo {
        installed: root.is_some(),
        running: is_steam_running(),
        root_path: root.as_ref().map(|path| path.display().to_string()),
        ui_language: steam_ui_language(),
        active_account_id: account_id,
        library_paths: steam_library_roots()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
        shortcuts_path,
        spacewar_installed: is_spacewar_installed(),
        pending_shortcut_actions: load_pending_actions(app)
            .map(|actions| actions.len())
            .unwrap_or_default(),
    }
}

pub fn is_steam_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("tasklist.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command
            .args(["/FI", "IMAGENAME eq steam.exe", "/FO", "CSV", "/NH"])
            .output();
        return output
            .ok()
            .filter(|result| result.status.success())
            .map(|result| {
                let text = String::from_utf8_lossy(&result.stdout).to_ascii_lowercase();
                text.contains("\"steam.exe\"")
            })
            .unwrap_or(false);
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn open_steam() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = find_steam_root() {
            let executable = root.join("steam.exe");
            if executable.is_file() {
                Command::new(executable)
                    .spawn()
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
        }
        open_steam_uri("steam://open/main")?;
    }
    Ok(())
}

pub fn open_big_picture() -> Result<(), String> {
    open_steam_uri("steam://open/bigpicture")
}

pub fn restart_steam() -> Result<RestartSteamReport, String> {
    #[cfg(target_os = "windows")]
    {
        let root =
            find_steam_root().ok_or_else(|| "Steam installation was not found".to_string())?;
        let executable = root.join("steam.exe");
        if !executable.is_file() {
            return Err(format!(
                "Steam executable was not found at {}",
                executable.display()
            ));
        }

        let was_running = is_steam_running();
        let mut forced = false;
        if was_running {
            let mut shutdown = Command::new(&executable);
            shutdown.creation_flags(CREATE_NO_WINDOW);
            let _ = shutdown.arg("-shutdown").spawn();

            if !wait_for_steam_state(false, Duration::from_secs(12)) {
                let mut terminate = Command::new("taskkill.exe");
                terminate.creation_flags(CREATE_NO_WINDOW);
                let output = terminate
                    .args(["/F", "/IM", "steam.exe"])
                    .output()
                    .map_err(|error| format!("Could not force-close Steam: {error}"))?;
                if !output.status.success() && is_steam_running() {
                    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    return Err(if detail.is_empty() {
                        "Steam did not close after the force-restart request".to_string()
                    } else {
                        format!("Steam did not close: {detail}")
                    });
                }
                forced = true;
                if !wait_for_steam_state(false, Duration::from_secs(8)) {
                    return Err("Steam is still running after it was force-closed".to_string());
                }
            }
        }

        thread::sleep(Duration::from_millis(700));
        Command::new(&executable)
            .spawn()
            .map_err(|error| format!("Could not start Steam again: {error}"))?;
        let running = wait_for_steam_state(true, Duration::from_secs(25));
        if !running {
            return Err(
                "Steam was closed, but it did not start again within 25 seconds".to_string(),
            );
        }

        return Ok(RestartSteamReport {
            was_running,
            forced,
            running,
            message: if forced {
                "Steam was force-closed and started again.".to_string()
            } else if was_running {
                "Steam closed normally and started again.".to_string()
            } else {
                "Steam was not running and has now been started.".to_string()
            },
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Restarting Steam is currently supported only on Windows".to_string())
    }
}

pub fn install_spacewar() -> Result<(), String> {
    open_steam_uri("steam://install/480")
}

pub fn is_spacewar_installed() -> bool {
    if steam_registry_has_spacewar() {
        return true;
    }

    steam_library_roots().into_iter().any(|library| {
        let steamapps = library.join("steamapps");
        if libraryfolders_declares_app(&steamapps.join("libraryfolders.vdf"), "480") {
            return true;
        }
        let manifest = steamapps.join("appmanifest_480.acf");
        if manifest.is_file() {
            let text = fs::read_to_string(&manifest).unwrap_or_default();
            let install_dir = text_vdf_value(&text, "installdir")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Spacewar".to_string());
            let installed_path = steamapps.join("common").join(install_dir);
            if installed_path.is_dir() {
                return true;
            }
        }

        steamapps.join("common").join("Spacewar").is_dir()
    })
}

fn wait_for_steam_state(expected_running: bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if is_steam_running() == expected_running {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(350));
    }
}

fn steam_registry_has_spacewar() -> bool {
    registry_string(r"HKCU\Software\Valve\Steam\Apps\480", "Name")
        .is_some_and(|name| spacewar_registration_name_is_valid(&name))
        || registry_dword(r"HKCU\Software\Valve\Steam\Apps\480", "Installed")
            .is_some_and(|installed| installed != 0)
}

fn spacewar_registration_name_is_valid(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("Spacewar")
}

fn libraryfolders_declares_app(path: &Path, app_id: &str) -> bool {
    fs::read_to_string(path)
        .ok()
        .is_some_and(|text| vdf_declares_app(&text, app_id))
}

fn vdf_declares_app(text: &str, app_id: &str) -> bool {
    text.lines().any(|line| {
        let fields = quoted_fields(line);
        fields.len() >= 2
            && fields[0] == app_id
            && fields[1]
                .chars()
                .all(|character| character.is_ascii_digit())
    })
}

pub fn ensure_game_shortcut(
    app: &AppHandle,
    game_id: &str,
    title: &str,
    install_path: &Path,
    launch_executable: &str,
    icon_path: Option<&Path>,
) -> Result<SteamShortcutOutcome, String> {
    let action = PendingSteamAction {
        action: "add".to_string(),
        game_id: game_id.to_string(),
        title: title.to_string(),
        install_path: install_path.display().to_string(),
        launch_executable: launch_executable.to_string(),
        icon_path: icon_path
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    };

    if is_steam_running() {
        queue_pending_action(app, action)?;
        start_pending_worker(app.clone());
        return Ok(SteamShortcutOutcome {
            changed: false,
            queued: true,
            shortcuts_path: None,
        });
    }

    apply_add_action(&action)
}

pub fn remove_game_shortcut(
    app: &AppHandle,
    game_id: &str,
) -> Result<SteamShortcutOutcome, String> {
    let action = PendingSteamAction {
        action: "remove".to_string(),
        game_id: game_id.to_string(),
        title: String::new(),
        install_path: String::new(),
        launch_executable: String::new(),
        icon_path: String::new(),
    };

    if is_steam_running() {
        queue_pending_action(app, action)?;
        start_pending_worker(app.clone());
        return Ok(SteamShortcutOutcome {
            changed: false,
            queued: true,
            shortcuts_path: None,
        });
    }

    apply_remove_action(&action)
}

pub fn start_pending_worker(app: AppHandle) {
    if PENDING_WORKER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        run_pending_worker(&app);
        PENDING_WORKER_RUNNING.store(false, Ordering::SeqCst);
    });
}

fn run_pending_worker(app: &AppHandle) {
    for _ in 0..2880 {
        let pending = pending_actions_path(app)
            .map(|path| path.is_file())
            .unwrap_or(false);
        if !pending {
            return;
        }
        if !is_steam_running() {
            let remaining = process_pending_actions(app).unwrap_or(1);
            if remaining == 0 {
                return;
            }
        }
        thread::sleep(Duration::from_secs(10));
    }
}

fn process_pending_actions(app: &AppHandle) -> Result<usize, String> {
    if is_steam_running() {
        return Ok(load_pending_actions(app)?.len());
    }

    let actions = load_pending_actions(app)?;
    if actions.is_empty() {
        return Ok(0);
    }

    let mut remaining = Vec::new();
    for action in actions {
        let result = match action.action.as_str() {
            "remove" => apply_remove_action(&action),
            _ => apply_add_action(&action),
        };
        if result.is_err() {
            remaining.push(action);
        }
    }
    save_pending_actions(app, &remaining)?;
    Ok(remaining.len())
}

fn queue_pending_action(app: &AppHandle, action: PendingSteamAction) -> Result<(), String> {
    let mut actions = load_pending_actions(app)?;
    actions.retain(|current| current.game_id != action.game_id);
    actions.push(action);
    save_pending_actions(app, &actions)
}

fn pending_actions_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(PENDING_FILE))
}

fn load_pending_actions(app: &AppHandle) -> Result<Vec<PendingSteamAction>, String> {
    let path = pending_actions_path(app)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn save_pending_actions(app: &AppHandle, actions: &[PendingSteamAction]) -> Result<(), String> {
    let path = pending_actions_path(app)?;
    if actions.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(actions).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn apply_add_action(action: &PendingSteamAction) -> Result<SteamShortcutOutcome, String> {
    let shortcuts_path = active_shortcuts_path()?;
    let launcher = std::env::current_exe().map_err(|error| error.to_string())?;
    let start_dir = launcher.parent().unwrap_or_else(|| Path::new("."));
    let launch_options = shortcut_argument_line(&[
        ("--launch-game", action.game_id.as_str()),
        ("--install-path", action.install_path.as_str()),
        ("--launch-executable", action.launch_executable.as_str()),
    ]);
    let icon = if action.icon_path.trim().is_empty() {
        launcher.display().to_string()
    } else {
        action.icon_path.clone()
    };
    let spec = NonSteamShortcutSpec {
        game_id: action.game_id.clone(),
        app_name: action.title.clone(),
        exe: quoted_path(&launcher),
        start_dir: quoted_path(start_dir),
        icon,
        launch_options,
    };
    let changed = update_shortcuts_file(&shortcuts_path, ShortcutMutation::Add(spec))?;
    Ok(SteamShortcutOutcome {
        changed,
        queued: false,
        shortcuts_path: Some(shortcuts_path.display().to_string()),
    })
}

fn apply_remove_action(action: &PendingSteamAction) -> Result<SteamShortcutOutcome, String> {
    let shortcuts_path = active_shortcuts_path()?;
    if !shortcuts_path.is_file() {
        return Ok(SteamShortcutOutcome {
            changed: false,
            queued: false,
            shortcuts_path: Some(shortcuts_path.display().to_string()),
        });
    }
    let changed = update_shortcuts_file(
        &shortcuts_path,
        ShortcutMutation::Remove(action.game_id.clone()),
    )?;
    Ok(SteamShortcutOutcome {
        changed,
        queued: false,
        shortcuts_path: Some(shortcuts_path.display().to_string()),
    })
}

fn active_shortcuts_path() -> Result<PathBuf, String> {
    let root = find_steam_root().ok_or_else(|| "Steam installation was not found".to_string())?;
    let userdata = root.join("userdata");
    let account_id = most_recent_account_id(&root)
        .or_else(|| most_recent_userdata_account(&userdata))
        .ok_or_else(|| {
            "No Steam user profile was found; sign in to Steam once first".to_string()
        })?;
    let config = userdata.join(account_id).join("config");
    fs::create_dir_all(&config).map_err(|error| error.to_string())?;
    Ok(config.join("shortcuts.vdf"))
}

fn most_recent_account_id(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("config").join("loginusers.vdf")).ok()?;
    let mut current_steam_id: Option<String> = None;
    for line in text.lines() {
        let fields = quoted_fields(line);
        if fields.len() == 1
            && fields[0].chars().all(|ch| ch.is_ascii_digit())
            && fields[0].len() >= 15
        {
            current_steam_id = Some(fields[0].clone());
            continue;
        }
        if fields.len() >= 2 && fields[0].eq_ignore_ascii_case("MostRecent") && fields[1] == "1" {
            if let Some(steam_id) = current_steam_id.as_deref() {
                if let Ok(value) = steam_id.parse::<u64>() {
                    return Some((value & 0xffff_ffff).to_string());
                }
            }
        }
    }
    None
}

fn most_recent_userdata_account(userdata: &Path) -> Option<String> {
    let mut candidates = fs::read_dir(userdata)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            let config = entry.path().join("config");
            let modified = config
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((name, modified))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    candidates.into_iter().next().map(|(name, _)| name)
}

pub fn find_steam_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let registry = [
            (r"HKCU\Software\Valve\Steam", "SteamPath"),
            (r"HKCU\Software\Valve\Steam", "SteamExe"),
            (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
            (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
        ];
        for (key, value) in registry {
            if let Some(path) = registry_string(key, value) {
                let mut candidate = PathBuf::from(path.replace('/', "\\"));
                if candidate
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case("steam.exe"))
                    .unwrap_or(false)
                {
                    candidate.pop();
                }
                if candidate.join("steam.exe").is_file() {
                    return Some(candidate);
                }
            }
        }

        let mut fallbacks = vec![
            PathBuf::from(r"C:\Program Files (x86)\Steam"),
            PathBuf::from(r"C:\Program Files\Steam"),
        ];
        if let Ok(program_files) = std::env::var("ProgramFiles(x86)") {
            fallbacks.push(PathBuf::from(program_files).join("Steam"));
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            fallbacks.push(PathBuf::from(program_files).join("Steam"));
        }
        return fallbacks
            .into_iter()
            .find(|candidate| candidate.join("steam.exe").is_file());
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn registry_string(key: &str, value: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("reg.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command.args(["query", key, "/v", value]).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            for marker in ["REG_EXPAND_SZ", "REG_SZ"] {
                if let Some(index) = line.find(marker) {
                    let result = line[index + marker.len()..].trim();
                    if !result.is_empty() {
                        return Some(result.to_string());
                    }
                }
            }
        }
    }
    let _ = (key, value);
    None
}

fn registry_dword(key: &str, value: &str) -> Option<u32> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("reg.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command.args(["query", key, "/v", value]).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(index) = line.find("REG_DWORD") {
                let raw = line[index + "REG_DWORD".len()..].trim();
                return raw
                    .strip_prefix("0x")
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
                    .or_else(|| raw.parse::<u32>().ok());
            }
        }
    }
    let _ = (key, value);
    None
}

fn steam_ui_language() -> Option<String> {
    registry_string(r"HKCU\Software\Valve\Steam", "Language")
        .filter(|language| !language.trim().is_empty())
}

fn open_steam_uri(uri: &'static str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        command
            .arg(uri)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = uri;
        Err("Steam URI actions are only supported on Windows".to_string())
    }
}

fn steam_library_roots() -> Vec<PathBuf> {
    let Some(root) = find_steam_root() else {
        return Vec::new();
    };
    let mut roots = vec![root.clone()];
    let file = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(file) {
        for line in text.lines() {
            let fields = quoted_fields(line);
            if fields.len() < 2 {
                continue;
            }
            let key = fields[0].as_str();
            let value = unescape_vdf_text(&fields[1]);
            let legacy_path = key.chars().all(|ch| ch.is_ascii_digit())
                && (value.contains(":\\") || value.starts_with("\\\\"));
            if key.eq_ignore_ascii_case("path") || legacy_path {
                let candidate = PathBuf::from(value);
                if !roots.iter().any(|current| paths_equal(current, &candidate)) {
                    roots.push(candidate);
                }
            }
        }
    }
    roots
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left.display()
        .to_string()
        .trim_end_matches(|character| character == '\\' || character == '/')
        .eq_ignore_ascii_case(
            right
                .display()
                .to_string()
                .trim_end_matches(|character| character == '\\' || character == '/'),
        )
}

fn text_vdf_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let fields = quoted_fields(line);
        (fields.len() >= 2 && fields[0].eq_ignore_ascii_case(key)).then(|| fields[1].clone())
    })
}

fn quoted_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if in_quote && character == '\\' {
            escaped = true;
            current.push(character);
            continue;
        }
        if character == '"' {
            if in_quote {
                fields.push(current.clone());
                current.clear();
            }
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            current.push(character);
        }
    }
    fields
}

fn unescape_vdf_text(value: &str) -> String {
    value.replace("\\\\", "\\")
}

#[derive(Debug, Clone)]
struct NonSteamShortcutSpec {
    game_id: String,
    app_name: String,
    exe: String,
    start_dir: String,
    icon: String,
    launch_options: String,
}

enum ShortcutMutation {
    Add(NonSteamShortcutSpec),
    Remove(String),
}

#[derive(Debug, Clone)]
enum VdfValue {
    Object(Vec<(String, VdfValue)>),
    String(String),
    Int32(i32),
    Float32([u8; 4]),
    Pointer([u8; 4]),
    WideString(Vec<u16>),
    Color([u8; 4]),
    Uint64(u64),
    Int64(i64),
    Int8(u8),
    IntZero,
    IntOne,
}

fn update_shortcuts_file(path: &Path, mutation: ShortcutMutation) -> Result<bool, String> {
    let mut document = if path.is_file() {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        parse_vdf_document(&bytes)?
    } else {
        vec![("shortcuts".to_string(), VdfValue::Object(Vec::new()))]
    };

    let shortcuts = document
        .iter_mut()
        .find(|(key, _)| key.eq_ignore_ascii_case("shortcuts"))
        .and_then(|(_, value)| match value {
            VdfValue::Object(entries) => Some(entries),
            _ => None,
        })
        .ok_or_else(|| "Steam shortcuts.vdf has no shortcuts object".to_string())?;

    let before = shortcuts.len();
    let (game_id, replacement) = match mutation {
        ShortcutMutation::Add(spec) => {
            let game_id = spec.game_id.clone();
            (game_id, Some(shortcut_object(spec)))
        }
        ShortcutMutation::Remove(game_id) => (game_id, None),
    };

    let is_add = replacement.is_some();
    shortcuts.retain(|(_, value)| !shortcut_matches_game(value, &game_id));
    if let Some(value) = replacement {
        shortcuts.push((String::new(), value));
    }

    for (index, (key, value)) in shortcuts.iter_mut().enumerate() {
        if matches!(value, VdfValue::Object(_)) {
            *key = index.to_string();
        }
    }

    let changed = is_add || shortcuts.len() != before;
    if !changed {
        return Ok(false);
    }

    let bytes = serialize_vdf_document(&document);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let backup = path.with_extension("vdf.0xolemon.bak");
    if path.is_file() {
        let _ = fs::copy(path, &backup);
    }
    let temporary = path.with_extension("vdf.0xolemon.tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.is_file() && !path.exists() {
            let _ = fs::copy(&backup, path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(true)
}

fn shortcut_matches_game(value: &VdfValue, game_id: &str) -> bool {
    let VdfValue::Object(entries) = value else {
        return false;
    };
    let options = object_string(entries, "LaunchOptions").unwrap_or_default();
    let tokens = options.split_whitespace().collect::<Vec<_>>();
    tokens.windows(2).any(|pair| {
        pair[0].eq_ignore_ascii_case("--launch-game")
            && pair[1].trim_matches('"').eq_ignore_ascii_case(game_id)
    })
}

fn object_string(entries: &[(String, VdfValue)], key: &str) -> Option<String> {
    entries.iter().find_map(|(name, value)| {
        if !name.eq_ignore_ascii_case(key) {
            return None;
        }
        match value {
            VdfValue::String(value) => Some(value.clone()),
            _ => None,
        }
    })
}

fn shortcut_object(spec: NonSteamShortcutSpec) -> VdfValue {
    let app_id = (crc32(format!("{}{}", spec.exe, spec.app_name).as_bytes()) | 0x8000_0000) as i32;
    VdfValue::Object(vec![
        ("appid".to_string(), VdfValue::Int32(app_id)),
        ("AppName".to_string(), VdfValue::String(spec.app_name)),
        ("Exe".to_string(), VdfValue::String(spec.exe)),
        ("StartDir".to_string(), VdfValue::String(spec.start_dir)),
        ("icon".to_string(), VdfValue::String(spec.icon)),
        ("ShortcutPath".to_string(), VdfValue::String(String::new())),
        (
            "LaunchOptions".to_string(),
            VdfValue::String(spec.launch_options),
        ),
        ("IsHidden".to_string(), VdfValue::Int32(0)),
        ("AllowDesktopConfig".to_string(), VdfValue::Int32(1)),
        ("AllowOverlay".to_string(), VdfValue::Int32(1)),
        ("OpenVR".to_string(), VdfValue::Int32(0)),
        ("Devkit".to_string(), VdfValue::Int32(0)),
        ("DevkitGameID".to_string(), VdfValue::String(String::new())),
        ("DevkitOverrideAppID".to_string(), VdfValue::Int32(0)),
        ("LastPlayTime".to_string(), VdfValue::Int32(0)),
        ("FlatpakAppID".to_string(), VdfValue::String(String::new())),
        (
            "tags".to_string(),
            VdfValue::Object(vec![(
                "0".to_string(),
                VdfValue::String(STEAM_SHORTCUT_TAG.to_string()),
            )]),
        ),
    ])
}

fn parse_vdf_document(bytes: &[u8]) -> Result<Vec<(String, VdfValue)>, String> {
    let mut reader = VdfReader { bytes, position: 0 };
    reader.read_entries(false)
}

struct VdfReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> VdfReader<'a> {
    fn read_entries(&mut self, stop_at_end: bool) -> Result<Vec<(String, VdfValue)>, String> {
        let mut entries = Vec::new();
        while self.position < self.bytes.len() {
            let kind = self.read_u8()?;
            if kind == 0x08 {
                if stop_at_end {
                    return Ok(entries);
                }
                continue;
            }
            let key = self.read_cstring()?;
            let value = match kind {
                0x00 => VdfValue::Object(self.read_entries(true)?),
                0x01 => VdfValue::String(self.read_cstring()?),
                0x02 => VdfValue::Int32(i32::from_le_bytes(self.read_array()?)),
                0x03 => VdfValue::Float32(self.read_array()?),
                0x04 => VdfValue::Pointer(self.read_array()?),
                0x05 => VdfValue::WideString(self.read_wide_string()?),
                0x06 => VdfValue::Color(self.read_array()?),
                0x07 => VdfValue::Uint64(u64::from_le_bytes(self.read_array()?)),
                0x09 => VdfValue::Int64(i64::from_le_bytes(self.read_array()?)),
                0x0a => VdfValue::Int8(self.read_u8()?),
                0x0b => VdfValue::IntZero,
                0x0c => VdfValue::IntOne,
                other => return Err(format!("Unsupported binary VDF value type 0x{other:02x}")),
            };
            entries.push((key, value));
        }
        if stop_at_end {
            return Err("Unexpected end of binary VDF object".to_string());
        }
        Ok(entries)
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        let value = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or_else(|| "Unexpected end of binary VDF".to_string())?;
        self.position += 1;
        Ok(value)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let end = self.position.saturating_add(N);
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "Unexpected end of binary VDF numeric value".to_string())?;
        self.position = end;
        slice
            .try_into()
            .map_err(|_| "Invalid binary VDF numeric value".to_string())
    }

    fn read_cstring(&mut self) -> Result<String, String> {
        let start = self.position;
        while self.position < self.bytes.len() && self.bytes[self.position] != 0 {
            self.position += 1;
        }
        if self.position >= self.bytes.len() {
            return Err("Unterminated binary VDF string".to_string());
        }
        let value = String::from_utf8_lossy(&self.bytes[start..self.position]).to_string();
        self.position += 1;
        Ok(value)
    }

    fn read_wide_string(&mut self) -> Result<Vec<u16>, String> {
        let mut value = Vec::new();
        loop {
            let raw: [u8; 2] = self.read_array()?;
            let character = u16::from_le_bytes(raw);
            if character == 0 {
                return Ok(value);
            }
            value.push(character);
        }
    }
}

fn serialize_vdf_document(entries: &[(String, VdfValue)]) -> Vec<u8> {
    let mut output = Vec::new();
    write_vdf_entries(&mut output, entries, false);
    output
}

fn write_vdf_entries(output: &mut Vec<u8>, entries: &[(String, VdfValue)], terminate: bool) {
    for (key, value) in entries {
        let kind = match value {
            VdfValue::Object(_) => 0x00,
            VdfValue::String(_) => 0x01,
            VdfValue::Int32(_) => 0x02,
            VdfValue::Float32(_) => 0x03,
            VdfValue::Pointer(_) => 0x04,
            VdfValue::WideString(_) => 0x05,
            VdfValue::Color(_) => 0x06,
            VdfValue::Uint64(_) => 0x07,
            VdfValue::Int64(_) => 0x09,
            VdfValue::Int8(_) => 0x0a,
            VdfValue::IntZero => 0x0b,
            VdfValue::IntOne => 0x0c,
        };
        output.push(kind);
        output.extend_from_slice(key.as_bytes());
        output.push(0);
        match value {
            VdfValue::Object(entries) => write_vdf_entries(output, entries, true),
            VdfValue::String(value) => {
                output.extend_from_slice(value.as_bytes());
                output.push(0);
            }
            VdfValue::Int32(value) => output.extend_from_slice(&value.to_le_bytes()),
            VdfValue::Float32(value) | VdfValue::Pointer(value) | VdfValue::Color(value) => {
                output.extend_from_slice(value)
            }
            VdfValue::WideString(value) => {
                for character in value {
                    output.extend_from_slice(&character.to_le_bytes());
                }
                output.extend_from_slice(&0_u16.to_le_bytes());
            }
            VdfValue::Uint64(value) => output.extend_from_slice(&value.to_le_bytes()),
            VdfValue::Int64(value) => output.extend_from_slice(&value.to_le_bytes()),
            VdfValue::Int8(value) => output.push(*value),
            VdfValue::IntZero | VdfValue::IntOne => {}
        }
    }
    if terminate {
        output.push(0x08);
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn quoted_path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn shortcut_argument_line(args: &[(&str, &str)]) -> String {
    args.iter()
        .flat_map(|(flag, value)| [(*flag).to_string(), win_arg_quote(value)])
        .collect::<Vec<_>>()
        .join(" ")
}

fn win_arg_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '\\' | '/')
        })
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_vdf_round_trip() {
        let document = vec![(
            "shortcuts".to_string(),
            VdfValue::Object(vec![(
                "0".to_string(),
                shortcut_object(NonSteamShortcutSpec {
                    game_id: "test-game".to_string(),
                    app_name: "Test Game".to_string(),
                    exe: "\"C:\\Test\\launcher.exe\"".to_string(),
                    start_dir: "\"C:\\Test\"".to_string(),
                    icon: "C:\\Test\\game.exe".to_string(),
                    launch_options: "--launch-game test-game".to_string(),
                }),
            )]),
        )];
        let bytes = serialize_vdf_document(&document);
        let parsed = parse_vdf_document(&bytes).expect("parse");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn vdf_path_unescape() {
        assert_eq!(unescape_vdf_text(r"D:\\SteamLibrary"), r"D:\SteamLibrary");
    }
}

// ============================================================================
// Lua-Game Mode DLL Management
// ============================================================================

const LUA_GAME_MODE_MARKER: &str = ".0xo-lua-game-mode-enabled";

/// Check if Lua-Game Mode is currently enabled
pub fn is_lua_game_mode_enabled() -> bool {
    if let Some(steam_root) = get_steam_path() {
        let marker_path = Path::new(&steam_root).join(LUA_GAME_MODE_MARKER);
        marker_path.exists()
    } else {
        false
    }
}

/// Check and update DLLs if launcher version changed (run on startup)
pub fn check_and_update_dlls() -> Result<(), String> {
    // Check if Lua-Game Mode is enabled
    if !is_lua_game_mode_enabled() {
        return Ok(()); // Not enabled, skip update
    }

    let steam_root = get_steam_path()
        .ok_or_else(|| "Steam installation not found".to_string())?;
    let steam_path = Path::new(&steam_root);

    // Get DLL source path (bundled with launcher)
    let dll_source = std::env::current_exe()
        .map_err(|e| format!("Failed to get executable path: {}", e))?
        .parent()
        .ok_or_else(|| "Failed to get parent directory".to_string())?
        .join("resources")
        .join("steam_hooks");

    if !dll_source.exists() {
        return Err(format!("Steam hooks directory not found at: {}", dll_source.display()));
    }

    // List of DLLs to check and update
    let dlls = vec![
        ("0xoCore.dll", "0xoCore.dll"),
        ("0xoPayload.dll", "0xoPayload.dll"),
        ("dwmapi.dll", "dwmapi.dll"),
    ];

    let mut updated = false;

    for (source_name, dest_name) in dlls {
        let source = dll_source.join(source_name);
        let dest = steam_path.join(dest_name);

        if !source.exists() {
            eprintln!("Warning: Source DLL not found: {}", source.display());
            continue;
        }

        // Check if destination exists
        if !dest.exists() {
            // DLL missing, copy it
            fs::copy(&source, &dest)
                .map_err(|e| format!("Failed to copy {}: {}", source_name, e))?;
            updated = true;
            println!("Installed missing DLL: {}", dest_name);
            continue;
        }

        // Compare file sizes and timestamps to detect updates
        let source_meta = fs::metadata(&source)
            .map_err(|e| format!("Failed to read source metadata: {}", e))?;
        let dest_meta = fs::metadata(&dest)
            .map_err(|e| format!("Failed to read dest metadata: {}", e))?;

        let needs_update = source_meta.len() != dest_meta.len()
            || source_meta.modified().ok() > dest_meta.modified().ok();

        if needs_update {
            // Backup existing file
            let backup = steam_path.join(format!("{}.backup", dest_name));
            if dest.exists() {
                fs::copy(&dest, &backup)
                    .map_err(|e| format!("Failed to backup {}: {}", dest_name, e))?;
            }

            // Update DLL
            fs::copy(&source, &dest)
                .map_err(|e| format!("Failed to update {}: {}", source_name, e))?;

            updated = true;
            println!("Updated DLL: {} (size: {} -> {})", 
                dest_name, 
                dest_meta.len(), 
                source_meta.len()
            );
        }
    }

    // Also check CloudRedirect DLL
    let cloudredirect_source = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("resources").join("cloud_redirect").join("0xoCloudRedirect.dll"));

    if let Some(source) = cloudredirect_source {
        if source.exists() {
            let dest = steam_path.join("0xoCloudRedirect.dll");
            let marker = steam_path.join(".0xo-cloud-redirect-enabled");

            // Always install/update CloudRedirect DLL alongside Lua-Game Mode
            // Users can choose to use it or not from the launcher UI
            if !dest.exists() {
                // Missing, install it
                fs::copy(&source, &dest)
                    .map_err(|e| format!("Failed to copy CloudRedirect: {}", e))?;
                
                // Create marker
                if !marker.exists() {
                    fs::write(&marker, "CloudRedirect auto-installed with Lua-Game Mode").ok();
                }
                
                updated = true;
                println!("✅ Installed CloudRedirect DLL");
            } else {
                // Check if needs update
                let source_meta = fs::metadata(&source).ok();
                let dest_meta = fs::metadata(&dest).ok();

                if let (Some(src), Some(dst)) = (source_meta, dest_meta) {
                    if src.len() != dst.len() || src.modified().ok() > dst.modified().ok() {
                        // Backup and update
                        let backup = steam_path.join("0xoCloudRedirect.dll.backup");
                        fs::copy(&dest, &backup).ok();
                        fs::copy(&source, &dest)
                            .map_err(|e| format!("Failed to update CloudRedirect: {}", e))?;
                        
                        // Ensure marker exists
                        if !marker.exists() {
                            fs::write(&marker, "CloudRedirect auto-installed with Lua-Game Mode").ok();
                        }
                        
                        updated = true;
                        println!("✅ Updated CloudRedirect DLL");
                    }
                }
            }
        }
    }

    if updated {
        println!("✅ DLLs updated successfully");
    }

    Ok(())
}

/// Enable Lua-Game Mode by copying DLLs to Steam directory
pub fn enable_lua_game_mode() -> Result<(), String> {
    let steam_root = get_steam_path()
        .ok_or_else(|| "Steam installation not found".to_string())?;
    
    let steam_path = Path::new(&steam_root);
    
    // Get DLL source path (bundled with launcher)
    let dll_source = std::env::current_exe()
        .map_err(|e| format!("Failed to get executable path: {}", e))?
        .parent()
        .ok_or_else(|| "Failed to get parent directory".to_string())?
        .join("resources")
        .join("steam_hooks");
    
    if !dll_source.exists() {
        return Err(format!("Steam hooks directory not found at: {}", dll_source.display()));
    }
    
    // List of DLLs to copy
    let dlls = vec![
        ("0xoCore.dll", "0xoCore.dll"),
        ("0xoPayload.dll", "0xoPayload.dll"),
        ("dwmapi.dll", "dwmapi.dll"),
    ];
    
    // Copy each DLL
    for (source_name, dest_name) in dlls {
        let source = dll_source.join(source_name);
        let dest = steam_path.join(dest_name);
        
        if !source.exists() {
            return Err(format!("DLL not found: {}", source.display()));
        }
        
        // Backup existing file if present
        if dest.exists() {
            let backup = steam_path.join(format!("{}.backup", dest_name));
            fs::copy(&dest, &backup)
                .map_err(|e| format!("Failed to backup {}: {}", dest_name, e))?;
        }
        
        // Copy new DLL
        fs::copy(&source, &dest)
            .map_err(|e| format!("Failed to copy {}: {}", source_name, e))?;
    }
    
    // Create marker file
    let marker_path = steam_path.join(LUA_GAME_MODE_MARKER);
    fs::write(&marker_path, "Lua-Game Mode enabled by 0xoLemon Launcher")
        .map_err(|e| format!("Failed to create marker file: {}", e))?;

    // Auto-install CloudRedirect DLL alongside Lua-Game Mode
    match crate::cloud_redirect_v2::install_dll(
        &std::env::current_exe()
            .map_err(|e| format!("Failed to get exe path: {}", e))?
            .parent()
            .ok_or("Failed to get parent dir")?
            .join("resources")
            .join("cloud_redirect")
            .join("0xoCloudRedirect.dll"),
        steam_path,
    ) {
        Ok(_) => {
            // CloudRedirect installed successfully
            println!("CloudRedirect DLL installed with Lua-Game Mode");
        }
        Err(e) => {
            // Don't fail the entire operation if CloudRedirect fails
            eprintln!("Warning: Failed to install CloudRedirect: {}", e);
        }
    }
    
    Ok(())
}

/// Disable Lua-Game Mode by removing DLLs from Steam directory
pub fn disable_lua_game_mode() -> Result<(), String> {
    let steam_root = get_steam_path()
        .ok_or_else(|| "Steam installation not found".to_string())?;
    
    let steam_path = Path::new(&steam_root);
    
    // If Steam is running, DLLs are locked and cannot be deleted
    // We need to close Steam first
    if is_steam_running() {
        return Err("Steam is currently running. Please close Steam before disabling Lua-Game Mode, or restart Steam to apply changes.".to_string());
    }
    
    // List of DLLs to remove
    let dlls = vec!["0xoCore.dll", "0xoPayload.dll", "dwmapi.dll"];
    
    // Remove each DLL
    for dll_name in dlls {
        let dll_path = steam_path.join(dll_name);
        
        if dll_path.exists() {
            fs::remove_file(&dll_path)
                .map_err(|e| format!("Failed to remove {}: {}", dll_name, e))?;
        }
        
        // Remove backup files (DO NOT RESTORE - these are 0xo DLLs, not original Steam files)
        let backup_path = steam_path.join(format!("{}.backup", dll_name));
        if backup_path.exists() {
            fs::remove_file(&backup_path).ok();
        }
    }
    
    // Remove marker file
    let marker_path = steam_path.join(LUA_GAME_MODE_MARKER);
    if marker_path.exists() {
        fs::remove_file(&marker_path)
            .map_err(|e| format!("Failed to remove marker file: {}", e))?;
    }

    // Also uninstall CloudRedirect DLL
    let _ = crate::cloud_redirect_v2::uninstall_dll(steam_path);
    
    Ok(())
}

/// Find the actual Steam install directory for a game by its Steam appid.
/// Scans all Steam library roots, reads the appmanifest_<appid>.acf, and
/// returns the full path to the game's common folder (e.g. E:\SteamLibrary\steamapps\common\Geometry Dash).
/// Returns None if the game is not found in any Steam library.
#[tauri::command]
pub fn get_steam_game_install_dir(appid: u32) -> Option<String> {
    get_steam_game_install_info(appid).map(|(dir, _)| dir)
}

#[tauri::command]
pub fn get_steam_game_buildid(appid: u32) -> Option<String> {
    for library in steam_library_roots() {
        let steamapps = library.join("steamapps");
        let manifest = steamapps.join(format!("appmanifest_{}.acf", appid));
        if !manifest.is_file() {
            continue;
        }
        let text = fs::read_to_string(&manifest).unwrap_or_default();
        if let Some(buildid) = text_vdf_value(&text, "buildid") {
            return Some(buildid);
        }
    }
    None
}

/// Returns (install_dir, buildid)
pub fn get_steam_game_install_info(appid: u32) -> Option<(String, String)> {
    for library in steam_library_roots() {
        let steamapps = library.join("steamapps");
        let manifest = steamapps.join(format!("appmanifest_{}.acf", appid));
        if !manifest.is_file() {
            continue;
        }
        let text = fs::read_to_string(&manifest).unwrap_or_default();
        let install_dir = text_vdf_value(&text, "installdir")?;
        let buildid = text_vdf_value(&text, "buildid").unwrap_or_else(|| "unknown".to_string());
        
        let game_dir = steamapps.join("common").join(install_dir.trim());
        if game_dir.is_dir() {
            return Some((game_dir.to_string_lossy().into_owned(), buildid));
        }
    }
    None
}

/// Check Windows Defender real-time protection status via registry.
/// Returns: true = realtime protection is ON (bad for lua-game mode)
///          false = realtime protection is OFF (good)
///          None = could not determine (Defender may not be installed)
/// Uses registry_dword which runs reg.exe with CREATE_NO_WINDOW — no console window shown.
#[tauri::command]
pub fn check_defender_realtime_status() -> Option<bool> {
    // DisableRealtimeMonitoring: 0 = protection ON, 1 = protection OFF
    // Key under HKLM (policy/actual state)
    let disable = registry_dword(
        r"HKLM\SOFTWARE\Microsoft\Windows Defender\Real-Time Protection",
        "DisableRealtimeMonitoring",
    );
    match disable {
        Some(0) => Some(true),  // monitoring is NOT disabled → protection is ON
        Some(_) => Some(false), // monitoring is disabled → protection is OFF
        None => {
            // Try the WinDefend service policy key as fallback
            let disable2 = registry_dword(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows Defender\Real-Time Protection",
                "DisableRealtimeMonitoring",
            );
            match disable2 {
                Some(0) => Some(true),
                Some(_) => Some(false),
                None => None, // Can't determine — possibly Defender not present or no access
            }
        }
    }
}
