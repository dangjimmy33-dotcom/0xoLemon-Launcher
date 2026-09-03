from pathlib import Path

from gse_autosetup.core.coldclient import ColdClientInstaller
from gse_autosetup.core.package import GSEPackage


def _package(tmp_path: Path) -> GSEPackage:
    root = tmp_path / "gse"
    cc = root / "steamclient_experimental"
    (cc / "extra_dlls").mkdir(parents=True)
    for name in (
        "steamclient64.dll", "steamclient.dll",
        "steamclient_loader_x64.exe", "steamclient_loader_x86.exe",
        "GameOverlayRenderer64.dll", "GameOverlayRenderer.dll",
    ):
        (cc / name).write_bytes(name.encode())
    (cc / "ColdClientLoader.ini").write_text("[SteamClient]\nExe=game.exe\nAppId=0\n", encoding="utf-8")
    (cc / "extra_dlls" / "steamclient_extra_x64.dll").write_bytes(b"x64-extra")
    (cc / "extra_dlls" / "steamclient_extra_x86.dll").write_bytes(b"x86-extra")
    return GSEPackage(root, "test")


def test_package_exposes_full_coldclient_resources(tmp_path: Path):
    pkg = _package(tmp_path)
    assert pkg.coldclient_loader("x64").name == "steamclient_loader_x64.exe"
    assert pkg.overlay_renderer("x64").name == "GameOverlayRenderer64.dll"
    assert pkg.coldclient_extra("x86").name == "steamclient_extra_x86.dll"


def test_coldclient_install_copies_runtime_and_writes_appid(tmp_path: Path):
    pkg = _package(tmp_path)
    exe_dir = tmp_path / "game"
    exe_dir.mkdir()
    installer = ColdClientInstaller(pkg)
    deployed = installer.install(exe_dir, 1940340, "x64", include_renderer=True, include_extra=True)
    names = {p.name for p in deployed}
    assert "steamclient_loader_x64.exe" in names
    assert "steamclient64.dll" in names
    assert "GameOverlayRenderer64.dll" in names
    assert "steamclient_extra_x64.dll" in names
    ini = (exe_dir / "ColdClientLoader.ini").read_text(encoding="utf-8")
    assert "1940340" in ini
