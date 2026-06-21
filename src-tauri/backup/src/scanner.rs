use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

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
    // Kept for frontend/API compatibility. Version resolution is intentionally
    // handled by job metadata (state.0xo/manifest.0xo), never by game signatures.
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

/// Generic filesystem inventory only. This function must remain title-agnostic:
/// it does not know executable names, file sizes, versions, or game IDs.
pub fn scan_install(root: &Path) -> Result<ScanReport, ScanError> {
    if !root.exists() {
        return Err(ScanError::MissingRoot(root.display().to_string()));
    }

    let mut file_count = 0usize;
    let mut total_size = 0u64;
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        file_count += 1;
        total_size = total_size.saturating_add(metadata.len());
    }

    Ok(ScanReport {
        root: root.display().to_string(),
        file_count,
        total_size,
        detected_version: None,
        important_files: Vec::new(),
        warnings: Vec::new(),
    })
}

pub fn normalize_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("\\")
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
        let joined = safe_join(Path::new("E:\\Game"), "Data\\chunk0.bin")
            .expect("valid manifest path should join");
        assert!(joined.ends_with("Data\\chunk0.bin"));
    }
}
