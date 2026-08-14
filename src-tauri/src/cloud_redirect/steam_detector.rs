// Steam installation detection and version checks.
use std::path::{Path, PathBuf};

pub const SUPPORTED_STEAM_VERSIONS: [i64; 9] = [
    1782866176, 1782344391, 1782257239, 1781041600, 1780352834, 1779918128, 1779486452, 1778281814,
    1778003620,
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
            if dir_exists(&path) {
                return Some(PathBuf::from(path));
            }
        }
    }
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            if dir_exists(&path) {
                return Some(PathBuf::from(path));
            }
        }
    }
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

/// Return exact process IDs for running Steam client processes.
///
/// Process enumeration uses Toolhelp directly. Spawning `tasklist.exe` can
/// block indefinitely when Windows process-management services are unhealthy,
/// which used to freeze synchronous launcher commands such as disabling the
/// Steam hooks.
#[cfg(target_os = "windows")]
pub fn try_steam_process_ids() -> Result<Vec<u32>, String> {
    use std::mem::{size_of, zeroed};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(format!(
                "Windows process snapshot failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut has_entry = Process32FirstW(snapshot, &mut entry);
        if has_entry == 0 {
            let error = std::io::Error::last_os_error();
            CloseHandle(snapshot);
            return Err(format!("Windows process enumeration failed: {error}"));
        }
        let mut ids = Vec::new();

        while has_entry != 0 {
            if wide_process_name_eq(&entry.szExeFile, "steam.exe") {
                ids.push(entry.th32ProcessID);
            }
            has_entry = Process32NextW(snapshot, &mut entry);
        }

        CloseHandle(snapshot);
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn try_steam_process_ids() -> Result<Vec<u32>, String> {
    Ok(Vec::new())
}

pub fn steam_process_ids() -> Vec<u32> {
    try_steam_process_ids().unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn wide_process_name_eq(buffer: &[u16], expected: &str) -> bool {
    let end = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end]).eq_ignore_ascii_case(expected)
}

/// True only when an actual process named `steam.exe` is present.
pub fn is_steam_running() -> bool {
    !steam_process_ids().is_empty()
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::{try_steam_process_ids, wide_process_name_eq};
    use std::time::{Duration, Instant};

    #[test]
    fn process_name_match_is_case_insensitive_and_null_terminated() {
        let mut name = "STEAM.EXE".encode_utf16().collect::<Vec<_>>();
        name.extend([0, 'x' as u16]);

        assert!(wide_process_name_eq(&name, "steam.exe"));
        assert!(!wide_process_name_eq(&name, "steamwebhelper.exe"));
    }

    #[test]
    fn native_process_snapshot_returns_without_external_process_timeout() {
        let started = Instant::now();
        let _ = try_steam_process_ids().expect("native process enumeration");

        assert!(
            started.elapsed() < Duration::from_secs(5),
            "native process enumeration unexpectedly blocked"
        );
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
        let last = trimmed.rfind('"')?;
        let second_last = trimmed[..last].rfind('"')?;
        let val = &trimmed[second_last + 1..last];
        if let Ok(v) = val.parse::<i64>() {
            return Some(v);
        }
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
        if !is_steam_running() {
            return;
        }
    }
    let mut kill_cmd = std::process::Command::new("taskkill");
    kill_cmd.creation_flags(0x08000000);
    let _ = kill_cmd.args(["/F", "/IM", "steam.exe"]).output();
    std::thread::sleep(std::time::Duration::from_millis(1000));
}
