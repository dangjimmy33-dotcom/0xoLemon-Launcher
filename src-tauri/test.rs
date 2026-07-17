use std::path::PathBuf;
use std::process::Command;

fn find_steam_root() -> Option<PathBuf> {
    let registry = [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKCU\Software\Valve\Steam", "SteamExe"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ];
    for (key, value) in registry {
        if let Some(path) = registry_string(key, value) {
            let mut candidate = PathBuf::from(path.replace('/', "\\"));
            if candidate
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("steam.exe"))
                .unwrap_or(false)
            {
                candidate.pop();
            }
            if candidate.join("steam.exe").is_file() {
                return Some(candidate);
            }
        }
    }

    let mut fallbacks = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
    ];
    if let Ok(program_files) = std::env::var("ProgramFiles(x86)") {
        fallbacks.push(PathBuf::from(program_files).join("Steam"));
    }
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        fallbacks.push(PathBuf::from(program_files).join("Steam"));
    }
    return fallbacks
        .into_iter()
        .find(|candidate| candidate.join("steam.exe").is_file());
}

fn registry_string(key: &str, value: &str) -> Option<String> {
    let mut command = Command::new("reg.exe");
    command.creation_flags(0x08000000);
    let output = command.args(["query", key, "/v", value]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let mut tokens = line.split_whitespace();
        if tokens.next().map(|name| name.eq_ignore_ascii_case(value)) == Some(true) {
            tokens.next();
            Some(tokens.collect::<Vec<_>>().join(" "))
        } else {
            None
        }
    })
}

fn main() {
    println!("root: {:?}", find_steam_root());
}
