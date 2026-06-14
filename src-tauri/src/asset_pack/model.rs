use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetPackError {
    #[error("asset pack io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("asset pack json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("asset pack codec error: {0}")]
    Codec(String),
    #[error("asset pack integrity error: {0}")]
    Integrity(String),
    #[error("asset not found: {0}")]
    AssetNotFound(String),
    #[error("game not found: {0}")]
    GameNotFound(String),
    #[error("invalid asset pack: {0}")]
    InvalidPack(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCatalog {
    pub default_locale: String,
    pub games: Vec<GameSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub developer: String,
    pub publisher: String,
    pub latest_version: String,
    pub available_versions: Vec<GameVersionInfo>,
    pub grid_asset_id: String,
    pub hero_asset_id: String,
    pub logo_asset_id: String,
    pub icon_asset_id: String,
    pub install: GameInstallMetadata,
    pub asset_pack_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionInfo {
    pub version: String,
    pub label: String,
    pub build_id: String,
    pub size_bytes: u64,
    pub latest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInstallMetadata {
    pub default_store_root: String,
    pub default_install_folder: String,
    pub default_downloading_folder: String,
    pub storage_label: String,
    pub supports_resume: bool,
    #[serde(default)]
    pub launch_executable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub game_id: String,
    pub locale: String,
    pub title: String,
    pub short_description: String,
    pub detailed_description: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub release_date: String,
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub ratings: Vec<GameRating>,
    pub media: Vec<GameMedia>,
    pub achievements: Vec<GameAchievement>,
    pub sounds: Vec<GameSound>,
    pub install: GameInstallMetadata,
    pub description_images: Vec<String>,
    pub versions: Vec<GameVersionInfo>,
    pub metadata_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRating {
    pub source: String,
    pub score: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMedia {
    pub id: String,
    pub role: String,
    pub title: String,
    pub mime_type: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAchievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon_asset_id: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSound {
    pub id: String,
    pub role: String,
    pub mime_type: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBlob {
    pub mime_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBuildSummary {
    pub output_path: String,
    pub game_count: usize,
    pub asset_count: usize,
    pub achievement_count: usize,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetPackManifest {
    pub(super) generated_at: String,
    pub(super) catalog: GameCatalog,
    pub(super) details: HashMap<String, GameDetail>,
    pub(super) assets: HashMap<String, AssetRecord>,
    pub(super) i18n: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetRecord {
    pub(super) game_id: String,
    pub(super) role: String,
    pub(super) mime_type: String,
    pub(super) block_index: usize,
    pub(super) size: u64,
    pub(super) blake3: String,
}

#[derive(Debug, Clone)]
pub(super) struct RawAsset {
    pub(super) game_id: String,
    pub(super) role: String,
    pub(super) mime_type: String,
    pub(super) bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteMediaSource {
    pub(super) id: String,
    pub(super) role: String,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) mime_type: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RemoteAchievement {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) hidden: bool,
}

#[derive(Debug, Clone)]
pub(super) struct PackBlockHeader {
    pub(super) cipher_len: u64,
    pub(super) compressed_len: u64,
    pub(super) plain_len: u64,
    pub(super) plain_hash: [u8; 32],
    pub(super) cipher_hash: [u8; 32],
}

pub(super) struct LoadedPack {
    pub(super) manifest: AssetPackManifest,
    pub(super) blocks: Vec<LoadedBlock>,
    pub(super) bytes: Vec<u8>,
    pub(super) salt: [u8; 16],
}

pub(super) struct LoadedBlock {
    pub(super) header: PackBlockHeader,
    pub(super) offset: usize,
}

pub(super) struct SourceAssetBuild {
    pub(super) game_id: String,
    pub(super) manifest: AssetPackManifest,
    pub(super) assets: HashMap<String, RawAsset>,
    pub(super) achievement_count: usize,
}
