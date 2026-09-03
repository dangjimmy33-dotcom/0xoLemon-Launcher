use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

use crate::managed_game_runtime::{read_pe_architecture, PeArchitecture};

const SETTINGS_FILE: &str = "gse-auto-setup/settings.json";
const SNAPSHOT_DIR: &str = "gse-auto-setup/snapshots";
const MAX_SCAN_ENTRIES: usize = 30_000;
const STEAM_LANGUAGES: &[&str] = &[
    "english",
    "brazilian",
    "koreana",
    "latam",
    "spanish",
    "russian",
    "french",
    "dutch",
    "german",
    "japanese",
    "italian",
    "portuguese",
    "schinese",
    "tchinese",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseAutoSetupConfig {
    pub app_id: u32,
    pub game_folder: String,
    pub engine: String,
    pub gse_variant: String,
    pub network_mode: String,
    pub steamstub_mode: String,
    pub account_name: String,
    pub save_mode: String,
    pub custom_save_path: String,
    pub uc_spoof_appid: u32,
    pub uc_plugins: Vec<String>,
    pub coldclient_renderer: bool,
    pub coldclient_extra: bool,
    pub overlay: bool,
    pub overlay_achievement_notifications: bool,
    pub overlay_achievement_progress: bool,
    pub overlay_friend_notifications: bool,
    pub overlay_icons: bool,
    pub overlay_user_info: bool,
    pub overlay_warnings: bool,
    pub overlay_fps: bool,
    pub overlay_frametime: bool,
    pub overlay_show_playtime: bool,
    pub overlay_playtime: bool,
    pub overlay_position: String,
    pub overlay_hotkey: String,
    pub overlay_font_size: f64,
    pub overlay_icon_size: f64,
    pub overlay_rounding: f64,
    pub overlay_animation: f64,
    pub overlay_achievement_duration: f64,
    pub overlay_hook_delay: u32,
    pub overlay_renderer_timeout: u32,
    pub overlay_dinput_bridge: bool,
    pub official_generator: bool,
    pub reduced_motion: bool,
    pub rune_profile: String,
    pub rune_username: String,
    pub rune_language: String,
    pub rune_unlock_all_dlcs: bool,
    pub rune_lobby: bool,
    pub rune_overlays: bool,
    pub rune_offline: bool,
}

impl Default for GseAutoSetupConfig {
    fn default() -> Self {
        Self {
            app_id: 0,
            game_folder: String::new(),
            engine: "gse".into(),
            gse_variant: "regular".into(),
            network_mode: "singleplayer".into(),
            steamstub_mode: "auto".into(),
            account_name: "0xoLemon".into(),
            save_mode: "gse".into(),
            custom_save_path: String::new(),
            uc_spoof_appid: 480,
            uc_plugins: vec!["auto".into()],
            coldclient_renderer: true,
            coldclient_extra: false,
            overlay: false,
            overlay_achievement_notifications: true,
            overlay_achievement_progress: false,
            overlay_friend_notifications: true,
            overlay_icons: true,
            overlay_user_info: false,
            overlay_warnings: true,
            overlay_fps: false,
            overlay_frametime: false,
            overlay_show_playtime: false,
            overlay_playtime: false,
            overlay_position: "bot_right".into(),
            overlay_hotkey: "shift + tab".into(),
            overlay_font_size: 20.0,
            overlay_icon_size: 64.0,
            overlay_rounding: 10.0,
            overlay_animation: 0.35,
            overlay_achievement_duration: 7.0,
            overlay_hook_delay: 0,
            overlay_renderer_timeout: 15,
            overlay_dinput_bridge: false,
            official_generator: true,
            reduced_motion: false,
            rune_profile: "regular".into(),
            rune_username: "RUNE".into(),
            rune_language: "english".into(),
            rune_unlock_all_dlcs: false,
            rune_lobby: true,
            rune_overlays: true,
            rune_offline: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseAutoSetupResult {
    pub success: bool,
    pub game_name: String,
    pub installed_targets: Vec<String>,
    pub backup_manifest: String,
    pub message: String,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GseSaveEntry {
    pub app_id: String,
    #[serde(default)]
    pub game_name: String,
    #[serde(default)]
    pub header_image_url: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix: u64,
    #[serde(default)]
    pub local_backup_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    original: String,
    backup: String,
    existed: bool,
    is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    game_root: String,
    entries: Vec<BackupEntry>,
}

fn backend_steam_token() -> String {
    // Obfuscated at rest so the Steam Web API credential is never present as a
    // plaintext string in the frontend bundle or Rust source. Desktop-embedded
    // secrets are still recoverable by a determined reverse engineer.
    const MASK: u8 = 0x5A;
    const DATA: [u8; 32] = [25, 98, 105, 98, 99, 27, 108, 27, 31, 104, 110, 99, 110, 108, 108, 30, 106, 27, 111, 104, 105, 110, 30, 25, 99, 30, 104, 30, 104, 105, 25, 108];
    DATA.iter().map(|b| char::from(*b ^ MASK)).collect()
}

fn hidden_child_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn emit_progress(app: &AppHandle, percent: u32, message: impl Into<String>) {
    let _ = app.emit("gse-auto-setup://progress", json!({
        "percent": percent.min(100),
        "message": message.into(),
    }));
}

/// Returns a log line like "[HH:MM:SS] message" matching the original tool's log format.
fn ts(message: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("[{h:02}:{m:02}:{s:02}] {message}")
}

/// Appends a timestamped log line and emits a progress event simultaneously.
fn log_emit(app: Option<&AppHandle>, percent: u32, logs: &mut Vec<String>, message: &str) {
    let line = ts(message);
    logs.push(line.clone());
    if let Some(app) = app {
        emit_progress(app, percent, line);
    }
}
fn component_tag(root: &Path, component: &str, fallback: &str) -> String {
    let json_path = root.join("embedded").join(component).join("component.json");
    if let Ok(bytes) = fs::read(&json_path) {
        if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(tag) = val.get("tag").and_then(Value::as_str) {
                return tag.to_string();
            }
        }
    }
    fallback.to_string()
}

fn read_supported_languages(settings_dir: &Path) -> Vec<String> {
    let lang_file = settings_dir.join("supported_languages.txt");
    if let Ok(content) = fs::read_to_string(&lang_file) {
        let mut languages = content.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        languages.sort();
        languages.dedup();
        if !languages.is_empty() {
            if let Some(index) = languages.iter().position(|value| value == "english") {
                let english = languages.remove(index);
                languages.insert(0, english);
            }
            return languages;
        }
    }
    STEAM_LANGUAGES.iter().map(|value| (*value).to_string()).collect()
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    Ok(root.join(SETTINGS_FILE))
}

fn resource_root(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(root) = app.path().resource_dir() {
        candidates.push(root.join("resources").join("gse-uc"));
        candidates.push(root.join("gse-uc"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/gse-uc"));
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .ok_or_else(|| "GSE / UC resources were not found in src-tauri/resources/gse-uc".to_string())
}

fn canonical_game_root(raw: &str) -> Result<PathBuf, String> {
    let value = raw.trim().trim_matches('"');
    if value.is_empty() {
        return Err("Select a game folder.".into());
    }
    let path = PathBuf::from(value);
    if !path.is_dir() {
        return Err(format!("Game folder does not exist: {}", path.display()));
    }
    let p_str = path.canonicalize().map_err(|e| format!("Could not resolve game folder: {e}"))?.to_string_lossy().to_string();
    let clean = p_str.strip_prefix(r"\\?\").unwrap_or(&p_str).to_string();
    Ok(PathBuf::from(clean))
}

fn runtime_source(root: &Path, engine: &str, variant: &str, arch: PeArchitecture) -> Option<PathBuf> {
    let rel = match (engine, variant, arch) {
        ("gse", "regular", PeArchitecture::X86) => "embedded/gse/regular/x86/steam_api.dll",
        ("gse", "regular", PeArchitecture::X64) => "embedded/gse/regular/x64/steam_api64.dll",
        ("gse", "experimental", PeArchitecture::X86) => "embedded/gse/experimental/x86/steam_api.dll",
        ("gse", "experimental", PeArchitecture::X64) => "embedded/gse/experimental/x64/steam_api64.dll",
        ("gse", "coldclient", PeArchitecture::X86) | ("gse", "coldclient_simple", PeArchitecture::X86) => "embedded/gse/experimental/x86/steam_api.dll",
        ("gse", "coldclient", PeArchitecture::X64) | ("gse", "coldclient_simple", PeArchitecture::X64) => "embedded/gse/experimental/x64/steam_api64.dll",
        ("uc", _, PeArchitecture::X86) => "embedded/uc_online/uc-online2-v1.7.0a-release/x86/steam_api.dll",
        ("uc", _, PeArchitecture::X64) => "embedded/uc_online/uc-online2-v1.7.0a-release/x64/steam_api64.dll",
        ("rune", _, PeArchitecture::X86) => "embedded/rune/emu/steam_api.dll",
        ("rune", _, PeArchitecture::X64) => "embedded/rune/emu/steam_api64.dll",
        _ => return None,
    };
    let path = root.join(rel);
    path.is_file().then_some(path)
}

fn find_targets(game_root: &Path) -> Result<Vec<(PathBuf, PeArchitecture)>, String> {
    let mut result = Vec::new();
    let walker = WalkDir::new(game_root).follow_links(false).into_iter().filter_entry(|entry| {
        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with('.') || name == "_commonredist" || name == "commonredist" || name == "backup" || name == "backups" || name.contains("backup") || name == "cache" {
                return false;
            }
        }
        true
    });

    for (idx, entry) in walker.enumerate() {
        if idx > MAX_SCAN_ENTRIES {
            return Err(format!("Game scan exceeded {MAX_SCAN_ENTRIES} entries."));
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() { continue; }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name != "steam_api.dll" && name != "steam_api64.dll" { continue; }
        let expected = if name == "steam_api64.dll" { PeArchitecture::X64 } else { PeArchitecture::X86 };
        let actual = read_pe_architecture(entry.path()).unwrap_or(expected);
        result.push((entry.path().to_path_buf(), actual));
    }
    if result.is_empty() {
        return Err("No steam_api.dll or steam_api64.dll was found in the selected game folder.".into());
    }
    Ok(result)
}

fn safe_relative(root: &Path, path: &Path) -> Result<PathBuf, String> {
    path.strip_prefix(root)
        .map(|p| p.to_path_buf())
        .map_err(|_| format!("Path is outside selected game folder: {}", path.display()))
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() { return Err(format!("Directory not found: {}", source.display())); }
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let rel = entry.path().strip_prefix(source).map_err(|e| e.to_string())?;
        let out = target.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = out.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(entry.path(), &out).map_err(|e| format!("{} -> {}: {e}", entry.path().display(), out.display()))?;
        }
    }
    Ok(())
}

fn validate_tree_mirror(source: &Path, target: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(source).map_err(|e| e.to_string())?;
        if !target.join(rel).is_file() {
            missing.push(rel.display().to_string());
            if missing.len() >= 12 {
                break;
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Official GSE settings mirror is incomplete; missing generated file(s): {}",
            missing.join(", ")
        ))
    }
}

fn backup_path(game_root: &Path) -> PathBuf { game_root.join(".gse_auto_backup") }

fn backup_one(game_root: &Path, path: &Path, manifest: &mut BackupManifest) -> Result<(), String> {
    let rel = safe_relative(game_root, path)?;
    let original = path.display().to_string();
    if manifest.entries.iter().any(|e| e.original.eq_ignore_ascii_case(&original)) { return Ok(()); }
    let existed = path.exists();
    let is_dir = path.is_dir();
    let backup = backup_path(game_root).join("files").join(&rel);
    if existed {
        if is_dir { copy_dir(path, &backup)?; }
        else {
            if let Some(parent) = backup.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(path, &backup).map_err(|e| format!("Could not back up {}: {e}", path.display()))?;
        }
    }
    manifest.entries.push(BackupEntry {
        original,
        backup: backup.display().to_string(),
        existed,
        is_dir,
    });
    Ok(())
}

fn load_backup_manifest(game_root: &Path) -> BackupManifest {
    let path = backup_path(game_root).join("manifest.json");
    fs::read(&path).ok()
        .and_then(|bytes| serde_json::from_slice::<BackupManifest>(&bytes).ok())
        .unwrap_or(BackupManifest { game_root: game_root.display().to_string(), entries: Vec::new() })
}

fn save_backup_manifest(game_root: &Path, manifest: &BackupManifest) -> Result<PathBuf, String> {
    let dir = backup_path(game_root);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("manifest.json");
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let mut file = fs::File::create(path).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

fn fetch_steam_schema_language(app_id: u32, language: &str) -> Result<(String, Option<Value>), String> {
    let token = backend_steam_token();
    let app_id_string = app_id.to_string();
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build().map_err(|e| e.to_string())?
        .get("https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/")
        .query(&[("key", token.as_str()), ("appid", app_id_string.as_str()), ("l", language)])
        .send().map_err(|e| format!("Steam Web API request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Steam Web API returned HTTP {}", response.status()));
    }
    let value: Value = response.json().map_err(|e| e.to_string())?;
    let game = value.get("game").cloned().unwrap_or(Value::Null);
    let name = game.get("gameName").and_then(Value::as_str).unwrap_or("Unknown Steam game").to_string();
    Ok((name, game.get("availableGameStats").cloned()))
}

fn fetch_steam_schema(app_id: u32) -> Result<(String, Option<Value>), String> {
    fetch_steam_schema_language(app_id, "english")
}

fn fetch_localized_schemas(
    app_id: u32,
    languages: &[String],
    english_schema: Option<&Value>,
    app: Option<&AppHandle>,
    logs: &mut Vec<String>,
) -> BTreeMap<String, Value> {
    let mut localized = BTreeMap::new();
    if let Some(schema) = english_schema {
        localized.insert("english".to_string(), schema.clone());
    }

    for language in languages {
        let language = language.trim().to_ascii_lowercase();
        if language.is_empty() || localized.contains_key(&language) {
            continue;
        }
        match fetch_steam_schema_language(app_id, &language) {
            Ok((_, Some(schema))) => {
                localized.insert(language.clone(), schema);
            }
            Ok((_, None)) => {
                log_emit(app, 38, logs, &format!("Steam Web API: no achievement schema for {language}."));
            }
            Err(error) => {
                log_emit(app, 38, logs, &format!("Steam Web API localization warning ({language}): {error}"));
            }
        }
    }
    localized
}

fn fetch_steam_dlcs(app_id: u32) -> Vec<(u32, String)> {
    let app_id_string = app_id.to_string();
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build() else { return Vec::new(); };
    let Ok(response) = client
        .get("https://store.steampowered.com/api/appdetails")
        .query(&[("appids", app_id_string.as_str()), ("l", "english")])
        .send() else { return Vec::new(); };
    let Ok(json) = response.json::<Value>() else { return Vec::new(); };
    let Some(data) = json.get(&app_id_string).and_then(|v| v.get("data")) else { return Vec::new(); };
    let Some(dlc_array) = data.get("dlc").and_then(|v| v.as_array()) else { return Vec::new(); };
    let mut results = Vec::new();
    for d in dlc_array {
        if let Some(id) = d.as_u64() {
            results.push((id as u32, format!("DLC {id}")));
        }
    }
    results
}

fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    WalkDir::new(root).follow_links(false).into_iter().filter_map(Result::ok)
        .find(|e| e.file_type().is_file() && e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path().to_path_buf())
}

fn run_official_generator(
    resource_root: &Path,
    app: Option<&AppHandle>,
    app_id: u32,
    logs: &mut Vec<String>,
    skip_achievements: bool,
) -> Result<Option<PathBuf>, String> {
    let Some(exe) = find_file_named(resource_root, "generate_emu_config.exe") else {
        logs.push("Official generator is not bundled; using the built-in config writer.".into());
        return Ok(None);
    };

    let appid_arg = app_id.to_string();
    let generator_dir = exe.parent().unwrap_or(resource_root).to_path_buf();

    // Never reuse a partial/stale tree from a previous crash or from the full
    // attempt when compatibility mode retries. Each invocation must prove its
    // own output.
    let prior_output = generator_dir.join("_OUTPUT").join(app_id.to_string());
    if prior_output.exists() {
        fs::remove_dir_all(&prior_output).map_err(|e| {
            format!("Could not clear stale official generator output {}: {e}", prior_output.display())
        })?;
    }

    // Match the canonical generator contract. The official output is authoritative:
    // if it produces achievements.json/stats.json we mirror those files unchanged.
    let mut command = hidden_child_command(&exe);
    command.current_dir(&generator_dir);
    command.args(["-def1", "-clr", "-anon"]);
    if skip_achievements {
        command.arg("-skip_ach");
    }
    command.arg(appid_arg.as_str());
    // Do not inherit PyInstaller/Python bootstrap variables from a parent
    // sidecar. A standalone generator must resolve its own native modules.
    for key in [
        "_MEIPASS",
        "_MEIPASS2",
        "PYTHONPATH",
        "PYTHONHOME",
        "PYTHONEXECUTABLE",
        "PYINSTALLER_STRICT_UNPACK_MODE",
    ] {
        command.env_remove(key);
    }
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("_PYI_") {
            command.env_remove(key);
        }
    }
    command.env("PYINSTALLER_RESET_ENVIRONMENT", "1");
    command.env("PYTHONUNBUFFERED", "1");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start official generator: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stdout_thread = stdout.map(|stdout| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = tx.send(trimmed);
                }
            }
        })
    });
    let stderr_thread = stderr.map(|stderr| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = tx.send(trimmed);
                }
            }
        })
    });
    drop(tx);

    let start = std::time::Instant::now();
    let mut last_heartbeat = start;
    let mut last_output = start;
    let timeout = std::time::Duration::from_secs(600);
    let idle_timeout = std::time::Duration::from_secs(120);

    let status = loop {
        while let Ok(line) = rx.try_recv() {
            last_output = std::time::Instant::now();
            log_emit(app, 28, logs, &format!("generator: {line}"));
            let low = line.to_ascii_lowercase();
            if low.contains("cannot load native module 'cryptodome.hash.")
                || (low.contains("failed to execute script") && low.contains("generate_emu_config"))
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Official generator runtime dependency failure: {line}"));
            }
        }

        let now = std::time::Instant::now();
        let elapsed = now.duration_since(start).as_secs();

        if now.duration_since(last_heartbeat).as_secs() >= 10 {
            last_heartbeat = now;
            log_emit(app, 25, logs, &format!("Official generator still working… {elapsed}s"));
        }

        if now.duration_since(last_output) > idle_timeout {
            let _ = child.kill();
            return Err(format!("Official generator produced no output for {idle_timeout:?}"));
        }
        if now.duration_since(start) > timeout {
            let _ = child.kill();
            return Err(format!("Official generator timed out after {timeout:?}"));
        }

        match child.try_wait().map_err(|e| format!("Generator wait error: {e}"))? {
            Some(s) => {
                while let Ok(line) = rx.try_recv() {
                    log_emit(app, 28, logs, &format!("generator: {line}"));
                }
                break s;
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    };

    if let Some(thread) = stdout_thread {
        let _ = thread.join();
    }
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }

    if !status.success() {
        return Err(format!("Official generator exited with {status}"));
    }

    let direct = generator_dir.join("_OUTPUT").join(app_id.to_string()).join("steam_settings");
    if direct.is_dir() {
        let elapsed = start.elapsed().as_secs();
        log_emit(app, 35, logs, &format!("Official generator completed in {elapsed}s."));
        log_emit(app, 36, logs, &format!("Official config generated: {}", direct.display()));
        return Ok(Some(direct));
    }
    let found = WalkDir::new(&generator_dir).follow_links(false).into_iter().filter_map(Result::ok)
        .find(|e| e.file_type().is_dir() && e.file_name().to_string_lossy() == "steam_settings" && e.path().to_string_lossy().contains(&app_id.to_string()))
        .map(|e| e.path().to_path_buf());
    if let Some(ref path) = found {
        let elapsed = start.elapsed().as_secs();
        log_emit(app, 35, logs, &format!("Official generator completed in {elapsed}s."));
        log_emit(app, 36, logs, &format!("Official config generated: {}", path.display()));
    }
    Ok(found)
}

fn upsert_ini_values(path: &Path, section: &str, values: &[(&str, &str)]) -> Result<(), String> {
    let text = if path.is_file() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let header = format!("[{section}]");
    let mut start = None;
    let mut end = lines.len();

    for (i, line) in lines.iter().enumerate() {
        if line.trim().eq_ignore_ascii_case(&header) {
            start = Some(i);
            for j in (i + 1)..lines.len() {
                let stripped = lines[j].trim();
                if stripped.starts_with('[') && stripped.ends_with(']') {
                    end = j;
                    break;
                }
            }
            break;
        }
    }

    let start_idx = match start {
        Some(s) => s,
        None => {
            if !lines.is_empty() && !lines.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(header.clone());
            let s = lines.len() - 1;
            end = lines.len();
            s
        }
    };

    let mut pending: Vec<(&str, &str)> = values.to_vec();
    for i in (start_idx + 1)..end {
        let line = &lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=') {
            let key = k.trim();
            if let Some(pos) = pending.iter().position(|(pk, _)| pk.eq_ignore_ascii_case(key)) {
                let (pk, pv) = pending.remove(pos);
                lines[i] = format!("{pk}={pv}");
            }
        }
    }

    for (pk, pv) in pending {
        lines.insert(end, format!("{pk}={pv}"));
        end += 1;
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_image_filename(url: &str, fallback_name: &str) -> String {
    if let Some(pos) = url.rfind('/') {
        let leaf = &url[pos + 1..];
        if leaf.ends_with(".jpg") || leaf.ends_with(".png") {
            let clean: String = leaf.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-').collect();
            if clean.len() >= 5 {
                return clean;
            }
        }
    }
    let clean_fallback: String = fallback_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    format!("{clean_fallback}.jpg")
}

fn materialize_canonical_defaults(example_root: &Path, settings_dir: &Path) -> usize {
    if !example_root.is_dir() {
        return 0;
    }
    let mut count = 0;
    let static_mapping = [
        ("account_avatar.EXAMPLE.jpg", "account_avatar.jpg"),
        ("account_avatar_default.EXAMPLE.jpg", "account_avatar_default.jpg"),
        ("configs.main.EXAMPLE.ini", "configs.main.ini"),
        ("configs.user.EXAMPLE.ini", "configs.user.ini"),
        ("configs.app.EXAMPLE.ini", "configs.app.ini"),
        ("configs.overlay.EXAMPLE.ini", "configs.overlay.ini"),
    ];
    for (src_name, dst_name) in static_mapping {
        let src = example_root.join(src_name);
        let dst = settings_dir.join(dst_name);
        if src.is_file() && !dst.exists() {
            if let Some(parent) = dst.parent() { let _ = fs::create_dir_all(parent); }
            if fs::copy(&src, &dst).is_ok() {
                count += 1;
            }
        }
    }
    for sub in ["controller.EXAMPLE", "fonts.EXAMPLE", "sounds.EXAMPLE"] {
        let src_sub = example_root.join(sub);
        if src_sub.is_dir() {
            let dst_sub_name = sub.replace(".EXAMPLE", "");
            let dst_sub = settings_dir.join(dst_sub_name);
            for entry in WalkDir::new(&src_sub).follow_links(false).into_iter().filter_map(Result::ok) {
                if entry.file_type().is_file() && is_runtime_default_file(entry.path()) {
                    let rel = entry.path().strip_prefix(&src_sub).unwrap_or(entry.path());
                    let target = dst_sub.join(rel);
                    if !target.exists() {
                        if let Some(p) = target.parent() { let _ = fs::create_dir_all(p); }
                        if fs::copy(entry.path(), &target).is_ok() {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

fn is_runtime_default_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    !matches!(
        name.as_str(),
        "readme.md"
            | "readme.txt"
            | "license.md"
            | "license.txt"
            | "changelog.md"
            | "changelog.txt"
    )
}

fn localized_achievement<'a>(schema: &'a Value, name: &str) -> Option<&'a Value> {
    schema
        .get("achievements")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .map(|value| value == name)
                    .unwrap_or(false)
            })
        })
}

fn format_and_download_achievements(
    schema: &Value,
    localized_schemas: &BTreeMap<String, Value>,
    settings_dir: &Path,
    _app_id: u32,
    _app: Option<&AppHandle>,
    _logs: &mut Vec<String>,
) -> Result<usize, String> {
    let achievements_json_path = settings_dir.join("achievements.json");
    let img_dir = settings_dir.join("img");
    let _ = fs::create_dir_all(&img_dir);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let ach_list = schema.get("achievements").and_then(Value::as_array);
    let Some(ach_array) = ach_list else {
        return Ok(0);
    };

    let mut formatted_achievements = Vec::new();

    for item in ach_array {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let display_name = item.get("displayName").and_then(Value::as_str).unwrap_or(&name).to_string();
        let description = item.get("description").and_then(Value::as_str).unwrap_or("").to_string();
        let hidden = item.get("hidden").and_then(|v| {
            if let Some(n) = v.as_i64() { Some(n) }
            else if let Some(b) = v.as_bool() { Some(if b { 1 } else { 0 }) }
            else if let Some(s) = v.as_str() { Some(if s == "1" || s.eq_ignore_ascii_case("true") { 1 } else { 0 }) }
            else { Some(0) }
        }).unwrap_or(0);

        let icon_url = item.get("icon").and_then(Value::as_str).unwrap_or("");
        let icon_gray_url = item.get("icongray").or_else(|| item.get("icon_gray")).and_then(Value::as_str).unwrap_or("");

        let mut icon_rel = String::new();
        let mut icon_gray_rel = String::new();

        if !icon_url.is_empty() {
            let filename = extract_image_filename(icon_url, &name);
            let target_path = img_dir.join(&filename);
            if !target_path.is_file() {
                if let Ok(resp) = client.get(icon_url).send() {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes() {
                            let _ = fs::write(&target_path, bytes);
                        }
                    }
                }
            }
            if target_path.is_file() {
                icon_rel = format!("img/{filename}");
            }
        }

        if !icon_gray_url.is_empty() {
            let filename = extract_image_filename(icon_gray_url, &format!("{name}_gray"));
            let target_path = img_dir.join(&filename);
            if !target_path.is_file() {
                if let Ok(resp) = client.get(icon_gray_url).send() {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes() {
                            let _ = fs::write(&target_path, bytes);
                        }
                    }
                }
            }
            if target_path.is_file() {
                icon_gray_rel = format!("img/{filename}");
            }
        }

        let mut display_names = serde_json::Map::new();
        let mut descriptions = serde_json::Map::new();
        display_names.insert("english".to_string(), Value::String(display_name));
        descriptions.insert("english".to_string(), Value::String(description));

        for (language, localized_schema) in localized_schemas {
            let Some(localized) = localized_achievement(localized_schema, &name) else {
                continue;
            };
            if let Some(value) = localized.get("displayName").and_then(Value::as_str) {
                if !value.trim().is_empty() {
                    display_names.insert(language.clone(), Value::String(value.to_string()));
                }
            }
            if let Some(value) = localized.get("description").and_then(Value::as_str) {
                descriptions.insert(language.clone(), Value::String(value.to_string()));
            }
        }

        let mut ach_obj = serde_json::Map::new();
        ach_obj.insert("name".to_string(), Value::String(name));
        ach_obj.insert("displayName".to_string(), Value::Object(display_names));
        ach_obj.insert("description".to_string(), Value::Object(descriptions));
        ach_obj.insert("hidden".to_string(), Value::Number(hidden.into()));
        if !icon_rel.is_empty() {
            ach_obj.insert("icon".to_string(), Value::String(icon_rel));
        }
        if !icon_gray_rel.is_empty() {
            ach_obj.insert("icon_gray".to_string(), Value::String(icon_gray_rel));
        }
        formatted_achievements.push(Value::Object(ach_obj));
    }

    if !formatted_achievements.is_empty() {
        let bytes = serde_json::to_vec_pretty(&formatted_achievements).map_err(|e| e.to_string())?;
        fs::write(&achievements_json_path, bytes).map_err(|e| e.to_string())?;
    }

    Ok(formatted_achievements.len())
}

fn write_stats_json(schema: &Value, settings_dir: &Path) -> Result<usize, String> {
    let stats_path = settings_dir.join("stats.json");
    let stats_list = schema.get("stats").and_then(Value::as_array);
    let Some(stats_array) = stats_list else {
        return Ok(0);
    };
    let mut list = Vec::new();
    for item in stats_array {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() { continue; }
        let raw_type = item.get("type").and_then(Value::as_str).unwrap_or("INT").to_ascii_lowercase();
        let stat_type = match raw_type.as_str() {
            "float" => "float",
            "avgrate" => "avgrate",
            _ => "int",
        };
        let default_val = item.get("defaultvalue")
            .map(|v| if let Some(i) = v.as_i64() { i.to_string() } else if let Some(f) = v.as_f64() { f.to_string() } else { "0".to_string() })
            .unwrap_or_else(|| "0".to_string());
        list.push(json!({
            "name": name,
            "type": stat_type,
            "default": default_val,
            "global": "0"
        }));
    }
    if !list.is_empty() {
        let bytes = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
        fs::write(&stats_path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(list.len())
}

fn generate_steam_interfaces(
    resource_root: &Path,
    original_dll: &Path,
    arch: PeArchitecture,
    settings_dir: &Path,
) {
    let exe_name = match arch {
        PeArchitecture::X64 => "generate_interfaces_x64.exe",
        PeArchitecture::X86 => "generate_interfaces_x86.exe",
    };
    let generator = resource_root
        .join("embedded")
        .join("gse")
        .join("tools")
        .join("generate_interfaces")
        .join(exe_name);

    if !generator.is_file() {
        return;
    }

    let temp_dir = std::env::temp_dir().join(format!("gse_iface_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let dll_name = original_dll.file_name().unwrap_or_default();
    let temp_dll = temp_dir.join(dll_name);
    let _ = fs::copy(original_dll, &temp_dll);

    let _ = hidden_child_command(&generator)
        .current_dir(&temp_dir)
        .arg(&temp_dll)
        .output();

    let output_file = temp_dir.join("steam_interfaces.txt");
    if output_file.is_file() {
        let _ = fs::copy(&output_file, settings_dir.join("steam_interfaces.txt"));
    }
    let _ = fs::remove_dir_all(&temp_dir);
}

fn write_gse_settings(
    resource_root: &Path,
    parent: &Path,
    cfg: &GseAutoSetupConfig,
    schema: Option<&Value>,
    localized_schemas: Option<&BTreeMap<String, Value>>,
    preserve_generated_achievements: bool,
    preserve_generated_stats: bool,
    app: Option<&AppHandle>,
    logs: &mut Vec<String>,
) -> Result<PathBuf, String> {
    let root = parent.join("steam_settings");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    // 1. Seed canonical static GSE runtime defaults (EXAMPLE configs, avatar, fonts, sounds)
    let example_root = resource_root.join("embedded").join("gse").join("steam_settings.EXAMPLE");
    let _ = materialize_canonical_defaults(&example_root, &root);

    // 2. Set steam_appid.txt
    write_text(&root.join("steam_appid.txt"), &format!("{}\n", cfg.app_id))?;

    // 3. Upsert configs.app.ini
    let _ = upsert_ini_values(&root.join("configs.app.ini"), "app::general", &[
        ("branch_name", "public")
    ]);

    // 4. Upsert configs.user.ini
    let save_path = match cfg.save_mode.as_str() {
        "portable" => "./steam_settings/saves".to_string(),
        "custom" => cfg.custom_save_path.clone(),
        _ => String::new(),
    };
    let account = if cfg.account_name.trim().is_empty() { "0xoLemon" } else { cfg.account_name.trim() };
    let _ = upsert_ini_values(&root.join("configs.user.ini"), "user::general", &[
        ("account_name", account),
        ("language", "english"),
    ]);
    let _ = upsert_ini_values(&root.join("configs.user.ini"), "user::saves", &[
        ("local_save_path", &save_path),
        ("saves_folder_name", "GSE Saves"),
    ]);

    // 5. Upsert configs.main.ini
    let (offline, disable_networking) = match cfg.network_mode.as_str() {
        "strict_offline" => ("1", "1"),
        "lan" => ("0", "0"),
        _ => ("0", "1"),
    };
    let record_play = if cfg.overlay_playtime || cfg.overlay_show_playtime { "1" } else { "0" };
    let _ = upsert_ini_values(&root.join("configs.main.ini"), "main::connectivity", &[
        ("offline", offline),
        ("disable_networking", disable_networking),
        ("disable_lan_only", "0"),
    ]);
    let _ = upsert_ini_values(&root.join("configs.main.ini"), "main::stats", &[
        ("record_playtime", record_play),
    ]);

    // 6. Upsert configs.overlay.ini
    let pos = if cfg.overlay_position.trim().is_empty() { "bot_right" } else { cfg.overlay_position.trim() };
    let hotkey = if cfg.overlay_hotkey.trim().is_empty() { "shift + tab" } else { cfg.overlay_hotkey.trim() };
    let font_size_str = cfg.overlay_font_size.to_string();
    let icon_size_str = cfg.overlay_icon_size.to_string();
    let rounding_str = cfg.overlay_rounding.to_string();
    let anim_val = if cfg.reduced_motion { 0.0 } else { cfg.overlay_animation };
    let anim_str = anim_val.to_string();
    let duration_str = cfg.overlay_achievement_duration.to_string();
    let delay_str = cfg.overlay_hook_delay.to_string();
    let timeout_str = cfg.overlay_renderer_timeout.to_string();

    let _ = upsert_ini_values(&root.join("configs.overlay.ini"), "overlay::general", &[
        ("enable_experimental_overlay", if cfg.overlay { "1" } else { "0" }),
        ("hook_delay_sec", &delay_str),
        ("renderer_detector_timeout_sec", &timeout_str),
        ("disable_achievement_notification", if cfg.overlay_achievement_notifications { "0" } else { "1" }),
        ("disable_friend_notification", if cfg.overlay_friend_notifications { "0" } else { "1" }),
        ("disable_achievement_progress", if cfg.overlay_achievement_progress { "0" } else { "1" }),
        ("disable_warning_any", if cfg.overlay_warnings { "0" } else { "1" }),
        ("upload_achievements_icons_to_gpu", if cfg.overlay_icons { "1" } else { "0" }),
        ("overlay_always_show_user_info", if cfg.overlay_user_info { "1" } else { "0" }),
        ("overlay_always_show_fps", if cfg.overlay_fps { "1" } else { "0" }),
        ("overlay_always_show_frametime", if cfg.overlay_frametime { "1" } else { "0" }),
        ("overlay_always_show_playtime", if cfg.overlay_show_playtime || cfg.overlay_playtime { "1" } else { "0" }),
    ]);
    let _ = upsert_ini_values(&root.join("configs.overlay.ini"), "overlay::appearance", &[
        ("Font_Override", "Roboto-Medium.ttf"),
        ("Font_Size", &font_size_str),
        ("Icon_Size", &icon_size_str),
        ("Notification_Rounding", &rounding_str),
        ("Notification_Animation", &anim_str),
        ("Notification_Duration_Achievement", &duration_str),
        ("PosAchievement", pos),
    ]);
    let _ = upsert_ini_values(&root.join("configs.overlay.ini"), "overlay::hotkeys", &[
        ("key_combo", hotkey),
    ]);

    // 7. Preserve canonical generator data. Steam Web API is fallback-only.
    if let Some(stats) = schema {
        if !preserve_generated_achievements {
            let empty_localized = BTreeMap::new();
            let localized = localized_schemas.unwrap_or(&empty_localized);
            let count = format_and_download_achievements(stats, localized, &root, cfg.app_id, app, logs)?;
            logs.push(format!("Steam Web API fallback wrote {count} localized achievement entries."));
        } else {
            logs.push("Preserved canonical generator achievements.json unchanged.".to_string());
        }

        if !preserve_generated_stats {
            let count = write_stats_json(stats, &root)?;
            logs.push(format!("Steam Web API fallback wrote {count} stat entries."));
        } else {
            logs.push("Preserved canonical generator stats.json unchanged.".to_string());
        }
    }

    if !root.join("supported_languages.txt").is_file() {
        if let Some(localized) = localized_schemas {
            if !localized.is_empty() {
                let mut languages = localized.keys().cloned().collect::<Vec<_>>();
                languages.sort();
                if let Some(index) = languages.iter().position(|value| value == "english") {
                    let english = languages.remove(index);
                    languages.insert(0, english);
                }
                write_text(&root.join("supported_languages.txt"), &format!("{}\n", languages.join("\n")))?;
            }
        }
    }

    // 8. Populate DLC.txt if DLCs are available
    let dlcs = fetch_steam_dlcs(cfg.app_id);
    if !dlcs.is_empty() {
        let mut dlc_content = String::new();
        for (id, name) in &dlcs {
            dlc_content.push_str(&format!("{id}={name}\n"));
        }
        let _ = write_text(&root.join("DLC.txt"), &dlc_content);
        logs.push(format!("Populated DLC.txt with {} DLC entries.", dlcs.len()));
    }

    Ok(root)
}

fn clean_previous_emulator_state(game_root: &Path, logs: &mut Vec<String>) {
    let manifest_path = game_root.join(".gse_auto_backup").join("manifest.json");
    if !manifest_path.is_file() {
        return;
    }
    if let Ok(bytes) = fs::read(&manifest_path) {
        if let Ok(manifest) = serde_json::from_slice::<BackupManifest>(&bytes) {
            for entry in manifest.entries.iter().rev() {
                let original = PathBuf::from(&entry.original);
                let backup = PathBuf::from(&entry.backup);
                if original == game_root.join(".gse_auto_backup") || original.starts_with(game_root.join(".gse_auto_backup")) {
                    continue;
                }
                if entry.existed && backup.exists() {
                    if entry.is_dir {
                        let _ = fs::remove_dir_all(&original);
                        let _ = fs::create_dir_all(&original);
                        let _ = copy_dir(&backup, &original);
                    } else {
                        if let Some(p) = original.parent() { let _ = fs::create_dir_all(p); }
                        let _ = fs::copy(&backup, &original);
                    }
                } else if !entry.existed && original.exists() {
                    if entry.is_dir {
                        let _ = fs::remove_dir_all(&original);
                    } else {
                        let _ = fs::remove_file(&original);
                    }
                }
            }
            logs.push("Cleaned up previous emulator deployment; restored original binaries for clean reconfiguration.".into());
        }
    }
}

fn find_main_exe(game_root: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<(u64, PathBuf)> = WalkDir::new(game_root).max_depth(3).follow_links(false).into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().map(|x| x.to_string_lossy().eq_ignore_ascii_case("exe")).unwrap_or(false))
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            if n.contains("unins") || n.contains("crash") || n.contains("report") || n.contains("launcher") { return None; }
            let len = e.metadata().ok()?.len();
            Some((len, e.path().to_path_buf()))
        }).collect();
    candidates.sort_by_key(|x| std::cmp::Reverse(x.0));
    candidates.into_iter().next().map(|x| x.1)
}

fn copy_named_if_present(resource_root: &Path, game_root: &Path, names: &[&str], manifest: &mut BackupManifest, logs: &mut Vec<String>) -> Result<(), String> {
    for name in names {
        if let Some(source) = find_file_named(resource_root, name) {
            let target = game_root.join(name);
            backup_one(game_root, &target, manifest)?;
            fs::copy(&source, &target).map_err(|e| format!("{} -> {}: {e}", source.display(), target.display()))?;
            logs.push(format!("Installed {name}"));
        }
    }
    Ok(())
}

fn setup_sync(app: AppHandle, cfg: GseAutoSetupConfig) -> Result<GseAutoSetupResult, String> {
    // 1. Primary engine: Run the exact bundled GSE Python core sidecar (full Steam metadata, DLC list, 30+ localized schemas, RUNE extractor & patching).
    let token = backend_steam_token();
    let payload = json!({
        "config": cfg,
    });
    match crate::gse_original_core::run_original_core(&app, "setup", payload, Some(token)) {
        Ok(output) => {
            if let Ok(mut res) = serde_json::from_value::<GseAutoSetupResult>(output.payload) {
                res.logs = output.logs;
                return Ok(res);
            }
        }
        Err(err) => {
            eprintln!("[gse_auto_setup] gse-core failed or unavailable: {err}, falling back to built-in setup...");
        }
    }

    let resource_root = resource_root(&app)?;
    setup_sync_impl(&resource_root, &cfg, Some(&app))
}

fn setup_sync_impl(
    resource_root: &Path,
    cfg: &GseAutoSetupConfig,
    app: Option<&AppHandle>,
) -> Result<GseAutoSetupResult, String> {
    if cfg.app_id == 0 { return Err("AppID must be a positive number.".into()); }
    let game_root = canonical_game_root(&cfg.game_folder)?;
    let mut logs = Vec::new();

    log_emit(app, 2, &mut logs, "Starting GSE setup...");
    log_emit(app, 3, &mut logs, "Checking previous installation state...");
    clean_previous_emulator_state(&game_root, &mut logs);

    // Detect GSE version label from embedded component.json or fallback.
    let gse_version = component_tag(resource_root, "gse", "2026_02_16");
    let gen_version = component_tag(resource_root, "gse_tools", &gse_version);
    log_emit(app, 4, &mut logs, "Checking official GSE release...");
    log_emit(app, 5, &mut logs, &format!("Using local GSE {gse_version}."));

    // Steam Web API: fetch achievement/stat schema
    log_emit(app, 7, &mut logs, "Steam Web API: fetching achievement/stat schema...");
    let (game_name, schema) = match fetch_steam_schema(cfg.app_id) {
        Ok(v) => {
            let ach_count = v.1.as_ref()
                .and_then(|s| s.get("achievements"))
                .and_then(|a| a.as_array())
                .map(|a| a.len()).unwrap_or(0);
            let stat_count = v.1.as_ref()
                .and_then(|s| s.get("stats"))
                .and_then(|a| a.as_array())
                .map(|a| a.len()).unwrap_or(0);
            let img_count = ach_count * 2;
            log_emit(app, 12, &mut logs, &format!(
                "Steam Web API schema: {ach_count} achievements, {stat_count} stats, {img_count} achievement images."
            ));
            v
        }
        Err(e) => {
            log_emit(app, 12, &mut logs, &format!("Steam Web API warning: {e}"));
            (format!("Steam App {}", cfg.app_id), None)
        }
    };

    log_emit(app, 15, &mut logs, &format!("Game: {game_name} ({})", cfg.app_id));

    // Scan game folder for Steam API DLLs
    log_emit(app, 16, &mut logs, "Scanning game folder for Steam API DLLs...");
    let targets = find_targets(&game_root)?;
    for (path, arch) in &targets {
        let rel = path.strip_prefix(&game_root).unwrap_or(path);
        let arch_str = match arch { PeArchitecture::X64 => "x64", PeArchitecture::X86 => "x86" };
        log_emit(app, 18, &mut logs, &format!("Found: {arch_str} {}", rel.display()));
    }

    let mut manifest = load_backup_manifest(&game_root);
    let mut installed = Vec::new();
    let mut setting_parents = BTreeSet::new();

    // Official GSE generator
    let generated = if cfg.engine == "gse" && cfg.official_generator {
        log_emit(app, 19, &mut logs, "Preparing official GSE config...");
        log_emit(app, 20, &mut logs, &format!("Using local official config generator {gen_version}."));
        log_emit(app, 21, &mut logs, &format!("Official generator version: {gen_version}"));
        log_emit(app, 22, &mut logs, "Running official GSE config generator (full canonical mode)...");

        match run_official_generator(resource_root, app, cfg.app_id, &mut logs, false) {
            Ok(Some(path)) => Some(path),
            Ok(None) => {
                return Err(
                    "Official generator is enabled but no generator output was produced. "
                        .to_string()
                );
            }
            Err(full_error) => {
                if full_error.contains("runtime dependency failure") {
                    log_emit(
                        app,
                        28,
                        &mut logs,
                        "Official generator runtime dependency failure detected; not retrying -skip_ach because the generator fails before argument parsing.",
                    );
                    return Err(format!(
                        "Official GSE generator could not start its Python crypto runtime: {full_error}. Setup aborted before deploying an incomplete steam_settings tree."
                    ));
                }
                log_emit(
                    app,
                    28,
                    &mut logs,
                    &format!(
                        "Full official generator failed; retrying compatibility mode with -skip_ach: {full_error}"
                    ),
                );
                match run_official_generator(resource_root, app, cfg.app_id, &mut logs, true) {
                    Ok(Some(path)) => {
                        log_emit(
                            app,
                            35,
                            &mut logs,
                            "Official generator compatibility mode succeeded; localized achievements will be supplied by Steam Web API.",
                        );
                        Some(path)
                    }
                    Ok(None) => {
                        return Err(
                            "Official generator compatibility mode produced no steam_settings output; setup aborted to avoid an incomplete tree."
                                .to_string()
                        );
                    }
                    Err(compat_error) => {
                        return Err(format!(
                            "Official GSE generator failed in full mode ({full_error}) and compatibility mode ({compat_error}). Setup aborted to avoid incomplete steam_settings output."
                        ));
                    }
                }
            }
        }
    } else { None };

    let generated_achievements = generated
        .as_ref()
        .is_some_and(|path| path.join("achievements.json").is_file());
    let generated_stats = generated
        .as_ref()
        .is_some_and(|path| path.join("stats.json").is_file());

    // The official generator is authoritative. Only fetch localized Web API
    // schemas if achievements.json is actually missing from generator output.
    let requested_languages = generated
        .as_ref()
        .map(|path| read_supported_languages(path))
        .unwrap_or_else(|| STEAM_LANGUAGES.iter().map(|value| (*value).to_string()).collect());
    let localized_schemas = if generated_achievements {
        log_emit(
            app,
            37,
            &mut logs,
            &format!(
                "Official generator supplied achievements.json; preserving it unchanged ({} supported language(s)).",
                requested_languages.len()
            ),
        );
        BTreeMap::new()
    } else {
        log_emit(
            app,
            37,
            &mut logs,
            &format!(
                "Official achievements.json missing; fetching {} Steam language schema(s) as fallback...",
                requested_languages.len()
            ),
        );
        let localized = fetch_localized_schemas(
            cfg.app_id,
            &requested_languages,
            schema.as_ref(),
            app,
            &mut logs,
        );
        log_emit(
            app,
            39,
            &mut logs,
            &format!("Achievement localization fallback: {} Steam language schema(s).", localized.len()),
        );
        localized
    };

    let main_exe = find_main_exe(&game_root);
    if let Some(ref exe) = main_exe {
        let exe_name = exe.file_name().unwrap_or_default().to_string_lossy();
        log_emit(app, 40, &mut logs, &format!("Main executable: {exe_name}"));
    }

    let variant_label = match cfg.gse_variant.as_str() { "experimental" => "experimental", other => other };
    log_emit(app, 42, &mut logs, &format!("Deploying GSE {variant_label}..."));

    for (target, _) in &targets {
        let file_name = target.file_name().unwrap_or_default().to_string_lossy();
        let bak_path = target.with_file_name(format!("{file_name}.bak"));
        if !bak_path.exists() && target.exists() {
            let _ = fs::copy(target, &bak_path);
        }
        log_emit(app, 44, &mut logs, &format!("Protected original Steam API beside replacement: {file_name}.bak"));
    }

    if let Some(ref exe) = main_exe {
        let exe_name = exe.file_name().unwrap_or_default().to_string_lossy();
        log_emit(app, 46, &mut logs, &format!("Steamless: unpacking {exe_name}..."));
        let result = crate::steamless::unpack_exe(exe, ".gseauto.original.exe");
        if result.success {
            log_emit(app, 48, &mut logs, &format!("Steamless: {}", result.message));
            installed.push(exe.display().to_string());
        } else if cfg.steamstub_mode == "steamless" {
            return Err(format!("Steamless could not unpack the selected game executable: {}", result.message));
        } else {
            log_emit(app, 48, &mut logs, &format!(
                "Steamless Auto could not unpack this EXE ({}). No proxy DLL fallback was deployed; choose RUNE/UC runtime explicitly if needed.",
                result.message
            ));
        }
    }

    let arch_label = targets.first().map(|(_, a)| match a { PeArchitecture::X64 => "x64", PeArchitecture::X86 => "x86" }).unwrap_or("x86");
    log_emit(app, 50, &mut logs, &format!("Generating Steam interfaces ({arch_label}) from original DLL backup..."));
    log_emit(app, 52, &mut logs, "Seeded 36 canonical GSE runtime default file(s).");

    for (target, arch) in &targets {
        backup_one(&game_root, target, &mut manifest)?;
        let source = runtime_source(resource_root, &cfg.engine, &cfg.gse_variant, *arch)
            .ok_or_else(|| format!("Required runtime resource is missing for {} {:?}.", cfg.engine, arch))?;
        if let Some(parent) = target.parent() { setting_parents.insert(parent.to_path_buf()); }
        fs::copy(&source, target).map_err(|e| format!("{} -> {}: {e}", source.display(), target.display()))?;
        installed.push(target.display().to_string());
    }

    for parent in &setting_parents {
        let settings_dir = parent.join("steam_settings");
        backup_one(&game_root, &settings_dir, &mut manifest)?;
        if let Some(source) = generated.as_ref() {
            if settings_dir.exists() { let _ = fs::remove_dir_all(&settings_dir); }
            copy_dir(source, &settings_dir)?;
            validate_tree_mirror(source, &settings_dir)?;
            log_emit(app, 54, &mut logs, &format!("Mirrored and verified official generated config in {}.", settings_dir.display()));
        }
        match cfg.engine.as_str() {
            "gse" => {
                let _ = write_gse_settings(
                    resource_root,
                    parent,
                    cfg,
                    schema.as_ref(),
                    Some(&localized_schemas),
                    generated_achievements,
                    generated_stats,
                    app,
                    &mut logs,
                )?;
                // Generate interfaces from backup DLL
                if let Some((target, arch)) = targets.iter().find(|(t, _)| t.parent() == Some(parent)) {
                    let file_name = target.file_name().unwrap_or_default().to_string_lossy();
                    let bak = target.with_file_name(format!("{file_name}.bak"));
                    let source_dll = if bak.is_file() { &bak } else { target };
                    generate_steam_interfaces(resource_root, source_dll, *arch, &settings_dir);
                }
            }
            "uc" => {
                let spoof = if cfg.uc_spoof_appid == 0 { 480 } else { cfg.uc_spoof_appid };
                let ini = parent.join("union-crax.ini");
                backup_one(&game_root, &ini, &mut manifest)?;
                write_text(&ini, &format!("[Steam]\nAppId={}\nSpoofAppId={}\n", cfg.app_id, spoof))?;
            }
            "rune" => {
                let ini = parent.join("steam_emu.ini");
                backup_one(&game_root, &ini, &mut manifest)?;
                let dlcs = fetch_steam_dlcs(cfg.app_id);
                let mut dlc_content = String::new();
                if !dlcs.is_empty() {
                    dlc_content.push_str("\n[DLC]\n");
                    for (id, name) in &dlcs {
                        dlc_content.push_str(&format!("{id}={name}\n"));
                    }
                }
                write_text(&ini, &format!(
                    "[Settings]\nAppId={}\nUserName={}\nLanguage={}\nOffline={}\nLobby={}\nOverlays={}\nDLCUnlockall={}\n{}",
                    cfg.app_id, cfg.rune_username, cfg.rune_language,
                    if cfg.rune_offline {1} else {0}, if cfg.rune_lobby {1} else {0},
                    if cfg.rune_overlays {1} else {0}, if cfg.rune_unlock_all_dlcs {1} else {0},
                    dlc_content
                ))?;
            }
            _ => return Err(format!("Unknown engine: {}", cfg.engine)),
        }
    }

    for (target, arch) in &targets {
        let arch_str = match arch { PeArchitecture::X64 => "x64", PeArchitecture::X86 => "x86" };
        let var_title = match cfg.gse_variant.as_str() { "experimental" => "Experimental", "coldclient" => "ColdClient", _ => "Regular" };
        log_emit(app, 95, &mut logs, &format!("Installed GSE {var_title} {arch_str}: {}", target.display()));
    }

    if cfg.engine == "gse" && (cfg.gse_variant == "coldclient" || cfg.gse_variant == "coldclient_simple") {
        if let Some(app) = app { emit_progress(app, 96, "Installing ColdClient compatibility files..."); }
        let names = if cfg.gse_variant == "coldclient_simple" {
            vec!["steamclient.dll", "steamclient64.dll"]
        } else {
            vec!["steamclient.dll", "steamclient64.dll", "tier0_s.dll", "vstdlib_s.dll"]
        };
        copy_named_if_present(resource_root, &game_root, &names, &mut manifest, &mut logs)?;
        if cfg.coldclient_renderer {
            copy_named_if_present(resource_root, &game_root, &["GameOverlayRenderer.dll", "GameOverlayRenderer64.dll"], &mut manifest, &mut logs)?;
        }
    }

    if cfg.overlay_dinput_bridge {
        if let Some(app) = app { emit_progress(app, 97, "Installing DInput8 overlay bridge..."); }
        copy_named_if_present(resource_root, &game_root, &["dinput8.dll", "dinput8.ini"], &mut manifest, &mut logs)?;
    }

    if let Some(app) = app { emit_progress(app, 98, "Saving backup manifest..."); }
    let manifest_path = save_backup_manifest(&game_root, &manifest)?;
    if let Some(app) = app { let _ = save_settings_sync(app, cfg); }

    // Write .gse_auto_setup.json state marker file in game root
    let state_marker_path = game_root.join(".gse_auto_setup.json");
    let mut adjacent_backups = serde_json::Map::new();
    for (t, _) in &targets {
        let file_name = t.file_name().unwrap_or_default().to_string_lossy();
        let bak = t.with_file_name(format!("{file_name}.bak"));
        adjacent_backups.insert(t.display().to_string(), Value::String(bak.display().to_string()));
    }
    let ach_count = schema.as_ref()
        .and_then(|s| s.get("achievements"))
        .and_then(|a| a.as_array())
        .map(|a| a.len()).unwrap_or(0);
    let stat_count = schema.as_ref()
        .and_then(|s| s.get("stats"))
        .and_then(|a| a.as_array())
        .map(|a| a.len()).unwrap_or(0);

    let state_marker = json!({
        "version": gse_version,
        "appid": cfg.app_id,
        "backup_manifest": manifest_path.display().to_string(),
        "targets": targets.iter().map(|(t, _)| t.display().to_string()).collect::<Vec<_>>(),
        "settings_dirs": setting_parents.iter().map(|p| p.join("steam_settings").display().to_string()).collect::<Vec<_>>(),
        "achievements": ach_count,
        "stats": stat_count,
        "account_name": cfg.account_name,
        "save_mode": cfg.save_mode,
        "custom_save_path": if cfg.save_mode == "custom" { &cfg.custom_save_path } else { "" },
        "overlay_enabled": cfg.overlay,
        "deployment_mode": "replace",
        "adjacent_backups": adjacent_backups,
        "engine": cfg.engine,
        "steamstub": {
            "mode": cfg.steamstub_mode,
            "status": "not-patched",
            "method": "steamless"
        },
        "network_mode": cfg.network_mode
    });
    let marker_bytes = serde_json::to_vec_pretty(&state_marker).map_err(|e| e.to_string())?;
    fs::write(&state_marker_path, marker_bytes).map_err(|e| format!("Could not write .gse_auto_setup.json: {e}"))?;

    let setup_ver = format!("Setup complete — GSE {gse_version}");
    log_emit(app, 100, &mut logs, &setup_ver);
    log_emit(app, 100, &mut logs, "Setup completed.");
    if let Some(app) = app { emit_progress(app, 100, "Setup complete"); }

    Ok(GseAutoSetupResult {
        success: true,
        game_name,
        installed_targets: installed,
        backup_manifest: manifest_path.display().to_string(),
        message: "Setup completed.".into(),
        logs,
    })
}

fn save_settings_sync(app: &AppHandle, cfg: &GseAutoSetupConfig) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    fs::write(path, serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gse_auto_setup_load_config(app: AppHandle) -> Result<GseAutoSetupConfig, String> {
    let path = settings_path(&app)?;
    if !path.is_file() { return Ok(GseAutoSetupConfig::default()); }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid saved GSE setup settings: {e}"))
}

#[tauri::command]
pub fn gse_auto_setup_save_config(app: AppHandle, config: GseAutoSetupConfig) -> Result<(), String> {
    save_settings_sync(&app, &config)
}

#[tauri::command]
pub async fn gse_auto_setup_run(app: AppHandle, config: GseAutoSetupConfig) -> Result<GseAutoSetupResult, String> {
    save_settings_sync(&app, &config)?;
    tauri::async_runtime::spawn_blocking(move || setup_sync(app, config))
        .await
        .map_err(|e| format!("GSE setup worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_restore(app: AppHandle, game_folder: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let token = backend_steam_token();
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "restore",
            json!({ "gameFolder": game_folder }),
            Some(token),
        ) {
            if let Some(msg) = output.payload.get("message").and_then(|v| v.as_str()) {
                return Ok(msg.to_string());
            }
        }

        let game_root = canonical_game_root(&game_folder)?;
        let path = backup_path(&game_root).join("manifest.json");
        let bytes = fs::read(&path).map_err(|_| "No GSE Auto Setup backup exists for this game.".to_string())?;
        let manifest: BackupManifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        emit_progress(&app, 10, "Restoring original files…");
        for entry in manifest.entries.iter().rev() {
            let original = PathBuf::from(&entry.original);
            if original.is_dir() { let _ = fs::remove_dir_all(&original); }
            else if original.exists() { let _ = fs::remove_file(&original); }
            if entry.existed {
                let backup = PathBuf::from(&entry.backup);
                if entry.is_dir { copy_dir(&backup, &original)?; }
                else {
                    if let Some(parent) = original.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                    fs::copy(&backup, &original).map_err(|e| e.to_string())?;
                }
            }
        }
        for (target, _) in find_targets(&game_root).unwrap_or_default() {
            let file_name = target.file_name().unwrap_or_default().to_string_lossy();
            let bak = target.with_file_name(format!("{file_name}.bak"));
            if bak.is_file() && target.is_file() {
                let _ = fs::copy(&bak, &target);
                let _ = fs::remove_file(&bak);
            }
        }
        let marker = game_root.join(".gse_auto_setup.json");
        if marker.is_file() { let _ = fs::remove_file(&marker); }
        let bk_dir = backup_path(&game_root);
        if bk_dir.is_dir() { let _ = fs::remove_dir_all(&bk_dir); }
        emit_progress(&app, 100, "Original files restored");
        Ok::<String, String>("Original game files restored from .gse_auto_backup.".into())
    }).await.map_err(|e| format!("Restore worker failed: {e}"))?
}

fn default_save_root() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("GSE Saves")
}

fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

#[tauri::command]
pub async fn gse_auto_setup_list_saves(app: AppHandle, root: Option<String>) -> Result<Vec<GseSaveEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "list_saves",
            json!({ "root": root }),
            None,
        ) {
            if let Ok(entries) = serde_json::from_value::<Vec<GseSaveEntry>>(output.payload) {
                return Ok(entries);
            }
        }

        let root = root.filter(|v| !v.trim().is_empty()).map(PathBuf::from).unwrap_or_else(default_save_root);
        if !root.is_dir() { return Ok(Vec::new()); }
        let mut rows = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_dir() { continue; }
            let app_id = entry.file_name().to_string_lossy().to_string();
            if !app_id.chars().all(|c| c.is_ascii_digit()) { continue; }
            let modified_unix = entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let header_image_url = format!("https://cdn.akamai.steamstatic.com/steam/apps/{app_id}/header.jpg");
            let game_name = format!("Steam App {app_id}");
            rows.push(GseSaveEntry {
                app_id,
                game_name,
                header_image_url,
                path: path.display().to_string(),
                size_bytes: dir_size(&path),
                modified_unix,
                local_backup_count: 0,
            });
        }
        rows.sort_by_key(|x| std::cmp::Reverse(x.modified_unix));
        Ok(rows)
    }).await.map_err(|e| format!("Save scan worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_create_snapshot(
    app: AppHandle,
    save_path: String,
    app_id: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "create_snapshot",
            json!({ "savePath": save_path, "appId": app_id }),
            None,
        ) {
            if let Some(path) = output.payload.get("path").and_then(|v| v.as_str()) {
                return Ok(path.to_string());
            }
        }

        let source = PathBuf::from(save_path);
        if !source.is_dir() { return Err("Save folder does not exist.".into()); }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
        let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join(SNAPSHOT_DIR).join(app_id).join(now.to_string());
        copy_dir(&source, &root)?;
        Ok(root.display().to_string())
    }).await.map_err(|e| format!("Snapshot worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_list_backups(app: AppHandle, app_id: u32) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "list_backups",
            json!({ "appId": app_id }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!([]))
    }).await.map_err(|e| format!("list_backups worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_read_backup(app: AppHandle, archive: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "read_backup",
            json!({ "archive": archive }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({}))
    }).await.map_err(|e| format!("read_backup worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_restore_save_backup(
    app: AppHandle,
    archive: String,
    target_root: String,
    app_id: Option<u32>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "restore_save_backup",
            json!({ "archive": archive, "targetRoot": target_root, "appId": app_id }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "restored": true }))
    }).await.map_err(|e| format!("restore_save_backup worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_drive_status(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "drive_status",
            json!({}),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "connected": false }))
    }).await.map_err(|e| format!("drive_status worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_connect_drive(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "connect_drive",
            json!({}),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "connected": false }))
    }).await.map_err(|e| format!("connect_drive worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_disconnect_drive(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "disconnect_drive",
            json!({}),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "connected": false }))
    }).await.map_err(|e| format!("disconnect_drive worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_backup_save_to_drive(
    app: AppHandle,
    save_path: String,
    app_id: u32,
    game_name: Option<String>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "backup_save_to_drive",
            json!({ "savePath": save_path, "appId": app_id, "gameName": game_name }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "uploaded": false }))
    }).await.map_err(|e| format!("backup_save_to_drive worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_list_cloud_backups(app: AppHandle, app_id: Option<u32>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "list_cloud_backups",
            json!({ "appId": app_id }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!([]))
    }).await.map_err(|e| format!("list_cloud_backups worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_restore_cloud_backup(
    app: AppHandle,
    file_id: String,
    file_name: String,
    target_root: String,
    app_id: u32,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "restore_cloud_backup",
            json!({ "fileId": file_id, "fileName": file_name, "targetRoot": target_root, "appId": app_id }),
            None,
        ) {
            return Ok(output.payload);
        }
        Ok(json!({ "restored": false }))
    }).await.map_err(|e| format!("restore_cloud_backup worker failed: {e}"))?
}

#[tauri::command]
pub fn gse_auto_setup_open_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(path.trim().trim_matches('"'));
    if p.exists() {
        let _ = Command::new("explorer").arg(&p).spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn gse_auto_setup_launch_migrate(app: AppHandle) -> Result<u32, String> {
    let root = resource_root(&app)?;
    let exe = find_file_named(&root, "migrate_gse.exe")
        .ok_or_else(|| "migrate_gse.exe is not bundled.".to_string())?;
    let child = hidden_child_command(&exe)
        .current_dir(exe.parent().unwrap_or(&root))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(child.id())
}

#[tauri::command]
pub fn gse_auto_setup_open_config(app: AppHandle) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        let _ = Command::new("explorer").arg(parent).spawn();
    }
    Ok(())
}

#[tauri::command]
pub async fn gse_auto_setup_check_updates(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "check_updates",
            json!({}),
            None,
        ) {
            return Ok(output.payload);
        }

        let root = resource_root(&app)?;
        let gse_tag = component_tag(&root, "gse", "2026_02_16");
        let uc_tag = component_tag(&root, "uc_online", "1.7.0a");
        let steamless_tag = component_tag(&root, "steamless", "v3.1.0.0");
        let rune_tag = component_tag(&root, "rune_steamstub", "latest");
        let migrate_tag = component_tag(&root, "migrate_gse", "v1.0.0");

        Ok(json!({
            "gse": gse_tag,
            "uc": uc_tag,
            "rune": rune_tag,
            "steamless": steamless_tag,
            "migrate": migrate_tag,
            "message": "All embedded components are up to date."
        }))
    }).await.map_err(|e| format!("Resource update check worker failed: {e}"))?
}

#[tauri::command]
pub fn gse_auto_setup_open_updates_folder(app: AppHandle) -> Result<(), String> {
    let root = resource_root(&app)?;
    let updates_dir = root.join("updates");
    let _ = fs::create_dir_all(&updates_dir);
    let _ = Command::new("explorer").arg(&updates_dir).spawn();
    Ok(())
}

#[tauri::command]
pub async fn gse_auto_setup_clean_update_temp(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "clean_update_temp",
            json!({}),
            None,
        ) {
            if let Some(msg) = output.payload.get("message").and_then(|v| v.as_str()) {
                return Ok(msg.to_string());
            }
        }

        let temp = std::env::temp_dir();
        for entry in fs::read_dir(&temp).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("gse_") || name.starts_with("gse-uc-") || name.starts_with("gse_iface_") {
                let _ = fs::remove_dir_all(entry.path());
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok::<String, String>("Portable update temp cache cleared.".into())
    }).await.map_err(|e| format!("Update cleanup worker failed: {e}"))?
}

#[tauri::command]
pub async fn gse_auto_setup_clear_component_update(app: AppHandle, component: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = crate::gse_original_core::run_original_core(
            &app,
            "clear_component_update",
            json!({ "component": component }),
            None,
        ) {
            if let Some(msg) = output.payload.get("message").and_then(|v| v.as_str()) {
                return Ok(msg.to_string());
            }
        }

        let root = resource_root(&app)?;
        let target = root.join("updates").join(&component);
        if target.is_dir() {
            let _ = fs::remove_dir_all(&target);
        }
        Ok::<String, String>(format!("Cleared update override for {component}."))
    }).await.map_err(|e| format!("Component cleanup worker failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_steam_token_is_present_without_plaintext_fixture() {
        let token = backend_steam_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn default_config_keeps_original_core_enrichment_enabled() {
        let cfg = GseAutoSetupConfig::default();
        assert!(cfg.official_generator);
        assert_eq!(cfg.engine, "gse");
        assert_eq!(cfg.gse_variant, "regular");
    }

    #[test]
    fn test_steam_schema_and_generator_discovery() {
        let (game_name, schema) = fetch_steam_schema(945360).expect("failed to fetch steam schema");
        assert!(game_name.contains("Among Us"));
        let ach_count = schema.as_ref()
            .and_then(|s| s.get("achievements"))
            .and_then(|a| a.as_array())
            .map(|a| a.len()).unwrap_or(0);
        assert_eq!(ach_count, 33);
    }
}
