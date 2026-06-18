use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateInfo {
    pub version: String,
    pub notes: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherUpdateProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

pub async fn check_update(app: &AppHandle) -> Result<Option<LauncherUpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;

    Ok(update.map(|update| LauncherUpdateInfo {
        version: update.version,
        notes: update.body.unwrap_or_default(),
        published_at: update.date.map(|date| date.to_string()).unwrap_or_default(),
    }))
}

pub async fn download_and_apply(app: &AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No launcher update is currently available".to_string())?;

    let progress_app = app.clone();
    let finished_app = app.clone();
    let downloaded_bytes = Arc::new(AtomicU64::new(0));
    let progress_downloaded_bytes = downloaded_bytes.clone();
    update
        .download_and_install(
            move |chunk_length, total_bytes| {
                let downloaded_bytes = progress_downloaded_bytes
                    .fetch_add(chunk_length as u64, Ordering::Relaxed)
                    .saturating_add(chunk_length as u64);
                let _ = progress_app.emit(
                    "launcher://update-progress",
                    LauncherUpdateProgress {
                        phase: "downloading".to_string(),
                        downloaded_bytes,
                        total_bytes,
                    },
                );
            },
            move || {
                let downloaded_bytes = downloaded_bytes.load(Ordering::Relaxed);
                let _ = finished_app.emit(
                    "launcher://update-progress",
                    LauncherUpdateProgress {
                        phase: "installing".to_string(),
                        downloaded_bytes,
                        total_bytes: Some(downloaded_bytes),
                    },
                );
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    app.restart()
}
