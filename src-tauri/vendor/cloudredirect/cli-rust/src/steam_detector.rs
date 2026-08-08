// Steam installation detection and version checks.

use std::path::{Path, PathBuf};

pub const SUPPORTED_STEAM_VERSIONS: [i64; 9] =
    [1782866176, 1782344391, 1782257239, 1781041600, 1780352834, 1779918128, 1779486452, 1778281814, 1778003620];

pub fn is_supported_steam_version(version: i64) -> bool {
    SUPPORTED_STEAM_VERSIONS.contains(&version)
}

/// Locate the Steam install directory via the registry, then common paths.
pub fn find_steam_path() -> Option<PathBuf> {
    try_registry().or_else(try_known_paths)
}

fn dir_exists(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

fn try_registry() -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // HKLM\SOFTWARE\Wow6432Node\Valve\Steam : InstallPath
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\Wow6432Node\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            if dir_exists(&path) {
                return Some(PathBuf::from(path));
            }
        }
    }
    // HKLM\SOFTWARE\Valve\Steam : InstallPath
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            if dir_exists(&path) {
                return Some(PathBuf::from(path));
            }
        }
    }
    // HKCU\SOFTWARE\Valve\Steam : SteamPath
    if let Ok(key) = hkcu.open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("SteamPath") {
            if dir_exists(&path) {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn try_known_paths() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from(r"C:\Games\Steam"),
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
        PathBuf::from(r"D:\Steam"),
        PathBuf::from(r"D:\Games\Steam"),
    ];
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(pf86).join("Steam"));
    }
    candidates
        .into_iter()
        .find(|p| p.is_dir() && p.join("steam.exe").is_file())
}

/// True if any process named "steam.exe" is running. Uses `tasklist` to avoid
/// pulling in a process-enumeration dependency.
pub fn is_steam_running() -> bool {
    use std::process::Command;
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .to_lowercase()
            .contains("steam.exe"),
        Err(_) => false,
    }
}

/// Read the installed Steam client version from the package manifest.
pub fn get_steam_version(steam_path: &Path) -> Option<i64> {
    let manifest = steam_path
        .join("package")
        .join("steam_client_win64.manifest");
    let text = std::fs::read_to_string(manifest).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("\"version\"") {
            continue;
        }
        // value is the last quoted token on the line
        let last = trimmed.rfind('"')?;
        let second_last = trimmed[..last].rfind('"')?;
        let val = &trimmed[second_last + 1..last];
        if let Ok(v) = val.parse::<i64>() {
            return Some(v);
        }
    }
    None
}
