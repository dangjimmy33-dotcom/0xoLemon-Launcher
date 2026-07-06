use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{TimeZone, Utc};
use rand_core::{OsRng, RngCore};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use url::Url;

use crate::secret_store::{protect, unprotect};

const AUTH_FILE: &str = "discord-auth.json";
const DISCORD_API: &str = "https://discord.com/api/v10";
const DISCORD_AUTHORIZE_ENDPOINT: &str = "https://discord.com/oauth2/authorize";
const DEFAULT_CLIENT_ID: &str = "1512105027270082651";
const REQUIRED_GUILD_ID: &str = "1492076309323714570";
const REQUIRED_GUILD_INVITE: &str = "https://discord.gg/7ZXdTUVsJE";

/// Roles that are allowed access (GOONER and above)
const ALLOWED_ROLE_IDS: &[&str] = &[
    "1492080961125355621",
    "1492130518869999737",
    "1492130703549267999",
    "1492131096937238588",
    "1510584783485403287",
    "1493617856238063669",
    "1492082591652909086",
    "1492568133486252182", // Newly added allowed role
];
const CALLBACK_ADDRESS: &str = "127.0.0.1:48176";
const CALLBACK_URL: &str = "http://127.0.0.1:48176/discord/callback";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);
const MINIMUM_ACCOUNT_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

static LOGIN_LOCK: Mutex<()> = Mutex::new(());
static CURRENT_AUTH_URL: Mutex<Option<String>> = Mutex::new(None);
static CACHED_STATUS: Mutex<Option<(u64, DiscordAuthStatus)>> = Mutex::new(None);
static SESSION_AUTHORIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordAuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub account_created_at: String,
    pub account_age_days: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordAuthStatus {
    pub state: String,
    pub configured: bool,
    pub message: String,
    pub user: Option<DiscordAuthUser>,
    pub guild_id: String,
    pub guild_name: Option<String>,
    pub guild_invite: String,
    pub eligible_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDiscordAuth {
    client_id: String,
    encrypted_access_token: String,
    expires_at: u64,
}

#[derive(Debug, Deserialize)]
struct DiscordUserResponse {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    discriminator: String,
}

#[derive(Debug, Deserialize)]
struct DiscordGuildResponse {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct GuildMemberResponse {
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    NetworkError(String),
    Other(String),
}

pub fn require_authorized_session() -> Result<(), String> {
    if SESSION_AUTHORIZED.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err("Discord authorization is required before using this launcher.".to_string())
    }
}

pub fn get_status(app: &AppHandle) -> DiscordAuthStatus {
    let client_id = client_id();
    if client_id.is_empty() {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        return status(
            "notConfigured",
            false,
            "Discord OAuth is not configured for this build.",
        );
    }

    let stored = match read_stored_auth(app) {
        Ok(stored) => stored,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            return status("signedOut", true, "Sign in with Discord to continue.");
        }
        Err(error) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            return status(
                "error",
                true,
                format!("Discord sign-in data could not be read: {error}"),
            );
        }
    };

    if stored.client_id != client_id {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        return status(
            "signedOut",
            true,
            "Discord application configuration changed. Sign in again.",
        );
    }
    if stored.expires_at <= unix_seconds().saturating_add(30) {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        return status(
            "expired",
            true,
            "Your Discord session expired. Sign in again.",
        );
    }

    if let Ok(guard) = CACHED_STATUS.lock() {
        if let Some((cached_at, ref cached_status)) = *guard {
            if unix_seconds().saturating_sub(cached_at) < 600 {
                SESSION_AUTHORIZED.store(true, Ordering::Release);
                return cached_status.clone();
            }
        }
    }

    let encrypted = match STANDARD.decode(stored.encrypted_access_token) {
        Ok(encrypted) => encrypted,
        Err(error) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            return status(
                "error",
                true,
                format!("Discord session data is invalid: {error}"),
            );
        }
    };
    let token = match unprotect(&encrypted)
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
    {
        Ok(token) => token,
        Err(error) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            return status(
                "error",
                true,
                format!("Discord session could not be unlocked: {error}"),
            );
        }
    };

    match validate_token(&token) {
        Ok(result) => result,
        Err(ApiError::Unauthorized) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            status(
                "expired",
                true,
                "Your Discord session expired. Sign in again.",
            )
        }
        Err(ApiError::NetworkError(_)) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            status(
                "networkError",
                true,
                "No internet connection. Discord access cannot be verified.",
            )
        }
        Err(ApiError::Other(error)) => {
            SESSION_AUTHORIZED.store(false, Ordering::Release);
            status(
                "error",
                true,
                format!("Discord could not be verified: {error}"),
            )
        }
    }
}

pub fn login(app: &AppHandle) -> Result<DiscordAuthStatus, String> {
    let _login_guard = match LOGIN_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            if let Some(url) = CURRENT_AUTH_URL.lock().unwrap().as_ref() {
                let _ = open_system_browser(url);
                return Err(
                    "A Discord sign-in is already in progress. The browser has been reopened."
                        .to_string(),
                );
            }
            return Err("A Discord sign-in is already in progress.".to_string());
        }
    };
    let client_id = client_id();
    if client_id.is_empty() {
        return Err(
            "Discord OAuth client ID is missing. Configure OXO_DISCORD_CLIENT_ID and rebuild."
                .to_string(),
        );
    }

    let listener = TcpListener::bind(CALLBACK_ADDRESS).map_err(|error| {
        format!(
            "Discord callback port 48176 is unavailable. Close the app using that port and try again: {error}"
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;

    let state_nonce = random_urlsafe(32);
    let mut authorize_url =
        Url::parse(DISCORD_AUTHORIZE_ENDPOINT).map_err(|error| error.to_string())?;
    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", CALLBACK_URL)
        .append_pair("response_type", "token")
        .append_pair("scope", "identify guilds guilds.members.read")
        .append_pair("state", &state_nonce)
        .append_pair("prompt", "consent")
        .append_pair("integration_type", "1");
    let _ = app.emit("discord-oauth-url", authorize_url.as_str());
    *CURRENT_AUTH_URL.lock().unwrap() = Some(authorize_url.to_string());
    let _ = open_system_browser(authorize_url.as_str());

    let deadline = Instant::now() + OAUTH_TIMEOUT;
    let mut callback_page_served = false;
    let mut token_result = None;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let request = match read_http_request(&mut stream) {
                    Ok(req) => req,
                    Err(_) => continue,
                };
                if request.path.starts_with("/discord/callback") {
                    let _ = write_http_response(&mut stream, callback_page());
                    callback_page_served = true;
                    continue;
                }
                if request.method == "POST" && request.path == "/discord/complete" {
                    let fields = url::form_urlencoded::parse(request.body.as_bytes())
                        .into_owned()
                        .collect::<Vec<_>>();
                    let returned_state = field(&fields, "state");
                    let access_token = field(&fields, "access_token");
                    let expires_in =
                        field(&fields, "expires_in").and_then(|value| value.parse::<u64>().ok());
                    let oauth_error = field(&fields, "error");

                    if returned_state != Some(state_nonce.as_str()) {
                        let _ = write_http_response(
                            &mut stream,
                            "<h2>Discord sign-in was rejected.</h2><p>Security state validation failed. Return to 0xoLemon and try again.</p>",
                        );
                        return Err("Discord OAuth state validation failed.".to_string());
                    }
                    if let Some(error) = oauth_error {
                        let _ = write_http_response(
                            &mut stream,
                            "<h2>Discord sign-in was canceled.</h2><p>You can close this tab and return to 0xoLemon.</p>",
                        );
                        return Err(format!("Discord authorization was denied: {error}"));
                    }
                    let token = access_token
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "Discord did not return an access token.".to_string())?
                        .to_string();
                    let expires_in = expires_in.filter(|value| *value > 0).ok_or_else(|| {
                        "Discord did not return a valid token lifetime.".to_string()
                    })?;
                    let _ = write_http_response(
                        &mut stream,
                        "<h2>Discord sign-in received.</h2><p>You can close this tab and return to 0xoLemon.</p>",
                    );
                    token_result = Some((token, expires_in));
                    break;
                }
                write_not_found(&mut stream)?;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    *CURRENT_AUTH_URL.lock().unwrap() = None;
    if !callback_page_served {
        return Err("Discord sign-in timed out before the callback was received.".to_string());
    }
    let (token, expires_in) =
        token_result.ok_or_else(|| "Discord sign-in timed out.".to_string())?;
    let validation = validate_token(&token).map_err(|error| match error {
        ApiError::Unauthorized => "Discord rejected the new access token.".to_string(),
        ApiError::NetworkError(message) => format!("Network error: {message}"),
        ApiError::Other(message) => format!("Discord verification failed: {message}"),
    })?;
    write_stored_auth(app, &client_id, &token, expires_in)?;
    Ok(validation)
}

pub fn logout(app: &AppHandle) -> Result<DiscordAuthStatus, String> {
    let path = auth_path(app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    if let Ok(mut guard) = CACHED_STATUS.lock() {
        *guard = None;
    }
    SESSION_AUTHORIZED.store(false, Ordering::Release);
    Ok(status(
        "signedOut",
        !client_id().is_empty(),
        "Signed out of Discord.",
    ))
}

fn validate_token(token: &str) -> Result<DiscordAuthStatus, ApiError> {
    let client = http_client().map_err(ApiError::Other)?;
    let user =
        match discord_get::<DiscordUserResponse>(&client, token, &format!("{DISCORD_API}/users/@me")) {
            Ok(u) => u,
            Err(ApiError::NetworkError(_)) => return Err(ApiError::NetworkError("offline".to_string())),
            Err(e) => return Err(e),
        };
    let guild = find_required_guild(&client, token)?;
    let now_ms = unix_seconds().saturating_mul(1000);
    let created_ms = snowflake_timestamp_ms(&user.id).ok_or_else(|| {
        ApiError::Other("Discord returned an invalid user snowflake.".to_string())
    })?;
    let eligible_ms = created_ms.saturating_add(MINIMUM_ACCOUNT_AGE.as_millis() as u64);
    let profile = map_user(user, created_ms, now_ms);

    if guild.is_none() {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        let mut result = status(
            "notMember",
            true,
            "Join the required Discord server, then check access again.",
        );
        result.user = Some(profile);
        return Ok(result);
    }
    if !account_is_old_enough(created_ms, now_ms) {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        let mut result = status(
            "accountTooNew",
            true,
            "Discord accounts must be at least 7 days old.",
        );
        result.user = Some(profile);
        result.guild_name = guild.map(|value| value.name);
        result.eligible_at = timestamp_to_iso(eligible_ms);
        return Ok(result);
    }

    // Check role-based access
    let guild_name = guild.map(|g| g.name);
    let has_access = check_guild_roles(&client, token);
    if !has_access {
        SESSION_AUTHORIZED.store(false, Ordering::Release);
        let mut result = status(
            "noRole",
            true,
            "Your role in the server does not grant access to the launcher.",
        );
        result.user = Some(profile);
        result.guild_name = guild_name;
        return Ok(result);
    }

    SESSION_AUTHORIZED.store(true, Ordering::Release);
    let mut result = status("authorized", true, "Discord access verified.");
    result.user = Some(profile);
    result.guild_name = guild_name;

    if let Ok(mut guard) = CACHED_STATUS.lock() {
        *guard = Some((unix_seconds(), result.clone()));
    }

    Ok(result)
}

/// Returns true if the user has at least one ALLOWED_ROLE.
fn check_guild_roles(client: &Client, token: &str) -> bool {
    let url = format!("{DISCORD_API}/users/@me/guilds/{REQUIRED_GUILD_ID}/member");
    match discord_get::<GuildMemberResponse>(client, token, &url) {
        Ok(member) => {
            member.roles.iter().any(|r| ALLOWED_ROLE_IDS.contains(&r.as_str()))
        }
        Err(_) => false, // If we can't fetch member info, deny access
    }
}

fn find_required_guild(
    client: &Client,
    token: &str,
) -> Result<Option<DiscordGuildResponse>, ApiError> {
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let mut url = Url::parse(&format!("{DISCORD_API}/users/@me/guilds"))
            .map_err(|error| ApiError::Other(error.to_string()))?;
        url.query_pairs_mut().append_pair("limit", "200");
        if let Some(after) = after.as_deref() {
            url.query_pairs_mut().append_pair("after", after);
        }
        let page = discord_get::<Vec<DiscordGuildResponse>>(client, token, url.as_str())?;
        if let Some(position) = page.iter().position(|guild| guild.id == REQUIRED_GUILD_ID) {
            return Ok(page.into_iter().nth(position));
        }
        if page.len() < 200 {
            return Ok(None);
        }
        after = page.last().map(|guild| guild.id.clone());
    }
    Ok(None)
}

fn discord_get<T: for<'de> Deserialize<'de>>(
    client: &Client,
    token: &str,
    url: &str,
) -> Result<T, ApiError> {
    let response = match client.get(url).bearer_auth(token).send() {
        Ok(res) => res,
        Err(e) => return Err(ApiError::NetworkError(e.to_string())),
    };
    checked_discord_json(response)
}

fn checked_discord_json<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, ApiError> {
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        return Err(ApiError::Unauthorized);
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(ApiError::Other(format!("HTTP {status} {body}")));
    }
    response
        .json::<T>()
        .map_err(|error| ApiError::Other(error.to_string()))
}

fn map_user(user: DiscordUserResponse, created_ms: u64, now_ms: u64) -> DiscordAuthUser {
    let display_name = user
        .global_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&user.username)
        .to_string();
    let avatar_url = match user.avatar {
        Some(hash) => format!(
            "https://cdn.discordapp.com/avatars/{}/{}.png?size=128",
            user.id, hash
        ),
        None => {
            let index = if user.discriminator != "0" && !user.discriminator.is_empty() {
                user.discriminator.parse::<u64>().unwrap_or_default() % 5
            } else {
                user.id.parse::<u64>().unwrap_or_default().wrapping_shr(22) % 6
            };
            format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
        }
    };
    DiscordAuthUser {
        id: user.id,
        username: user.username,
        display_name,
        avatar_url,
        account_created_at: timestamp_to_iso(created_ms).unwrap_or_default(),
        account_age_days: now_ms.saturating_sub(created_ms) / 86_400_000,
    }
}

fn status(
    state: impl Into<String>,
    configured: bool,
    message: impl Into<String>,
) -> DiscordAuthStatus {
    DiscordAuthStatus {
        state: state.into(),
        configured,
        message: message.into(),
        user: None,
        guild_id: REQUIRED_GUILD_ID.to_string(),
        guild_name: None,
        guild_invite: REQUIRED_GUILD_INVITE.to_string(),
        eligible_at: None,
    }
}

fn client_id() -> String {
    let runtime = std::env::var("OXO_DISCORD_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let built = option_env!("OXO_DISCORD_CLIENT_ID")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    runtime
        .or(built)
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

fn read_stored_auth(app: &AppHandle) -> Result<StoredDiscordAuth, std::io::Error> {
    let bytes = fs::read(auth_path(app).map_err(std::io::Error::other)?)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(ErrorKind::InvalidData, error))
}

fn write_stored_auth(
    app: &AppHandle,
    client_id: &str,
    token: &str,
    expires_in: u64,
) -> Result<(), String> {
    let stored = StoredDiscordAuth {
        client_id: client_id.to_string(),
        encrypted_access_token: STANDARD.encode(protect(token.as_bytes())?),
        expires_at: unix_seconds().saturating_add(expires_in),
    };
    let destination = auth_path(app)?;
    let temporary = destination.with_extension("json.tmp");
    let backup = destination.with_extension("json.bak");
    {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(&stored).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        fs::rename(&destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(error.to_string());
    }
    if backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    let mut request = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 4096];
    let mut expected_length = None;
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > 32 * 1024 {
            return Err("Discord callback request was too large.".to_string());
        }
        if let Some(header_end) = find_header_end(&request) {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            expected_length = Some(header_end + 4 + content_length);
        }
        if expected_length.is_some_and(|length| request.len() >= length) {
            break;
        }
    }

    let header_end =
        find_header_end(&request).ok_or_else(|| "Discord callback was malformed.".to_string())?;
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let request_line = headers
        .lines()
        .next()
        .ok_or_else(|| "Discord callback request line was missing.".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "Discord callback method was missing.".to_string())?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| "Discord callback path was missing.".to_string())?
        .to_string();
    let body = String::from_utf8_lossy(&request[header_end + 4..]).to_string();
    Ok(HttpRequest { method, path, body })
}

fn find_header_end(request: &[u8]) -> Option<usize> {
    request.windows(4).position(|window| window == b"\r\n\r\n")
}

fn callback_page() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>0xoLemon Discord Sign-in</title></head>
<body style="font-family:system-ui;background:#0b0e14;color:#f3f5f8;display:grid;place-items:center;min-height:100vh;margin:0">
<main style="max-width:520px;padding:32px;border:1px solid #2d3440;border-radius:18px;background:#111722;text-align:center">
<h2>Completing Discord sign-in...</h2><p id="status" style="color:#aeb8c8">Return to 0xoLemon after this page confirms access.</p>
</main>
<script>
(async () => {
  const values = new URLSearchParams(location.hash.slice(1));
  const body = new URLSearchParams();
  for (const key of ['access_token', 'token_type', 'expires_in', 'state', 'error', 'error_description']) {
    const value = values.get(key);
    if (value) body.set(key, value);
  }
  history.replaceState(null, '', '/discord/callback');
  try {
    const response = await fetch('/discord/complete', {
      method: 'POST',
      headers: {'Content-Type': 'application/x-www-form-urlencoded'},
      body
    });
    document.getElementById('status').innerHTML = await response.text();
  } catch {
    document.getElementById('status').textContent = 'Could not return the sign-in result to 0xoLemon. Close this tab and try again.';
  }
})();
</script>
</body></html>"#
}

fn write_http_response(stream: &mut TcpStream, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; connect-src 'self'; style-src 'unsafe-inline'\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| error.to_string())
}

fn write_not_found(stream: &mut TcpStream) -> Result<(), String> {
    let body = "Not found";
    let response = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| error.to_string())
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn snowflake_timestamp_ms(value: &str) -> Option<u64> {
    value
        .parse::<u64>()
        .ok()
        .map(|snowflake| snowflake.wrapping_shr(22).saturating_add(DISCORD_EPOCH_MS))
}

fn account_is_old_enough(created_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(created_ms) >= MINIMUM_ACCOUNT_AGE.as_millis() as u64
}

fn timestamp_to_iso(timestamp_ms: u64) -> Option<String> {
    Utc.timestamp_millis_opt(timestamp_ms as i64)
        .single()
        .map(|timestamp| timestamp.to_rfc3339())
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
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!(
            "0xoLemon-DiscordAccess/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| error.to_string())
}

fn open_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let escaped_url = url.replace("&", "^&");
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &escaped_url])
            .creation_flags(CREATE_NO_WINDOW)
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
    Err("Opening the system browser is not supported on this platform.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        account_is_old_enough, snowflake_timestamp_ms, DISCORD_EPOCH_MS, MINIMUM_ACCOUNT_AGE,
    };

    #[test]
    fn extracts_creation_time_from_discord_snowflake() {
        let timestamp = DISCORD_EPOCH_MS + 123_456_789;
        let snowflake = ((timestamp - DISCORD_EPOCH_MS) << 22).to_string();
        assert_eq!(snowflake_timestamp_ms(&snowflake), Some(timestamp));
    }

    #[test]
    fn rejects_non_numeric_snowflake() {
        assert_eq!(snowflake_timestamp_ms("not-a-snowflake"), None);
    }

    #[test]
    fn account_age_policy_accepts_exactly_seven_days() {
        let created_ms = 1_700_000_000_000;
        let minimum_ms = MINIMUM_ACCOUNT_AGE.as_millis() as u64;
        assert!(!account_is_old_enough(
            created_ms,
            created_ms + minimum_ms - 1
        ));
        assert!(account_is_old_enough(created_ms, created_ms + minimum_ms));
    }
}
