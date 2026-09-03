from pathlib import Path

from gse_autosetup.core.models import SteamApiTarget
from gse_autosetup.core.uc_online import UCOnlineInstaller, UCOnlinePackage, detect_uc_backends


def _uc_package(tmp_path: Path) -> UCOnlinePackage:
    root = tmp_path / "uc"
    (root / "x64").mkdir(parents=True)
    (root / "x86").mkdir(parents=True)
    (root / "plugins").mkdir(parents=True)
    (root / "x64" / "steam_api64.dll").write_bytes(b"UC64")
    (root / "x86" / "steam_api.dll").write_bytes(b"UC86")
    (root / "plugins" / "photon_universal_plugin.dll").write_bytes(b"PHOTON")
    (root / "plugins" / "EOS_custom_plugin.dll").write_bytes(b"EOS")
    return UCOnlinePackage(root, "vtest")


def test_uc_online_package_selects_architecture(tmp_path: Path):
    pkg = _uc_package(tmp_path)
    assert pkg.dll("x64").read_bytes() == b"UC64"
    assert pkg.dll("x86").read_bytes() == b"UC86"


def test_uc_online_install_backs_up_api_and_writes_ini_and_plugin(tmp_path: Path):
    pkg = _uc_package(tmp_path)
    game = tmp_path / "game"
    api_dir = game / "Plugins" / "x86_64"
    api_dir.mkdir(parents=True)
    api = api_dir / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    exe = game / "Game.exe"
    exe.write_bytes(b"MZ")

    installer = UCOnlineInstaller(pkg)
    installer.install(
        game,
        [SteamApiTarget(api, "x64")],
        exe,
        appid=1940340,
        spoof_appid=480,
        plugins=["photon"],
        runtime_steamstub=True,
    )

    assert api.read_bytes() == b"UC64"
    assert Path(str(api) + ".bak").read_bytes() == b"ORIGINAL"
    ini = (game / "union-crax.ini").read_text(encoding="utf-8")
    assert "AppId=480" in ini
    assert "ogAppId=1940340" in ini
    assert "GetStubbedLol=true" in ini
    assert (game / "plugins" / "photon_universal_plugin.dll").is_file()


def test_detect_uc_backends_uses_file_presence(tmp_path: Path):
    game = tmp_path / "game"
    game.mkdir()
    (game / "EOSSDK-Win64-Shipping.dll").write_bytes(b"")
    (game / "PhotonRealtime.dll").write_bytes(b"")
    found = detect_uc_backends(game)
    assert "eos" in found
    assert "photon" in found
