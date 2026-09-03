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

        // Protect and reconcile the local save before the game starts. Transient
        // Drive failures are handled offline-first by the Cloud Save engine; only
        // a real two-sided conflict or a reliably known newer remote save blocks launch.
        if let Err(error) = crate::cloud_save::sync_before_launch(&app, &game_id) {
            if error.starts_with("CLOUD_SAVE_CONFLICT:")
                || error.starts_with("CLOUD_SAVE_REMOTE_NEWER:")
            {
                return Err(error);
            }
            let _ = app.emit(
                "launcher://cloud-save-error",
                serde_json::json!({ "gameId": &game_id, "message": error }),
            );
        }

        // The authenticated named-pipe server must exist before the game loads
        // steam_api. The PID is bound immediately after spawn and verified by
        // the server against the kernel-reported pipe client process.
        let watcher = if crate::managed_game_runtime::supports_managed_achievements(&game_id) {
            Some(crate::achievement_watcher::start_session(
                app.clone(),
                &game_id,
                None,
                install_path,
                0,
            )?)
        } else {
            None
        };

        let mut command = Command::new(executable);
        if let Some(parent) = executable.parent() {
            command.current_dir(parent);
        } else {
            command.current_dir(install_path);
        }
        command.stdin(Stdio::null());
        if let Some(watcher) = watcher.as_ref() {
            for (key, value) in watcher.environment() {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let pid = child.id();
        if let Some(watcher) = watcher.as_ref() {
            if let Err(error) = watcher.bind_pid(pid) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let started = Instant::now();
        let (started_event, achievement_events) =
            match platform::begin_game_session(&app, &game_id, pid, install_path, executable) {
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
            achievement_watcher: watcher.map(Arc::new),
        };
        manager
            .running
            .lock()
            .map_err(|_| "process manager lock poisoned".to_string())?
            .insert(game_id.clone(), running_info);
        crate::cloud_save::mark_game_running(&game_id, true);
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
            crate::cloud_save::mark_game_running(&game_id, false);
            match platform::end_game_session(&app, &game_id, pid, session_seconds, exit_code) {
                Ok((event, achievement_events)) => {
                    let _ = app.emit("launcher://game-exited", event);
                    platform::emit_achievement_events(&app, &achievement_events);
                }
                Err(error) => {
                    let _ = app.emit("launcher://runtime-error", error);
                }
            }
            crate::cloud_save::sync_after_exit_async(app.clone(), game_id.clone());
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

/// Reads the PE header without guessing on malformed or unsupported files.
pub fn is_pe64(path: &Path) -> Result<bool, String> {
    crate::managed_game_runtime::read_pe_architecture(path)
        .map(|architecture| architecture == crate::managed_game_runtime::PeArchitecture::X64)
}
