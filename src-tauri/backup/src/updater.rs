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
pub struct LauncherUpdateProgress {
    version: String,
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    timestamp: String,
    error: Option<String>,
}

pub async fn check_update(app: &AppHandle) -> Result<Option<LauncherUpdateInfo>, String> {
    emit_progress(app, "", "checking", 0, None, None);
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

    let version = update.version.clone();
    emit_progress(app, &version, "downloading", 0, None, None);
    let progress_app = app.clone();
    let progress_version = version.clone();
    let finished_app = app.clone();
    let finished_version = version.clone();
    let downloaded_bytes = Arc::new(AtomicU64::new(0));
    let progress_downloaded_bytes = downloaded_bytes.clone();
    let bytes = match update
        .download(
            move |chunk_length, total_bytes| {
                let downloaded_bytes = progress_downloaded_bytes
                    .fetch_add(chunk_length as u64, Ordering::Relaxed)
                    .saturating_add(chunk_length as u64);
                emit_progress(
                    &progress_app,
                    &progress_version,
                    "downloading",
                    downloaded_bytes,
                    total_bytes,
                    None,
                );
            },
            move || {
                let downloaded_bytes = downloaded_bytes.load(Ordering::Relaxed);
                emit_progress(
                    &finished_app,
                    &finished_version,
                    "verifying",
                    downloaded_bytes,
                    Some(downloaded_bytes),
                    None,
                );
            },
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = error.to_string();
            emit_progress(app, &version, "failed", 0, None, Some(message.clone()));
            return Err(message);
        }
    };

    let total = bytes.len() as u64;
    emit_progress(app, &version, "installing", total, Some(total), None);
    if let Err(error) = update.install(&bytes) {
        let message = error.to_string();
        emit_progress(
            app,
            &version,
            "failed",
            total,
            Some(total),
            Some(message.clone()),
        );
        return Err(message);
    }

    emit_progress(app, &version, "restarting", total, Some(total), None);
    app.restart()
}

fn emit_progress(
    app: &AppHandle,
    version: &str,
    phase: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
) {
    let _ = app.emit(
        "launcher://update-progress",
        LauncherUpdateProgress {
            version: version.to_string(),
            phase: phase.to_string(),
            downloaded_bytes,
            total_bytes,
            timestamp: chrono::Utc::now().to_rfc3339(),
            error,
        },
    );
}
