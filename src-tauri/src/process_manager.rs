use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGameInfo {
    pub game_id: String,
    pub pid: u32,
    pub executable: String,
    pub install_path: String,
    pub started_at: String,
    #[serde(skip)]
    pub achievement_watcher: Option<Arc<crate::achievement_watcher::AchievementWatcher>>,
}

#[derive(Default)]
pub struct ProcessManager {
    running: Mutex<HashMap<String, RunningGameInfo>>,
}

impl ProcessManager {
    pub fn list(&self) -> Vec<RunningGameInfo> {
        let mut values = self
            .running
            .lock()
            .map(|running| running.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        values.sort_by(|a, b| a.game_id.cmp(&b.game_id));
        values
    }

    pub fn is_running(&self, game_id: &str) -> bool {
        self.running
            .lock()
            .map(|running| running.contains_key(game_id))
            .unwrap_or(false)
    }

    pub fn spawn_game(
        app: AppHandle,
        manager: Arc<Self>,
        game_id: String,
        install_path: &Path,
        executable: &Path,
    ) -> Result<u32, String> {
        {
            let running = manager
                .running
                .lock()
                .map_err(|_| "process manager lock poisoned".to_string())?;
            if let Some(existing) = running.get(&game_id) {
                return Err(format!(
                    "{} is already running (PID {})",
                    game_id, existing.pid
                ));
            }
        }

        // --- Steam Emulator injection ---
        // Determine if game exe is 64-bit or 32-bit, then copy the matching DLL.
        let game_dir = executable.parent().unwrap_or(install_path);
        let exe_is_64 = is_pe64(executable);
        let (src_dll_name, dst_dll_name) = if exe_is_64 {
            ("steam_api64.dll", "steam_api64.dll")
        } else {
            ("steam_api.dll", "steam_api.dll")
        };
        let arch_dir = if exe_is_64 { "x64" } else { "x86" };
        
        // resource_dir() already IS the "resources" folder, don't double-add it
        let emu_dll = if let Ok(res_dir) = app.path().resource_dir() {
            let candidate = res_dir.join("emu").join(arch_dir).join(src_dll_name);
            eprintln!("[steam_emu] resource_dir candidate: {}", candidate.display());
            candidate
        } else {
            let candidate = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources").join("emu").join(arch_dir).join(src_dll_name);
            eprintln!("[steam_emu] CARGO_MANIFEST_DIR candidate: {}", candidate.display());
            candidate
        };

        if emu_dll.exists() {
            let dest = game_dir.join(dst_dll_name);
            match std::fs::copy(&emu_dll, &dest) {
                Ok(_) => eprintln!("[steam_emu] Copied {} -> {}", emu_dll.display(), dest.display()),
                Err(e) => eprintln!("[steam_emu] Failed to copy DLL: {}", e),
            }
        } else {
            eprintln!("[steam_emu] DLL not found at: {}", emu_dll.display());
        }

        // Start the achievement watcher BEFORE spawning so we have the TCP port
        let watcher = crate::achievement_watcher::start_session(
            app.clone(),
            &game_id,
            None,
            install_path,
            0, // pid not known yet
        );
        let tcp_port = watcher.tcp_port();

        let mut command = Command::new(executable);
        if let Some(parent) = executable.parent() {
            command.current_dir(parent);
        } else {
            command.current_dir(install_path);
        }
        command.stdin(Stdio::null());
        if tcp_port > 0 {
            command.env("ACHIEVEMENT_TCP_PORT", tcp_port.to_string());
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let pid = child.id();
        let started = Instant::now();
        let (started_event, achievement_events) = match platform::begin_game_session(
            &app,
            &game_id,
            pid,
            install_path,
            executable,
        ) {
            Ok(session) => session,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let running_info = RunningGameInfo {
            game_id: game_id.clone(),
            pid,
            executable: executable.display().to_string(),
            install_path: install_path.display().to_string(),
            started_at: started_event.started_at.clone(),
            achievement_watcher: Some(Arc::new(watcher)),
        };
        manager
            .running
            .lock()
            .map_err(|_| "process manager lock poisoned".to_string())?
            .insert(game_id.clone(), running_info);
        let _ = app.emit("launcher://game-started", started_event);
        platform::emit_achievement_events(&app, &achievement_events);

        thread::spawn(move || {
            let status = child.wait();
            let session_seconds = started.elapsed().as_secs();
            let exit_code = status.ok().and_then(|status| status.code());
            if let Ok(mut running) = manager.running.lock() {
                if running.get(&game_id).is_some_and(|entry| entry.pid == pid) {
                    running.remove(&game_id);
                }
            }
            match platform::end_game_session(
                &app,
                &game_id,
                pid,
                session_seconds,
                exit_code,
            ) {
                Ok((event, achievement_events)) => {
                    let _ = app.emit("launcher://game-exited", event);
                    platform::emit_achievement_events(&app, &achievement_events);
                }
                Err(error) => {
                    let _ = app.emit("launcher://runtime-error", error);
                }
            }
        });

        Ok(pid)
    }
    pub fn kill_game(&self, game_id: &str) -> Result<(), String> {
        let pid = {
            let running = self
                .running
                .lock()
                .map_err(|_| "process manager lock poisoned".to_string())?;
            running.get(game_id).map(|info| info.pid)
        };

        if let Some(pid) = pid {
            let mut command = Command::new("taskkill");
            command.args(["/F", "/T", "/PID", &pid.to_string()]);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                command.creation_flags(CREATE_NO_WINDOW);
            }
            let status = command
                .status()
                .map_err(|e| format!("Failed to execute taskkill: {}", e))?;

            if !status.success() {
                return Err(format!("taskkill failed with status: {}", status));
            }
            Ok(())
        } else {
            Err(format!("Game {} is not running", game_id))
        }
    }
}

/// Reads the PE header of an executable to determine if it is 64-bit.
/// Returns true for AMD64/x64, false for x86 or on any read error (safe fallback).
pub fn is_pe64(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return true };
    // Read DOS header magic "MZ"
    let mut dos = [0u8; 64];
    if f.read_exact(&mut dos).is_err() || &dos[0..2] != b"MZ" { return true; }
    // e_lfanew is at offset 0x3C
    let e_lfanew = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap_or([0;4]));
    if f.seek(SeekFrom::Start(e_lfanew as u64)).is_err() { return true; }
    // Read PE signature + machine type (4 + 2 bytes)
    let mut pe = [0u8; 6];
    if f.read_exact(&mut pe).is_err() { return true; }
    if &pe[0..4] != b"PE\0\0" { return true; }
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    // IMAGE_FILE_MACHINE_AMD64 = 0x8664
    machine == 0x8664
}
