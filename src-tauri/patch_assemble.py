import re

with open('src/job.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_logic = r'''    // Assemble directly in install folder (no staging)
    let temp = sibling_path(&target, "007launcher.tmp")?;
    let backup = sibling_path(&target, "007launcher.bak")?;
    let mut output = File::create(&temp)?;
    let mut hasher = Sha256::new();

    for chunk in &file.chunks {
        let data = read_chunk_bytes(chunk, local_sources, staged_chunks_root)?;
        hasher.update(&data);
        output.write_all(&data)?;
    }
    output.flush()?;
    drop(output);

    let actual = hex::encode(hasher.finalize());
    if actual != file.sha256 {
        let _ = fs::remove_file(&temp);
        return Err(JobError::Depot(format!(
            "assembled file hash mismatch: {}",
            file.path
        )));
    }'''

new_logic = r'''    // Assemble directly in install folder (no staging)
    let temp = sibling_path(&target, "007launcher.tmp")?;
    let backup = sibling_path(&target, "007launcher.bak")?;
    
    let mut using_delta = false;
    
    if let Some(delta_chunk) = get_applicable_delta_patch(file, install_root) {
        using_delta = true;
        
        let delta_payload = read_chunk_bytes(&delta_chunk, local_sources, staged_chunks_root)?;
        let old_file_bytes = fs::read(&target).map_err(|e| JobError::Depot(format!("Failed to read old file for delta patching: {e}")))?;
        
        let new_file_bytes = oxidelta::io::decode_file(&old_file_bytes, &delta_payload)
            .map_err(|e| JobError::Depot(format!("Delta patch decode failed: {:?}", e)))?;
            
        let mut hasher = Sha256::new();
        hasher.update(&new_file_bytes);
        let actual = hex::encode(hasher.finalize());
        
        if actual != file.sha256 {
            return Err(JobError::Depot(format!(
                "assembled file (delta) hash mismatch: {}",
                file.path
            )));
        }
        
        let mut output = File::create(&temp)?;
        output.write_all(&new_file_bytes)?;
        output.flush()?;
        drop(output);
    }
    
    if !using_delta {
        let mut output = File::create(&temp)?;
        let mut hasher = Sha256::new();

        for chunk in &file.chunks {
            let data = read_chunk_bytes(chunk, local_sources, staged_chunks_root)?;
            hasher.update(&data);
            output.write_all(&data)?;
        }
        output.flush()?;
        drop(output);

        let actual = hex::encode(hasher.finalize());
        if actual != file.sha256 {
            let _ = fs::remove_file(&temp);
            return Err(JobError::Depot(format!(
                "assembled file hash mismatch: {}",
                file.path
            )));
        }
    }'''

content = content.replace(old_logic, new_logic)

with open('src/job.rs', 'w', encoding='utf-8') as f:
    f.write(content)
print('Done!')
