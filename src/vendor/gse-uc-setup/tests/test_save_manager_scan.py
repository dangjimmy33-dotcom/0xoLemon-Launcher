from pathlib import Path

from gse_autosetup.save_manager.scanner import default_gse_saves_root, scan_save_root


def test_default_gse_saves_root_uses_appdata(monkeypatch):
    monkeypatch.setenv('APPDATA', r'C:\Users\Test\AppData\Roaming')
    assert str(default_gse_saves_root()).endswith(r'AppData\Roaming/GSE Saves') or str(default_gse_saves_root()).endswith(r'AppData\Roaming\GSE Saves')


def test_scanner_uses_numeric_child_folder_as_appid_and_ignores_settings(tmp_path):
    root = tmp_path / 'GSE Saves'
    (root / '2050650').mkdir(parents=True)
    (root / '2050650' / 'save.bin').write_bytes(b'abc')
    (root / 'settings').mkdir()
    (root / 'not-a-game').mkdir()

    entries = scan_save_root(root)

    assert [e.appid for e in entries] == [2050650]
    assert entries[0].save_folder == root / '2050650'
    assert entries[0].size_bytes == 3
