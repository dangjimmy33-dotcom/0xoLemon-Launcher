from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Sequence

import requests

from .resources import ResourceManager

STEAMLESS_LATEST = "https://api.github.com/repos/atom0s/Steamless/releases/latest"


@dataclass(frozen=True)
class SteamlessRelease:
    tag: str
    url: str
    name: str
    sha256: str = ""


def parse_steamless_release(data: dict) -> SteamlessRelease:
    tag = str(data.get("tag_name") or data.get("name") or "latest")
    for raw in data.get("assets", []) or []:
        name = str(raw.get("name") or "")
        if not name.lower().endswith(".zip"):
            continue
        url = str(raw.get("browser_download_url") or "")
        if not url:
            continue
        digest = str(raw.get("digest") or "")
        if digest.lower().startswith("sha256:"):
            digest = digest.split(":", 1)[1]
        return SteamlessRelease(tag=tag, url=url, name=name, sha256=digest.lower())
    raise RuntimeError("Steamless latest release has no ZIP asset.")


def steamless_backup_path(exe_path: Path) -> Path:
    exe_path = Path(exe_path)
    return exe_path.with_name(exe_path.name + ".bak")


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


class SteamlessManager:
    def __init__(
        self,
        resources: ResourceManager | None = None,
        log: Callable[[str], None] | None = None,
        runner: Callable[[Sequence[str], Path], int] | None = None,
    ) -> None:
        self.resources = resources or ResourceManager()
        self.log = log or (lambda _m: None)
        self.runner = runner or self._default_runner
        self.session = requests.Session()
        self.session.headers.update({
            "Accept": "application/vnd.github+json",
            "User-Agent": "GSE-UC-Setup/1.8.3",
        })

    @staticmethod
    def _default_runner(cmd: Sequence[str], cwd: Path) -> int:
        process = subprocess.run(
            list(cmd), cwd=str(cwd), capture_output=True, text=True, timeout=300, check=False
        )
        return int(process.returncode)

    @staticmethod
    def _root_tag(root: Path, fallback: str = "embedded") -> str:
        try:
            return str(json.loads((root / "component.json").read_text(encoding="utf-8-sig")).get("tag") or fallback)
        except Exception:
            return fallback

    @staticmethod
    def _find_cli(root: Path) -> Path:
        direct = root / "Steamless.CLI.exe"
        if direct.is_file():
            return direct
        matches = list(root.rglob("Steamless.CLI.exe")) if root.is_dir() else []
        if not matches:
            raise FileNotFoundError("Embedded/updated Steamless.CLI.exe is missing.")
        return matches[0]

    def latest_release(self) -> SteamlessRelease:
        response = self.session.get(STEAMLESS_LATEST, timeout=20)
        response.raise_for_status()
        return parse_steamless_release(response.json())

    def ensure_component(self, *, check_updates: bool = True) -> tuple[Path, str]:
        current = self.resources.component_root("steamless", require=False)
        try:
            self._find_cli(current)
            local_ok = True
        except Exception:
            local_ok = False

        if local_ok and not check_updates:
            return current, self._root_tag(current)

        try:
            release = self.latest_release() if check_updates else None
        except Exception as exc:
            if local_ok:
                self.log(f"Steamless update check unavailable ({exc}); using local component.")
                return current, self._root_tag(current)
            raise

        if release is None:
            if local_ok:
                return current, self._root_tag(current)
            raise FileNotFoundError("Steamless runtime is unavailable.")

        update = self.resources.update_root("steamless")
        try:
            self._find_cli(update)
            if self._root_tag(update, "") == release.tag:
                return update, release.tag
        except Exception:
            pass

        embedded = self.resources.embedded_root("steamless")
        try:
            self._find_cli(embedded)
            if self._root_tag(embedded, "") == release.tag:
                return embedded, release.tag
        except Exception:
            pass

        with self.resources.temp_dir("steamless") as temp:
            archive = temp / release.name
            self.log(f"Downloading Steamless {release.tag} into portable resources...")
            with self.session.get(release.url, stream=True, timeout=(15, 120)) as response:
                response.raise_for_status()
                with archive.open("wb") as f:
                    for chunk in response.iter_content(1024 * 512):
                        if chunk:
                            f.write(chunk)
            if release.sha256 and len(release.sha256) == 64:
                actual = _sha256(archive)
                if actual.lower() != release.sha256.lower():
                    raise RuntimeError(
                        f"Steamless SHA-256 mismatch: expected {release.sha256}, got {actual}."
                    )
            unpack = temp / "unpack"
            unpack.mkdir()
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(unpack)
            cli = self._find_cli(unpack)
            package_root = cli.parent
            (package_root / "component.json").write_text(
                json.dumps({"source": "atom0s/Steamless", "tag": release.tag}, indent=2),
                encoding="utf-8",
            )
            installed = self.resources.install_update_tree("steamless", package_root)
            self._find_cli(installed)
            return installed, release.tag

    def cli(self, *, check_updates: bool = False) -> Path:
        root, _tag = self.ensure_component(check_updates=check_updates)
        return self._find_cli(root)

    def patch(
        self,
        exe_path: Path,
        *,
        options: Sequence[str] | None = None,
        backup_file: Callable[[Path], None] | None = None,
        check_updates: bool = False,
    ) -> Path:
        exe_path = Path(exe_path).resolve()
        if not exe_path.is_file():
            raise FileNotFoundError(exe_path)
        unpacked = exe_path.with_name(exe_path.name + ".unpacked.exe")
        unpacked.unlink(missing_ok=True)

        cmd = [str(self.cli(check_updates=check_updates))]
        if options:
            cmd.extend(str(x) for x in options)
        cmd.append(exe_path.name)
        self.log(f"Steamless: unpacking {exe_path.name}...")
        code = self.runner(cmd, exe_path.parent)
        if code != 0 or not unpacked.is_file():
            unpacked.unlink(missing_ok=True)
            raise RuntimeError(
                f"Steamless did not produce {unpacked.name} (exit {code}). Original EXE was not replaced."
            )

        backup = steamless_backup_path(exe_path)
        if not backup.exists():
            if backup_file:
                backup_file(backup)
            shutil.copy2(exe_path, backup)
            self.log(f"Protected original EXE: {backup.name}")
        else:
            self.log(f"Existing original EXE backup preserved: {backup.name}")

        if backup_file:
            backup_file(exe_path)
        unpacked.replace(exe_path)
        self.log(f"Steamless: promoted unpacked output to {exe_path.name}")
        return exe_path
