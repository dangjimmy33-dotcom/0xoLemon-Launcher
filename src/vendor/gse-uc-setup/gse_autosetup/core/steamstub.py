from __future__ import annotations

import hashlib
import json
import shutil
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import requests

from .resources import ResourceManager

RUNE_EMU_LATEST = "https://api.github.com/repos/Mush-iii/rune-emu/releases/latest"
ASSET_NAME = "steamstub.zip"


@dataclass(frozen=True)
class SteamStubRelease:
    tag: str
    url: str
    sha256: str
    size: int = 0


def parse_steamstub_release(data: dict) -> SteamStubRelease:
    tag = str(data.get("tag_name") or data.get("name") or "latest")
    for raw in data.get("assets", []) or []:
        if str(raw.get("name") or "").lower() != ASSET_NAME:
            continue
        url = str(raw.get("browser_download_url") or "")
        digest = str(raw.get("digest") or "")
        if digest.lower().startswith("sha256:"):
            digest = digest.split(":", 1)[1]
        if not url or len(digest) != 64:
            raise RuntimeError("RUNE SteamStub release does not expose a verifiable SHA-256 asset digest.")
        return SteamStubRelease(tag, url, digest.lower(), int(raw.get("size") or 0))
    raise RuntimeError("Latest RUNE component release does not contain steamstub.zip.")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


class SteamStubManager:
    def __init__(
        self,
        resources: ResourceManager | None = None,
        log: Callable[[str], None] | None = None,
    ):
        self.resources = resources or ResourceManager()
        self.log = log or (lambda _m: None)
        self.session = requests.Session()
        self.session.headers.update({
            "Accept": "application/vnd.github+json",
            "User-Agent": "GSE-UC-Setup/1.8.3",
        })

    def latest_release(self) -> SteamStubRelease:
        response = self.session.get(RUNE_EMU_LATEST, timeout=20)
        response.raise_for_status()
        return parse_steamstub_release(response.json())

    @staticmethod
    def _valid_root(root: Path) -> bool:
        return (root / "steamstub_x64.dll").is_file() and (root / "steamstub_x32.dll").is_file()

    @staticmethod
    def _root_tag(root: Path, fallback: str) -> str:
        info = root / "component.json"
        try:
            return str(json.loads(info.read_text(encoding="utf-8-sig")).get("tag") or fallback)
        except Exception:
            return fallback

    def ensure_component(self, *, check_updates: bool = True) -> tuple[Path, str]:
        current = self.resources.component_root("rune_steamstub", require=False)
        if self._valid_root(current) and not check_updates:
            kind = "update" if current == self.resources.update_root("rune_steamstub") else "embedded"
            return current, self._root_tag(current, kind)

        try:
            release = self.latest_release() if check_updates else None
        except Exception as exc:
            if self._valid_root(current):
                self.log(f"RUNE SteamStub update check unavailable ({exc}); using local component.")
                return current, self._root_tag(current, "embedded")
            raise

        if release is None:
            if self._valid_root(current):
                return current, self._root_tag(current, "embedded")
            raise FileNotFoundError("RUNE SteamStub component is unavailable.")

        update = self.resources.update_root("rune_steamstub")
        if self._valid_root(update) and self._root_tag(update, "") == release.tag:
            return update, release.tag

        # Embedded version can satisfy the requested tag without downloading.
        embedded = self.resources.embedded_root("rune_steamstub")
        if self._valid_root(embedded) and self._root_tag(embedded, "") == release.tag:
            return embedded, release.tag

        with self.resources.temp_dir("rune_steamstub") as temp:
            archive = temp / ASSET_NAME
            self.log(f"Downloading RUNE SteamStub component {release.tag}...")
            with self.session.get(release.url, stream=True, timeout=(15, 120)) as response:
                response.raise_for_status()
                with archive.open("wb") as f:
                    for chunk in response.iter_content(1024 * 512):
                        if chunk:
                            f.write(chunk)
            actual = sha256(archive)
            if actual.lower() != release.sha256:
                raise RuntimeError(
                    f"RUNE SteamStub SHA-256 mismatch: expected {release.sha256}, got {actual}."
                )
            unpack = temp / "unpacked"
            unpack.mkdir()
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(unpack)
            normalized = temp / "normalized"
            normalized.mkdir()
            for wanted in ("steamstub_x64.dll", "steamstub_x32.dll"):
                matches = list(unpack.rglob(wanted))
                if not matches:
                    raise RuntimeError(f"RUNE SteamStub package is missing {wanted}.")
                shutil.copy2(matches[0], normalized / wanted)
            (normalized / "component.json").write_text(
                json.dumps({"tag": release.tag, "sha256": release.sha256}, indent=2), encoding="utf-8"
            )
            installed = self.resources.install_update_tree("rune_steamstub", normalized)
            return installed, release.tag

    def deploy(
        self,
        exe_path: Path,
        arch: str,
        backup_file: Callable[[Path], None] | None = None,
        *,
        check_updates: bool = True,
    ) -> Path:
        exe_path = Path(exe_path).resolve()
        component_dir, tag = self.ensure_component(check_updates=check_updates)
        source_name = "steamstub_x64.dll" if arch == "x64" else "steamstub_x32.dll"
        source = component_dir / source_name
        destination = exe_path.parent / "winmm.dll"
        state_path = exe_path.parent / ".gse_steamstub.json"
        if destination.exists():
            if not state_path.is_file():
                raise RuntimeError(
                    f"SteamStub needs winmm.dll beside {exe_path.name}, but an unrelated winmm.dll already exists."
                )
            try:
                state = json.loads(state_path.read_text(encoding="utf-8"))
            except Exception:
                state = {}
            if str(state.get("sha256") or "").lower() != sha256(destination).lower():
                raise RuntimeError("Existing winmm.dll no longer matches the previously deployed SteamStub component.")
        if backup_file:
            backup_file(destination)
        shutil.copy2(source, destination)
        if backup_file:
            backup_file(state_path)
        state_path.write_text(json.dumps({
            "source": "Mush-iii/rune-emu",
            "tag": tag,
            "arch": arch,
            "sha256": sha256(destination),
        }, indent=2), encoding="utf-8")
        self.log(f"RUNE SteamStub: {source_name} -> {destination.name} ({tag})")
        return destination
