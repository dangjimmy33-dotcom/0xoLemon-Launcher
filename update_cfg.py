import re
with open(r'E:\FC.26.NOTACRACK\DenuvoToken_Output.txt', 'r', encoding='utf-8') as f:
    token = f.read().strip()

cfg_path = r'E:\FC.26.NOTACRACK\anadius.cfg'
with open(cfg_path, 'r', encoding='utf-8') as f:
    cfg = f.read()

cfg = re.sub(r'("DenuvoToken"\s+)"[^"]+"', r'\1"' + token + '"', cfg)

with open(cfg_path, 'w', encoding='utf-8') as f:
    f.write(cfg)

print('Updated anadius.cfg with new token!')
