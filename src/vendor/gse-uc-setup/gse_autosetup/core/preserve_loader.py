from __future__ import annotations

import configparser
import hashlib
import shutil
from dataclasses import dataclass
from pathlib import Path

from .package import GSEPackage, resource_path


PROXY_CANDIDATES = ("version.dll", "winhttp.dll", "dinput8.dll")


@dataclass(frozen=True)
class PreserveDeployment:
    proxy_path: Path
    coldloader_path: Path
    coldloader_ini: Path
    emulator_path: Path
    settings_dir: Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def _seed_root() -> Path:
    root = resource_path("preserve_seed")
    required = [root / "version.dll", root / "coldloader.asi", root / "coldloader.ini", root / "version.ini"]
    missing = [p.name for p in required if not p.is_file()]
    if missing:
        raise RuntimeError("Preserve-original loader resources are incomplete: " + ", ".join(missing))
    return root


def choose_proxy_name(exe_dir: Path, steamstub_enabled: bool = True) -> str:
    exe_dir = Path(exe_dir)
    for name in PROXY_CANDIDATES:
        if steamstub_enabled and name.lower() == "winmm.dll":
            continue
        if not (exe_dir / name).exists():
            return name
    raise RuntimeError(
        "Preserve-original mode could not find a free proxy DLL name. "
        "version.dll, winhttp.dll and dinput8.dll are already present."
    )


def _write_coldloader_ini(template: Path, destination: Path, appid: int, emulator_name: str) -> None:
    parser = configparser.ConfigParser(interpolation=None)
    parser.read(template, encoding="utf-8")
    if not parser.has_section("settings"):
        parser.add_section("settings")
    parser.set("settings", "appid", str(int(appid)))
    parser.set("settings", "steamclient64", f'"{emulator_name}"')
    with destination.open("w", encoding="utf-8", newline="\n") as f:
        parser.write(f, space_around_delimiters=True)


def deploy_preserve_loader(
    package: GSEPackage,
    exe_dir: Path,
    appid: int,
    *,
    steamstub_enabled: bool = True,
    proxy_name: str | None = None,
    log=None,
) -> PreserveDeployment:
    """Deploy an x64 ColdLoader/ASI path while leaving steam_api64.dll untouched."""
    log = log or (lambda _m: None)
    exe_dir = Path(exe_dir).resolve()
    seed = _seed_root()
    proxy_name = proxy_name or choose_proxy_name(exe_dir, steamstub_enabled=steamstub_enabled)
    proxy_path = exe_dir / proxy_name
    coldloader_path = exe_dir / "coldloader.asi"
    ini_path = exe_dir / "coldloader.ini"
    emulator_path = exe_dir / "gse_steamclient64.dll"
    settings_dir = exe_dir / "steam_settings"

    shutil.copy2(seed / "version.dll", proxy_path)
    proxy_ini_name = Path(proxy_name).with_suffix(".ini").name
    shutil.copy2(seed / "version.ini", exe_dir / proxy_ini_name)
    shutil.copy2(seed / "coldloader.asi", coldloader_path)
    source_emu = package.steamclient("x64")
    shutil.copy2(source_emu, emulator_path)
    _write_coldloader_ini(seed / "coldloader.ini", ini_path, appid, emulator_path.name)
    log(f"Preserve mode proxy: {proxy_name}")
    log(f"ColdLoader will load GSE steamclient: {emulator_path.name}")
    return PreserveDeployment(proxy_path, coldloader_path, ini_path, emulator_path, settings_dir)
