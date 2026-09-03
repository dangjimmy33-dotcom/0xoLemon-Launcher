use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::StreamExt;
use hf_hub::repository::download::HFByteStream;
use hf_hub::repository::files::FileMetadataInfo;
use hf_hub::HFClient;
use url::Url;

const MIN_RANGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RANGE_BYTES: u64 = 64 * 1024 * 1024;
const XET_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;
const XET_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(2);
const XET_STREAM_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HfRepoKind {
    Model,
    Dataset,
    Space,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HfRepoLocation {
    pub endpoint: String,
    pub owner: String,
    pub name: String,
    pub revision: String,
    pub prefix: String,
    pub kind: HfRepoKind,
}

impl HfRepoLocation {
    pub(crate) fn parse_resolve_base(value: &str) -> Option<Self> {
        let parsed = Url::parse(value).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }

        let encoded_segments = parsed.path_segments()?.collect::<Vec<_>>();
        let (kind, repo_offset) = match encoded_segments.first().copied() {
            Some("datasets") => (HfRepoKind::Dataset, 1),
            Some("spaces") => (HfRepoKind::Space, 1),
            _ => (HfRepoKind::Model, 0),
        };
        if encoded_segments.len() < repo_offset + 5
            || encoded_segments.get(repo_offset + 2).copied() != Some("resolve")
        {
            return None;
        }

        let decode = |segment: &str| {
            urlencoding::decode(segment)
                .ok()
                .map(|value| value.into_owned())
        };
        let owner = decode(encoded_segments[repo_offset])?;
        let name = decode(encoded_segments[repo_offset + 1])?;
        let revision = decode(encoded_segments[repo_offset + 3])?;
        if owner.is_empty() || name.is_empty() || revision.is_empty() {
            return None;
        }
        let prefix = encoded_segments[(repo_offset + 4)..]
            .iter()
            .map(|segment| decode(segment))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/");

        let mut endpoint = parsed;
        endpoint.set_path("");
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        Some(Self {
            endpoint: endpoint.as_str().trim_end_matches('/').to_string(),
            owner,
            name,
            revision,
            prefix,
            kind,
        })
    }

    pub(crate) fn file_path(&self, relative_path: &str) -> String {
        let relative_path = relative_path.trim_matches('/');
        match (self.prefix.is_empty(), relative_path.is_empty()) {
            (true, _) => relative_path.to_string(),
            (_, true) => self.prefix.clone(),
            (false, false) => format!("{}/{}", self.prefix.trim_end_matches('/'), relative_path),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HfPackMetadata {
    pub source_identity: String,
    pub resolved_revision: String,
    pub file_size: u64,
    pub xet_hash: Option<String>,
}

fn source_identity_value(identity: &str) -> &str {
    let value = ["x-xet-hash:", "x-linked-etag:", "etag:"]
        .into_iter()
        .find_map(|prefix| identity.strip_prefix(prefix))
        .unwrap_or(identity)
        .trim();
    value.strip_prefix("W/").unwrap_or(value).trim_matches('"')
}

/// Hugging Face exposes the same immutable Xet object under different header
/// names before and after its redirect. The resolve response uses X-Xet-Hash,
/// while the signed CDN response uses ETag. Compare their opaque values so a
/// safe Xet-to-range fallback does not report a false source revision change.
pub(crate) fn source_identities_match(expected: &str, observed: &str) -> bool {
    !expected.trim().is_empty()
        && !observed.trim().is_empty()
        && source_identity_value(expected) == source_identity_value(observed)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct XetStreamStats {
    pub wire_bytes: u64,
    pub elapsed: Duration,
}

fn build_hf_client(
    location: &HfRepoLocation,
    token: Option<&str>,
    cache_dir: &Path,
) -> Result<HFClient, String> {
    let mut builder = HFClient::builder()
        .endpoint(location.endpoint.clone())
        .cache_dir(cache_dir)
        .cache_enabled(true)
        .retry_max_attempts(2)
        .retry_base_delay(Duration::from_millis(200));
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        builder = builder.token(token.trim().to_string());
    }
    builder
        .build()
        .map_err(|error| format!("Hugging Face client initialization failed: {error}"))
}

async fn get_file_metadata_async(
    client: &HFClient,
    location: &HfRepoLocation,
    file_path: &str,
) -> Result<FileMetadataInfo, String> {
    let result = match location.kind {
        HfRepoKind::Model => {
            client
                .model(location.owner.clone(), location.name.clone())
                .get_file_metadata()
                .filepath(file_path.to_string())
                .revision(location.revision.as_str())
                .send()
                .await
        }
        HfRepoKind::Dataset => {
            client
                .dataset(location.owner.clone(), location.name.clone())
                .get_file_metadata()
                .filepath(file_path.to_string())
                .revision(location.revision.as_str())
                .send()
                .await
        }
        HfRepoKind::Space => {
            client
                .space(location.owner.clone(), location.name.clone())
                .get_file_metadata()
                .filepath(file_path.to_string())
                .revision(location.revision.as_str())
                .send()
                .await
        }
    };
    result.map_err(|error| format!("Hugging Face metadata request failed: {error}"))
}

async fn download_file_stream_async(
    client: &HFClient,
    location: &HfRepoLocation,
    file_path: &str,
) -> Result<(Option<u64>, HFByteStream), String> {
    let result = match location.kind {
        HfRepoKind::Model => {
            client
                .model(location.owner.clone(), location.name.clone())
                .download_file_stream()
                .filename(file_path.to_string())
                .revision(location.revision.clone())
                .send()
                .await
        }
        HfRepoKind::Dataset => {
            client
                .dataset(location.owner.clone(), location.name.clone())
                .download_file_stream()
                .filename(file_path.to_string())
                .revision(location.revision.clone())
                .send()
                .await
        }
        HfRepoKind::Space => {
            client
                .space(location.owner.clone(), location.name.clone())
                .download_file_stream()
                .filename(file_path.to_string())
                .revision(location.revision.clone())
                .send()
                .await
        }
    };
    result.map_err(|error| format!("Hugging Face Xet stream failed: {error}"))
}

fn current_thread_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        // hf-hub uses reqwest for metadata and Xet reconstruction. A time-only
        // runtime compiles but fails as soon as reqwest needs Tokio's I/O
        // reactor, so the transport runtime must enable both I/O and timers.
        .enable_all()
        .build()
        .map_err(|error| format!("Hugging Face runtime initialization failed: {error}"))
}

pub(crate) fn probe_hf_pack(
    location: &HfRepoLocation,
    token: Option<&str>,
    cache_dir: &Path,
    relative_path: &str,
) -> Result<HfPackMetadata, String> {
    fs::create_dir_all(cache_dir)
        .map_err(|error| format!("failed to create Xet cache directory: {error}"))?;
    let client = build_hf_client(location, token, cache_dir)?;
    let file_path = location.file_path(relative_path);
    let metadata = current_thread_runtime()?
        .block_on(get_file_metadata_async(&client, location, &file_path))?;
    let source_identity = metadata
        .xet_hash
        .as_ref()
        .map(|hash| format!("x-xet-hash:{hash}"))
        .unwrap_or_else(|| format!("etag:{}", metadata.etag));
    Ok(HfPackMetadata {
        source_identity,
        resolved_revision: metadata.commit_hash,
        file_size: metadata.file_size,
        xet_hash: metadata.xet_hash,
    })
}

fn checkpoint_path(spool_path: &Path) -> PathBuf {
    let name = spool_path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    spool_path.with_file_name(format!("{name}.checkpoint"))
}

fn read_durable_counter(path: &Path, maximum: u64) -> u64 {
    let mut value = String::new();
    File::open(super::long_path(path))
        .and_then(|mut file| file.read_to_string(&mut value))
        .ok()
        .and_then(|_| value.trim().parse::<u64>().ok())
        .filter(|value| *value <= maximum)
        .unwrap_or(0)
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let source = super::long_path(source);
    let destination = super::long_path(destination);
    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(format!(
            "failed to atomically replace checkpoint {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "failed to atomically replace checkpoint {}: {error}",
            destination.display()
        )
    })
}

pub(crate) fn persist_durable_counter(path: &Path, bytes: u64) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("checkpoint path has no parent: {}", path.display()))?;
    fs::create_dir_all(super::long_path(parent))
        .map_err(|error| format!("failed to create checkpoint directory: {error}"))?;
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!("{file_name}.next"));
    {
        let mut output = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(super::long_path(&temporary))
            .map_err(|error| format!("failed to create checkpoint: {error}"))?;
        write!(output, "{bytes}")
            .map_err(|error| format!("failed to write checkpoint: {error}"))?;
        output
            .sync_all()
            .map_err(|error| format!("failed to flush checkpoint: {error}"))?;
    }
    if let Err(error) = atomic_replace(&temporary, path) {
        let _ = fs::remove_file(super::long_path(&temporary));
        return Err(error);
    }
    Ok(())
}

pub(crate) fn cleanup_xet_spool(spool_path: &Path) {
    let _ = fs::remove_file(spool_path);
    let _ = fs::remove_file(checkpoint_path(spool_path));
}

fn persist_checkpoint(spool_path: &Path, bytes: u64) -> Result<(), String> {
    let checkpoint = checkpoint_path(spool_path);
    persist_durable_counter(&checkpoint, bytes)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stream_hf_xet_pack_to_spool<Cancel, Paused, Progress>(
    location: &HfRepoLocation,
    token: Option<&str>,
    cache_dir: &Path,
    relative_path: &str,
    spool_path: &Path,
    expected_size: u64,
    mut is_canceled: Cancel,
    mut is_paused: Paused,
    mut on_progress: Progress,
) -> Result<XetStreamStats, String>
where
    Cancel: FnMut() -> bool,
    Paused: FnMut() -> bool,
    Progress: FnMut(u64, u64, u64) -> Result<(), String>,
{
    let parent = spool_path
        .parent()
        .ok_or_else(|| "Xet spool path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create Xet spool directory: {error}"))?;
    fs::create_dir_all(cache_dir)
        .map_err(|error| format!("failed to create Xet cache directory: {error}"))?;

    let existing_size = fs::metadata(super::long_path(spool_path))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let durable = read_durable_counter(&checkpoint_path(spool_path), existing_size);
    if expected_size > 0 && existing_size == expected_size && durable == expected_size {
        on_progress(0, expected_size, expected_size)?;
        return Ok(XetStreamStats {
            wire_bytes: 0,
            elapsed: Duration::ZERO,
        });
    }
    if existing_size > 0 || checkpoint_path(spool_path).exists() {
        // hf-xet keeps its verified CAS blocks in cache_dir. An incomplete
        // sequential spool cannot be appended safely, so rebuild only that
        // spool while allowing hf-xet to reuse its durable cache.
        cleanup_xet_spool(spool_path);
    }

    let client = build_hf_client(location, token, cache_dir)?;
    let file_path = location.file_path(relative_path);
    let started = Instant::now();
    let runtime = current_thread_runtime()?;
    let wire_bytes = runtime.block_on(async {
        let (content_length, mut stream) =
            download_file_stream_async(&client, location, &file_path).await?;
        if let Some(content_length) = content_length {
            if expected_size > 0 && content_length != expected_size {
                return Err(format!(
                    "Xet source size changed; expected {expected_size}, got {content_length}"
                ));
            }
        }

        let output = File::create(spool_path)
            .map_err(|error| format!("failed to create Xet spool: {error}"))?;
        let mut output = BufWriter::with_capacity(1024 * 1024, output);
        let mut written = 0_u64;
        let mut durable = 0_u64;
        let mut uncheckpointed = 0_u64;
        let mut last_checkpoint = Instant::now();

        loop {
            if is_canceled() {
                return Err("download canceled".to_string());
            }
            while is_paused() {
                if is_canceled() {
                    return Err("download canceled".to_string());
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }

            let next = tokio::time::timeout(XET_STREAM_INACTIVITY_TIMEOUT, stream.next())
                .await
                .map_err(|_| "Hugging Face Xet stream timed out".to_string())?;
            let Some(chunk) = next else {
                break;
            };
            let chunk =
                chunk.map_err(|error| format!("Hugging Face Xet stream failed: {error}"))?;
            output
                .write_all(&chunk)
                .map_err(|error| format!("failed to write Xet spool: {error}"))?;
            let bytes = chunk.len() as u64;
            written = written.saturating_add(bytes);
            uncheckpointed = uncheckpointed.saturating_add(bytes);
            if uncheckpointed >= XET_CHECKPOINT_BYTES
                || last_checkpoint.elapsed() >= XET_CHECKPOINT_INTERVAL
            {
                output
                    .flush()
                    .map_err(|error| format!("failed to flush Xet spool: {error}"))?;
                output
                    .get_ref()
                    .sync_data()
                    .map_err(|error| format!("failed to checkpoint Xet spool: {error}"))?;
                persist_checkpoint(spool_path, written)?;
                durable = written;
                uncheckpointed = 0;
                last_checkpoint = Instant::now();
            }
            on_progress(bytes, written, durable)?;
        }

        output
            .flush()
            .map_err(|error| format!("failed to flush Xet spool: {error}"))?;
        output
            .get_ref()
            .sync_data()
            .map_err(|error| format!("failed to commit Xet spool: {error}"))?;
        persist_checkpoint(spool_path, written)?;
        durable = written;
        on_progress(0, written, durable)?;
        if expected_size > 0 && written != expected_size {
            return Err(format!(
                "Xet source ended early; expected {expected_size}, got {written}"
            ));
        }
        Ok(written)
    })?;

    Ok(XetStreamStats {
        wire_bytes,
        elapsed: started.elapsed(),
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DownloadTransportKind {
    XetPack,
    HttpRange,
}

impl Default for DownloadTransportKind {
    fn default() -> Self {
        Self::HttpRange
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PackTransportPlan {
    pub pack_id: String,
    pub source_identity: String,
    pub required_bytes: u64,
    pub total_pack_bytes: u64,
    pub selected_transport: DownloadTransportKind,
    pub estimated_overfetch: u64,
}

impl Default for PackTransportPlan {
    fn default() -> Self {
        Self {
            pack_id: String::new(),
            source_identity: String::new(),
            required_bytes: 0,
            total_pack_bytes: 0,
            selected_transport: DownloadTransportKind::HttpRange,
            estimated_overfetch: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadTelemetry {
    pub job_id: String,
    pub wire_bytes_done: u64,
    pub apply_bytes_done: u64,
    pub durable_bytes_done: u64,
    pub wire_bytes_per_second: u64,
    pub apply_bytes_per_second: u64,
    pub active_connections: usize,
    pub queue_bytes: u64,
    pub ttfb_ms: u64,
    pub retry_wait_ms: u64,
    pub rate_limit_wait_ms: u64,
    pub current_transport: DownloadTransportKind,
    pub stall_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportOperation {
    FreshInstall,
    Update,
    Downgrade,
    Repair,
    Patch,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TransportCandidate {
    pub required_bytes: u64,
    pub total_pack_bytes: u64,
    pub xet_metadata_available: bool,
    pub public_read_available: bool,
    pub xet_estimated_bytes_per_second: Option<u64>,
    pub raw_estimated_bytes_per_second: Option<u64>,
}

pub(crate) fn select_transport(
    operation: TransportOperation,
    candidate: TransportCandidate,
) -> DownloadTransportKind {
    if !candidate.xet_metadata_available
        || !candidate.public_read_available
        || candidate.required_bytes == 0
        || candidate.total_pack_bytes == 0
    {
        return DownloadTransportKind::HttpRange;
    }

    // Compare as u128 so a malformed near-u64::MAX pack cannot overflow the
    // 20% overfetch guard.
    let within_overfetch_limit =
        u128::from(candidate.total_pack_bytes) * 100 <= u128::from(candidate.required_bytes) * 120;
    if !within_overfetch_limit {
        return DownloadTransportKind::HttpRange;
    }

    let coverage_percent =
        u128::from(candidate.required_bytes) * 100 / u128::from(candidate.total_pack_bytes);
    // A full-pack transport is only worthwhile when the target manifest needs
    // nearly the entire pack. The 20% overfetch guard alone would accept
    // 83.34% coverage, which is too aggressive for a fresh install too.
    if coverage_percent < 90 {
        return DownloadTransportKind::HttpRange;
    }

    match (
        candidate.xet_estimated_bytes_per_second,
        candidate.raw_estimated_bytes_per_second,
    ) {
        (Some(xet), Some(raw)) if u128::from(xet) * 100 >= u128::from(raw) * 115 => {
            DownloadTransportKind::XetPack
        }
        // With no history only a fresh install may optimistically use Xet.
        (None, None) if operation == TransportOperation::FreshInstall => {
            DownloadTransportKind::XetPack
        }
        _ => DownloadTransportKind::HttpRange,
    }
}

#[derive(Debug)]
pub(crate) struct AdaptiveGovernor {
    active_connections: usize,
    max_connections: usize,
    range_bytes: u64,
    previous_throughput: u64,
}

impl AdaptiveGovernor {
    pub(crate) fn new(
        max_connections: usize,
        initial_connections: usize,
        initial_range_bytes: u64,
    ) -> Self {
        let max_connections = max_connections.max(1);
        Self {
            active_connections: initial_connections.clamp(1, max_connections),
            max_connections,
            range_bytes: initial_range_bytes.clamp(MIN_RANGE_BYTES, MAX_RANGE_BYTES),
            previous_throughput: 0,
        }
    }

    pub(crate) fn active_connections(&self) -> usize {
        self.active_connections
    }

    pub(crate) fn range_bytes(&self) -> u64 {
        self.range_bytes
    }

    pub(crate) fn observe_window(
        &mut self,
        elapsed: Duration,
        throughput: u64,
        error_rate: f32,
        rate_limited: bool,
        ttfb_ms: u64,
        backpressured: bool,
    ) {
        if rate_limited || error_rate >= 0.02 || backpressured {
            self.active_connections = self.active_connections.saturating_sub(2).max(1);
            self.range_bytes = (self.range_bytes / 2).max(MIN_RANGE_BYTES);
            return;
        }
        if elapsed < Duration::from_secs(5) || throughput == 0 {
            return;
        }

        let improving = self.previous_throughput == 0
            || u128::from(throughput) * 100 >= u128::from(self.previous_throughput) * 105;
        if improving {
            self.active_connections = self
                .active_connections
                .saturating_add(2)
                .min(self.max_connections);
        }
        if ttfb_ms >= 1_500 {
            self.range_bytes = (self.range_bytes.saturating_mul(2)).min(MAX_RANGE_BYTES);
        } else if ttfb_ms <= 250 && throughput >= 32 * 1024 * 1024 {
            self.range_bytes = (self.range_bytes / 2).max(MIN_RANGE_BYTES);
        }
        self.previous_throughput = throughput;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "0xolemon-transport-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn candidate(coverage_percent: u64) -> TransportCandidate {
        TransportCandidate {
            required_bytes: coverage_percent,
            total_pack_bytes: 100,
            xet_metadata_available: true,
            public_read_available: true,
            xet_estimated_bytes_per_second: Some(230),
            raw_estimated_bytes_per_second: Some(200),
        }
    }

    #[test]
    fn parses_dataset_resolve_base_and_joins_game_prefix() {
        let location = HfRepoLocation::parse_resolve_base(
            "https://huggingface.co/datasets/Akatsuki-tusu/naruto/resolve/main/007-first-light",
        )
        .expect("dataset resolve URL should parse");
        assert_eq!(location.kind, HfRepoKind::Dataset);
        assert_eq!(location.owner, "Akatsuki-tusu");
        assert_eq!(location.name, "naruto");
        assert_eq!(location.revision, "main");
        assert_eq!(location.prefix, "007-first-light");
        assert_eq!(
            location.file_path("packs/pack-1.bin"),
            "007-first-light/packs/pack-1.bin"
        );
    }

    #[test]
    fn parses_encoded_revision_and_prefix_without_losing_spaces() {
        let location = HfRepoLocation::parse_resolve_base(
            "https://huggingface.co/datasets/owner/repo/resolve/release%2Fv2/Game%20Folder",
        )
        .expect("encoded resolve URL should parse");
        assert_eq!(location.revision, "release/v2");
        assert_eq!(location.prefix, "Game Folder");
        assert_eq!(location.file_path("packs/p.bin"), "Game Folder/packs/p.bin");
    }

    #[test]
    fn sparse_pack_coverages_stay_on_exact_ranges() {
        for coverage in [10, 50, 84] {
            assert_eq!(
                select_transport(TransportOperation::FreshInstall, candidate(coverage)),
                DownloadTransportKind::HttpRange
            );
        }
    }

    #[test]
    fn dense_install_and_delta_coverages_use_xet_when_it_is_faster() {
        assert_eq!(
            select_transport(TransportOperation::FreshInstall, candidate(90)),
            DownloadTransportKind::XetPack
        );
        assert_eq!(
            select_transport(TransportOperation::FreshInstall, candidate(100)),
            DownloadTransportKind::XetPack
        );
        assert_eq!(
            select_transport(TransportOperation::Update, candidate(90)),
            DownloadTransportKind::XetPack
        );
    }

    #[test]
    fn delta_requires_ninety_percent_even_when_overfetch_guard_would_allow_it() {
        assert_eq!(
            select_transport(TransportOperation::Repair, candidate(84)),
            DownloadTransportKind::HttpRange
        );
    }

    #[test]
    fn only_fresh_install_uses_xet_without_history() {
        let mut no_history = candidate(100);
        no_history.xet_estimated_bytes_per_second = None;
        no_history.raw_estimated_bytes_per_second = None;
        assert_eq!(
            select_transport(TransportOperation::FreshInstall, no_history),
            DownloadTransportKind::XetPack
        );
        assert_eq!(
            select_transport(TransportOperation::Patch, no_history),
            DownloadTransportKind::HttpRange
        );
    }

    #[test]
    fn xet_hash_and_signed_cdn_etag_identify_the_same_source() {
        assert!(source_identities_match(
            "x-xet-hash:6679dad4d852733a",
            "etag:\"6679dad4d852733a\""
        ));
        assert!(source_identities_match(
            "etag:W/\"6679dad4d852733a\"",
            "x-xet-hash:6679dad4d852733a"
        ));
        assert!(!source_identities_match(
            "x-xet-hash:6679dad4d852733a",
            "etag:\"different-object\""
        ));
    }

    #[test]
    fn governor_scales_up_and_reacts_immediately_to_backpressure() {
        let mut governor = AdaptiveGovernor::new(16, 4, 16 * 1024 * 1024);
        assert_eq!(governor.active_connections(), 4);
        governor.observe_window(Duration::from_secs(5), 10_000, 0.0, false, 500, false);
        assert_eq!(governor.active_connections(), 6);
        governor.observe_window(Duration::from_secs(5), 12_000, 0.0, false, 500, false);
        assert_eq!(governor.active_connections(), 8);
        governor.observe_window(Duration::from_secs(1), 0, 0.0, true, 0, false);
        assert_eq!(governor.active_connections(), 6);
        assert_eq!(governor.range_bytes(), 8 * 1024 * 1024);
    }

    #[test]
    fn durable_counter_replaces_existing_value_without_a_missing_checkpoint_window() {
        let root = test_root("checkpoint");
        fs::create_dir_all(&root).expect("test directory should be created");
        let checkpoint = root.join("range.part.checkpoint");

        persist_durable_counter(&checkpoint, 11).expect("first checkpoint should persist");
        persist_durable_counter(&checkpoint, 27).expect("replacement checkpoint should persist");

        assert_eq!(
            fs::read_to_string(&checkpoint).expect("checkpoint should be readable"),
            "27"
        );
        assert!(!checkpoint
            .with_file_name("range.part.checkpoint.next")
            .exists());

        fs::remove_file(&checkpoint).expect("checkpoint cleanup should succeed");
        fs::remove_dir(&root).expect("test directory cleanup should succeed");
    }

    #[test]
    fn complete_xet_spool_resumes_without_opening_the_network() {
        let root = test_root("xet-resume");
        let spool_dir = root.join("spool");
        let cache_dir = root.join("cache");
        fs::create_dir_all(&spool_dir).expect("spool directory should be created");
        let spool = spool_dir.join("pack.part");
        {
            let mut output = File::create(&spool).expect("spool should be created");
            output
                .write_all(b"pack")
                .expect("spool bytes should be written");
            output.sync_all().expect("spool should be durable");
        }
        persist_checkpoint(&spool, 4).expect("spool checkpoint should persist");
        let location = HfRepoLocation {
            endpoint: "not-a-valid-endpoint".to_string(),
            owner: "owner".to_string(),
            name: "repo".to_string(),
            revision: "main".to_string(),
            prefix: String::new(),
            kind: HfRepoKind::Dataset,
        };
        let mut progress = Vec::new();

        let stats = stream_hf_xet_pack_to_spool(
            &location,
            None,
            &cache_dir,
            "packs/pack.bin",
            &spool,
            4,
            || false,
            || false,
            |wire, written, durable| {
                progress.push((wire, written, durable));
                Ok(())
            },
        )
        .expect("a complete durable spool should not need a client");

        assert_eq!(stats.wire_bytes, 0);
        assert_eq!(progress, vec![(0, 4, 4)]);

        cleanup_xet_spool(&spool);
        fs::remove_dir(&cache_dir).expect("cache directory cleanup should succeed");
        fs::remove_dir(&spool_dir).expect("spool directory cleanup should succeed");
        fs::remove_dir(&root).expect("test directory cleanup should succeed");
    }
}
