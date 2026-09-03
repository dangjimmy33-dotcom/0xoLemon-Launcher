from __future__ import annotations

import configparser
import shutil
from pathlib import Path
from typing import Callable

from .package import GSEPackage


class ColdClientInstaller:
    """Deploy the official GSE ``steamclient_experimental`` runtime.

    This is deliberately separate from the legacy ASI preserve-loader path.
    It mirrors the files shipped by the GSE Windows package and keeps optional
    compatibility pieces explicit.
    """

    def __init__(self, package: GSEPackage, log: Callable[[str], None] | None = None) -> None:
        self.package = package
        self.log = log or (lambda _m: None)

    def install(
        self,
        exe_dir: Path,
        appid: int,
        arch: str,
        *,
        main_exe: Path | None = None,
        include_renderer: bool = True,
        include_extra: bool = False,
        backup_file: Callable[[Path], None] | None = None,
    ) -> list[Path]:
        exe_dir = Path(exe_dir).resolve()
        exe_dir.mkdir(parents=True, exist_ok=True)
        cc = self.package.coldclient_root()
        destinations: list[tuple[Path, Path]] = []

        loader = self.package.coldclient_loader(arch)
        steamclient = self.package.steamclient(arch)
        ini_src = cc / "ColdClientLoader.ini"
        ini_dst = exe_dir / "ColdClientLoader.ini"
        destinations.extend([
            (loader, exe_dir / loader.name),
            (steamclient, exe_dir / steamclient.name),
        ])
        if include_renderer:
            renderer = self.package.overlay_renderer(arch)
            destinations.append((renderer, exe_dir / renderer.name))
        if include_extra:
            extra = self.package.coldclient_extra(arch)
            destinations.append((extra, exe_dir / extra.name))

        deployed: list[Path] = []
        for source, destination in destinations:
            if backup_file:
                backup_file(destination)
            shutil.copy2(source, destination)
            deployed.append(destination)
            self.log(f"ColdClient: {source.name} -> {destination.name}")

        if backup_file:
            backup_file(ini_dst)
        if ini_src.is_file():
            text = ini_src.read_text(encoding="utf-8", errors="replace")
        else:
            text = "[SteamClient]\n"

        # Configure ColdClientLoader.ini with the target Exe and AppId as required by GSE ColdClient.
        exe_rel = main_exe.name if main_exe else "game.exe"
        if main_exe:
            try:
                exe_rel = str(main_exe.resolve().relative_to(exe_dir))
            except ValueError:
                exe_rel = main_exe.name

        parser = configparser.ConfigParser(interpolation=None, strict=False)
        parser.optionxform = str
        try:
            parser.read_string(text)
        except Exception:
            pass

        if not parser.has_section("SteamClient"):
            parser.add_section("SteamClient")
        parser.set("SteamClient", "AppId", str(int(appid)))
        parser.set("SteamClient", "Exe", exe_rel)
        parser.set("SteamClient", "SteamClientDll", "steamclient.dll")
        parser.set("SteamClient", "SteamClient64Dll", "steamclient64.dll")

        if not parser.has_section("GSEAutoSetup"):
            parser.add_section("GSEAutoSetup")
        parser.set("GSEAutoSetup", "AppId", str(int(appid)))
        if main_exe:
            parser.set("GSEAutoSetup", "Exe", exe_rel)

        from io import StringIO
        out = StringIO()
        parser.write(out, space_around_delimiters=False)
        text = out.getvalue()

        ini_dst.write_text(text, encoding="utf-8", newline="\n")
        deployed.append(ini_dst)
        return deployed
