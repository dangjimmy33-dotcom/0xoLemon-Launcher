use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scanner::safe_join;

const DEFAULT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PICKER_MODE: &str = "auto";
const DEFAULT_PROCESS_ROLE: &str = "main";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub game_id: String,
    #[serde(default = "default_picker_mode")]
    pub picker_mode: String,
    #[serde(default)]
    pub default_option_id: String,
    #[serde(default)]
    pub options: Vec<GameLaunchOption>,
}

impl Default for GameLaunchConfig {
    fn default() -> Self {
        Self {
            schema_version: DEFAULT_SCHEMA_VERSION,
            game_id: String::new(),
            picker_mode: DEFAULT_PICKER_MODE.to_string(),
            default_option_id: String::new(),
            options: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchOption {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub processes: Vec<GameLaunchProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchProcess {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub run_as_admin: bool,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub wait_for_exit: bool,
    #[serde(default)]
    pub delay_before_ms: u64,
    #[serde(default)]
    pub delay_after_ms: u64,
    #[serde(default)]
    pub optional: bool,
    #[serde(default = "default_process_role")]
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedGameLaunchConfig {
    pub schema_version: u32,
    pub game_id: String,
    pub picker_mode: String,
    pub default_option_id: String,
    pub source: String,
    pub options: Vec<ResolvedGameLaunchOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedGameLaunchOption {
    pub id: String,
    pub title: String,
    pub description: String,
    pub recommended: bool,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

fn default_schema_version() -> u32 {
    DEFAULT_SCHEMA_VERSION
}

fn default_picker_mode() -> String {
    DEFAULT_PICKER_MODE.to_string()
}

fn default_process_role() -> String {
    DEFAULT_PROCESS_ROLE.to_string()
}

pub fn fallback_launch_config(game_id: &str, relative_executable: &str) -> GameLaunchConfig {
    let process = GameLaunchProcess {
        path: relative_executable.to_string(),
        args: Vec::new(),
        working_directory: String::new(),
        environment: HashMap::new(),
        // Always run as admin.
        run_as_admin: true,
        hidden: None,
        wait_for_exit: false,
        delay_before_ms: 0,
        delay_after_ms: 0,
        optional: false,
        role: DEFAULT_PROCESS_ROLE.to_string(),
    };
    GameLaunchConfig {
        schema_version: DEFAULT_SCHEMA_VERSION,
        game_id: game_id.to_string(),
        picker_mode: "never".to_string(),
        default_option_id: "default".to_string(),
        options: vec![GameLaunchOption {
            id: "default".to_string(),
            title: "Play".to_string(),
            description: String::new(),
            recommended: true,
            processes: vec![process],
        }],
    }
}

pub fn load_source_launch_config(
    source: &Path,
    game_id: &str,
    fallback_executable: &str,
) -> Result<GameLaunchConfig, String> {
    let candidates = [
        source.join("launch.json"),
        source.join("details").join("launch.json"),
    ];
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let config = serde_json::from_slice::<GameLaunchConfig>(&bytes)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        return normalize_launch_config(config, game_id, fallback_executable);
    }
    Ok(fallback_launch_config(game_id, fallback_executable))
}

pub fn load_install_override(
    install_root: &Path,
) -> Result<Option<(GameLaunchConfig, String)>, String> {
    let candidates = [
        install_root.join("0xo-launch.json"),
        install_root.join(".0xolemon").join("launch.json"),
    ];
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let config = serde_json::from_slice::<GameLaunchConfig>(&bytes)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        return Ok(Some((config, path.display().to_string())));
    }
    Ok(None)
}

pub fn normalize_launch_config(
    mut config: GameLaunchConfig,
    game_id: &str,
    fallback_executable: &str,
) -> Result<GameLaunchConfig, String> {
    if config.game_id.trim().is_empty() {
        config.game_id = game_id.to_string();
    } else if !config.game_id.eq_ignore_ascii_case(game_id) {
        return Err(format!(
            "launch config gameId '{}' does not match '{}'",
            config.game_id, game_id
        ));
    }

    config.schema_version = config.schema_version.max(1);
    config.picker_mode = match config.picker_mode.trim().to_ascii_lowercase().as_str() {
        "always" => "always".to_string(),
        "never" => "never".to_string(),
        _ => "auto".to_string(),
    };

    let mut seen = HashSet::new();
    for option in &mut config.options {
        option.id = option.id.trim().to_string();
        option.title = option.title.trim().to_string();
        if option.id.is_empty() {
            return Err("launch option id cannot be empty".to_string());
        }
        if option.title.is_empty() {
            option.title = option.id.clone();
        }
        if !seen.insert(option.id.to_ascii_lowercase()) {
            return Err(format!("duplicate launch option id: {}", option.id));
        }
        for process in &mut option.processes {
            process.path = clean_relative_input(&process.path);
            process.working_directory = clean_relative_input(&process.working_directory);
            process.role = match process.role.trim().to_ascii_lowercase().as_str() {
                "helper" | "server" | "auxiliary" => "helper".to_string(),
                _ => "main".to_string(),
            };
            // Always run every game process as admin.
            process.run_as_admin = true;
        }
    }

    if config.options.is_empty() {
        return Ok(fallback_launch_config(game_id, fallback_executable));
    }

    if config.default_option_id.trim().is_empty()
        || !config
            .options
            .iter()
            .any(|option| option.id.eq_ignore_ascii_case(&config.default_option_id))
    {
        config.default_option_id = config
            .options
            .iter()
            .find(|option| option.recommended)
            .or_else(|| config.options.first())
            .map(|option| option.id.clone())
            .unwrap_or_else(|| "default".to_string());
    }

    Ok(config)
}

pub fn resolve_launch_config(
    config: &GameLaunchConfig,
    install_root: &Path,
    source: String,
) -> ResolvedGameLaunchConfig {
    let options = config
        .options
        .iter()
        .map(|option| {
            let unavailable_reason =
                option_unavailable_reason(option, install_root, &config.game_id);
            ResolvedGameLaunchOption {
                id: option.id.clone(),
                title: option.title.clone(),
                description: option.description.clone(),
                recommended: option.recommended,
                available: unavailable_reason.is_none(),
                unavailable_reason,
            }
        })
        .collect();

    ResolvedGameLaunchConfig {
        schema_version: config.schema_version,
        game_id: config.game_id.clone(),
        picker_mode: config.picker_mode.clone(),
        default_option_id: config.default_option_id.clone(),
        source,
        options,
    }
}

pub fn option_unavailable_reason(
    option: &GameLaunchOption,
    install_root: &Path,
    game_id: &str,
) -> Option<String> {
    if option.processes.is_empty() {
        return Some("No processes are configured for this option".to_string());
    }

    for process in &option.processes {
        if process.path.trim().is_empty() {
            if process.optional {
                continue;
            }
            return Some("A required process path is empty".to_string());
        }
        let expanded = expand_placeholders(&process.path, install_root, game_id);
        let Some(path) = safe_join(install_root, &clean_relative_input(&expanded)) else {
            if process.optional {
                continue;
            }
            return Some(format!("Unsafe process path: {}", process.path));
        };
        if !path.exists() && !process.optional {
            return Some(format!("Missing file: {}", process.path));
        }
        if !process.working_directory.trim().is_empty() {
            let expanded_dir =
                expand_placeholders(&process.working_directory, install_root, game_id);
            let dir = if expanded_dir.trim() == "." {
                Some(install_root.to_path_buf())
            } else {
                safe_join(install_root, &clean_relative_input(&expanded_dir))
            };
            let Some(dir) = dir else {
                if process.optional {
                    continue;
                }
                return Some(format!(
                    "Unsafe working directory: {}",
                    process.working_directory
                ));
            };
            if !dir.is_dir() && !process.optional {
                return Some(format!(
                    "Missing working directory: {}",
                    process.working_directory
                ));
            }
        }
    }
    None
}

pub fn select_launch_option<'a>(
    config: &'a GameLaunchConfig,
    requested_option_id: Option<&str>,
    requested_executable: Option<&str>,
) -> Option<&'a GameLaunchOption> {
    if let Some(id) = requested_option_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(option) = config
            .options
            .iter()
            .find(|option| option.id.eq_ignore_ascii_case(id))
        {
            return Some(option);
        }
    }

    if let Some(option) = config
        .options
        .iter()
        .find(|option| option.id.eq_ignore_ascii_case(&config.default_option_id))
    {
        return Some(option);
    }

    if let Some(option) = config.options.iter().find(|option| option.recommended) {
        return Some(option);
    }

    // Backward compatibility for old shortcuts/install markers that only store an executable.
    if let Some(executable) = requested_executable
        .map(normalize_relative_path)
        .filter(|value| !value.is_empty())
    {
        if let Some(option) = config.options.iter().find(|option| {
            main_process(option)
                .map(|process| normalize_relative_path(&process.path) == executable)
                .unwrap_or(false)
        }) {
            return Some(option);
        }
    }

    config.options.first()
}

pub fn main_process(option: &GameLaunchOption) -> Option<&GameLaunchProcess> {
    option
        .processes
        .iter()
        .find(|process| process.role.eq_ignore_ascii_case("main"))
        .or_else(|| option.processes.last())
}

pub fn expand_placeholders(value: &str, install_root: &Path, game_id: &str) -> String {
    value
        .replace("{installDir}", &install_root.display().to_string())
        .replace("${INSTALL_DIR}", &install_root.display().to_string())
        .replace("{gameId}", game_id)
        .replace("${GAME_ID}", game_id)
}

pub fn process_path(
    install_root: &Path,
    process: &GameLaunchProcess,
    game_id: &str,
) -> Option<PathBuf> {
    let expanded = expand_placeholders(&process.path, install_root, game_id);
    safe_join(install_root, &clean_relative_input(&expanded))
}

pub fn process_working_directory(
    install_root: &Path,
    process_path: &Path,
    process: &GameLaunchProcess,
    game_id: &str,
) -> Option<PathBuf> {
    if process.working_directory.trim().is_empty() {
        return process_path.parent().map(Path::to_path_buf);
    }
    let expanded = expand_placeholders(&process.working_directory, install_root, game_id);
    if expanded.trim() == "." {
        return Some(install_root.to_path_buf());
    }
    safe_join(install_root, &clean_relative_input(&expanded))
}

pub fn is_script_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("bat") | Some("cmd")
    )
}

fn clean_relative_input(value: &str) -> String {
    let mut value = value.trim().replace('/', "\\");
    while value.starts_with(".\\") {
        value = value[2..].to_string();
    }
    value
}

fn normalize_relative_path(value: &str) -> String {
    clean_relative_input(value).to_ascii_lowercase()
}
