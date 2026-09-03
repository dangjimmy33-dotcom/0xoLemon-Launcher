// Steam Cloud Specs & Signatures Auto-Fetch Engine
// Handles dynamic downloading and local caching of Steam IPC specs and supported build versions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;

const SPECS_CACHE_FILE: &str = "steam_specs_cache.json";
const REMOTE_SPECS_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/dangjimmy33-dotcom/0xoLemon-Launcher/main/steam_specs.json",
    "https://huggingface.co/datasets/Immaking/Luas/raw/main/steam_specs.json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownBuildInfo {
    pub hash: Option<String>,
    pub description: Option<String>,
    pub dynamic_scan_allowed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSpecsCache {
    pub last_updated: u64,
    pub supported_versions: Vec<i64>,
    pub known_builds: HashMap<String, KnownBuildInfo>,
}

impl Default for CloudSpecsCache {
    fn default() -> Self {
        let mut known = HashMap::new();
        known.insert(
            "1788291500".to_string(),
            KnownBuildInfo {
                hash: Some("2a5c1b50bbc052bc725b488ca299902ab934fe26f710135a0207d6e8958c5542".to_string()),
                description: Some("Steam Client Build 1788291500 (Auto-Adapted)".to_string()),
                dynamic_scan_allowed: Some(true),
            },
        );
        Self {
            last_updated: 1788291500,
            supported_versions: vec![
                1788291500,
                1782866176,
                1782344391,
                1782257239,
                1781041600,
                1780352834,
                1779918128,
                1779486452,
                1778281814,
                1778003620,
            ],
            known_builds: known,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecsUpdateSummary {
    pub success: bool,
    pub message: String,
    pub total_supported_versions: usize,
    pub latest_version: Option<i64>,
}

static SPECS_CACHE: Lazy<Mutex<Option<CloudSpecsCache>>> = Lazy::new(|| Mutex::new(None));

fn get_cache_path() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("007Launcher");
        let _ = fs::create_dir_all(&dir);
        return Some(dir.join(SPECS_CACHE_FILE));
    }
    None
}

pub fn load_cached_specs() -> CloudSpecsCache {
    if let Ok(guard) = SPECS_CACHE.lock() {
        if let Some(ref cache) = *guard {
            return cache.clone();
        }
    }

    let loaded = if let Some(path) = get_cache_path() {
        if path.is_file() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<CloudSpecsCache>(&data) {
                    parsed
                } else {
                    CloudSpecsCache::default()
                }
            } else {
                CloudSpecsCache::default()
            }
        } else {
            let def = CloudSpecsCache::default();
            let _ = save_cached_specs(&def);
            def
        }
    } else {
        CloudSpecsCache::default()
    };

    if let Ok(mut guard) = SPECS_CACHE.lock() {
        *guard = Some(loaded.clone());
    }
    loaded
}

pub fn save_cached_specs(cache: &CloudSpecsCache) -> Result<(), String> {
    if let Some(path) = get_cache_path() {
        let json = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())?;
    }
    if let Ok(mut guard) = SPECS_CACHE.lock() {
        *guard = Some(cache.clone());
    }
    Ok(())
}

pub fn is_cloud_supported_version(version: i64) -> bool {
    let cache = load_cached_specs();
    if cache.supported_versions.contains(&version) {
        return true;
    }
    let ver_str = version.to_string();
    if let Some(info) = cache.known_builds.get(&ver_str) {
        return info.dynamic_scan_allowed.unwrap_or(true);
    }
    false
}

pub fn register_dynamically_adapted_version(version: i64, hash: Option<String>) {
    let mut cache = load_cached_specs();
    if !cache.supported_versions.contains(&version) {
        cache.supported_versions.insert(0, version);
    }
    let ver_str = version.to_string();
    cache.known_builds.entry(ver_str).or_insert_with(|| KnownBuildInfo {
        hash,
        description: Some(format!("Dynamic AOB adapted build {version}")),
        dynamic_scan_allowed: Some(true),
    });
    let _ = save_cached_specs(&cache);
}

pub fn fetch_and_update_cloud_specs() -> Result<SpecsUpdateSummary, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut fetched_cache: Option<CloudSpecsCache> = None;

    for url in REMOTE_SPECS_URLS {
        if let Ok(resp) = client.get(*url).send() {
            if resp.status().is_success() {
                if let Ok(text) = resp.text() {
                    if let Ok(parsed) = serde_json::from_str::<CloudSpecsCache>(&text) {
                        fetched_cache = Some(parsed);
                        break;
                    }
                }
            }
        }
    }

    let mut current = load_cached_specs();

    if let Some(remote) = fetched_cache {
        for v in remote.supported_versions {
            if !current.supported_versions.contains(&v) {
                current.supported_versions.push(v);
            }
        }
        for (k, v) in remote.known_builds {
            current.known_builds.insert(k, v);
        }
        current.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(current.last_updated);
        save_cached_specs(&current)?;

        let latest = current.supported_versions.first().copied();
        Ok(SpecsUpdateSummary {
            success: true,
            message: "Đồng bộ đặc tả Steam specs từ Cloud thành công.".to_string(),
            total_supported_versions: current.supported_versions.len(),
            latest_version: latest,
        })
    } else {
        // Even if offline/CDN not reachable, ensure local default is persisted with latest build 1788291500
        if !current.supported_versions.contains(&1788291500) {
            current.supported_versions.insert(0, 1788291500);
            let _ = save_cached_specs(&current);
        }
        let latest = current.supported_versions.first().copied();
        Ok(SpecsUpdateSummary {
            success: true,
            message: "Đã sử dụng cấu hình đặc tả offline mới nhất (Build 1788291500 đã sẵn sàng).".to_string(),
            total_supported_versions: current.supported_versions.len(),
            latest_version: latest,
        })
    }
}
