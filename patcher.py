import sys
import os

def apply_patch_to_file(file_path, patch_text):
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()

    chunks = patch_text.split('@@\n')[1:]
    for chunk in chunks:
        search_lines = []
        replace_lines = []
        for line in chunk.split('\n'):
            if line == '*** End Patch' or line.startswith('*** Update File') or line == '*** Begin Patch':
                continue
            if line.startswith('-'):
                search_lines.append(line[1:])
            elif line.startswith('+'):
                replace_lines.append(line[1:])
            elif line.startswith(' '):
                search_lines.append(line[1:])
                replace_lines.append(line[1:])
            elif line == '':
                search_lines.append('')
                replace_lines.append('')
            else:
                search_lines.append(line)
                replace_lines.append(line)
        
        # Remove trailing empty lines from parsing
        while search_lines and search_lines[-1] == '':
            search_lines.pop()
        while replace_lines and replace_lines[-1] == '':
            replace_lines.pop()

        search_text = '\n'.join(search_lines)
        replace_text = '\n'.join(replace_lines)

        if search_text in content:
            content = content.replace(search_text, replace_text, 1)
            print(f'Applied a chunk successfully to {file_path}')
        else:
            print(f'FAILED to apply chunk to {file_path}')
            print('Search Text:', repr(search_text[:200]))

    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)

with open('all_patches.txt', 'r', encoding='utf-8') as f:
    text = f.read()

for patch in text.split('*** Begin Patch'):
    if 'job.rs' in patch:
        print('Processing a job.rs patch')
        apply_patch_to_file('src-tauri/src/job.rs', patch)
