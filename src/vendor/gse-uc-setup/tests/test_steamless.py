from pathlib import Path

import pytest

from gse_autosetup.core.resources import ResourceManager
from gse_autosetup.core.steamless import SteamlessManager, steamless_backup_path


def _seed(tmp_path: Path) -> ResourceManager:
    resources = tmp_path / "runtime" / "resources"
    cli = resources / "embedded" / "steamless" / "Steamless.CLI.exe"
    cli.parent.mkdir(parents=True)
    cli.write_bytes(b"cli")
    return ResourceManager(app_dir=tmp_path / "portable", runtime_resources=resources)


def test_steamless_promotes_unpacked_and_preserves_original_once(tmp_path: Path):
    rm = _seed(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    exe = game / "Game.exe"
    exe.write_bytes(b"ORIGINAL")
    calls = []

    def runner(cmd, cwd):
        calls.append((cmd, cwd))
        (cwd / "Game.exe.unpacked.exe").write_bytes(b"PATCHED")
        return 0

    manager = SteamlessManager(resources=rm, runner=runner)
    result = manager.patch(exe)

    assert result == exe
    assert exe.read_bytes() == b"PATCHED"
    assert steamless_backup_path(exe).read_bytes() == b"ORIGINAL"
    assert calls and calls[0][0][0].endswith("Steamless.CLI.exe")

    exe.write_bytes(b"SECOND")
    manager.patch(exe)
    assert steamless_backup_path(exe).read_bytes() == b"ORIGINAL"


def test_steamless_failure_does_not_replace_original(tmp_path: Path):
    rm = _seed(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    exe = game / "Game.exe"
    exe.write_bytes(b"ORIGINAL")

    manager = SteamlessManager(resources=rm, runner=lambda cmd, cwd: 1)
    with pytest.raises(RuntimeError):
        manager.patch(exe)
    assert exe.read_bytes() == b"ORIGINAL"
    assert not steamless_backup_path(exe).exists()


def test_parse_steamless_release_selects_release_zip_and_digest():
    from gse_autosetup.core.steamless import parse_steamless_release

    release = parse_steamless_release({
        'tag_name': 'v3.1.0.5',
        'assets': [
            {'name': 'notes.txt', 'browser_download_url': 'n'},
            {
                'name': 'Steamless.v3.1.0.5.-.by.atom0s.zip',
                'browser_download_url': 'https://example/steamless.zip',
                'digest': 'sha256:' + ('a' * 64),
            },
        ],
    })
    assert release.tag == 'v3.1.0.5'
    assert release.url.endswith('steamless.zip')
    assert release.sha256 == 'a' * 64
