"""Savegame Manager subsystem for GSE / UC Setup."""

from .models import SaveEntry, SteamGameInfo, RestoreResult
from .scanner import default_gse_saves_root, scan_save_root
from .backup import SaveBackupManager

__all__ = [
    "SaveEntry",
    "SteamGameInfo",
    "RestoreResult",
    "default_gse_saves_root",
    "scan_save_root",
    "SaveBackupManager",
]
