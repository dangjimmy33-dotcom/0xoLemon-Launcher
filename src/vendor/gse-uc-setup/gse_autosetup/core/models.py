
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass(frozen=True)
class ReleaseAsset:
    name: str
    download_url: str
    size: int = 0


@dataclass(frozen=True)
class ReleaseInfo:
    tag: str
    name: str
    published_at: str
    asset: ReleaseAsset


@dataclass(frozen=True)
class SteamApiTarget:
    path: Path
    arch: str


@dataclass(frozen=True)
class GameMetadata:
    appid: int
    name: str
    header_image: Optional[str] = None


@dataclass
class SetupResult:
    appid: int
    game_name: str
    gse_version: str
    installed_targets: list[Path]
    backup_manifest: Path
    settings_dirs: list[Path]
