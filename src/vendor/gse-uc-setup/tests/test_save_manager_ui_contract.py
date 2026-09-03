from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_main_window_has_two_top_level_tabs_and_save_manager_actions():
    main = (ROOT / 'gse_autosetup' / 'ui' / 'main_window.py').read_text(encoding='utf-8')
    save = (ROOT / 'gse_autosetup' / 'ui' / 'save_manager_tab.py').read_text(encoding='utf-8')
    assert 'Setup & Emulator' in main
    assert 'Savegame Manager' in main
    assert 'Connect Google Drive' in save
    assert 'Backup to Drive' in save
    assert 'Restore from Drive' in save


def test_pyinstaller_and_requirements_include_google_drive_runtime():
    req = (ROOT / 'requirements.txt').read_text(encoding='utf-8')
    assert 'google-auth' in req
    assert 'google-auth-oauthlib' in req
    assert 'google-api-python-client' in req
    spec = (ROOT / 'GSEAutoSetup.spec').read_text(encoding='utf-8')
    assert 'resources/google' in spec.replace('\\', '/')
