use std::path::{Path, PathBuf};

pub fn downloading_dir_for_install(install_path: &Path) -> PathBuf {
    // Use AppID (short numeric) instead of game folder name to save path length
    // Extract game_id from install path to look up AppID
    let game_folder = install_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    if let Some(parent) = install_path.parent() {
        if parent
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("common"))
            .unwrap_or(false)
        {
            if let Some(store_root) = parent.parent() {
                // Use game folder name as fallback (will be replaced with AppID in job.rs)
                // Format: store_root/dl/{game_folder_or_appid}
                return store_root.join("dl").join(game_folder);
            }
        }
    }

    // Fallback for non-standard install paths
    install_path.join(".0xolemon").join("dl")
}
