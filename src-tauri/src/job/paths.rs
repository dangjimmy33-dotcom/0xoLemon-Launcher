use std::fs;
use std::path::{Path, PathBuf};

use crate::remote_paths;

use super::{
    DepotSource, JobError, DEFAULT_GAME_DIR_NAME, DEFAULT_GAME_ID, DEFAULT_STORE_ROOT,
    INSTALLED_MANIFEST_FILE, INSTALL_MARKER_DIR, INSTALL_MARKER_FILE, LEGACY_INSTALL_MARKER_FILE,
};

pub(super) fn staged_chunk_dir(downloading_root: &Path) -> PathBuf {
    downloading_root.join("chunks")
}

pub(super) fn staged_chunk_path_from(staged_chunks_root: &Path, hash: &str) -> PathBuf {
    let prefix = hash.get(0..2).unwrap_or("xx");
    staged_chunks_root
        .join(prefix)
        .join(format!("{hash}.chunk"))
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
    //   E:\0xoLemon store\downloading\Game Name -> staging/resume cache
    //
    // Older code used install_root/.0xolemon/downloading for non-default games,
    // which created paths like common/Geometry Dash/.0xolemon/downloading.
    let game_folder = install_root
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| source.game_dir_name.clone().into());

    if let Some(common_dir) = install_root.parent() {
        let is_common_dir = common_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("common"))
            .unwrap_or(false);

        if is_common_dir {
            if let Some(store_root) = common_dir.parent() {
                return store_root.join("downloading").join(game_folder);
            }
        }
    }

    if install_root == source.default_common_game_dir() {
        source.default_downloading_game_dir()
    } else {
        install_root.join(INSTALL_MARKER_DIR).join("downloading")
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

pub(super) fn remote_repo_base_urls(game_id: &str) -> Vec<String> {
    remote_paths::depot_base_urls_for_game(game_id)
}

pub(super) fn encode_hf_relative_path(relative_path: &str) -> String {
    remote_paths::encode_hf_relative_path(relative_path)
}
