use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use zip::ZipArchive;

const MAX_IMPORT_SOURCES: usize = 32;
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4_096;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_LUA_BYTES: usize = 8 * 1024 * 1024;
const STEAM_MANIFEST_MAGIC: [u8; 4] = [0xd0, 0x17, 0xf6, 0x71];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameToolsImportResult {
    pub transaction_id: String,
    pub installed_files: usize,
    pub app_ids: Vec<u32>,
    pub depot_ids: Vec<u32>,
    pub requires_steam_restart: bool,
}

#[derive(Debug)]
struct StagedImport {
    source: PathBuf,
    file_name: String,
    destination: ImportDestination,
    sha256: String,
}

#[derive(Debug, Clone, Copy)]
enum ImportDestination {
    Lua,
    Manifest,
}

pub async fn import_files(
    app: AppHandle,
    paths: Option<Vec<String>>,
) -> Result<Option<GameToolsImportResult>, String> {
    tauri::async_runtime::spawn_blocking(move || import_files_blocking(&app, paths))
        .await
        .map_err(|error| format!("GAME_TOOLS_IMPORT_TASK_FAILED: {error}"))?
}

fn import_files_blocking(
    app: &AppHandle,
    paths: Option<Vec<String>>,
) -> Result<Option<GameToolsImportResult>, String> {
    let paths = match paths {
        Some(paths) => paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        None => {
            let Some(selected) = app
                .dialog()
                .file()
                .set_title("Import Steam Lua, manifest or ZIP package")
                .add_filter("Steam package", &["lua", "manifest", "zip"])
                .blocking_pick_files()
            else {
                return Ok(None);
            };
            selected
                .into_iter()
                .map(|path| {
                    path.into_path()
                        .map_err(|error| format!("GAME_TOOLS_IMPORT_PATH_INVALID: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    if paths.is_empty() || paths.len() > MAX_IMPORT_SOURCES {
        return Err("GAME_TOOLS_IMPORT_SOURCE_COUNT_INVALID".to_string());
    }

    let steam_root = crate::steam::get_steam_path()
        .ok_or_else(|| "GAME_TOOLS_STEAM_NOT_FOUND".to_string())?
        .canonicalize()
        .map_err(|error| format!("GAME_TOOLS_STEAM_PATH_INVALID: {error}"))?;
    let lua_root = steam_root.join("config").join("stplug-in");
    let manifest_root = steam_root.join("depotcache");
    fs::create_dir_all(&lua_root)
        .and_then(|_| fs::create_dir_all(&manifest_root))
        .map_err(|error| format!("GAME_TOOLS_IMPORT_TARGET_CREATE_FAILED: {error}"))?;

    let stage_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("GAME_TOOLS_IMPORT_STAGE_UNAVAILABLE: {error}"))?
        .join("game-tools-import")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&stage_root)
        .map_err(|error| format!("GAME_TOOLS_IMPORT_STAGE_CREATE_FAILED: {error}"))?;

    let mut staged = Vec::new();
    let mut app_ids = HashSet::new();
    let mut depot_ids = HashSet::new();
    let result = (|| {
        let mut source_keys = HashSet::new();
        for source in paths {
            let canonical = validate_source_path(&source)?;
            let key = canonical.to_string_lossy().to_ascii_lowercase();
            if !source_keys.insert(key) {
                return Err("GAME_TOOLS_IMPORT_DUPLICATE_SOURCE".to_string());
            }
            let metadata = fs::metadata(&canonical)
                .map_err(|error| format!("GAME_TOOLS_IMPORT_SOURCE_INVALID: {error}"))?;
            if metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
                return Err("GAME_TOOLS_IMPORT_SOURCE_SIZE_INVALID".to_string());
            }
            let extension = canonical
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            match extension.as_str() {
                "lua" | "manifest" => {
                    let bytes = fs::read(&canonical)
                        .map_err(|error| format!("GAME_TOOLS_IMPORT_READ_FAILED: {error}"))?;
                    stage_payload(
                        &stage_root,
                        canonical
                            .file_name()
                            .and_then(|value| value.to_str())
                            .ok_or_else(|| "GAME_TOOLS_IMPORT_FILE_NAME_INVALID".to_string())?,
                        &bytes,
                        &mut staged,
                        &mut app_ids,
                        &mut depot_ids,
                    )?;
                }
                "zip" => stage_zip(
                    &canonical,
                    &stage_root,
                    &mut staged,
                    &mut app_ids,
                    &mut depot_ids,
                )?,
                _ => return Err("GAME_TOOLS_IMPORT_TYPE_REJECTED".to_string()),
            }
        }
        if staged.is_empty() {
            return Err("GAME_TOOLS_IMPORT_PAYLOAD_EMPTY".to_string());
        }

        let mut target_keys = HashSet::new();
        let specs = staged
            .iter()
            .map(|item| {
                let allowed_target_root = match item.destination {
                    ImportDestination::Lua => lua_root.clone(),
                    ImportDestination::Manifest => manifest_root.clone(),
                };
                let target = allowed_target_root.join(&item.file_name);
                let key = target.to_string_lossy().to_ascii_lowercase();
                if !target_keys.insert(key) {
                    return Err("GAME_TOOLS_IMPORT_DUPLICATE_TARGET".to_string());
                }
                Ok(crate::managed_file_transaction::ManagedFileSpec {
                    source: item.source.clone(),
                    target,
                    allowed_source_root: stage_root.clone(),
                    allowed_target_root,
                    expected_sha256: item.sha256.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        crate::managed_file_transaction::apply_files(app, "game-tools-import", specs)
    })();

    cleanup_stage_exact(&stage_root, &staged);
    let receipt = result?;

    let mut app_ids = app_ids.into_iter().collect::<Vec<_>>();
    let mut depot_ids = depot_ids.into_iter().collect::<Vec<_>>();
    app_ids.sort_unstable();
    depot_ids.sort_unstable();
    Ok(Some(GameToolsImportResult {
        transaction_id: receipt.transaction_id,
        installed_files: staged.len(),
        app_ids,
        depot_ids,
        requires_steam_restart: crate::steam_integration::is_steam_running(),
    }))
}

fn validate_source_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("GAME_TOOLS_IMPORT_PATH_MUST_BE_ABSOLUTE".to_string());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("GAME_TOOLS_IMPORT_SOURCE_INVALID: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("GAME_TOOLS_IMPORT_SOURCE_NOT_REGULAR".to_string());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x0000_0400 != 0 {
            return Err("GAME_TOOLS_IMPORT_SOURCE_REPARSE_REJECTED".to_string());
        }
    }
    path.canonicalize()
        .map_err(|error| format!("GAME_TOOLS_IMPORT_SOURCE_INVALID: {error}"))
}

fn stage_zip(
    source: &Path,
    stage_root: &Path,
    staged: &mut Vec<StagedImport>,
    app_ids: &mut HashSet<u32>,
    depot_ids: &mut HashSet<u32>,
) -> Result<(), String> {
    let mut magic = [0u8; 4];
    File::open(source)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|error| format!("GAME_TOOLS_IMPORT_ZIP_INVALID: {error}"))?;
    if !matches!(
        magic,
        [0x50, 0x4b, 0x03, 0x04] | [0x50, 0x4b, 0x05, 0x06] | [0x50, 0x4b, 0x07, 0x08]
    ) {
        return Err("GAME_TOOLS_IMPORT_ZIP_MAGIC_INVALID".to_string());
    }
    let file =
        File::open(source).map_err(|error| format!("GAME_TOOLS_IMPORT_ZIP_INVALID: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("GAME_TOOLS_IMPORT_ZIP_INVALID: {error}"))?;
    if archive.len() == 0 || archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("GAME_TOOLS_IMPORT_ZIP_ENTRY_COUNT_INVALID".to_string());
    }

    let mut expanded = 0u64;
    let mut entry_keys = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("GAME_TOOLS_IMPORT_ZIP_INVALID: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("GAME_TOOLS_IMPORT_ZIP_SYMLINK_REJECTED".to_string());
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "GAME_TOOLS_IMPORT_ZIP_TRAVERSAL_REJECTED".to_string())?;
        let file_name = enclosed
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "GAME_TOOLS_IMPORT_FILE_NAME_INVALID".to_string())?;
        let extension = Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("lua") && !extension.eq_ignore_ascii_case("manifest") {
            return Err("GAME_TOOLS_IMPORT_ZIP_ENTRY_TYPE_REJECTED".to_string());
        }
        let key = file_name.to_ascii_lowercase();
        if !entry_keys.insert(key) {
            return Err("GAME_TOOLS_IMPORT_ZIP_DUPLICATE_PATH".to_string());
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| "GAME_TOOLS_IMPORT_ZIP_SIZE_OVERFLOW".to_string())?;
        if expanded > MAX_EXPANDED_BYTES || entry.size() > MAX_SOURCE_BYTES {
            return Err("GAME_TOOLS_IMPORT_ZIP_SIZE_INVALID".to_string());
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("GAME_TOOLS_IMPORT_ZIP_READ_FAILED: {error}"))?;
        stage_payload(stage_root, file_name, &bytes, staged, app_ids, depot_ids)?;
    }
    Ok(())
}

fn stage_payload(
    stage_root: &Path,
    file_name: &str,
    bytes: &[u8],
    staged: &mut Vec<StagedImport>,
    app_ids: &mut HashSet<u32>,
    depot_ids: &mut HashSet<u32>,
) -> Result<(), String> {
    if Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        != Some(file_name)
        || file_name.contains(['/', '\\', '\0'])
    {
        return Err("GAME_TOOLS_IMPORT_FILE_NAME_INVALID".to_string());
    }
    if staged
        .iter()
        .any(|item| item.file_name.eq_ignore_ascii_case(file_name))
    {
        return Err("GAME_TOOLS_IMPORT_DUPLICATE_TARGET".to_string());
    }
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let destination = if extension.eq_ignore_ascii_case("lua") {
        let app_id = validate_lua(file_name, bytes)?;
        app_ids.insert(app_id);
        ImportDestination::Lua
    } else if extension.eq_ignore_ascii_case("manifest") {
        let depot_id = validate_manifest(file_name, bytes)?;
        depot_ids.insert(depot_id);
        ImportDestination::Manifest
    } else {
        return Err("GAME_TOOLS_IMPORT_TYPE_REJECTED".to_string());
    };

    let staged_path = stage_root.join(format!("{:04}-{file_name}", staged.len()));
    fs::write(&staged_path, bytes)
        .map_err(|error| format!("GAME_TOOLS_IMPORT_STAGE_WRITE_FAILED: {error}"))?;
    staged.push(StagedImport {
        source: staged_path,
        file_name: file_name.to_string(),
        destination,
        sha256: hex::encode(Sha256::digest(bytes)),
    });
    Ok(())
}

fn validate_lua(file_name: &str, bytes: &[u8]) -> Result<u32, String> {
    if bytes.is_empty() || bytes.len() > MAX_LUA_BYTES || bytes.contains(&0) {
        return Err("GAME_TOOLS_IMPORT_LUA_SIZE_OR_MAGIC_INVALID".to_string());
    }
    let app_id = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "GAME_TOOLS_IMPORT_LUA_APP_ID_INVALID".to_string())?;
    let source =
        std::str::from_utf8(bytes).map_err(|_| "GAME_TOOLS_IMPORT_LUA_UTF8_INVALID".to_string())?;
    let compact = source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if !compact.contains(&format!("addappid({app_id}")) {
        return Err("GAME_TOOLS_IMPORT_LUA_APP_ID_MISMATCH".to_string());
    }
    Ok(app_id)
}

fn validate_manifest(file_name: &str, bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < 8 || bytes[..4] != STEAM_MANIFEST_MAGIC {
        return Err("GAME_TOOLS_IMPORT_MANIFEST_MAGIC_INVALID".to_string());
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "GAME_TOOLS_IMPORT_MANIFEST_NAME_INVALID".to_string())?;
    let mut parts = stem.split('_');
    let depot_id = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "GAME_TOOLS_IMPORT_MANIFEST_DEPOT_ID_INVALID".to_string())?;
    let manifest_id = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "GAME_TOOLS_IMPORT_MANIFEST_ID_INVALID".to_string())?;
    if parts.next().is_some() || manifest_id == 0 {
        return Err("GAME_TOOLS_IMPORT_MANIFEST_NAME_INVALID".to_string());
    }
    Ok(depot_id)
}

fn cleanup_stage_exact(stage_root: &Path, staged: &[StagedImport]) {
    for item in staged {
        if item.source.starts_with(stage_root) {
            let _ = fs::remove_file(&item.source);
        }
    }
    let _ = fs::remove_dir(stage_root);
    if let Some(parent) = stage_root.parent() {
        let _ = fs::remove_dir(parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_requires_filename_app_id_in_payload() {
        assert_eq!(
            validate_lua("1174180.lua", b"addappid(1174180)\n").unwrap(),
            1174180
        );
        assert!(validate_lua("1174180.lua", b"addappid(1196590)\n").is_err());
        assert!(validate_lua("game.lua", b"addappid(1174180)\n").is_err());
    }

    #[test]
    fn manifest_requires_steam_magic_and_numeric_identity() {
        let mut bytes = STEAM_MANIFEST_MAGIC.to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(
            validate_manifest("1174180_1814527371043186154.manifest", &bytes).unwrap(),
            1174180
        );
        assert!(validate_manifest("1174180_bad.manifest", &bytes).is_err());
        assert!(validate_manifest("1174180_1.manifest", b"PK\x03\x04bad").is_err());
    }
}
