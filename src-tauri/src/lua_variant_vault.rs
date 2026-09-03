use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const VAULT_SCHEMA_VERSION: u32 = 1;
const MAX_VARIANT_BYTES: u64 = 4 * 1024 * 1024;
static VAULT_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaVariantOrigin {
    Active,
    ProviderLive,
    ProviderRaw,
    Imported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaVariantCaptureReason {
    Manual,
    BeforeUpdate,
    BeforeProviderSwitch,
    BeforeChannelSwitch,
    BeforeImport,
    BeforeRestore,
    LegacyMigration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LuaVariantValidationStatus {
    Valid,
    RecoveryOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuaVariantEntry {
    pub id: String,
    pub app_id: u32,
    pub sha256: String,
    pub byte_length: u64,
    pub origin: LuaVariantOrigin,
    pub capture_reason: LuaVariantCaptureReason,
    pub provider: Option<String>,
    pub source: Option<String>,
    pub channel: Option<String>,
    pub build_id: Option<String>,
    pub revision: Option<String>,
    pub captured_at: String,
    #[serde(default = "unknown_encoding_status")]
    pub encoding_status: String,
    pub validation_status: LuaVariantValidationStatus,
    pub manifest_snapshot_identity: Option<String>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaVariantRestoreRequest {
    pub app_id: u32,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CaptureMetadata {
    pub origin: LuaVariantOrigin,
    pub reason: LuaVariantCaptureReason,
    pub provider: Option<String>,
    pub source: Option<String>,
    pub channel: Option<String>,
    pub build_id: Option<String>,
    pub revision: Option<String>,
    pub validation_status: LuaVariantValidationStatus,
    pub manifest_snapshot_identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LuaVariantIndex {
    schema_version: u32,
    entries: Vec<LuaVariantEntry>,
}

impl Default for LuaVariantIndex {
    fn default() -> Self {
        Self {
            schema_version: VAULT_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

pub(crate) fn vault_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|root| root.join("lua-variant-vault"))
        .map_err(|error| format!("Lua variant vault directory unavailable: {error}"))
}

pub(crate) fn objects_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(vault_root(app)?.join("objects"))
}

pub(crate) fn object_path(app: &AppHandle, sha256: &str) -> Result<PathBuf, String> {
    validate_sha256(sha256)?;
    Ok(objects_root(app)?.join(format!("{}.lua", sha256.to_ascii_lowercase())))
}

pub(crate) fn capture_bytes(
    app: &AppHandle,
    app_id: u32,
    bytes: &[u8],
    metadata: CaptureMetadata,
) -> Result<LuaVariantEntry, String> {
    capture_bytes_at(&vault_root(app)?, app_id, bytes, metadata)
}

fn capture_bytes_at(
    root: &Path,
    app_id: u32,
    bytes: &[u8],
    metadata: CaptureMetadata,
) -> Result<LuaVariantEntry, String> {
    if app_id == 0 {
        return Err("Lua variant AppID must be positive".to_string());
    }
    if bytes.len() as u64 > MAX_VARIANT_BYTES {
        return Err(format!(
            "Lua variant exceeds the {} byte safety limit",
            MAX_VARIANT_BYTES
        ));
    }
    let _guard = VAULT_LOCK
        .lock()
        .map_err(|_| "Lua variant vault is busy".to_string())?;
    let objects = root.join("objects");
    fs::create_dir_all(&objects)
        .map_err(|error| format!("Could not prepare Lua variant objects: {error}"))?;
    let sha256 = hex::encode(Sha256::digest(bytes));
    let object = objects.join(format!("{sha256}.lua"));
    write_object_once(&object, bytes, &sha256)?;

    let entry = LuaVariantEntry {
        id: Uuid::new_v4().to_string(),
        app_id,
        sha256,
        byte_length: bytes.len() as u64,
        origin: metadata.origin,
        capture_reason: metadata.reason,
        provider: metadata.provider,
        source: metadata.source,
        channel: metadata.channel,
        build_id: metadata.build_id,
        revision: metadata.revision,
        captured_at: Utc::now().to_rfc3339(),
        encoding_status: detect_encoding(bytes).to_string(),
        validation_status: metadata.validation_status,
        manifest_snapshot_identity: metadata.manifest_snapshot_identity,
        pinned: false,
    };
    let mut index = load_index(root)?;
    index.entries.push(entry.clone());
    save_index(root, &index)?;
    Ok(entry)
}

fn unknown_encoding_status() -> String {
    "unknown".to_string()
}

fn detect_encoding(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) && std::str::from_utf8(&bytes[3..]).is_ok() {
        "utf8Bom"
    } else if std::str::from_utf8(bytes).is_ok() {
        "utf8"
    } else {
        "nonUtf8"
    }
}

pub(crate) fn list(app: &AppHandle, app_id: u32) -> Result<Vec<LuaVariantEntry>, String> {
    let _guard = VAULT_LOCK
        .lock()
        .map_err(|_| "Lua variant vault is busy".to_string())?;
    let mut entries = load_index(&vault_root(app)?)?
        .entries
        .into_iter()
        .filter(|entry| entry.app_id == app_id)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.captured_at.cmp(&left.captured_at))
    });
    Ok(entries)
}

pub(crate) fn find(app: &AppHandle, app_id: u32, sha256: &str) -> Result<LuaVariantEntry, String> {
    validate_sha256(sha256)?;
    let _guard = VAULT_LOCK
        .lock()
        .map_err(|_| "Lua variant vault is busy".to_string())?;
    load_index(&vault_root(app)?)?
        .entries
        .into_iter()
        .rev()
        .find(|entry| entry.app_id == app_id && entry.sha256.eq_ignore_ascii_case(sha256.trim()))
        .ok_or_else(|| "Lua variant was not found for this AppID".to_string())
}

pub(crate) fn pin(
    app: &AppHandle,
    app_id: u32,
    sha256: &str,
    pinned: bool,
) -> Result<Vec<LuaVariantEntry>, String> {
    validate_sha256(sha256)?;
    let _guard = VAULT_LOCK
        .lock()
        .map_err(|_| "Lua variant vault is busy".to_string())?;
    let root = vault_root(app)?;
    let mut index = load_index(&root)?;
    let mut found = false;
    for entry in &mut index.entries {
        if entry.app_id == app_id && entry.sha256.eq_ignore_ascii_case(sha256.trim()) {
            entry.pinned = pinned;
            found = true;
        }
    }
    if !found {
        return Err("Lua variant was not found for this AppID".to_string());
    }
    save_index(&root, &index)?;
    let mut entries = index
        .entries
        .into_iter()
        .filter(|entry| entry.app_id == app_id)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.captured_at.cmp(&left.captured_at))
    });
    Ok(entries)
}

fn write_object_once(path: &Path, bytes: &[u8], expected: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(format!(
                    "Lua variant object is not a regular file: {}",
                    path.display()
                ));
            }
            let actual = hash_file(path)?;
            if actual != expected {
                return Err(format!(
                    "Lua variant object hash mismatch at {}",
                    path.display()
                ));
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect Lua variant object: {error}")),
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Could not create Lua variant object: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("Could not write Lua variant object: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Could not flush Lua variant object: {error}"))?;
    Ok(())
}

fn load_index(root: &Path) -> Result<LuaVariantIndex, String> {
    let path = root.join("index.json");
    if !path.exists() {
        return Ok(LuaVariantIndex::default());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read Lua variant index: {error}"))?;
    let index: LuaVariantIndex = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Lua variant index is invalid: {error}"))?;
    if index.schema_version != VAULT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Lua variant index schema {}",
            index.schema_version
        ));
    }
    Ok(index)
}

fn save_index(root: &Path, index: &LuaVariantIndex) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not prepare Lua variant vault: {error}"))?;
    let path = root.join("index.json");
    let next = root.join("index.json.next");
    let backup = root.join("index.json.bak");
    let bytes = serde_json::to_vec_pretty(index)
        .map_err(|error| format!("Could not serialize Lua variant index: {error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&next)
        .map_err(|error| format!("Could not create Lua variant index staging file: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("Could not write Lua variant index staging file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Could not flush Lua variant index staging file: {error}"))?;
    if path.is_file() {
        fs::copy(&path, &backup)
            .map_err(|error| format!("Could not preserve Lua variant index backup: {error}"))?;
    }
    replace_file(&next, &path)
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "Could not atomically replace Lua variant index: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target)
        .map_err(|error| format!("Could not atomically replace Lua variant index: {error}"))
}

fn validate_sha256(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Lua variant SHA-256 must contain 64 hexadecimal characters".to_string());
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Could not open Lua variant object: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash Lua variant object: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(status: LuaVariantValidationStatus) -> CaptureMetadata {
        CaptureMetadata {
            origin: LuaVariantOrigin::Active,
            reason: LuaVariantCaptureReason::BeforeUpdate,
            provider: Some("fixture".to_string()),
            source: None,
            channel: Some("live".to_string()),
            build_id: None,
            revision: None,
            validation_status: status,
            manifest_snapshot_identity: None,
        }
    }

    fn cleanup(root: &Path) {
        for name in ["index.json", "index.json.next", "index.json.bak"] {
            fs::remove_file(root.join(name)).ok();
        }
        let objects = root.join("objects");
        if let Ok(entries) = fs::read_dir(&objects) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    fs::remove_file(entry.path()).ok();
                }
            }
        }
        fs::remove_dir(objects).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn preserves_exact_bytes_and_deduplicates_objects() {
        let root = std::env::temp_dir().join(format!("0xo-lua-vault-{}", Uuid::new_v4()));
        let bytes = b"\xEF\xBB\xBFaddappid(42)\r\n-- \xFF\r\n";
        let first = capture_bytes_at(
            &root,
            42,
            bytes,
            metadata(LuaVariantValidationStatus::RecoveryOnly),
        )
        .unwrap();
        let second = capture_bytes_at(
            &root,
            42,
            bytes,
            metadata(LuaVariantValidationStatus::RecoveryOnly),
        )
        .unwrap();

        assert_eq!(first.sha256, second.sha256);
        assert_ne!(first.id, second.id);
        assert_eq!(
            fs::read(root.join("objects").join(format!("{}.lua", first.sha256))).unwrap(),
            bytes
        );
        assert_eq!(load_index(&root).unwrap().entries.len(), 2);
        assert_eq!(fs::read_dir(root.join("objects")).unwrap().count(), 1);
        assert_eq!(first.encoding_status, "nonUtf8");
        cleanup(&root);
    }

    #[test]
    fn records_utf8_bom_encoding_without_normalizing_bytes() {
        let root = std::env::temp_dir().join(format!("0xo-lua-vault-{}", Uuid::new_v4()));
        let bytes = b"\xEF\xBB\xBFaddappid(42)\r\n";
        let entry = capture_bytes_at(
            &root,
            42,
            bytes,
            metadata(LuaVariantValidationStatus::Valid),
        )
        .unwrap();
        assert_eq!(entry.encoding_status, "utf8Bom");
        assert_eq!(
            fs::read(root.join("objects").join(format!("{}.lua", entry.sha256))).unwrap(),
            bytes
        );
        cleanup(&root);
    }
}
