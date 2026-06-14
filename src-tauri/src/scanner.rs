use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

const V1_EXE_SIZE: u64 = 342_511_496;
const V11_EXE_SIZE: u64 = 334_589_320;
const V1_CHUNK0_SIZE: u64 = 20_668_252_289;
const V11_CHUNK0_SIZE: u64 = 20_668_299_755;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("install path does not exist: {0}")]
    MissingRoot(String),
    #[error("walk error: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub root: String,
    pub file_count: usize,
    pub total_size: u64,
    pub detected_version: Option<String>,
    pub important_files: Vec<ImportantFile>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportantFile {
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub status: String,
}

pub fn scan_install(root: &Path) -> Result<ScanReport, ScanError> {
    if !root.exists() {
        return Err(ScanError::MissingRoot(root.display().to_string()));
    }

    let mut file_count = 0usize;
    let mut total_size = 0u64;
    let mut important_files = Vec::new();
    let mut warnings = Vec::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        file_count += 1;
        total_size += metadata.len();
        let rel = normalize_relative(root, entry.path());
        if is_important(&rel) {
            let sha256 = if metadata.len() <= 512 * 1024 * 1024 {
                Some(sha256_file(entry.path())?)
            } else {
                None
            };
            important_files.push(ImportantFile {
                path: rel,
                size: metadata.len(),
                sha256,
                status: "present".to_string(),
            });
        }
    }

    for expected in [
        "Retail\\007FirstLight.exe",
        "Retail\\PipelineCache.bin",
        "Runtime\\chunk0.rpkg",
        "Runtime\\chunk1.rpkg",
    ] {
        if !important_files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(expected))
        {
            warnings.push(format!("Missing expected game file: {expected}"));
        }
    }

    let exe_size = important_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case("Retail\\007FirstLight.exe"))
        .map(|file| file.size);
    let chunk0_size = important_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case("Runtime\\chunk0.rpkg"))
        .map(|file| file.size);
    let detected_version = match (exe_size, chunk0_size) {
        (Some(V1_EXE_SIZE), Some(V1_CHUNK0_SIZE)) => Some("v1.0".to_string()),
        (Some(V11_EXE_SIZE), Some(V11_CHUNK0_SIZE)) => Some("v1.1".to_string()),
        _ => None,
    };

    Ok(ScanReport {
        root: root.display().to_string(),
        file_count,
        total_size,
        detected_version,
        important_files,
        warnings,
    })
}

fn is_important(path: &str) -> bool {
    matches!(
        path,
        "Retail\\007FirstLight.exe"
            | "Retail\\PipelineCache.bin"
            | "Runtime\\chunk0.rpkg"
            | "Runtime\\chunk1.rpkg"
    )
}

pub fn normalize_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("\\")
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for component in rel.replace('/', "\\").split('\\') {
        if component.is_empty() || component == "." || component == ".." {
            return None;
        }
        out.push(component);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::safe_join;
    use std::path::Path;

    #[test]
    fn safe_join_rejects_parent_traversal() {
        assert!(safe_join(Path::new("E:\\Game"), "..\\secret.bin").is_none());
        assert!(safe_join(Path::new("E:\\Game"), "Runtime\\..\\secret.bin").is_none());
    }

    #[test]
    fn safe_join_accepts_manifest_owned_relative_path() {
        let joined = safe_join(Path::new("E:\\Game"), "Runtime\\chunk0.rpkg")
            .expect("valid manifest path should join");
        assert!(joined.ends_with("Runtime\\chunk0.rpkg"));
    }
}
