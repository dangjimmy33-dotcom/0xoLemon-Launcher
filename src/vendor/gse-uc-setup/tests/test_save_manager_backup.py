import json
import zipfile
from pathlib import Path

from gse_autosetup.save_manager.backup import SaveBackupManager


def test_backup_contains_complete_appid_folder_and_manifest(tmp_path):
    saves = tmp_path / 'GSE Saves'
    source = saves / '2050650'
    (source / 'remote').mkdir(parents=True)
    (source / 'remote' / 'slot1.sav').write_bytes(b'one')
    (source / 'stats').mkdir()
    (source / 'stats' / 'stats.json').write_text('{"x":1}', encoding='utf-8')

    manager = SaveBackupManager(tmp_path / 'backups')
    archive = manager.create_backup(2050650, source, 'Resident Evil 4')

    assert archive.is_file()
    with zipfile.ZipFile(archive) as z:
        names = set(z.namelist())
        assert '2050650/remote/slot1.sav' in names
        assert '2050650/stats/stats.json' in names
        manifest = json.loads(z.read('.gse-save-manifest.json'))
    assert manifest['appid'] == 2050650
    assert manifest['game_name'] == 'Resident Evil 4'
    assert manifest['file_count'] == 2


def test_restore_creates_safety_backup_and_replaces_live_folder(tmp_path):
    saves = tmp_path / 'GSE Saves'
    live = saves / '2050650'
    live.mkdir(parents=True)
    (live / 'old.sav').write_bytes(b'old')
    manager = SaveBackupManager(tmp_path / 'backups')

    original = tmp_path / 'original'
    original.mkdir()
    (original / 'new.sav').write_bytes(b'new')
    archive = manager.create_backup(2050650, original, 'Resident Evil 4')

    result = manager.restore_backup(archive, saves)

    assert (saves / '2050650' / 'new.sav').read_bytes() == b'new'
    assert not (saves / '2050650' / 'old.sav').exists()
    assert result.safety_backup is not None
    assert result.safety_backup.is_file()
