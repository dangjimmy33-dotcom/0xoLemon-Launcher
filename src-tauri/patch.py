import re

with open('src/job.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Update plan_missing_chunks calls
content = content.replace('plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &manifest.files)?', 'plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &manifest.files, None)?')
content = content.replace('plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &repair_files)?', 'plan_missing_chunks(&HashMap::new(), &staged_chunks_root, &repair_files, Some(&install_root))?')
content = content.replace('plan_missing_chunks(&local_sources, &staged_chunks_root, &changed)?', 'plan_missing_chunks(&local_sources, &staged_chunks_root, &changed, Some(&install_root))?')
content = content.replace('plan_missing_chunks(&local_sources, &staged_chunks_root, &repair_files)?', 'plan_missing_chunks(&local_sources, &staged_chunks_root, &repair_files, Some(&install_root))?')

# 2. Add get_applicable_delta_patch before plan_missing_chunks
delta_helper = r'''fn get_applicable_delta_patch(file: &FileEntry, install_root: &Path) -> Option<ChunkRef> {
    let patches = file.delta_patches.as_ref()?;
    if patches.is_empty() {
        return None;
    }
    
    let target = install_root.join(&file.path);
    if !target.exists() {
        return None;
    }
    
    if let Ok(hash) = sha256_file(&target) {
        if let Some(patch) = patches.iter().find(|p| p.from_sha256 == hash) {
            return Some(ChunkRef {
                hash: format!("delta-{}", patch.compressed_sha256),
                file_offset: 0,
                uncompressed_size: patch.uncompressed_size,
                pack_id: patch.pack_id.clone(),
                pack_offset: patch.pack_offset,
                compressed_size: patch.compressed_size,
                compressed_sha256: patch.compressed_sha256.clone(),
                codec: patch.codec,
                encryption: patch.encryption.clone(),
            });
        }
    }
    None
}

fn plan_missing_chunks('''

content = content.replace('fn plan_missing_chunks(', delta_helper)

# 3. Update plan_missing_chunks definition
plan_old = r'''fn plan_missing_chunks(
    local_sources: &HashMap<String, LocalChunkSource>,
    staged_chunks_root: &Path,
    changed: &[FileEntry],
) -> Result<Vec<ChunkRef>, JobError> {
    let mut seen = HashSet::new();
    let mut missing = Vec::new();
    for file in changed {
        for chunk in &file.chunks {'''

plan_new = r'''fn plan_missing_chunks(
    local_sources: &HashMap<String, LocalChunkSource>,
    staged_chunks_root: &Path,
    changed: &[FileEntry],
    install_root: Option<&Path>,
) -> Result<Vec<ChunkRef>, JobError> {
    let mut seen = HashSet::new();
    let mut missing = Vec::new();
    for file in changed {
        let mut chunks_to_check = Vec::new();
        if let Some(root) = install_root {
            if let Some(delta_chunk) = get_applicable_delta_patch(file, root) {
                chunks_to_check.push(delta_chunk);
            }
        }
        if chunks_to_check.is_empty() {
            chunks_to_check.extend(file.chunks.iter().cloned());
        }

        for chunk in &chunks_to_check {'''

content = content.replace(plan_old, plan_new)

with open('src/job.rs', 'w', encoding='utf-8') as f:
    f.write(content)
print('Done!')
