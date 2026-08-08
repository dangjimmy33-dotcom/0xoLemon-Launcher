use crate::secret_store;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::models::{
    ProviderConfigInput, ProviderConfigView, R2CredentialView, S3CredentialView,
};

pub const PROVIDERS: [&str; 6] = ["gdrive", "onedrive", "r2", "s3", "folder", "local"];

pub fn config_dir() -> Result<PathBuf, String> {
    let root = dirs::config_dir().ok_or_else(|| "Cannot resolve AppData directory".to_string())?;
    let directory = root.join("CloudRedirect");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create {}: {error}", directory.display()))?;
    Ok(directory)
}

pub fn config_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.json"))
}

pub fn settings_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("settings.json"))
}

pub fn default_token_path(provider: &str) -> Result<Option<PathBuf>, String> {
    let directory = config_dir()?;
    Ok(match provider {
        "gdrive" => Some(directory.join("google_tokens.json")),
        "onedrive" => Some(directory.join("onedrive_tokens.json")),
        "r2" => Some(directory.join("r2_credentials.json")),
        "s3" => Some(directory.join("s3_credentials.json")),
        _ => None,
    })
}

fn provider_token_path(config: &Map<String, Value>, provider: &str) -> Option<String> {
    config
        .get("token_paths")
        .and_then(Value::as_object)
        .and_then(|paths| paths.get(provider))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            let active = config.get("provider").and_then(Value::as_str);
            if active == Some(provider) {
                config
                    .get("token_path")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(ToString::to_string)
            } else {
                None
            }
        })
}

fn register_token_path(config: &mut Map<String, Value>, provider: &str, path: &Path) {
    let mut paths = config
        .get("token_paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    paths.insert(
        provider.to_string(),
        Value::String(path.to_string_lossy().into_owned()),
    );
    config.insert("token_paths".to_string(), Value::Object(paths));
}

fn read_json_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    let temporary = path.with_extension("new");
    fs::write(&temporary, bytes)
        .map_err(|error| format!("Cannot write {}: {error}", temporary.display()))?;
    if path.exists() {
        let backup = path.with_extension("bak");
        let _ = fs::copy(path, &backup);
        fs::remove_file(path)
            .map_err(|error| format!("Cannot replace {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "Cannot commit {} to {}: {error}",
            temporary.display(),
            path.display()
        )
    })
}

pub fn read_config_value() -> Result<Value, String> {
    Ok(Value::Object(read_json_object(&config_path()?)))
}

pub fn provider_from_config() -> Option<String> {
    read_config_value()
        .ok()
        .and_then(|value| value.get("provider").and_then(Value::as_str).map(ToString::to_string))
}

pub fn mode_from_settings() -> String {
    let settings = settings_path().ok().map(|path| read_json_object(&path)).unwrap_or_default();
    settings
        .get("mode")
        .and_then(Value::as_str)
        .filter(|mode| matches!(*mode, "cloudredirect" | "stfixer" | "thirdparty"))
        .unwrap_or("cloudredirect")
        .to_string()
}

pub fn save_mode(mode: &str) -> Result<(), String> {
    if !matches!(mode, "cloudredirect" | "stfixer" | "thirdparty") {
        return Err(format!("Unsupported CloudRedirect mode: {mode}"));
    }
    let path = settings_path()?;
    let mut settings = read_json_object(&path);
    settings.insert("mode".to_string(), Value::String(mode.to_string()));
    let bytes = serde_json::to_vec_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    atomic_write(&path, &bytes)
}

pub fn save_provider(input: &ProviderConfigInput) -> Result<ProviderConfigView, String> {
    if !PROVIDERS.contains(&input.provider.as_str()) {
        return Err(format!("Unsupported CloudRedirect provider: {}", input.provider));
    }

    let path = config_path()?;
    let mut config = read_json_object(&path);
    config.insert("provider".to_string(), Value::String(input.provider.clone()));
    config.insert(
        "upload_inflight_mb".to_string(),
        Value::from(input.upload_inflight_mb.unwrap_or(24).clamp(24, 64)),
    );

    match input.provider.as_str() {
        "folder" => {
            let sync_path = required_trimmed(input.sync_path.as_deref(), "A sync folder is required")?;
            let sync_path = PathBuf::from(sync_path);
            config.insert(
                "sync_path".to_string(),
                Value::String(sync_path.to_string_lossy().into_owned()),
            );
            // Upstream's local-folder provider uses the registered token path when
            // invoked through the CLI, while the DLL reads sync_path. Keep both in
            // sync so the integrated UI, migration, and Steam DLL all target the
            // same directory.
            register_token_path(&mut config, "folder", &sync_path);
            config.insert(
                "token_path".to_string(),
                Value::String(sync_path.to_string_lossy().into_owned()),
            );
        }
        "local" => {
            config.remove("token_path");
            config.remove("sync_path");
        }
        "gdrive" | "onedrive" => {
            let token_path = input
                .token_path
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .or(default_token_path(&input.provider)?)
                .ok_or_else(|| "Token path is unavailable".to_string())?;
            config.insert(
                "token_path".to_string(),
                Value::String(token_path.to_string_lossy().into_owned()),
            );
            register_token_path(&mut config, &input.provider, &token_path);
            config.remove("sync_path");
        }
        "r2" => {
            let credentials = input.r2.as_ref().ok_or_else(|| "R2 credentials are required".to_string())?;
            let token_path = default_token_path("r2")?.ok_or_else(|| "R2 credential path is unavailable".to_string())?;
            save_r2_credentials(&token_path, credentials)?;
            config.insert(
                "token_path".to_string(),
                Value::String(token_path.to_string_lossy().into_owned()),
            );
            register_token_path(&mut config, "r2", &token_path);
            config.remove("sync_path");
        }
        "s3" => {
            let credentials = input.s3.as_ref().ok_or_else(|| "S3 credentials are required".to_string())?;
            let token_path = default_token_path("s3")?.ok_or_else(|| "S3 credential path is unavailable".to_string())?;
            save_s3_credentials(&token_path, credentials)?;
            config.insert(
                "token_path".to_string(),
                Value::String(token_path.to_string_lossy().into_owned()),
            );
            register_token_path(&mut config, "s3", &token_path);
            config.remove("sync_path");
        }
        _ => unreachable!(),
    }

    let bytes = serde_json::to_vec_pretty(&Value::Object(config)).map_err(|e| e.to_string())?;
    atomic_write(&path, &bytes)?;
    get_provider_view()
}

fn required_trimmed(value: Option<&str>, message: &str) -> Result<String, String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        Err(message.to_string())
    } else {
        Ok(value.to_string())
    }
}

fn read_secret_json(path: &Path) -> Option<Value> {
    let raw = fs::read(path).ok()?;
    if raw.is_empty() {
        return None;
    }
    let plain = if raw.first() == Some(&b'{') {
        raw
    } else {
        secret_store::unprotect(&raw).ok()?
    };
    serde_json::from_slice(&plain).ok()
}

fn write_secret_json(path: &Path, value: &Value) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let encrypted = secret_store::protect(&json)?;
    atomic_write(path, &encrypted)
}

fn old_secret(path: &Path) -> Option<String> {
    read_secret_json(path)
        .and_then(|value| value.get("secret_access_key").and_then(Value::as_str).map(ToString::to_string))
}

fn save_r2_credentials(path: &Path, input: &super::models::R2CredentialsInput) -> Result<(), String> {
    let account_id = required_trimmed(Some(&input.account_id), "R2 account ID is required")?;
    let access_key_id = required_trimmed(Some(&input.access_key_id), "R2 access key is required")?;
    let bucket = required_trimmed(Some(&input.bucket), "R2 bucket is required")?;
    let secret = input
        .secret_access_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| old_secret(path))
        .ok_or_else(|| "R2 secret access key is required".to_string())?;

    let mut object = json!({
        "account_id": account_id,
        "access_key_id": access_key_id,
        "secret_access_key": secret,
        "bucket": bucket,
    });
    if let Some(value) = input.key_prefix.as_deref().filter(|value| !value.trim().is_empty()) {
        object["key_prefix"] = Value::String(value.trim().to_string());
    }
    if let Some(value) = input.endpoint.as_deref().filter(|value| !value.trim().is_empty()) {
        object["endpoint"] = Value::String(value.trim().to_string());
    }
    write_secret_json(path, &object)
}

fn save_s3_credentials(path: &Path, input: &super::models::S3CredentialsInput) -> Result<(), String> {
    let access_key_id = required_trimmed(Some(&input.access_key_id), "S3 access key is required")?;
    let bucket = required_trimmed(Some(&input.bucket), "S3 bucket is required")?;
    let endpoint = required_trimmed(Some(&input.endpoint), "S3 endpoint is required")?;
    let region = required_trimmed(Some(&input.region), "S3 region is required")?;
    let secret = input
        .secret_access_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| old_secret(path))
        .ok_or_else(|| "S3 secret access key is required".to_string())?;

    let mut object = json!({
        "access_key_id": access_key_id,
        "secret_access_key": secret,
        "bucket": bucket,
        "endpoint": endpoint,
        "region": region,
    });
    if let Some(value) = input.key_prefix.as_deref().filter(|value| !value.trim().is_empty()) {
        object["key_prefix"] = Value::String(value.trim().to_string());
    }
    if let Some(value) = input.ca_cert_path.as_deref().filter(|value| !value.trim().is_empty()) {
        object["ca_cert_path"] = Value::String(value.trim().to_string());
    }
    if input.sign_payload {
        object["sign_payload"] = Value::Bool(true);
    }
    if input.allow_insecure_http {
        object["allow_insecure_http"] = Value::Bool(true);
    }
    if input.allow_insecure_tls {
        object["allow_insecure_tls"] = Value::Bool(true);
    }
    write_secret_json(path, &object)
}

pub fn save_oauth_tokens(
    provider: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: i64,
) -> Result<ProviderConfigView, String> {
    if !matches!(provider, "gdrive" | "onedrive") {
        return Err(format!("OAuth tokens are not supported for provider: {provider}"));
    }
    let token_path = default_token_path(provider)?
        .ok_or_else(|| format!("Token path is unavailable for {provider}"))?;
    let previous_refresh = read_secret_json(&token_path)
        .and_then(|value| value.get("refresh_token").and_then(Value::as_str).map(ToString::to_string));
    let refresh = refresh_token
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .or(previous_refresh)
        .ok_or_else(|| "The provider did not return a refresh token. Revoke the old permission and sign in again.".to_string())?;
    let token = json!({
        "access_token": access_token,
        "refresh_token": refresh,
        "expires_at": expires_at,
    });
    write_secret_json(&token_path, &token)?;

    let path = config_path()?;
    let mut config = read_json_object(&path);
    config.insert("provider".to_string(), Value::String(provider.to_string()));
    config.insert("token_path".to_string(), Value::String(token_path.to_string_lossy().into_owned()));
    register_token_path(&mut config, provider, &token_path);
    config.remove("sync_path");
    let bytes = serde_json::to_vec_pretty(&Value::Object(config)).map_err(|error| error.to_string())?;
    atomic_write(&path, &bytes)?;
    get_provider_view()
}

pub fn oauth_token_is_readable(provider: &str) -> bool {
    default_token_path(provider)
        .ok()
        .flatten()
        .and_then(|path| read_secret_json(&path))
        .and_then(|value| value.get("refresh_token").and_then(Value::as_str).map(|value| !value.is_empty()))
        .unwrap_or(false)
}

/// Loaded (decrypted) OAuth token data.
pub struct OAuthTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
}

/// Load OAuth tokens from the upstream-compatible DPAPI token file.
/// Returns an error if the provider is not supported or the token file is missing/unreadable.
pub fn load_oauth_tokens(provider: &str) -> Result<OAuthTokenData, String> {
    if !matches!(provider, "gdrive" | "onedrive") {
        return Err(format!("OAuth tokens are not supported for provider: {provider}"));
    }
    let token_path = default_token_path(provider)?
        .ok_or_else(|| format!("Token path is unavailable for {provider}"))?;
    let value = read_secret_json(&token_path)
        .ok_or_else(|| format!("No OAuth token found for {provider}. Please sign in first."))?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("OAuth token for {provider} is missing access_token. Please sign in again."))?
        .to_string();
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let expires_at = value
        .get("expires_at")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(OAuthTokenData { access_token, refresh_token, expires_at })
}

pub fn activate_provider(provider: &str) -> Result<ProviderConfigView, String> {
    if !PROVIDERS.contains(&provider) {
        return Err(format!("Unsupported CloudRedirect provider: {provider}"));
    }
    let path = config_path()?;
    let mut config = read_json_object(&path);
    config.insert("provider".to_string(), Value::String(provider.to_string()));
    match provider {
        "folder" => {
            let sync_path = config
                .get("sync_path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| "Folder provider is not configured".to_string())?;
            register_token_path(&mut config, "folder", &sync_path);
            config.insert(
                "token_path".to_string(),
                Value::String(sync_path.to_string_lossy().into_owned()),
            );
        }
        "local" => {
            config.remove("token_path");
            config.remove("sync_path");
        }
        _ => {
            let token = provider_token_path(&config, provider)
                .map(PathBuf::from)
                .or(default_token_path(provider)?)
                .ok_or_else(|| format!("No token path is registered for {provider}"))?;
            if !token.is_file() {
                return Err(format!("Provider credentials are not available: {}", token.display()));
            }
            config.insert("token_path".to_string(), Value::String(token.to_string_lossy().into_owned()));
            register_token_path(&mut config, provider, &token);
            config.remove("sync_path");
        }
    }
    let bytes = serde_json::to_vec_pretty(&Value::Object(config)).map_err(|error| error.to_string())?;
    atomic_write(&path, &bytes)?;
    get_provider_view()
}

pub fn get_provider_view() -> Result<ProviderConfigView, String> {
    let config = read_config_value()?;
    let provider = config
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("local")
        .to_string();
    let token_path = config
        .as_object()
        .and_then(|object| provider_token_path(object, &provider))
        .or_else(|| default_token_path(&provider).ok().flatten().map(|path| path.to_string_lossy().into_owned()));
    let sync_path = config
        .get("sync_path")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let upload_inflight_mb = config
        .get("upload_inflight_mb")
        .and_then(Value::as_u64)
        .unwrap_or(24)
        .clamp(24, 64) as u32;

    let r2 = if provider == "r2" {
        token_path
            .as_deref()
            .map(Path::new)
            .and_then(read_secret_json)
            .map(|value| R2CredentialView {
                account_id: string_value(&value, "account_id"),
                access_key_id: string_value(&value, "access_key_id"),
                has_secret: !string_value(&value, "secret_access_key").is_empty(),
                bucket: string_value(&value, "bucket"),
                key_prefix: optional_string(&value, "key_prefix"),
                endpoint: optional_string(&value, "endpoint"),
            })
    } else {
        None
    };

    let s3 = if provider == "s3" {
        token_path
            .as_deref()
            .map(Path::new)
            .and_then(read_secret_json)
            .map(|value| S3CredentialView {
                access_key_id: string_value(&value, "access_key_id"),
                has_secret: !string_value(&value, "secret_access_key").is_empty(),
                bucket: string_value(&value, "bucket"),
                endpoint: string_value(&value, "endpoint"),
                region: string_value(&value, "region"),
                key_prefix: optional_string(&value, "key_prefix"),
                sign_payload: bool_value(&value, "sign_payload"),
                allow_insecure_http: bool_value(&value, "allow_insecure_http"),
                allow_insecure_tls: bool_value(&value, "allow_insecure_tls"),
                ca_cert_path: optional_string(&value, "ca_cert_path"),
            })
    } else {
        None
    };

    let authenticated = match provider.as_str() {
        "local" => true,
        "folder" => sync_path.as_deref().map(Path::new).is_some_and(Path::is_dir),
        _ => token_path.as_deref().map(Path::new).is_some_and(Path::is_file),
    };

    Ok(ProviderConfigView {
        provider,
        token_path,
        sync_path,
        authenticated,
        upload_inflight_mb,
        r2,
        s3,
    })
}

pub fn provider_display_name(provider: &str) -> String {
    match provider {
        "gdrive" => "Google Drive",
        "onedrive" => "OneDrive",
        "r2" => "Cloudflare R2",
        "s3" => "S3 Compatible",
        "folder" => "Folder / mapped drive",
        "local" => "Local only",
        other => other,
    }
    .to_string()
}

fn string_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn bool_value(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
