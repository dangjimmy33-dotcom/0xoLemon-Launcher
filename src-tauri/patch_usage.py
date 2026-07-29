import re

with open('src/job.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Replace file.chunks usage with our new logic for chunk_usage population and cleanup
old_usage_pop = r'''          for file in &changed {
              for chunk in &file.chunks {
                  *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
              }
          }'''

new_usage_pop = r'''          for file in &changed {
              let mut chunks_to_check = Vec::new();
              if let Some(delta_chunk) = get_applicable_delta_patch(file, &install_root) {
                  chunks_to_check.push(delta_chunk);
              } else {
                  chunks_to_check.extend(file.chunks.iter().cloned());
              }
              for chunk in &chunks_to_check {
                  *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
              }
          }'''

content = content.replace(old_usage_pop, new_usage_pop)

old_usage_pop_repair = r'''          for file in &repair_files {
              for chunk in &file.chunks {
                  *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
              }
          }'''

new_usage_pop_repair = r'''          for file in &repair_files {
              let mut chunks_to_check = Vec::new();
              if let Some(delta_chunk) = get_applicable_delta_patch(file, &install_root) {
                  chunks_to_check.push(delta_chunk);
              } else {
                  chunks_to_check.extend(file.chunks.iter().cloned());
              }
              for chunk in &chunks_to_check {
                  *chunk_usage.entry(chunk.hash.clone()).or_insert(0) += 1;
              }
          }'''

content = content.replace(old_usage_pop_repair, new_usage_pop_repair)

old_cleanup = r'''              // Free up disk space immediately by deleting chunks no longer needed
              for chunk in &file.chunks {
                  if let Some(count) = chunk_usage.get_mut(&chunk.hash) {
                      *count = count.saturating_sub(1);
                      if *count == 0 {
                          let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
                          let _ = fs::remove_file(&chunk_path);
                      }
                  }
              }'''

new_cleanup = r'''              // Free up disk space immediately by deleting chunks no longer needed
              let mut chunks_to_check = Vec::new();
              if let Some(delta_chunk) = get_applicable_delta_patch(file, &install_root) {
                  chunks_to_check.push(delta_chunk);
              } else {
                  chunks_to_check.extend(file.chunks.iter().cloned());
              }
              for chunk in &chunks_to_check {
                  if let Some(count) = chunk_usage.get_mut(&chunk.hash) {
                      *count = count.saturating_sub(1);
                      if *count == 0 {
                          let chunk_path = staged_chunks_root.join(format!("{}.chunk", chunk.hash));
                          let _ = fs::remove_file(&chunk_path);
                      }
                  }
              }'''

content = content.replace(old_cleanup, new_cleanup)

with open('src/job.rs', 'w', encoding='utf-8') as f:
    f.write(content)
print('Done chunk_usage patch!')
