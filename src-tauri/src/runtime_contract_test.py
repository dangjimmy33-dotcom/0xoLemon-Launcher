from pathlib import Path

root = Path(__file__).parent
job = (root / 'job.rs').read_text(encoding='utf-8')
launch = (root / 'launch.rs').read_text(encoding='utf-8')
deps = (root / 'job' / 'dependencies.rs').read_text(encoding='utf-8')
platform = (root / 'platform.rs').read_text(encoding='utf-8')
lib = (root / 'lib.rs').read_text(encoding='utf-8')

assert 'Self::Manual' in platform, 'game update mode must default to manual'
assert 'PLATFORM_STATE_SCHEMA: u32 = 2' in platform and 'migrated update policy to manual opt-in' in platform, 'existing automatic defaults must migrate to manual once'
assert 'automatic_updates_allowed_now' in job, 'auto patch/update schedulers need one explicit policy gate'
assert 'versions_equivalent(&installed_version, &latest)' in job, 'auto updater must compare canonical versions'
assert 'process.run_as_admin = true' not in launch, 'normalization must not force every executable to elevate'
assert 'run_as_admin: false' in launch, 'fallback launch should be directly trackable'
assert 'WScript.Shell' in deps and '.lnk' in deps, 'desktop shortcut must be a real Windows shell link'
assert 'if request.launch_executable.is_none()' in lib, 'multi-executable shortcuts must defer to the frontend picker'
assert 'TrackedMainProcess' in deps, 'both normal and elevated launch processes must be trackable'
assert 'platform::begin_game_session' in job and 'platform::end_game_session' in job, 'runtime state must be committed on start and exit'

print('backend runtime contract tests passed')
