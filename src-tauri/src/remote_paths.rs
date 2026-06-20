use std::collections::HashSet;
use std::env;

use serde::Deserialize;

/// Central mapping for game ids, local install folder names, Hugging Face folder names,
/// and default launch executables.
///
/// Add every new game here instead of hard-coding Hugging Face paths in `job.rs`
/// or `asset_pack.rs`. Hugging Face file paths are case-sensitive, so the `hf_dir_name`
/// value must match the folder name in the dataset exactly.
#[derive(Debug, Clone, Copy)]
pub struct GamePathMapping {
    pub game_id: &'static str,
    pub install_dir_name: &'static str,
    pub hf_dir_name: &'static str,
    pub launch_executable: &'static str,
}

pub const GAME_PATH_MAPPINGS: &[GamePathMapping] = &[
    GamePathMapping {
        game_id: "007-first-light",
        install_dir_name: "007 First Light",
        hf_dir_name: "007-first-light",
        launch_executable: r"Retail\007FirstLight.exe",
    },
    GamePathMapping {
        game_id: "geometry-dash",
        install_dir_name: "Geometry Dash",
        hf_dir_name: "Geometry-Dash",
        launch_executable: "Geometry Dash.exe",
    },
    GamePathMapping {
        game_id: "stellar-blade",
        install_dir_name: "Stellar Blade",
        hf_dir_name: "stellar-blade",
        launch_executable: "SB.exe",
    },
    GamePathMapping {
        game_id: "among-us",
        install_dir_name: "Among Us",
        hf_dir_name: "among-us",
        launch_executable: "Among Us.exe",
    },
    GamePathMapping {
        game_id: "meccha-chameleon",
        install_dir_name: "Meccha Chameleon",
        hf_dir_name: "meccha-chameleon",
        launch_executable: "PenguinHotel.exe",
    },
    GamePathMapping {
        game_id: "octopath-traveler-0",
        install_dir_name: "OCTOPATH TRAVELER 0",
        hf_dir_name: "octopath-traveler-0",
        launch_executable: "OCTOPATH TRAVELER 0.exe",
    },
    GamePathMapping {
        game_id: "tom-clancy-s-splinter-cell-blacklist",
        install_dir_name: "Tom Clancy's Splinter Cell Blacklist",
        hf_dir_name: "tom-clancy-s-splinter-cell-blacklist",
        launch_executable: "Blacklist_DX11_game.exe",
    },
    GamePathMapping {
        game_id: "microsoft-flight-simulator-2020-40th-anniversary-edition",
        install_dir_name: "Microsoft Flight Simulator 2020 - 40th Anniversary Edition",
        hf_dir_name: "microsoft-flight-simulator-2020-40th-anniversary-edition",
        launch_executable: "FlightSimulator.exe",
    },
    GamePathMapping {
        game_id: "hello-kitty-island-adventure",
        install_dir_name: "Hello Kitty Island Adventure",
        hf_dir_name: "hello-kitty-island-adventure",
        launch_executable: "Hello Kitty.exe",
    },
     GamePathMapping {
        game_id: "ea-sports-fc-26",
        install_dir_name: "EA SPORTS FC™ 26",
        hf_dir_name: "ea-sports-fc-26",
        launch_executable: "FC26.exe",
    },
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HuggingFaceRepoConfig {
    #[serde(default)]
    repositories: Vec<HuggingFaceRepoEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HuggingFaceRepoEntry {
    repo_id: String,
    #[serde(default = "default_repo_type")]
    repo_type: String,
    #[serde(default = "default_revision")]
    revision: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_repo_type() -> String {
    "dataset".to_string()
}

fn default_revision() -> String {
    "main".to_string()
}

fn default_enabled() -> bool {
    true
}

pub fn mapping_for_game_id(game_id: &str) -> Option<&'static GamePathMapping> {
    let clean = game_id.trim();
    GAME_PATH_MAPPINGS
        .iter()
        .find(|entry| entry.game_id.eq_ignore_ascii_case(clean))
}

pub fn install_dir_name_for_game_id(game_id: &str) -> String {
    mapping_for_game_id(game_id)
        .map(|entry| entry.install_dir_name.to_string())
        .unwrap_or_else(|| fallback_title_from_game_id(game_id))
}

pub fn hf_dir_name_for_game_id(game_id: &str) -> String {
    mapping_for_game_id(game_id)
        .map(|entry| entry.hf_dir_name.to_string())
        .unwrap_or_else(|| game_id.trim().to_string())
}

pub fn launch_executable_for_game_id(game_id: &str) -> String {
    mapping_for_game_id(game_id)
        .map(|entry| entry.launch_executable.to_string())
        .unwrap_or_else(|| format!("{}.exe", install_dir_name_for_game_id(game_id)))
}

pub fn fallback_title_from_game_id(game_id: &str) -> String {
    let title = game_id
        .trim()
        .split(|ch| ch == '-' || ch == '_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    if title.trim().is_empty() {
        "Unknown Game".to_string()
    } else {
        title
    }
}

/// Returns all configured Hugging Face repository bases, in priority order.
///
/// Sources, highest priority first:
/// 1. `OXO_DEPOT_REPO_BASES` / `OXO_DEPOT_REPOS` (comma, semicolon or newline separated)
/// 2. Legacy `OXO_DEPOT_REPO_BASE`
/// 3. `src-tauri/huggingface-repos.json`, embedded at build time
///
/// Entries can be either a full `/resolve/<revision>` URL or an `owner/repo` id.
/// Duplicate entries are removed while preserving order.
pub fn depot_repo_base_urls() -> Vec<String> {
    let mut values = Vec::new();

    for key in ["OXO_DEPOT_REPO_BASES", "OXO_DEPOT_REPOS"] {
        if let Ok(raw) = env::var(key) {
            values.extend(split_repo_list(&raw));
        }
    }

    if let Ok(raw) = env::var("OXO_DEPOT_REPO_BASE") {
        values.extend(split_repo_list(&raw));
    }

    if let Ok(config) =
        serde_json::from_str::<HuggingFaceRepoConfig>(include_str!("../huggingface-repos.json"))
    {
        for entry in config
            .repositories
            .into_iter()
            .filter(|entry| entry.enabled)
        {
            if let Some(base) = repo_entry_to_base_url(&entry) {
                values.push(base);
            }
        }
    }

    dedupe_preserving_order(
        values
            .into_iter()
            .filter_map(|value| normalize_repo_base(&value))
            .collect(),
    )
}

/// Returns game-specific depot roots for every configured repository.
/// Example: `<repo resolve base>/<game folder>`.
pub fn depot_base_urls_for_game(game_id: &str) -> Vec<String> {
    let remote_prefix = encode_hf_relative_path(&hf_dir_name_for_game_id(game_id));
    depot_repo_base_urls()
        .into_iter()
        .map(|base| {
            if remote_prefix.is_empty() {
                base
            } else {
                format!("{}/{}", base.trim_end_matches('/'), remote_prefix)
            }
        })
        .collect()
}

fn split_repo_list(raw: &str) -> Vec<String> {
    raw.split(|ch| matches!(ch, ',' | ';' | '\n' | '\r'))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn repo_entry_to_base_url(entry: &HuggingFaceRepoEntry) -> Option<String> {
    let repo_id = entry.repo_id.trim().trim_matches('/');
    if repo_id.is_empty() {
        return None;
    }

    let revision = entry.revision.trim().trim_matches('/');
    let revision = if revision.is_empty() {
        "main"
    } else {
        revision
    };
    let repo_type = entry.repo_type.trim().to_ascii_lowercase();

    let prefix = match repo_type.as_str() {
        "dataset" | "datasets" => "datasets/",
        "model" | "models" => "",
        "space" | "spaces" => "spaces/",
        _ => "datasets/",
    };

    Some(format!(
        "https://huggingface.co/{prefix}{repo_id}/resolve/{}",
        encode_hf_relative_path(revision)
    ))
}

fn normalize_repo_base(value: &str) -> Option<String> {
    let clean = value.trim().trim_matches('"').trim_end_matches('/');
    if clean.is_empty() {
        return None;
    }

    if clean.starts_with("https://") || clean.starts_with("http://") {
        if clean.contains("/resolve/") {
            return Some(clean.to_string());
        }
        if let Some((repo_page, revision)) = clean.split_once("/tree/") {
            let revision = revision.trim_matches('/');
            if !revision.is_empty() {
                return Some(format!("{repo_page}/resolve/{revision}"));
            }
        }
        if clean.contains("huggingface.co/") {
            return Some(format!("{clean}/resolve/main"));
        }
        return Some(clean.to_string());
    }

    let repo_id = clean.trim_matches('/');
    if repo_id.split('/').count() < 2 {
        return None;
    }

    Some(format!(
        "https://huggingface.co/datasets/{repo_id}/resolve/main"
    ))
}

fn dedupe_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub fn encode_hf_relative_path(relative_path: &str) -> String {
    relative_path
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .map(percent_encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_repo_order_prefers_new_repo() {
        let config = serde_json::from_str::<HuggingFaceRepoConfig>(include_str!(
            "../huggingface-repos.json"
        ))
        .expect("embedded repository config should parse");
        assert_eq!(
            config
                .repositories
                .first()
                .map(|repo| repo.repo_id.as_str()),
            Some("Penaldo-CR7/PenaldoCR7")
        );
    }

    #[test]
    fn repo_id_and_repo_page_are_normalized() {
        assert_eq!(
            normalize_repo_base("Penaldo-CR7/PenaldoCR7").as_deref(),
            Some("https://huggingface.co/datasets/Penaldo-CR7/PenaldoCR7/resolve/main")
        );
        assert_eq!(
            normalize_repo_base("https://huggingface.co/datasets/Penaldo-CR7/PenaldoCR7")
                .as_deref(),
            Some("https://huggingface.co/datasets/Penaldo-CR7/PenaldoCR7/resolve/main")
        );
    }
}
