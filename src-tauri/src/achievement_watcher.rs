use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::AppHandle;

use crate::notifications::{self, NewNotification};
use crate::platform;

// How long we wait for game process to connect (Layer 1 TCP is realtime, others are polled)
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct AchievementWatcher {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    /// The TCP port the emulator should connect to. Set as env var ACHIEVEMENT_TCP_PORT on game start.
    pub tcp_port: u16,
}

impl AchievementWatcher {
    pub fn tcp_port(&self) -> u16 {
        self.tcp_port
    }
}

impl Drop for AchievementWatcher {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

pub fn start_session(
    app: AppHandle,
    game_id: &str,
    app_id: Option<u32>,
    install_path: &Path,
    _pid: u32,
) -> AchievementWatcher {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    // Layer 1: Bind a TCP server on any free port
    let tcp_listener = TcpListener::bind("127.0.0.1:0").ok();
    let tcp_port = tcp_listener
        .as_ref()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(0);

    let game_id = game_id.to_string();
    let install_path = install_path.to_path_buf();

    thread::spawn(move || {
        let mut seen: HashSet<String> = HashSet::new();

        // --- Layer 3: Goldberg JSON file fallback ---
        let goldberg_file = find_goldberg_file(app_id, &install_path);
        let mut last_goldberg_mtime = goldberg_file
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());

        // --- Layer 1: Non-blocking TCP listener thread ---
        // Achievements pushed here by our patched steam_api.dll in realtime.
        let pending_tcp: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let pending_tcp_clone = Arc::clone(&pending_tcp);

        if let Some(listener) = tcp_listener {
            listener.set_nonblocking(true).ok();
            thread::spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(mut s) => {
                            let mut buf = vec![0u8; 256];
                            if let Ok(n) = s.read(&mut buf) {
                                if n > 0 {
                                    if let Ok(name) = std::str::from_utf8(&buf[..n]) {
                                        let name = name.trim_end_matches('\0').to_string();
                                        if !name.is_empty() {
                                            pending_tcp_clone.lock().unwrap().push(name);
                                        }
                                    }
                                }
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // --- Main polling loop ---
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            // Drain Layer 1 TCP (realtime, from our patched emulator)
            {
                let mut pending = pending_tcp.lock().unwrap();
                for ach_name in pending.drain(..) {
                    if seen.insert(ach_name.clone()) {
                        emit_achievement(&app, &game_id, &ach_name);
                    }
                }
            }

            // Layer 3 fallback: poll Goldberg achievements.json
            if let Some(ref path) = goldberg_file {
                if let Ok(metadata) = fs::metadata(path) {
                    if let Ok(mtime) = metadata.modified() {
                        if Some(mtime) != last_goldberg_mtime {
                            last_goldberg_mtime = Some(mtime);
                            parse_goldberg_json(path, &mut seen, &app, &game_id);
                        }
                    }
                }
            }

            thread::sleep(POLL_INTERVAL);
        }
    });

    AchievementWatcher {
        stop_tx: Some(stop_tx),
        tcp_port,
    }
}

fn find_goldberg_file(app_id: Option<u32>, install_path: &Path) -> Option<PathBuf> {
    if let Some(app_id) = app_id {
        if let Some(appdata) = dirs::data_dir() {
            let candidates = [
                appdata.join("GSE Saves").join(app_id.to_string()).join("achievements.json"),
                appdata.join("GSE Saves").join(app_id.to_string()).join("stats").join("achievements.json"),
                appdata.join("Goldberg SteamEmu Saves").join(app_id.to_string()).join("achievements.json"),
            ];
            for p in &candidates {
                if p.exists() {
                    return Some(p.clone());
                }
            }
        }
    }
    // Last resort: check inside game dir
    let fallback = install_path.join("steam_settings").join("achievements.json");
    if fallback.exists() {
        return Some(fallback);
    }
    None
}

fn parse_goldberg_json(path: &Path, seen: &mut HashSet<String>, app: &AppHandle, game_id: &str) {
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(obj) = json.as_object() {
                for (key, value) in obj {
                    let unlocked = match value {
                        serde_json::Value::Bool(b) => *b,
                        serde_json::Value::Object(o) => {
                            o.get("earned").and_then(|v| v.as_bool()).unwrap_or(false)
                                || o.get("unlocked").and_then(|v| v.as_bool()).unwrap_or(false)
                        }
                        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) > 0,
                        _ => false,
                    };
                    if unlocked && seen.insert(key.clone()) {
                        emit_achievement(app, game_id, key);
                    }
                }
            }
        }
    }
}

fn emit_achievement(app: &AppHandle, game_id: &str, achievement_id: &str) {
    if let Ok(events) = platform::record_activity(app, game_id, achievement_id) {
        platform::emit_achievement_events(app, &events);
        for event in events {
            let _ = notifications::push(
                app,
                NewNotification {
                    category: "achievements".to_string(),
                    severity: "success".to_string(),
                    title: "Achievement Unlocked".to_string(),
                    message: event.id.clone(),
                    dedupe_key: format!("{}-{}", game_id, event.id),
                    entity: None,
                    action: None,
                },
            );
        }
    }
}
