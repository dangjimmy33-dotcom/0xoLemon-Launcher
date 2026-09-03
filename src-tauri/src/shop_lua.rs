// shop_lua.rs — 0xoLemon Lua Shop backend
// Fetches the curated Lua catalog from HuggingFace and keeps the legacy
// Depotdownloader tree for build/manifest metadata, downloads manifests, and writes
// properly-formatted Lua scripts for the SteamPlugin system.

use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ─── HuggingFace API helpers ──────────────────────────────────────────────────

/// Load the HuggingFace access token from the embedded config.
pub(crate) fn get_hf_token() -> String {
    let json_str = include_str!("../huggingface-repos.json");
    let config: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
    config["repositories"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|r| r["repoId"].as_str() == Some("Immaking/Luas"))
        .and_then(|r| r["token"].as_str())
        .unwrap_or("")
        .to_string()
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| "Service temporarily unavailable".to_string())
}

fn build_client_long() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|_| "Service temporarily unavailable".to_string())
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

/// Base URL for HuggingFace tree API (directory listings).
fn api_tree_base() -> String {
    "https://huggingface.co/api/datasets/Immaking/Luas/tree/main".to_string()
}

/// Base URL for HuggingFace raw file downloads.
fn raw_base() -> String {
    "https://huggingface.co/datasets/Immaking/Luas/resolve/main".to_string()
}

/// Minimal percent-encoding for HuggingFace URL path segments.
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

// ─── Types ────────────────────────────────────────────────────────────────────

/// A single HuggingFace tree entry (file or directory node).
#[derive(Deserialize, Debug)]
struct HfNode {
    #[serde(rename = "type")]
    node_type: String,
    path: String,
}

/// A game available from the curated Lua catalog.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShopGame {
    pub name: String,
    pub appid: u32,
}

/// A single manifest entry: depot ID + manifest GID.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManifestEntry {
    pub depot_id: u32,
    pub manifest_gid: String,
}

/// A specific build snapshot of a game.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuildInfo {
    /// Raw numeric build ID (e.g. "23430993")
    pub build_id: String,
    /// Content of version.txt if present
    pub version: Option<String>,
    /// Date timestamp string if available
    pub build_date: Option<String>,
    /// All manifest files found in this build folder
    pub manifests: Vec<ManifestEntry>,
}

/// Full detail returned when the user opens a game card.
#[derive(Serialize, Deserialize, Debug)]
pub struct GameBuildsInfo {
    /// Builds sorted newest-first by build ID
    pub builds: Vec<BuildInfo>,
    /// Whether a depot key file (.key) exists for this game
    pub has_key: bool,
}

// ─── Commands ─────────────────────────────────────────────────────────────────

const PATCH_RSS_TTL: Duration = Duration::from_secs(6 * 60 * 60);
static PATCH_RSS_CACHE: Lazy<Mutex<HashMap<u32, (Instant, String)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static CATALOG_FOLDER_CACHE: Lazy<Mutex<HashMap<u32, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Fetch SteamDB Patchnotes RSS for one AppID.
/// The feed is optional metadata only and is cached aggressively because SteamDB
/// explicitly documents it as heavily cached and not intended for realtime polling.
#[tauri::command]
pub async fn lua_shop_get_patchnotes_rss(appid: u32) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || patchnotes_rss_blocking(appid))
        .await
        .map_err(|_| "Patch history service unavailable".to_string())?
}

fn patchnotes_rss_blocking(appid: u32) -> Result<String, String> {
    if let Ok(cache) = PATCH_RSS_CACHE.lock() {
        if let Some((fetched_at, xml)) = cache.get(&appid) {
            if fetched_at.elapsed() < PATCH_RSS_TTL {
                return Ok(xml.clone());
            }
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("0xoLemonLauncher/1.0 LuaShopPatchHistory")
        .build()
        .map_err(|_| "Patch history service unavailable".to_string())?;
    let url = format!("https://steamdb.info/api/PatchnotesRSS/?appid={}", appid);
    let response = client
        .get(url)
        .send()
        .map_err(|_| "Patch history service unavailable".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Patch history unavailable (HTTP {})",
            response.status()
        ));
    }
    let xml = response
        .text()
        .map_err(|_| "Patch history response was invalid".to_string())?;
    if !xml.contains("<rss") && !xml.contains("<feed") {
        return Err("Patch history response was invalid".to_string());
    }

    if let Ok(mut cache) = PATCH_RSS_CACHE.lock() {
        cache.insert(appid, (Instant::now(), xml.clone()));
    }
    Ok(xml)
}

/// Return the list of all games available in the curated flat Lua catalog.
#[tauri::command]
pub async fn lua_shop_get_catalog() -> Result<Vec<ShopGame>, String> {
    tauri::async_runtime::spawn_blocking(catalog_blocking)
        .await
        .map_err(|_| "Service temporarily unavailable".to_string())?
}

fn resolve_catalog_folder_name(
    client: &Client,
    token: &str,
    appid: u32,
    fallback_game_name: &str,
) -> Result<Option<String>, String> {
    if let Ok(cache) = CATALOG_FOLDER_CACHE.lock() {
        if let Some(folder) = cache.get(&appid) {
            return Ok(Some(folder.clone()));
        }
    }

    let url = format!("{}/Depotdownloader", api_tree_base());
    let response = client
        .get(&url)
        .headers(auth_headers(token))
        .send()
        .map_err(|_| "Service temporarily unavailable".to_string())?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let nodes: Vec<HfNode> = response
        .json()
        .map_err(|_| "Service temporarily unavailable".to_string())?;

    let suffix = format!("({})", appid);
    for node in nodes {
        if node.node_type != "directory" {
            continue;
        }
        let folder = node.path.split('/').last().unwrap_or("");
        if folder.ends_with(&suffix) {
            let folder = folder.to_string();
            if let Ok(mut cache) = CATALOG_FOLDER_CACHE.lock() {
                cache.insert(appid, folder.clone());
            }
            return Ok(Some(folder));
        }
    }

    if fallback_game_name.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!("{} ({})", fallback_game_name.trim(), appid)))
    }
}

fn flat_lua_catalog_from_nodes(nodes: Vec<HfNode>) -> Vec<ShopGame> {
    let mut appids = BTreeSet::new();

    for node in nodes {
        if node.node_type != "file" {
            continue;
        }
        let filename = node.path.rsplit('/').next().unwrap_or("");
        let Some(stem) = filename.strip_suffix(".lua") else {
            continue;
        };
        let Ok(appid) = stem.parse::<u32>() else {
            continue;
        };
        if appid > 0 {
            appids.insert(appid);
        }
    }

    appids
        .into_iter()
        .map(|appid| ShopGame {
            // The card resolves the real Steam title lazily from AppID. Keeping
            // the fallback catalog independent of Depotdownloader prevents the
            // browse list from being truncated to the small legacy folder set.
            name: format!("AppID {}", appid),
            appid,
        })
        .collect()
}

pub(crate) fn catalog_blocking() -> Result<Vec<ShopGame>, String> {
    let token = get_hf_token();
    let client = build_client()?;
    // The actual curated source is the flat lua/<appid>.lua directory. The old
    // code listed Depotdownloader/ instead, so a backend fallback exposed only
    // the handful of legacy game folders even though hundreds of Lua files
    // were available. 1000 comfortably covers the current catalog and matches
    // Hugging Face's non-expanded tree page size.
    let url = format!("{}/lua?limit=1000", api_tree_base());

    let resp = client
        .get(&url)
        .headers(auth_headers(&token))
        .send()
        .map_err(|_| "Service temporarily unavailable".to_string())?;

    if !resp.status().is_success() {
        return Err("Service temporarily unavailable".to_string());
    }

    let nodes: Vec<HfNode> = resp
        .json()
        .map_err(|_| "Service temporarily unavailable".to_string())?;
    let games = flat_lua_catalog_from_nodes(nodes);
    if games.is_empty() {
        return Err("Lua catalog is temporarily unavailable".to_string());
    }
    Ok(games)
}

/// Return the available builds and depot-key presence for a given game.
#[tauri::command]
pub async fn lua_shop_get_game_builds(
    appid: u32,
    game_name: String,
) -> Result<GameBuildsInfo, String> {
    tauri::async_runtime::spawn_blocking(move || builds_blocking(appid, &game_name))
        .await
        .map_err(|_| "Service temporarily unavailable".to_string())?
}

fn builds_blocking(appid: u32, game_name: &str) -> Result<GameBuildsInfo, String> {
    let token = get_hf_token();
    let client = build_client()?;

    // Resolve by AppID first so Steam display-name differences do not break version lookup.
    let Some(folder_name) = resolve_catalog_folder_name(&client, &token, appid, game_name)? else {
        return Ok(GameBuildsInfo {
            builds: Vec::new(),
            has_key: false,
        });
    };
    let rel_path = format!("Depotdownloader/{}/{}", folder_name, appid);
    let url = format!("{}/{}", api_tree_base(), pct_encode(&rel_path));

    let resp = client
        .get(&url)
        .headers(auth_headers(&token))
        .send()
        .map_err(|_| "Service temporarily unavailable".to_string())?;

    if !resp.status().is_success() {
        return Ok(GameBuildsInfo {
            builds: Vec::new(),
            has_key: false,
        });
    }

    let nodes: Vec<HfNode> = resp
        .json()
        .map_err(|_| "Service temporarily unavailable".to_string())?;

    let mut raw_build_ids: Vec<String> = Vec::new();
    let mut has_key = false;

    for node in &nodes {
        let leaf = node.path.split('/').last().unwrap_or("");
        if node.node_type == "directory" && leaf.starts_with("BuildID_") {
            raw_build_ids.push(leaf.trim_start_matches("BuildID_").to_string());
        } else if node.node_type == "file" && leaf.ends_with(".key") {
            has_key = true;
        }
    }

    // Sort numerically descending so the newest build comes first
    raw_build_ids.sort_by(|a, b| {
        b.parse::<u64>()
            .unwrap_or(0)
            .cmp(&a.parse::<u64>().unwrap_or(0))
    });

    // For each BuildID, list the manifest files (and optionally version.txt)
    let mut builds = Vec::new();
    for bid in &raw_build_ids {
        let build_rel = format!("Depotdownloader/{}/{}/BuildID_{}", folder_name, appid, bid);
        let build_url = format!("{}/{}", api_tree_base(), pct_encode(&build_rel));

        let build_resp = client.get(&build_url).headers(auth_headers(&token)).send();

        let mut manifests: Vec<ManifestEntry> = Vec::new();
        let mut version: Option<String> = None;

        if let Ok(r) = build_resp {
            if r.status().is_success() {
                if let Ok(build_nodes) = r.json::<Vec<HfNode>>() {
                    for file in &build_nodes {
                        let fname = file.path.split('/').last().unwrap_or("");

                        if fname.ends_with(".manifest") {
                            // Name format:  <depotId>_<manifestGID>.manifest
                            let stem = fname.trim_end_matches(".manifest");
                            if let Some(up) = stem.find('_') {
                                let depot_str = &stem[..up];
                                let gid_str = &stem[up + 1..];
                                if let Ok(depot_id) = depot_str.parse::<u32>() {
                                    manifests.push(ManifestEntry {
                                        depot_id,
                                        manifest_gid: gid_str.to_string(),
                                    });
                                }
                            }
                        } else if fname == "version.txt" {
                            let ver_rel = format!(
                                "Depotdownloader/{}/{}/BuildID_{}/version.txt",
                                folder_name, appid, bid
                            );
                            let ver_url = format!("{}/{}", raw_base(), pct_encode(&ver_rel));
                            if let Ok(vr) =
                                client.get(&ver_url).headers(auth_headers(&token)).send()
                            {
                                if vr.status().is_success() {
                                    version = vr.text().ok().map(|t| t.trim().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        builds.push(BuildInfo {
            build_id: bid.clone(),
            version,
            build_date: None,
            manifests,
        });
    }

    // Attempt to fetch real build dates from SteamCMD API
    if let Ok(steamcmd_resp) = client
        .get(format!("https://api.steamcmd.net/v1/info/{}", appid))
        .send()
    {
        if steamcmd_resp.status().is_success() {
            if let Ok(json) = steamcmd_resp.json::<serde_json::Value>() {
                if let Some(branches) = json
                    .get("data")
                    .and_then(|d| d.get(appid.to_string()))
                    .and_then(|a| a.get("depots"))
                    .and_then(|d| d.get("branches"))
                    .and_then(|b| b.as_object())
                {
                    let mut date_map = std::collections::HashMap::new();
                    for (_branch_name, branch_data) in branches {
                        if let Some(bid) = branch_data.get("buildid").and_then(|v| v.as_str()) {
                            if let Some(tupdate) =
                                branch_data.get("timeupdated").and_then(|v| v.as_str())
                            {
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

    Ok(GameBuildsInfo { builds, has_key })
}

/// Install a game: download manifests to depotcache/ and write the Lua config.
#[tauri::command]
pub async fn lua_shop_install_game(
    app: tauri::AppHandle,
    appid: u32,
    game_name: String,
    build_id: String,
    access_token: Option<String>,
    stat_steam_id: Option<String>,
    skip_manifest_pin: Option<bool>,
) -> Result<(), String> {
    let live = skip_manifest_pin.unwrap_or(false);
    crate::lua_live::install_lua_game(
        app,
        crate::lua_live::InstallLuaGameRequest {
            appid,
            game_name,
            channel: if live {
                crate::lua_live::LuaGameChannel::Live
            } else {
                crate::lua_live::LuaGameChannel::Locked
            },
            build_id: if live { None } else { Some(build_id) },
            access_token,
            stat_steam_id,
            conflict_resolution: None,
            provider: None,
            request_id: None,
            timezone: None,
        },
    )
    .await
    .map(|_| ())
}

fn fetch_text_rel(client: &Client, token: &str, rel: &str) -> Option<String> {
    let url = format!("{}/{}", raw_base(), pct_encode(rel));
    let response = client.get(&url).headers(auth_headers(token)).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().ok()?;
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn choose_lua_base(
    client: &Client,
    token: &str,
    stplug_in_dir: &Path,
    folder_name: &str,
    appid: u32,
) -> String {
    let local_path = stplug_in_dir.join(format!("{}.lua", appid));
    let local = std::fs::read_to_string(&local_path)
        .ok()
        .filter(|v| !v.trim().is_empty());
    let canonical = fetch_text_rel(client, token, &format!("lua/{}.lua", appid));

    // Repair files produced by older launcher versions that regenerated a tiny
    // script from scratch instead of preserving the canonical Lua. Rich local
    // files still win because they may contain user-specific options/tickets.
    if let Some(local_text) = local {
        let looks_generated = local_text.contains("-- Generated by 0xoLemon Launcher");
        if !looks_generated {
            return local_text;
        }
        if let Some(canonical_text) = canonical.clone() {
            return canonical_text;
        }
        return local_text;
    }

    if let Some(canonical_text) = canonical {
        return canonical_text;
    }

    fetch_text_rel(
        client,
        token,
        &format!("Depotdownloader/{}/{}/{}.lua", folder_name, appid, appid),
    )
    .unwrap_or_default()
}

fn append_lua_line(content: &mut String, line: &str) {
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(line);
    content.push('\n');
}

fn patch_manifest_bindings(
    base: &str,
    manifest_entries: &[(u32, String, String)],
) -> Result<String, String> {
    let exact = manifest_entries
        .iter()
        .map(|(depot_id, manifest_gid, _)| (*depot_id, manifest_gid.clone()))
        .collect::<Vec<_>>();
    crate::lua_live::replace_manifest_bindings_exact(base, &exact)
}

fn has_lua_app_call(content: &str, function_name: &str, appid: u32) -> bool {
    let pattern = format!(r#"{}\s*\(\s*{}\s*,"#, regex::escape(function_name), appid);
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(content))
        .unwrap_or(false)
}

fn lua_string_literal(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn upsert_lua_string_call(content: &mut String, function_name: &str, appid: u32, value: &str) {
    let pattern = format!(
        r#"({}\s*\(\s*{}\s*,\s*)[\"'][^\"']*[\"']"#,
        regex::escape(function_name),
        appid
    );
    if let Ok(re) = regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
    {
        if re.is_match(content) {
            let literal = lua_string_literal(value);
            *content = re
                .replace_all(content, |captures: &regex::Captures<'_>| {
                    format!("{}{}", &captures[1], literal)
                })
                .to_string();
            return;
        }
    }
    append_lua_line(
        content,
        &format!(
            "{}({}, {})",
            function_name,
            appid,
            lua_string_literal(value)
        ),
    );
}

const MAX_LOCKED_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const LOCKED_SOURCE_LABEL: &str = "Hugging Face curated (Immaking/Luas)";

#[derive(Debug)]
struct VerifiedLockedManifest {
    depot_id: u32,
    manifest_gid: String,
    file_name: String,
    bytes: Vec<u8>,
}

enum FileMutation {
    Write { path: PathBuf, bytes: Vec<u8> },
    Delete { path: PathBuf },
}

impl FileMutation {
    fn path(&self) -> &Path {
        match self {
            Self::Write { path, .. } | Self::Delete { path } => path,
        }
    }
}

struct FileSnapshot {
    path: PathBuf,
    prior: Option<Vec<u8>>,
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not snapshot {}: {error}", path.display())),
    }
}

fn restore_snapshot(snapshot: &FileSnapshot) -> Result<(), String> {
    if let Some(bytes) = snapshot.prior.as_deref() {
        crate::lua_live::atomic_write_path(&snapshot.path, bytes)
    } else {
        match std::fs::remove_file(&snapshot.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Could not remove transaction output {}: {error}",
                snapshot.path.display()
            )),
        }
    }
}

fn apply_file_transaction(mutations: Vec<FileMutation>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for mutation in &mutations {
        if !seen.insert(mutation.path().to_path_buf()) {
            return Err(format!(
                "Locked version transaction contains duplicate path {}",
                mutation.path().display()
            ));
        }
    }

    // Snapshot every destination before the first write. If any path cannot be
    // read, no mutation starts and the currently active Steam payload remains intact.
    let snapshots = mutations
        .iter()
        .map(|mutation| {
            Ok(FileSnapshot {
                path: mutation.path().to_path_buf(),
                prior: read_optional_file(mutation.path())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut applied = 0usize;
    for mutation in &mutations {
        let result = match mutation {
            FileMutation::Write { path, bytes } => crate::lua_live::atomic_write_path(path, bytes),
            FileMutation::Delete { path } => match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!(
                    "Could not remove stale depot manifest {}: {error}",
                    path.display()
                )),
            },
        };
        if let Err(error) = result {
            let rollback_errors = snapshots[..applied]
                .iter()
                .rev()
                .filter_map(|snapshot| restore_snapshot(snapshot).err())
                .collect::<Vec<_>>();
            if rollback_errors.is_empty() {
                return Err(error);
            }
            return Err(format!(
                "{error}; rollback also reported: {}",
                rollback_errors.join(" | ")
            ));
        }
        applied += 1;
    }
    Ok(())
}

fn download_verified_manifest(
    client: &Client,
    token: &str,
    depot_id: u32,
    manifest_gid: &str,
    raw_path: &str,
) -> Result<VerifiedLockedManifest, String> {
    let file_name = format!("{depot_id}_{manifest_gid}.manifest");
    let manifest_url = format!("{}/{}", raw_base(), pct_encode(raw_path));
    let response = client
        .get(&manifest_url)
        .headers(auth_headers(token))
        .send()
        .map_err(|error| format!("Could not download {file_name}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Could not download {file_name}: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_LOCKED_MANIFEST_BYTES)
    {
        return Err(format!("Manifest {file_name} exceeds the safety limit"));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_LOCKED_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {file_name}: {error}"))?;
    if bytes.len() as u64 > MAX_LOCKED_MANIFEST_BYTES {
        return Err(format!("Manifest {file_name} exceeds the safety limit"));
    }
    crate::lua_sources::validate_manifest_magic(&bytes)
        .map_err(|error| format!("Manifest {file_name} is invalid: {error}"))?;
    Ok(VerifiedLockedManifest {
        depot_id,
        manifest_gid: manifest_gid.to_string(),
        file_name,
        bytes,
    })
}

fn upsert_depot_key_call(content: &mut String, depot_id: u32, key: &str) {
    let literal = lua_string_literal(key);
    let keyed_pattern = format!(
        r#"(?im)^([\t ]*addappid[\t ]*\([\t ]*{}[\t ]*,[\t ]*\d+[\t ]*,[\t ]*)[\"'][^\"'\r\n]*[\"']([\t ]*\)[\t ]*;?)"#,
        depot_id
    );
    if let Ok(keyed) = regex::Regex::new(&keyed_pattern) {
        if keyed.is_match(content) {
            *content = keyed
                .replace(content, |captures: &regex::Captures<'_>| {
                    format!("{}{}{}", &captures[1], literal, &captures[2])
                })
                .to_string();
            return;
        }
    }

    let existing_pattern = format!(
        r#"(?im)^([\t ]*)addappid[\t ]*\([\t ]*{}(?:[^)\r\n]*)\)([\t ]*;?)"#,
        depot_id
    );
    if let Ok(existing) = regex::Regex::new(&existing_pattern) {
        if existing.is_match(content) {
            *content = existing
                .replace(content, |captures: &regex::Captures<'_>| {
                    format!(
                        "{}addappid({}, 0, {}){}",
                        &captures[1], depot_id, literal, &captures[2]
                    )
                })
                .to_string();
            return;
        }
    }
    append_lua_line(content, &format!("addappid({depot_id}, 0, {literal})"));
}

fn selected_source_metadata(source: &str, build_id: &str) -> Result<String, String> {
    let legacy =
        regex::Regex::new(r"(?im)^[\t ]*--[\t ]*Generated by 0xoLemon Launcher[\t ]*(?:\r?\n|$)")
            .map_err(|_| "Could not prepare legacy Lua metadata cleanup".to_string())?;
    let selected = regex::Regex::new(
        r"(?im)^[\t ]*--[\t ]*Selected[\t ]+(?:source|BuildID)[\t ]*:[^\r\n]*(?:\r?\n|$)",
    )
    .map_err(|_| "Could not prepare Lua source metadata".to_string())?;
    let without_legacy = legacy.replace_all(source, "").into_owned();
    let cleaned = selected.replace_all(&without_legacy, "").into_owned();
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = format!(
        "-- Selected source: {LOCKED_SOURCE_LABEL}{newline}-- Selected BuildID: {build_id}{newline}"
    );
    let cleaned = cleaned.strip_prefix('\u{feff}').unwrap_or(&cleaned);
    output.push_str(cleaned.trim_start_matches(|value| value == '\r' || value == '\n'));
    if !output.ends_with('\n') {
        output.push_str(newline);
    }
    Ok(output)
}

fn sync_state_bytes(stplug_in_dir: &Path, pending_lua_name: &str) -> Vec<u8> {
    let mut names = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(stplug_in_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".lua") {
                names.insert(name);
            }
        }
    }
    names.insert(pending_lua_name.to_string());
    if names.is_empty() {
        Vec::new()
    } else {
        (names.into_iter().collect::<Vec<_>>().join("\n") + "\n").into_bytes()
    }
}

fn manifests_referenced_by_other_lua_files(
    stplug_in_dir: &Path,
    current_lua: &Path,
) -> Option<BTreeSet<String>> {
    let mut referenced = BTreeSet::new();
    let entries = std::fs::read_dir(stplug_in_dir).ok()?;
    for entry in entries {
        let path = entry.ok()?.path();
        if path == current_lua
            || !path.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("lua")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).ok()?;
        referenced.extend(crate::lua_live::manifest_refs_from_lua(&source).ok()?);
    }
    Some(referenced)
}

fn stale_unreferenced_manifest_paths(
    depotcache_dir: &Path,
    old_refs: &BTreeSet<String>,
    target_refs: &BTreeSet<String>,
    other_refs: Option<&BTreeSet<String>>,
) -> Vec<PathBuf> {
    let Some(other_refs) = other_refs else {
        // If another Lua file could not be inspected, retain the cache. A
        // stale file costs little; deleting a shared manifest would be worse.
        return Vec::new();
    };
    old_refs
        .difference(target_refs)
        .filter(|name| !other_refs.contains(*name))
        .map(|name| depotcache_dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

pub(crate) fn install_locked_game_blocking(
    appid: u32,
    game_name: &str,
    build_id: &str,
    access_token: Option<&str>,
    stat_steam_id: Option<&str>,
) -> Result<(), String> {
    if build_id.is_empty() || !build_id.chars().all(|value| value.is_ascii_digit()) {
        return Err("BuildID must contain decimal digits only".to_string());
    }
    let steam_path = crate::steam::get_steam_path().ok_or("Steam installation not found")?;
    let token = get_hf_token();
    let client = build_client_long()?;

    let stplug_in_dir = steam_path.join("config").join("stplug-in");
    let depotcache_dir = steam_path.join("depotcache");
    std::fs::create_dir_all(&stplug_in_dir)
        .map_err(|_| "Failed to prepare directories".to_string())?;
    std::fs::create_dir_all(&depotcache_dir)
        .map_err(|_| "Failed to prepare directories".to_string())?;

    let folder_name = resolve_catalog_folder_name(&client, &token, appid, game_name)?
        .ok_or_else(|| format!("No exact-version source is configured for AppID {}", appid))?;

    // ── 1. Fetch depot keys from the primary custom source ────────────────────────────────
    let key_rel = format!("Depotdownloader/{}/{}/{}.key", folder_name, appid, appid);
    let key_url = format!("{}/{}", raw_base(), pct_encode(&key_rel));
    let mut depot_keys = BTreeMap::<u32, String>::new();

    if let Ok(r) = client.get(&key_url).headers(auth_headers(&token)).send() {
        if r.status().is_success() {
            if let Ok(text) = r.text() {
                for line in text.lines() {
                    let parts: Vec<&str> = if line.contains(';') {
                        line.trim().split(';').collect()
                    } else {
                        line.trim().split_whitespace().collect()
                    };
                    if parts.len() >= 2 {
                        if let Ok(depot_id) = parts[0].parse::<u32>() {
                            let key = parts[1].trim();
                            if depot_id > 0 && !key.is_empty() && key.len() <= 512 {
                                depot_keys.insert(depot_id, key.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 2. List manifest files in the requested BuildID on HuggingFace ───────
    let build_rel = format!(
        "Depotdownloader/{}/{}/BuildID_{}",
        folder_name, appid, build_id
    );
    let build_url = format!("{}/{}", api_tree_base(), pct_encode(&build_rel));

    let build_resp = client
        .get(&build_url)
        .headers(auth_headers(&token))
        .send()
        .map_err(|error| format!("Could not inspect exact BuildID {build_id}: {error}"))?;
    if !build_resp.status().is_success() {
        return Err(format!(
            "Build {build_id} is known, but its exact depot manifest set is unavailable (HTTP {})",
            build_resp.status()
        ));
    }

    let nodes = build_resp
        .json::<Vec<HfNode>>()
        .map_err(|error| format!("BuildID {build_id} returned invalid metadata: {error}"))?;
    let mut manifest_map = BTreeMap::<u32, (String, String)>::new();
    for node in nodes {
        if node.node_type != "file" {
            continue;
        }
        let file_name = node.path.rsplit('/').next().unwrap_or_default();
        let Some(stem) = file_name.strip_suffix(".manifest") else {
            continue;
        };
        let Some((depot_text, gid)) = stem.split_once('_') else {
            return Err(format!(
                "BuildID {build_id} contains invalid manifest name {file_name}"
            ));
        };
        let depot_id = depot_text
            .parse::<u32>()
            .map_err(|_| format!("BuildID {build_id} contains invalid depot ID in {file_name}"))?;
        if depot_id == 0
            || gid.is_empty()
            || !gid.chars().all(|value| value.is_ascii_digit())
            || gid.parse::<u64>().is_err()
        {
            return Err(format!(
                "BuildID {build_id} contains invalid manifest GID in {file_name}"
            ));
        }
        if let Some((previous_gid, _)) =
            manifest_map.insert(depot_id, (gid.to_string(), node.path.clone()))
        {
            if previous_gid != gid {
                return Err(format!(
                    "BuildID {build_id} contains more than one manifest for depot {depot_id}"
                ));
            }
        }
    }
    if manifest_map.is_empty() {
        return Err(format!(
            "Build {build_id} contains no exact depot manifests"
        ));
    }

    let manifest_entries = manifest_map
        .iter()
        .map(|(depot_id, (gid, path))| (*depot_id, gid.clone(), path.clone()))
        .collect::<Vec<_>>();

    // Download and validate the complete target set before touching Steam.
    // Runtime request-code fallback remains available after commit, but an
    // explicit historical BuildID never commits a partial local package.
    let verified_manifests = manifest_entries
        .iter()
        .map(|(depot_id, manifest_gid, raw_path)| {
            download_verified_manifest(&client, &token, *depot_id, manifest_gid, raw_path)
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Preserve provider metadata and all non-version functionality. Exact
    // version ownership is rebuilt below rather than patched incrementally.
    let lua_file = stplug_in_dir.join(format!("{appid}.lua"));
    let active_lua = std::fs::read_to_string(&lua_file).unwrap_or_default();
    let old_manifest_refs = crate::lua_live::manifest_refs_from_lua(&active_lua)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let original_lua = choose_lua_base(&client, &token, &stplug_in_dir, &folder_name, appid);

    let hf_token_rel = format!("Depotdownloader/{}/{}/{}.token", folder_name, appid, appid);
    let downloaded_token = fetch_text_rel(&client, &token, &hf_token_rel)
        .map(|text| text.trim().to_string())
        .unwrap_or_default();

    let mut final_lua = if original_lua.trim().is_empty() {
        let mut minimal = String::new();
        let game_label = game_name.replace('\r', " ").replace('\n', " ");
        append_lua_line(
            &mut minimal,
            &format!("-- Game: {} | AppID: {}", game_label.trim(), appid),
        );
        append_lua_line(&mut minimal, &format!("addappid({})", appid));
        minimal
    } else {
        original_lua
    };

    for (depot_id, key) in &depot_keys {
        upsert_depot_key_call(&mut final_lua, *depot_id, key);
    }
    final_lua = patch_manifest_bindings(&final_lua, &manifest_entries)?;

    // Keep unrelated Lua content byte-for-byte as much as possible. Only update
    // optional token/stat calls when the user explicitly supplied them. The
    // repository token is used only when the base Lua does not already define one.
    if let Some(tok) = access_token.filter(|tok| !tok.is_empty()) {
        upsert_lua_string_call(&mut final_lua, "addtoken", appid, tok);
    } else if !downloaded_token.is_empty() && !has_lua_app_call(&final_lua, "addtoken", appid) {
        append_lua_line(
            &mut final_lua,
            &format!(r#"addtoken({}, "{}")"#, appid, downloaded_token),
        );
    }

    if let Some(sid) = stat_steam_id.filter(|sid| !sid.is_empty()) {
        upsert_lua_string_call(&mut final_lua, "setStat", appid, sid);
    }
    final_lua = selected_source_metadata(&final_lua, build_id)?;

    let target_refs = verified_manifests
        .iter()
        .map(|manifest| manifest.file_name.clone())
        .collect::<BTreeSet<_>>();
    let other_refs = manifests_referenced_by_other_lua_files(&stplug_in_dir, &lua_file);
    let stale_paths = stale_unreferenced_manifest_paths(
        &depotcache_dir,
        &old_manifest_refs,
        &target_refs,
        other_refs.as_ref(),
    );

    let mut mutations = verified_manifests
        .into_iter()
        .map(|manifest| {
            debug_assert_eq!(
                manifest.file_name,
                format!("{}_{}.manifest", manifest.depot_id, manifest.manifest_gid)
            );
            FileMutation::Write {
                path: depotcache_dir.join(manifest.file_name),
                bytes: manifest.bytes,
            }
        })
        .collect::<Vec<_>>();
    mutations.push(FileMutation::Write {
        path: lua_file.clone(),
        bytes: final_lua.into_bytes(),
    });
    mutations.push(FileMutation::Write {
        path: stplug_in_dir.join(".sync_state"),
        bytes: sync_state_bytes(&stplug_in_dir, &format!("{appid}.lua")),
    });
    mutations.extend(
        stale_paths
            .into_iter()
            .map(|path| FileMutation::Delete { path }),
    );

    apply_file_transaction(mutations)?;

    Ok(())
}

#[cfg(test)]
mod lua_patch_tests {
    use super::*;

    #[test]
    fn flat_lua_catalog_reads_all_curated_files_instead_of_depotdownloader_dirs() {
        let mut nodes = (1..=302)
            .map(|index| HfNode {
                node_type: "file".to_string(),
                path: format!("lua/{}.lua", 1_000_000 + index),
            })
            .collect::<Vec<_>>();
        nodes.push(HfNode {
            node_type: "file".to_string(),
            path: "lua/README.md".to_string(),
        });
        nodes.push(HfNode {
            node_type: "directory".to_string(),
            path: "lua/9999999.lua".to_string(),
        });
        nodes.push(HfNode {
            node_type: "file".to_string(),
            path: "lua/1000001.lua".to_string(),
        });

        let games = flat_lua_catalog_from_nodes(nodes);

        assert_eq!(games.len(), 302);
        assert_eq!(games.first().map(|game| game.appid), Some(1_000_001));
        assert_eq!(games.last().map(|game| game.appid), Some(1_000_302));
    }

    #[test]
    fn rebuilds_exact_manifest_set_and_preserves_non_version_content() {
        let base = "-- setManifestid(999, \"comment\")\naddappid(2840770)\nSETManifestid(2840771, \"111\", 123)\nsetManifestid(2840772,\n  \"222\")\nskipManifestPin(2840772)\nlocal sample = 'setManifestid(998, \"string\")'\nsetStat(2840770, \"7656\")\ncustomThing(\"keep me\")\n";
        let manifests = vec![(2840773, "333".to_string(), "x".to_string())];
        let patched = patch_manifest_bindings(base, &manifests).unwrap();
        assert!(patched.contains("setManifestid(2840773, \"333\")"));
        assert!(patched.contains("setStat(2840770, \"7656\")"));
        assert!(patched.contains("customThing(\"keep me\")"));
        assert!(patched.contains("-- setManifestid(999, \"comment\")"));
        assert!(patched.contains("'setManifestid(998, \"string\")'"));
        assert!(!patched.contains("SETManifestid(2840771"));
        assert!(!patched.contains("setManifestid(2840772"));
        assert!(!patched.contains("skipManifestPin("));
    }

    #[test]
    fn appends_only_missing_manifest_binding() {
        let base = "addappid(10)\ncustomThing()\n";
        let manifests = vec![(11, "12345".to_string(), "x".to_string())];
        let patched = patch_manifest_bindings(base, &manifests).unwrap();
        assert!(patched.starts_with(base));
        assert!(patched.contains("setManifestid(11, \"12345\")"));
    }

    #[test]
    fn detects_existing_call_case_insensitively() {
        let base = "AddToken(10, \"KEEP\")\n";
        assert!(has_lua_app_call(base, "addtoken", 10));
        assert!(!has_lua_app_call(base, "addtoken", 11));
    }

    #[test]
    fn upsert_optional_call_does_not_duplicate_existing_line() {
        let mut base = "addappid(10)\nAddToken(10, \"OLD\")\ncustomThing()\n".to_string();
        upsert_lua_string_call(&mut base, "addtoken", 10, "NEW");
        assert!(base.contains("AddToken(10, \"NEW\")"));
        assert_eq!(base.to_ascii_lowercase().matches("addtoken(").count(), 1);
        assert!(base.contains("customThing()"));
    }

    #[test]
    fn upsert_optional_call_escapes_lua_control_characters() {
        let mut base = "addappid(10)\n".to_string();
        upsert_lua_string_call(&mut base, "addtoken", 10, "a\"b\\c$1\nnext");
        assert!(base.contains("addtoken(10, \"a\\\"b\\\\c$1\\nnext\")"));
        assert!(!base.contains("\nnext\n"));
    }

    #[test]
    fn selected_build_metadata_is_neutral_and_preserves_provider_fields() {
        let source = "-- Generated by 0xoLemon Launcher\r\n-- Created by Hubcap Manifest\r\n-- Website: https://hubcapmanifest.com/\r\naddappid(10)\r\n";
        let rendered = selected_source_metadata(source, "19641208").unwrap();
        assert!(rendered.starts_with(
            "-- Selected source: Hugging Face curated (Immaking/Luas)\r\n-- Selected BuildID: 19641208\r\n"
        ));
        assert!(rendered.contains("-- Created by Hubcap Manifest\r\n"));
        assert!(rendered.contains("-- Website: https://hubcapmanifest.com/\r\n"));
        assert!(!rendered.contains("Generated by 0xoLemon"));
    }

    #[test]
    fn depot_key_update_preserves_the_rest_of_the_lua() {
        let mut source =
            "-- provider metadata\naddappid(11, 0, \"OLD\")\nsetStat(10, \"7656\")\n".to_string();
        upsert_depot_key_call(&mut source, 11, "NEW");
        assert!(source.contains("addappid(11, 0, \"NEW\")"));
        assert!(!source.contains("OLD"));
        assert!(source.contains("setStat(10, \"7656\")"));
    }

    #[test]
    fn stale_cache_cleanup_keeps_target_and_shared_manifests() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "0xolemon-lua-manifest-cleanup-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        for name in ["11_100.manifest", "12_200.manifest", "13_300.manifest"] {
            std::fs::write(root.join(name), b"fixture").unwrap();
        }
        let old_refs = ["11_100.manifest", "12_200.manifest"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let target_refs = ["13_300.manifest"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let other_refs = ["12_200.manifest"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();

        let stale =
            stale_unreferenced_manifest_paths(&root, &old_refs, &target_refs, Some(&other_refs));
        assert_eq!(stale, vec![root.join("11_100.manifest")]);

        for name in ["11_100.manifest", "12_200.manifest", "13_300.manifest"] {
            std::fs::remove_file(root.join(name)).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
