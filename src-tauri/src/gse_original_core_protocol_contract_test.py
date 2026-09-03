from pathlib import Path

root = Path(__file__).resolve().parent
rust = (root / 'gse_original_core.rs').read_text(encoding='utf-8')
bridge = (root.parent / 'gse-core' / 'gse_core_bridge.py').read_text(encoding='utf-8')

assert "read_until(b'\\n'" in rust, 'child output must be read as raw bytes, not BufRead::lines() UTF-8 strings'
assert 'String::from_utf8_lossy' in rust, 'child output must tolerate Windows/non-UTF-8 byte sequences'
assert 'ensure_ascii=True' in bridge, 'JSON-lines protocol must remain ASCII-safe across Windows locale encodings'
print('gse original-core protocol contract: PASS')
