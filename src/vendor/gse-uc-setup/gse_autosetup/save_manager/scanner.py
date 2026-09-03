from __future__ import annotations

import os
from datetime import datetime
from pathlib import Path

from .models import SaveEntry


def default_gse_saves_root() -> Path:
    appdata = os.environ.get("APPDATA")
    if appdata:
        return Path(appdata) / "GSE Saves"
    return Path.home() / "AppData" / "Roaming" / "GSE Saves"


def _folder_stats(folder: Path) -> tuple[int, float]:
    total = 0
    latest = folder.stat().st_mtime if folder.exists() else 0.0
    try:
        for path in folder.rglob("*"):
            try:
                stat = path.stat()
            except OSError:
                continue
            latest = max(latest, stat.st_mtime)
            if path.is_file():
                total += stat.st_size
    except OSError:
        pass
    return total, latest


def scan_save_root(root: Path) -> list[SaveEntry]:
    root = Path(root)
    if not root.is_dir():
        return []
    entries: list[SaveEntry] = []
    for child in root.iterdir():
        if not child.is_dir() or not child.name.isdecimal():
            continue
        appid = int(child.name)
        if appid <= 0:
            continue
        size, mtime = _folder_stats(child)
        entries.append(
            SaveEntry(
                appid=appid,
                source_root=root,
                save_folder=child,
                size_bytes=size,
                modified_at=datetime.fromtimestamp(mtime),
            )
        )
    entries.sort(key=lambda item: item.modified_at, reverse=True)
    return entries
