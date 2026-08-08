from pathlib import Path
import json

root = Path(__file__).resolve().parents[2]
module = (root / 'src/cloud_redirect_v2/mod.rs').read_text(encoding='utf-8')
lib = (root / 'src/lib.rs').read_text(encoding='utf-8')
models = (root / 'src/cloud_redirect_v2/models.rs').read_text(encoding='utf-8')
config = (root / 'src/cloud_redirect_v2/upstream_config.rs').read_text(encoding='utf-8')
integration = (root / 'src/cloud_redirect_v2/integration.rs').read_text(encoding='utf-8')
backup = (root / 'src/cloud_redirect_v2/backup.rs').read_text(encoding='utf-8')
build = (root / 'build-cloudredirect.ps1').read_text(encoding='utf-8')
tauri = json.loads((root / 'tauri.conf.json').read_text(encoding='utf-8'))

assert 'ENGINE_VERSION: &str = "2.6.3"' in models
for name in ('backup', 'engine', 'integration', 'manifest_pinning', 'models', 'upstream_config'):
    assert f'mod {name};' in module
for provider in ('gdrive', 'onedrive', 'r2', 's3', 'folder', 'local'):
    assert f'"{provider}"' in config
for command in (
    'cloud_redirect_engine_get_status', 'cloud_redirect_engine_install',
    'cloud_redirect_engine_run_required_patches', 'cloud_redirect_engine_save_provider',
    'cloud_redirect_engine_list_apps', 'cloud_redirect_engine_list_files',
    'cloud_redirect_engine_sync_app', 'cloud_redirect_engine_sync_all',
    'cloud_redirect_engine_delete_app',
    'cloud_redirect_engine_get_manifest_pins', 'cloud_redirect_engine_save_manifest_pins',
    'cloud_redirect_engine_create_backup', 'cloud_redirect_engine_list_backups',
    'cloud_redirect_engine_restore_backup', 'cloud_redirect_engine_list_stats',
    'cloud_redirect_engine_migrate',
    'cloud_redirect_engine_gc_blobs', 'cloud_redirect_engine_publish_manifest',
    'cloud_redirect_engine_prune_legacy', 'cloud_redirect_engine_run_cloud760',
    'cloud_redirect_engine_diagnostics',
):
    assert f'pub fn {command}' in integration
    assert f'cloud_redirect_v2::{command}' in lib

# The official engine adapter is the only exposed sync surface. Legacy mock
# commands remain source-compatible internally for OAuth migration only, but
# must not be registered with Tauri.
for legacy in (
    'cloud_redirect_v2_get_status', 'cloud_redirect_enable', 'cloud_redirect_disable',
    'cloud_redirect_set_local_path', 'cloud_redirect_trigger_sync',
    'cloud_redirect_get_sync_status', 'cloud_redirect_list_game_saves',
    'cloud_redirect_backup_save', 'cloud_redirect_reset_game',
    'cloud_redirect_list_backups', 'cloud_redirect_restore_backup',
):
    assert f'cloud_redirect_v2::{legacy}' not in lib

# Folder mode must be registered for both the DLL (sync_path) and upstream CLI
# (token_paths/token_path), otherwise integrated migration and game management
# silently point at different locations.
assert 'register_token_path(&mut config, "folder", &sync_path);' in config

# Destructive cloud deletion is blocked until the remote app is synchronized
# locally and a rollback archive has been created.
assert 'pub fn create_safety' in backup
assert '"before-remote-delete"' in integration
assert integration.index('"sync-remote-app".to_string()') < integration.index('"before-remote-delete"') < integration.index('"delete-remote-app".to_string()')

assert '-A x64' in build and '-A Win32' in build
assert '--target cloud_redirect cloud_redirect_cli' in build
assert '--target cloud760_tool' in build
resources = tauri['bundle']['resources']
assert 'resources/cloud_redirect/**/*' in resources
assert 'vendor/cloudredirect/Version.props' in resources
assert 'build-cloudredirect.ps1' in tauri['build']['beforeBuildCommand']
print('CloudRedirect backend contract PASS')
