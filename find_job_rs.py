import json
import re
import sys

sys.stdout.reconfigure(encoding='utf-8')

session_file = r'C:\Users\conte\.codex\sessions\2026\07\22\rollout-2026-07-22T07-43-00-019f8746-8d08-7791-8fc4-c0d2e2887604.jsonl'

with open(session_file, 'r', encoding='utf-8') as f:
    raw_lines = f.readlines()

# The Codex used apply_patch tool (the OpenAI apply_patch format)
# Let's find all the apply_patch content blocks in the session

# The apply_patch blocks look like:
# *** Begin Patch
# *** Update File: E:\007Launcher\src-tauri\src\job.rs
# ...
# *** End Patch

# These are embedded in shell_command arguments with command = "apply_patch ..."

all_patches_for_job = []

for i, line in enumerate(raw_lines):
    try:
        obj = json.loads(line)
    except:
        continue
    
    payload = obj.get('payload', {})
    ptype = payload.get('type', '')
    
    if ptype == 'function_call':
        name = payload.get('name', '')
        args_str = payload.get('arguments', '')
        
        if name == 'shell_command' and 'apply_patch' in args_str and 'job.rs' in args_str:
            try:
                args = json.loads(args_str)
                cmd = args.get('command', '')
                print(f"\nLine {i}: apply_patch for job.rs")
                print(cmd[:300])
                all_patches_for_job.append((i, cmd))
            except:
                pass

print(f"\n\nTotal apply_patch calls for job.rs: {len(all_patches_for_job)}")
