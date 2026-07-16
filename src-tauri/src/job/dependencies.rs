use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use tauri::{AppHandle, Manager};

use crate::launch::{
    expand_placeholders, is_script_path, process_path, process_working_directory, GameLaunchOption,
};

use super::{hidden_command, DepotSource, JobError, DEFAULT_GAME_ID};

// ══════════════════════════════════════════════════════════════
//  Dependency types
// ══════════════════════════════════════════════════════════════

/// How to detect whether a dependency is already installed on the machine.
#[derive(Debug, Clone)]
enum DependencyDetect {
    /// Query one or more HKLM registry paths; value name is checked for "Installed" == 1
    /// or for the presence of a non-empty string (for display-version checks).
    RegistryInstalled {
        /// HKLM registry path(s) to query. Any one matching counts as "installed".
        paths: &'static [&'static str],
        /// Registry value name to look up (e.g. "Installed", "Version").
        value: &'static str,
        /// If true the value must equal "0x1" or "1"; if false any non-empty value counts.
        require_flag: bool,
    },
    /// Check whether a specific file or directory exists on disk.
    PathExists {
        path: &'static str,
    },
}

/// Arguments to pass to the installer executable when running silently.
#[derive(Debug, Clone, Copy)]
enum InstallerArgs {
    VcRedist,   // /install /quiet /norestart
    Dxweb,      // /Q
    DotNetFull, // /q /norestart
    EaApp,      // /install /quiet
}

impl InstallerArgs {
    fn as_slice(self) -> &'static [&'static str] {
        match self {
            InstallerArgs::VcRedist => &["/install", "/quiet", "/norestart"],
            InstallerArgs::Dxweb => &["/Q"],
            InstallerArgs::DotNetFull => &["/q", "/norestart"],
            InstallerArgs::EaApp => &["/install", "/quiet"],
        }
    }
}

#[derive(Debug, Clone)]
struct DependencySpec {
    id: &'static str,
    display_name: &'static str,
    url: &'static str,
    file_name: &'static str,
    detect: DependencyDetect,
    installer_args: InstallerArgs,
}

// ══════════════════════════════════════════════════════════════
//  Well-known dependency definitions
// ══════════════════════════════════════════════════════════════

// ── Visual C++ 2022 ────────────────────────────────────────────
const VC_REDIST_X64: DependencySpec = DependencySpec {
    id: "vc-redist-x64-2022",
    display_name: "Microsoft Visual C++ 2022 Redistributable (x64)",
    url: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
    file_name: "vc_redist_2022.x64.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
        ],
        value: "Installed",
        require_flag: true,
    },
    installer_args: InstallerArgs::VcRedist,
};

const VC_REDIST_X86: DependencySpec = DependencySpec {
    id: "vc-redist-x86-2022",
    display_name: "Microsoft Visual C++ 2022 Redistributable (x86)",
    url: "https://aka.ms/vs/17/release/vc_redist.x86.exe",
    file_name: "vc_redist_2022.x86.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
        ],
        value: "Installed",
        require_flag: true,
    },
    installer_args: InstallerArgs::VcRedist,
};

// ── Visual C++ 2019 ────────────────────────────────────────────
// VC 2019 (14.2x) and VC 2022 (14.3x) share the same runtime key; having 2022
// installed fully satisfies a 2019 requirement — detection is intentionally identical.
const VC_REDIST_X64_2019: DependencySpec = DependencySpec {
    id: "vc-redist-x64-2019",
    display_name: "Microsoft Visual C++ 2019 Redistributable (x64)",
    url: "https://aka.ms/vs/16/release/vc_redist.x64.exe",
    file_name: "vc_redist_2019.x64.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
        ],
        value: "Installed",
        require_flag: true,
    },
    installer_args: InstallerArgs::VcRedist,
};

const VC_REDIST_X86_2019: DependencySpec = DependencySpec {
    id: "vc-redist-x86-2019",
    display_name: "Microsoft Visual C++ 2019 Redistributable (x86)",
    url: "https://aka.ms/vs/16/release/vc_redist.x86.exe",
    file_name: "vc_redist_2019.x86.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
        ],
        value: "Installed",
        require_flag: true,
    },
    installer_args: InstallerArgs::VcRedist,
};

// ── Visual C++ 2010 ────────────────────────────────────────────
// Detected via System32 DLL presence (msvcp100.dll) — the most reliable
// signal across all Windows versions and patch levels.
const VC_REDIST_X64_2010: DependencySpec = DependencySpec {
    id: "vc-redist-x64-2010",
    display_name: "Microsoft Visual C++ 2010 Redistributable (x64)",
    url: "https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x64.exe",
    file_name: "vc_redist_2010.x64.exe",
    detect: DependencyDetect::PathExists {
        path: r"C:\Windows\System32\msvcp100.dll",
    },
    installer_args: InstallerArgs::VcRedist,
};

const VC_REDIST_X86_2010: DependencySpec = DependencySpec {
    id: "vc-redist-x86-2010",
    display_name: "Microsoft Visual C++ 2010 Redistributable (x86)",
    url: "https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x86.exe",
    file_name: "vc_redist_2010.x86.exe",
    detect: DependencyDetect::PathExists {
        path: r"C:\Windows\SysWOW64\msvcp100.dll",
    },
    installer_args: InstallerArgs::VcRedist,
};

// ── DirectX Jun 2010 ───────────────────────────────────────────
const DIRECTX_JUN2010: DependencySpec = DependencySpec {
    id: "directx-jun2010",
    display_name: "DirectX End-User Runtime (June 2010)",
    url: "https://download.microsoft.com/download/1/7/1/1718CCC4-6315-4D8E-9543-8E28A4E18C4C/dxwebsetup.exe",
    file_name: "dxwebsetup.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\DirectX",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\DirectX",
        ],
        value: "Version",
        require_flag: false,
    },
    installer_args: InstallerArgs::Dxweb,
};

// ── .NET Framework 4.7 ─────────────────────────────────────────
const DOTNET_47: DependencySpec = DependencySpec {
    id: "dotnet-47",
    display_name: "Microsoft .NET Framework 4.7",
    url: "https://go.microsoft.com/fwlink/?linkid=843004",
    file_name: "NDP47-KB3186500-x86-x64-AllOS-ENU.exe",
    detect: DependencyDetect::RegistryInstalled {
        paths: &[
            r"HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\NET Framework Setup\NDP\v4\Full",
        ],
        value: "Release",
        require_flag: false,
    },
    installer_args: InstallerArgs::DotNetFull,
};

// ── EA App ─────────────────────────────────────────────────────
const EA_APP: DependencySpec = DependencySpec {
    id: "ea-app",
    display_name: "EA App",
    url: "https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe",
    file_name: "EAappInstaller.exe",
    detect: DependencyDetect::PathExists {
        path: r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop.exe",
    },
    installer_args: InstallerArgs::EaApp,
};

// ══════════════════════════════════════════════════════════════
//  Game → dependency mapping
// ══════════════════════════════════════════════════════════════

fn dependency_specs_for_game(game_id: &str) -> Vec<DependencySpec> {
    // Helper to deduplicate by id so games listing both 2019+2022 don't double-install.
    fn dedup(mut v: Vec<DependencySpec>) -> Vec<DependencySpec> {
        let mut seen = std::collections::HashSet::new();
        v.retain(|s| seen.insert(s.id));
        v
    }

    let specs = match game_id {
        // ── Black Myth: Wukong ─────────────────────────────────
        "black-myth-wukong" | "blackmythwukong" => vec![
            VC_REDIST_X64.clone(),
            DIRECTX_JUN2010.clone(),
            DOTNET_47.clone(),
        ],

        // ── Resident Evil: Requiem ────────────────────────────
        "resident-evil-requiem" => vec![VC_REDIST_X64.clone()],

        // ── Pragmata ─────────────────────────────────────────
        "pragmata" => vec![VC_REDIST_X64.clone()],

        // ── Among Us ─────────────────────────────────────────
        // User corrected: needs 2019, not 2022
        "among-us" => vec![VC_REDIST_X64_2019.clone(), VC_REDIST_X86_2019.clone()],

        // ── Geometry Dash ────────────────────────────────────
        "geometry-dash" => vec![VC_REDIST_X64.clone()],

        // ── 007 First Light ──────────────────────────────────
        "007-first-light" => vec![VC_REDIST_X64.clone()],

        // ── EA Sports FC 26 ──────────────────────────────────
        "ea-sports-fc-26" => vec![VC_REDIST_X64.clone(), EA_APP.clone()],

        // ── Meccha Chameleon ─────────────────────────────────
        "Meccha-Chameleon" => vec![
            VC_REDIST_X64.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Microsoft Flight Simulator 2020 40th Anniversary ─
        "microsoft-flight-simulator-2020-40th-anniversary-edition"
        | "microsoft-flight-simulator-2020-40th-anniversary-edition" => {
            vec![VC_REDIST_X64_2019.clone()]
        }

        // ── Octopath Traveler 0 ───────────────────────────────
        "octopath-traveler-0" => vec![
            VC_REDIST_X64.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Persona 5 Royal ───────────────────────────────────
        "persona-5-royal" => vec![
            DIRECTX_JUN2010.clone(),
            VC_REDIST_X64_2019.clone(),
        ],

        // ── Persona 3 Reload ──────────────────────────────────
        "persona-3-reload" => vec![
            VC_REDIST_X64.clone(),
            VC_REDIST_X64_2019.clone(), // dedup removes if 2022 already present
            DIRECTX_JUN2010.clone(),
        ],

        // ── Stellar Blade ─────────────────────────────────────
        "stellar-blade" => vec![
            DIRECTX_JUN2010.clone(),
            VC_REDIST_X64.clone(),
        ],

        // ── Tom Clancy's Splinter Cell Blacklist ──────────────
        "tom-clancy-s-splinter-cell-blacklist" => vec![
            VC_REDIST_X64_2010.clone(),
            VC_REDIST_X86_2010.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Judgment ─────────────────────────────────────────
        "judgment" => vec![VC_REDIST_X64_2019.clone()],

        // ── Grand Theft Auto: San Andreas (2005) ─────────────
        "grand-theft-auto-san-andreas-2005" => vec![DIRECTX_JUN2010.clone()],

        // ── Heavy Rain ───────────────────────────────────────
        "heavy-rain" => vec![
            VC_REDIST_X64_2019.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Yakuza: Like a Dragon ─────────────────────────────
        "yakuza-like-a-dragon" => vec![
            VC_REDIST_X64_2019.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Total War: Three Kingdoms ─────────────────────────
        "total-war-three-kingdoms" => vec![
            VC_REDIST_X64_2010.clone(),
            VC_REDIST_X64_2019.clone(),
            DIRECTX_JUN2010.clone(),
        ],

        // ── Assassin's Creed IV: Black Flag (Resynced) ───────
        "assassins-creed-black-flag-resynced" => vec![
            VC_REDIST_X64.clone(),
        ],

        // ── Default: every game needs at minimum VC++ 2022 x64
        DEFAULT_GAME_ID | _ => vec![VC_REDIST_X64.clone()],
    };

    dedup(specs)
}

// ══════════════════════════════════════════════════════════════
//  Dependency detection
// ══════════════════════════════════════════════════════════════

fn dependency_installed(spec: &DependencySpec) -> bool {
    #[cfg(target_os = "windows")]
    {
        match &spec.detect {
            DependencyDetect::RegistryInstalled {
                paths,
                value,
                require_flag,
            } => paths.iter().any(|path| {
                let output = hidden_command("reg.exe")
                    .args(["query", path, "/v", value])
                    .output()
                    .ok()
                    .filter(|output| output.status.success());

                let Some(output) = output else {
                    return false;
                };

                let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();

                if *require_flag {
                    text.contains("0x1") || text.split_whitespace().any(|part| part == "1")
                } else {
                    // Any non-empty value returned counts as installed.
                    let lines: Vec<&str> = text.lines().collect();
                    // Look for a line containing the value name; reg /v output format:
                    //     "    ValueName    REG_DWORD    0x…"
                    lines.iter().any(|line| {
                        let lower = line.to_ascii_lowercase();
                        let value_lower = value.to_ascii_lowercase();
                        lower.contains(&value_lower)
                            && (lower.contains("reg_dword")
                                || lower.contains("reg_sz")
                                || lower.contains("reg_expand_sz"))
                    })
                }
            }),
            DependencyDetect::PathExists { path } => Path::new(path).exists(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = spec;
        true
    }
}

// ══════════════════════════════════════════════════════════════
//  Installer download + run
// ══════════════════════════════════════════════════════════════

pub(super) fn ensure_game_dependencies(
    app: &AppHandle,
    source: &DepotSource,
) -> Result<Vec<String>, JobError> {
    let mut installed = Vec::new();
    for spec in dependency_specs_for_game(&source.game_id) {
        if dependency_installed(&spec) {
            continue;
        }
        let installer = download_dependency_installer(app, &spec)?;
        run_elevated(
            &installer,
            spec.installer_args.as_slice(),
            installer.parent(),
            true,
        )?;
        installed.push(spec.display_name.to_string());
    }
    Ok(installed)
}

fn download_dependency_installer(
    app: &AppHandle,
    spec: &DependencySpec,
) -> Result<PathBuf, JobError> {
    let redist_dir = app.path().app_data_dir()?.join("redist");
    fs::create_dir_all(&redist_dir)?;
    let destination = redist_dir.join(spec.file_name);
    if destination
        .metadata()
        .map(|metadata| metadata.len() > 512 * 1024)
        .unwrap_or(false)
    {
        return Ok(destination);
    }

    let temp = destination.with_extension("download");
    let mut response = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(240))
        .build()
        .unwrap_or_else(|_| Client::new())
        .get(spec.url)
        .header(USER_AGENT, "0xoLemon-launcher-redist/0.1")
        .send()?
        .error_for_status()?;
    let mut file = File::create(&temp)?;
    response.copy_to(&mut file)?;
    file.flush()?;
    fs::rename(temp, &destination)?;
    Ok(destination)
}

// ══════════════════════════════════════════════════════════════
//  Desktop shortcuts
// ══════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
fn desktop_directory(app: &AppHandle, fallback: &Path) -> PathBuf {
    let registry = hidden_command("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders",
            "/v",
            "Desktop",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().find_map(|line| {
                ["REG_EXPAND_SZ", "REG_SZ"].iter().find_map(|marker| {
                    line.find(*marker).and_then(|index| {
                        let raw = line[index + marker.len()..].trim();
                        (!raw.is_empty()).then(|| raw.to_string())
                    })
                })
            })
        })
        .map(|path| {
            let mut expanded = path;
            for (key, value) in env::vars() {
                expanded = expanded.replace(&format!("%{key}%"), &value);
            }
            PathBuf::from(expanded)
        });

    registry
        .or_else(|| {
            env::var("OneDrive")
                .ok()
                .map(PathBuf::from)
                .map(|home| home.join("Desktop"))
                .filter(|path| path.exists())
        })
        .or_else(|| {
            env::var("USERPROFILE")
                .ok()
                .map(PathBuf::from)
                .map(|home| home.join("Desktop"))
        })
        .unwrap_or_else(|| {
            app.path()
                .app_data_dir()
                .unwrap_or_else(|_| fallback.to_path_buf())
        })
}

#[cfg(not(target_os = "windows"))]
fn desktop_directory(app: &AppHandle, fallback: &Path) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| fallback.to_path_buf())
}

#[cfg(target_os = "windows")]
const GAME_SHORTCUT_BOOTSTRAP_FILE: &str = "0xoLemon Launcher.exe";

#[cfg(target_os = "windows")]
fn game_shortcut_bootstrap_path(install_root: &Path) -> PathBuf {
    install_root.join(GAME_SHORTCUT_BOOTSTRAP_FILE)
}

#[cfg(target_os = "windows")]
fn same_file_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn files_have_same_contents(left: &Path, right: &Path) -> bool {
    let left_len = left.metadata().map(|metadata| metadata.len()).ok();
    let right_len = right.metadata().map(|metadata| metadata.len()).ok();
    if left_len.is_none() || left_len != right_len {
        return false;
    }

    let (Ok(mut left_file), Ok(mut right_file)) = (File::open(left), File::open(right)) else {
        return false;
    };
    let mut left_buffer = [0_u8; 128 * 1024];
    let mut right_buffer = [0_u8; 128 * 1024];
    loop {
        let Ok(left_read) = left_file.read(&mut left_buffer) else {
            return false;
        };
        let Ok(right_read) = right_file.read(&mut right_buffer) else {
            return false;
        };
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return false;
        }
        if left_read == 0 {
            return true;
        }
    }
}

#[cfg(target_os = "windows")]
fn refresh_game_shortcut_bootstrap(
    launcher_exe: &Path,
    bootstrap_exe: &Path,
) -> Result<(), JobError> {
    if same_file_path(launcher_exe, bootstrap_exe)
        || files_have_same_contents(launcher_exe, bootstrap_exe)
    {
        return Ok(());
    }

    if let Some(parent) = bootstrap_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = bootstrap_exe.with_extension("exe.new");
    let _ = fs::remove_file(&temporary);
    fs::copy(launcher_exe, &temporary)?;
    if bootstrap_exe.exists() {
        if let Err(error) = fs::remove_file(bootstrap_exe) {
            let _ = fs::remove_file(&temporary);
            // A shortcut bootstrap that is currently running is locked on Windows.
            // Keep it for this session; the main launcher will refresh it later.
            if bootstrap_exe.is_file() {
                return Ok(());
            }
            return Err(error.into());
        }
    }
    fs::rename(temporary, bootstrap_exe)?;
    Ok(())
}

pub(super) fn remove_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
) -> Result<Vec<PathBuf>, JobError> {
    let mut removed = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let desktop = desktop_directory(app, install_root);
        let primary = desktop.join(format!("{}.lnk", source.game_dir_name));
        let mut candidates = vec![primary];
        if let Ok(profile) = env::var("USERPROFILE") {
            candidates.push(
                PathBuf::from(profile)
                    .join("Desktop")
                    .join(format!("{}.lnk", source.game_dir_name)),
            );
        }
        if let Ok(one_drive) = env::var("OneDrive") {
            candidates.push(
                PathBuf::from(one_drive)
                    .join("Desktop")
                    .join(format!("{}.lnk", source.game_dir_name)),
            );
        }
        candidates.sort();
        candidates.dedup();
        for path in candidates {
            if path.is_file() {
                fs::remove_file(&path)?;
                removed.push(path);
            }
        }

        let bootstrap = game_shortcut_bootstrap_path(install_root);
        if bootstrap.is_file() {
            fs::remove_file(&bootstrap)?;
            removed.push(bootstrap);
        }

        // Remove the legacy AppData bootstrap created by older launcher builds.
        if let Ok(app_data) = app.path().app_data_dir() {
            let legacy_bootstrap = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
            if legacy_bootstrap.is_file() {
                fs::remove_file(&legacy_bootstrap)?;
                removed.push(legacy_bootstrap);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root);
    }

    Ok(removed)
}

pub(super) fn create_game_shortcut(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    executable: &Path,
    relative_executable: &str,
) -> Result<Option<PathBuf>, JobError> {
    if !executable.exists() {
        return Ok(None);
    }
    #[cfg(target_os = "windows")]
    {
        let desktop = desktop_directory(app, install_root);
        fs::create_dir_all(&desktop)?;
        let shortcut_path = desktop.join(format!("{}.lnk", source.game_dir_name));
        let launcher_exe = std::env::current_exe().unwrap_or_else(|_| executable.to_path_buf());
        // Keep the per-game launcher bootstrap in the actual game directory. Windows
        // Explorer's "Open file location" follows a shortcut's TargetPath, so storing
        // this file in AppData made that action open AppData instead of the game folder.
        let bootstrap_exe = game_shortcut_bootstrap_path(install_root);
        refresh_game_shortcut_bootstrap(&launcher_exe, &bootstrap_exe)?;

        // Clean up the legacy AppData copy after the new in-game bootstrap is ready.
        if let Ok(app_data) = app.path().app_data_dir() {
            let legacy_bootstrap = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
            if !same_file_path(&legacy_bootstrap, &bootstrap_exe) && legacy_bootstrap.is_file() {
                let _ = fs::remove_file(legacy_bootstrap);
            }
        }

        let icon_location = format!("{},0", executable.display());
        let working_dir = install_root;
        let arguments = shortcut_argument_line(&[
            ("--launch-game", &source.game_id),
            ("--install-path", &install_root.display().to_string()),
            ("--launch-executable", relative_executable),
        ]);
        let script = format!(
            "$shell = New-Object -ComObject WScript.Shell; \
             $shortcut = $shell.CreateShortcut({}); \
             $shortcut.TargetPath = {}; \
             $shortcut.Arguments = {}; \
             $shortcut.WorkingDirectory = {}; \
             $shortcut.IconLocation = {}; \
             $shortcut.Description = {}; \
             $shortcut.Save()",
            ps_quote(&shortcut_path.display().to_string()),
            ps_quote(&bootstrap_exe.display().to_string()),
            ps_quote(&arguments),
            ps_quote(&working_dir.display().to_string()),
            ps_quote(&icon_location),
            ps_quote(&format!("Launch {}", source.game_dir_name)),
        );
        let status = hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()?;
        if status.success() {
            Ok(Some(shortcut_path))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root, executable, relative_executable);
        Ok(None)
    }
}

/// Like `create_game_shortcut` but does NOT pin a specific executable.
/// Used for multi-exe games so that clicking the shortcut triggers the picker.
pub(super) fn create_game_shortcut_no_exe(
    app: &AppHandle,
    source: &DepotSource,
    install_root: &Path,
    icon_executable: &Path,
) -> Result<Option<PathBuf>, JobError> {
    if !icon_executable.exists() {
        return Ok(None);
    }
    #[cfg(target_os = "windows")]
    {
        let desktop = desktop_directory(app, install_root);
        fs::create_dir_all(&desktop)?;
        let shortcut_path = desktop.join(format!("{}.lnk", source.game_dir_name));
        let launcher_exe = std::env::current_exe().unwrap_or_else(|_| icon_executable.to_path_buf());
        let bootstrap_exe = game_shortcut_bootstrap_path(install_root);
        refresh_game_shortcut_bootstrap(&launcher_exe, &bootstrap_exe)?;

        if let Ok(app_data) = app.path().app_data_dir() {
            let legacy_bootstrap = app_data.join(format!("0xoLemon-{}.exe", source.game_id));
            if !same_file_path(&legacy_bootstrap, &bootstrap_exe) && legacy_bootstrap.is_file() {
                let _ = fs::remove_file(legacy_bootstrap);
            }
        }

        let icon_location = format!("{},0", icon_executable.display());
        let working_dir = install_root;
        // Only --launch-game, no --launch-executable: launcher will call game_launch_config
        // which detects multiple options and shows the picker.
        let arguments = shortcut_argument_line(&[
            ("--launch-game", &source.game_id),
            ("--install-path", &install_root.display().to_string()),
        ]);
        let script = format!(
            "$shell = New-Object -ComObject WScript.Shell; \
             $shortcut = $shell.CreateShortcut({}); \
             $shortcut.TargetPath = {}; \
             $shortcut.Arguments = {}; \
             $shortcut.WorkingDirectory = {}; \
             $shortcut.IconLocation = {}; \
             $shortcut.Description = {}; \
             $shortcut.Save()",
            ps_quote(&shortcut_path.display().to_string()),
            ps_quote(&bootstrap_exe.display().to_string()),
            ps_quote(&arguments),
            ps_quote(&working_dir.display().to_string()),
            ps_quote(&icon_location),
            ps_quote(&format!("Launch {}", source.game_dir_name)),
        );
        let status = hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()?;
        if status.success() {
            Ok(Some(shortcut_path))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source, install_root, icon_executable);
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn shortcut_argument_line(args: &[(&str, &str)]) -> String {
    args.iter()
        .flat_map(|(flag, value)| [(*flag).to_string(), win_arg_quote(value)])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn win_arg_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '\\' | '/')
        })
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

// ══════════════════════════════════════════════════════════════
//  Process launching
// ══════════════════════════════════════════════════════════════

pub(super) fn launch_option_processes(
    game_id: &str,
    install_root: &Path,
    option: &GameLaunchOption,
) -> Result<LaunchedProcessSet, JobError> {
    let mut launched = Vec::new();
    let mut main_child = None;
    let main_index = option
        .processes
        .iter()
        .position(|process| process.role.eq_ignore_ascii_case("main"))
        .or_else(|| option.processes.len().checked_sub(1));

    for (index, process) in option.processes.iter().enumerate() {
        if process.delay_before_ms > 0 {
            std::thread::sleep(Duration::from_millis(process.delay_before_ms));
        }

        let Some(executable) = process_path(install_root, process, game_id) else {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "unsafe launch process path: {}",
                process.path
            )));
        };
        if !executable.exists() {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "launch process is missing: {}",
                executable.display()
            )));
        }

        let working_dir = process_working_directory(install_root, &executable, process, game_id)
            .ok_or_else(|| {
                JobError::Depot(format!("unsafe working directory for {}", process.path))
            })?;
        if !working_dir.is_dir() {
            if process.optional {
                continue;
            }
            return Err(JobError::Depot(format!(
                "launch working directory is missing: {}",
                working_dir.display()
            )));
        }

        let args = process
            .args
            .iter()
            .map(|arg| expand_placeholders(arg, install_root, game_id))
            .collect::<Vec<_>>();
        let environment = process
            .environment
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    expand_placeholders(value, install_root, game_id),
                )
            })
            .collect::<Vec<_>>();
        let hidden = process
            .hidden
            .unwrap_or_else(|| is_script_path(&executable));

        if process.run_as_admin {
            run_configured_process_elevated(
                &executable,
                &args,
                &working_dir,
                &environment,
                hidden,
                process.wait_for_exit,
            )?;
        } else {
            let child = run_configured_process(
                &executable,
                &args,
                &working_dir,
                &environment,
                hidden,
                process.wait_for_exit,
            )?;
            if main_index == Some(index) {
                main_child = child;
            }
        }

        launched.push(executable);
        if process.delay_after_ms > 0 {
            std::thread::sleep(Duration::from_millis(process.delay_after_ms));
        }
    }

    if launched.is_empty() {
        return Err(JobError::Depot(
            "the selected launch option did not start any process".to_string(),
        ));
    }
    Ok(LaunchedProcessSet {
        paths: launched,
        main_child,
    })
}

pub(super) struct LaunchedProcessSet {
    pub(super) paths: Vec<PathBuf>,
    pub(super) main_child: Option<Child>,
}

fn run_configured_process(
    executable: &Path,
    args: &[String],
    working_dir: &Path,
    environment: &[(String, String)],
    hidden: bool,
    wait: bool,
) -> Result<Option<Child>, JobError> {
    let mut command = if is_script_path(executable) {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/S", "/C"]);
        command.arg(batch_command_line(executable, args));
        command
    } else {
        let mut command = Command::new(executable);
        command.args(args);
        command
    };

    command.current_dir(working_dir);
    for (key, value) in environment {
        validate_environment_key(key)?;
        command.env(key, value);
    }

    if hidden {
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(super::CREATE_NO_WINDOW);
        }
    }

    if wait {
        let status = command.status()?;
        if !status.success() {
            return Err(JobError::Depot(format!(
                "launch process exited with code {}: {}",
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                executable.display()
            )));
        }
        Ok(None)
    } else {
        Ok(Some(command.spawn()?))
    }
}

fn run_configured_process_elevated(
    executable: &Path,
    args: &[String],
    working_dir: &Path,
    environment: &[(String, String)],
    hidden: bool,
    wait: bool,
) -> Result<(), JobError> {
    #[cfg(target_os = "windows")]
    {
        let (file_path, process_args) = if is_script_path(executable) {
            (
                PathBuf::from("cmd.exe"),
                vec![
                    "/D".to_string(),
                    "/S".to_string(),
                    "/C".to_string(),
                    batch_command_line(executable, args),
                ],
            )
        } else {
            (executable.to_path_buf(), args.to_vec())
        };

        let mut script = String::new();
        for (key, value) in environment {
            validate_environment_key(key)?;
            script.push_str(&format!("$env:{} = {}; ", key, ps_quote(value)));
        }
        script.push_str(&format!(
            "$p = Start-Process -FilePath {} -Verb RunAs -WindowStyle {} -WorkingDirectory {}",
            ps_quote_os(file_path.as_os_str()),
            if hidden { "Hidden" } else { "Normal" },
            ps_quote_os(working_dir.as_os_str()),
        ));
        if !process_args.is_empty() {
            let quoted_args = process_args
                .iter()
                .map(|arg| ps_quote(arg))
                .collect::<Vec<_>>()
                .join(", ");
            script.push_str(&format!(" -ArgumentList @({quoted_args})"));
        }
        if wait {
            script.push_str(" -Wait -PassThru; if ($p.ExitCode -ne 0) { exit $p.ExitCode }");
        }

        let status = hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(JobError::Depot(format!(
                "admin launch was canceled or failed: {}",
                executable.display()
            )))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_configured_process(executable, args, working_dir, environment, hidden, wait)
    }
}

fn validate_environment_key(key: &str) -> Result<(), JobError> {
    if !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(())
    } else {
        Err(JobError::Depot(format!(
            "invalid environment variable name in launch config: {key}"
        )))
    }
}

fn batch_command_line(executable: &Path, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 2);
    parts.push("call".to_string());
    parts.push(cmd_quote(&executable.display().to_string()));
    parts.extend(args.iter().map(|arg| cmd_quote(arg)));
    parts.join(" ")
}

fn cmd_quote(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn run_elevated(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
) -> Result<(), JobError> {
    #[cfg(target_os = "windows")]
    {
        run_elevated_windows(executable, args, working_dir, wait)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new(executable);
        command.args(args);
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        if wait {
            command.status()?;
        } else {
            command.spawn()?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn run_elevated_windows(
    executable: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    wait: bool,
) -> Result<(), JobError> {
    let mut script = format!(
        "$p = Start-Process -FilePath {} -Verb RunAs -WindowStyle Normal",
        ps_quote_os(executable.as_os_str())
    );
    if let Some(dir) = working_dir {
        script.push_str(&format!(
            " -WorkingDirectory {}",
            ps_quote_os(dir.as_os_str())
        ));
    }
    if !args.is_empty() {
        let quoted_args = args
            .iter()
            .map(|arg| ps_quote(arg))
            .collect::<Vec<_>>()
            .join(", ");
        script.push_str(&format!(" -ArgumentList @({quoted_args})"));
    }
    if wait {
        script.push_str(" -Wait -PassThru; if ($p.ExitCode -ne 0) { exit $p.ExitCode }");
    }

    let status = hidden_command("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(JobError::Depot(
            "admin launch was canceled or failed".to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
fn ps_quote_os(value: &OsStr) -> String {
    ps_quote(&value.to_string_lossy())
}
