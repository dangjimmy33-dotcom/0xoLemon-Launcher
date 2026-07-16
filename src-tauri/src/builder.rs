use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use fastcdc::v2020::StreamCDC;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

use crate::depot_crypto::{self, key_id_from_material, DEPOT_ENCRYPTION_ALGORITHM, DEPOT_KEY_ENV};
use crate::manifest::{
    Catalog, CatalogVersion, ChunkCodec, ChunkEncryption, ChunkRef, FileEntry, PackRecord,
    VersionManifest, CHUNK_MAX_SIZE, CHUNK_MIN_SIZE, CHUNK_TARGET_SIZE, FORMAT_VERSION,
    LEGACY_FORMAT_VERSION, PACK_TARGET_SIZE as DEFAULT_PACK_TARGET_SIZE,
};
use crate::scanner::normalize_relative;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("input root does not exist: {0}")]
    MissingInput(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("walk error: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("chunking error: {0}")]
    Chunking(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("publish error: {0}")]
    Publish(String),
    #[error("crypto error: {0}")]
    Crypto(String),
}

#[derive(Debug, Clone)]
pub struct BuildVersionInput {
    pub version: String,
    pub root: PathBuf,
    pub launch_executable: Option<String>,
    pub launch_options: Vec<crate::manifest::LaunchOption>,
}

#[derive(Debug, Clone)]
pub struct DepotEncryptionConfig {
    pub enabled: bool,
    pub key_material: Option<String>,
    pub key_id: Option<String>,
}

impl Default for DepotEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            key_material: None,
            key_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildDepotInput {
    pub game_id: String,
    pub latest_version: String,
    pub output_dir: PathBuf,
    pub versions: Vec<BuildVersionInput>,
    pub publish: Option<PublishTarget>,
    pub extend_existing: bool,
    pub encryption: DepotEncryptionConfig,
    pub pack_target_size: u64,
    pub pack_id_prefix: String,
    pub start_pack_index: Option<usize>,
    /// V1 remains the default in the CLI so repositories used by older launcher
    /// builds never receive raw-coded chunks accidentally. V2 must be explicit.
    pub format_version: u32,
    /// Delete source files immediately after they're packed to save disk space.
    /// Useful when building large depots with limited disk space.
    pub delete_source_after_pack: bool,
    /// Upload and delete each pack immediately after creation (incremental mode).
    /// Requires publish target. Saves disk space by keeping only 1 pack at a time.
    /// Useful when building very large depots (50GB+) with limited disk space.
    pub upload_packs_incrementally: bool,
}

#[derive(Debug, Clone)]
pub struct PublishTarget {
    pub repo_id: String,
    pub repo_type: String,
    pub repo_prefix: String,
    pub delete_local_packs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub game_id: String,
    pub output_dir: String,
    pub catalog_path: String,
    pub versions: Vec<CatalogVersion>,
    pub packs: Vec<PackRecord>,
}

#[derive(Debug, Clone)]
struct ChunkLocation {
    hash: String,
    pack_id: String,
    pack_offset: u64,
    compressed_size: u64,
    compressed_sha256: String,
    uncompressed_size: u64,
    codec: ChunkCodec,
    encryption: Option<ChunkEncryption>,
}

struct PackWriter {
    id: String,
    path: PathBuf,
    file: File,
    size: u64,
    hasher: Sha256,
}

impl PackWriter {
    fn create(pack_dir: &Path, id_prefix: &str, index: usize) -> Result<Self, io::Error> {
        let id = format!("{id_prefix}{index:05}");
        let path = pack_dir.join(format!("{id}.bin"));
        let file = File::create(&path)?;
        Ok(Self {
            id,
            path,
            file,
            size: 0,
            hasher: Sha256::new(),
        })
    }

    fn write_chunk(&mut self, compressed: &[u8]) -> Result<u64, io::Error> {
        let offset = self.size;
        self.file.write_all(compressed)?;
        self.hasher.update(compressed);
        self.size += compressed.len() as u64;
        Ok(offset)
    }

    fn finalize(mut self, root: &Path) -> Result<PackRecord, io::Error> {
        self.file.flush()?;
        let path = self
            .path
            .strip_prefix(root)
            .unwrap_or(&self.path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        Ok(PackRecord {
            id: self.id,
            path,
            size: self.size,
            sha256: hex::encode(self.hasher.finalize()),
        })
    }
}

pub fn build_depot(input: BuildDepotInput) -> Result<BuildReport, BuildError> {
    if !matches!(input.format_version, LEGACY_FORMAT_VERSION | FORMAT_VERSION) {
        return Err(BuildError::Chunking(format!(
            "unsupported depot format version: {}",
            input.format_version
        )));
    }
    for version in &input.versions {
        if !version.root.exists() {
            return Err(BuildError::MissingInput(version.root.display().to_string()));
        }
    }

    let manifest_dir = input.output_dir.join("manifests");
    let pack_dir = input.output_dir.join("packs");
    let version_dir = input.output_dir.join("versions");
    fs::create_dir_all(&manifest_dir)?;
    fs::create_dir_all(&pack_dir)?;
    fs::create_dir_all(&version_dir)?;

    let existing_catalog = if input.extend_existing {
        load_existing_catalog(&input.output_dir)?
    } else {
        None
    };
    let mut chunk_locations = HashMap::new();
    let mut pack_records = existing_catalog
        .as_ref()
        .map(|catalog| catalog.packs.clone())
        .unwrap_or_default();
    if let Some(catalog) = existing_catalog.as_ref() {
        seed_chunk_locations_from_existing_manifests(
            &input.output_dir,
            catalog,
            &mut chunk_locations,
        )?;
    }
    let pack_target_size = effective_pack_target_size(input.pack_target_size);
    let pack_id_prefix = normalize_pack_id_prefix(&input.pack_id_prefix);
    let requested_start_index = input.start_pack_index.unwrap_or(0);
    let mut current_pack: Option<PackWriter> = None;
    let mut next_pack_index = if input.extend_existing {
        next_pack_index(&pack_records, &pack_id_prefix).max(requested_start_index)
    } else {
        requested_start_index
    };
    eprintln!(
        "[DEPOT] pack target: {} MiB | pack prefix: {} | start index: {}",
        pack_target_size / 1024 / 1024,
        pack_id_prefix,
        next_pack_index
    );
    let replacing_versions = input
        .versions
        .iter()
        .map(|version| version.version.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut catalog_versions = existing_catalog
        .as_ref()
        .map(|catalog| {
            catalog
                .versions
                .iter()
                .filter(|version| !replacing_versions.contains(version.version.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut metadata_uploads = Vec::<(PathBuf, String)>::new();

    for version_input in &input.versions {
        let created_at = Utc::now().to_rfc3339();
        let mut files = Vec::new();
        let mut total_size = 0u64;
        let mut file_paths = Vec::new();

        for entry in WalkDir::new(&version_input.root).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_file() {
                file_paths.push(entry.into_path());
            }
        }
        file_paths.sort();

        for file_path in file_paths {
            let file_entry = build_file_entry(
                &version_input.root,
                &file_path,
                &pack_dir,
                &input.output_dir,
                &mut current_pack,
                &mut next_pack_index,
                &mut pack_records,
                &mut chunk_locations,
                input.publish.as_ref(),
                &input.encryption,
                input.format_version,
                pack_target_size,
                &pack_id_prefix,
                input.upload_packs_incrementally,
            )?;
            total_size += file_entry.size;
            files.push(file_entry);

            // Delete source file immediately after packing to save disk space
            if input.delete_source_after_pack {
                if let Err(err) = fs::remove_file(&file_path) {
                    eprintln!("[DEPOT] Warning: failed to delete source file {}: {}", file_path.display(), err);
                } else {
                    eprintln!("[DEPOT] Deleted source file: {}", file_path.display());
                }
            }
        }

        let chunk_count = files.iter().map(|file| file.chunks.len()).sum();
        let manifest = VersionManifest {
            format_version: input.format_version,
            game_id: input.game_id.clone(),
            version: version_input.version.clone(),
            created_at: created_at.clone(),
            root_label: format!("{} {}", input.game_id, version_input.version),
            launch_executable: version_input.launch_executable.clone(),
            launch_options: version_input.launch_options.clone(),
            total_size,
            files,
            signature: None,
        };
        let version_output_dir = version_dir.join(&version_input.version);
        fs::create_dir_all(&version_output_dir)?;
        let manifest_path = version_output_dir.join("manifest.json");
        let legacy_manifest_path = manifest_dir.join(format!("{}.json", version_input.version));
        let build_info_path = version_output_dir.join("build-info.json");
        write_json_pretty(&manifest_path, &manifest)?;
        write_json_pretty(&legacy_manifest_path, &manifest)?;
        write_json_pretty(
            &build_info_path,
            &serde_json::json!({
                "gameId": input.game_id.clone(),
                "version": version_input.version.clone(),
                "createdAt": created_at,
                "sourceLabel": format!("{} {}", input.game_id, version_input.version),
                "launchExecutable": version_input.launch_executable.clone(),
                "launchOptions": version_input.launch_options.clone(),
                "totalSize": total_size,
                "fileCount": manifest.files.len(),
                "chunkCount": chunk_count,
                "chunkTargetSize": CHUNK_TARGET_SIZE,
                "packTargetSize": pack_target_size,
                "packTargetSizeMiB": pack_target_size / 1024 / 1024,
                "packIdPrefix": pack_id_prefix.clone(),
                "packStartIndex": requested_start_index,
                "encryptedPacks": input.encryption.enabled,
                "formatVersion": input.format_version,
                "depotKeyEnv": DEPOT_KEY_ENV
            }),
        )?;
        metadata_uploads.push((
            manifest_path.clone(),
            format!("versions/{}/manifest.json", version_input.version),
        ));
        metadata_uploads.push((
            legacy_manifest_path.clone(),
            format!("manifests/{}.json", version_input.version),
        ));
        metadata_uploads.push((
            build_info_path,
            format!("versions/{}/build-info.json", version_input.version),
        ));
        catalog_versions.push(CatalogVersion {
            version: version_input.version.clone(),
            manifest_path: format!("versions/{}/manifest.json", version_input.version),
            total_size,
            file_count: manifest.files.len(),
            chunk_count,
            created_at,
        });
    }

    if let Some(pack) = current_pack.take() {
        if pack.size > 0 {
            finalize_pack(
                pack,
                &input.output_dir,
                input.publish.as_ref(),
                &mut pack_records,
                input.encryption.enabled,
                input.upload_packs_incrementally,
            )?;
        }
    }

    let catalog = Catalog {
        format_version: input.format_version,
        game_id: input.game_id.clone(),
        latest_version: Some(input.latest_version.clone()),
        versions: catalog_versions.clone(),
        packs: pack_records.clone(),
        signature: None,
    };
    let catalog_path = input.output_dir.join("catalog.json");
    write_json_pretty(&catalog_path, &catalog)?;
    metadata_uploads.push((catalog_path.clone(), "catalog.json".to_string()));

    if let Some(publish) = input.publish.as_ref() {
        for (local_path, remote_path) in &metadata_uploads {
            upload_owned_file(publish, local_path, remote_path)?;
        }
    }

    Ok(BuildReport {
        game_id: input.game_id,
        output_dir: input.output_dir.display().to_string(),
        catalog_path: catalog_path.display().to_string(),
        versions: catalog_versions,
        packs: pack_records,
    })
}

fn build_file_entry(
    root: &Path,
    file_path: &Path,
    pack_dir: &Path,
    output_dir: &Path,
    current_pack: &mut Option<PackWriter>,
    next_pack_index: &mut usize,
    pack_records: &mut Vec<PackRecord>,
    chunk_locations: &mut HashMap<String, ChunkLocation>,
    publish: Option<&PublishTarget>,
    encryption: &DepotEncryptionConfig,
    format_version: u32,
    pack_target_size: u64,
    pack_id_prefix: &str,
    upload_incrementally: bool,
) -> Result<FileEntry, BuildError> {
    let metadata = fs::metadata(file_path)?;
    let source = File::open(file_path)?;
    let mut chunker = StreamCDC::new(source, CHUNK_MIN_SIZE, CHUNK_TARGET_SIZE, CHUNK_MAX_SIZE);
    let mut file_hasher = Sha256::new();
    let mut chunks = Vec::new();

    for result in &mut chunker {
        let chunk = result.map_err(|err| BuildError::Chunking(err.to_string()))?;
        file_hasher.update(&chunk.data);
        let hash = blake3::hash(&chunk.data).to_hex().to_string();

        let reusable_existing = chunk_locations
            .get(&hash)
            .filter(|existing| chunk_location_matches_encryption(existing, encryption))
            .cloned();

        let location = if let Some(existing) = reusable_existing {
            existing
        } else {
            let (codec, encoded) = encode_chunk_payload(&chunk.data, format_version)?;
            let plaintext_compressed_sha256 = sha256_bytes(&encoded);
            let plaintext_compressed_size = encoded.len() as u64;
            let uncompressed_size = chunk.data.len() as u64;

            let (transport_bytes, encryption_meta) = if encryption.enabled {
                let key_material =
                    depot_crypto::resolve_key_material(encryption.key_material.as_deref());
                let key_id = encryption
                    .key_id
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| key_id_from_material(&key_material));
                let (encrypted, nonce) = depot_crypto::encrypt_compressed_chunk(
                    &encoded,
                    &hash,
                    &plaintext_compressed_sha256,
                    &key_material,
                )
                .map_err(|err| BuildError::Crypto(err.to_string()))?;
                (
                    encrypted,
                    Some(ChunkEncryption {
                        algorithm: DEPOT_ENCRYPTION_ALGORITHM.to_string(),
                        key_id,
                        nonce,
                        plaintext_compressed_size,
                        plaintext_compressed_sha256: plaintext_compressed_sha256.clone(),
                    }),
                )
            } else {
                (encoded, None)
            };
            let compressed_sha256 = sha256_bytes(&transport_bytes);

            if current_pack
                .as_ref()
                .map(|pack| {
                    pack.size + transport_bytes.len() as u64 > pack_target_size && pack.size > 0
                })
                .unwrap_or(true)
            {
                if let Some(pack) = current_pack.take() {
                    if pack.size > 0 {
                        finalize_pack(pack, output_dir, publish, pack_records, encryption.enabled, upload_incrementally)?;
                    }
                }
                let pack = PackWriter::create(pack_dir, pack_id_prefix, *next_pack_index)?;
                *next_pack_index += 1;
                *current_pack = Some(pack);
            }

            let pack = current_pack.as_mut().expect("pack writer must exist");
            let pack_offset = pack.write_chunk(&transport_bytes)?;
            let location = ChunkLocation {
                hash: hash.clone(),
                pack_id: pack.id.clone(),
                pack_offset,
                compressed_size: transport_bytes.len() as u64,
                compressed_sha256,
                uncompressed_size,
                codec,
                encryption: encryption_meta,
            };
            chunk_locations.insert(hash.clone(), location.clone());
            location
        };

        chunks.push(ChunkRef {
            hash: location.hash,
            file_offset: chunk.offset,
            uncompressed_size: location.uncompressed_size,
            pack_id: location.pack_id,
            pack_offset: location.pack_offset,
            compressed_size: location.compressed_size,
            compressed_sha256: location.compressed_sha256,
            codec: location.codec,
            encryption: location.encryption,
        });
    }

    let path = normalize_relative(root, file_path);
    Ok(FileEntry {
        path: path.clone(),
        size: metadata.len(),
        sha256: hex::encode(file_hasher.finalize()),
        chunks,
        executable: path.to_ascii_lowercase().ends_with(".exe"),
    })
}

fn finalize_pack(
    pack: PackWriter,
    output_dir: &Path,
    publish: Option<&PublishTarget>,
    pack_records: &mut Vec<PackRecord>,
    expect_encrypted: bool,
    upload_incrementally: bool,
) -> Result<(), BuildError> {
    let record = pack.finalize(output_dir)?;
    let local_path = output_dir.join(relative_to_path(&record.path));

    if expect_encrypted && pack_starts_with_zstd_magic(&local_path)? {
        return Err(BuildError::Crypto(format!(
            "encryption was enabled, but {} starts with ZSTD magic 28 B5 2F FD; refusing to upload plain pack",
            record.path
        )));
    }

    if let Some(publish) = publish {
        upload_owned_file(publish, &local_path, &record.path)?;
        // In incremental mode, ALWAYS delete pack after upload to save disk space
        if upload_incrementally || publish.delete_local_packs {
            eprintln!("[DISK] Deleting pack after upload: {}", record.path);
            fs::remove_file(&local_path)?;
        }
    }
    pack_records.push(record);
    Ok(())
}

fn pack_starts_with_zstd_magic(path: &Path) -> Result<bool, BuildError> {
    use std::io::Read;
    let mut file = File::open(path)?;
    let mut magic = [0_u8; 4];
    let read = file.read(&mut magic)?;
    Ok(read == 4 && magic == [0x28, 0xB5, 0x2F, 0xFD])
}

fn chunk_location_matches_encryption(
    existing: &ChunkLocation,
    encryption: &DepotEncryptionConfig,
) -> bool {
    if encryption.enabled {
        let Some(meta) = existing.encryption.as_ref() else {
            return false;
        };
        if meta.algorithm != DEPOT_ENCRYPTION_ALGORITHM {
            return false;
        }
        let key_material = depot_crypto::resolve_key_material(encryption.key_material.as_deref());
        let expected_key_id = encryption
            .key_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| key_id_from_material(&key_material));
        meta.key_id == expected_key_id
    } else {
        existing.encryption.is_none()
    }
}

fn upload_owned_file(
    publish: &PublishTarget,
    local_path: &Path,
    remote_relative_path: &str,
) -> Result<(), BuildError> {
    let remote_path = join_repo_path(&publish.repo_prefix, remote_relative_path);
    println!(
        "publishing {} -> {}/{}",
        local_path.display(),
        publish.repo_id,
        remote_path
    );
    let status = Command::new("hf")
        .arg("upload")
        .arg(&publish.repo_id)
        .arg(local_path)
        .arg(&remote_path)
        .arg("--repo-type")
        .arg(&publish.repo_type)
        .arg("--commit-message")
        .arg(format!("Upload {remote_path}"))
        .status()
        .map_err(|err| BuildError::Publish(format!("failed to start hf upload: {err}")))?;

    if !status.success() {
        return Err(BuildError::Publish(format!(
            "hf upload failed for {remote_path} with status {status}"
        )));
    }
    Ok(())
}

fn join_repo_path(prefix: &str, relative_path: &str) -> String {
    let prefix = prefix.trim_matches('/');
    let relative_path = relative_path.trim_matches('/');
    if prefix.is_empty() {
        relative_path.to_string()
    } else {
        format!("{prefix}/{relative_path}")
    }
}

fn relative_to_path(relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<PathBuf>()
}

fn load_existing_catalog(output_dir: &Path) -> Result<Option<Catalog>, BuildError> {
    let path = output_dir.join("catalog.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn seed_chunk_locations_from_existing_manifests(
    output_dir: &Path,
    catalog: &Catalog,
    chunk_locations: &mut HashMap<String, ChunkLocation>,
) -> Result<(), BuildError> {
    for version in &catalog.versions {
        let manifest_path = output_dir.join(relative_to_path(&version.manifest_path));
        if !manifest_path.exists() {
            continue;
        }
        let bytes = fs::read(manifest_path)?;
        let manifest: VersionManifest = serde_json::from_slice(&bytes)?;
        for file in manifest.files {
            for chunk in file.chunks {
                chunk_locations
                    .entry(chunk.hash.clone())
                    .or_insert_with(|| ChunkLocation {
                        hash: chunk.hash,
                        pack_id: chunk.pack_id,
                        pack_offset: chunk.pack_offset,
                        compressed_size: chunk.compressed_size,
                        compressed_sha256: chunk.compressed_sha256,
                        uncompressed_size: chunk.uncompressed_size,
                        codec: chunk.codec,
                        encryption: chunk.encryption,
                    });
            }
        }
    }
    Ok(())
}

fn next_pack_index(packs: &[PackRecord], id_prefix: &str) -> usize {
    packs
        .iter()
        .filter_map(|pack| {
            pack.id
                .strip_prefix(id_prefix)
                .and_then(|suffix| suffix.parse::<usize>().ok())
        })
        .max()
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn effective_pack_target_size(value: u64) -> u64 {
    if value == 0 {
        DEFAULT_PACK_TARGET_SIZE
    } else {
        value
    }
}

fn normalize_pack_id_prefix(value: &str) -> String {
    let mut out = value
        .trim()
        .chars()
        .filter_map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => Some(ch),
            ' ' | '.' => Some('-'),
            _ => None,
        })
        .collect::<String>();
    if out.is_empty() {
        out = "pack-".to_string();
    }
    out
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn encode_chunk_payload(
    data: &[u8],
    format_version: u32,
) -> Result<(ChunkCodec, Vec<u8>), io::Error> {
    let compressed = zstd::bulk::compress(data, 10)?;
    let use_raw = format_version >= FORMAT_VERSION
        && compressed.len().saturating_mul(10_000) >= data.len().saturating_mul(9_850);
    if use_raw {
        Ok((ChunkCodec::Raw, data.to_vec()))
    } else {
        Ok((ChunkCodec::Zstd, compressed))
    }
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), BuildError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_always_uses_zstd_for_backward_compatibility() {
        let mut data = vec![0_u8; 64 * 1024];
        for (index, byte) in data.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(73) ^ index.wrapping_mul(19).rotate_left(3)) as u8;
        }
        let (codec, _) = encode_chunk_payload(&data, LEGACY_FORMAT_VERSION).unwrap();
        assert_eq!(codec, ChunkCodec::Zstd);
    }

    #[test]
    fn v2_keeps_compressible_chunks_as_zstd() {
        let data = vec![b'A'; 64 * 1024];
        let (codec, encoded) = encode_chunk_payload(&data, FORMAT_VERSION).unwrap();
        assert_eq!(codec, ChunkCodec::Zstd);
        assert!(encoded.len() < data.len() / 10);
    }

    #[test]
    fn v2_stores_incompressible_chunks_raw() {
        let mut state = 0x1234_5678_u32;
        let mut data = vec![0_u8; 64 * 1024];
        for byte in &mut data {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        let (codec, encoded) = encode_chunk_payload(&data, FORMAT_VERSION).unwrap();
        assert_eq!(codec, ChunkCodec::Raw);
        assert_eq!(encoded, data);
    }
}
