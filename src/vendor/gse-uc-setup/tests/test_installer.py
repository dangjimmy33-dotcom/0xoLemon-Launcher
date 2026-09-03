from pathlib import Path
from gse_autosetup.core.installer import backup_files, restore_manifest

def test_backup_and_restore_round_trip(tmp_path):
    game = tmp_path / "game"
    game.mkdir()
    dll = game / "steam_api64.dll"
    dll.write_bytes(b"ORIGINAL")
    settings = game / "steam_settings"
    settings.mkdir()
    appid = settings / "steam_appid.txt"
    appid.write_text("111", encoding="utf-8")

    manifest = backup_files(game, [dll, appid], tmp_path / "backup")
    dll.write_bytes(b"CHANGED")
    appid.write_text("222", encoding="utf-8")
    restore_manifest(manifest)

    assert dll.read_bytes() == b"ORIGINAL"
    assert appid.read_text(encoding="utf-8") == "111"

from gse_autosetup.core.installer import adjacent_backup_path, fallback_adjacent_backup_path, choose_adjacent_backup_path, ensure_adjacent_original_backup


def test_adjacent_backup_is_created_once_and_never_overwritten(tmp_path):
    api = tmp_path / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    backup = ensure_adjacent_original_backup(api)
    assert backup == adjacent_backup_path(api)
    assert backup.read_bytes() == b"ORIGINAL"
    api.write_bytes(b"NEW-CURRENT")
    ensure_adjacent_original_backup(api)
    assert backup.read_bytes() == b"ORIGINAL"


def test_unrelated_dot_bak_uses_gseauto_fallback(tmp_path):
    api = tmp_path / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    adjacent_backup_path(api).write_bytes(b"UNRELATED")
    chosen = choose_adjacent_backup_path(api)
    assert chosen == fallback_adjacent_backup_path(api)
    ensure_adjacent_original_backup(api, destination=chosen)
    assert chosen.read_bytes() == b"ORIGINAL"
    assert adjacent_backup_path(api).read_bytes() == b"UNRELATED"
