use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock, RwLock,
    },
};
use tauri::{AppHandle, Emitter, Manager};

const NOTIFICATION_FILE: &str = "notifications.json";
const NOTIFICATION_LIMIT: usize = 200;
static NOTIFICATION_LOCK: OnceLock<RwLock<()>> = OnceLock::new();
static NOTIFICATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationEntity {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub kind: String,
    pub tab: Option<String>,
    pub game_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRecord {
    pub id: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub timestamp: String,
    pub read: bool,
    pub dedupe_key: String,
    pub entity: Option<NotificationEntity>,
    pub action: Option<NotificationAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewNotification {
    pub category: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub dedupe_key: String,
    pub entity: Option<NotificationEntity>,
    pub action: Option<NotificationAction>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushNotificationResult {
    pub record: NotificationRecord,
    pub inserted: bool,
}

fn notification_lock() -> &'static RwLock<()> {
    NOTIFICATION_LOCK.get_or_init(|| RwLock::new(()))
}

fn notification_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(NOTIFICATION_FILE))
}

fn load_unlocked(app: &AppHandle) -> Result<Vec<NotificationRecord>, String> {
    let path = notification_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    match serde_json::from_slice::<Vec<NotificationRecord>>(&bytes) {
        Ok(records) => Ok(records),
        Err(error) => {
            let backup = path.with_extension(format!("corrupt-{}.json", Utc::now().timestamp()));
            let _ = fs::rename(&path, backup);
            Err(format!(
                "notification history was corrupt and has been reset: {error}"
            ))
        }
    }
}

fn write_unlocked(app: &AppHandle, records: &[NotificationRecord]) -> Result<(), String> {
    let path = notification_path(app)?;
    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    let bytes = serde_json::to_vec_pretty(records).map_err(|error| error.to_string())?;
    {
        let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if path.exists() {
        fs::copy(&path, &backup).map_err(|error| error.to_string())?;
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        if backup.exists() {
            let _ = fs::copy(&backup, &path);
        }
        return Err(error.to_string());
    }
    Ok(())
}

pub fn list(app: &AppHandle) -> Result<Vec<NotificationRecord>, String> {
    let _guard = notification_lock()
        .read()
        .map_err(|_| "notification history lock poisoned".to_string())?;
    load_unlocked(app)
}

pub fn push(
    app: &AppHandle,
    mut notification: NewNotification,
) -> Result<PushNotificationResult, String> {
    let _guard = notification_lock()
        .write()
        .map_err(|_| "notification history lock poisoned".to_string())?;
    notification.category = sanitize_category(&notification.category);
    notification.severity = sanitize_severity(&notification.severity);
    notification.title = truncate_text(notification.title, 160);
    notification.message = truncate_text(notification.message, 1_000);
    notification.dedupe_key = truncate_text(notification.dedupe_key, 500);
    if notification.dedupe_key.trim().is_empty() {
        notification.dedupe_key = format!(
            "{}:{}:{}",
            notification.category, notification.title, notification.message
        );
    }
    let mut records = load_unlocked(app).unwrap_or_default();
    if let Some(existing) = records
        .iter()
        .find(|record| record.dedupe_key == notification.dedupe_key)
        .cloned()
    {
        return Ok(PushNotificationResult {
            record: existing,
            inserted: false,
        });
    }

    let now = Utc::now();
    let sequence = NOTIFICATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let record = NotificationRecord {
        id: format!("{}-{sequence}", now.timestamp_millis()),
        category: notification.category,
        severity: notification.severity,
        title: notification.title,
        message: notification.message,
        timestamp: now.to_rfc3339(),
        read: false,
        dedupe_key: notification.dedupe_key,
        entity: notification.entity,
        action: notification.action,
    };
    records.insert(0, record.clone());
    records.truncate(NOTIFICATION_LIMIT);
    write_unlocked(app, &records)?;
    let _ = app.emit("launcher://notification", record.clone());
    Ok(PushNotificationResult {
        record,
        inserted: true,
    })
}

fn sanitize_category(value: &str) -> String {
    match value {
        "launcher" | "installs" | "downloads" | "cloudSaves" | "storage" | "achievements"
        | "errors" => value.to_string(),
        _ => "errors".to_string(),
    }
}

fn sanitize_severity(value: &str) -> String {
    match value {
        "info" | "success" | "warning" | "error" => value.to_string(),
        _ => "info".to_string(),
    }
}

fn truncate_text(mut value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    let truncated = value.char_indices().nth(max_chars).map(|(index, _)| index);
    if let Some(index) = truncated {
        value.truncate(index);
    }
    value
}

pub fn mark_read(
    app: &AppHandle,
    notification_id: &str,
) -> Result<Vec<NotificationRecord>, String> {
    update(app, |records| {
        if let Some(record) = records
            .iter_mut()
            .find(|record| record.id == notification_id)
        {
            record.read = true;
        }
    })
}

pub fn mark_all_read(app: &AppHandle) -> Result<Vec<NotificationRecord>, String> {
    update(app, |records| {
        for record in records {
            record.read = true;
        }
    })
}

pub fn clear(app: &AppHandle) -> Result<Vec<NotificationRecord>, String> {
    update(app, Vec::clear)
}

pub fn open_action(app: &AppHandle, notification_id: &str) -> Result<(), String> {
    let records = mark_read(app, notification_id)?;
    let record = records
        .into_iter()
        .find(|record| record.id == notification_id)
        .ok_or_else(|| "notification no longer exists".to_string())?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(action) = record.action {
        app.emit("launcher://notification-action", action)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn update<F>(app: &AppHandle, update: F) -> Result<Vec<NotificationRecord>, String>
where
    F: FnOnce(&mut Vec<NotificationRecord>),
{
    let _guard = notification_lock()
        .write()
        .map_err(|_| "notification history lock poisoned".to_string())?;
    let mut records = load_unlocked(app).unwrap_or_default();
    update(&mut records);
    write_unlocked(app, &records)?;
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::{sanitize_category, sanitize_severity, truncate_text};

    #[test]
    fn notification_fields_are_bounded_and_validated() {
        assert_eq!(sanitize_category("installs"), "installs");
        assert_eq!(sanitize_category("unknown"), "errors");
        assert_eq!(sanitize_severity("warning"), "warning");
        assert_eq!(sanitize_severity("fatal"), "info");
        assert_eq!(truncate_text("abcdef".to_string(), 3), "abc");
    }
}
