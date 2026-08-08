// Steam installation detection and version checks.
use std::path::{Path, PathBuf};

pub const SUPPORTED_STEAM_VERSIONS: [i64; 9] = [
    1782866176, 1782344391, 1782257239, 1781041600, 1780352834,
    1779918128, 1779486452, 1778281814, 1778003620,
];

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

    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\Wow6432Node\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            if dir_exists(&path) { return Some(PathBuf::from(path)); }
        }
    }
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            if dir_exists(&path) { return Some(PathBuf::from(path)); }
        }
    }
    if let Ok(key) = hkcu.open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("SteamPath") {
            if dir_exists(&path) { return Some(PathBuf::from(path)); }
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
    candidates.into_iter().find(|p| p.is_dir() && p.join("steam.exe").is_file())
}

/// Return exact process IDs for running Steam client processes.
///
/// `tasklist` is available on supported Windows versions and avoids another
/// process-enumeration dependency. CSV output is parsed by exact image name so
/// localized "no tasks" messages cannot be mistaken for a running Steam.
pub fn steam_process_ids() -> Vec<u32> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let mut cmd = Command::new("tasklist");
    cmd.creation_flags(0x08000000);
    let output = match cmd
        .args(["/FI", "IMAGENAME eq steam.exe", "/FO", "CSV", "/NH"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let mut ids = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if !line.to_ascii_lowercase().starts_with("\"steam.exe\",") {
            continue;
        }
        let mut columns = line.split(',');
        let image = columns.next().unwrap_or_default().trim_matches('"');
        let pid = columns.next().unwrap_or_default().trim_matches('"');
        if image.eq_ignore_ascii_case("steam.exe") {
            if let Ok(pid) = pid.parse::<u32>() {
                ids.push(pid);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// True only when an actual process named `steam.exe` is present.
pub fn is_steam_running() -> bool {
    !steam_process_ids().is_empty()
}

/// Read the installed Steam client version from the package manifest.
pub fn get_steam_version(steam_path: &Path) -> Option<i64> {
    let manifest = steam_path.join("package").join("steam_client_win64.manifest");
    let text = std::fs::read_to_string(manifest).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("\"version\"") { continue; }
        let last = trimmed.rfind('"')?;
        let second_last = trimmed[..last].rfind('"')?;
        let val = &trimmed[second_last + 1..last];
        if let Ok(v) = val.parse::<i64>() { return Some(v); }
    }
    None
}

/// Shut down Steam gracefully (up to 15 s) then forcefully.
pub fn shutdown_steam(steam_path: &Path) {
    use std::os::windows::process::CommandExt;
    let steam_exe = steam_path.join("steam.exe");
    if steam_exe.is_file() {
        let mut cmd = std::process::Command::new(&steam_exe);
        cmd.creation_flags(0x08000000);
        let _ = cmd.arg("-shutdown").spawn();
    }
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !is_steam_running() { return; }
    }
    let mut kill_cmd = std::process::Command::new("taskkill");
    kill_cmd.creation_flags(0x08000000);
    let _ = kill_cmd
        .args(["/F", "/IM", "steam.exe"])
        .output();
    std::thread::sleep(std::time::Duration::from_millis(1000));
}
