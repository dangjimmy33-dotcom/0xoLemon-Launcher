import re

with open('extracted_all_input.txt', 'r', encoding='utf-8') as f:
    text = f.read()

matches = re.findall(r'TOOL: exec\nconst command = ".*?Get-Content.*?"', text)
print(f'Found {len(matches)} Get-Content commands')
for m in matches:
    print(m)
