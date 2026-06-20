use serde::Serialize;
use std::{fs, path::Path};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearCacheReport {
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub cache_path: String,
}

pub fn clear_chunk_cache(cache_path: &Path) -> Result<ClearCacheReport, String> {
    validate_cache_path(cache_path)?;
    if !cache_path.exists() {
        return Ok(ClearCacheReport {
            removed_files: 0,
            removed_bytes: 0,
            cache_path: cache_path.display().to_string(),
        });
    }

    let canonical_root = cache_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    validate_cache_path(&canonical_root)?;
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for entry in WalkDir::new(&canonical_root).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().to_path_buf();
        if !path.starts_with(&canonical_root) {
            return Err("cache cleanup escaped the validated chunk directory".to_string());
        }
        if entry.file_type().is_symlink() {
            return Err(format!(
                "cache cleanup refused a symbolic link or reparse point: {}",
                path.display()
            ));
        }
        if entry.file_type().is_file() {
            files.push(path);
        } else if entry.file_type().is_dir() && path != canonical_root {
            directories.push(path);
        }
    }

    let mut removed_files = 0u64;
    let mut removed_bytes = 0u64;
    for file in files {
        let metadata = fs::metadata(&file).map_err(|error| error.to_string())?;
        fs::remove_file(&file).map_err(|error| error.to_string())?;
        removed_files = removed_files.saturating_add(1);
        removed_bytes = removed_bytes.saturating_add(metadata.len());
    }

    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        if directory.starts_with(&canonical_root) {
            fs::remove_dir(&directory).map_err(|error| error.to_string())?;
        }
    }

    Ok(ClearCacheReport {
        removed_files,
        removed_bytes,
        cache_path: canonical_root.display().to_string(),
    })
}

fn validate_cache_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("cache path is empty".to_string());
    }
    let final_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !final_name.eq_ignore_ascii_case("chunks") {
        return Err("cache cleanup only accepts a directory named chunks".to_string());
    }
    let below_downloading = path.ancestors().skip(1).any(|ancestor| {
        ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("downloading"))
    });
    if !below_downloading {
        return Err("cache cleanup only accepts chunks stored below downloading".to_string());
    }
    if path.parent().is_none() {
        return Err("cache cleanup refused a filesystem root".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_cache_path;
    use std::path::Path;

    #[test]
    fn cache_path_requires_chunks_below_downloading() {
        assert!(validate_cache_path(Path::new(
            r"E:\0xoLemon store\downloading\007 First Light\chunks"
        ))
        .is_ok());
        assert!(validate_cache_path(Path::new(r"E:\chunks")).is_err());
        assert!(validate_cache_path(Path::new(
            r"E:\0xoLemon store\downloading\007 First Light\staging"
        ))
        .is_err());
    }
}
