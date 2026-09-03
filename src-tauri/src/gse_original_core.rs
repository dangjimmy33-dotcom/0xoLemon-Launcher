use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug)]
pub struct OriginalCoreOutput {
    pub payload: Value,
    pub logs: Vec<String>,
}

fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn original_core_resource_root(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(root) = app.path().resource_dir() {
        candidates.push(root.join("resources").join("gse-uc"));
        candidates.push(root.join("gse-uc"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/gse-uc"));
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .ok_or_else(|| "GSE / UC resource directory is missing (expected src-tauri/resources/gse-uc).".to_string())
}

pub fn original_core_state_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve launcher data directory: {error}"))?
        .join("gse-auto-setup")
        .join("python-core");
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create GSE core state directory: {error}"))?;
    Ok(root)
}

fn sidecar_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = original_core_resource_root(app)?;
    let preferred = root.join("bin").join("gse-core.exe");
    if preferred.is_file() {
        return Ok(preferred);
    }
    let fallback = root.join("bin").join("gse-core");
    if fallback.is_file() {
        return Ok(fallback);
    }
    Err(format!(
        "Original GSE Python core sidecar is missing: {}. Run src-tauri/build-gse-core.ps1 before building the launcher.",
        preferred.display()
    ))
}

fn emit_progress(app: &AppHandle, percent: u64, message: &str) {
    let _ = app.emit(
        "gse-auto-setup://progress",
        json!({
            "percent": percent.min(100),
            "message": message,
        }),
    );
}

/// Execute the exact bundled GSE_UC_Setup Python core as a headless child.
///
/// Request data is sent over stdin. The Steam Web API credential is injected
/// separately through the child environment and is never serialized into IPC.
pub fn run_original_core(
    app: &AppHandle,
    command: &str,
    payload: Value,
    steam_web_api_key: Option<String>,
) -> Result<OriginalCoreOutput, String> {
    let exe = sidecar_path(app)?;
    let resource_root = original_core_resource_root(app)?;
    let state_root = original_core_state_root(app)?;

    let request = json!({
        "command": command,
        "payload": payload,
        "resourceRoot": resource_root,
        "stateRoot": state_root,
    });
    let request_bytes = serde_json::to_vec(&request)
        .map_err(|error| format!("Could not serialize GSE core request: {error}"))?;

    let mut cmd = hidden_command(&exe);
    cmd.current_dir(exe.parent().unwrap_or(&resource_root))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(key) = steam_web_api_key {
        cmd.env("GSE_STEAM_WEB_API_KEY", key);
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| format!("Could not start original GSE core: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&request_bytes)
            .and_then(|_| stdin.flush())
            .map_err(|error| format!("Could not send request to original GSE core: {error}"))?;
        // Dropping stdin is intentional: the sidecar reads one JSON document to EOF.
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Original GSE core stdout was not available.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Original GSE core stderr was not available.".to_string())?;

    // Drain stderr concurrently so a noisy dependency can never fill the pipe
    // and deadlock the generator/setup process.
    let stderr_thread = thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut raw = Vec::new();
        let mut lines = Vec::new();
        loop {
            raw.clear();
            match reader.read_until(b'\n', &mut raw) {
                Ok(0) => break,
                Ok(_) => {
                    while matches!(raw.last(), Some(b'\n' | b'\r')) {
                        raw.pop();
                    }
                    let line = String::from_utf8_lossy(&raw).into_owned();
                    if !line.trim().is_empty() {
                        lines.push(line);
                    }
                }
                Err(_) => break,
            }
        }
        lines
    });

    let mut logs = Vec::new();
    let mut result_payload: Option<Value> = None;
    let mut protocol_error: Option<String> = None;
    let mut last_percent = 0_u64;

    let mut stdout_reader = BufReader::new(stdout);
    let mut raw_line = Vec::new();
    loop {
        raw_line.clear();
        let read = stdout_reader
            .read_until(b'\n', &mut raw_line)
            .map_err(|error| format!("Could not read original GSE core output: {error}"))?;
        if read == 0 {
            break;
        }
        while matches!(raw_line.last(), Some(b'\n' | b'\r')) {
            raw_line.pop();
        }
        let line = String::from_utf8_lossy(&raw_line).into_owned();
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(event) => match event.get("type").and_then(Value::as_str).unwrap_or("") {
                "progress" => {
                    let percent = event.get("percent").and_then(Value::as_u64).unwrap_or(last_percent);
                    last_percent = percent.min(100);
                    let message = event.get("message").and_then(Value::as_str).unwrap_or("");
                    if !message.is_empty() {
                        logs.push(message.to_string());
                    }
                    emit_progress(app, percent, message);
                }
                "log" => {
                    if let Some(message) = event.get("message").and_then(Value::as_str) {
                        if !message.trim().is_empty() {
                            logs.push(message.to_string());
                            emit_progress(app, last_percent, message);
                        }
                    }
                }
                "diagnostic" => {
                    if let Some(message) = event.get("message").and_then(Value::as_str) {
                        logs.push(message.to_string());
                    }
                }
                "error" => {
                    protocol_error = Some(
                        event
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Original GSE core failed.")
                            .to_string(),
                    );
                }
                "result" => {
                    result_payload = event.get("payload").cloned();
                }
                _ => logs.push(line),
            },
            Err(_) => logs.push(line),
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("Could not wait for original GSE core: {error}"))?;
    let stderr_lines = stderr_thread.join().unwrap_or_default();
    logs.extend(stderr_lines.iter().cloned());

    if let Some(error) = protocol_error {
        return Err(error);
    }
    if !status.success() {
        let detail = stderr_lines
            .last()
            .cloned()
            .unwrap_or_else(|| format!("exit code {}", status.code().unwrap_or(-1)));
        return Err(format!("Original GSE core failed: {detail}"));
    }
    let payload = result_payload.ok_or_else(|| {
        "Original GSE core exited without returning a result payload.".to_string()
    })?;

    Ok(OriginalCoreOutput { payload, logs })
}
