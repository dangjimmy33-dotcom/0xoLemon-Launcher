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

#[derive(Debug, Clone, Copy)]
enum DependencyArch {
    X64,
    X86,
}

#[derive(Debug, Clone, Copy)]
struct DependencySpec {
    _id: &'static str,
    display_name: &'static str,
    arch: DependencyArch,
    url: &'static str,
    file_name: &'static str,
}

fn dependency_specs_for_game(game_id: &str) -> Vec<DependencySpec> {
    const VC_REDIST_X64: DependencySpec = DependencySpec {
        _id: "vc-redist-x64",
        display_name: "Microsoft Visual C++ Redistributable x64",
        arch: DependencyArch::X64,
        url: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        file_name: "vc_redist.x64.exe",
    };
    const VC_REDIST_X86: DependencySpec = DependencySpec {
        _id: "vc-redist-x86",
        display_name: "Microsoft Visual C++ Redistributable x86",
        arch: DependencyArch::X86,
        url: "https://aka.ms/vs/17/release/vc_redist.x86.exe",
        file_name: "vc_redist.x86.exe",
    };

    match game_id {
        "among-us" => vec![VC_REDIST_X64, VC_REDIST_X86],
        DEFAULT_GAME_ID => vec![VC_REDIST_X64],
        _ => vec![VC_REDIST_X64],
    }
}

pub(super) fn ensure_game_dependencies(
    app: &AppHandle,
    source: &DepotSource,
) -> Result<Vec<String>, JobError> {
    let mut installed = Vec::new();
    for spec in dependency_specs_for_game(&source.game_id) {
        if dependency_installed(spec) {
            continue;
        }
        let installer = download_dependency_installer(app, spec)?;
        run_elevated(
            &installer,
            &["/install", "/quiet", "/norestart"],
            installer.parent(),
            true,
        )?;
        installed.push(spec.display_name.to_string());
    }
    Ok(installed)
}

fn download_dependency_installer(
    app: &AppHandle,
    spec: DependencySpec,
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

fn dependency_installed(spec: DependencySpec) -> bool {
    #[cfg(target_os = "windows")]
    {
        let paths: &[&str] = match spec.arch {
            DependencyArch::X64 => &[
                r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            ],
            DependencyArch::X86 => &[
                r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            ],
        };
        return paths.iter().any(|path| {
            hidden_command("reg.exe")
                .args(["query", path, "/v", "Installed"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| {
                    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                    text.contains("0x1") || text.split_whitespace().any(|part| part == "1")
                })
                .unwrap_or(false)
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = spec;
        true
    }
}

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
