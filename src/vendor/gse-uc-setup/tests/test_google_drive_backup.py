from pathlib import Path

from gse_autosetup.save_manager.google_drive import GoogleDriveBackupService


def test_upload_uses_resumable_media_and_app_properties(tmp_path, monkeypatch):
    archive = tmp_path / 'backup.zip'
    archive.write_bytes(b'zipdata')
    captured = {}

    class FakeMedia:
        def __init__(self, filename, mimetype, resumable, chunksize):
            captured['media'] = (filename, mimetype, resumable, chunksize)

    class FakeRequest:
        def next_chunk(self, num_retries=0):
            captured['retries'] = num_retries
            return type('Status', (), {'progress': lambda self: 1.0})(), {'id': 'file1', 'name': 'backup.zip'}

    class FakeFiles:
        def create(self, **kwargs):
            captured['create'] = kwargs
            return FakeRequest()

    class FakeService:
        def files(self):
            return FakeFiles()

    svc = GoogleDriveBackupService(service=FakeService(), media_upload_cls=FakeMedia)
    monkeypatch.setattr(svc, 'ensure_game_folder', lambda appid, game_name: 'folder123')
    out = svc.upload_backup(archive, 2050650, 'Resident Evil 4', sha256='abc')

    assert out['id'] == 'file1'
    assert captured['media'][2] is True
    assert captured['create']['body']['parents'] == ['folder123']
    assert captured['create']['body']['appProperties']['gse_appid'] == '2050650'
