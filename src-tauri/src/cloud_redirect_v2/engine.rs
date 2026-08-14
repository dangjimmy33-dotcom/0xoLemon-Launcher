use crate::cloud_redirect::steam_detector::{find_steam_path, is_steam_running};
use serde_json::Value;
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

use super::models::{MigrationEvent, ENGINE_VERSION};

static OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn operation_lock() -> &'static Mutex<()> {
    OPERATION_LOCK.get_or_init(|| Mutex::new(()))
}

fn runtime_lock() -> &'static Mutex<()> {
    RUNTIME_LOCK.get_or_init(|| Mutex::new(()))
}

pub fn with_operation_lock<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _guard = operation_lock()
        .lock()
        .map_err(|_| "CloudRedirect operation lock is poisoned".to_string())?;
    operation()
}

fn engine_resource_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(
            resource_dir
                .join("resources")
                .join("cloud_redirect")
                .join("engine")
                .join(ENGINE_VERSION),
        );
        candidates.push(
            resource_dir
                .join("cloud_redirect")
                .join("engine")
                .join(ENGINE_VERSION),
        );
        candidates.push(resource_dir.join("engine").join(ENGINE_VERSION));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(
            current_dir
                .join("src-tauri")
                .join("resources")
                .join("cloud_redirect")
                .join("engine")
                .join(ENGINE_VERSION),
        );
        candidates.push(
            current_dir
                .join("resources")
                .join("cloud_redirect")
                .join("engine")
                .join(ENGINE_VERSION),
        );
    }
    candidates
}

pub fn source_engine_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let required = [
        "0xoCloudRedirect.dll",
        "cloud_redirect_cli.exe",
        "cloud760_tool.exe",
    ];
    for candidate in engine_resource_candidates(app) {
        if required.iter().all(|name| candidate.join(name).is_file()) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "CloudRedirect {} runtime was not built. Run src-tauri/build-cloudredirect.ps1 before packaging.",
        ENGINE_VERSION
    ))
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("Invalid destination: {}", destination.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;

    let temporary = destination.with_extension("0xolemon-new");
    let rollback = destination.with_extension("0xolemon-prev");
    let _ = fs::remove_file(&temporary);
    let _ = fs::remove_file(&rollback);

    fs::copy(source, &temporary).map_err(|error| {
        format!(
            "Cannot copy {} to {}: {error}",
            source.display(),
            temporary.display()
        )
    })?;
    // On Windows File::open() yields a read-only handle. File::sync_all() maps
    // to FlushFileBuffers, which requires write access and otherwise returns
    // ERROR_ACCESS_DENIED (os error 5). Re-open the staged artifact with a
    // write-capable handle before flushing it.
    OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Cannot sync {}: {error}", temporary.display()))?;

    let had_destination = destination.is_file();
    if had_destination {
        fs::rename(destination, &rollback).map_err(|error| {
            format!(
                "Cannot prepare rollback for {}: {error}",
                destination.display()
            )
        })?;
    }

    if let Err(error) = fs::rename(&temporary, destination) {
        if had_destination {
            let _ = fs::rename(&rollback, destination);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Cannot commit {} to {}: {error}",
            temporary.display(),
            destination.display()
        ));
    }

    if had_destination {
        let _ = fs::remove_file(&rollback);
    }
    Ok(())
}

pub fn runtime_engine_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(app_data
        .join("cloud_redirect")
        .join("engine")
        .join(ENGINE_VERSION))
}

pub fn ensure_runtime(app: &AppHandle) -> Result<PathBuf, String> {
    let _guard = runtime_lock()
        .lock()
        .map_err(|_| "CloudRedirect runtime lock is poisoned".to_string())?;
    let source = source_engine_dir(app)?;
    let runtime = runtime_engine_dir(app)?;
    fs::create_dir_all(&runtime)
        .map_err(|error| format!("Cannot create engine directory: {error}"))?;

    for name in [
        "0xoCloudRedirect.dll",
        "cloud_redirect_cli.exe",
        "cloud760_tool.exe",
    ] {
        let source_file = source.join(name);
        let destination = runtime.join(name);
        let must_copy = match (fs::metadata(&source_file), fs::metadata(&destination)) {
            (Ok(source_meta), Ok(dest_meta)) => {
                source_meta.len() != dest_meta.len()
                    || source_meta.modified().ok() != dest_meta.modified().ok()
            }
            (Ok(_), Err(_)) => true,
            (Err(error), _) => {
                return Err(format!(
                    "Missing engine artifact {}: {error}",
                    source_file.display()
                ))
            }
        };
        if must_copy {
            atomic_copy(&source_file, &destination)?;
        }
    }

    if source.join("engine.json").is_file() {
        let _ = atomic_copy(&source.join("engine.json"), &runtime.join("engine.json"));
    }
    Ok(runtime)
}

pub fn install_dll(app: &AppHandle) -> Result<PathBuf, String> {
    with_operation_lock(|| {
        if is_steam_running() {
            return Err(
                "Steam is running. Close Steam before installing CloudRedirect.".to_string(),
            );
        }
        let steam =
            find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
        let runtime = ensure_runtime(app)?;
        let destination = steam.join("0xoCloudRedirect.dll");
        atomic_copy(&runtime.join("0xoCloudRedirect.dll"), &destination)?;
        Ok(destination)
    })
}

pub fn uninstall_dll() -> Result<(), String> {
    with_operation_lock(|| {
        if is_steam_running() {
            return Err("Steam is running. Close Steam before removing CloudRedirect.".to_string());
        }
        let steam =
            find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
        let dll = steam.join("0xoCloudRedirect.dll");
        if dll.exists() {
            fs::remove_file(&dll)
                .map_err(|error| format!("Cannot remove {}: {error}", dll.display()))?;
        }
        Ok(())
    })
}

pub fn run_cli_value(app: &AppHandle, args: &[String]) -> Result<Value, String> {
    let runtime = ensure_runtime(app)?;
    let output = Command::new(runtime.join("cloud_redirect_cli.exe"))
        .args(args)
        .current_dir(&runtime)
        .creation_flags_no_window()
        .output()
        .map_err(|error| format!("Cannot start CloudRedirect CLI: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() {
        return Err(if stderr.is_empty() {
            format!("CloudRedirect CLI exited with {}", output.status)
        } else {
            stderr
        });
    }

    let value: Value = serde_json::from_str(&stdout).map_err(|error| {
        format!("CloudRedirect returned invalid JSON: {error}; output={stdout}")
    })?;
    let success = value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            value
                .get("authenticated")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    if !output.status.success() && !success {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or(if stderr.is_empty() {
                "CloudRedirect operation failed"
            } else {
                &stderr
            });
        return Err(message.to_string());
    }
    Ok(value)
}

pub fn run_migration(
    app: &AppHandle,
    source_provider: &str,
    destination_provider: &str,
) -> Result<MigrationEvent, String> {
    with_operation_lock(|| {
        let runtime = ensure_runtime(app)?;
        let mut child = Command::new(runtime.join("cloud_redirect_cli.exe"))
            .args(["migrate", source_provider, destination_provider])
            .current_dir(&runtime)
            .creation_flags_no_window()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Cannot start CloudRedirect migration: {error}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "CloudRedirect migration stdout is unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "CloudRedirect migration stderr is unavailable".to_string())?;
        let stderr_reader = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        });
        let mut final_event = MigrationEvent::default();
        for line in BufReader::new(stdout).lines() {
            let line = line.map_err(|error| error.to_string())?;
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let event = MigrationEvent {
                event_type: value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("status")
                    .to_string(),
                phase: string_field(&value, "phase"),
                message: string_field(&value, "message"),
                file: string_field(&value, "file"),
                done: u64_field(&value, "done"),
                total: u64_field(&value, "total"),
                bytes: u64_field(&value, "bytes"),
                migrated: u64_field(&value, "migrated"),
                skipped: u64_field(&value, "skipped"),
                failed: u64_field(&value, "failed"),
                total_bytes: u64_field(&value, "total_bytes"),
            };
            let _ = app.emit("cloudredirect://migration-progress", &event);
            final_event = event;
        }

        let status = child.wait().map_err(|error| error.to_string())?;
        let stderr = stderr_reader
            .join()
            .unwrap_or_else(|_| "CloudRedirect migration stderr reader failed".to_string())
            .trim()
            .to_string();
        if !status.success() {
            if final_event.message.is_none() {
                final_event.message = Some(if stderr.is_empty() {
                    "CloudRedirect migration failed".to_string()
                } else {
                    stderr
                });
            }
            return Err(final_event
                .message
                .clone()
                .unwrap_or_else(|| "CloudRedirect migration failed".to_string()));
        }
        Ok(final_event)
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

pub fn hide_console(command: &mut Command) {
    command.creation_flags_no_window();
}

#[cfg(target_os = "windows")]
trait CommandWindowExt {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

#[cfg(target_os = "windows")]
impl CommandWindowExt for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x08000000)
    }
}

#[cfg(not(target_os = "windows"))]
trait CommandWindowExt {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

#[cfg(not(target_os = "windows"))]
impl CommandWindowExt for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        self
    }
}
