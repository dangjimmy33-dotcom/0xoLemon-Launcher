import re

with open('all_patches.txt', 'r', encoding='utf-8') as f:
    text = f.read()

funcs_to_find = [
    'default_patch_journal',
    'spawn_patch_job',
    'run_real_patch_job',
    'try_apply_patch_fix',
    'record_patch_download_progress',
    'fetch_patch_pack_span_with_journal_progress',
    'check_patch_available',
    'validate_patch_manifest',
    'patch_target_version_from_marker',
    'applied_patch_manifest_path',
    'write_applied_patch_manifest',
    'read_applied_patch_manifest',
    'clear_applied_patch_manifest',
    'patch_transfer_bytes'
]

with open('missing.rs', 'w', encoding='utf-8') as out:
    for i, patch in enumerate(text.split('*** Begin Patch')):
        if 'job.rs' in patch:
            lines = patch.split('\n')
            added = []
            for line in lines:
                if line.startswith('+'):
                    added.append(line[1:])
            
            content = '\n'.join(added)
            if any(f in content for f in funcs_to_find):
                out.write(content + '\n')
