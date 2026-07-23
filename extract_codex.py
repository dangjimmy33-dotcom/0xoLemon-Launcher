import json
import re

session_file = r'C:\Users\conte\.codex\sessions\2026\07\22\rollout-2026-07-22T07-43-00-019f8746-8d08-7791-8fc4-c0d2e2887604.jsonl'

with open(session_file, 'r', encoding='utf-8') as f:
    content = f.read()

print(f'Session file size: {len(content)} bytes')

# Search for job.rs content - look for large blocks of Rust code
# The codex usually writes file content via patch or full file write
# Try to find the longest continuous block that looks like job.rs

# Look for fn spawn_patch_job or run_real_patch_job in the raw content
matches = [(m.start(), m.end()) for m in re.finditer(r'pub fn spawn_patch_job', content)]
print(f'Found {len(matches)} occurrences of spawn_patch_job')
for start, end in matches:
    print(content[max(0, start-100):start+500])
    print('---')
