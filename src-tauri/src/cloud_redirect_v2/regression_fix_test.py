from pathlib import Path
root = Path(__file__).resolve().parents[2]
engine = (root/'src/cloud_redirect_v2/engine.rs').read_text(encoding='utf-8')
detector = (root/'src/cloud_redirect/steam_detector.rs').read_text(encoding='utf-8')
models = (root/'src/cloud_redirect_v2/models.rs').read_text(encoding='utf-8')
integration = (root/'src/cloud_redirect_v2/integration.rs').read_text(encoding='utf-8')
lib = (root/'src/lib.rs').read_text(encoding='utf-8')
# Windows FlushFileBuffers needs a write-capable handle.
assert 'OpenOptions::new()' in engine and '.write(true)' in engine and '.open(&temporary)' in engine
assert 'File::open(&temporary)' not in engine
# Process detection must parse tasklist CSV exactly and expose PIDs.
assert 'pub fn steam_process_ids() -> Vec<u32>' in detector
assert '/FO", "CSV"' in detector or '"/FO", "CSV"' in detector
# V2 status exposes exact Steam compatibility and a lightweight polling command.
for field in ('steam_version:', 'steam_version_supported:', 'steam_process_ids:'):
    assert field in models
assert 'pub fn cloud_redirect_engine_get_steam_state' in integration
assert 'pub fn cloud_redirect_engine_close_steam' in integration
assert 'cloud_redirect_v2::cloud_redirect_engine_get_steam_state' in lib
assert 'cloud_redirect_v2::cloud_redirect_engine_close_steam' in lib
print('CloudRedirect regression fix contract PASS')
