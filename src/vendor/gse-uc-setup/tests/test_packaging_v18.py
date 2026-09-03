from pathlib import Path

ROOT = Path(__file__).parents[1]


def test_pyinstaller_bundles_full_v18_embedded_resource_tree():
    spec = (ROOT / 'GSEAutoSetup.spec').read_text(encoding='utf-8')
    assert 'resources / "embedded"' in spec or "resources / 'embedded'" in spec
    assert 'resources/embedded' in spec


def test_v18_builder_fetches_optional_online_components_and_identifies_v18():
    build = (ROOT / 'BUILD_EXE.bat').read_text(encoding='utf-8')
    fetch = (ROOT / 'FETCH_V18_RESOURCES.ps1').read_text(encoding='utf-8')
    assert 'GSE / UC Setup V1.8.3' in build
    assert 'FETCH_V18_RESOURCES.ps1' not in build
    assert 'Validating existing runtime resources' in build
    assert 'UnionCrax-Team/uc-online2' in fetch
    assert 'Mush-iii/rune-emu' in fetch
    assert 'atom0s/Steamless' in fetch
    assert "Join-Path $embedded 'uc_online'" in fetch
    assert "Join-Path $embedded 'rune_steamstub'" in fetch


def test_package_version_is_v183():
    init = (ROOT / 'gse_autosetup' / '__init__.py').read_text(encoding='utf-8')
    assert '__version__ = "1.8.3"' in init


def test_v18_fetch_hook_can_embed_official_gse_generator():
    fetch = (ROOT / 'FETCH_V18_RESOURCES.ps1').read_text(encoding='utf-8')
    assert 'alex47exe/gse_fork_tools' in fetch
    assert "Join-Path $embedded 'gse_tools'" in fetch
    assert 'gen_emu_cfg-Windows-Release.7z' in fetch


def test_v181_fetcher_has_retry_and_requires_generator_baseline():
    from pathlib import Path
    text = (Path(__file__).parents[1] / 'FETCH_V18_RESOURCES.ps1').read_text(encoding='utf-8')
    assert 'Invoke-DownloadWithRetry' in text
    assert 'curl.exe' in text
    assert 'Assert-GseToolsBaseline' in text
    assert '$Name:' not in text


def test_builder_copies_portable_resources_beside_exe():
    build = (ROOT / 'BUILD_EXE.bat').read_text(encoding='utf-8')
    assert 'dist\\resources' in build
    assert 'resources\\embedded' in build


def test_resource_refresh_is_explicit_not_part_of_normal_build():
    refresh = (ROOT / 'UPDATE_RESOURCES.bat').read_text(encoding='utf-8')
    assert 'FETCH_V18_RESOURCES.ps1' in refresh
    assert 'BUILD_EXE.bat' not in refresh


def test_fast_builder_never_fetches_resources_or_installs_packages():
    fast = (ROOT / 'BUILD_EXE_FAST.bat').read_text(encoding='utf-8')
    assert 'FETCH_V18_RESOURCES.ps1' not in fast
    assert 'pip install' not in fast.lower()
    assert r'resources\embedded\gse_tools' in fast
    assert 'PyInstaller' in fast


def test_direct_builder_alias_exists_and_is_offline():
    direct = (ROOT / 'BUILD_EXE_DIRECT.bat').read_text(encoding='utf-8')
    assert 'FETCH_V18_RESOURCES.ps1' not in direct
    assert 'GitHub fetch' in direct
