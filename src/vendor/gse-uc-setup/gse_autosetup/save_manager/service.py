from __future__ import annotations

from dataclasses import replace
from pathlib import Path
from typing import Callable

from ..core.tool_config import app_directory
from ..core.steam_api import SteamApiClient
from .backup import SaveBackupManager, sha256_file
from .google_auth import GoogleAuthManager
from .google_drive import GoogleDriveBackupService
from .models import SaveEntry
from .scanner import default_gse_saves_root, scan_save_root
from .steam_metadata import SteamMetadataCache


class SaveManagerService:
    def __init__(self, *, app_dir: Path | None = None):
        self.app_dir = Path(app_dir) if app_dir else app_directory()
        self.data_dir = self.app_dir / "data"
        self.backups = SaveBackupManager(self.data_dir / "save_backups")
        self.metadata = SteamMetadataCache(
            self.data_dir / "save_manager" / "metadata.json",
            self.data_dir / "save_manager" / "covers",
        )
        self.auth = GoogleAuthManager()

    def scan(self, root: Path | None = None, *, resolve_metadata: bool = True) -> list[SaveEntry]:
        root = Path(root) if root else default_gse_saves_root()
        entries = scan_save_root(root)
        if not resolve_metadata:
            return entries
        client = SteamApiClient("")
        enriched: list[SaveEntry] = []
        for entry in entries:
            info = self.metadata.resolve(entry.appid, client=client, download_art=True)
            enriched.append(
                replace(
                    entry,
                    game_name=info.name,
                    header_image_url=info.header_image,
                    cover_path=info.cover_path,
                    local_backup_count=len(self.backups.list_backups(entry.appid)),
                )
            )
        return enriched

    def backup(self, entry: SaveEntry) -> Path:
        return self.backups.create_backup(entry.appid, entry.save_folder, entry.game_name or f"Steam App {entry.appid}")

    def restore(self, archive: Path, target_root: Path, appid: int | None = None):
        return self.backups.restore_backup(archive, target_root, expected_appid=appid)

    def connect_drive(self) -> dict:
        creds = self.auth.connect()
        return GoogleDriveBackupService(credentials=creds).account_info()

    def drive_status(self) -> dict:
        if not self.auth.is_connected():
            return {"connected": False}
        try:
            creds = self.auth.get_credentials()
            info = GoogleDriveBackupService(credentials=creds).account_info()
            return {"connected": True, **info}
        except Exception as exc:
            return {"connected": False, "error": str(exc)}

    def disconnect_drive(self) -> None:
        self.auth.disconnect()

    def backup_to_drive(self, entry: SaveEntry, progress: Callable[[int], None] | None = None) -> tuple[Path, dict]:
        archive = self.backup(entry)
        creds = self.auth.get_credentials()
        service = GoogleDriveBackupService(credentials=creds)
        uploaded = service.upload_backup(
            archive,
            entry.appid,
            entry.game_name or f"Steam App {entry.appid}",
            sha256=sha256_file(archive),
            progress=progress,
        )
        return archive, uploaded

    def cloud_backups(self, appid: int | None = None) -> list[dict]:
        creds = self.auth.get_credentials()
        return GoogleDriveBackupService(credentials=creds).list_backups(appid)

    def restore_from_drive(
        self,
        file_id: str,
        file_name: str,
        target_root: Path,
        *,
        appid: int,
        progress: Callable[[int], None] | None = None,
    ):
        creds = self.auth.get_credentials()
        service = GoogleDriveBackupService(credentials=creds)
        tmp = self.data_dir / "save_manager" / "downloads" / file_name
        service.download_backup(file_id, tmp, progress=progress)
        return self.restore(tmp, target_root, appid=appid)
