use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
#[cfg(windows)]
use std::{ffi::OsString, path::Path};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[cfg(windows)]
const NSIS_PRESERVE_INSTALL_DIR_FLAG: &str = "/OXO_PRESERVE_INSTALL_DIR";

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
    let update = configured_updater(app)?
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
    let update = configured_updater(app)?
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

    // Tauri launches NSIS with /UPDATE. configured_updater also passes /D as
    // the final argument so registry scope changes cannot redirect the update.
    if let Err(error) = update.install(bytes) {
        let msg = error.to_string();
        emit_progress(
            app,
            &version,
            "failed",
            total,
            Some(total),
            Some(msg.clone()),
        );
        return Err(msg);
    }

    emit_progress(app, &version, "restarting", total, Some(total), None);
    Ok(())
}

fn configured_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    #[cfg(windows)]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Unable to locate the running launcher: {error}"))?;
        let installer_args = nsis_install_args_for_executable(&executable)?;

        // /D must be the final NSIS argument and cannot be quoted. The updater
        // preserves insertion order, so keep the install directory last.
        return app
            .updater_builder()
            .installer_args(installer_args)
            .build()
            .map_err(|error| error.to_string());
    }

    #[cfg(not(windows))]
    {
        app.updater().map_err(|error| error.to_string())
    }
}

#[cfg(windows)]
fn nsis_install_args_for_executable(executable: &Path) -> Result<Vec<OsString>, String> {
    let install_dir = executable
        .parent()
        .filter(|path| path.is_absolute() && !path.as_os_str().is_empty())
        .ok_or_else(|| {
            format!(
                "Unable to determine the launcher install directory from {}",
                executable.display()
            )
        })?;

    let mut install_dir_arg = OsString::from("/D=");
    install_dir_arg.push(install_dir.as_os_str());

    Ok(vec![
        OsString::from(NSIS_PRESERVE_INSTALL_DIR_FLAG),
        install_dir_arg,
    ])
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn nsis_install_dir_is_the_running_executable_directory() {
        let args = nsis_install_args_for_executable(Path::new(
            r"E:\Apps With Spaces\0xoLemon\0xoLemon.exe",
        ))
        .expect("absolute executable path should be accepted");

        assert_eq!(args.len(), 2);
        assert_eq!(args[0], OsString::from(NSIS_PRESERVE_INSTALL_DIR_FLAG));
        assert_eq!(args[1], OsString::from(r"/D=E:\Apps With Spaces\0xoLemon"));
    }

    #[test]
    fn nsis_install_dir_rejects_relative_executable_paths() {
        let error = nsis_install_args_for_executable(Path::new("0xoLemon.exe"))
            .expect_err("relative executable path must not fall back to Program Files");

        assert!(error.contains("Unable to determine"));
    }
}
