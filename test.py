import re
data = open(r'E:\007Launcher\src-tauri\assets\games\persona-5-royal\core.0xo', 'rb').read()
match = re.search(b'"id":"([^"]+)"', data)
print(match.group(1).decode() if match else 'not found')
