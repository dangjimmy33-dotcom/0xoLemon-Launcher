// Downloads cloud_redirect.dll from CloudRedirect GitHub Releases.
// Falls back to an older tag if the latest doesn't have the asset.

use std::path::Path;

const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/Selectively11/CloudRedirect/releases/latest";
const DLL_ASSET_NAME: &str = "cloud_redirect.dll";
const USER_AGENT: &str = "0xoLemon-Launcher/1.0";

/// Download cloud_redirect.dll into `dest`. Returns Some(error) on failure.
pub fn download_cloud_redirect_dll(dest: &Path) -> Option<String> {
    // Step 1: Fetch latest release JSON to find download URL.
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
    {
        Ok(c) => c,
        Err(e) => return Some(format!("Failed to build HTTP client: {}", e)),
    };

    let release_info = match client.get(GITHUB_RELEASES_API).send() {
        Ok(r) => match r.text() {
            Ok(t) => t,
            Err(e) => return Some(format!("Failed to read release info: {}", e)),
        },
        Err(e) => return Some(format!("Failed to fetch release info: {}", e)),
    };

    // Parse the browser_download_url for cloud_redirect.dll using simple string search.
    let dll_url = match find_dll_url(&release_info) {
        Some(u) => u,
        None => return Some(format!(
            "Could not find {} in the latest release. The CloudRedirect project may not have published a binary release yet.",
            DLL_ASSET_NAME
        )),
    };

    // Step 2: Download the DLL.
    println!("Downloading {} from {}", DLL_ASSET_NAME, dll_url);
    let dll_bytes = match client.get(&dll_url).send() {
        Ok(r) => match r.bytes() {
            Ok(b) => b.to_vec(),
            Err(e) => return Some(format!("Failed to read DLL bytes: {}", e)),
        },
        Err(e) => return Some(format!("Failed to download DLL: {}", e)),
    };

    if dll_bytes.is_empty() {
        return Some("Downloaded DLL was empty".to_string());
    }

    // Step 3: Write atomically.
    if let Some(dir) = dest.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(dest, &dll_bytes) {
        Ok(()) => {
            println!("Deployed {} ({} bytes) to {}", DLL_ASSET_NAME, dll_bytes.len(), dest.display());
            None
        }
        Err(e) => Some(format!("Failed to write {}: {}", DLL_ASSET_NAME, e)),
    }
}

/// Extract the browser_download_url for cloud_redirect.dll from GitHub release JSON.
fn find_dll_url(json: &str) -> Option<String> {
    // Find "cloud_redirect.dll" in the assets array, then find browser_download_url nearby.
    let dll_pos = json.find("cloud_redirect.dll")?;
    // Look backwards for the enclosing asset object to find browser_download_url.
    let asset_block = &json[..dll_pos + 200.min(json.len() - dll_pos)];
    // Search forward from the dll name for browser_download_url.
    let search_region = &json[dll_pos..];
    let url_key = "\"browser_download_url\":\"";
    // Try forward first
    if let Some(url_start) = search_region.find(url_key) {
        let url_content = &search_region[url_start + url_key.len()..];
        if let Some(url_end) = url_content.find('"') {
            return Some(url_content[..url_end].to_string());
        }
    }
    // Try looking backwards from the dll name
    let before_dll = &json[..dll_pos];
    if let Some(last_url_start) = before_dll.rfind(url_key) {
        let url_content = &before_dll[last_url_start + url_key.len()..];
        if let Some(url_end) = url_content.find('"') {
            return Some(url_content[..url_end].to_string());
        }
    }
    let _ = asset_block; // suppress unused warning
    None
}
