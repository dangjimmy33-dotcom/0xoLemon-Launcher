use std::fs;
use std::path::{Path, PathBuf};

use crate::remote_paths;

use super::{
    DepotSource, JobError, DEFAULT_GAME_DIR_NAME, DEFAULT_GAME_ID, DEFAULT_STORE_ROOT,
    INSTALLED_MANIFEST_FILE, INSTALL_MARKER_DIR, INSTALL_MARKER_FILE, LEGACY_INSTALL_MARKER_FILE,
};

/// Returns the path to the shared chunks directory.
/// 
/// Steam-like layout: chunks are stored DIRECTLY at the root of dl/ folder!
/// No subfolders - all .chunk files go directly into dl/
pub(super) fn staged_chunk_dir(downloading_root: &Path) -> PathBuf {
    // Get parent of downloading_root (which is dl/{appid}/) to get dl/ root
    if let Some(dl_root) = downloading_root.parent() {
        return dl_root.to_path_buf();  // Direct at dl/ root - NO chunks/ subfolder!
    }
    // Fallback: keep old behavior if path structure is unexpected
    downloading_root.to_path_buf()
}

pub(super) fn staged_chunk_path_from(staged_chunks_root: &Path, hash: &str) -> PathBuf {
    // Store chunks DIRECTLY in dl/ root with hash as filename
    // Format: dl/{hash}.chunk (no prefix subfolder)
    staged_chunks_root.join(format!("{hash}.chunk"))
}

/// Returns the size of verified transport chunks kept beside the game library.
///
/// Heavy depot data deliberately stays under:
///   <library>\\downloading\\<game>\\chunks
///
/// AppData is reserved for small launcher state (settings, journals, playtime,
/// achievements). Keeping chunks next to the library avoids silently filling
/// the system drive and avoids a second cross-volume copy.
pub(super) fn downloading_chunk_cache_path(install_root: &Path, source: &DepotSource) -> PathBuf {
    staged_chunk_dir(&downloading_dir_for_install(install_root, source))
}

pub(super) fn downloading_chunk_cache_size(
    install_root: &Path,
    source: &DepotSource,
) -> Result<u64, JobError> {
    directory_size(&downloading_chunk_cache_path(install_root, source))
}

pub(super) fn downloading_cache_free_space(install_root: &Path, source: &DepotSource) -> u64 {
    let downloading_root = downloading_dir_for_install(install_root, source);
    let probe = downloading_root
        .ancestors()
        .find(|path| path.exists())
        .unwrap_or(install_root);
    fs2::free_space(probe).unwrap_or(0)
}

fn directory_size(root: &Path) -> Result<u64, JobError> {
    if !root.exists() {
        return Ok(0);
    }

    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("chunk")
            {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

pub(super) fn default_game_id_string() -> String {
    DEFAULT_GAME_ID.to_string()
}

pub(super) fn sanitize_game_id(game_id: &str) -> String {
    let clean = game_id
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if clean.is_empty() {
        DEFAULT_GAME_ID.to_string()
    } else {
        clean
    }
}

pub(super) fn game_dir_name(game_id: &str) -> String {
    remote_paths::install_dir_name_for_game_id(game_id)
}

pub(super) fn default_launch_executable(game_id: &str) -> String {
    remote_paths::launch_executable_for_game_id(game_id)
}

pub(super) fn default_store_root() -> PathBuf {
    PathBuf::from(DEFAULT_STORE_ROOT)
}

pub(super) fn default_common_game_dir() -> PathBuf {
    default_store_root()
        .join("common")
        .join(DEFAULT_GAME_DIR_NAME)
}

pub(super) fn resolve_install_root(install_path: Option<String>, source: &DepotSource) -> PathBuf {
    install_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| source.default_common_game_dir())
}

pub(super) fn downloading_dir_for_install(install_root: &Path, source: &DepotSource) -> PathBuf {
    // Normal 0xoLemon layout:
    //   E:\0xoLemon store\common\Game Name      -> final install
    //   E:\0xoLemon store\dl\{appid}            -> staging/resume cache (short path!)
    //
    // Use Steam AppID (numeric) instead of game slug to save path length:
    //   Old: E:\0xoLemon store\dl\microsoft-flight-simulator-2020-40th-anniversary-edition\
    //   New: E:\0xoLemon store\dl\1250410\
    //   Saves: 51 characters!
    if let Some(common_dir) = install_root.parent() {
        let is_common_dir = common_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("common"))
            .unwrap_or(false);

        if is_common_dir {
            if let Some(store_root) = common_dir.parent() {
                // Prefer AppID (short numeric) over game_id (long slug)
                let folder_name = source
                    .app_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| source.game_id.clone());
                
                return store_root.join("dl").join(folder_name);
            }
        }
    }

    if install_root == source.default_common_game_dir() {
        let mut store_root = PathBuf::from(DEFAULT_STORE_ROOT);
        store_root.push("dl");
        
        // Prefer AppID over game_id
        let folder_name = source
            .app_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| source.game_id.clone());
        
        store_root.push(folder_name);
        store_root
    } else {
        // Shorten fallback directory name
        install_root.join(".0x_dl")
    }
}

pub(super) fn install_marker_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(INSTALL_MARKER_FILE)
}

pub(super) fn legacy_install_marker_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(LEGACY_INSTALL_MARKER_FILE)
}

pub(super) fn installed_manifest_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(INSTALLED_MANIFEST_FILE)
}

pub(super) fn remote_repo_prefix(game_id: &str) -> String {
    remote_paths::hf_dir_name_for_game_id(game_id)
}

pub(super) fn remote_repo_base_urls(game_id: &str) -> Vec<(String, Option<String>)> {
    remote_paths::depot_base_urls_for_game(game_id)
}

pub(super) fn encode_hf_relative_path(relative_path: &str) -> String {
    remote_paths::encode_hf_relative_path(relative_path)
}

pub(super) fn state_backup_dir(backup_root: &Path, install_root: &Path) -> PathBuf {
    let path_str = install_root.to_string_lossy().to_string();
    let safe_name: String = path_str.chars().map(|c| {
        if c.is_alphanumeric() || c == ' ' || c == '-' {
            c
        } else {
            '_'
        }
    }).collect();
    backup_root.join(safe_name)
}

pub(super) fn get_launcher_backup_root() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|parent| parent.join("0xo_Backups")))
}
