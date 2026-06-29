use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rand_core::{OsRng, RngCore};
use reqwest::blocking::{Body, Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use url::Url;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::secret_store::{protect as protect_secret, unprotect as unprotect_secret};

use super::{
    expanded_save_roots, file_entry, parse_manifest_path, replace_file_with_rollback,
    scan_local_roots, secure_local_target, CloudManifest, GameCloudRecord,
};

const AUTH_FILE: &str = "google-drive-auth.json";
const DEFAULT_CLIENT_ID: &str =
    "745435850820-k7v8oqp0g640l8eed7p7nu6f7fd8njoh.apps.googleusercontent.com";
const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);

static ACCESS_TOKEN: OnceLock<Mutex<Option<CachedAccessToken>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<Option<CachedAccessToken>> {
    ACCESS_TOKEN.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    value: String,
    expires_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAuth {
    #[serde(default)]
    client_id: String,
    encrypted_refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveFileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DriveFileCreated {
    id: String,
}

pub(super) fn client_configured() -> bool {
    !client_id().is_empty()
}

pub(super) fn connected(app: &AppHandle) -> bool {
    read_stored_auth(app).is_ok_and(|stored| stored.client_id == client_id())
}

pub(super) fn disconnect(app: &AppHandle) -> Result<(), String> {
    let path = auth_path(app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    if let Ok(mut cached) = token_cache().lock() {
        *cached = None;
    }
    Ok(())
}

pub(super) fn authorize(app: &AppHandle) -> Result<(), String> {
    let client_id = client_id();
    if client_id.is_empty() {
        return Err("Google Drive OAuth client ID is not configured.".to_string());
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random_urlsafe(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(32);
    let mut auth_url = Url::parse(AUTH_ENDPOINT).map_err(|error| error.to_string())?;
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", DRIVE_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    open_system_browser(auth_url.as_str())?;

    let deadline = Instant::now() + OAUTH_TIMEOUT;
    let mut authorization_code = None;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = [0_u8; 8192];
                let read = stream
                    .read(&mut request)
                    .map_err(|error| error.to_string())?;
                let request = String::from_utf8_lossy(&request[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| "Google OAuth callback request was invalid".to_string())?;
                let callback = Url::parse(&format!("http://127.0.0.1:{port}{target}"))
                    .map_err(|error| error.to_string())?;
                let params = callback.query_pairs().into_owned().collect::<Vec<_>>();
                let returned_state = params
                    .iter()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.as_str());
                let error = params
                    .iter()
                    .find(|(key, _)| key == "error")
                    .map(|(_, value)| value.clone());
                let code = params
                    .iter()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.clone());

                let success =
                    returned_state == Some(state.as_str()) && error.is_none() && code.is_some();
                let body = if success {
                    "<html><body><h2>0xoLemon connected to Google Drive.</h2><p>You can close this tab and return to the launcher.</p></body></html>"
                } else {
                    "<html><body><h2>Google Drive authorization failed.</h2><p>Return to the launcher for details.</p></body></html>"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .map_err(|error| error.to_string())?;

                if returned_state != Some(state.as_str()) {
                    return Err("Google OAuth state validation failed".to_string());
                }
                if let Some(error) = error {
                    return Err(format!("Google authorization was denied: {error}"));
                }
                authorization_code = code;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let code = authorization_code.ok_or_else(|| "Google Drive sign-in timed out".to_string())?;
    let token = http_client()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .map_err(|error| error.to_string())?;
    let token = checked_json::<TokenResponse>(token)?;
    let refresh_token = token.refresh_token.ok_or_else(|| {
        "Google did not return a refresh token; revoke access and sign in again".to_string()
    })?;
    write_refresh_token(app, &refresh_token)?;
    cache_access_token(token.access_token, token.expires_in);
    Ok(())
}

pub(super) fn backup(
    app: &AppHandle,
    game_id: &str,
    record: &GameCloudRecord,
) -> Result<(), String> {
    let roots = expanded_save_roots(app, game_id, record)?;
    let manifest = scan_local_roots(&roots, &record.include, &record.exclude)?;
    if manifest.files.is_empty() {
        return Err("No save files were found to back up.".to_string());
    }
    let archive = archive_path(app, game_id)?;
    write_archive(&archive, &roots, &manifest)?;
    let access_token = access_token(app)?;
    let existing = find_backup_file(&access_token, game_id)?;
    upload_archive(&access_token, game_id, &archive, existing.as_deref())?;
    fs::remove_file(archive).map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn restore_missing(
    app: &AppHandle,
    game_id: &str,
    record: &GameCloudRecord,
) -> Result<usize, String> {
    if !connected(app) {
        return Ok(0);
    }
    let access_token = access_token(app)?;
    let Some(file_id) = find_backup_file(&access_token, game_id)? else {
        return Ok(0);
    };
    let archive = archive_path(app, game_id)?;
    download_archive(&access_token, &file_id, &archive)?;
    let roots = expanded_save_roots(app, game_id, record)?;
    let restored = extract_missing_files(&archive, &roots)?;
    fs::remove_file(archive).map_err(|error| error.to_string())?;
    Ok(restored)
}

fn write_archive(
    archive_path: &Path,
    roots: &[PathBuf],
    manifest: &CloudManifest,
) -> Result<(), String> {
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = File::create(archive_path).map_err(|error| error.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    zip.start_file("manifest.json", options)
        .map_err(|error| error.to_string())?;
    zip.write_all(&serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let mut buffer = [0_u8; 1024 * 1024];
    for entry in &manifest.files {
        let (root_index, relative) = parse_manifest_path(&entry.path)?;
        let root = roots
            .get(root_index)
            .ok_or_else(|| "Google Drive backup root index is invalid".to_string())?;
        let source = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mut input = File::open(source).map_err(|error| error.to_string())?;
        zip.start_file(format!("files/{}", entry.path), options)
            .map_err(|error| error.to_string())?;
        loop {
            let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
    }
    let output = zip.finish().map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())
}

fn extract_missing_files(archive_path: &Path, roots: &[PathBuf]) -> Result<usize, String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let manifest: CloudManifest = {
        let mut entry = zip
            .by_name("manifest.json")
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
    };
    let mut restored = 0;
    for expected in &manifest.files {
        let (root_index, relative) = parse_manifest_path(&expected.path)?;
        let root = roots
            .get(root_index)
            .ok_or_else(|| "Google Drive restore root index is invalid".to_string())?;
        let target = secure_local_target(root, &relative)?;
        if target.exists() {
            continue;
        }
        let mut zipped = zip
            .by_name(&format!("files/{}", expected.path))
            .map_err(|error| error.to_string())?;
        let temporary = target.with_extension(format!(
            "{}.0xo-drive.tmp",
            target
                .extension()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        if let Some(parent) = temporary.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        {
            let mut output = File::create(&temporary).map_err(|error| error.to_string())?;
            std::io::copy(&mut zipped, &mut output).map_err(|error| error.to_string())?;
            output.sync_all().map_err(|error| error.to_string())?;
        }
        let actual = file_entry(&temporary, expected.path.clone())?;
        if actual.size != expected.size || actual.blake3 != expected.blake3 {
            fs::remove_file(&temporary).map_err(|error| error.to_string())?;
            return Err(format!(
                "Google Drive backup verification failed for {}",
                expected.path
            ));
        }
        fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
        restored += 1;
    }
    Ok(restored)
}

fn upload_archive(
    access_token: &str,
    game_id: &str,
    archive_path: &Path,
    existing_file_id: Option<&str>,
) -> Result<String, String> {
    let metadata = if existing_file_id.is_some() {
        serde_json::json!({
            "name": backup_file_name(game_id),
            "mimeType": "application/zip",
            "appProperties": { "gameId": game_id, "schema": "1" }
        })
    } else {
        serde_json::json!({
            "name": backup_file_name(game_id),
            "mimeType": "application/zip",
            "parents": ["appDataFolder"],
            "appProperties": { "gameId": game_id, "schema": "1" }
        })
    };
    let size = fs::metadata(archive_path)
        .map_err(|error| error.to_string())?
        .len();
    let url = if let Some(file_id) = existing_file_id {
        format!("{DRIVE_UPLOAD_API}/files/{file_id}?uploadType=resumable&fields=id")
    } else {
        format!("{DRIVE_UPLOAD_API}/files?uploadType=resumable&fields=id")
    };
    let request = if existing_file_id.is_some() {
        http_client()?.patch(url)
    } else {
        http_client()?.post(url)
    };
    let session = request
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/json; charset=UTF-8")
        .header("X-Upload-Content-Type", "application/zip")
        .header("X-Upload-Content-Length", size)
        .json(&metadata)
        .send()
        .map_err(|error| error.to_string())?;
    if !session.status().is_success() {
        return Err(response_error(
            "Google Drive upload session failed",
            session,
        ));
    }
    let location = session
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "Google Drive did not return an upload session URL".to_string())?
        .to_string();
    let upload = http_client()?
        .put(location)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/zip")
        .header(CONTENT_LENGTH, size)
        .body(Body::new(
            File::open(archive_path).map_err(|error| error.to_string())?,
        ))
        .send()
        .map_err(|error| error.to_string())?;
    Ok(checked_json::<DriveFileCreated>(upload)?.id)
}

fn download_archive(access_token: &str, file_id: &str, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut response = http_client()?
        .get(format!("{DRIVE_API}/files/{file_id}?alt=media"))
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(response_error("Google Drive download failed", response));
    }
    let mut output = File::create(target).map_err(|error| error.to_string())?;
    response
        .copy_to(&mut output)
        .map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())
}

fn find_backup_file(access_token: &str, game_id: &str) -> Result<Option<String>, String> {
    let escaped_name = backup_file_name(game_id).replace('\'', "\\'");
    let response = http_client()?
        .get(format!("{DRIVE_API}/files"))
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .query(&[
            ("spaces", "appDataFolder"),
            ("q", &format!("name = '{escaped_name}' and trashed = false")),
            ("fields", "files(id)"),
            ("pageSize", "1"),
        ])
        .send()
        .map_err(|error| error.to_string())?;
    Ok(checked_json::<DriveFileList>(response)?
        .files
        .into_iter()
        .next()
        .map(|file| file.id))
}

fn access_token(app: &AppHandle) -> Result<String, String> {
    let now = unix_seconds();
    if let Ok(cached) = token_cache().lock() {
        if let Some(token) = cached.as_ref().filter(|token| token.expires_at > now + 60) {
            return Ok(token.value.clone());
        }
    }
    let refresh_token = read_refresh_token(app)?;
    let client_id = client_id();
    let response = http_client()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|error| error.to_string())?;
    let token = checked_json::<TokenResponse>(response)?;
    cache_access_token(token.access_token.clone(), token.expires_in);
    Ok(token.access_token)
}

fn cache_access_token(value: String, expires_in: u64) {
    if let Ok(mut cached) = token_cache().lock() {
        *cached = Some(CachedAccessToken {
            value,
            expires_at: unix_seconds().saturating_add(expires_in),
        });
    }
}

fn client_id() -> String {
    std::env::var("OXO_GOOGLE_DRIVE_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
        .trim()
        .to_string()
}

fn auth_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(AUTH_FILE))
}

fn archive_path(app: &AppHandle, game_id: &str) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("google-drive");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(format!("{}.zip", sanitize_name(game_id))))
}

fn backup_file_name(game_id: &str) -> String {
    format!("0xoLemon-CloudSave-{}.zip", sanitize_name(game_id))
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect()
}

fn random_urlsafe(length: usize) -> String {
    let mut bytes = vec![0_u8; length];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .user_agent("0xoLemon-GoogleDrive/0.1.1")
        .build()
        .map_err(|error| error.to_string())
}

fn checked_json<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(response_error("Google API request failed", response));
    }
    response.json::<T>().map_err(|error| error.to_string())
}

fn response_error(context: &str, response: Response) -> String {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    format!("{context}: HTTP {status} {body}")
}

fn open_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err("Opening the system browser is not supported on this platform".to_string())
}

fn write_refresh_token(app: &AppHandle, token: &str) -> Result<(), String> {
    let encrypted = protect_secret(token.as_bytes())?;
    let stored = StoredAuth {
        client_id: client_id(),
        encrypted_refresh_token: STANDARD.encode(encrypted),
    };
    let path = auth_path(app)?;
    let temporary = path.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&stored).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    replace_file_with_rollback(&temporary, &path)
}

fn read_refresh_token(app: &AppHandle) -> Result<String, String> {
    let stored = read_stored_auth(app)?;
    if stored.client_id != client_id() {
        return Err(
            "Google Drive authorization changed. Sign in again to reconnect this launcher."
                .to_string(),
        );
    }
    let encrypted = STANDARD
        .decode(stored.encrypted_refresh_token)
        .map_err(|error| error.to_string())?;
    String::from_utf8(unprotect_secret(&encrypted)?).map_err(|error| error.to_string())
}

fn read_stored_auth(app: &AppHandle) -> Result<StoredAuth, String> {
    serde_json::from_slice(&fs::read(auth_path(app)?).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_save::clear_tree_safely;

    #[test]
    fn archive_restore_only_recreates_missing_files() {
        let root = std::env::temp_dir().join(format!(
            "0xo-drive-archive-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let first = root.join("first");
        let second = root.join("second");
        let archive = root.join("backup.zip");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("profile.sav"), b"first-backup").unwrap();
        fs::write(second.join("profile.sav"), b"second-backup").unwrap();
        let roots = vec![first.clone(), second.clone()];
        let manifest = scan_local_roots(&roots, &[], &[]).unwrap();
        write_archive(&archive, &roots, &manifest).unwrap();

        fs::remove_file(first.join("profile.sav")).unwrap();
        fs::write(second.join("profile.sav"), b"local-newer").unwrap();
        let restored = extract_missing_files(&archive, &roots).unwrap();

        assert_eq!(restored, 1);
        assert_eq!(
            fs::read(first.join("profile.sav")).unwrap(),
            b"first-backup"
        );
        assert_eq!(
            fs::read(second.join("profile.sav")).unwrap(),
            b"local-newer"
        );
        clear_tree_safely(&root).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn refresh_token_round_trips_through_windows_dpapi() {
        let encrypted = protect_secret(b"refresh-token").unwrap();
        assert_ne!(encrypted, b"refresh-token");
        assert_eq!(unprotect_secret(&encrypted).unwrap(), b"refresh-token");
    }
}
