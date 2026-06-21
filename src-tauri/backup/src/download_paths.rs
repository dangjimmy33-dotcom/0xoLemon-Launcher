use std::path::{Path, PathBuf};

pub fn downloading_dir_for_install(install_path: &Path) -> PathBuf {
    let game_folder = install_path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| "unknown-game".into());

    if let Some(parent) = install_path.parent() {
        if parent
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("common"))
            .unwrap_or(false)
        {
            if let Some(store_root) = parent.parent() {
                return store_root.join("downloading").join(game_folder);
            }
        }
    }

    install_path.join(".0xolemon").join("downloading")
}
