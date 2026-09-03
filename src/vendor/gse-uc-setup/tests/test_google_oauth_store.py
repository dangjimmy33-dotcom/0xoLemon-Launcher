import json
from pathlib import Path

from gse_autosetup.save_manager import google_auth
from gse_autosetup.save_manager.oauth_store import OAuthCredentialStore


def test_drive_scope_is_drive_file_not_full_drive():
    assert 'https://www.googleapis.com/auth/drive.file' in google_auth.SCOPES
    assert 'https://www.googleapis.com/auth/drive' not in google_auth.SCOPES


def test_oauth_store_roundtrip_never_writes_plain_refresh_token(tmp_path, monkeypatch):
    monkeypatch.setenv('GSE_OAUTH_TEST_KEY', 'unit-test-key')
    store = OAuthCredentialStore(tmp_path / 'google_oauth.bin')
    payload = {
        'token': 'access-secret',
        'refresh_token': 'refresh-secret',
        'token_uri': 'https://oauth2.googleapis.com/token',
        'client_id': 'client',
        'client_secret': 'secret',
        'scopes': ['https://www.googleapis.com/auth/drive.file'],
    }
    store.save(payload)
    raw = store.path.read_bytes()
    assert b'refresh-secret' not in raw
    assert store.load()['refresh_token'] == 'refresh-secret'
