from __future__ import annotations

import re
from pathlib import Path
from typing import Callable

ROOT_FOLDER_NAME = "GSE Save Backups"
FOLDER_MIME = "application/vnd.google-apps.folder"


def _escape_query(value: str) -> str:
    return str(value).replace("\\", "\\\\").replace("'", "\\'")


def _safe_game_name(value: str) -> str:
    clean = re.sub(r'[<>:"/\\|?*]+', "_", str(value or "")).strip().rstrip(".")
    return clean[:100] or "Steam Game"


class GoogleDriveBackupService:
    def __init__(self, *, credentials=None, service=None, media_upload_cls=None):
        if service is None:
            try:
                from googleapiclient.discovery import build
            except ImportError as exc:
                raise RuntimeError("google-api-python-client is missing. Rebuild after installing requirements.txt.") from exc
            service = build("drive", "v3", credentials=credentials, cache_discovery=False)
        self.service = service
        if media_upload_cls is None:
            from googleapiclient.http import MediaFileUpload
            media_upload_cls = MediaFileUpload
        self.media_upload_cls = media_upload_cls

    def _find_folder(self, name: str, parent: str | None = None) -> str | None:
        clauses = [
            f"name = '{_escape_query(name)}'",
            f"mimeType = '{FOLDER_MIME}'",
            "trashed = false",
        ]
        if parent:
            clauses.append(f"'{_escape_query(parent)}' in parents")
        result = self.service.files().list(
            q=" and ".join(clauses), spaces="drive", fields="files(id,name)", pageSize=20
        ).execute()
        files = result.get("files", [])
        return str(files[0]["id"]) if files else None

    def _create_folder(self, name: str, parent: str | None = None, app_properties: dict | None = None) -> str:
        body = {"name": name, "mimeType": FOLDER_MIME}
        if parent:
            body["parents"] = [parent]
        if app_properties:
            body["appProperties"] = app_properties
        result = self.service.files().create(body=body, fields="id").execute()
        return str(result["id"])

    def ensure_root_folder(self) -> str:
        return self._find_folder(ROOT_FOLDER_NAME) or self._create_folder(
            ROOT_FOLDER_NAME, app_properties={"gse_save_manager": "1"}
        )

    def ensure_game_folder(self, appid: int, game_name: str) -> str:
        root = self.ensure_root_folder()
        name = f"{int(appid)} - {_safe_game_name(game_name)}"
        return self._find_folder(name, root) or self._create_folder(
            name, root, {"gse_appid": str(int(appid)), "gse_save_manager": "1"}
        )

    def account_info(self) -> dict:
        return self.service.about().get(fields="user(displayName,emailAddress,photoLink),storageQuota").execute()

    def upload_backup(
        self,
        archive: Path,
        appid: int,
        game_name: str,
        *,
        sha256: str = "",
        progress: Callable[[int], None] | None = None,
    ) -> dict:
        archive = Path(archive)
        folder = self.ensure_game_folder(appid, game_name)
        media = self.media_upload_cls(
            str(archive), mimetype="application/zip", resumable=True, chunksize=8 * 1024 * 1024
        )
        body = {
            "name": archive.name,
            "parents": [folder],
            "appProperties": {
                "gse_save_manager": "1",
                "gse_appid": str(int(appid)),
                "gse_sha256": str(sha256 or ""),
            },
        }
        request = self.service.files().create(body=body, media_body=media, fields="id,name,size,createdTime")
        response = None
        while response is None:
            status, response = request.next_chunk(num_retries=4)
            if status is not None and progress:
                try:
                    progress(max(0, min(100, int(status.progress() * 100))))
                except Exception:
                    pass
        if progress:
            progress(100)
        return response

    def list_backups(self, appid: int | None = None) -> list[dict]:
        clauses = [
            "trashed = false",
            f"mimeType != '{FOLDER_MIME}'",
            "appProperties has { key='gse_save_manager' and value='1' }",
        ]
        if appid is not None:
            clauses.append(f"appProperties has {{ key='gse_appid' and value='{int(appid)}' }}")
        result = self.service.files().list(
            q=" and ".join(clauses),
            spaces="drive",
            fields="files(id,name,size,createdTime,modifiedTime,appProperties,parents)",
            orderBy="createdTime desc",
            pageSize=200,
        ).execute()
        return list(result.get("files", []))

    def download_backup(self, file_id: str, destination: Path, progress: Callable[[int], None] | None = None) -> Path:
        try:
            from googleapiclient.http import MediaIoBaseDownload
        except ImportError as exc:
            raise RuntimeError("google-api-python-client is missing.") from exc
        destination = Path(destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        request = self.service.files().get_media(fileId=str(file_id))
        with destination.open("wb") as f:
            downloader = MediaIoBaseDownload(f, request, chunksize=8 * 1024 * 1024)
            done = False
            while not done:
                status, done = downloader.next_chunk(num_retries=4)
                if status is not None and progress:
                    progress(max(0, min(100, int(status.progress() * 100))))
        if progress:
            progress(100)
        return destination
