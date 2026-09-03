from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from ..core.steam_api import SteamApiClient
from .models import SteamGameInfo


class SteamMetadataCache:
    def __init__(self, metadata_path: Path, covers_dir: Path):
        self.metadata_path = Path(metadata_path)
        self.covers_dir = Path(covers_dir)

    def _load(self) -> dict[str, dict[str, Any]]:
        try:
            data = json.loads(self.metadata_path.read_text(encoding="utf-8"))
            return data if isinstance(data, dict) else {}
        except Exception:
            return {}

    def _save(self, data: dict[str, dict[str, Any]]) -> None:
        self.metadata_path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.metadata_path.with_suffix(self.metadata_path.suffix + ".tmp")
        tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
        tmp.replace(self.metadata_path)

    def _cover_path(self, appid: int) -> Path:
        return self.covers_dir / f"{int(appid)}.jpg"

    def resolve(
        self,
        appid: int,
        *,
        client: SteamApiClient | None = None,
        download_art: bool = True,
    ) -> SteamGameInfo:
        appid = int(appid)
        data = self._load()
        cached = data.get(str(appid)) or {}
        cover = self._cover_path(appid)
        if cached.get("name"):
            header = str(cached.get("header_image") or "") or None
            if download_art and header and not cover.is_file():
                client = client or SteamApiClient("")
                self.covers_dir.mkdir(parents=True, exist_ok=True)
                client.download_file(header, cover)
            return SteamGameInfo(
                appid=appid,
                name=str(cached["name"]),
                header_image=header,
                cover_path=cover if cover.is_file() else None,
            )

        client = client or SteamApiClient("")
        try:
            metadata = client.get_store_metadata(appid)
            record = {
                "name": metadata.name,
                "header_image": metadata.header_image or "",
            }
            data[str(appid)] = record
            self._save(data)
            if download_art and metadata.header_image and not cover.is_file():
                self.covers_dir.mkdir(parents=True, exist_ok=True)
                client.download_file(metadata.header_image, cover)
            return SteamGameInfo(
                appid=appid,
                name=metadata.name,
                header_image=metadata.header_image,
                cover_path=cover if cover.is_file() else None,
            )
        except Exception:
            return SteamGameInfo(appid=appid, name=f"Steam App {appid}")
