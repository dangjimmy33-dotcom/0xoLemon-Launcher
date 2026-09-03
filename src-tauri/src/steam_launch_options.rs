use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const MAGIC_V27: u32 = 0x0756_4427;
const MAGIC_V28: u32 = 0x0756_4428;
const MAGIC_V29: u32 = 0x0756_4429;
const STORE_SCHEMA_VERSION: u32 = 2;
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const MAX_STRING_BYTES: usize = 16 * 1024 * 1024;
const MAX_STRING_TABLE_ENTRIES: u32 = 2_000_000;
const MAX_LAUNCH_OPTIONS: usize = 128;
const MAX_OPTION_FIELD_BYTES: usize = 32 * 1024;
const BACKUP_PREFIX: &str = ".0xolemon-appinfo-backup-";
const STAGE_PREFIX: &str = ".0xolemon-appinfo-stage-";

static APPINFO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SteamAppInfoFormat {
    V27,
    V28,
    V29,
}

impl SteamAppInfoFormat {
    fn from_magic(magic: u32) -> Result<Self, String> {
        match magic {
            MAGIC_V27 => Ok(Self::V27),
            MAGIC_V28 => Ok(Self::V28),
            MAGIC_V29 => Ok(Self::V29),
            _ => Err(format!(
                "UNSUPPORTED_APPINFO_FORMAT: refusing to write appinfo magic 0x{magic:08X}"
            )),
        }
    }

    fn magic(self) -> u32 {
        match self {
            Self::V27 => MAGIC_V27,
            Self::V28 => MAGIC_V28,
            Self::V29 => MAGIC_V29,
        }
    }

    fn meta_len(self) -> u64 {
        if self == Self::V27 {
            40
        } else {
            60
        }
    }

    fn indexed_keys(self) -> bool {
        self == Self::V29
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchOption {
    pub index: String,
    #[serde(default)]
    pub source_index: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub launch_type: String,
    #[serde(default)]
    pub os_list: String,
    #[serde(default)]
    pub os_arch: String,
    #[serde(default)]
    pub beta_key: String,
    #[serde(default)]
    pub owns_dlc: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SteamLaunchDriftState {
    #[default]
    Staged,
    Applied,
    Drifted,
    RebaseReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchMod {
    pub app_id: u32,
    pub change_number: u32,
    pub original: Vec<SteamLaunchOption>,
    pub desired: Vec<SteamLaunchOption>,
    pub source_file_hash: String,
    pub saved_at: String,
    pub applied_at: Option<String>,
    #[serde(default)]
    pub drift_state: SteamLaunchDriftState,
    #[serde(default)]
    pub initial_baseline: Option<SteamLaunchBaseline>,
    #[serde(default)]
    pub latest_steam_baseline: Option<SteamLaunchBaseline>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchBaseline {
    pub change_number: u32,
    pub source_file_hash: String,
    pub options: Vec<SteamLaunchOption>,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchAuditReceipt {
    pub receipt_id: String,
    pub app_ids: Vec<u32>,
    pub operation: String,
    pub transaction_id: String,
    pub before_file_hash: String,
    pub after_file_hash: String,
    pub retained_backup_sha256: String,
    pub initial_baselines: BTreeMap<u32, SteamLaunchBaseline>,
    pub latest_steam_baselines: BTreeMap<u32, SteamLaunchBaseline>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchState {
    pub app_id: u32,
    pub format: SteamAppInfoFormat,
    pub change_number: u32,
    pub current: Vec<SteamLaunchOption>,
    pub install_dir: Option<String>,
    pub is_modded: bool,
    pub steam_running: bool,
    pub drift_state: Option<SteamLaunchDriftState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchApplyRequest {
    pub app_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchApplyResult {
    pub ok: bool,
    pub applied_app_ids: Vec<u32>,
    pub backup_sha256: Option<String>,
    pub transaction_id: Option<String>,
    pub audit_receipt: Option<SteamLaunchAuditReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchModStore {
    schema_version: u32,
    mods: BTreeMap<u32, SteamLaunchMod>,
    #[serde(default)]
    audit_receipts: Vec<SteamLaunchAuditReceipt>,
}

impl Default for LaunchModStore {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            mods: BTreeMap::new(),
            audit_receipts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct AppInfoEntry {
    app_id: u32,
    offset: u64,
    size: u32,
    change_number: u32,
}

#[derive(Debug, Clone)]
struct AppInfoIndex {
    path: PathBuf,
    format: SteamAppInfoFormat,
    universe: u32,
    entries: Vec<AppInfoEntry>,
    strings: Vec<Vec<u8>>,
    pre_table_tail: Vec<u8>,
    post_table_tail: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VdfKey {
    raw: Vec<u8>,
    index: Option<u32>,
}

impl VdfKey {
    fn new_ascii(value: &str) -> Self {
        Self {
            raw: value.as_bytes().to_vec(),
            index: None,
        }
    }

    fn matches(&self, expected: &str) -> bool {
        self.raw.eq_ignore_ascii_case(expected.as_bytes())
    }

    fn display_lossy(&self) -> String {
        String::from_utf8_lossy(&self.raw).to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum VdfValue {
    Table(VdfTable),
    CString(Vec<u8>),
    WString(Vec<u8>),
    Fixed(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
struct VdfProperty {
    marker: u8,
    key: VdfKey,
    value: VdfValue,
}

#[derive(Debug, Clone, PartialEq)]
struct VdfTable {
    items: Vec<VdfProperty>,
    end_marker: u8,
}

impl Default for VdfTable {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            end_marker: 0x08,
        }
    }
}

impl VdfTable {
    fn table(&self, name: &str) -> Option<&VdfTable> {
        self.items.iter().find_map(|item| {
            if item.key.matches(name) {
                if let VdfValue::Table(table) = &item.value {
                    return Some(table);
                }
            }
            None
        })
    }

    fn table_mut(&mut self, name: &str) -> Option<&mut VdfTable> {
        self.items.iter_mut().find_map(|item| {
            if item.key.matches(name) {
                if let VdfValue::Table(table) = &mut item.value {
                    return Some(table);
                }
            }
            None
        })
    }

    fn ensure_table(&mut self, name: &str) -> &mut VdfTable {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.key.matches(name) && matches!(item.value, VdfValue::Table(_)))
        {
            return match &mut self.items[index].value {
                VdfValue::Table(table) => table,
                _ => unreachable!(),
            };
        }
        self.items.push(VdfProperty {
            marker: 0,
            key: VdfKey::new_ascii(name),
            value: VdfValue::Table(VdfTable::default()),
        });
        match &mut self.items.last_mut().expect("just inserted").value {
            VdfValue::Table(table) => table,
            _ => unreachable!(),
        }
    }

    fn text_bytes(&self, name: &str) -> Option<&[u8]> {
        self.items.iter().find_map(|item| {
            if item.key.matches(name) {
                if let VdfValue::CString(bytes) = &item.value {
                    return Some(bytes.as_slice());
                }
            }
            None
        })
    }

    fn text(&self, name: &str) -> Option<String> {
        self.text_bytes(name)
            .map(|value| String::from_utf8_lossy(value).to_string())
    }

    fn set_text(&mut self, name: &str, value: &str) {
        if value.is_empty() {
            self.items.retain(|item| !item.key.matches(name));
            return;
        }
        if let Some(item) = self.items.iter_mut().find(|item| item.key.matches(name)) {
            if matches!(item.value, VdfValue::CString(_)) {
                item.marker = 1;
                item.value = VdfValue::CString(value.as_bytes().to_vec());
                return;
            }
        }
        self.items.push(VdfProperty {
            marker: 1,
            key: VdfKey::new_ascii(name),
            value: VdfValue::CString(value.as_bytes().to_vec()),
        });
    }

    fn replace_table(&mut self, name: &str, replacement: VdfTable) {
        let insert_at = self
            .items
            .iter()
            .position(|item| item.key.matches(name))
            .unwrap_or(self.items.len());
        self.items.retain(|item| !item.key.matches(name));
        self.items.insert(
            insert_at.min(self.items.len()),
            VdfProperty {
                marker: 0,
                key: VdfKey::new_ascii(name),
                value: VdfValue::Table(replacement),
            },
        );
    }
}

#[derive(Debug, Clone)]
struct ParsedRecord {
    root: VdfTable,
    trailing: Vec<u8>,
}

impl AppInfoIndex {
    fn open(path: &Path) -> Result<Self, String> {
        let mut reader = BufReader::new(
            File::open(path)
                .map_err(|error| format!("Could not open {}: {error}", path.display()))?,
        );
        let file_len = reader
            .get_ref()
            .metadata()
            .map_err(|error| format!("Could not inspect appinfo.vdf: {error}"))?
            .len();
        if file_len < 12 {
            return Err("appinfo.vdf is truncated before its header".to_string());
        }
        let magic = read_u32(&mut reader)?;
        let format = SteamAppInfoFormat::from_magic(magic)?;
        let universe = read_u32(&mut reader)?;
        let table_offset = if format.indexed_keys() {
            let value = read_i64(&mut reader)?;
            if value < 16 || value as u64 > file_len.saturating_sub(4) {
                return Err("appinfo.vdf string-table offset is outside the file".to_string());
            }
            Some(value as u64)
        } else {
            None
        };
        let apps_start = reader.stream_position().map_err(io_error)?;
        let records_end = table_offset.unwrap_or(file_len);
        let mut entries = Vec::new();
        let sentinel_end = loop {
            let offset = reader.stream_position().map_err(io_error)?;
            if offset.saturating_add(4) > records_end {
                return Err("appinfo.vdf has no AppID terminator".to_string());
            }
            let app_id = read_u32(&mut reader)?;
            if app_id == 0 {
                break reader.stream_position().map_err(io_error)?;
            }
            if reader
                .stream_position()
                .map_err(io_error)?
                .saturating_add(4)
                > records_end
            {
                return Err("appinfo.vdf ended before an app record size".to_string());
            }
            let size = read_u32(&mut reader)?;
            if (size as u64) < format.meta_len() {
                return Err(format!(
                    "appinfo record {app_id} has an invalid size {size}"
                ));
            }
            let record_end = offset
                .checked_add(8)
                .and_then(|value| value.checked_add(size as u64))
                .ok_or_else(|| "appinfo record boundary overflow".to_string())?;
            if record_end > records_end {
                return Err(format!("appinfo record {app_id} crosses the record region"));
            }
            reader
                .seek(SeekFrom::Start(offset + 8 + 36))
                .map_err(io_error)?;
            let change_number = read_u32(&mut reader)?;
            entries.push(AppInfoEntry {
                app_id,
                offset,
                size,
                change_number,
            });
            reader.seek(SeekFrom::Start(record_end)).map_err(io_error)?;
        };
        let mut ids = BTreeSet::new();
        if entries.iter().any(|entry| !ids.insert(entry.app_id)) {
            return Err("appinfo.vdf contains duplicate AppID records".to_string());
        }

        let (strings, pre_table_tail, post_table_tail) = if let Some(table_offset) = table_offset {
            if sentinel_end > table_offset {
                return Err("appinfo.vdf terminator crosses its string table".to_string());
            }
            reader
                .seek(SeekFrom::Start(sentinel_end))
                .map_err(io_error)?;
            let pre_table_tail =
                read_exact_vec(&mut reader, (table_offset - sentinel_end) as usize)?;
            reader
                .seek(SeekFrom::Start(table_offset))
                .map_err(io_error)?;
            let count = read_u32(&mut reader)?;
            if count > MAX_STRING_TABLE_ENTRIES {
                return Err("appinfo.vdf string table is unreasonably large".to_string());
            }
            let mut strings = Vec::with_capacity(count as usize);
            for _ in 0..count {
                strings.push(read_cstring(&mut reader)?);
            }
            let position = reader.stream_position().map_err(io_error)?;
            let post_table_tail = read_exact_vec(&mut reader, (file_len - position) as usize)?;
            (strings, pre_table_tail, post_table_tail)
        } else {
            reader
                .seek(SeekFrom::Start(sentinel_end))
                .map_err(io_error)?;
            (
                Vec::new(),
                Vec::new(),
                read_exact_vec(&mut reader, (file_len - sentinel_end) as usize)?,
            )
        };

        if apps_start != if format.indexed_keys() { 16 } else { 8 } {
            return Err("appinfo.vdf header boundary is invalid".to_string());
        }
        Ok(Self {
            path: path.to_path_buf(),
            format,
            universe,
            entries,
            strings,
            pre_table_tail,
            post_table_tail,
        })
    }

    fn entry(&self, app_id: u32) -> Option<&AppInfoEntry> {
        self.entries.iter().find(|entry| entry.app_id == app_id)
    }

    fn read_record(&self, app_id: u32) -> Result<ParsedRecord, String> {
        let entry = self
            .entry(app_id)
            .ok_or_else(|| format!("AppID {app_id} was not found in appinfo.vdf"))?;
        let blob_len = entry.size as u64 - self.format.meta_len();
        let mut file = BufReader::new(File::open(&self.path).map_err(io_error)?);
        file.seek(SeekFrom::Start(entry.offset + 8 + self.format.meta_len()))
            .map_err(io_error)?;
        let blob = read_exact_vec(
            &mut file,
            usize::try_from(blob_len)
                .map_err(|_| format!("AppID {app_id} blob is too large for this platform"))?,
        )?;
        let mut cursor = Cursor::new(blob.as_slice());
        let root = read_vdf_table(&mut cursor, &self.strings, self.format.indexed_keys())?;
        let consumed = cursor.position() as usize;
        Ok(ParsedRecord {
            root,
            trailing: blob[consumed..].to_vec(),
        })
    }

    fn save_as(
        &self,
        destination: &Path,
        edits: &BTreeMap<u32, ParsedRecord>,
    ) -> Result<(), String> {
        for app_id in edits.keys() {
            if self.entry(*app_id).is_none() {
                return Err(format!("AppID {app_id} disappeared from appinfo.vdf"));
            }
        }
        let mut source = BufReader::new(File::open(&self.path).map_err(io_error)?);
        let mut output = BufWriter::with_capacity(
            COPY_BUFFER_BYTES,
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(destination)
                .map_err(|error| format!("Could not create appinfo staging file: {error}"))?,
        );
        write_u32(&mut output, self.format.magic())?;
        write_u32(&mut output, self.universe)?;
        let table_offset_position = if self.format.indexed_keys() {
            let position = output.stream_position().map_err(io_error)?;
            write_i64(&mut output, 0)?;
            Some(position)
        } else {
            None
        };

        let mut strings = self.strings.clone();
        for entry in &self.entries {
            if let Some(edit) = edits.get(&entry.app_id) {
                source
                    .seek(SeekFrom::Start(entry.offset + 8))
                    .map_err(io_error)?;
                let mut meta = read_exact_vec(&mut source, self.format.meta_len() as usize)?;
                let mut blob = Vec::new();
                write_vdf_table(
                    &mut blob,
                    &edit.root,
                    &mut strings,
                    self.format.indexed_keys(),
                )?;
                blob.extend_from_slice(&edit.trailing);
                if self.format != SteamAppInfoFormat::V27 {
                    let digest = Sha1::digest(&blob);
                    meta[40..60].copy_from_slice(digest.as_slice());
                }
                let size = meta
                    .len()
                    .checked_add(blob.len())
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| format!("AppID {} record is too large", entry.app_id))?;
                write_u32(&mut output, entry.app_id)?;
                write_u32(&mut output, size)?;
                output.write_all(&meta).map_err(io_error)?;
                output.write_all(&blob).map_err(io_error)?;
            } else {
                source
                    .seek(SeekFrom::Start(entry.offset))
                    .map_err(io_error)?;
                copy_exact(&mut source, &mut output, 8 + entry.size as u64)?;
            }
        }
        write_u32(&mut output, 0)?;
        if self.format.indexed_keys() {
            output.write_all(&self.pre_table_tail).map_err(io_error)?;
            let table_offset = output.stream_position().map_err(io_error)?;
            write_u32(
                &mut output,
                u32::try_from(strings.len())
                    .map_err(|_| "appinfo string table grew too large".to_string())?,
            )?;
            for string in &strings {
                write_cstring(&mut output, string)?;
            }
            output.write_all(&self.post_table_tail).map_err(io_error)?;
            output.flush().map_err(io_error)?;
            output
                .seek(SeekFrom::Start(
                    table_offset_position.expect("indexed header"),
                ))
                .map_err(io_error)?;
            write_i64(
                &mut output,
                i64::try_from(table_offset)
                    .map_err(|_| "appinfo string-table offset exceeds i64".to_string())?,
            )?;
        } else {
            output.write_all(&self.post_table_tail).map_err(io_error)?;
        }
        output.flush().map_err(io_error)?;
        output.get_ref().sync_all().map_err(io_error)
    }
}

fn read_vdf_table<R: Read>(
    reader: &mut R,
    strings: &[Vec<u8>],
    indexed_keys: bool,
) -> Result<VdfTable, String> {
    let mut table = VdfTable::default();
    loop {
        let marker = read_u8(reader)?;
        if marker == 0x08 || marker == 0x0B {
            table.end_marker = marker;
            return Ok(table);
        }
        let key = if indexed_keys {
            let index = read_u32(reader)?;
            let raw = strings
                .get(index as usize)
                .ok_or_else(|| format!("VDF key index {index} is outside the string table"))?
                .clone();
            VdfKey {
                raw,
                index: Some(index),
            }
        } else {
            VdfKey {
                raw: read_cstring(reader)?,
                index: None,
            }
        };
        let value = match marker {
            0x00 => VdfValue::Table(read_vdf_table(reader, strings, indexed_keys)?),
            0x01 => VdfValue::CString(read_cstring(reader)?),
            0x02 | 0x03 | 0x04 | 0x06 => VdfValue::Fixed(read_exact_vec(reader, 4)?),
            0x05 => VdfValue::WString(read_wstring(reader)?),
            0x07 | 0x0A => VdfValue::Fixed(read_exact_vec(reader, 8)?),
            0x09 => VdfValue::Fixed(read_exact_vec(reader, 1)?),
            0x0C | 0x0D => VdfValue::Fixed(Vec::new()),
            _ => {
                return Err(format!(
                    "Unsupported binary VDF value marker 0x{marker:02X}; refusing to guess its boundary"
                ))
            }
        };
        table.items.push(VdfProperty { marker, key, value });
    }
}

fn write_vdf_table<W: Write>(
    writer: &mut W,
    table: &VdfTable,
    strings: &mut Vec<Vec<u8>>,
    indexed_keys: bool,
) -> Result<(), String> {
    for property in &table.items {
        writer.write_all(&[property.marker]).map_err(io_error)?;
        if indexed_keys {
            let index = if let Some(index) = property.key.index {
                if strings.get(index as usize) != Some(&property.key.raw) {
                    return Err("VDF key string-table identity changed unexpectedly".to_string());
                }
                index
            } else if let Some(index) = strings.iter().position(|value| value == &property.key.raw)
            {
                index as u32
            } else {
                strings.push(property.key.raw.clone());
                (strings.len() - 1) as u32
            };
            write_u32(writer, index)?;
        } else {
            write_cstring(writer, &property.key.raw)?;
        }
        match &property.value {
            VdfValue::Table(value) => write_vdf_table(writer, value, strings, indexed_keys)?,
            VdfValue::CString(value) => write_cstring(writer, value)?,
            VdfValue::WString(value) => {
                writer.write_all(value).map_err(io_error)?;
                writer.write_all(&[0, 0]).map_err(io_error)?;
            }
            VdfValue::Fixed(value) => writer.write_all(value).map_err(io_error)?,
        }
    }
    writer.write_all(&[table.end_marker]).map_err(io_error)
}

fn read_launch_options(body: &VdfTable) -> Vec<SteamLaunchOption> {
    let Some(launch) = body.table("config").and_then(|table| table.table("launch")) else {
        return Vec::new();
    };
    launch
        .items
        .iter()
        .filter_map(|property| {
            let VdfValue::Table(table) = &property.value else {
                return None;
            };
            let config = table.table("config");
            Some(SteamLaunchOption {
                index: property.key.display_lossy(),
                source_index: property.key.display_lossy(),
                executable: table.text("executable").unwrap_or_default(),
                arguments: table.text("arguments").unwrap_or_default(),
                working_dir: table.text("workingdir").unwrap_or_default(),
                description: table.text("description").unwrap_or_default(),
                launch_type: table.text("type").unwrap_or_default(),
                os_list: config
                    .and_then(|value| value.text("oslist"))
                    .unwrap_or_default(),
                os_arch: config
                    .and_then(|value| value.text("osarch"))
                    .unwrap_or_default(),
                beta_key: config
                    .and_then(|value| value.text("betakey"))
                    .unwrap_or_default(),
                owns_dlc: config
                    .and_then(|value| value.text("ownsdlc"))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn write_launch_options(body: &mut VdfTable, options: &[SteamLaunchOption]) {
    let config = body.ensure_table("config");
    let previous = config.table("launch").cloned();
    let mut launch = VdfTable::default();
    for option in options {
        let source_index = if option.source_index.is_empty() {
            option.index.as_str()
        } else {
            option.source_index.as_str()
        };
        let mut table = previous
            .as_ref()
            .and_then(|previous| {
                previous.items.iter().find_map(|property| {
                    if property.key.matches(source_index) {
                        if let VdfValue::Table(table) = &property.value {
                            return Some(table.clone());
                        }
                    }
                    None
                })
            })
            .unwrap_or_default();
        table.set_text("executable", &option.executable);
        table.set_text("arguments", &option.arguments);
        table.set_text("workingdir", &option.working_dir);
        table.set_text("description", &option.description);
        table.set_text("type", &option.launch_type);
        let needs_config = !option.os_list.is_empty()
            || !option.os_arch.is_empty()
            || !option.beta_key.is_empty()
            || !option.owns_dlc.is_empty();
        if needs_config {
            let option_config = table.ensure_table("config");
            option_config.set_text("oslist", &option.os_list);
            option_config.set_text("osarch", &option.os_arch);
            option_config.set_text("betakey", &option.beta_key);
            option_config.set_text("ownsdlc", &option.owns_dlc);
        } else if let Some(option_config) = table.table_mut("config") {
            option_config.set_text("oslist", "");
            option_config.set_text("osarch", "");
            option_config.set_text("betakey", "");
            option_config.set_text("ownsdlc", "");
        }
        launch.items.push(VdfProperty {
            marker: 0,
            key: VdfKey::new_ascii(&option.index),
            value: VdfValue::Table(table),
        });
    }
    config.replace_table("launch", launch);
}

fn app_body(record: &ParsedRecord) -> Result<&VdfTable, String> {
    record
        .root
        .table("appinfo")
        .ok_or_else(|| "appinfo record has no appinfo body".to_string())
}

fn app_body_mut(record: &mut ParsedRecord) -> Result<&mut VdfTable, String> {
    record
        .root
        .table_mut("appinfo")
        .ok_or_else(|| "appinfo record has no appinfo body".to_string())
}

fn appinfo_path() -> Result<PathBuf, String> {
    let steam = crate::steam::get_steam_path()
        .ok_or_else(|| "Steam installation was not found".to_string())?;
    Ok(steam.join("appcache").join("appinfo.vdf"))
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|root| root.join("steam-launch-options").join("mods.json"))
        .map_err(|error| format!("Steam launch-option store directory unavailable: {error}"))
}

fn load_store(app: &AppHandle) -> Result<LaunchModStore, String> {
    load_store_at(&store_path(app)?)
}

fn load_store_at(path: &Path) -> Result<LaunchModStore, String> {
    if !path.exists() {
        return Ok(LaunchModStore::default());
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("Could not read Steam launch-option store: {error}"))?;
    let mut store: LaunchModStore = match serde_json::from_slice(&bytes) {
        Ok(store) => store,
        Err(error) => {
            let corrupt = path.with_extension("json.corrupt");
            if corrupt.exists() {
                return Err(format!(
                    "Steam launch-option store is invalid and {} already exists; refusing to overwrite either file: {error}",
                    corrupt.display()
                ));
            }
            fs::rename(path, &corrupt).map_err(|rename_error| {
                format!(
                    "Steam launch-option store is invalid and could not be preserved as {}: {rename_error}",
                    corrupt.display()
                )
            })?;
            return Err(format!(
                "Steam launch-option store was invalid and was preserved as {}: {error}",
                corrupt.display()
            ));
        }
    };
    if !matches!(store.schema_version, 1 | STORE_SCHEMA_VERSION) {
        return Err(format!(
            "Unsupported Steam launch-option store schema {}",
            store.schema_version
        ));
    }
    if store.schema_version == 1 {
        for mod_entry in store.mods.values_mut() {
            let baseline = SteamLaunchBaseline {
                change_number: mod_entry.change_number,
                source_file_hash: mod_entry.source_file_hash.clone(),
                options: mod_entry.original.clone(),
                captured_at: mod_entry.saved_at.clone(),
            };
            mod_entry.initial_baseline = Some(baseline.clone());
            mod_entry.latest_steam_baseline = Some(baseline);
        }
        store.schema_version = STORE_SCHEMA_VERSION;
    }
    Ok(store)
}

fn save_store(app: &AppHandle, store: &LaunchModStore) -> Result<(), String> {
    save_store_at(&store_path(app)?, store)
}

fn save_store_at(path: &Path, store: &LaunchModStore) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Steam launch-option store path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not prepare Steam launch-option store: {error}"))?;
    let next = path.with_extension("json.next");
    let backup = path.with_extension("json.bak");
    let bytes = serde_json::to_vec_pretty(store)
        .map_err(|error| format!("Could not serialize Steam launch-option store: {error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&next)
        .map_err(|error| {
            format!("Could not create Steam launch-option store staging file: {error}")
        })?;
    file.write_all(&bytes).map_err(|error| {
        format!("Could not write Steam launch-option store staging file: {error}")
    })?;
    file.sync_all().map_err(|error| {
        format!("Could not flush Steam launch-option store staging file: {error}")
    })?;
    if path.is_file() {
        fs::copy(path, &backup).map_err(|error| {
            format!("Could not preserve Steam launch-option store backup: {error}")
        })?;
    }
    replace_file(&next, path)
}

fn validate_options(options: &[SteamLaunchOption]) -> Result<(), String> {
    if options.len() > MAX_LAUNCH_OPTIONS {
        return Err(format!(
            "A game may have at most {MAX_LAUNCH_OPTIONS} Steam launch options"
        ));
    }
    let mut indexes = BTreeSet::new();
    for option in options {
        if option.index.is_empty()
            || option.index.len() > 32
            || !option.index.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("Steam launch-option indexes must be short decimal values".to_string());
        }
        if !indexes.insert(option.index.clone()) {
            return Err("Steam launch-option indexes must be unique".to_string());
        }
        if option.executable.is_empty() {
            return Err("Every Steam launch option requires an executable".to_string());
        }
        for field in [
            &option.executable,
            &option.arguments,
            &option.working_dir,
            &option.description,
            &option.launch_type,
            &option.os_list,
            &option.os_arch,
            &option.beta_key,
            &option.owns_dlc,
        ] {
            if field.as_bytes().len() > MAX_OPTION_FIELD_BYTES || field.as_bytes().contains(&0) {
                return Err("Steam launch-option field is too large or contains NUL".to_string());
            }
        }
    }
    Ok(())
}

fn state_from_index(
    index: &AppInfoIndex,
    app_id: u32,
    mod_entry: Option<&SteamLaunchMod>,
) -> Result<SteamLaunchState, String> {
    let record = index.read_record(app_id)?;
    let body = app_body(&record)?;
    Ok(SteamLaunchState {
        app_id,
        format: index.format,
        change_number: index.entry(app_id).expect("record exists").change_number,
        current: read_launch_options(body),
        install_dir: body
            .table("config")
            .and_then(|value| value.text("installdir")),
        is_modded: mod_entry.is_some(),
        steam_running: crate::steam_integration::is_steam_running(),
        drift_state: mod_entry.map(|value| value.drift_state),
    })
}

fn baseline_from_state(
    state: &SteamLaunchState,
    source_file_hash: &str,
) -> SteamLaunchBaseline {
    SteamLaunchBaseline {
        change_number: state.change_number,
        source_file_hash: source_file_hash.to_string(),
        options: state.current.clone(),
        captured_at: Utc::now().to_rfc3339(),
    }
}

fn reconcile_mod(
    mod_entry: &mut SteamLaunchMod,
    state: &SteamLaunchState,
    source_file_hash: &str,
) -> bool {
    let change_changed = mod_entry.change_number != state.change_number;
    let desired_live = mod_entry.desired == state.current;
    if change_changed && desired_live {
        mod_entry.latest_steam_baseline = Some(baseline_from_state(state, source_file_hash));
        mod_entry.source_file_hash = source_file_hash.to_string();
        if mod_entry.drift_state != SteamLaunchDriftState::RebaseReviewRequired {
            mod_entry.drift_state = SteamLaunchDriftState::RebaseReviewRequired;
            return true;
        }
        return false;
    }
    if change_changed {
        mod_entry.change_number = state.change_number;
        mod_entry.original = state.current.clone();
        mod_entry.latest_steam_baseline = Some(baseline_from_state(state, source_file_hash));
        mod_entry.source_file_hash = source_file_hash.to_string();
        mod_entry.drift_state = SteamLaunchDriftState::Drifted;
        return true;
    }
    let next = if desired_live {
        if mod_entry.applied_at.is_some() {
            SteamLaunchDriftState::Applied
        } else {
            SteamLaunchDriftState::Staged
        }
    } else if mod_entry.applied_at.is_some() {
        SteamLaunchDriftState::Drifted
    } else {
        SteamLaunchDriftState::Staged
    };
    if mod_entry.drift_state != next {
        mod_entry.drift_state = next;
        return true;
    }
    false
}

#[tauri::command]
pub async fn read_steam_launch_options(
    app: AppHandle,
    app_id: u32,
) -> Result<SteamLaunchState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = APPINFO_LOCK
            .lock()
            .map_err(|_| "Steam appinfo editor is busy".to_string())?;
        let index = AppInfoIndex::open(&appinfo_path()?)?;
        let current_file_hash = hash_file(&index.path)?;
        let mut store = load_store(&app)?;
        let mut state = state_from_index(&index, app_id, store.mods.get(&app_id))?;
        let mut store_changed = false;
        if let Some(mod_entry) = store.mods.get_mut(&app_id) {
            store_changed = reconcile_mod(mod_entry, &state, &current_file_hash);
            state.drift_state = Some(mod_entry.drift_state);
        }
        if store_changed {
            save_store(&app, &store)?;
        }
        Ok(state)
    })
    .await
    .map_err(|error| format!("Steam launch-option read task failed: {error}"))?
}

#[tauri::command]
pub async fn stage_steam_launch_options(
    app: AppHandle,
    app_id: u32,
    change_number: u32,
    desired: Vec<SteamLaunchOption>,
) -> Result<SteamLaunchMod, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_options(&desired)?;
        let _guard = APPINFO_LOCK
            .lock()
            .map_err(|_| "Steam appinfo editor is busy".to_string())?;
        let path = appinfo_path()?;
        let index = AppInfoIndex::open(&path)?;
        let state = state_from_index(&index, app_id, None)?;
        if state.change_number != change_number {
            return Err("APPINFO_DRIFTED: reload this AppID before staging".to_string());
        }
        let mut store = load_store(&app)?;
        let source_file_hash = hash_file(&path)?;
        let now = Utc::now().to_rfc3339();
        if let Some(existing) = store.mods.get_mut(&app_id) {
            reconcile_mod(existing, &state, &source_file_hash);
        }
        let mod_entry = store.mods.entry(app_id).or_insert_with(|| SteamLaunchMod {
            app_id,
            change_number,
            original: state.current.clone(),
            desired: Vec::new(),
            source_file_hash: source_file_hash.clone(),
            saved_at: now.clone(),
            applied_at: None,
            drift_state: SteamLaunchDriftState::Staged,
            initial_baseline: Some(baseline_from_state(&state, &source_file_hash)),
            latest_steam_baseline: Some(baseline_from_state(&state, &source_file_hash)),
        });
        mod_entry.change_number = state.change_number;
        mod_entry.desired = desired;
        mod_entry.saved_at = now;
        mod_entry.applied_at = None;
        mod_entry.source_file_hash = source_file_hash;
        if mod_entry.initial_baseline.is_none() {
            mod_entry.initial_baseline = Some(baseline_from_state(&state, &mod_entry.source_file_hash));
        }
        mod_entry.latest_steam_baseline = Some(baseline_from_state(&state, &mod_entry.source_file_hash));
        mod_entry.drift_state = SteamLaunchDriftState::Staged;
        let result = mod_entry.clone();
        save_store(&app, &store)?;
        Ok(result)
    })
    .await
    .map_err(|error| format!("Steam launch-option stage task failed: {error}"))?
}

#[derive(Debug)]
struct ApplyOutcome {
    app_ids: Vec<u32>,
    backup_sha256: String,
    transaction_id: String,
    before_file_hash: String,
    new_file_hash: String,
}

fn build_audit_receipt(
    store: &LaunchModStore,
    outcome: &ApplyOutcome,
    operation: &str,
) -> SteamLaunchAuditReceipt {
    let mut initial_baselines = BTreeMap::new();
    let mut latest_steam_baselines = BTreeMap::new();
    for app_id in &outcome.app_ids {
        if let Some(mod_entry) = store.mods.get(app_id) {
            if let Some(baseline) = mod_entry.initial_baseline.clone() {
                initial_baselines.insert(*app_id, baseline);
            }
            if let Some(baseline) = mod_entry.latest_steam_baseline.clone() {
                latest_steam_baselines.insert(*app_id, baseline);
            }
        }
    }
    SteamLaunchAuditReceipt {
        receipt_id: Uuid::new_v4().to_string(),
        app_ids: outcome.app_ids.clone(),
        operation: operation.to_string(),
        transaction_id: outcome.transaction_id.clone(),
        before_file_hash: outcome.before_file_hash.clone(),
        after_file_hash: outcome.new_file_hash.clone(),
        retained_backup_sha256: outcome.backup_sha256.clone(),
        initial_baselines,
        latest_steam_baselines,
        created_at: Utc::now().to_rfc3339(),
    }
}

fn retain_audit_receipt(store: &mut LaunchModStore, receipt: SteamLaunchAuditReceipt) {
    const MAX_AUDIT_RECEIPTS: usize = 256;
    store.audit_receipts.push(receipt);
    if store.audit_receipts.len() > MAX_AUDIT_RECEIPTS {
        let excess = store.audit_receipts.len() - MAX_AUDIT_RECEIPTS;
        store.audit_receipts.drain(..excess);
    }
}

fn apply_options(
    app: &AppHandle,
    edits: &BTreeMap<u32, Vec<SteamLaunchOption>>,
) -> Result<ApplyOutcome, String> {
    if edits.is_empty() {
        return Err("No staged Steam launch options were selected".to_string());
    }
    if crate::steam_integration::is_steam_running() {
        return Err("STEAM_MUST_BE_CLOSED".to_string());
    }
    for options in edits.values() {
        validate_options(options)?;
    }
    let target = appinfo_path()?;
    let parent = target
        .parent()
        .ok_or_else(|| "Steam appinfo path has no parent".to_string())?
        .to_path_buf();
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&target)
        .map_err(|error| format!("Steam appinfo cache is locked or not writable: {error}"))?;
    let file_size = fs::metadata(&target)
        .map_err(|error| format!("Could not inspect Steam appinfo cache: {error}"))?
        .len();
    let required = file_size.saturating_mul(2).saturating_add(64 * 1024 * 1024);
    let available = fs2::available_space(&parent)
        .map_err(|error| format!("Could not inspect Steam drive free space: {error}"))?;
    if available < required {
        return Err(format!(
            "INSUFFICIENT_SPACE: appinfo apply needs at least {required} free bytes, found {available}"
        ));
    }

    let source_hash = hash_file(&target)?;
    let nonce = Uuid::new_v4();
    let stage = parent.join(format!("{STAGE_PREFIX}{nonce}.vdf"));
    let backup = parent.join(format!("{BACKUP_PREFIX}{nonce}.vdf"));
    let result = (|| -> Result<ApplyOutcome, String> {
        let index = AppInfoIndex::open(&target)?;
        let mut replacements = BTreeMap::new();
        for (app_id, options) in edits {
            let mut record = index.read_record(*app_id)?;
            write_launch_options(app_body_mut(&mut record)?, options);
            replacements.insert(*app_id, record);
        }
        index.save_as(&stage, &replacements)?;
        let staged_index = AppInfoIndex::open(&stage)?;
        if staged_index.format != index.format
            || staged_index.universe != index.universe
            || staged_index.entries.len() != index.entries.len()
        {
            return Err("Generated appinfo cache failed structural validation".to_string());
        }
        for (app_id, desired) in edits {
            let staged = state_from_index(&staged_index, *app_id, None)?;
            if staged.current != *desired {
                return Err(format!(
                    "Generated AppID {app_id} launch options failed verification"
                ));
            }
        }
        verify_untouched_records(&index, &staged_index, edits.keys().copied().collect())?;
        let staged_hash = hash_file(&stage)?;
        prune_old_backups(&parent, &backup)?;
        let receipt = crate::managed_file_transaction::apply_prepared_file(
            app,
            "steam-launch-options-appinfo",
            crate::managed_file_transaction::ManagedPreparedFileSpec {
                prepared: stage.clone(),
                target: target.clone(),
                retained_backup: backup.clone(),
                allowed_target_root: parent.clone(),
                expected_sha256: staged_hash.clone(),
                expected_original_sha256: Some(source_hash.clone()),
            },
        )?;
        let backup_sha256 = hash_file(&backup)?;
        if backup_sha256 != source_hash {
            return Err(
                "Retained appinfo backup hash does not match the pre-apply file".to_string(),
            );
        }
        Ok(ApplyOutcome {
            app_ids: edits.keys().copied().collect(),
            backup_sha256,
            transaction_id: receipt.transaction_id,
            before_file_hash: source_hash,
            new_file_hash: staged_hash,
        })
    })();
    if result.is_err() && stage.exists() {
        fs::remove_file(&stage).ok();
    }
    result
}

fn apply_staged_blocking(
    app: &AppHandle,
    app_ids: Vec<u32>,
    operation: &str,
) -> Result<SteamLaunchApplyResult, String> {
    let _guard = APPINFO_LOCK
        .lock()
        .map_err(|_| "Steam appinfo editor is busy".to_string())?;
    let mut store = load_store(app)?;
    let selected = app_ids.into_iter().collect::<BTreeSet<_>>();
    let mut edits = BTreeMap::new();
    let index = AppInfoIndex::open(&appinfo_path()?)?;
    let current_file_hash = hash_file(&index.path)?;
    let mut store_changed = false;
    for app_id in selected {
        let mod_entry = store
            .mods
            .get_mut(&app_id)
            .ok_or_else(|| format!("AppID {app_id} has no staged launch-option mod"))?;
        let state = state_from_index(&index, app_id, Some(mod_entry))?;
        if mod_entry.source_file_hash != current_file_hash {
            return Err(format!(
                "APPINFO_DRIFTED: appinfo.vdf changed after AppID {app_id} was staged; reload before applying"
            ));
        }
        store_changed |= reconcile_mod(mod_entry, &state, &current_file_hash);
        if mod_entry.drift_state == SteamLaunchDriftState::RebaseReviewRequired {
            if store_changed {
                save_store(app, &store)?;
            }
            return Err(format!(
                "REBASE_REVIEW_REQUIRED: AppID {app_id} still has the desired options after Steam changed its record"
            ));
        }
        edits.insert(app_id, mod_entry.desired.clone());
    }
    if store_changed {
        save_store(app, &store)?;
    }
    let outcome = apply_options(app, &edits)?;
    let audit_receipt = build_audit_receipt(&store, &outcome, operation);
    let now = Utc::now().to_rfc3339();
    for mod_entry in store.mods.values_mut() {
        mod_entry.source_file_hash = outcome.new_file_hash.clone();
    }
    for app_id in &outcome.app_ids {
        if let Some(mod_entry) = store.mods.get_mut(app_id) {
            mod_entry.applied_at = Some(now.clone());
            mod_entry.drift_state = SteamLaunchDriftState::Applied;
            mod_entry.source_file_hash = outcome.new_file_hash.clone();
        }
    }
    retain_audit_receipt(&mut store, audit_receipt.clone());
    save_store(app, &store)?;
    Ok(SteamLaunchApplyResult {
        ok: true,
        applied_app_ids: outcome.app_ids,
        backup_sha256: Some(outcome.backup_sha256),
        transaction_id: Some(outcome.transaction_id),
        audit_receipt: Some(audit_receipt),
    })
}

#[tauri::command]
pub async fn apply_staged_steam_launch_options(
    app: AppHandle,
    request: SteamLaunchApplyRequest,
) -> Result<SteamLaunchApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        apply_staged_blocking(&app, request.app_ids, "apply")
    })
        .await
        .map_err(|error| format!("Steam launch-option apply task failed: {error}"))?
}

#[tauri::command]
pub async fn reapply_steam_launch_mods(
    app: AppHandle,
    request: SteamLaunchApplyRequest,
) -> Result<SteamLaunchApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        apply_staged_blocking(&app, request.app_ids, "reapply")
    })
        .await
        .map_err(|error| format!("Steam launch-option reapply task failed: {error}"))?
}

#[tauri::command]
pub async fn restore_steam_launch_options(
    app: AppHandle,
    app_id: u32,
) -> Result<SteamLaunchApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = APPINFO_LOCK
            .lock()
            .map_err(|_| "Steam appinfo editor is busy".to_string())?;
        let mut store = load_store(&app)?;
        let index = AppInfoIndex::open(&appinfo_path()?)?;
        let current_file_hash = hash_file(&index.path)?;
        let mod_entry = store
            .mods
            .get_mut(&app_id)
            .ok_or_else(|| format!("AppID {app_id} has no launch-option snapshot to restore"))?;
        let state = state_from_index(&index, app_id, Some(mod_entry))?;
        if mod_entry.source_file_hash != current_file_hash {
            return Err("APPINFO_DRIFTED: appinfo.vdf changed after the last review; reload before restoring".to_string());
        }
        if reconcile_mod(mod_entry, &state, &current_file_hash) {
            save_store(&app, &store)?;
        }
        let original = store.mods[&app_id].original.clone();
        let outcome = apply_options(&app, &BTreeMap::from([(app_id, original.clone())]))?;
        let audit_receipt = build_audit_receipt(&store, &outcome, "restore");
        let index = AppInfoIndex::open(&appinfo_path()?)?;
        if state_from_index(&index, app_id, None)?.current != original {
            return Err(
                "Restored Steam launch options failed post-transaction verification".to_string(),
            );
        }
        store.mods.remove(&app_id);
        for mod_entry in store.mods.values_mut() {
            mod_entry.source_file_hash = outcome.new_file_hash.clone();
        }
        retain_audit_receipt(&mut store, audit_receipt.clone());
        save_store(&app, &store)?;
        Ok(SteamLaunchApplyResult {
            ok: true,
            applied_app_ids: outcome.app_ids,
            backup_sha256: Some(outcome.backup_sha256),
            transaction_id: Some(outcome.transaction_id),
            audit_receipt: Some(audit_receipt),
        })
    })
    .await
    .map_err(|error| format!("Steam launch-option restore task failed: {error}"))?
}

#[tauri::command]
pub async fn list_steam_launch_mods(app: AppHandle) -> Result<Vec<SteamLaunchMod>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = APPINFO_LOCK
            .lock()
            .map_err(|_| "Steam appinfo editor is busy".to_string())?;
        let mut store = load_store(&app)?;
        if store.mods.is_empty() {
            return Ok(Vec::new());
        }
        let index = AppInfoIndex::open(&appinfo_path()?)?;
        let current_file_hash = hash_file(&index.path)?;
        let mut changed = false;
        for mod_entry in store.mods.values_mut() {
            if index.entry(mod_entry.app_id).is_none() {
                continue;
            }
            let state = state_from_index(&index, mod_entry.app_id, None)?;
            changed |= reconcile_mod(mod_entry, &state, &current_file_hash);
        }
        if changed {
            save_store(&app, &store)?;
        }
        Ok(store.mods.into_values().collect())
    })
    .await
    .map_err(|error| format!("Steam launch-option list task failed: {error}"))?
}

fn verify_untouched_records(
    source: &AppInfoIndex,
    generated: &AppInfoIndex,
    edited: BTreeSet<u32>,
) -> Result<(), String> {
    if source.entries.len() != generated.entries.len() {
        return Err("Generated appinfo cache changed the record count".to_string());
    }
    let mut source_file = BufReader::new(File::open(&source.path).map_err(io_error)?);
    let mut generated_file = BufReader::new(File::open(&generated.path).map_err(io_error)?);
    for (before, after) in source.entries.iter().zip(&generated.entries) {
        if before.app_id != after.app_id {
            return Err("Generated appinfo cache changed record order".to_string());
        }
        if edited.contains(&before.app_id) {
            continue;
        }
        if before.size != after.size {
            return Err(format!(
                "Generated appinfo cache changed untouched AppID {}",
                before.app_id
            ));
        }
        source_file
            .seek(SeekFrom::Start(before.offset))
            .map_err(io_error)?;
        generated_file
            .seek(SeekFrom::Start(after.offset))
            .map_err(io_error)?;
        if !streams_equal_exact(
            &mut source_file,
            &mut generated_file,
            8 + before.size as u64,
        )? {
            return Err(format!(
                "Generated appinfo cache changed untouched AppID {}",
                before.app_id
            ));
        }
    }
    Ok(())
}

fn streams_equal_exact<R: Read, S: Read>(
    left: &mut R,
    right: &mut S,
    mut count: u64,
) -> Result<bool, String> {
    let mut left_buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut right_buffer = vec![0u8; COPY_BUFFER_BYTES];
    while count > 0 {
        let limit =
            usize::try_from(count.min(COPY_BUFFER_BYTES as u64)).unwrap_or(COPY_BUFFER_BYTES);
        left.read_exact(&mut left_buffer[..limit])
            .map_err(io_error)?;
        right
            .read_exact(&mut right_buffer[..limit])
            .map_err(io_error)?;
        if left_buffer[..limit] != right_buffer[..limit] {
            return Ok(false);
        }
        count -= limit as u64;
    }
    Ok(true)
}

fn prune_old_backups(parent: &Path, keep: &Path) -> Result<(), String> {
    for entry in fs::read_dir(parent)
        .map_err(|error| format!("Could not inspect appinfo backups: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not inspect appinfo backup: {error}"))?;
        let path = entry.path();
        if path == keep {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(BACKUP_PREFIX) || !name.ends_with(".vdf") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("Could not inspect appinfo backup: {error}"))?;
        if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
            fs::remove_file(&path)
                .map_err(|error| format!("Could not remove prior appinfo backup: {error}"))?;
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = BufReader::new(File::open(path).map_err(io_error)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
fn hash_range(path: &Path, offset: u64, length: u64) -> Result<Vec<u8>, String> {
    let mut file = BufReader::new(File::open(path).map_err(io_error)?);
    file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    let mut remaining = length;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    while remaining > 0 {
        let limit = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..limit]).map_err(io_error)?;
        if read == 0 {
            return Err("appinfo.vdf ended while hashing a record".to_string());
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher.finalize().to_vec())
}

fn copy_exact<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    mut count: u64,
) -> Result<(), String> {
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    while count > 0 {
        let limit = usize::try_from(count.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = reader.read(&mut buffer[..limit]).map_err(io_error)?;
        if read == 0 {
            return Err("appinfo.vdf ended while copying a record".to_string());
        }
        writer.write_all(&buffer[..read]).map_err(io_error)?;
        count -= read as u64;
    }
    Ok(())
}

fn read_cstring<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let byte = read_u8(reader)?;
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
        if bytes.len() > MAX_STRING_BYTES {
            return Err("Binary VDF string exceeds the safety limit".to_string());
        }
    }
}

fn read_wstring<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let pair = read_exact_vec(reader, 2)?;
        if pair == [0, 0] {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&pair);
        if bytes.len() > MAX_STRING_BYTES {
            return Err("Binary VDF wide string exceeds the safety limit".to_string());
        }
    }
}

fn write_cstring<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<(), String> {
    if bytes.contains(&0) {
        return Err("Binary VDF key or string contains an embedded NUL".to_string());
    }
    writer.write_all(bytes).map_err(io_error)?;
    writer.write_all(&[0]).map_err(io_error)
}

fn read_exact_vec<R: Read>(reader: &mut R, count: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; count];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("Truncated appinfo.vdf: {error}"))?;
    Ok(bytes)
}

fn read_u8<R: Read>(reader: &mut R) -> Result<u8, String> {
    Ok(read_exact_vec(reader, 1)?[0])
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        read_exact_vec(reader, 4)?.try_into().expect("four bytes"),
    ))
}

fn read_i64<R: Read>(reader: &mut R) -> Result<i64, String> {
    Ok(i64::from_le_bytes(
        read_exact_vec(reader, 8)?.try_into().expect("eight bytes"),
    ))
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> Result<(), String> {
    writer.write_all(&value.to_le_bytes()).map_err(io_error)
}

fn write_i64<W: Write>(writer: &mut W, value: i64) -> Result<(), String> {
    writer.write_all(&value.to_le_bytes()).map_err(io_error)
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
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
            "Could not atomically replace Steam launch-option store: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target)
        .map_err(|error| format!("Could not atomically replace Steam launch-option store: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch_option(executable: &str) -> SteamLaunchOption {
        SteamLaunchOption {
            index: "0".to_string(),
            source_index: "0".to_string(),
            executable: executable.to_string(),
            arguments: String::new(),
            working_dir: String::new(),
            description: "Play".to_string(),
            launch_type: "default".to_string(),
            os_list: "windows".to_string(),
            os_arch: String::new(),
            beta_key: String::new(),
            owns_dlc: String::new(),
        }
    }

    fn app_record(app_id: u32, indexed: bool, strings: &mut Vec<Vec<u8>>) -> Vec<u8> {
        let mut root = VdfTable::default();
        let mut body = VdfTable::default();
        write_launch_options(&mut body, &[launch_option("game.exe")]);
        body.items.push(VdfProperty {
            marker: 1,
            key: VdfKey::new_ascii("duplicate"),
            value: VdfValue::CString(b"one".to_vec()),
        });
        body.items.push(VdfProperty {
            marker: 1,
            key: VdfKey::new_ascii("duplicate"),
            value: VdfValue::CString(b"two".to_vec()),
        });
        body.items.push(VdfProperty {
            marker: 7,
            key: VdfKey::new_ascii("opaque-int64"),
            value: VdfValue::Fixed([0xFF, 1, 2, 3, 4, 5, 6, 7].to_vec()),
        });
        root.items.push(VdfProperty {
            marker: 0,
            key: VdfKey::new_ascii("appinfo"),
            value: VdfValue::Table(body),
        });
        let mut blob = Vec::new();
        write_vdf_table(&mut blob, &root, strings, indexed).unwrap();
        let mut record = Vec::new();
        write_u32(&mut record, app_id).unwrap();
        write_u32(&mut record, (60 + blob.len()) as u32).unwrap();
        let mut meta = vec![0u8; 60];
        meta[36..40].copy_from_slice(&7u32.to_le_bytes());
        meta[40..60].copy_from_slice(Sha1::digest(&blob).as_slice());
        record.extend_from_slice(&meta);
        record.extend_from_slice(&blob);
        record
    }

    #[test]
    fn parses_real_appinfo_read_only_when_explicitly_requested() {
        let Ok(path) = std::env::var("OXO_APPINFO_TEST_PATH") else {
            return;
        };
        let app_id = std::env::var("OXO_APPINFO_TEST_APP_ID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok());
        let index = AppInfoIndex::open(Path::new(&path)).unwrap();
        assert!(!index.entries.is_empty());
        let selected = app_id
            .or_else(|| index.entries.first().map(|entry| entry.app_id))
            .unwrap();
        let record = index.read_record(selected).unwrap();
        let _ = app_body(&record).unwrap();

        let root = test_root("real-roundtrip");
        let output = root.join("appinfo.vdf");
        index.save_as(&output, &BTreeMap::new()).unwrap();
        assert_eq!(
            fs::metadata(&output).unwrap().len(),
            fs::metadata(&path).unwrap().len()
        );
        assert_eq!(
            hash_file(&output).unwrap(),
            hash_file(Path::new(&path)).unwrap()
        );
        fs::remove_file(output).ok();
        fs::remove_dir(root).ok();
    }

    fn fixture(format: SteamAppInfoFormat) -> Vec<u8> {
        let mut strings = if format.indexed_keys() {
            vec![b"\xFFraw-key".to_vec()]
        } else {
            Vec::new()
        };
        let first = app_record(10, format.indexed_keys(), &mut strings);
        let second = app_record(20, format.indexed_keys(), &mut strings);
        let mut bytes = Vec::new();
        write_u32(&mut bytes, format.magic()).unwrap();
        write_u32(&mut bytes, 1).unwrap();
        let table_pos = if format.indexed_keys() {
            let pos = bytes.len();
            write_i64(&mut bytes, 0).unwrap();
            Some(pos)
        } else {
            None
        };
        bytes.extend(first);
        bytes.extend(second);
        write_u32(&mut bytes, 0).unwrap();
        if let Some(table_pos) = table_pos {
            let offset = bytes.len() as i64;
            write_u32(&mut bytes, strings.len() as u32).unwrap();
            for value in strings {
                write_cstring(&mut bytes, &value).unwrap();
            }
            bytes[table_pos..table_pos + 8].copy_from_slice(&offset.to_le_bytes());
        }
        bytes
    }

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("0xo-appinfo-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn no_edit_round_trip_is_byte_identical_for_supported_formats() {
        for format in [
            SteamAppInfoFormat::V27,
            SteamAppInfoFormat::V28,
            SteamAppInfoFormat::V29,
        ] {
            let root = test_root("roundtrip");
            let source = root.join("appinfo.vdf");
            let output = root.join("out.vdf");
            let mut bytes = fixture(format);
            if format == SteamAppInfoFormat::V27 {
                bytes = fixture_v27();
            }
            fs::write(&source, &bytes).unwrap();
            AppInfoIndex::open(&source)
                .unwrap()
                .save_as(&output, &BTreeMap::new())
                .unwrap();
            assert_eq!(fs::read(&output).unwrap(), bytes);
            fs::remove_file(source).ok();
            fs::remove_file(output).ok();
            fs::remove_dir(root).ok();
        }
    }

    fn fixture_v27() -> Vec<u8> {
        let mut root = VdfTable::default();
        let mut body = VdfTable::default();
        write_launch_options(&mut body, &[launch_option("legacy.exe")]);
        root.items.push(VdfProperty {
            marker: 0,
            key: VdfKey::new_ascii("appinfo"),
            value: VdfValue::Table(body),
        });
        let mut blob = Vec::new();
        write_vdf_table(&mut blob, &root, &mut Vec::new(), false).unwrap();
        let mut bytes = Vec::new();
        write_u32(&mut bytes, MAGIC_V27).unwrap();
        write_u32(&mut bytes, 1).unwrap();
        write_u32(&mut bytes, 10).unwrap();
        write_u32(&mut bytes, (40 + blob.len()) as u32).unwrap();
        let mut meta = vec![0u8; 40];
        meta[36..40].copy_from_slice(&7u32.to_le_bytes());
        bytes.extend(meta);
        bytes.extend(blob);
        write_u32(&mut bytes, 0).unwrap();
        bytes
    }

    #[test]
    fn edits_only_selected_record_and_restamps_binary_sha1() {
        let root = test_root("edit");
        let source = root.join("appinfo.vdf");
        let output = root.join("out.vdf");
        fs::write(&source, fixture(SteamAppInfoFormat::V29)).unwrap();
        let index = AppInfoIndex::open(&source).unwrap();
        let before_second = hash_range(
            &source,
            index.entry(20).unwrap().offset,
            8 + index.entry(20).unwrap().size as u64,
        )
        .unwrap();
        let mut record = index.read_record(10).unwrap();
        write_launch_options(
            app_body_mut(&mut record).unwrap(),
            &[launch_option("changed.exe")],
        );
        index
            .save_as(&output, &BTreeMap::from([(10, record)]))
            .unwrap();
        let generated = AppInfoIndex::open(&output).unwrap();
        assert_eq!(
            read_launch_options(app_body(&generated.read_record(10).unwrap()).unwrap())[0]
                .executable,
            "changed.exe"
        );
        assert_eq!(
            before_second,
            hash_range(
                &output,
                generated.entry(20).unwrap().offset,
                8 + generated.entry(20).unwrap().size as u64,
            )
            .unwrap()
        );
        let entry = generated.entry(10).unwrap();
        let mut file = File::open(&output).unwrap();
        file.seek(SeekFrom::Start(entry.offset + 8)).unwrap();
        let meta = read_exact_vec(&mut file, 60).unwrap();
        let blob = read_exact_vec(&mut file, entry.size as usize - 60).unwrap();
        assert_eq!(&meta[40..60], Sha1::digest(&blob).as_slice());
        fs::remove_file(source).ok();
        fs::remove_file(output).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn rejects_unknown_format_and_truncated_record() {
        let root = test_root("reject");
        let unknown = root.join("unknown.vdf");
        fs::write(&unknown, [0x30, 0x44, 0x56, 0x07, 1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(AppInfoIndex::open(&unknown)
            .unwrap_err()
            .contains("UNSUPPORTED_APPINFO_FORMAT"));
        let truncated = root.join("truncated.vdf");
        let mut bytes = fixture(SteamAppInfoFormat::V28);
        bytes.truncate(bytes.len() - 12);
        fs::write(&truncated, bytes).unwrap();
        assert!(AppInfoIndex::open(&truncated).is_err());
        fs::remove_file(unknown).ok();
        fs::remove_file(truncated).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn mod_store_keeps_first_original_and_preserves_corrupt_file() {
        let root = test_root("store");
        let path = root.join("mods.json");
        let original = launch_option("original.exe");
        let mut store = LaunchModStore::default();
        store.mods.insert(
            10,
            SteamLaunchMod {
                app_id: 10,
                change_number: 1,
                original: vec![original.clone()],
                desired: vec![launch_option("first.exe")],
                source_file_hash: "00".repeat(32),
                saved_at: Utc::now().to_rfc3339(),
                applied_at: None,
                drift_state: SteamLaunchDriftState::Staged,
                initial_baseline: None,
                latest_steam_baseline: None,
            },
        );
        save_store_at(&path, &store).unwrap();
        let mut loaded = load_store_at(&path).unwrap();
        loaded.mods.get_mut(&10).unwrap().desired = vec![launch_option("second.exe")];
        save_store_at(&path, &loaded).unwrap();
        assert_eq!(
            load_store_at(&path).unwrap().mods[&10].original,
            vec![original]
        );
        fs::write(&path, b"not json").unwrap();
        assert!(load_store_at(&path).is_err());
        assert_eq!(
            fs::read(path.with_extension("json.corrupt")).unwrap(),
            b"not json"
        );
        for name in ["mods.json.bak", "mods.json.corrupt"] {
            fs::remove_file(root.join(name)).ok();
        }
        fs::remove_dir(root).ok();
    }

    fn drift_state(change_number: u32, current: Vec<SteamLaunchOption>) -> SteamLaunchState {
        SteamLaunchState {
            app_id: 10,
            format: SteamAppInfoFormat::V29,
            change_number,
            current,
            install_dir: None,
            is_modded: true,
            steam_running: false,
            drift_state: None,
        }
    }

    fn staged_mod() -> SteamLaunchMod {
        SteamLaunchMod {
            app_id: 10,
            change_number: 1,
            original: vec![launch_option("original.exe")],
            desired: vec![launch_option("desired.exe")],
            source_file_hash: "00".repeat(32),
            saved_at: Utc::now().to_rfc3339(),
            applied_at: Some(Utc::now().to_rfc3339()),
            drift_state: SteamLaunchDriftState::Applied,
            initial_baseline: Some(SteamLaunchBaseline {
                change_number: 1,
                source_file_hash: "00".repeat(32),
                options: vec![launch_option("original.exe")],
                captured_at: Utc::now().to_rfc3339(),
            }),
            latest_steam_baseline: None,
        }
    }

    #[test]
    fn drift_rebase_updates_original_only_when_steam_owns_new_live_options() {
        let mut mod_entry = staged_mod();
        let steam_owned = vec![launch_option("steam-rewrite.exe")];
        assert!(reconcile_mod(
            &mut mod_entry,
            &drift_state(2, steam_owned.clone()),
            &"11".repeat(32),
        ));
        assert_eq!(mod_entry.change_number, 2);
        assert_eq!(mod_entry.original, steam_owned);
        assert_eq!(mod_entry.drift_state, SteamLaunchDriftState::Drifted);

        let mut review = staged_mod();
        let first_original = review.original.clone();
        let desired = review.desired.clone();
        assert!(reconcile_mod(
            &mut review,
            &drift_state(2, desired),
            &"22".repeat(32),
        ));
        assert_eq!(review.change_number, 1);
        assert_eq!(review.original, first_original);
        assert_eq!(
            review.drift_state,
            SteamLaunchDriftState::RebaseReviewRequired
        );
    }
}
