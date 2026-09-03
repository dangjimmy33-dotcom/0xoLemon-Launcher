// depot_downloader.rs — 0xoLemon Depot Downloader backend
// Downloads game depot manifests & keys from HuggingFace dataset (Immaking/Luas/Depotdownloader/)
// and executes DepotDownloaderMod.exe (self-contained .NET 9 binary) to download clean game files.

use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{command, AppHandle, Emitter};

// ─── HuggingFace Config & Auth ───────────────────────────────────────────────

fn get_hf_token() -> String {
    let json_str = include_str!("../huggingface-repos.json");
    let config: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
    config["repositories"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|r| {
            let id = r["repoId"].as_str().unwrap_or("");
            id.eq_ignore_ascii_case("Immaking/Luas") || id.eq_ignore_ascii_case("lmmaking/Luas")
        })
        .and_then(|r| r["token"].as_str())
        .unwrap_or("")
        .to_string()
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .user_agent("0xoLemon-Launcher/2.0.50 (Windows NT 10.0; Win64; x64)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP Client Error: {}", e))
}

fn auth_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if !token.is_empty() {
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
            headers.insert(AUTHORIZATION, val);
        }
    }
    headers
}

fn api_tree_base() -> String {
    "https://huggingface.co/api/datasets/Immaking/Luas/tree/main".to_string()
}

fn raw_base() -> String {
    "https://huggingface.co/datasets/Immaking/Luas/raw/main".to_string()
}

fn pct_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '(' => "%28".to_string(),
            ')' => "%29".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

fn catalog_folder_appid(folder_name: &str) -> Option<u32> {
    let open = folder_name.rfind('(')?;
    let close = folder_name.rfind(')')?;
    if close <= open + 1 {
        return None;
    }
    folder_name[open + 1..close].trim().parse::<u32>().ok()
}

fn validate_catalog_identity(appid: u32, folder_name: &str) -> Result<(), String> {
    let folder_appid = catalog_folder_appid(folder_name).ok_or_else(|| {
        format!(
            "DEPOT_CATALOG_INVALID: thư mục kho '{}' không chứa AppID hợp lệ.",
            folder_name
        )
    })?;

    if folder_appid != appid {
        return Err(format!(
            "DEPOT_SELECTION_MISMATCH: game đang chọn là AppID {} nhưng thư mục kho '{}' thuộc AppID {}. Hãy chọn lại game trước khi tải.",
            appid, folder_name, folder_appid
        ));
    }

    Ok(())
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct HfNode {
    #[serde(rename = "type")]
    node_type: String,
    path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotGameItem {
    pub appid: u32,
    pub title: String,
    pub folder_name: String,
    pub banner_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotManifestInfo {
    pub depot_id: u32,
    pub manifest_gid: String,
    pub manifest_file: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotBuildOption {
    pub build_id: String,
    pub version: Option<String>,
    pub build_date: Option<String>,
    pub manifests: Vec<DepotManifestInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotGameDetail {
    pub appid: u32,
    pub title: String,
    pub folder_name: String,
    pub builds: Vec<DepotBuildOption>,
    pub has_key: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotDownloadProgressEvent {
    pub event_type: String, // "start" | "depot-start" | "progress" | "log" | "depot-done" | "paused" | "resumed" | "complete" | "error" | "cancelled"
    pub appid: u32,
    pub build_id: String,
    pub depot_id: Option<String>,
    pub message: Option<String>,
    pub current_depot_index: usize,
    pub total_depots: usize,
    pub progress_percent: Option<f64>,
    pub speed_mbps: Option<f64>,
    pub transferred_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub success: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepotDownloaderStatus {
    pub is_downloading: bool,
    pub is_paused: bool,
    pub can_resume: bool,
    pub active_appid: Option<u32>,
    pub active_build_id: Option<String>,
    pub destination_dir: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DepotInstallState {
    pub appid: u32,
    pub installed_build_id: Option<String>,
    pub manifests: HashMap<String, String>,
    pub completed_unix: Option<u64>,
    pub has_depot_state: bool,
}

// ─── Active Download State ───────────────────────────────────────────────────

static ACTIVE_DOWNLOAD: Lazy<Mutex<Option<ActiveDownloadState>>> = Lazy::new(|| Mutex::new(None));
static CANCEL_REQUESTED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static PAUSE_REQUESTED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[derive(Clone)]
struct ActiveDownloadState {
    appid: u32,
    folder_name: String,
    build_id: String,
    destination_dir: String,
    max_downloads: u32,
    verify_all: bool,
    child_process_id: Option<u32>,
    paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineOutcome {
    Completed,
    Paused,
}

fn launcher_depot_state_dir(destination_dir: &Path) -> PathBuf {
    destination_dir.join(".DepotDownloader").join("0xolemon")
}

fn version_state_path(destination_dir: &Path) -> PathBuf {
    launcher_depot_state_dir(destination_dir).join("version-state.json")
}

fn manifest_cache_dir(destination_dir: &Path, build_id: &str) -> PathBuf {
    launcher_depot_state_dir(destination_dir)
        .join("versions")
        .join(format!("BuildID_{}", build_id))
}

fn read_install_state_path(destination_dir: &Path, appid: u32) -> DepotInstallState {
    let state_path = version_state_path(destination_dir);
    if let Ok(bytes) = fs::read(&state_path) {
        if let Ok(state) = serde_json::from_slice::<DepotInstallState>(&bytes) {
            if state.appid == appid {
                return state;
            }
        }
    }
    DepotInstallState {
        appid,
        installed_build_id: None,
        manifests: HashMap::new(),
        completed_unix: None,
        has_depot_state: destination_dir.join(".DepotDownloader").is_dir(),
    }
}

fn write_install_state(
    destination_dir: &Path,
    appid: u32,
    build_id: &str,
    manifests: &[DepotManifestInfo],
) -> Result<(), String> {
    let state_dir = launcher_depot_state_dir(destination_dir);
    fs::create_dir_all(&state_dir).map_err(|e| format!("Could not create Depot state folder: {e}"))?;
    let manifest_map = manifests
        .iter()
        .map(|m| (m.depot_id.to_string(), m.manifest_gid.clone()))
        .collect::<HashMap<_, _>>();
    let completed_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_secs());
    let state = DepotInstallState {
        appid,
        installed_build_id: Some(build_id.to_string()),
        manifests: manifest_map,
        completed_unix,
        has_depot_state: true,
    };
    let bytes = serde_json::to_vec_pretty(&state).map_err(|e| format!("Could not serialize Depot version state: {e}"))?;
    let path = version_state_path(destination_dir);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("Could not write Depot version state: {e}"))?;
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&tmp, &path).map_err(|e| format!("Could not commit Depot version state: {e}"))
}

/// Seed DepotDownloaderMod's native manifest cache rather than using
/// `-manifestfile`. In this fork, `-manifestfile` replaces `oldManifest` with
/// the supplied target manifest, which prevents A -> B deleted-file diffing.
/// The normal code path loads `<depot>_<gid>.manifest` plus its raw SHA-1 from
/// `.DepotDownloader`, preserving the real previous manifest from depot.config.
pub(crate) fn seed_native_manifest_cache(destination_dir: &Path, manifest_path: &Path) -> Result<PathBuf, String> {
    let file_name = manifest_path
        .file_name()
        .ok_or_else(|| "Manifest cache source has no file name".to_string())?;
    let native_dir = destination_dir.join(".DepotDownloader");
    fs::create_dir_all(&native_dir)
        .map_err(|e| format!("Could not create DepotDownloader native manifest cache: {e}"))?;

    let native_path = native_dir.join(file_name);
    if manifest_path != native_path {
        fs::copy(manifest_path, &native_path)
            .map_err(|e| format!("Could not seed native DepotDownloader manifest {}: {e}", native_path.display()))?;
    }

    let bytes = fs::read(&native_path)
        .map_err(|e| format!("Could not hash native DepotDownloader manifest {}: {e}", native_path.display()))?;
    let digest = Sha1::digest(&bytes);
    let sha_path = native_dir.join(format!("{}.sha", file_name.to_string_lossy()));
    fs::write(&sha_path, digest.as_slice())
        .map_err(|e| format!("Could not write native DepotDownloader manifest checksum {}: {e}", sha_path.display()))?;
    Ok(native_path)
}

/// Compatibility migration for downloads completed by older launcher builds:
/// replay every cached manifest for the recorded BuildID into the fork's
/// `.DepotDownloader` cache so depot.config can resolve the true old manifest.
fn seed_previous_build_manifests(destination_dir: &Path, build_id: &str) -> Result<usize, String> {
    let cache = manifest_cache_dir(destination_dir, build_id);
    if !cache.is_dir() {
        return Ok(0);
    }
    let mut seeded = 0usize;
    let entries = fs::read_dir(&cache)
        .map_err(|e| format!("Could not read cached BuildID manifests {}: {e}", cache.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Could not inspect cached BuildID manifest: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()).map(|value| value.eq_ignore_ascii_case("manifest")) != Some(true) {
            continue;
        }
        seed_native_manifest_cache(destination_dir, &path)?;
        seeded += 1;
    }
    Ok(seeded)
}

// ─── DepotDownloader Binary Resolution ───────────────────────────────────────

pub fn resolve_depot_downloader_exe() -> Result<PathBuf, String> {
    // 1. Check if packaged or installed via sff_packages
    if let Some(installed) =
        crate::sff_packages::resolve_installed_entrypoint("depot-downloader-mod")
    {
        if installed.is_file() {
            return Ok(installed);
        }
    }

    // 2. Check in src-tauri/binaries/ (development/direct release single-file executable)
    let binaries_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let candidates = [
        binaries_dir.join("DepotDownloaderMod-x86_64-pc-windows-msvc.exe"),
        binaries_dir.join("_ddmod_pub").join("DepotDownloaderMod.exe"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("release")
            .join("DepotDownloaderMod.exe"),
        PathBuf::from(r"E:\Among Us DepotDownloader\Among Us (945360)\DepotDownloaderMod\DepotDownloaderMod.exe"),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    // 3. Fallback: try on-demand feature package download
    crate::sff_packages::ensure_feature_package("depot-downloader-mod")
}

// ─── Commands ───────────────────────────────────────────────────────────────

/// List all games available under the `Depotdownloader/` catalog on HuggingFace.
#[command]
pub async fn depot_downloader_get_catalog() -> Result<Vec<DepotGameItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = build_client()?;
        let token = get_hf_token();
        let url = format!("{}/Depotdownloader", api_tree_base());

        let resp = client
            .get(&url)
            .headers(auth_headers(&token))
            .send()
            .map_err(|e| format!("Không thể tải danh sách kho Depot: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Lỗi kết nối máy chủ kho (HTTP {})", resp.status()));
        }

        let nodes: Vec<HfNode> = resp
            .json()
            .map_err(|e| format!("Lỗi phân tích dữ liệu kho: {}", e))?;

        let mut items = Vec::new();

        for node in nodes {
            if node.node_type != "directory" {
                continue;
            }
            let folder = node.path.split('/').last().unwrap_or("").to_string();
            // Format is: "<Game Title> (<AppID>)"
            if let Some(open_paren) = folder.rfind('(') {
                if let Some(close_paren) = folder.rfind(')') {
                    if close_paren > open_paren {
                        let appid_str = &folder[open_paren + 1..close_paren].trim();
                        let title = folder[..open_paren].trim().to_string();
                        if let Ok(appid) = appid_str.parse::<u32>() {
                            if appid > 0 {
                                let banner_url = Some(format!(
                                    "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg",
                                    appid
                                ));
                                items.push(DepotGameItem {
                                    appid,
                                    title: if title.is_empty() { format!("Game {}", appid) } else { title },
                                    folder_name: folder,
                                    banner_url,
                                });
                            }
                        }
                    }
                }
            }
        }

        items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(items)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Fetch build versions, dates, manifests, and key status for a selected game.
#[command]
pub async fn depot_downloader_get_game_detail(
    appid: u32,
    folder_name: String,
) -> Result<DepotGameDetail, String> {
    validate_catalog_identity(appid, &folder_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let client = build_client()?;
        let token = get_hf_token();

        let rel_path = format!("Depotdownloader/{}/{}", folder_name, appid);
        let url = format!("{}/{}", api_tree_base(), pct_encode(&rel_path));

        let resp = client
            .get(&url)
            .headers(auth_headers(&token))
            .send()
            .map_err(|e| format!("Lỗi kết nối kho game: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Không tìm thấy dữ liệu game (HTTP {})", resp.status()));
        }

        let nodes: Vec<HfNode> = resp
            .json()
            .map_err(|e| format!("Lỗi đọc cấu trúc game: {}", e))?;

        let mut raw_build_ids: Vec<String> = Vec::new();
        let mut has_key = false;

        for node in &nodes {
            let leaf = node.path.split('/').last().unwrap_or("");
            if node.node_type == "directory" {
                if let Some(stripped) = leaf.strip_prefix("BuildID_")
                    .or_else(|| leaf.strip_prefix("BuildId_"))
                    .or_else(|| leaf.strip_prefix("buildid_"))
                {
                    raw_build_ids.push(stripped.to_string());
                }
            } else if node.node_type == "file" && leaf.ends_with(".key") {
                has_key = true;
            }
        }

        raw_build_ids.sort_by(|a, b| {
            b.parse::<u64>()
                .unwrap_or(0)
                .cmp(&a.parse::<u64>().unwrap_or(0))
        });

        let mut builds = Vec::new();
        for bid in &raw_build_ids {
            let build_rel = format!("Depotdownloader/{}/{}/BuildID_{}", folder_name, appid, bid);
            let build_url = format!("{}/{}", api_tree_base(), pct_encode(&build_rel));

            let mut manifests: Vec<DepotManifestInfo> = Vec::new();
            let mut version: Option<String> = None;

            if let Ok(b_resp) = client.get(&build_url).headers(auth_headers(&token)).send() {
                if b_resp.status().is_success() {
                    if let Ok(b_nodes) = b_resp.json::<Vec<HfNode>>() {
                        for file in &b_nodes {
                            let fname = file.path.split('/').last().unwrap_or("").to_string();
                            if fname.ends_with(".manifest") {
                                let stem = fname.trim_end_matches(".manifest");
                                if let Some(up) = stem.find('_') {
                                    let depot_str = &stem[..up];
                                    let gid_str = &stem[up + 1..];
                                    if let Ok(depot_id) = depot_str.parse::<u32>() {
                                        manifests.push(DepotManifestInfo {
                                            depot_id,
                                            manifest_gid: gid_str.to_string(),
                                            manifest_file: fname,
                                        });
                                    }
                                }
                            } else if fname == "version.txt" {
                                let ver_rel = format!(
                                    "Depotdownloader/{}/{}/BuildID_{}/version.txt",
                                    folder_name, appid, bid
                                );
                                let ver_url = format!("{}/{}", raw_base(), pct_encode(&ver_rel));
                                if let Ok(vr) = client.get(&ver_url).headers(auth_headers(&token)).send() {
                                    if vr.status().is_success() {
                                        version = vr.text().ok().map(|t| t.trim().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            manifests.sort_by(|a, b| a.depot_id.cmp(&b.depot_id));

            builds.push(DepotBuildOption {
                build_id: bid.clone(),
                version,
                build_date: None,
                manifests,
            });
        }

        // Try to fetch real update dates from SteamCMD API
        if let Ok(steamcmd_resp) = client.get(format!("https://api.steamcmd.net/v1/info/{}", appid)).send() {
            if steamcmd_resp.status().is_success() {
                if let Ok(json) = steamcmd_resp.json::<serde_json::Value>() {
                    if let Some(branches) = json
                        .get("data")
                        .and_then(|d| d.get(appid.to_string()))
                        .and_then(|a| a.get("depots"))
                        .and_then(|d| d.get("branches"))
                        .and_then(|b| b.as_object())
                    {
                        let mut date_map = HashMap::new();
                        for (_branch_name, branch_data) in branches {
                            if let Some(bid) = branch_data.get("buildid").and_then(|v| v.as_str()) {
                                if let Some(tupdate) = branch_data.get("timeupdated").and_then(|v| v.as_str()) {
                                    date_map.insert(bid.to_string(), tupdate.to_string());
                                }
                            }
                        }

                        for build in &mut builds {
                            if let Some(date_str) = date_map.get(&build.build_id) {
                                build.build_date = Some(date_str.clone());
                            }
                        }
                    }
                }
            }
        }

        Ok(DepotGameDetail {
            appid,
            title: folder_name.split('(').next().unwrap_or(&folder_name).trim().to_string(),
            folder_name,
            builds,
            has_key,
        })
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Download a file from HuggingFace to a local path.
fn download_hf_raw_file(
    client: &Client,
    rel_path: &str,
    dest: &Path,
    token: &str,
) -> Result<(), String> {
    let url = format!("{}/{}", raw_base(), pct_encode(rel_path));
    let mut req = client.get(&url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .map_err(|e| format!("Lỗi tải tệp {}: {}", rel_path, e))?;

    if !resp.status().is_success() {
        return Err(format!("Máy chủ từ chối tải tệp {} (HTTP {})", rel_path, resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| format!("Lỗi đọc dữ liệu: {}", e))?;
    fs::write(dest, &bytes).map_err(|e| format!("Lỗi ghi tệp ra đĩa: {}", e))
}

/// Start a background DepotDownloaderMod task from a complete, immutable job snapshot.
fn launch_download_task(app: AppHandle, job: ActiveDownloadState) {
    tauri::async_runtime::spawn(async move {
        let result = run_download_pipeline(
            &app,
            job.appid,
            &job.folder_name,
            &job.build_id,
            &job.destination_dir,
            job.max_downloads,
            job.verify_all,
        );

        match result {
            Ok(PipelineOutcome::Completed) => {
                if let Ok(mut active) = ACTIVE_DOWNLOAD.lock() {
                    *active = None;
                }
                let _ = app.emit(
                    "depot-download-progress",
                    DepotDownloadProgressEvent {
                        event_type: "complete".to_string(),
                        appid: job.appid,
                        build_id: job.build_id,
                        depot_id: None,
                        message: Some("Tải hoàn tất toàn bộ depots của game!".to_string()),
                        current_depot_index: 0,
                        total_depots: 0,
                        progress_percent: Some(100.0),
                        speed_mbps: None,
                        transferred_bytes: None,
                        total_bytes: None,
                        success: Some(true),
                    },
                );
            }
            Ok(PipelineOutcome::Paused) => {
                if let Ok(mut active) = ACTIVE_DOWNLOAD.lock() {
                    if let Some(state) = active.as_mut() {
                        state.child_process_id = None;
                        state.paused = true;
                    }
                }
                let _ = app.emit(
                    "depot-download-progress",
                    DepotDownloadProgressEvent {
                        event_type: "paused".to_string(),
                        appid: job.appid,
                        build_id: job.build_id,
                        depot_id: None,
                        message: Some("Đã tạm dừng. Dữ liệu hiện có, .DepotDownloader và staging được giữ nguyên để tiếp tục.".to_string()),
                        current_depot_index: 0,
                        total_depots: 0,
                        progress_percent: None,
                        speed_mbps: None,
                        transferred_bytes: None,
                        total_bytes: None,
                        success: None,
                    },
                );
            }
            Err(error) => {
                let was_cancelled = CANCEL_REQUESTED.load(Ordering::SeqCst);
                if let Ok(mut active) = ACTIVE_DOWNLOAD.lock() {
                    *active = None;
                }
                let _ = app.emit(
                    "depot-download-progress",
                    DepotDownloadProgressEvent {
                        event_type: if was_cancelled { "cancelled".to_string() } else { "error".to_string() },
                        appid: job.appid,
                        build_id: job.build_id,
                        depot_id: None,
                        message: Some(if was_cancelled {
                            "Tiến trình tải đã bị hủy bởi người dùng.".to_string()
                        } else {
                            error
                        }),
                        current_depot_index: 0,
                        total_depots: 0,
                        progress_percent: None,
                        speed_mbps: None,
                        transferred_bytes: None,
                        total_bytes: None,
                        success: Some(false),
                    },
                );
            }
        }
    });
}

/// Start downloading game files directly via DepotDownloaderMod.
#[command]
pub async fn depot_downloader_start_download(
    app: AppHandle,
    appid: u32,
    folder_name: String,
    build_id: String,
    destination_dir: String,
    max_downloads: Option<u32>,
    verify_all: Option<bool>,
) -> Result<String, String> {
    validate_catalog_identity(appid, &folder_name)?;
    let max_concurrency = max_downloads.unwrap_or(64).clamp(1, 256);
    let do_verify = verify_all.unwrap_or(true);

    let job = ActiveDownloadState {
        appid,
        folder_name,
        build_id,
        destination_dir,
        max_downloads: max_concurrency,
        verify_all: do_verify,
        child_process_id: None,
        paused: false,
    };

    {
        let mut active = ACTIVE_DOWNLOAD.lock().map_err(|_| "Lock error")?;
        if active.is_some() {
            return Err("Đang có một tiến trình tải hoặc một phiên tạm dừng chưa hoàn tất.".to_string());
        }
        CANCEL_REQUESTED.store(false, Ordering::SeqCst);
        PAUSE_REQUESTED.store(false, Ordering::SeqCst);
        *active = Some(job.clone());
    }

    launch_download_task(app, job);
    Ok("Depot download started".to_string())
}

/// Pause by stopping the DepotDownloaderMod child process while preserving the
/// game files, .DepotDownloader/depot.config, staging, and cached manifests.
#[command]
pub fn depot_downloader_pause_download() -> Result<bool, String> {
    PAUSE_REQUESTED.store(true, Ordering::SeqCst);
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let mut active = ACTIVE_DOWNLOAD.lock().map_err(|_| "Lock error")?;
    let state = active.as_mut().ok_or_else(|| "Không có tiến trình DepotDownloader đang chạy.".to_string())?;
    if state.paused {
        return Ok(true);
    }
    if let Some(pid) = state.child_process_id {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .creation_flags(0x08000000)
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = pid;
        }
    }
    Ok(true)
}

/// Resume the exact paused job. DepotDownloaderMod re-opens its existing
/// .DepotDownloader state and verifies/reuses already-valid chunks.
#[command]
pub async fn depot_downloader_resume_download(app: AppHandle) -> Result<String, String> {
    let job = {
        let mut active = ACTIVE_DOWNLOAD.lock().map_err(|_| "Lock error")?;
        let state = active.as_mut().ok_or_else(|| "Không có phiên tải nào để tiếp tục.".to_string())?;
        if !state.paused {
            return Err("Tiến trình DepotDownloader chưa ở trạng thái tạm dừng.".to_string());
        }
        state.paused = false;
        state.child_process_id = None;
        state.clone()
    };

    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    PAUSE_REQUESTED.store(false, Ordering::SeqCst);
    let _ = app.emit(
        "depot-download-progress",
        DepotDownloadProgressEvent {
            event_type: "resumed".to_string(),
            appid: job.appid,
            build_id: job.build_id.clone(),
            depot_id: None,
            message: Some("Đang tiếp tục: kiểm tra dữ liệu đã có rồi chỉ tải các chunk còn thiếu/sai.".to_string()),
            current_depot_index: 0,
            total_depots: 0,
            progress_percent: None,
            speed_mbps: None,
            transferred_bytes: None,
            total_bytes: None,
            success: None,
        },
    );
    launch_download_task(app, job);
    Ok("Depot download resumed".to_string())
}

/// Read launcher-owned version metadata without touching DepotDownloader's own
/// compressed depot.config format.
#[command]
pub fn depot_downloader_get_install_state(appid: u32, destination_dir: String) -> Result<DepotInstallState, String> {
    Ok(read_install_state_path(Path::new(&destination_dir), appid))
}

fn run_download_pipeline(
    app: &AppHandle,
    appid: u32,
    folder_name: &str,
    build_id: &str,
    destination_dir: &str,
    max_concurrency: u32,
    do_verify: bool,
) -> Result<PipelineOutcome, String> {
    let exe = resolve_depot_downloader_exe()?;
    let client = build_client()?;
    let token = get_hf_token();

    // Create target dir
    let dest_path = PathBuf::from(destination_dir);
    fs::create_dir_all(&dest_path).map_err(|e| format!("Không thể tạo thư mục lưu game: {}", e))?;

    // Clean build ID
    let clean_build_id = build_id
        .strip_prefix("BuildID_")
        .or_else(|| build_id.strip_prefix("BuildId_"))
        .or_else(|| build_id.strip_prefix("buildid_"))
        .unwrap_or(build_id)
        .trim();

    let installed_state = read_install_state_path(&dest_path, appid);
    let is_version_switch = installed_state
        .installed_build_id
        .as_deref()
        .map(|current| current != clean_build_id)
        .unwrap_or(false);
    let effective_verify = do_verify || is_version_switch;

    // Keep target manifests beside the working copy so A -> B -> A can reuse
    // historical metadata without depending on the OS temp directory.
    let manifest_cache = manifest_cache_dir(&dest_path, clean_build_id);
    fs::create_dir_all(&manifest_cache)
        .map_err(|e| format!("Không thể tạo cache manifest theo BuildID: {e}"))?;

    if let Some(previous_build) = installed_state.installed_build_id.as_deref() {
        let _ = seed_previous_build_manifests(&dest_path, previous_build)?;
    }

    let start_message = if is_version_switch {
        format!(
            "Chuyển phiên bản {} → {}: giữ nguyên dữ liệu hiện có, .DepotDownloader/staging và bắt buộc verify chunk trước khi tải phần khác biệt.",
            installed_state.installed_build_id.as_deref().unwrap_or("unknown"),
            clean_build_id
        )
    } else {
        "Đang tải các file mã khóa (Key) và Manifest từ kho lưu trữ...".to_string()
    };

    let _ = app.emit(
        "depot-download-progress",
        DepotDownloadProgressEvent {
            event_type: "start".to_string(),
            appid,
            build_id: clean_build_id.to_string(),
            depot_id: None,
            message: Some(start_message),
            current_depot_index: 0,
            total_depots: 0,
            progress_percent: Some(0.0),
            speed_mbps: None,
            transferred_bytes: None,
            total_bytes: None,
            success: None,
        },
    );

    // Download .key
    let key_rel = format!("Depotdownloader/{}/{}/{}.key", folder_name, appid, appid);
    let key_local = manifest_cache.join(format!("{}.key", appid));
    if !key_local.is_file() {
        download_hf_raw_file(&client, &key_rel, &key_local, &token)?;
    }

    // Query manifests for this BuildID
    let build_rel = format!("Depotdownloader/{}/{}/BuildID_{}", folder_name, appid, clean_build_id);
    let build_url = format!("{}/{}", api_tree_base(), pct_encode(&build_rel));
    let b_resp = client
        .get(&build_url)
        .headers(auth_headers(&token))
        .send()
        .map_err(|e| format!("Lỗi kết nối đọc danh sách manifest: {}", e))?;

    if !b_resp.status().is_success() {
        let code = b_resp.status();
        if code.as_u16() == 404 {
            return Err(format!(
                "DEPOT_BUILD_NOT_FOUND: BuildID {} không còn tồn tại trong kho cho {} (AppID {}). Hãy làm mới danh mục, chọn lại game/build rồi tải lại.",
                clean_build_id, folder_name, appid
            ));
        }
        let txt = b_resp.text().unwrap_or_default();
        return Err(format!("Lỗi kết nối máy chủ kho (HTTP {}): {}", code, txt));
    }

    let b_nodes: Vec<HfNode> = b_resp
        .json()
        .map_err(|e| format!("Lỗi phân tích manifest nodes: {}", e))?;

    let mut manifests: Vec<DepotManifestInfo> = Vec::new();
    for file in &b_nodes {
        let fname = file.path.split('/').last().unwrap_or("").to_string();
        if fname.ends_with(".manifest") {
            let stem = fname.trim_end_matches(".manifest");
            if let Some(up) = stem.find('_') {
                let depot_str = &stem[..up];
                let gid_str = &stem[up + 1..];
                if let Ok(depot_id) = depot_str.parse::<u32>() {
                    manifests.push(DepotManifestInfo {
                        depot_id,
                        manifest_gid: gid_str.to_string(),
                        manifest_file: fname,
                    });
                }
            }
        }
    }

    if manifests.is_empty() {
        return Err(format!("Không tìm thấy tệp manifest nào cho BuildID {}", clean_build_id));
    }

    manifests.sort_by(|a, b| a.depot_id.cmp(&b.depot_id));
    let total_depots = manifests.len();

    // Iterate through each depot and run DepotDownloaderMod
    for (index, m_info) in manifests.iter().enumerate() {
        if PAUSE_REQUESTED.load(Ordering::SeqCst) {
            return Ok(PipelineOutcome::Paused);
        }
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            return Err("Cancelled by user".to_string());
        }

        // Persist each BuildID's manifest instead of leaving it in %TEMP%.
        let manifest_rel = format!("{}/{}", build_rel, m_info.manifest_file);
        let manifest_local = manifest_cache.join(&m_info.manifest_file);
        if !manifest_local.is_file() {
            download_hf_raw_file(&client, &manifest_rel, &manifest_local, &token)?;
        }
        // Pre-seed the fork's own cache with the target manifest and checksum.
        // This keeps UseManifestFile=false so `previousManifest` comes from the
        // actual InstalledManifestIDs entry and obsolete files can be deleted.
        seed_native_manifest_cache(&dest_path, &manifest_local)?;

        let _ = app.emit(
            "depot-download-progress",
            DepotDownloadProgressEvent {
                event_type: "depot-start".to_string(),
                appid,
                build_id: clean_build_id.to_string(),
                depot_id: Some(m_info.depot_id.to_string()),
                message: Some(format!(
                    "[{}/{}] Bắt đầu tải Depot {}...",
                    index + 1,
                    total_depots,
                    m_info.depot_id
                )),
                current_depot_index: index + 1,
                total_depots,
                progress_percent: Some(((index as f64) / (total_depots as f64)) * 100.0),
                speed_mbps: None,
                transferred_bytes: None,
                total_bytes: None,
                success: None,
            },
        );

        // Build command
        let mut cmd = Command::new(&exe);
        cmd.arg("-app")
            .arg(appid.to_string())
            .arg("-depot")
            .arg(m_info.depot_id.to_string())
            .arg("-manifest")
            .arg(&m_info.manifest_gid)
            .arg("-depotkeys")
            .arg(&key_local)
            .arg("-dir")
            .arg(destination_dir)
            .arg("-max-downloads")
            .arg(max_concurrency.to_string())
            .arg("-progress")
            .arg("line");

        if effective_verify {
            cmd.arg("-verify-all");
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Không thể khởi chạy DepotDownloaderMod: {}", e))?;

        // Record child PID for cancellation
        if let Ok(mut active) = ACTIVE_DOWNLOAD.lock() {
            if let Some(ref mut state) = *active {
                state.child_process_id = Some(child.id());
            }
        }

        // Stream stdout line-by-line
        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                if PAUSE_REQUESTED.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    return Ok(PipelineOutcome::Paused);
                }
                if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    return Err("Cancelled by user".to_string());
                }

                // Parse percentage from stdout lines (e.g. "12.34%" or "[12.34%]")
                let mut parsed_pct: Option<f64> = None;
                if let Some(pct_idx) = trimmed.find('%') {
                    let before = &trimmed[..pct_idx];
                    let num_str: String = before.chars().rev().take_while(|c| c.is_digit(10) || *c == '.').collect();
                    let num_str: String = num_str.chars().rev().collect();
                    if let Ok(pct) = num_str.parse::<f64>() {
                        // Blend overall progress
                        let depot_fraction = (index as f64) / (total_depots as f64);
                        let depot_weight = 1.0 / (total_depots as f64);
                        let total_pct = (depot_fraction + (pct / 100.0) * depot_weight) * 100.0;
                        parsed_pct = Some(total_pct.min(99.9));
                    }
                }

                let _ = app.emit(
                    "depot-download-progress",
                    DepotDownloadProgressEvent {
                        event_type: "log".to_string(),
                        appid,
                        build_id: clean_build_id.to_string(),
                        depot_id: Some(m_info.depot_id.to_string()),
                        message: Some(trimmed),
                        current_depot_index: index + 1,
                        total_depots,
                        progress_percent: parsed_pct,
                        speed_mbps: None,
                        transferred_bytes: None,
                        total_bytes: None,
                        success: None,
                    },
                );
            }
        }

        let status = child.wait().map_err(|e| format!("DepotDownloaderMod wait error: {}", e))?;
        if !status.success() {
            if PAUSE_REQUESTED.load(Ordering::SeqCst) {
                return Ok(PipelineOutcome::Paused);
            }
            if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                return Err("Cancelled by user".to_string());
            }
            return Err(format!("Lỗi trong quá trình tải depot {}: Mã thoát {:?}", m_info.depot_id, status.code()));
        }

        let _ = app.emit(
            "depot-download-progress",
            DepotDownloadProgressEvent {
                event_type: "depot-done".to_string(),
                appid,
                build_id: clean_build_id.to_string(),
                depot_id: Some(m_info.depot_id.to_string()),
                message: Some(format!("Depot {} hoàn tất.", m_info.depot_id)),
                current_depot_index: index + 1,
                total_depots,
                progress_percent: Some(((index + 1) as f64 / (total_depots as f64)) * 100.0),
                speed_mbps: None,
                transferred_bytes: None,
                total_bytes: None,
                success: Some(true),
            },
        );
    }

    write_install_state(&dest_path, appid, clean_build_id, &manifests)?;
    Ok(PipelineOutcome::Completed)
}

/// Cancel the current active depot download task.
#[command]
pub fn depot_downloader_cancel_download() -> Result<bool, String> {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    PAUSE_REQUESTED.store(false, Ordering::SeqCst);

    let mut clear_immediately = false;
    if let Ok(mut active) = ACTIVE_DOWNLOAD.lock() {
        if let Some(ref mut state) = *active {
            if state.paused && state.child_process_id.is_none() {
                clear_immediately = true;
            }
            if let Some(pid) = state.child_process_id {
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .creation_flags(0x08000000)
                        .status();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = pid;
                }
            }
        }
        if clear_immediately {
            *active = None;
        }
    }
    Ok(true)
}

/// Get current download task status.
#[command]
pub fn depot_downloader_get_status() -> Result<DepotDownloaderStatus, String> {
    let active = ACTIVE_DOWNLOAD.lock().map_err(|_| "Lock error")?;
    if let Some(ref state) = *active {
        Ok(DepotDownloaderStatus {
            is_downloading: !state.paused,
            is_paused: state.paused,
            can_resume: state.paused,
            active_appid: Some(state.appid),
            active_build_id: Some(state.build_id.clone()),
            destination_dir: Some(state.destination_dir.clone()),
        })
    } else {
        Ok(DepotDownloaderStatus {
            is_downloading: false,
            is_paused: false,
            can_resume: false,
            active_appid: None,
            active_build_id: None,
            destination_dir: None,
        })
    }
}
