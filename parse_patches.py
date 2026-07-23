import json
import re
import codecs

with open('extracted_all_input.txt', 'r', encoding='utf-8') as f:
    content = f.read()

patches = re.findall(r'const patch = \"(.*?)\";\s*const result = await tools\.apply_patch', content, re.DOTALL)
print(f'Found {len(patches)} patches')

with open('all_patches.txt', 'w', encoding='utf-8') as out:
    for i, p in enumerate(patches):
        p = p.replace('\\n', '\n').replace('\\\"', '\"').replace('\\\\', '\\')
        out.write(f'--- Patch {i+1} ---\n')
        out.write(p + '\n\n')
