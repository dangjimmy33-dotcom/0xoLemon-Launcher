from pathlib import Path
import re

job = Path(__file__).with_name('job.rs').read_text(encoding='utf-8')

start = job.index('fn load_installed_update_base(')
end = job.index('\nfn load_manifest_pair(', start)
body = job[start:end]

assert 'catalog history pruned' in body, 'missing pruned-catalog local manifest recovery branch'
assert re.search(r'if\s*!catalog_has_version\(catalog,\s*&version\).*?installed_manifest\.as_ref\(\)', body, re.S), \
    'state.0xo path must consult local manifest.0xo before failing on pruned catalog history'
assert 'versions_equivalent(manifest_version, &version)' in body, \
    'local manifest version must be matched canonically against the committed state marker'

# A manifest-only recovery must not require the historical version to remain in the mutable catalog.
manifest_recovery = body[body.index('// Generic recovery path'):]
assert 'if !catalog_has_version(catalog, &version)' not in manifest_recovery, \
    'manifest-only recovery still incorrectly depends on catalog history'

print('pruned catalog update regression contract: PASS')
