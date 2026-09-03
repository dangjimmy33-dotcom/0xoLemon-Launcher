from pathlib import Path

ROOT = Path(__file__).resolve().parent
LIB = ROOT / 'lib.rs'
MODULE = ROOT / 'gse_uc_setup.rs'
LUA_PROFILES = ROOT / 'lua_runtime_profiles.rs'
TAURI_CONF = ROOT.parent / 'tauri.conf.json'


def test_module_and_commands_are_registered():
    lib = LIB.read_text(encoding='utf-8')
    assert 'mod gse_uc_setup;' in lib
    for command in (
        'get_gse_uc_resource_health',
        'plan_gse_uc_setup',
        'apply_gse_uc_setup',
        'verify_gse_uc_setup',
        'repair_gse_uc_setup',
        'restore_gse_uc_setup',
        'launch_migrate_gse',
    ):
        assert f'gse_uc_setup::{command}' in lib


def test_service_loads_manifest_and_separates_integrity_from_provenance():
    src = MODULE.read_text(encoding='utf-8')
    assert 'manifest.json' in src
    assert 'integrity_verified' in src
    assert 'provenance_verified' in src
    assert 'RUNTIME_INTEGRITY_FAILED' in src
    assert 'RUNTIME_TARGET_NOT_APPROVED' in src
    assert 'managed_file_transaction' in src


def test_lua_runtime_health_uses_manifest_only_summary_on_settings_surfaces():
    src = LUA_PROFILES.read_text(encoding='utf-8')
    assert 'gse_uc_setup::lua_component_summary' in src
    assert 'gse_uc_setup::lua_component_health' not in src

    service = MODULE.read_text(encoding='utf-8')
    start = service.index('pub fn lua_component_summary')
    end = service.index('pub fn lua_component_health', start)
    summary = service[start:end]
    assert 'read_manifest' in summary
    assert 'compute_health' not in summary
    assert 'verify_component_at' not in summary
    assert 'sha256_file' not in summary


def test_tauri_bundles_gse_uc_directory():
    src = TAURI_CONF.read_text(encoding='utf-8')
    assert 'resources/gse-uc/' in src


def test_steamless_is_planned_and_committed_inside_managed_transaction():
    src = MODULE.read_text(encoding='utf-8')
    assert '@generated/steamless/' in src
    assert 'steamless_generated_bytes' in src
    # The orchestrator must not patch the live executable after the managed transaction.
    transaction_pos = src.index('managed_file_transaction::apply_changes(')
    tail = src[transaction_pos:]
    assert 'crate::steamless::unpack_exe(&exe' not in tail


def test_repair_allows_known_missing_or_original_state_but_blocks_unknown_drift():
    src = MODULE.read_text(encoding='utf-8')
    assert 'current_hash.is_none()' in src
    assert 'owned.original_sha256.as_deref() == current_hash' in src
    assert 'RUNTIME_EXTERNAL_MODIFICATION' in src
    repair = src[src.index('pub fn repair_gse_uc_setup'):src.index('pub fn restore_gse_uc_setup')]
    assert 'apply_gse_uc_setup' in repair
    assert 'Repair refuses external modification' not in repair


def test_backup_is_flushed_before_receipt_can_reference_it():
    src = MODULE.read_text(encoding='utf-8')
    backup = src[src.index('fn copy_backup'):src.index('fn generated_bytes')]
    assert '.sync_all()' in backup
