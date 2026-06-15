use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

mod generic_source;
mod keys;
mod model;

#[derive(Default)]
pub struct AssetPackCache(Mutex<HashMap<String, Arc<LoadedPack>>>);

use crate::manifest::{Catalog, VersionManifest};
use generic_source::build_generic_manifest_and_assets;
use keys::{derive_asset_pack_key, with_steam_api_key, with_steamgriddb_key};
pub use model::{
    AssetBlob, AssetBuildSummary, AssetPackError, GameAchievement, GameCatalog, GameDetail,
    GameInstallMetadata, GameMedia, GameRating, GameSound, GameSummary, GameVersionInfo,
};
use model::{
    AssetPackManifest, AssetRecord, LoadedBlock, LoadedPack, PackBlockHeader, RawAsset,
    RemoteAchievement, RemoteMediaSource, SourceAssetBuild,
};

const MAGIC: &[u8; 8] = b"0XOASSET";
const FORMAT_VERSION: u32 = 1;
const DEFAULT_GAME_ID: &str = "007-first-light";
const DEFAULT_GAME_TITLE: &str = "007 First Light";
const STEAM_APP_ID: u64 = 3768760;
const GAME_CORE_PART: &str = "core";
const GAME_MEDIA_PART: &str = "media";
const GAME_ACHIEVEMENTS_PART: &str = "achievements";
const GAME_AUDIO_PART: &str = "audio";
const GAME_PACK_PARTS: [&str; 4] = [
    GAME_CORE_PART,
    GAME_MEDIA_PART,
    GAME_ACHIEVEMENTS_PART,
    GAME_AUDIO_PART,
];
const ZSTD_MAX_COMPRESSION_LEVEL: i32 = 22;
const DEFAULT_ASSET_SOURCE: &str = r"E:\007Launcher\src\assets\007 first light";
const DEFAULT_PACK_OUTPUT: &str = r"E:\007Launcher\src-tauri\assets\catalog.0xo";
const DEFAULT_DEPOT_ROOT: &str = r"E:\007Launcher\depot\007-first-light";
const DEFAULT_STORE_ROOT: &str = r"E:\0xoLemon store";
const DEFAULT_COMMON_GAME: &str = r"E:\0xoLemon store\common\007 First Light";
const DEFAULT_DOWNLOADING_GAME: &str = r"E:\0xoLemon store\downloading\007 First Light";
const DEFAULT_DEPOT_REPO_BASE: &str =
    "https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main";

pub fn default_asset_source() -> PathBuf {
    PathBuf::from(DEFAULT_ASSET_SOURCE)
}

pub fn default_pack_output() -> PathBuf {
    PathBuf::from(DEFAULT_PACK_OUTPUT)
}

pub fn get_game_catalog(app: &AppHandle) -> Result<GameCatalog, AssetPackError> {
    let packs = load_catalog_packs(app)?;
    let mut default_locale = "en-US".to_string();
    let mut games = Vec::new();
    let mut seen = HashSet::new();

    for pack in packs {
        if games.is_empty() {
            default_locale = pack.manifest.catalog.default_locale.clone();
        }
        for game in &pack.manifest.catalog.games {
            if seen.insert(game.id.clone()) {
                let mut game = game.clone();
                overlay_summary_depot_versions(&mut game);
                games.push(game);
            }
        }
    }

    if games.is_empty() {
        return Err(AssetPackError::InvalidPack(
            "no games found in asset catalogs".to_string(),
        ));
    }

    Ok(GameCatalog {
        default_locale,
        games,
    })
}

pub fn get_game_detail(
    app: &AppHandle,
    game_id: &str,
    _locale: Option<String>,
) -> Result<GameDetail, AssetPackError> {
    let mut detail = load_legacy_game_pack(app, game_id)
        .or_else(|_| load_game_part_pack(app, game_id, GAME_CORE_PART))
        .or_else(|_| load_pack(app))?
        .manifest
        .details
        .get(game_id)
        .cloned()
        .ok_or_else(|| AssetPackError::GameNotFound(game_id.to_string()))?;
    overlay_detail_depot_versions(&mut detail);
    Ok(detail)
}

pub fn get_game_asset(
    app: &AppHandle,
    game_id: &str,
    asset_id: &str,
) -> Result<AssetBlob, AssetPackError> {
    for pack in load_asset_candidate_packs(app, game_id, asset_id) {
        let Ok(pack) = pack else {
            continue;
        };
        let Some(record) = pack.manifest.assets.get(asset_id) else {
            continue;
        };
        if record.game_id != game_id && record.game_id != "shared" {
            continue;
        }
        let bytes = pack.read_asset_block(record)?;
        return Ok(AssetBlob {
            mime_type: record.mime_type.clone(),
            data_base64: STANDARD.encode(bytes),
        });
    }

    Err(AssetPackError::AssetNotFound(asset_id.to_string()))
}

pub fn build_default_pack(
    source: &Path,
    output: &Path,
) -> Result<AssetBuildSummary, AssetPackError> {
    let build = build_manifest_and_assets(source)?;
    let game_id = build.game_id.clone();
    let manifest = build.manifest;
    let assets = build.assets;
    let achievement_count = build.achievement_count;
    let game_pack_dir = output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("games")
        .join(&game_id);
    let catalog_manifest = AssetPackManifest {
        generated_at: manifest.generated_at.clone(),
        catalog: manifest.catalog.clone(),
        details: HashMap::new(),
        assets: HashMap::new(),
        i18n: manifest.i18n.clone(),
    };
    let empty_assets = HashMap::new();
    write_pack(&catalog_manifest, &empty_assets, output)?;

    let split_assets = split_assets_by_part(&assets);
    let mut output_paths = vec![output.display().to_string()];
    for part in GAME_PACK_PARTS {
        let part_output = game_pack_dir.join(format!("{part}.0xo"));
        let part_assets = split_assets.get(part).cloned().unwrap_or_default();
        let part_manifest = if part == GAME_CORE_PART {
            manifest.clone()
        } else {
            minimal_part_manifest(&manifest)
        };
        write_pack(&part_manifest, &part_assets, &part_output)?;
        output_paths.push(part_output.display().to_string());
    }

    Ok(AssetBuildSummary {
        output_path: output_paths.join("; "),
        game_count: manifest.catalog.games.len(),
        asset_count: assets.len(),
        achievement_count,
        generated_at: manifest.generated_at,
    })
}

fn split_assets_by_part(
    assets: &HashMap<String, RawAsset>,
) -> HashMap<&'static str, HashMap<String, RawAsset>> {
    let mut split = GAME_PACK_PARTS
        .iter()
        .map(|part| (*part, HashMap::new()))
        .collect::<HashMap<_, _>>();

    for (id, asset) in assets {
        let part = asset_pack_part_for_asset(id, asset);
        split
            .entry(part)
            .or_default()
            .insert(id.clone(), asset.clone());
    }

    split
}

fn minimal_part_manifest(base: &AssetPackManifest) -> AssetPackManifest {
    AssetPackManifest {
        generated_at: base.generated_at.clone(),
        catalog: GameCatalog {
            default_locale: base.catalog.default_locale.clone(),
            games: Vec::new(),
        },
        details: HashMap::new(),
        assets: HashMap::new(),
        i18n: HashMap::new(),
    }
}

fn load_pack_cached(
    app: &AppHandle,
    key: &str,
    loader: impl FnOnce() -> Result<LoadedPack, AssetPackError>,
) -> Result<Arc<LoadedPack>, AssetPackError> {
    let state = app.state::<AssetPackCache>();
    let mut cache = state.0.lock().unwrap();
    if let Some(pack) = cache.get(key) {
        return Ok(Arc::clone(pack));
    }
    let pack = Arc::new(loader()?);
    cache.insert(key.to_string(), Arc::clone(&pack));
    Ok(pack)
}

fn load_catalog_packs(app: &AppHandle) -> Result<Vec<Arc<LoadedPack>>, AssetPackError> {
    locate_catalog_pack_paths(app)?
        .into_iter()
        .map(|path| {
            let key = format!("catalog:{}", path.display());
            load_pack_cached(app, &key, || {
                let bytes = fs::read(&path)?;
                parse_pack(bytes)
            })
        })
        .collect()
}

fn load_pack(app: &AppHandle) -> Result<Arc<LoadedPack>, AssetPackError> {
    load_pack_cached(app, "catalog", || {
        let path = locate_catalog_pack_paths(app)?
            .into_iter()
            .next()
            .ok_or_else(|| AssetPackError::InvalidPack("asset catalog is missing".to_string()))?;
        let bytes = fs::read(path)?;
        parse_pack(bytes)
    })
}

fn load_game_part_pack(
    app: &AppHandle,
    game_id: &str,
    part: &str,
) -> Result<Arc<LoadedPack>, AssetPackError> {
    load_pack_cached(app, &format!("game:{game_id}:{part}"), || {
        let path = locate_pack(app, &format!("assets/games/{game_id}/{part}.0xo"))?;
        let bytes = fs::read(path)?;
        parse_pack(bytes)
    })
}

fn load_legacy_game_pack(
    app: &AppHandle,
    game_id: &str,
) -> Result<Arc<LoadedPack>, AssetPackError> {
    load_pack_cached(app, &format!("game:{game_id}:legacy"), || {
        let path = locate_first_pack(
            app,
            &[
                format!("assets/games/{game_id}/{game_id}.0xo"),
                format!("assets/games/{game_id}.0xo"),
            ],
        )?;
        let bytes = fs::read(path)?;
        parse_pack(bytes)
    })
}

fn load_asset_candidate_packs(
    app: &AppHandle,
    game_id: &str,
    asset_id: &str,
) -> Vec<Result<Arc<LoadedPack>, AssetPackError>> {
    let preferred = asset_pack_part_for_id(asset_id);
    let mut parts = Vec::with_capacity(GAME_PACK_PARTS.len());
    parts.push(preferred);
    for part in GAME_PACK_PARTS {
        if part != preferred {
            parts.push(part);
        }
    }

    let mut candidates = parts
        .into_iter()
        .map(|part| load_game_part_pack(app, game_id, part))
        .collect::<Vec<_>>();
    candidates.push(load_legacy_game_pack(app, game_id));
    candidates.push(load_pack(app));
    candidates
}

fn asset_pack_part_for_id(asset_id: &str) -> &'static str {
    let logical = asset_id
        .split_once(':')
        .map(|(_, value)| value)
        .unwrap_or(asset_id);
    if logical.starts_with("achievement:") {
        GAME_ACHIEVEMENTS_PART
    } else if logical.starts_with("sound:") {
        GAME_AUDIO_PART
    } else if logical.starts_with("media:")
        || logical.starts_with("steam:")
        || logical.starts_with("desc-img-")
    {
        GAME_MEDIA_PART
    } else {
        GAME_CORE_PART
    }
}

fn asset_pack_part_for_asset(id: &str, asset: &RawAsset) -> &'static str {
    match asset.role.as_str() {
        "achievement" => GAME_ACHIEVEMENTS_PART,
        "sound" => GAME_AUDIO_PART,
        "video" | "video-preview" | "video-thumb" | "screenshot" | "gif" | "description-image"
        | "header" | "capsule" | "background" => GAME_MEDIA_PART,
        _ => asset_pack_part_for_id(id),
    }
}

fn locate_pack(app: &AppHandle, relative_path: &str) -> Result<PathBuf, AssetPackError> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let path = resource_dir.join(relative_to_path(relative_path));
        if path.exists() {
            return Ok(path);
        }
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_to_path(relative_path));
    if dev_path.exists() {
        return Ok(dev_path);
    }

    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_to_path(relative_path));
    if fallback.exists() {
        return Ok(fallback);
    }

    Err(AssetPackError::InvalidPack(format!(
        "{relative_path} is missing"
    )))
}

fn locate_first_pack(
    app: &AppHandle,
    relative_paths: &[String],
) -> Result<PathBuf, AssetPackError> {
    for relative_path in relative_paths {
        if let Ok(path) = locate_pack(app, relative_path) {
            return Ok(path);
        }
    }

    Err(AssetPackError::InvalidPack(format!(
        "{} is missing",
        relative_paths.join(" or ")
    )))
}

fn locate_catalog_pack_paths(app: &AppHandle) -> Result<Vec<PathBuf>, AssetPackError> {
    let mut roots = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        roots.push(resource_dir);
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    for root in roots {
        let assets_dir = root.join("assets");
        let mut paths = catalog_pack_paths_in(&assets_dir)?;
        paths.extend(game_catalog_pack_paths_in(&assets_dir)?);
        paths = unique_paths(paths);
        if !paths.is_empty() {
            return Ok(paths);
        }
    }

    Err(AssetPackError::InvalidPack(
        "assets/catalog*.0xo is missing".to_string(),
    ))
}

fn game_catalog_pack_paths_in(assets_dir: &Path) -> Result<Vec<PathBuf>, AssetPackError> {
    let games_dir = assets_dir.join("games");
    if !games_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(games_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let game_dir = entry.path();
        let game_id = entry.file_name().to_string_lossy().to_string();
        let core = game_dir.join("core.0xo");
        if core.is_file() {
            paths.push(core);
            continue;
        }
        let legacy = game_dir.join(format!("{game_id}.0xo"));
        if legacy.is_file() {
            paths.push(legacy);
        }
    }
    paths.sort();
    Ok(paths)
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for path in paths {
        let key = path.to_string_lossy().to_string();
        if seen.insert(key) {
            unique.push(path);
        }
    }
    unique
}

fn catalog_pack_paths_in(assets_dir: &Path) -> Result<Vec<PathBuf>, AssetPackError> {
    if !assets_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(assets_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("catalog") && name.ends_with(".0xo") {
            paths.push(entry.path());
        }
    }
    paths.sort_by_key(|path| {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        (usize::from(name != "catalog.0xo"), name)
    });
    Ok(paths)
}

fn build_manifest_and_assets(source: &Path) -> Result<SourceAssetBuild, AssetPackError> {
    if source
        .join("details")
        .join("metadata")
        .join("game-detail.normalized.json")
        .exists()
    {
        return build_generic_manifest_and_assets(source);
    }

    let mut assets = HashMap::new();
    let versions = detect_versions();
    let install = default_install_metadata();

    let grid_id = asset_id(DEFAULT_GAME_ID, "grid");
    let hero_id = asset_id(DEFAULT_GAME_ID, "hero");
    let logo_id = asset_id(DEFAULT_GAME_ID, "logo");
    let icon_id = asset_id(DEFAULT_GAME_ID, "icon");
    add_file_asset(
        &mut assets,
        DEFAULT_GAME_ID,
        "grid",
        &grid_id,
        &source.join("grid-007.png"),
    )?;
    add_file_asset(
        &mut assets,
        DEFAULT_GAME_ID,
        "hero",
        &hero_id,
        &source.join("hero-007.png"),
    )?;
    add_file_asset(
        &mut assets,
        DEFAULT_GAME_ID,
        "logo",
        &logo_id,
        &source.join("logo-007.png"),
    )?;
    add_file_asset(
        &mut assets,
        DEFAULT_GAME_ID,
        "icon",
        &icon_id,
        &source.join("icon-007.ico"),
    )?;

    let mut media = Vec::new();
    add_details_media_assets(source, &mut assets, &mut media)?;
    let details_empty = source
        .join("details")
        .read_dir()
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true);
    let remote_metadata = if details_empty && std::env::var_os("OXO_ASSET_PACK_OFFLINE").is_none() {
        fetch_remote_metadata()
    } else {
        None
    };
    if let Some(remote) = remote_metadata.as_ref() {
        add_remote_media_assets(&mut assets, &mut media, remote);
        add_remote_brand_assets(&mut assets, remote);
    }

    let mut achievements = add_achievement_assets(source, &mut assets)?;
    if let Some(remote) = remote_metadata.as_ref() {
        apply_remote_achievement_metadata(&mut achievements, remote);
    }
    let sounds = add_sound_assets(source, &mut assets)?;
    add_font_assets(&mut assets);

    let mut detail =
        read_local_detail(source, &install, &versions, &media, &achievements, &sounds)?
            .unwrap_or_else(|| {
                default_game_detail(
                    &install,
                    &versions,
                    media.clone(),
                    achievements.clone(),
                    sounds.clone(),
                )
            });
    if let Some(remote) = remote_metadata {
        apply_remote_metadata_overlay(&mut detail, remote, &mut assets);
    }

    let summary = GameSummary {
        id: DEFAULT_GAME_ID.to_string(),
        title: detail.title.clone(),
        subtitle: "IO Interactive A/S".to_string(),
        developer: detail
            .developers
            .first()
            .cloned()
            .unwrap_or_else(|| "IO Interactive A/S".to_string()),
        publisher: detail
            .publishers
            .first()
            .cloned()
            .unwrap_or_else(|| "IO Interactive A/S".to_string()),
        latest_version: versions
            .iter()
            .find(|version| version.latest)
            .map(|version| version.version.clone())
            .unwrap_or_else(|| "v1.1".to_string()),
        available_versions: versions.clone(),
        grid_asset_id: grid_id,
        hero_asset_id: hero_id,
        logo_asset_id: logo_id,
        icon_asset_id: icon_id,
        install,
        asset_pack_path: format!("assets/games/{DEFAULT_GAME_ID}/{GAME_CORE_PART}.0xo"),
    };

    let mut details = HashMap::new();
    details.insert(DEFAULT_GAME_ID.to_string(), detail);

    let manifest = AssetPackManifest {
        generated_at: Utc::now().to_rfc3339(),
        catalog: GameCatalog {
            default_locale: "en-US".to_string(),
            games: vec![summary],
        },
        details,
        assets: HashMap::new(),
        i18n: default_i18n(),
    };

    Ok(SourceAssetBuild {
        game_id: DEFAULT_GAME_ID.to_string(),
        manifest,
        assets,
        achievement_count: achievements.len(),
    })
}

fn write_pack(
    manifest_without_assets: &AssetPackManifest,
    assets: &HashMap<String, RawAsset>,
    output: &Path,
) -> Result<(), AssetPackError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let salt = pack_salt();
    let mut ids = assets.keys().cloned().collect::<Vec<_>>();
    ids.sort_by_key(|id| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&salt);
        hasher.update(id.as_bytes());
        hasher.finalize().to_hex().to_string()
    });

    let mut asset_records = HashMap::new();
    let mut block_headers = Vec::with_capacity(ids.len());
    let mut block_payloads = Vec::with_capacity(ids.len());
    let mut block_by_plain_hash: HashMap<[u8; 32], usize> = HashMap::new();

    for id in ids.iter() {
        let asset = assets
            .get(id)
            .ok_or_else(|| AssetPackError::AssetNotFound(id.clone()))?;
        let plain_hash = *blake3::hash(&asset.bytes).as_bytes();
        if let Some(&block_index) = block_by_plain_hash.get(&plain_hash) {
            asset_records.insert(
                id.clone(),
                AssetRecord {
                    game_id: asset.game_id.clone(),
                    role: asset.role.clone(),
                    mime_type: asset.mime_type.clone(),
                    block_index,
                    size: asset.bytes.len() as u64,
                    blake3: hex::encode(plain_hash),
                },
            );
            continue;
        }

        let block_index = block_payloads.len();
        let compressed = zstd::bulk::compress(&asset.bytes, ZSTD_MAX_COMPRESSION_LEVEL)?;
        let mut cipher = compressed.clone();
        xor_stream(&mut cipher, &salt, block_index as u64 + 1);
        let cipher_hash = *blake3::hash(&cipher).as_bytes();
        block_headers.push(PackBlockHeader {
            cipher_len: cipher.len() as u64,
            compressed_len: compressed.len() as u64,
            plain_len: asset.bytes.len() as u64,
            plain_hash,
            cipher_hash,
        });
        block_payloads.push(cipher);
        asset_records.insert(
            id.clone(),
            AssetRecord {
                game_id: asset.game_id.clone(),
                role: asset.role.clone(),
                mime_type: asset.mime_type.clone(),
                block_index,
                size: asset.bytes.len() as u64,
                blake3: hex::encode(plain_hash),
            },
        );
        block_by_plain_hash.insert(plain_hash, block_index);
    }

    let mut manifest = manifest_without_assets.clone();
    manifest.assets = asset_records;
    let manifest_json = serde_json::to_vec(&manifest)?;
    let manifest_hash = *blake3::hash(&manifest_json).as_bytes();
    let mut manifest_cipher = zstd::bulk::compress(&manifest_json, ZSTD_MAX_COMPRESSION_LEVEL)?;
    let manifest_plain_len = manifest_json.len() as u64;
    xor_stream(&mut manifest_cipher, &salt, 0);

    let mut file = File::create(output)?;
    file.write_all(MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())?;
    file.write_all(&salt)?;
    file.write_all(&(manifest_cipher.len() as u64).to_le_bytes())?;
    file.write_all(&manifest_plain_len.to_le_bytes())?;
    file.write_all(&manifest_hash)?;
    file.write_all(&(block_headers.len() as u32).to_le_bytes())?;
    for header in &block_headers {
        file.write_all(&header.cipher_len.to_le_bytes())?;
        file.write_all(&header.compressed_len.to_le_bytes())?;
        file.write_all(&header.plain_len.to_le_bytes())?;
        file.write_all(&header.plain_hash)?;
        file.write_all(&header.cipher_hash)?;
    }
    file.write_all(&manifest_cipher)?;
    for payload in &block_payloads {
        file.write_all(payload)?;
    }
    Ok(())
}

fn parse_pack(bytes: Vec<u8>) -> Result<LoadedPack, AssetPackError> {
    let mut cursor = Cursor::new(bytes.as_slice());
    let mut magic = [0_u8; 8];
    cursor.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(AssetPackError::InvalidPack("bad magic".to_string()));
    }

    let version = read_u32(&mut cursor)?;
    if version != FORMAT_VERSION {
        return Err(AssetPackError::InvalidPack(format!(
            "unsupported format version {version}"
        )));
    }

    let mut salt = [0_u8; 16];
    cursor.read_exact(&mut salt)?;
    let manifest_cipher_len = read_u64(&mut cursor)? as usize;
    let manifest_plain_len = read_u64(&mut cursor)? as usize;
    let mut manifest_hash = [0_u8; 32];
    cursor.read_exact(&mut manifest_hash)?;
    let block_count = read_u32(&mut cursor)? as usize;

    let mut headers = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let cipher_len = read_u64(&mut cursor)?;
        let compressed_len = read_u64(&mut cursor)?;
        let plain_len = read_u64(&mut cursor)?;
        let mut plain_hash = [0_u8; 32];
        cursor.read_exact(&mut plain_hash)?;
        let mut cipher_hash = [0_u8; 32];
        cursor.read_exact(&mut cipher_hash)?;
        headers.push(PackBlockHeader {
            cipher_len,
            compressed_len,
            plain_len,
            plain_hash,
            cipher_hash,
        });
    }

    let manifest_offset = cursor.position() as usize;
    let manifest_end = manifest_offset
        .checked_add(manifest_cipher_len)
        .ok_or_else(|| AssetPackError::InvalidPack("manifest length overflow".to_string()))?;
    if manifest_end > bytes.len() {
        return Err(AssetPackError::InvalidPack(
            "manifest exceeds file length".to_string(),
        ));
    }
    let mut manifest_cipher = bytes[manifest_offset..manifest_end].to_vec();
    xor_stream(&mut manifest_cipher, &salt, 0);
    let manifest_json = zstd::bulk::decompress(&manifest_cipher, manifest_plain_len)?;
    if *blake3::hash(&manifest_json).as_bytes() != manifest_hash {
        return Err(AssetPackError::Integrity(
            "manifest hash mismatch".to_string(),
        ));
    }
    let manifest: AssetPackManifest = serde_json::from_slice(&manifest_json)?;

    let mut offset = manifest_end;
    let mut blocks = Vec::with_capacity(headers.len());
    for header in headers {
        let end = offset
            .checked_add(header.cipher_len as usize)
            .ok_or_else(|| AssetPackError::InvalidPack("block length overflow".to_string()))?;
        if end > bytes.len() {
            return Err(AssetPackError::InvalidPack(
                "block exceeds file length".to_string(),
            ));
        }
        if *blake3::hash(&bytes[offset..end]).as_bytes() != header.cipher_hash {
            return Err(AssetPackError::Integrity(
                "block cipher hash mismatch".to_string(),
            ));
        }
        blocks.push(LoadedBlock { header, offset });
        offset = end;
    }

    Ok(LoadedPack {
        manifest,
        blocks,
        bytes,
        salt,
    })
}

impl LoadedPack {
    fn read_asset_block(&self, record: &AssetRecord) -> Result<Vec<u8>, AssetPackError> {
        let block = self
            .blocks
            .get(record.block_index)
            .ok_or_else(|| AssetPackError::AssetNotFound(record.role.clone()))?;
        let end = block.offset + block.header.cipher_len as usize;
        let cipher = &self.bytes[block.offset..end];
        if *blake3::hash(cipher).as_bytes() != block.header.cipher_hash {
            return Err(AssetPackError::Integrity(format!(
                "block hash mismatch for {}",
                record.role
            )));
        }

        let mut compressed = cipher.to_vec();
        xor_stream(&mut compressed, &self.salt, record.block_index as u64 + 1);
        if compressed.len() != block.header.compressed_len as usize {
            return Err(AssetPackError::Integrity(format!(
                "compressed length mismatch for {}",
                record.role
            )));
        }
        let bytes = zstd::bulk::decompress(&compressed, block.header.plain_len as usize)?;
        let plain_hash = *blake3::hash(&bytes).as_bytes();
        if plain_hash != block.header.plain_hash || hex::encode(plain_hash) != record.blake3 {
            return Err(AssetPackError::Integrity(format!(
                "plain hash mismatch for {}",
                record.role
            )));
        }
        Ok(bytes)
    }
}

fn add_file_asset(
    assets: &mut HashMap<String, RawAsset>,
    game_id: &str,
    role: &str,
    id: &str,
    path: &Path,
) -> Result<(), AssetPackError> {
    let bytes = fs::read(path)?;
    assets.insert(
        id.to_string(),
        RawAsset {
            game_id: game_id.to_string(),
            role: role.to_string(),
            mime_type: mime_for_path(path),
            bytes,
        },
    );
    Ok(())
}

fn add_details_media_assets(
    source: &Path,
    assets: &mut HashMap<String, RawAsset>,
    media: &mut Vec<GameMedia>,
) -> Result<(), AssetPackError> {
    let details = source.join("details");
    if !details.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(details)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let mime = mime_for_path(&path);
        if !(mime.starts_with("image/") || mime.starts_with("video/")) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("media-{}", media.len() + 1));
        let role = if mime.starts_with("video/") {
            "video"
        } else if mime == "image/gif" {
            "gif"
        } else {
            "screenshot"
        };
        let id = asset_id(DEFAULT_GAME_ID, &format!("media:{stem}"));
        add_file_asset(assets, DEFAULT_GAME_ID, role, &id, &path)?;
        media.push(GameMedia {
            id: stem.clone(),
            role: role.to_string(),
            title: title_from_slug(&stem),
            mime_type: mime,
            asset_id: id,
        });
    }
    Ok(())
}

fn add_remote_media_assets(
    assets: &mut HashMap<String, RawAsset>,
    media: &mut Vec<GameMedia>,
    remote: &RemoteMetadata,
) {
    if remote.media_sources.is_empty() {
        return;
    }

    let Ok(client) = Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("0xoLemonAssetPackBuilder/0.1")
        .build()
    else {
        return;
    };

    for item in &remote.media_sources {
        if media.iter().any(|existing| existing.id == item.id) {
            continue;
        }

        let (mime_type, bytes) = if is_stream_manifest_url(&item.url)
            || item.mime_type == "application/vnd.apple.mpegurl"
        {
            let Some(bytes) = transcode_stream_preview(&item.url, &item.id) else {
                continue;
            };
            ("video/mp4".to_string(), bytes)
        } else {
            let Ok(response) = client.get(&item.url).send() else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let mime_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.split(';').next().unwrap_or(value).to_string())
                .unwrap_or_else(|| item.mime_type.clone());
            let Ok(bytes) = response.bytes() else {
                continue;
            };
            (mime_type, bytes.to_vec())
        };
        let id = asset_id(DEFAULT_GAME_ID, &format!("steam:{}", item.id));
        assets.insert(
            id.clone(),
            RawAsset {
                game_id: DEFAULT_GAME_ID.to_string(),
                role: item.role.clone(),
                mime_type: mime_type.clone(),
                bytes,
            },
        );
        media.push(GameMedia {
            id: item.id.clone(),
            role: item.role.clone(),
            title: item.title.clone(),
            mime_type,
            asset_id: id,
        });
    }
}

fn is_stream_manifest_url(url: &str) -> bool {
    let clean = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    clean.ends_with(".m3u8") || clean.ends_with(".mpd")
}

fn transcode_stream_preview(url: &str, id: &str) -> Option<Vec<u8>> {
    let output_path = std::env::temp_dir().join(format!(
        "0xo-{}-{}-{}.mp4",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis(),
        safe_temp_stem(id),
    ));

    let status = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-y")
        .arg("-i")
        .arg(url)
        .arg("-t")
        .arg("120")
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a:0?")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("veryfast")
        .arg("-crf")
        .arg("30")
        .arg("-vf")
        .arg("scale=min(1280\\,iw):-2")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("96k")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&output_path)
        .status()
        .ok()?;

    if !status.success() {
        let _ = fs::remove_file(&output_path);
        return None;
    }

    let bytes = fs::read(&output_path).ok()?;
    let _ = fs::remove_file(&output_path);
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

fn safe_temp_stem(value: &str) -> String {
    let mut stem = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if stem.is_empty() {
        stem.push_str("media");
    }
    stem
}

fn add_remote_brand_assets(assets: &mut HashMap<String, RawAsset>, remote: &RemoteMetadata) {
    if remote.brand_sources.is_empty() {
        return;
    }

    let Ok(client) = Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("0xoLemonAssetPackBuilder/0.1")
        .build()
    else {
        return;
    };

    for item in &remote.brand_sources {
        let Ok(response) = client.get(&item.url).send() else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or(value).to_string())
            .unwrap_or_else(|| item.mime_type.clone());
        let Ok(bytes) = response.bytes() else {
            continue;
        };
        let id = asset_id(DEFAULT_GAME_ID, &format!("brand:{}", item.id));
        assets.insert(
            id,
            RawAsset {
                game_id: DEFAULT_GAME_ID.to_string(),
                role: format!("brand-{}", item.role),
                mime_type,
                bytes: bytes.to_vec(),
            },
        );
    }
}

fn add_achievement_assets(
    source: &Path,
    assets: &mut HashMap<String, RawAsset>,
) -> Result<Vec<GameAchievement>, AssetPackError> {
    let dir = source.join("achievement_images");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    let mut achievements = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        let path = entry.path();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("achievement-{index:02}"));
        let asset = asset_id(DEFAULT_GAME_ID, &format!("achievement:{stem}"));
        add_file_asset(assets, DEFAULT_GAME_ID, "achievement", &asset, &path)?;
        achievements.push(GameAchievement {
            id: stem,
            name: format!("Classified Objective {:02}", index + 1),
            description: "Achievement metadata can be refreshed from Steam when the publisher data is available.".to_string(),
            icon_asset_id: asset,
            hidden: true,
        });
    }
    Ok(achievements)
}

fn apply_remote_achievement_metadata(
    achievements: &mut [GameAchievement],
    remote: &RemoteMetadata,
) {
    for (achievement, remote_achievement) in achievements.iter_mut().zip(remote.achievements.iter())
    {
        if !remote_achievement.name.trim().is_empty() {
            achievement.name = remote_achievement.name.clone();
        }
        if !remote_achievement.description.trim().is_empty() {
            achievement.description = remote_achievement.description.clone();
        }
        achievement.hidden = remote_achievement.hidden;
    }
}

fn add_sound_assets(
    source: &Path,
    assets: &mut HashMap<String, RawAsset>,
) -> Result<Vec<GameSound>, AssetPackError> {
    let dir = source.join("sounds");
    let mut sounds = Vec::new();
    if !dir.exists() {
        return Ok(sounds);
    }

    let mut entries = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "sound".to_string());
        let id = asset_id(DEFAULT_GAME_ID, &format!("sound:{stem}"));
        let mime = mime_for_path(&path);
        add_file_asset(assets, DEFAULT_GAME_ID, "sound", &id, &path)?;
        sounds.push(GameSound {
            id: stem,
            role: "achievement-unlock".to_string(),
            mime_type: mime,
            asset_id: id,
        });
    }
    Ok(sounds)
}

fn add_font_assets(assets: &mut HashMap<String, RawAsset>) {
    let fonts = [
        (r"E:\Arya\Arya-Regular.ttf", "font:arya-regular"),
        (r"E:\Arya\Arya-Bold.ttf", "font:arya-bold"),
    ];
    for (path, role) in fonts {
        let path = PathBuf::from(path);
        if let Ok(bytes) = fs::read(&path) {
            assets.insert(
                asset_id("shared", role),
                RawAsset {
                    game_id: "shared".to_string(),
                    role: role.to_string(),
                    mime_type: mime_for_path(&path),
                    bytes,
                },
            );
        }
    }
}

fn read_local_detail(
    source: &Path,
    install: &GameInstallMetadata,
    versions: &[GameVersionInfo],
    media: &[GameMedia],
    achievements: &[GameAchievement],
    sounds: &[GameSound],
) -> Result<Option<GameDetail>, AssetPackError> {
    let candidates = [
        source.join("details").join("game-detail.json"),
        source.join("details").join("metadata.json"),
    ];
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let mut detail = serde_json::from_slice::<GameDetail>(&fs::read(path)?)?;
        if detail.media.is_empty() {
            detail.media = media.to_vec();
        }
        if detail.achievements.is_empty() {
            detail.achievements = achievements.to_vec();
        }
        if detail.sounds.is_empty() {
            detail.sounds = sounds.to_vec();
        }
        if detail.versions.is_empty() {
            detail.versions = versions.to_vec();
        }
        detail.install = install.clone();
        return Ok(Some(detail));
    }
    Ok(None)
}

fn default_game_detail(
    install: &GameInstallMetadata,
    versions: &[GameVersionInfo],
    media: Vec<GameMedia>,
    achievements: Vec<GameAchievement>,
    sounds: Vec<GameSound>,
) -> GameDetail {
    GameDetail {
        game_id: DEFAULT_GAME_ID.to_string(),
        locale: "en-US".to_string(),
        title: DEFAULT_GAME_TITLE.to_string(),
        short_description:
            "Earn the number in a cinematic action adventure from IO Interactive.".to_string(),
        detailed_description: "A young, resourceful James Bond is thrown into a dangerous world of espionage, high-stakes action, and sharp improvisation. This launcher manages install, update, repair, rollback, cache, and resume-safe downloads from the local content depot.".to_string(),
        developers: vec!["IO Interactive A/S".to_string()],
        publishers: vec!["IO Interactive A/S".to_string()],
        release_date: "May 26, 2026".to_string(),
        genres: vec!["Action".to_string(), "Adventure".to_string()],
        categories: vec![
            "Single-player".to_string(),
            "Controller support".to_string(),
            "Cloud saves ready".to_string(),
            "SSD recommended".to_string(),
        ],
        ratings: vec![GameRating {
            source: "Metacritic".to_string(),
            score: "85".to_string(),
        }],
        media,
        achievements,
        sounds,
        install: install.clone(),
        description_images: vec![],
        versions: versions.to_vec(),
        metadata_source: "local-default".to_string(),
    }
}

fn apply_remote_metadata_overlay(
    detail: &mut GameDetail,
    remote: RemoteMetadata,
    assets: &mut HashMap<String, RawAsset>,
) {
    if !remote.achievements.is_empty() {
        apply_remote_achievement_metadata(&mut detail.achievements, &remote);
    }
    if let Some(short) = remote
        .short_description
        .filter(|value| !value.trim().is_empty())
    {
        detail.short_description = strip_html(&short);
    }
    if let Some(mut full) = remote
        .detailed_description
        .filter(|value| !value.trim().is_empty())
    {
        if let Ok(client) = Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("0xoLemonAssetPackBuilder/0.1")
            .build()
        {
            let mut extracted_images = Vec::new();
            // Match <img ... src="URL" ...>
            let re = regex::Regex::new(r#"<img[^>]+src="([^"]+)""#).unwrap();

            // Collect all unique URLs to avoid borrowing issues during replacement
            let urls: Vec<String> = re
                .captures_iter(&full)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            let mut unique_urls = urls.clone();
            unique_urls.sort();
            unique_urls.dedup();

            for url in unique_urls {
                if let Ok(response) = client.get(&url).send() {
                    if response.status().is_success() {
                        let mime_type = response
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok())
                            .map(|value| value.split(';').next().unwrap_or(value).to_string())
                            .unwrap_or_else(|| "image/jpeg".to_string());
                        if let Ok(bytes) = response.bytes() {
                            let img_id = format!("desc-img-{}", extracted_images.len());
                            let full_asset_id = asset_id(DEFAULT_GAME_ID, &img_id);
                            assets.insert(
                                full_asset_id.clone(),
                                RawAsset {
                                    game_id: DEFAULT_GAME_ID.to_string(),
                                    role: "description-image".to_string(),
                                    mime_type,
                                    bytes: bytes.to_vec(),
                                },
                            );
                            extracted_images.push(full_asset_id.clone());
                            full = full.replace(&url, &format!("asset:{}", full_asset_id));
                        }
                    }
                }
            }
            detail.description_images.extend(extracted_images);
        }
        detail.detailed_description = full; // keep HTML!
    }
    if !remote.developers.is_empty() {
        detail.developers = remote.developers;
    }
    if !remote.publishers.is_empty() {
        detail.publishers = remote.publishers;
    }
    if !remote.release_date.is_empty() {
        detail.release_date = remote.release_date;
    }
    if !remote.genres.is_empty() {
        detail.genres = remote.genres;
    }
    if !remote.categories.is_empty() {
        detail.categories = remote.categories;
    }
    detail.metadata_source = remote.source;
}

#[derive(Default)]
struct RemoteMetadata {
    short_description: Option<String>,
    detailed_description: Option<String>,
    developers: Vec<String>,
    publishers: Vec<String>,
    release_date: String,
    genres: Vec<String>,
    categories: Vec<String>,
    achievements: Vec<RemoteAchievement>,
    media_sources: Vec<RemoteMediaSource>,
    brand_sources: Vec<RemoteMediaSource>,
    source: String,
}

fn fetch_remote_metadata() -> Option<RemoteMetadata> {
    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("0xoLemonAssetPackBuilder/0.1")
        .build()
        .ok()?;
    let app_id = STEAM_APP_ID;
    let mut remote = fetch_steam_store_detail(&client, app_id)
        .ok()
        .flatten()
        .unwrap_or_default();
    let achievements = fetch_steam_achievements(&client, app_id)
        .ok()
        .flatten()
        .unwrap_or_default();
    if !achievements.is_empty() {
        remote.achievements = achievements;
    }
    remote.brand_sources = fetch_steamgriddb_brand_sources(&client, app_id).unwrap_or_default();
    if remote.source.is_empty() {
        remote.source = "steam".to_string();
    }
    Some(remote)
}

fn fetch_steam_store_detail(
    client: &Client,
    app_id: u64,
) -> Result<Option<RemoteMetadata>, AssetPackError> {
    let app_id_text = app_id.to_string();
    let response = client
        .get("https://store.steampowered.com/api/appdetails/")
        .query(&[("appids", app_id_text.as_str()), ("cc", "US"), ("l", "en")])
        .send()
        .map_err(|_| AssetPackError::Codec("Steam app detail request failed".to_string()))?;
    let json = response
        .json::<Value>()
        .map_err(|_| AssetPackError::Codec("Steam app detail response was invalid".to_string()))?;
    let data = json
        .get(&app_id_text)
        .and_then(|entry| entry.get("data"))
        .ok_or_else(|| AssetPackError::Codec("Steam app detail was missing data".to_string()))?;

    let developers = string_array(data.get("developers"));
    let publishers = string_array(data.get("publishers"));
    let genres = data
        .get("genres")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("description")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    let categories = data
        .get("categories")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("description")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    let media_sources = steam_media_sources(data);

    Ok(Some(RemoteMetadata {
        short_description: data
            .get("short_description")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        detailed_description: data
            .get("detailed_description")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        developers,
        publishers,
        release_date: data
            .get("release_date")
            .and_then(|release| release.get("date"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        genres,
        categories,
        achievements: Vec::new(),
        media_sources,
        brand_sources: Vec::new(),
        source: "steam-store".to_string(),
    }))
}

fn steam_media_sources(data: &Value) -> Vec<RemoteMediaSource> {
    let mut sources = Vec::new();

    if let Some(movies) = data.get("movies").and_then(Value::as_array) {
        for (index, movie) in movies.iter().enumerate() {
            let url = movie
                .get("mp4")
                .and_then(|mp4| mp4.get("480").or_else(|| mp4.get("max")))
                .and_then(Value::as_str)
                .or_else(|| {
                    movie
                        .get("webm")
                        .and_then(|webm| webm.get("480").or_else(|| webm.get("max")))
                        .and_then(Value::as_str)
                })
                .or_else(|| movie.get("hls_h264").and_then(Value::as_str))
                .or_else(|| movie.get("dash_h264").and_then(Value::as_str))
                .or_else(|| {
                    movie
                        .get("highlight_movie_webm")
                        .and_then(|webm| webm.get("webm").or_else(|| webm.get("max")))
                        .and_then(Value::as_str)
                });
            let title = movie
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Trailer {}", index + 1));
            if let Some(url) = url {
                sources.push(RemoteMediaSource {
                    id: format!("movie-{index:02}"),
                    role: "video-preview".to_string(), // Frontend uses video-preview for video
                    title: title.clone(),
                    url: url.to_string(),
                    mime_type: if is_stream_manifest_url(url) {
                        "application/vnd.apple.mpegurl".to_string()
                    } else {
                        mime_for_url(url)
                    },
                });
            }
            if let Some(url) = movie.get("thumbnail").and_then(Value::as_str) {
                sources.push(RemoteMediaSource {
                    id: format!("movie-thumb-{index:02}"),
                    role: "video-thumb".to_string(),
                    title,
                    url: url.to_string(),
                    mime_type: mime_for_url(url),
                });
            }
        }
    }

    if let Some(screenshots) = data.get("screenshots").and_then(Value::as_array) {
        for (index, screenshot) in screenshots.iter().enumerate() {
            let Some(url) = screenshot
                .get("path_full")
                .and_then(Value::as_str)
                .or_else(|| screenshot.get("path_thumbnail").and_then(Value::as_str))
            else {
                continue;
            };
            sources.push(RemoteMediaSource {
                id: format!("screenshot-{index:02}"),
                role: "screenshot".to_string(),
                title: format!("Screenshot {}", index + 1),
                url: url.to_string(),
                mime_type: mime_for_url(url),
            });
        }
    }

    sources
}

fn fetch_steam_achievements(
    client: &Client,
    app_id: u64,
) -> Result<Option<Vec<RemoteAchievement>>, AssetPackError> {
    let app_id_text = app_id.to_string();
    with_steam_api_key(|steam_key| {
        client
            .get("https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/")
            .query(&[("key", steam_key), ("appid", app_id_text.as_str())])
            .send()
    })
    .map_err(|_| AssetPackError::Codec("Steam achievement schema request failed".to_string()))
    .and_then(|response| {
        response.json::<Value>().map_err(|_| {
            AssetPackError::Codec("Steam achievement schema response was invalid".to_string())
        })
    })
    .map(|json| {
        json.pointer("/game/availableGameStats/achievements")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim();
                        if name.is_empty() {
                            return None;
                        }
                        Some(RemoteAchievement {
                            name: name.to_string(),
                            description: item
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .to_string(),
                            hidden: item.get("hidden").and_then(Value::as_i64).unwrap_or(0) != 0,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|items| !items.is_empty())
    })
}

fn fetch_steamgriddb_brand_sources(
    client: &Client,
    app_id: u64,
) -> Result<Vec<RemoteMediaSource>, AssetPackError> {
    let endpoints = [
        ("grid", "grids", 1_usize),
        ("hero", "heroes", 1_usize),
        ("logo", "logos", 1_usize),
        ("icon", "icons", 1_usize),
    ];
    let mut sources = Vec::new();
    for (role, endpoint, limit) in endpoints {
        let mut role_sources =
            fetch_steamgriddb_role_sources(client, app_id, endpoint, role, limit)?;
        sources.append(&mut role_sources);
    }
    Ok(sources)
}

fn fetch_steamgriddb_role_sources(
    client: &Client,
    app_id: u64,
    endpoint: &str,
    role: &str,
    limit: usize,
) -> Result<Vec<RemoteMediaSource>, AssetPackError> {
    let url = format!("https://www.steamgriddb.com/api/v2/{endpoint}/steam/{app_id}");
    let response = with_steamgriddb_key(|sgdb_key| {
        let mut header = String::with_capacity(7 + sgdb_key.len());
        header.push_str("Bearer ");
        header.push_str(sgdb_key);
        let result = client
            .get(&url)
            .header("Authorization", header.as_str())
            .send();
        header.clear();
        result
    })
    .map_err(|_| AssetPackError::Codec("SteamGridDB brand asset request failed".to_string()))?;
    let json = response.json::<Value>().map_err(|_| {
        AssetPackError::Codec("SteamGridDB brand asset response was invalid".to_string())
    })?;
    Ok(json
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.get("url").and_then(Value::as_str)?;
                    Some(RemoteMediaSource {
                        id: format!(
                            "sgdb-{role}-{}",
                            item.get("id").and_then(Value::as_i64).unwrap_or(0)
                        ),
                        role: role.to_string(),
                        title: format!("SteamGridDB {}", title_from_slug(role)),
                        url: url.to_string(),
                        mime_type: mime_for_url(url),
                    })
                })
                .take(limit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

fn detect_versions() -> Vec<GameVersionInfo> {
    let catalog_path = PathBuf::from(DEFAULT_DEPOT_ROOT).join("catalog.json");
    let mut versions = if let Ok(bytes) = fs::read(catalog_path) {
        serde_json::from_slice::<Catalog>(&bytes)
            .ok()
            .map(|catalog| {
                catalog
                    .versions
                    .iter()
                    .map(|entry| {
                        let size = manifest_total_size(
                            &PathBuf::from(DEFAULT_DEPOT_ROOT).join(&entry.manifest_path),
                        );
                        GameVersionInfo {
                            version: entry.version.clone(),
                            label: entry.version.clone(),
                            build_id: version_build_id(&entry.version),
                            size_bytes: size,
                            latest: Some(entry.version.clone()) == catalog.effective_latest_version().map(str::to_string),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if versions.is_empty() {
        versions = vec![
            GameVersionInfo {
                version: "v1.0".to_string(),
                label: "Release Patch".to_string(),
                build_id: "2338871".to_string(),
                size_bytes: 49_690_000_000,
                latest: false,
            },
            GameVersionInfo {
                version: "v1.1".to_string(),
                label: "Update 1.1".to_string(),
                build_id: "23531465".to_string(),
                size_bytes: 49_690_000_000,
                latest: false,
            },
            GameVersionInfo {
                version: "v1.2".to_string(),
                label: "Update 1.2".to_string(),
                build_id: "23600000".to_string(),
                size_bytes: 49_690_000_000,
                latest: true,
            },
        ];
    }
    versions.sort_by(|a, b| a.version.cmp(&b.version));
    versions
}

fn overlay_summary_depot_versions(game: &mut GameSummary) {
    if let Some((latest, versions)) = load_depot_versions_for_game(&game.id) {
        game.latest_version = latest;
        game.available_versions = versions;
    }
}

fn overlay_detail_depot_versions(detail: &mut GameDetail) {
    if let Some((latest, versions)) = load_depot_versions_for_game(&detail.game_id) {
        detail.versions = versions;
        if let Some(version) = detail
            .versions
            .iter_mut()
            .find(|version| version.version == latest)
        {
            version.latest = true;
        }
    }
}

fn load_depot_versions_for_game(game_id: &str) -> Option<(String, Vec<GameVersionInfo>)> {
    let catalog = load_local_depot_catalog(game_id).or_else(|| fetch_remote_depot_catalog(game_id))?;
    let latest = catalog.effective_latest_version()?.to_string();
    let versions = catalog
        .versions
        .iter()
        .map(|entry| GameVersionInfo {
            version: entry.version.clone(),
            label: entry.version.clone(),
            build_id: version_build_id(&entry.version),
            size_bytes: entry.total_size,
            latest: entry.version == latest,
        })
        .collect::<Vec<_>>();
    if versions.is_empty() {
        None
    } else {
        Some((latest, versions))
    }
}

fn load_local_depot_catalog(game_id: &str) -> Option<Catalog> {
    let path = PathBuf::from(r"E:\007Launcher\depot")
        .join(game_id)
        .join("catalog.json");
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Catalog>(&bytes).ok())
}

fn fetch_remote_depot_catalog(game_id: &str) -> Option<Catalog> {
    let base = std::env::var("OXO_DEPOT_REPO_BASE")
        .unwrap_or_else(|_| DEFAULT_DEPOT_REPO_BASE.to_string());
    let url = format!("{}/{}/catalog.json", base.trim_end_matches('/'), game_id);
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let mut request = client.get(url).header("User-Agent", "0xolemon-launcher/0.1");
    if let Ok(token) = std::env::var("HF_TOKEN").or_else(|_| std::env::var("FIRST_LIGHT_HF_TOKEN"))
    {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    request
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Catalog>()
        .ok()
}

fn manifest_total_size(path: &Path) -> u64 {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<VersionManifest>(&bytes).ok())
        .map(|manifest| manifest.files.iter().map(|file| file.size).sum())
        .unwrap_or(49_690_000_000)
}

fn version_build_id(version: &str) -> String {
    match version {
        "v1.0" => "2338871".to_string(),
        "v1.1" => "23531465".to_string(),
        other => other.trim_start_matches('v').replace('.', ""),
    }
}

fn default_install_metadata() -> GameInstallMetadata {
    GameInstallMetadata {
        default_store_root: DEFAULT_STORE_ROOT.to_string(),
        default_install_folder: DEFAULT_COMMON_GAME.to_string(),
        default_downloading_folder: DEFAULT_DOWNLOADING_GAME.to_string(),
        storage_label: "SSD".to_string(),
        supports_resume: true,
        launch_executable: r"Retail\007FirstLight.exe".to_string(),
    }
}

fn default_i18n() -> HashMap<String, HashMap<String, String>> {
    let mut en = HashMap::new();
    en.insert("library".to_string(), "Library".to_string());
    en.insert("downloads".to_string(), "Downloads".to_string());
    en.insert("chooseInstall".to_string(), "Choose install".to_string());
    en.insert("startDownload".to_string(), "Start download".to_string());
    en.insert(
        "verifyIntegrity".to_string(),
        "Verify file integrity".to_string(),
    );
    en.insert("achievements".to_string(), "Achievements".to_string());
    HashMap::from([("en-US".to_string(), en)])
}

fn asset_id(game_id: &str, role: &str) -> String {
    format!("{game_id}:{role}")
}

fn mime_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn mime_for_url(url: &str) -> String {
    let clean = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if clean.ends_with(".png") {
        "image/png"
    } else if clean.ends_with(".jpg") || clean.ends_with(".jpeg") {
        "image/jpeg"
    } else if clean.ends_with(".gif") {
        "image/gif"
    } else if clean.ends_with(".webp") {
        "image/webp"
    } else if clean.ends_with(".mp4") {
        "video/mp4"
    } else if clean.ends_with(".webm") {
        "video/webm"
    } else {
        "application/octet-stream"
    }
    .to_string()
}

fn relative_to_path(relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<PathBuf>()
}

fn title_from_slug(slug: &str) -> String {
    slug.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn strip_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in input.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                let tag = tag.trim().trim_start_matches('/').to_ascii_lowercase();
                if tag.starts_with("br")
                    || tag.starts_with('p')
                    || tag.starts_with("div")
                    || tag.starts_with("li")
                    || tag.starts_with("h1")
                    || tag.starts_with("h2")
                    || tag.starts_with("h3")
                {
                    output.push('\n');
                } else {
                    output.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    let decoded = output
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ");
    decoded
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, AssetPackError> {
    let mut bytes = [0_u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, AssetPackError> {
    let mut bytes = [0_u8; 8];
    cursor.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn pack_salt() -> [u8; 16] {
    [
        0x31, 0x4f, 0x78, 0x6f, 0x2d, 0x46, 0x69, 0x72, 0x73, 0x74, 0x4c, 0x69, 0x67, 0x68, 0x74,
        0x21,
    ]
}

#[inline(never)]
fn derive_pack_key(salt: &[u8; 16]) -> [u8; 32] {
    derive_asset_pack_key(salt)
}

fn xor_stream(bytes: &mut [u8], salt: &[u8; 16], stream_id: u64) {
    let key = derive_pack_key(salt);
    let mut offset = 0usize;
    let mut counter = 0_u64;
    while offset < bytes.len() {
        let mut hasher = blake3::Hasher::new_keyed(&key);
        hasher.update(salt);
        hasher.update(&stream_id.to_le_bytes());
        hasher.update(&counter.to_le_bytes());
        let block = hasher.finalize();
        for byte in block.as_bytes() {
            if offset >= bytes.len() {
                break;
            }
            bytes[offset] ^= byte.rotate_left(((offset as u32) & 7) + 1);
            offset += 1;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_default_pack, default_asset_source, parse_pack, DEFAULT_GAME_ID, GAME_CORE_PART,
        GAME_PACK_PARTS,
    };

    #[test]
    fn built_pack_loads_and_corrupt_pack_rejects() {
        let source = default_asset_source();
        if !source.exists() {
            return;
        }
        let output = std::env::temp_dir().join(format!(
            "catalog-test-{}.0xo",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::env::set_var("OXO_ASSET_PACK_OFFLINE", "1");
        let summary = build_default_pack(&source, &output).expect("build pack");
        std::env::remove_var("OXO_ASSET_PACK_OFFLINE");
        assert!(summary.asset_count > 0);

        let bytes = std::fs::read(&output).expect("read pack");
        let loaded = parse_pack(bytes.clone()).expect("parse pack");
        assert_eq!(loaded.manifest.catalog.games.len(), 1);

        let game_pack_dir = output
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("games")
            .join(DEFAULT_GAME_ID);
        for part in GAME_PACK_PARTS {
            let part_path = game_pack_dir.join(format!("{part}.0xo"));
            let part_bytes = std::fs::read(&part_path).expect("read split pack");
            let loaded_part = parse_pack(part_bytes).expect("parse split pack");
            if part == GAME_CORE_PART {
                assert!(loaded_part.manifest.details.contains_key(DEFAULT_GAME_ID));
            } else {
                assert!(loaded_part.manifest.catalog.games.is_empty());
            }
            let _ = std::fs::remove_file(part_path);
        }

        let mut corrupt = bytes;
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0x7d;
        assert!(parse_pack(corrupt).is_err());
        let _ = std::fs::remove_file(output);
    }
}
