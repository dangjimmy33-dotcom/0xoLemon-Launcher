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

use crate::manifest::{
    Catalog, CatalogVersion, ChunkRef, FileEntry, PackRecord, VersionManifest, CHUNK_MAX_SIZE,
    CHUNK_MIN_SIZE, CHUNK_TARGET_SIZE, FORMAT_VERSION, PACK_TARGET_SIZE,
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
}

#[derive(Debug, Clone)]
pub struct BuildVersionInput {
    pub version: String,
    pub root: PathBuf,
    pub launch_executable: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BuildDepotInput {
    pub game_id: String,
    pub latest_version: String,
    pub output_dir: PathBuf,
    pub versions: Vec<BuildVersionInput>,
    pub publish: Option<PublishTarget>,
    pub extend_existing: bool,
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
}

struct PackWriter {
    id: String,
    path: PathBuf,
    file: File,
    size: u64,
    hasher: Sha256,
}

impl PackWriter {
    fn create(pack_dir: &Path, index: usize) -> Result<Self, io::Error> {
        let id = format!("pack-{index:05}");
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
    let mut current_pack: Option<PackWriter> = None;
    let mut next_pack_index = next_pack_index(&pack_records);
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
            )?;
            total_size += file_entry.size;
            files.push(file_entry);
        }

        let chunk_count = files.iter().map(|file| file.chunks.len()).sum();
        let manifest = VersionManifest {
            format_version: FORMAT_VERSION,
            game_id: input.game_id.clone(),
            version: version_input.version.clone(),
            created_at: created_at.clone(),
            root_label: format!("{} {}", input.game_id, version_input.version),
            launch_executable: version_input.launch_executable.clone(),
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
                "totalSize": total_size,
                "fileCount": manifest.files.len(),
                "chunkCount": chunk_count,
                "chunkTargetSize": CHUNK_TARGET_SIZE,
                "packTargetSize": PACK_TARGET_SIZE
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
            )?;
        }
    }

    let catalog = Catalog {
        format_version: FORMAT_VERSION,
        game_id: input.game_id.clone(),
        latest_version: input.latest_version.clone(),
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

        let location = if let Some(existing) = chunk_locations.get(&hash) {
            existing.clone()
        } else {
            let compressed = zstd::bulk::compress(&chunk.data, 10)?;
            let compressed_sha256 = sha256_bytes(&compressed);
            let uncompressed_size = chunk.data.len() as u64;

            if current_pack
                .as_ref()
                .map(|pack| pack.size + compressed.len() as u64 > PACK_TARGET_SIZE && pack.size > 0)
                .unwrap_or(true)
            {
                if let Some(pack) = current_pack.take() {
                    if pack.size > 0 {
                        finalize_pack(pack, output_dir, publish, pack_records)?;
                    }
                }
                let pack = PackWriter::create(pack_dir, *next_pack_index)?;
                *next_pack_index += 1;
                *current_pack = Some(pack);
            }

            let pack = current_pack.as_mut().expect("pack writer must exist");
            let pack_offset = pack.write_chunk(&compressed)?;
            let location = ChunkLocation {
                hash: hash.clone(),
                pack_id: pack.id.clone(),
                pack_offset,
                compressed_size: compressed.len() as u64,
                compressed_sha256,
                uncompressed_size,
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
) -> Result<(), BuildError> {
    let record = pack.finalize(output_dir)?;
    if let Some(publish) = publish {
        let local_path = output_dir.join(relative_to_path(&record.path));
        upload_owned_file(publish, &local_path, &record.path)?;
        if publish.delete_local_packs {
            fs::remove_file(&local_path)?;
        }
    }
    pack_records.push(record);
    Ok(())
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
                    });
            }
        }
    }
    Ok(())
}

fn next_pack_index(packs: &[PackRecord]) -> usize {
    packs
        .iter()
        .filter_map(|pack| {
            pack.id
                .strip_prefix("pack-")
                .and_then(|suffix| suffix.parse::<usize>().ok())
        })
        .max()
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), BuildError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}
