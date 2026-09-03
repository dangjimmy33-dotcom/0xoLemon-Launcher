from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Optional


@dataclass(frozen=True)
class SaveEntry:
    appid: int
    source_root: Path
    save_folder: Path
    size_bytes: int
    modified_at: datetime
    game_name: str = ""
    header_image_url: Optional[str] = None
    cover_path: Optional[Path] = None
    local_backup_count: int = 0
    cloud_backup_count: int = 0


@dataclass(frozen=True)
class SteamGameInfo:
    appid: int
    name: str
    header_image: Optional[str] = None
    cover_path: Optional[Path] = None


@dataclass(frozen=True)
class RestoreResult:
    appid: int
    restored_folder: Path
    safety_backup: Optional[Path]
