
from pathlib import Path

from gse_autosetup.core.cache import CacheManager
from gse_autosetup.core.installer import backup_files, restore_manifest


def test_cache_manager_does_not_create_localappdata_backups(tmp_path):
    cache = CacheManager(tmp_path / "cache")
    assert not hasattr(cache, "backups")
    assert not (cache.root / "backups").exists()


def test_stable_game_local_backup_preserves_first_snapshot(tmp_path):
    game = tmp_path / "game"
    game.mkdir()
    target = game / "file.bin"
    target.write_bytes(b"ORIGINAL")
    backup_root = game / ".gse_auto_backup"

    manifest = backup_files(game, [target], backup_root)
    target.write_bytes(b"MODIFIED")
    backup_files(game, [target], backup_root)  # rerun must not overwrite baseline

    restore_manifest(manifest)
    assert target.read_bytes() == b"ORIGINAL"
