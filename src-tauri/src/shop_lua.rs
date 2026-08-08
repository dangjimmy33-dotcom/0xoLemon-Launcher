// shop_lua.rs — 0xoLemon Lua Shop backend
// Fetches game catalog from HuggingFace Depotdownloader dataset,
// parses build/manifest structure, downloads manifests, and writes
// properly-formatted Lua scripts for the SteamPlugin system.

use std::path::Path;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::blocking::Client;
use once_cell::sync::Lazy;

// ─── HuggingFace API helpers ──────────────────────────────────────────────────

/// Load the HuggingFace access token from the embedded config.
fn get_hf_token() -> String {
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

/// A game available in the Depotdownloader catalog.
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
        return Err(format!("Patch history unavailable (HTTP {})", response.status()));
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

/// Return the list of all games available in the Depotdownloader catalog.
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

fn catalog_blocking() -> Result<Vec<ShopGame>, String> {
    let token = get_hf_token();
    let client = build_client()?;
    let url = format!("{}/Depotdownloader", api_tree_base());

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

    let mut games = Vec::new();
    for node in nodes {
        if node.node_type != "directory" {
            continue;
        }
        // Folder names look like  "Depotdownloader/Game Name (12345)"
        let folder = node.path.split('/').last().unwrap_or("");
        if let Some(paren_start) = folder.rfind('(') {
            if let Some(paren_end) = folder.rfind(')') {
                let name = folder[..paren_start].trim().to_string();
                if let Ok(appid) = folder[paren_start + 1..paren_end].trim().parse::<u32>() {
                    if !name.is_empty() {
                        if let Ok(mut cache) = CATALOG_FOLDER_CACHE.lock() {
                            cache.insert(appid, folder.to_string());
                        }
                        games.push(ShopGame { name, appid });
                    }
                }
            }
        }
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
        return Ok(GameBuildsInfo { builds: Vec::new(), has_key: false });
    };
    let rel_path = format!("Depotdownloader/{}/{}", folder_name, appid);
    let url = format!("{}/{}", api_tree_base(), pct_encode(&rel_path));

    let resp = client
        .get(&url)
        .headers(auth_headers(&token))
        .send()
        .map_err(|_| "Service temporarily unavailable".to_string())?;

    if !resp.status().is_success() {
        return Ok(GameBuildsInfo { builds: Vec::new(), has_key: false });
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
        let build_rel = format!(
            "Depotdownloader/{}/{}/BuildID_{}",
            folder_name, appid, bid
        );
        let build_url = format!("{}/{}", api_tree_base(), pct_encode(&build_rel));

        let build_resp = client
            .get(&build_url)
            .headers(auth_headers(&token))
            .send();

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
                            if let Ok(vr) = client
                                .get(&ver_url)
                                .headers(auth_headers(&token))
                                .send()
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
    if let Ok(steamcmd_resp) = client.get(format!("https://api.steamcmd.net/v1/info/{}", appid)).send() {
        if steamcmd_resp.status().is_success() {
            if let Ok(json) = steamcmd_resp.json::<serde_json::Value>() {
                if let Some(branches) = json.get("data")
                    .and_then(|d| d.get(appid.to_string()))
                    .and_then(|a| a.get("depots"))
                    .and_then(|d| d.get("branches"))
                    .and_then(|b| b.as_object()) 
                {
                    let mut date_map = std::collections::HashMap::new();
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

    Ok(GameBuildsInfo { builds, has_key })
}

/// Install a game: download manifests to depotcache/ and write the Lua config.
#[tauri::command]
pub async fn lua_shop_install_game(
    appid: u32,
    game_name: String,
    build_id: String,
    access_token: Option<String>,
    stat_steam_id: Option<String>,
    skip_manifest_pin: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        install_game_blocking(
            appid,
            &game_name,
            &build_id,
            access_token.as_deref(),
            stat_steam_id.as_deref(),
            skip_manifest_pin.unwrap_or(false),
        )
    })
    .await
    .map_err(|_| "Installation failed".to_string())?
}


fn fetch_text_rel(client: &Client, token: &str, rel: &str) -> Option<String> {
    let url = format!("{}/{}", raw_base(), pct_encode(rel));
    let response = client.get(&url).headers(auth_headers(token)).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().ok()?;
    if text.trim().is_empty() { None } else { Some(text) }
}

fn choose_lua_base(
    client: &Client,
    token: &str,
    stplug_in_dir: &Path,
    folder_name: &str,
    appid: u32,
) -> String {
    let local_path = stplug_in_dir.join(format!("{}.lua", appid));
    let local = std::fs::read_to_string(&local_path).ok().filter(|v| !v.trim().is_empty());
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
    skip_manifest_pin: bool,
) -> Result<String, String> {
    let mut output = base.to_string();

    if skip_manifest_pin {
        let patterns: Vec<regex::Regex> = manifest_entries
            .iter()
            .filter_map(|(depot_id, _, _)| {
                regex::RegexBuilder::new(&format!(r"setmanifestid\s*\(\s*{}\s*,", depot_id))
                    .case_insensitive(true)
                    .build()
                    .ok()
            })
            .collect();
        let newline = if output.contains("\r\n") { "\r\n" } else { "\n" };
        let trailing_newline = output.ends_with('\n');
        let kept: Vec<&str> = output
            .lines()
            .filter(|line| !patterns.iter().any(|re| re.is_match(line)))
            .collect();
        output = kept.join(newline);
        if trailing_newline && !output.is_empty() {
            output.push_str(newline);
        }
        return Ok(output);
    }

    for (depot_id, manifest_gid, _) in manifest_entries {
        let re = regex::RegexBuilder::new(&format!(
            r#"(setmanifestid\s*\(\s*{}\s*,\s*)[\"'][^\"']*[\"']"#,
            depot_id
        ))
        .case_insensitive(true)
        .build()
        .map_err(|_| "Failed to prepare Lua manifest patch".to_string())?;

        if re.is_match(&output) {
            output = re
                .replace_all(&output, format!(r#"${{1}}"{}""#, manifest_gid))
                .to_string();
        } else {
            append_lua_line(
                &mut output,
                &format!(r#"setManifestid({}, "{}")"#, depot_id, manifest_gid),
            );
        }
    }

    Ok(output)
}

fn has_lua_app_call(content: &str, function_name: &str, appid: u32) -> bool {
    let pattern = format!(
        r#"{}\s*\(\s*{}\s*,"#,
        regex::escape(function_name),
        appid
    );
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(content))
        .unwrap_or(false)
}

fn upsert_lua_string_call(content: &mut String, function_name: &str, appid: u32, value: &str) {
    let pattern = format!(
        r#"({}\s*\(\s*{}\s*,\s*)[\"'][^\"']*[\"']"#,
        regex::escape(function_name),
        appid
    );
    if let Ok(re) = regex::RegexBuilder::new(&pattern).case_insensitive(true).build() {
        if re.is_match(content) {
            *content = re
                .replace_all(content, format!(r#"${{1}}"{}""#, value))
                .to_string();
            return;
        }
    }
    append_lua_line(content, &format!(r#"{}({}, "{}")"#, function_name, appid, value));
}

fn install_game_blocking(
    appid: u32,
    game_name: &str,
    build_id: &str,
    access_token: Option<&str>,
    stat_steam_id: Option<&str>,
    skip_manifest_pin: bool,
) -> Result<(), String> {
    let steam_path =
        crate::steam::get_steam_path().ok_or("Steam installation not found")?;
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
    let key_rel = format!(
        "Depotdownloader/{}/{}/{}.key",
        folder_name, appid, appid
    );
    let key_url = format!("{}/{}", raw_base(), pct_encode(&key_rel));
    let mut depot_keys: Vec<(u32, String)> = Vec::new();

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
                            depot_keys.push((depot_id, parts[1].to_string()));
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
        .ok();

    let hf_found = build_resp
        .as_ref()
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    // (depot_id, manifest_gid, hf_path)
    let mut manifest_entries: Vec<(u32, String, String)> = Vec::new();

    if hf_found {
        if let Some(resp) = build_resp {
            if let Ok(nodes) = resp.json::<Vec<HfNode>>() {
                for node in &nodes {
                    let fname = node.path.split('/').last().unwrap_or("");
                    if fname.ends_with(".manifest") {
                        let stem = fname.trim_end_matches(".manifest");
                        if let Some(up) = stem.find('_') {
                            if let Ok(depot_id) = stem[..up].parse::<u32>() {
                                manifest_entries.push((
                                    depot_id,
                                    stem[up + 1..].to_string(),
                                    node.path.clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if !hf_found || manifest_entries.is_empty() {
        return Err(format!(
            "Build {} is known from patch history, but exact depot manifests are unavailable from the configured primary source",
            build_id
        ));
    }

    // ── 3. Prefer manifest binaries from the custom source. If a binary is
    // unavailable, keep the known depot/GID mapping: 0xoLemonCore can resolve
    // the manifest request code for that GID through its configured runtime
    // manifest resolver (OpenSteamTool-compatible fallback).
    if hf_found {
        for (depot_id, manifest_gid, raw_path) in &manifest_entries {
            let manifest_url = format!("{}/{}", raw_base(), pct_encode(raw_path));
            let mr = client
                .get(&manifest_url)
                .headers(auth_headers(&token))
                .send();

            match mr {
                Ok(response) if response.status().is_success() => {
                    if let Ok(bytes) = response.bytes() {
                        let dest = depotcache_dir.join(format!("{}_{}.manifest", depot_id, manifest_gid));
                        if std::fs::write(&dest, &bytes).is_err() {
                        }
                    } else {
                    }
                }
                _ => {
                }
            }
        }
    }

    // ── 4. Preserve the best available Lua base and patch only version data ──
    let original_lua = choose_lua_base(&client, &token, &stplug_in_dir, &folder_name, appid);

    let hf_token_rel = format!("Depotdownloader/{}/{}/{}.token", folder_name, appid, appid);
    let downloaded_token = fetch_text_rel(&client, &token, &hf_token_rel)
        .map(|text| text.trim().to_string())
        .unwrap_or_default();

    let mut final_lua = if original_lua.trim().is_empty() {
        let mut minimal = String::new();
        append_lua_line(&mut minimal, "-- Generated by 0xoLemon Launcher");
        append_lua_line(&mut minimal, &format!("-- Game: {} | AppID: {}", game_name, appid));
        append_lua_line(&mut minimal, &format!("addappid({})", appid));
        for (depot_id, key) in &depot_keys {
            append_lua_line(&mut minimal, &format!(r#"addappid({}, 0, "{}")"#, depot_id, key));
        }
        minimal
    } else {
        original_lua
    };

    final_lua = patch_manifest_bindings(&final_lua, &manifest_entries, skip_manifest_pin)?;

    // Keep unrelated Lua content byte-for-byte as much as possible. Only update
    // optional token/stat calls when the user explicitly supplied them. The
    // repository token is used only when the base Lua does not already define one.
    if let Some(tok) = access_token.filter(|tok| !tok.is_empty()) {
        upsert_lua_string_call(&mut final_lua, "addtoken", appid, tok);
    } else if !downloaded_token.is_empty() && !has_lua_app_call(&final_lua, "addtoken", appid) {
        append_lua_line(&mut final_lua, &format!(r#"addtoken({}, "{}")"#, appid, downloaded_token));
    }

    if let Some(sid) = stat_steam_id.filter(|sid| !sid.is_empty()) {
        upsert_lua_string_call(&mut final_lua, "setStat", appid, sid);
    }

    // ── 5. Write Lua to stplug-in/ ────────────────────────────────────────────
    let lua_file = stplug_in_dir.join(format!("{}.lua", appid));
    std::fs::write(&lua_file, final_lua)
        .map_err(|_| "Failed to write game configuration".to_string())?;

    // ── 6. Refresh .sync_state ────────────────────────────────────────────────
    update_sync_state(&stplug_in_dir)?;

    Ok(())
}


#[cfg(test)]
mod lua_patch_tests {
    use super::*;

    #[test]
    fn patches_manifest_case_insensitively_and_preserves_other_lines() {
        let base = "addappid(2840770)\nSETManifestid(2840771, \"OLD\", 123)\nsetStat(2840770, \"7656\")\ncustomThing(\"keep me\")\n";
        let manifests = vec![(2840771, "NEWGID".to_string(), "x".to_string())];
        let patched = patch_manifest_bindings(base, &manifests, false).unwrap();
        assert!(patched.contains("SETManifestid(2840771, \"NEWGID\", 123)"));
        assert!(patched.contains("setStat(2840770, \"7656\")"));
        assert!(patched.contains("customThing(\"keep me\")"));
        assert!(!patched.contains("OLD"));
    }

    #[test]
    fn appends_only_missing_manifest_binding() {
        let base = "addappid(10)\ncustomThing()\n";
        let manifests = vec![(11, "12345".to_string(), "x".to_string())];
        let patched = patch_manifest_bindings(base, &manifests, false).unwrap();
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
}

/// Re-generate the .sync_state file listing every .lua in stplug-in/.
fn update_sync_state(stplug_in_dir: &Path) -> Result<(), String> {
    let sync_state_path = stplug_in_dir.join(".sync_state");
    let mut lua_files: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(stplug_in_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".lua") {
                    lua_files.push(name.to_string());
                }
            }
        }
    }

    if lua_files.is_empty() {
        return Ok(());
    }
    lua_files.sort();
    let content = lua_files.join("\n") + "\n";
    std::fs::write(&sync_state_path, content)
        .map_err(|_| "Failed to update sync state".to_string())
}
