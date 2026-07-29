import re

with open('src/job.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_logic = r'''        let delta_payload = read_chunk_bytes(&delta_chunk, local_sources, staged_chunks_root)?;
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
        drop(output);'''

new_logic = r'''        let delta_payload = read_chunk_bytes(&delta_chunk, local_sources, staged_chunks_root)?;
        
        let delta_tmp_path = sibling_path(&target, "007launcher.delta")?;
        fs::write(&delta_tmp_path, &delta_payload)?;
        
        let stats = oxidelta::io::decode_file(&target, &delta_tmp_path, &temp)
            .map_err(|e| JobError::Depot(format!("Delta patch decode failed: {:?}", e)))?;
            
        let _ = fs::remove_file(&delta_tmp_path);
        
        let actual = hex::encode(stats.output_sha256.unwrap_or([0; 32]));
        
        if actual != file.sha256 {
            let _ = fs::remove_file(&temp);
            return Err(JobError::Depot(format!(
                "assembled file (delta) hash mismatch: {}",
                file.path
            )));
        }'''

if old_logic in content:
    content = content.replace(old_logic, new_logic)
    with open('src/job.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print('Done!')
else:
    print('Not found!')
