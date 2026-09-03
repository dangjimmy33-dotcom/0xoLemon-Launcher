from pathlib import Path
import hashlib
import re

root = Path(__file__).resolve().parent
text = (root / 'gse_auto_setup.rs').read_text(encoding='utf-8')
lib = (root / 'lib.rs').read_text(encoding='utf-8')
bridge = (root.parent / 'gse-core' / 'gse_core_bridge.py').read_text(encoding='utf-8')
original_core = (root / 'gse_original_core.rs').read_text(encoding='utf-8')

assert 'gse_auto_setup_run' in text
assert 'gse_auto_setup_restore' in text
assert 'gse_auto_setup_list_saves' in text
assert 'gse_auto_setup_create_snapshot' in text
assert 'steam_web_api_key' not in text.lower(), 'plaintext-named API key should not be exposed as a config field'
assert 'STEAM_API_KEY' not in text, 'API key must not be stored as a plaintext constant'
assert 'gse_auto_setup::gse_auto_setup_run' in lib
assert 'const MASK: u8 = 0x5A' in text
assert 'setup_sync(app, config)' in text
assert 'skip_achievements: bool' in text
assert 'command.arg("-skip_ach")' in text, 'Rust fallback must support compatibility retry for broken full generator builds'
assert 'run_official_generator(resource_root, app, cfg.app_id, &mut logs, false)' in text
assert 'run_official_generator(resource_root, app, cfg.app_id, &mut logs, true)' in text
assert 'Setup aborted to avoid incomplete steam_settings output' in text
assert 'preserve_generated_achievements' in text
assert 'Preserved canonical generator achievements.json unchanged.' in text
assert 'Official achievements.json missing; fetching' in text

# Reconstruct the XOR-obfuscated backend token without embedding the plaintext
# credential in this test file, and verify only its expected SHA-256 digest.
mask_match = re.search(r'const MASK: u8 = 0x([0-9A-Fa-f]+);', text)
data_match = re.search(r'const DATA: \[u8; 32\] = \[([^\]]+)\];', text, re.S)
assert mask_match and data_match
mask = int(mask_match.group(1), 16)
encoded = [int(value.strip()) for value in data_match.group(1).split(',') if value.strip()]
assert len(encoded) == 32
reconstructed = bytes(value ^ mask for value in encoded)
assert hashlib.sha256(reconstructed).hexdigest() == '94572da7868e80b88c110f7168fbede4cdea6bb3b23a878044107481ba649be9'

# A PyInstaller onefile sidecar launches generate_emu_config.exe as a second
# frozen executable. It must reset inherited bootloader state and drain stderr,
# otherwise Cryptodome native modules can resolve against the wrong _MEI tree
# and fatal tracebacks can sit unseen until the idle timeout.
assert 'PYINSTALLER_RESET_ENVIRONMENT' in text
assert 'PYTHONUNBUFFERED' in text
assert 'child.stderr.take()' in text
assert 'stderr_thread' in text
assert 'runtime dependency failure' in text
assert 'not retrying -skip_ach' in text
