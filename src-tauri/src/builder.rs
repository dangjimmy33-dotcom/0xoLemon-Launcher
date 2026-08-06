use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use fastcdc::v2020::StreamCDC;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, RANGE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

use crate::depot_crypto::{self, key_id_from_material, DEPOT_ENCRYPTION_ALGORITHM, DEPOT_KEY_ENV};
use crate::manifest::{
    Catalog, CatalogVersion, ChunkCodec, ChunkEncryption, ChunkRef, DeltaPatch, FileEntry,
    PackRecord, VersionManifest, CHUNK_MAX_SIZE, CHUNK_MIN_SIZE, CHUNK_TARGET_SIZE, FORMAT_VERSION,
    LEGACY_FORMAT_VERSION, PACK_TARGET_SIZE as DEFAULT_PACK_TARGET_SIZE,
};
use crate::scanner::normalize_relative;

const BUILD_LEGACY_OXIDELTA: bool = false;

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
    #[error("validation error: {0}")]
    Validation(String),
    #[error("validation request failed: {0}")]
    ValidationRequest(String),
}

#[derive(Debug, Clone)]
pub struct BuildVersionInput {
    pub version: String,
    pub root: PathBuf,
    pub launch_executable: Option<String>,
    pub launch_options: Vec<crate::manifest::LaunchOption>,
    pub dependencies: Option<Vec<String>>,
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
    /// Glob patterns (using `/` separators, `*` matches within a segment, `**`
    /// matches across segments). Files whose depot-relative path matches ANY
    /// pattern are NOT re-chunked.  Instead their FileEntry is inherited verbatim
    /// from the most-recent existing manifest that contains that path.
    /// Set of glob patterns. Files whose path matches any of these patterns will NOT be re-chunked
    /// if they exist in the previous version's manifest.
    pub skip_file_patterns: Vec<String>,
    pub preserve_patterns: Vec<String>,
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
    #[serde(default)]
    pub transitions: Vec<BuildTransitionReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildTransitionReport {
    pub base_version: String,
    pub target_version: String,
    pub target_bytes: u64,
    pub reused_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteChunkVerificationReport {
    pub version: String,
    pub file_path: String,
    pub chunk_index: usize,
    pub pack_id: String,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub encrypted: bool,
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
    // Seed chunk dedup locations from all existing manifests.
    if let Some(catalog) = existing_catalog.as_ref() {
        seed_chunk_locations_from_existing_manifests(
            &input.output_dir,
            catalog,
            &mut chunk_locations,
        )?;
    }
    // Build a lookup of inherited FileEntry values for any path that matches a
    // skip pattern.  We use the LATEST manifest in the catalog that contains
    // the file so that callers always get the most-recent known good entry.
    let inherited_entries: HashMap<String, FileEntry> = if !input.skip_file_patterns.is_empty() {
        if let Some(catalog) = existing_catalog.as_ref() {
            seed_inherited_file_entries(&input.output_dir, catalog, &input.skip_file_patterns)?
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };
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

    let mut previous_version_root: Option<PathBuf> = None;
    let mut previous_version_name: Option<String> = None;

    for version_input in &input.versions {
        let created_at = Utc::now().to_rfc3339();
        let mut files = Vec::new();
        let mut total_size = 0u64;
        let mut file_paths = Vec::new();
        let mut packed_source_paths = Vec::new();

        for entry in WalkDir::new(&version_input.root).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_file() {
                file_paths.push(entry.into_path());
            }
        }
        file_paths.sort();

        for file_path in file_paths {
            // Check whether this file should be skipped and inherited from a
            // previous manifest instead of re-chunked.
            let rel_path = file_path
                .strip_prefix(&version_input.root)
                .unwrap_or(&file_path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");

            let preserve = (!input.preserve_patterns.is_empty())
                .then(|| {
                    input
                        .preserve_patterns
                        .iter()
                        .any(|pat| glob_matches(pat, &rel_path))
                })
                .unwrap_or(false);

            let inherited = (!input.skip_file_patterns.is_empty())
                .then(|| {
                    input
                        .skip_file_patterns
                        .iter()
                        .any(|pat| glob_matches(pat, &rel_path))
                })
                .unwrap_or(false);

            let file_entry = if inherited {
                if let Some(entry) = inherited_entries.get(&rel_path) {
                    eprintln!("[DEPOT] Inheriting from previous manifest: {rel_path}");
                    entry.clone()
                } else {
                    eprintln!("[DEPOT] WARNING: skip pattern matched '{rel_path}' but no previous manifest entry found — building normally");
                    build_file_entry(
                        &version_input.root,
                        &file_path,
                        previous_version_root.as_deref(),
                        previous_version_name.as_deref(),
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
                        preserve,
                    )?
                }
            } else {
                build_file_entry(
                    &version_input.root,
                    &file_path,
                    previous_version_root.as_deref(),
                    previous_version_name.as_deref(),
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
                    preserve,
                )?
            };
            total_size += file_entry.size;
            files.push(file_entry);

            if input.delete_source_after_pack && !inherited {
                packed_source_paths.push(file_path);
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
            dependencies: version_input.dependencies.clone(),
            total_size,
            files,
            signature: None,
        };
        validate_manifest_against_root(&manifest, &version_input.root)?;
        for source_path in packed_source_paths {
            if let Err(error) = fs::remove_file(&source_path) {
                eprintln!(
                    "[DEPOT] Warning: failed to delete source file {}: {}",
                    source_path.display(),
                    error
                );
            } else {
                eprintln!("[DEPOT] Deleted source file: {}", source_path.display());
            }
        }
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
                "dependencies": version_input.dependencies.clone(),
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

        previous_version_root = Some(version_input.root.clone());
        previous_version_name = Some(version_input.version.clone());
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
    let transitions = validate_catalog_manifests(&input.output_dir, &catalog)?;
    for transition in &transitions {
        eprintln!(
            "[DEPOT] {} -> {}: target={} reused={} download={}",
            transition.base_version,
            transition.target_version,
            transition.target_bytes,
            transition.reused_bytes,
            transition.download_bytes
        );
    }
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
        transitions,
    })
}

fn validate_manifest_against_root(
    manifest: &VersionManifest,
    root: &Path,
) -> Result<(), BuildError> {
    let mut total_size = 0_u64;
    let mut seen_paths = HashSet::new();
    for entry in &manifest.files {
        let relative = relative_to_path(&entry.path);
        if relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            return Err(BuildError::Validation(format!(
                "unsafe manifest path in {}: {}",
                manifest.version, entry.path
            )));
        }
        let normalized = entry.path.replace('\\', "/").to_ascii_lowercase();
        if !seen_paths.insert(normalized) {
            return Err(BuildError::Validation(format!(
                "duplicate manifest path in {}: {}",
                manifest.version, entry.path
            )));
        }
        let source_path = root.join(relative);
        let metadata = fs::metadata(&source_path).map_err(|error| {
            BuildError::Validation(format!(
                "target source is missing for {}: {} ({error})",
                manifest.version, entry.path
            ))
        })?;
        if metadata.len() != entry.size {
            return Err(BuildError::Validation(format!(
                "target size mismatch for {} in {}: manifest={}, source={}",
                entry.path,
                manifest.version,
                entry.size,
                metadata.len()
            )));
        }
        let actual_sha256 = sha256_file(&source_path)?;
        if !actual_sha256.eq_ignore_ascii_case(&entry.sha256) {
            return Err(BuildError::Validation(format!(
                "target SHA-256 mismatch for {} in {}",
                entry.path, manifest.version
            )));
        }
        validate_file_chunk_layout(entry, None)?;
        total_size = total_size.saturating_add(entry.size);
    }
    if total_size != manifest.total_size {
        return Err(BuildError::Validation(format!(
            "manifest total size mismatch for {}: manifest={}, files={}",
            manifest.version, manifest.total_size, total_size
        )));
    }
    Ok(())
}

fn validate_catalog_manifests(
    output_dir: &Path,
    catalog: &Catalog,
) -> Result<Vec<BuildTransitionReport>, BuildError> {
    let mut packs = HashMap::<String, &PackRecord>::new();
    for pack in &catalog.packs {
        if packs.insert(pack.id.clone(), pack).is_some() {
            return Err(BuildError::Validation(format!(
                "duplicate pack id: {}",
                pack.id
            )));
        }
    }

    let mut manifests = Vec::with_capacity(catalog.versions.len());
    let mut chunk_sizes = HashMap::<String, u64>::new();
    for version in &catalog.versions {
        let path = output_dir.join(relative_to_path(&version.manifest_path));
        let bytes = fs::read(&path).map_err(|error| {
            BuildError::Validation(format!(
                "catalog version {} has no local manifest {} ({error})",
                version.version,
                path.display()
            ))
        })?;
        let manifest: VersionManifest = serde_json::from_slice(&bytes)?;
        if manifest.version != version.version || manifest.game_id != catalog.game_id {
            return Err(BuildError::Validation(format!(
                "catalog ownership mismatch for version {}",
                version.version
            )));
        }
        if manifest.total_size != version.total_size
            || manifest.files.len() != version.file_count
            || manifest
                .files
                .iter()
                .map(|file| file.chunks.len())
                .sum::<usize>()
                != version.chunk_count
        {
            return Err(BuildError::Validation(format!(
                "catalog counters do not match manifest {}",
                version.version
            )));
        }
        let mut total_size = 0_u64;
        for file in &manifest.files {
            validate_file_chunk_layout(file, Some(&packs))?;
            total_size = total_size.saturating_add(file.size);
            for chunk in &file.chunks {
                if let Some(existing) = chunk_sizes.get(&chunk.hash) {
                    if *existing != chunk.uncompressed_size {
                        return Err(BuildError::Validation(format!(
                            "chunk {} has conflicting uncompressed sizes across manifests",
                            chunk.hash
                        )));
                    }
                } else {
                    chunk_sizes.insert(chunk.hash.clone(), chunk.uncompressed_size);
                }
            }
        }
        if total_size != manifest.total_size {
            return Err(BuildError::Validation(format!(
                "manifest file sizes do not add up for {}",
                manifest.version
            )));
        }
        manifests.push(manifest);
    }

    let mut reports = Vec::new();
    for pair in manifests.windows(2) {
        let base = &pair[0];
        let target = &pair[1];
        let base_hashes = base
            .files
            .iter()
            .flat_map(|file| file.chunks.iter().map(|chunk| chunk.hash.as_str()))
            .collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        let mut reused_bytes = 0_u64;
        let mut download_bytes = 0_u64;
        for chunk in target.files.iter().flat_map(|file| file.chunks.iter()) {
            if !seen.insert(chunk.hash.as_str()) {
                continue;
            }
            if base_hashes.contains(chunk.hash.as_str()) {
                reused_bytes = reused_bytes.saturating_add(chunk.uncompressed_size);
            } else {
                download_bytes = download_bytes.saturating_add(chunk.compressed_size);
            }
        }
        reports.push(BuildTransitionReport {
            base_version: base.version.clone(),
            target_version: target.version.clone(),
            target_bytes: target.total_size,
            reused_bytes,
            download_bytes,
        });
    }
    Ok(reports)
}

pub fn validate_depot_output(output_dir: &Path) -> Result<Vec<BuildTransitionReport>, BuildError> {
    let catalog_bytes = fs::read(output_dir.join("catalog.json"))?;
    let catalog: Catalog = serde_json::from_slice(&catalog_bytes)?;
    validate_catalog_manifests(output_dir, &catalog)
}

pub fn verify_remote_chunk(
    output_dir: &Path,
    remote_base: &str,
    version: &str,
    file_path: &str,
    chunk_index: usize,
) -> Result<RemoteChunkVerificationReport, BuildError> {
    let catalog_bytes = fs::read(output_dir.join("catalog.json"))?;
    let catalog: Catalog = serde_json::from_slice(&catalog_bytes)?;
    let version_record = catalog
        .versions
        .iter()
        .find(|record| record.version == version)
        .ok_or_else(|| BuildError::Validation(format!("unknown version: {version}")))?;
    let manifest_path = output_dir.join(relative_to_path(&version_record.manifest_path));
    let manifest: VersionManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    let file = manifest
        .files
        .iter()
        .find(|entry| entry.path.eq_ignore_ascii_case(file_path))
        .ok_or_else(|| BuildError::Validation(format!("unknown target file: {file_path}")))?;
    let chunk = file.chunks.get(chunk_index).ok_or_else(|| {
        BuildError::Validation(format!(
            "chunk index {chunk_index} is out of bounds for {file_path}"
        ))
    })?;
    let pack = catalog
        .packs
        .iter()
        .find(|record| record.id == chunk.pack_id)
        .ok_or_else(|| BuildError::Validation(format!("unknown pack: {}", chunk.pack_id)))?;
    let range_end = chunk
        .pack_offset
        .checked_add(chunk.compressed_size)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| BuildError::Validation("invalid chunk range".to_string()))?;
    let url = format!(
        "{}/{}",
        remote_base.trim_end_matches('/'),
        crate::remote_paths::encode_hf_relative_path(&pack.path)
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| BuildError::ValidationRequest(error.without_url().to_string()))?;
    let mut request = client
        .get(&url)
        .header(RANGE, format!("bytes={}-{}", chunk.pack_offset, range_end));
    if let Ok(token) = std::env::var("HF_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
    }
    let response = request
        .send()
        .map_err(|error| BuildError::ValidationRequest(error.without_url().to_string()))?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(BuildError::Validation(format!(
            "remote server ignored the requested pack range (HTTP {})",
            response.status()
        )));
    }
    let payload = response
        .bytes()
        .map_err(|error| BuildError::ValidationRequest(error.without_url().to_string()))?
        .to_vec();
    if payload.len() as u64 != chunk.compressed_size
        || sha256_bytes(&payload) != chunk.compressed_sha256
    {
        return Err(BuildError::Validation(format!(
            "transport hash mismatch for chunk {}",
            chunk.hash
        )));
    }

    let compressed = if let Some(encryption) = &chunk.encryption {
        let key_material = depot_crypto::resolve_key_material(None);
        if key_id_from_material(&key_material) != encryption.key_id {
            return Err(BuildError::Validation(format!(
                "OXO_DEPOT_KEY does not match chunk key id {}",
                encryption.key_id
            )));
        }
        let plaintext = depot_crypto::decrypt_compressed_chunk(
            &payload,
            &chunk.hash,
            &encryption.plaintext_compressed_sha256,
            &encryption.nonce,
            &key_material,
            &encryption.algorithm,
        )
        .map_err(|error| BuildError::Crypto(error.to_string()))?;
        if plaintext.len() as u64 != encryption.plaintext_compressed_size
            || sha256_bytes(&plaintext) != encryption.plaintext_compressed_sha256
        {
            return Err(BuildError::Validation(format!(
                "decrypted compressed hash mismatch for chunk {}",
                chunk.hash
            )));
        }
        plaintext
    } else {
        payload
    };
    let decoded = match chunk.codec {
        ChunkCodec::Raw => compressed,
        ChunkCodec::Zstd => zstd::bulk::decompress(&compressed, chunk.uncompressed_size as usize)
            .map_err(|error| {
            BuildError::Validation(format!("chunk decompress failed: {error}"))
        })?,
    };
    if decoded.len() as u64 != chunk.uncompressed_size
        || blake3::hash(&decoded).to_hex().as_str() != chunk.hash
    {
        return Err(BuildError::Validation(format!(
            "decoded BLAKE3 mismatch for chunk {}",
            chunk.hash
        )));
    }
    Ok(RemoteChunkVerificationReport {
        version: manifest.version,
        file_path: file.path.clone(),
        chunk_index,
        pack_id: chunk.pack_id.clone(),
        compressed_bytes: chunk.compressed_size,
        uncompressed_bytes: chunk.uncompressed_size,
        encrypted: chunk.encryption.is_some(),
    })
}

fn validate_file_chunk_layout(
    file: &FileEntry,
    packs: Option<&HashMap<String, &PackRecord>>,
) -> Result<(), BuildError> {
    let mut expected_offset = 0_u64;
    for chunk in &file.chunks {
        if chunk.file_offset != expected_offset || chunk.uncompressed_size == 0 {
            return Err(BuildError::Validation(format!(
                "non-contiguous chunk layout for {} at offset {} (expected {})",
                file.path, chunk.file_offset, expected_offset
            )));
        }
        expected_offset = expected_offset.saturating_add(chunk.uncompressed_size);
        if let Some(packs) = packs {
            let pack = packs.get(&chunk.pack_id).ok_or_else(|| {
                BuildError::Validation(format!(
                    "{} references unknown pack {}",
                    file.path, chunk.pack_id
                ))
            })?;
            if chunk
                .pack_offset
                .checked_add(chunk.compressed_size)
                .is_none_or(|end| end > pack.size)
            {
                return Err(BuildError::Validation(format!(
                    "chunk {} exceeds pack {} bounds",
                    chunk.hash, chunk.pack_id
                )));
            }
        }
    }
    if expected_offset != file.size {
        return Err(BuildError::Validation(format!(
            "chunk sizes for {} total {} but target size is {}",
            file.path, expected_offset, file.size
        )));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, BuildError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

fn build_file_entry(
    root: &Path,
    file_path: &Path,
    previous_version_root: Option<&Path>,
    _previous_version_name: Option<&str>,
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
    preserve: bool,
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
                        finalize_pack(
                            pack,
                            output_dir,
                            publish,
                            pack_records,
                            encryption.enabled,
                            upload_incrementally,
                        )?;
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
    let new_file_hash = hex::encode(file_hasher.finalize());
    let mut delta_patches = None;

    // Runtime updates are assembled from target FastCDC chunks. Keep the old
    // encoder source-compatible for now, but do not publish unused oxidelta
    // payloads into new depots.
    if let Some(prev_root) = previous_version_root.filter(|_| BUILD_LEGACY_OXIDELTA) {
        let old_file_path = prev_root.join(crate::scanner::normalize_relative(root, file_path));
        if old_file_path.exists() {
            let mut generate_delta = true;
            let mut old_hash_str = String::new();
            if let Ok(mut old_file) = File::open(&old_file_path) {
                let mut old_hasher = Sha256::new();
                if let Ok(_) = std::io::copy(&mut old_file, &mut old_hasher) {
                    old_hash_str = hex::encode(old_hasher.finalize());
                    if old_hash_str == new_file_hash {
                        generate_delta = false;
                    }
                }
            }

            if generate_delta && !old_hash_str.is_empty() {
                let delta_tmp_path = pack_dir.join(format!("{}.delta.tmp", new_file_hash));

                match oxidelta::io::encode_file(
                    &old_file_path,
                    file_path,
                    &delta_tmp_path,
                    oxidelta::compress::encoder::CompressOptions::default(),
                ) {
                    Ok(_) => {
                        if let Ok(delta_bytes) = fs::read(&delta_tmp_path) {
                            let delta_hash = blake3::hash(&delta_bytes).to_hex().to_string();

                            let (codec, encoded) =
                                encode_chunk_payload(&delta_bytes, format_version)
                                    .unwrap_or((ChunkCodec::Raw, delta_bytes.clone()));

                            let plaintext_compressed_sha256 = sha256_bytes(&encoded);
                            let plaintext_compressed_size = encoded.len() as u64;
                            let uncompressed_size = delta_bytes.len() as u64;

                            let (transport_bytes, encryption_meta) = if encryption.enabled {
                                let key_material = depot_crypto::resolve_key_material(
                                    encryption.key_material.as_deref(),
                                );
                                let key_id = encryption
                                    .key_id
                                    .clone()
                                    .filter(|value| !value.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        depot_crypto::key_id_from_material(&key_material)
                                    });

                                if let Ok((encrypted, nonce)) =
                                    depot_crypto::encrypt_compressed_chunk(
                                        &encoded,
                                        &delta_hash,
                                        &plaintext_compressed_sha256,
                                        &key_material,
                                    )
                                {
                                    (
                                        encrypted,
                                        Some(ChunkEncryption {
                                            algorithm: DEPOT_ENCRYPTION_ALGORITHM.to_string(),
                                            key_id,
                                            nonce,
                                            plaintext_compressed_size,
                                            plaintext_compressed_sha256:
                                                plaintext_compressed_sha256.clone(),
                                        }),
                                    )
                                } else {
                                    (encoded, None)
                                }
                            } else {
                                (encoded, None)
                            };

                            let compressed_sha256 = sha256_bytes(&transport_bytes);

                            if current_pack
                                .as_ref()
                                .map(|pack| {
                                    pack.size + (transport_bytes.len() as u64) > pack_target_size
                                        && pack.size > 0
                                })
                                .unwrap_or(true)
                            {
                                if let Some(pack) = current_pack.take() {
                                    if pack.size > 0 {
                                        finalize_pack(
                                            pack,
                                            output_dir,
                                            publish,
                                            pack_records,
                                            encryption.enabled,
                                            upload_incrementally,
                                        )?;
                                    }
                                }
                                let pack =
                                    PackWriter::create(pack_dir, pack_id_prefix, *next_pack_index)?;
                                *next_pack_index += 1;
                                *current_pack = Some(pack);
                            }

                            let pack = current_pack.as_mut().expect("pack writer must exist");
                            if let Ok(pack_offset) = pack.write_chunk(&transport_bytes) {
                                let patch = DeltaPatch {
                                    from_sha256: old_hash_str,
                                    pack_id: pack.id.clone(),
                                    pack_offset,
                                    uncompressed_size,
                                    compressed_size: transport_bytes.len() as u64,
                                    compressed_sha256,
                                    codec,
                                    encryption: encryption_meta,
                                };
                                delta_patches = Some(vec![patch]);
                            }
                        }
                        let _ = fs::remove_file(&delta_tmp_path);
                    }
                    Err(e) => {
                        eprintln!("[DEPOT] warning: oxidelta failed for {}: {}", path, e);
                    }
                }
            }
        }
    }

    Ok(FileEntry {
        path: path.clone(),
        size: metadata.len(),
        sha256: new_file_hash,
        chunks,
        delta_patches,
        executable: path.to_ascii_lowercase().ends_with(".exe"),
        preserve,
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
    println!("(Uploading... this may take 1-5 minutes per pack depending on your network speed. Please do not close the window...)");
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

/// Build a path→FileEntry map from the LATEST manifest that contains each path,
/// but only for paths matching at least one of `patterns`.
/// Iterates versions in order so later entries overwrite earlier ones, giving
/// callers the most-recent known-good manifest entry for each file.
fn seed_inherited_file_entries(
    output_dir: &Path,
    catalog: &Catalog,
    patterns: &[String],
) -> Result<HashMap<String, FileEntry>, BuildError> {
    let mut map: HashMap<String, FileEntry> = HashMap::new();
    for version in &catalog.versions {
        let manifest_path = output_dir.join(relative_to_path(&version.manifest_path));
        if !manifest_path.exists() {
            continue;
        }
        let bytes = fs::read(&manifest_path)?;
        let manifest: VersionManifest = serde_json::from_slice(&bytes)?;
        for file in manifest.files {
            if patterns.iter().any(|pat| glob_matches(pat, &file.path)) {
                map.insert(file.path.clone(), file);
            }
        }
    }
    eprintln!(
        "[DEPOT] Inherited {} file entries from previous manifests (skip patterns: {})",
        map.len(),
        patterns.join(", ")
    );
    Ok(map)
}

/// Simple glob matcher supporting `*` (any chars within a path segment) and
/// `**` (any chars across multiple segments, i.e. zero or more path components).
/// Matching is case-insensitive on Windows, case-sensitive elsewhere.
/// Path separators are normalised to `/` before comparison.
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pat = pattern.replace('\\', "/");
    let p = path.replace('\\', "/");
    #[cfg(windows)]
    let (pat, p) = (pat.to_lowercase(), p.to_lowercase());
    glob_match_inner(&pat, &p)
}

fn glob_match_inner(pat: &str, path: &str) -> bool {
    let mut pi = pat.chars().peekable();
    let mut si = path.chars().peekable();
    loop {
        match pi.peek() {
            None => return si.peek().is_none(),
            Some(&'*') => {
                pi.next();
                if pi.peek() == Some(&'*') {
                    // `**` — consume all remaining path chars (greedy, try each suffix)
                    pi.next();
                    // skip optional separator after **
                    if pi.peek() == Some(&'/') {
                        pi.next();
                    }
                    let rest_pat: String = pi.collect();
                    // try matching rest_pat against every suffix of the remaining path
                    let remaining: String = si.collect();
                    if rest_pat.is_empty() {
                        return true;
                    }
                    // try at each `/` boundary and at start
                    let mut start = 0usize;
                    loop {
                        if glob_match_inner(&rest_pat, &remaining[start..]) {
                            return true;
                        }
                        match remaining[start..].find('/') {
                            Some(pos) => start += pos + 1,
                            None => return false,
                        }
                    }
                } else {
                    // single `*` — matches anything within current segment (no `/`)
                    let rest_pat: String = pi.collect();
                    let remaining: String = si.collect();
                    // try matching rest_pat against every non-slash suffix
                    let mut start = 0usize;
                    loop {
                        if remaining[start..].contains('/') {
                            // don't let single * cross path separators
                            let seg_end = remaining[start..].find('/').unwrap() + start;
                            for end in start..=seg_end {
                                if glob_match_inner(&rest_pat, &remaining[end..]) {
                                    return true;
                                }
                            }
                            return false;
                        } else {
                            if glob_match_inner(&rest_pat, &remaining[start..]) {
                                return true;
                            }
                            if start >= remaining.len() {
                                return false;
                            }
                            start += remaining[start..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(1);
                        }
                    }
                }
            }
            Some(&'?') => {
                pi.next();
                // `?` matches any single non-separator char
                match si.next() {
                    Some('/') | None => return false,
                    Some(_) => {}
                }
            }
            Some(&pc) => match si.next() {
                Some(sc) if sc == pc => {
                    pi.next();
                }
                _ => return false,
            },
        }
    }
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

    fn test_chunk(hash: &str, file_offset: u64, size: u64, pack_offset: u64) -> ChunkRef {
        ChunkRef {
            hash: hash.to_string(),
            file_offset,
            uncompressed_size: size,
            pack_id: "pack-00000".to_string(),
            pack_offset,
            compressed_size: 2,
            compressed_sha256: format!("compressed-{hash}"),
            codec: ChunkCodec::Raw,
            encryption: None,
        }
    }

    fn test_manifest(version: &str, chunks: Vec<ChunkRef>, size: u64) -> VersionManifest {
        VersionManifest {
            format_version: FORMAT_VERSION,
            game_id: "fixture-game".to_string(),
            version: version.to_string(),
            created_at: "2026-08-06T00:00:00Z".to_string(),
            root_label: version.to_string(),
            launch_executable: None,
            launch_options: Vec::new(),
            dependencies: None,
            total_size: size,
            files: vec![FileEntry {
                path: "Content/game.pak".to_string(),
                size,
                sha256: "file-sha".to_string(),
                chunks,
                delta_patches: None,
                executable: false,
                preserve: false,
            }],
            signature: None,
        }
    }

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

    #[test]
    fn validator_reports_reused_and_download_bytes_between_versions() {
        let root = std::env::temp_dir().join(format!(
            "0xolemon-builder-validator-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let versions = root.join("versions");
        let v1_dir = versions.join("v1");
        let v2_dir = versions.join("v2");
        fs::create_dir_all(&v1_dir).unwrap();
        fs::create_dir_all(&v2_dir).unwrap();
        let base = test_manifest("v1", vec![test_chunk("shared", 0, 4, 0)], 4);
        let target = test_manifest(
            "v2",
            vec![test_chunk("shared", 0, 4, 0), test_chunk("new", 4, 3, 2)],
            7,
        );
        write_json_pretty(&v1_dir.join("manifest.json"), &base).unwrap();
        write_json_pretty(&v2_dir.join("manifest.json"), &target).unwrap();
        let catalog = Catalog {
            format_version: FORMAT_VERSION,
            game_id: "fixture-game".to_string(),
            latest_version: Some("v2".to_string()),
            versions: vec![
                CatalogVersion {
                    version: "v1".to_string(),
                    manifest_path: "versions/v1/manifest.json".to_string(),
                    total_size: 4,
                    file_count: 1,
                    chunk_count: 1,
                    created_at: "2026-08-06T00:00:00Z".to_string(),
                },
                CatalogVersion {
                    version: "v2".to_string(),
                    manifest_path: "versions/v2/manifest.json".to_string(),
                    total_size: 7,
                    file_count: 1,
                    chunk_count: 2,
                    created_at: "2026-08-06T00:00:00Z".to_string(),
                },
            ],
            packs: vec![PackRecord {
                id: "pack-00000".to_string(),
                path: "packs/pack-00000.bin".to_string(),
                size: 4,
                sha256: "pack-sha".to_string(),
            }],
            signature: None,
        };
        let reports = validate_catalog_manifests(&root, &catalog).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].reused_bytes, 4);
        assert_eq!(reports[0].download_bytes, 2);
        assert_eq!(reports[0].target_bytes, 7);

        fs::remove_file(v1_dir.join("manifest.json")).unwrap();
        fs::remove_file(v2_dir.join("manifest.json")).unwrap();
        fs::remove_dir(v1_dir).unwrap();
        fs::remove_dir(v2_dir).unwrap();
        fs::remove_dir(versions).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn validator_rejects_non_contiguous_target_chunks() {
        let file = test_manifest("v2", vec![test_chunk("bad", 2, 4, 0)], 4)
            .files
            .remove(0);
        let error = validate_file_chunk_layout(&file, None).unwrap_err();
        assert!(error.to_string().contains("non-contiguous"));
    }
}
