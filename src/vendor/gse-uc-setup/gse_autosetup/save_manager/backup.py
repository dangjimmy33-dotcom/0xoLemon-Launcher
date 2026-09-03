from __future__ import annotations

import hashlib
import json
import os
import shutil
import tempfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

from .models import RestoreResult

MANIFEST_NAME = ".gse-save-manifest.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class SaveBackupManager:
    def __init__(self, backup_root: Path):
        self.backup_root = Path(backup_root)

    @staticmethod
    def _timestamp() -> str:
        return datetime.now().strftime("%Y-%m-%d_%H-%M-%S")

    def _appid_root(self, appid: int) -> Path:
        root = self.backup_root / str(int(appid))
        root.mkdir(parents=True, exist_ok=True)
        return root

    def create_backup(
        self,
        appid: int,
        source_folder: Path,
        game_name: str,
        *,
        prefix: str = "",
    ) -> Path:
        appid = int(appid)
        source_folder = Path(source_folder)
        if appid <= 0 or not source_folder.is_dir():
            raise ValueError("A valid AppID save folder is required.")

        output_dir = self._appid_root(appid)
        stem = f"{prefix}{self._timestamp()}"
        archive = output_dir / f"{stem}.zip"
        part = archive.with_suffix(".zip.part")
        file_count = 0
        total_bytes = 0

        manifest = {
            "format_version": 1,
            "appid": appid,
            "game_name": str(game_name or f"Steam App {appid}"),
            "source_root": str(source_folder.parent),
            "source_folder": str(source_folder),
            "created_at": datetime.now(timezone.utc).astimezone().isoformat(),
            "file_count": 0,
            "total_uncompressed_bytes": 0,
        }

        try:
            with zipfile.ZipFile(part, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as z:
                for path in sorted(source_folder.rglob("*")):
                    if not path.is_file():
                        continue
                    rel = path.relative_to(source_folder)
                    z.write(path, PurePosixPath(str(appid)) / PurePosixPath(rel.as_posix()))
                    file_count += 1
                    try:
                        total_bytes += path.stat().st_size
                    except OSError:
                        pass
                manifest["file_count"] = file_count
                manifest["total_uncompressed_bytes"] = total_bytes
                z.writestr(MANIFEST_NAME, json.dumps(manifest, ensure_ascii=False, indent=2))

            with zipfile.ZipFile(part, "r") as z:
                bad = z.testzip()
                if bad:
                    raise RuntimeError(f"Backup archive validation failed at {bad}.")
                if MANIFEST_NAME not in z.namelist():
                    raise RuntimeError("Backup manifest is missing.")
            part.replace(archive)
            meta = archive.with_suffix(".json")
            meta.write_text(
                json.dumps({"sha256": sha256_file(archive), **manifest}, ensure_ascii=False, indent=2),
                encoding="utf-8",
            )
            return archive
        except Exception:
            part.unlink(missing_ok=True)
            raise

    def list_backups(self, appid: int) -> list[Path]:
        root = self.backup_root / str(int(appid))
        if not root.is_dir():
            return []
        return sorted(root.glob("*.zip"), key=lambda p: p.stat().st_mtime, reverse=True)

    def read_manifest(self, archive: Path) -> dict:
        archive = Path(archive)
        with zipfile.ZipFile(archive, "r") as z:
            bad = z.testzip()
            if bad:
                raise RuntimeError(f"Backup archive is corrupt at {bad}.")
            try:
                manifest = json.loads(z.read(MANIFEST_NAME).decode("utf-8"))
            except KeyError as exc:
                raise RuntimeError("Backup manifest is missing.") from exc
        if int(manifest.get("appid", 0) or 0) <= 0:
            raise RuntimeError("Backup manifest has an invalid AppID.")
        return manifest

    @staticmethod
    def _safe_members(z: zipfile.ZipFile) -> list[zipfile.ZipInfo]:
        safe: list[zipfile.ZipInfo] = []
        for member in z.infolist():
            p = PurePosixPath(member.filename)
            if p.is_absolute() or ".." in p.parts:
                raise RuntimeError("Backup contains an unsafe path.")
            safe.append(member)
        return safe

    def restore_backup(self, archive: Path, target_root: Path, *, expected_appid: int | None = None) -> RestoreResult:
        archive = Path(archive)
        target_root = Path(target_root)
        manifest = self.read_manifest(archive)
        appid = int(manifest["appid"])
        if expected_appid is not None and int(expected_appid) != appid:
            raise ValueError("The selected backup belongs to a different AppID.")

        target_root.mkdir(parents=True, exist_ok=True)
        live = target_root / str(appid)
        safety: Path | None = None
        if live.is_dir():
            safety = self.create_backup(appid, live, str(manifest.get("game_name") or f"Steam App {appid}"), prefix="safety_")

        temp_parent = Path(tempfile.mkdtemp(prefix=f".gse_restore_{appid}_", dir=target_root))
        old = target_root / f".{appid}.restore-old"
        try:
            with zipfile.ZipFile(archive, "r") as z:
                members = self._safe_members(z)
                z.extractall(temp_parent, members=members)
            extracted = temp_parent / str(appid)
            if not extracted.is_dir():
                raise RuntimeError("Backup does not contain the expected AppID folder.")

            if old.exists():
                shutil.rmtree(old, ignore_errors=True)
            if live.exists():
                live.replace(old)
            extracted.replace(live)
            shutil.rmtree(old, ignore_errors=True)
            return RestoreResult(appid=appid, restored_folder=live, safety_backup=safety)
        except Exception:
            if live.exists() and old.exists():
                shutil.rmtree(live, ignore_errors=True)
                old.replace(live)
            elif old.exists() and not live.exists():
                old.replace(live)
            raise
        finally:
            shutil.rmtree(temp_parent, ignore_errors=True)
