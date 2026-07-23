import re

funcs = [
    'default_patch_journal',
    'spawn_patch_job',
    'run_real_patch_job',
    'try_apply_patch_fix',
    'record_patch_download_progress',
    'fetch_patch_pack_span_with_journal_progress',
    'patch_transfer_bytes',
    'check_patch_available',
    'load_patch_manifest',
    'validate_patch_manifest',
    'applied_patch_manifest_path',
    'write_applied_patch_manifest',
    'read_applied_patch_manifest',
    'clear_applied_patch_manifest',
    'patch_target_version_from_marker'
]

with open('all_patches.txt', 'r', encoding='utf-8') as f:
    content = f.read()

# collect all lines
added_lines = []
for patch in content.split('*** Begin Patch'):
    if 'job.rs' in patch:
        for line in patch.split('\n'):
            if line.startswith('+'):
                added_lines.append(line[1:])

added_text = '\n'.join(added_lines)

out_funcs = []
for func in funcs:
    matches = list(re.finditer(r'(?:pub\s+)?fn\s+' + func + r'\s*<.*?>?\s*\(|(?:pub\s+)?fn\s+' + func + r'\s*\(', added_text))
    if matches:
        # Take the LAST match, as it represents the most recent version of the function in the patch sequence!
        match = matches[-1]
        start_idx = match.start()
        
        brace_count = 0
        in_func = False
        end_idx = -1
        for i in range(start_idx, len(added_text)):
            if added_text[i] == '{':
                brace_count += 1
                in_func = True
            elif added_text[i] == '}':
                brace_count -= 1
                if in_func and brace_count == 0:
                    end_idx = i + 1
                    break
        
        if end_idx != -1:
            out_funcs.append(added_text[start_idx:end_idx])
            print(f'Extracted {func}')
        else:
            print(f'Failed to find end of {func}')
    else:
        print(f'Could not find {func}')

with open('final_funcs.rs', 'w', encoding='utf-8') as f:
    f.write('\n\n'.join(out_funcs))
