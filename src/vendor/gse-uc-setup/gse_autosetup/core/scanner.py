
from __future__ import annotations

import os
from pathlib import Path

from .models import SteamApiTarget

_EXCLUDED_DIR_NAMES = {
    "_commonredist", "commonredist", "redistributables", "redist",
    "backup", "backups", ".gseautosetup", "gse_backup", "gse backups",
    "cache", "__pycache__", "steamworks shared",
}


def detect_pe_architecture(path: Path) -> str:
    with Path(path).open("rb") as f:
        mz = f.read(64)
        if len(mz) < 64 or mz[:2] != b"MZ":
            raise ValueError(f"Not a PE file: {path}")
        pe_offset = int.from_bytes(mz[0x3C:0x40], "little")
        f.seek(pe_offset)
        if f.read(4) != b"PE\0\0":
            raise ValueError(f"Invalid PE signature: {path}")
        machine = int.from_bytes(f.read(2), "little")
    if machine == 0x14C:
        return "x86"
    if machine == 0x8664:
        return "x64"
    raise ValueError(f"Unsupported PE machine 0x{machine:04X}: {path}")


def _is_excluded(path: Path) -> bool:
    return any(part.lower() in _EXCLUDED_DIR_NAMES for part in path.parts)


def scan_steam_api_targets(game_root: Path) -> list[SteamApiTarget]:
    root = Path(game_root).resolve()
    if not root.is_dir():
        raise ValueError("Game folder does not exist.")

    found: list[SteamApiTarget] = []
    for current, dirs, filenames in os.walk(root):
        current_path = Path(current)
        dirs[:] = [d for d in dirs if d.lower() not in _EXCLUDED_DIR_NAMES and not d.startswith(".")]
        if _is_excluded(current_path.relative_to(root)):
            continue
        lowered = {name.lower(): name for name in filenames}
        for wanted in ("steam_api.dll", "steam_api64.dll"):
            real = lowered.get(wanted)
            if not real:
                continue
            path = current_path / real
            try:
                arch = detect_pe_architecture(path)
            except (OSError, ValueError):
                continue
            expected = "x64" if wanted.endswith("64.dll") else "x86"
            if arch != expected:
                continue
            found.append(SteamApiTarget(path.resolve(), arch))

    found.sort(key=lambda t: (len(t.path.relative_to(root).parts), str(t.path).lower()))
    return found
