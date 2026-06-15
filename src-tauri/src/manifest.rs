use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
pub const CHUNK_MIN_SIZE: usize = 512 * 1024;
pub const CHUNK_TARGET_SIZE: usize = 1024 * 1024;
pub const CHUNK_MAX_SIZE: usize = 2 * 1024 * 1024;
pub const PACK_TARGET_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub format_version: u32,
    pub game_id: String,
    #[serde(default)]
    pub latest_version: Option<String>,
    pub versions: Vec<CatalogVersion>,
    pub packs: Vec<PackRecord>,
    pub signature: Option<SignatureEnvelope>,
}

impl Catalog {
    /// Returns the effective latest version:
    /// uses `latest_version` if set, otherwise falls back to the last version in the list.
    pub fn effective_latest_version(&self) -> Option<&str> {
        self.latest_version
            .as_deref()
            .filter(|v| !v.is_empty())
            .or_else(|| self.versions.last().map(|v| v.version.as_str()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    pub version: String,
    pub manifest_path: String,
    pub total_size: u64,
    pub file_count: usize,
    pub chunk_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub format_version: u32,
    pub game_id: String,
    pub version: String,
    pub created_at: String,
    pub root_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_executable: Option<String>,
    pub total_size: u64,
    pub files: Vec<FileEntry>,
    pub signature: Option<SignatureEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub chunks: Vec<ChunkRef>,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkRef {
    pub hash: String,
    pub file_offset: u64,
    pub uncompressed_size: u64,
    pub pack_id: String,
    pub pack_offset: u64,
    pub compressed_size: u64,
    pub compressed_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRecord {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureEnvelope {
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}
