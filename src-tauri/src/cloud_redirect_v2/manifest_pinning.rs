use crate::cloud_redirect::steam_detector::{find_steam_path, is_steam_running};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::models::{ManifestPinConfig, ManifestPinConfigInput};

const OWNED_KEYS: [&str; 3] = ["manifest_pinning", "auto_comment", "pinned_apps"];

fn config_path() -> Result<PathBuf, String> {
    let steam = find_steam_path().ok_or_else(|| "Steam installation was not found".to_string())?;
    Ok(steam.join("cloud_redirect").join("config.json"))
}

fn read_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

pub fn load() -> Result<ManifestPinConfig, String> {
    let path = config_path()?;
    let root = read_object(&path);
    let mut pinned_apps: Vec<String> = root
        .get("pinned_apps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value
                .as_u64()
                .map(|number| number.to_string())
                .or_else(|| value.as_str().map(ToString::to_string))
        })
        .filter(|value| value.parse::<u32>().ok().is_some_and(|number| number > 0))
        .collect();
    pinned_apps.sort();
    pinned_apps.dedup();

    Ok(ManifestPinConfig {
        enabled: root
            .get("manifest_pinning")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        auto_comment: root
            .get("auto_comment")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        pinned_apps,
        path: path.to_string_lossy().into_owned(),
        restart_required: is_steam_running(),
    })
}

pub fn save(input: ManifestPinConfigInput) -> Result<ManifestPinConfig, String> {
    let path = config_path()?;
    let mut root = read_object(&path);
    for key in OWNED_KEYS {
        root.remove(key);
    }

    let mut pinned_apps: Vec<u32> = input
        .pinned_apps
        .iter()
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("Invalid Steam AppID: {value}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    pinned_apps.retain(|value| *value > 0);
    pinned_apps.sort_unstable();
    pinned_apps.dedup();

    root.insert("manifest_pinning".to_string(), Value::Bool(input.enabled));
    root.insert("auto_comment".to_string(), Value::Bool(input.auto_comment));
    root.insert(
        "pinned_apps".to_string(),
        Value::Array(
            pinned_apps
                .iter()
                .map(|value| Value::Number((*value).into()))
                .collect(),
        ),
    );

    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid manifest pin path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;

    let backup = path.with_extension("json.bak");
    if path.is_file() {
        let _ = fs::copy(&path, &backup);
    }
    let temporary = path.with_extension("json.new");
    let payload = serde_json::to_vec_pretty(&Value::Object(root))
        .map_err(|error| format!("Cannot serialize manifest pin config: {error}"))?;
    fs::write(&temporary, payload)
        .map_err(|error| format!("Cannot write {}: {error}", temporary.display()))?;
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|error| format!("Cannot replace {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, &path).map_err(|error| {
        format!(
            "Cannot commit manifest pin config {}: {error}",
            path.display()
        )
    })?;

    Ok(ManifestPinConfig {
        enabled: input.enabled,
        auto_comment: input.auto_comment,
        pinned_apps: pinned_apps.into_iter().map(|value| value.to_string()).collect(),
        path: path.to_string_lossy().into_owned(),
        restart_required: is_steam_running(),
    })
}
