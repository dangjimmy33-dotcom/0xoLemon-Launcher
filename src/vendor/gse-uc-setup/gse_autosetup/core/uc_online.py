from __future__ import annotations

import configparser
import json
import shutil
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable

import requests

from .installer import ensure_adjacent_original_backup
from .models import SteamApiTarget
from .resources import ResourceManager

UC_REPO_LATEST = "https://api.github.com/repos/UnionCrax-Team/uc-online2/releases/latest"


@dataclass(frozen=True)
class UCOnlineRelease:
    tag: str
    url: str
    name: str
    sha256: str = ""


def parse_uc_release(data: dict) -> UCOnlineRelease:
    tag = str(data.get("tag_name") or data.get("name") or "latest")
    assets = data.get("assets") or []
    candidates = []
    for raw in assets:
        name = str(raw.get("name") or "")
        low = name.lower()
        if low.endswith(".zip") and "release" in low and "debug" not in low:
            candidates.append(raw)
    if not candidates:
        raise RuntimeError("UC Online2 latest release has no release ZIP asset.")
    raw = candidates[0]
    digest = str(raw.get("digest") or "")
    if digest.lower().startswith("sha256:"):
        digest = digest.split(":", 1)[1]
    return UCOnlineRelease(
        tag=tag,
        url=str(raw.get("browser_download_url") or ""),
        name=str(raw.get("name") or "uc-online2-release.zip"),
        sha256=digest.lower(),
    )


@dataclass(frozen=True)
class UCOnlinePackage:
    root: Path
    version: str

    def dll(self, arch: str) -> Path:
        name = "steam_api64.dll" if arch == "x64" else "steam_api.dll"
        candidates = [
            self.root / arch / name,
            self.root / ("x64" if arch == "x64" else "x86") / name,
            self.root / name,
        ]
        for path in candidates:
            if path.is_file():
                return path
        matches = list(self.root.rglob(name))
        if matches:
            # Prefer release layout directories named exactly x64/x86.
            wanted_dir = "x64" if arch == "x64" else "x86"
            matches.sort(key=lambda p: 0 if p.parent.name.lower() == wanted_dir else 1)
            return matches[0]
        raise FileNotFoundError(f"UC Online2 package is missing {name}.")

    def plugin(self, key: str) -> Path:
        key = key.lower().strip()
        aliases = {
            "eos": ("eos",),
            "photon": ("photon",),
            "playfab": ("playfab",),
            "coherence": ("coherence",),
            "overlay": ("overlay", "steam_overlay"),
            "unity_auth": ("unity_auth",),
        }
        terms = aliases.get(key, (key,))
        dlls = list(self.root.rglob("*.dll"))
        for path in dlls:
            low = path.name.lower()
            if any(term in low for term in terms):
                return path
        raise FileNotFoundError(f"UC Online2 package has no built plugin for '{key}'.")


class UCOnlineResourceManager:
    def __init__(
        self,
        resources: ResourceManager | None = None,
        log: Callable[[str], None] | None = None,
    ) -> None:
        self.resources = resources or ResourceManager()
        self.log = log or (lambda _m: None)
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "GSE-UC-Setup/1.8.3", "Accept": "application/vnd.github+json"})

    def local_package(self) -> UCOnlinePackage | None:
        root = self.resources.component_root("uc_online", require=False)
        try:
            pkg = UCOnlinePackage(root, self._version(root, "embedded"))
            pkg.dll("x64")
            return pkg
        except Exception:
            return None

    @staticmethod
    def _version(root: Path, fallback: str) -> str:
        try:
            return str(json.loads((root / "component.json").read_text(encoding="utf-8-sig")).get("tag") or fallback)
        except Exception:
            return fallback

    def latest_release(self) -> UCOnlineRelease:
        response = self.session.get(UC_REPO_LATEST, timeout=20)
        response.raise_for_status()
        return parse_uc_release(response.json())

    def ensure_package(self, *, check_updates: bool = True) -> UCOnlinePackage:
        local = self.local_package()
        if not check_updates and local is not None:
            return local
        try:
            release = self.latest_release() if check_updates else None
        except Exception as exc:
            if local is not None:
                self.log(f"UC Online2 update check unavailable ({exc}); using {local.version}.")
                return local
            raise
        if release is None:
            if local is not None:
                return local
            raise FileNotFoundError("UC Online2 runtime is not embedded and no portable update is installed.")

        update_root = self.resources.update_root("uc_online")
        if update_root.is_dir() and self._version(update_root, "") == release.tag:
            pkg = UCOnlinePackage(update_root, release.tag)
            pkg.dll("x64")
            return pkg
        if local is not None and local.version == release.tag:
            return local

        with self.resources.temp_dir("uc_online") as temp:
            archive = temp / release.name
            self.log(f"Downloading UC Online2 {release.tag}...")
            with self.session.get(release.url, stream=True, timeout=(15, 120)) as response:
                response.raise_for_status()
                with archive.open("wb") as f:
                    for chunk in response.iter_content(1024 * 512):
                        if chunk:
                            f.write(chunk)
            if release.sha256:
                import hashlib
                h = hashlib.sha256(archive.read_bytes()).hexdigest()
                if h.lower() != release.sha256.lower():
                    raise RuntimeError("UC Online2 release SHA-256 mismatch.")
            unpack = temp / "unpack"
            unpack.mkdir()
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(unpack)
            candidates = [unpack] + [p for p in unpack.iterdir() if p.is_dir()]
            package_root = None
            for candidate in candidates:
                try:
                    UCOnlinePackage(candidate, release.tag).dll("x64")
                    package_root = candidate
                    break
                except Exception:
                    continue
            if package_root is None:
                raise RuntimeError("Downloaded UC Online2 release does not contain steam_api64.dll.")
            (package_root / "component.json").write_text(json.dumps({"tag": release.tag}, indent=2), encoding="utf-8")
            installed = self.resources.install_update_tree("uc_online", package_root)
            return UCOnlinePackage(installed, release.tag)


def detect_uc_backends(game_root: Path) -> set[str]:
    root = Path(game_root)
    found: set[str] = set()
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        low = path.name.lower()
        if "eossdk" in low or "onlinesubsystemeos" in low:
            found.add("eos")
        if "photon" in low:
            found.add("photon")
        if "playfab" in low:
            found.add("playfab")
        if "coherence" in low:
            found.add("coherence")
    return found


def _write_ini(path: Path, appid: int, spoof_appid: int, runtime_steamstub: bool, plugins: bool) -> None:
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    parser["Settings"] = {
        "AppId": str(int(spoof_appid)),
        "ogAppId": str(int(appid)),
        "PluginsFolder": "plugins" if plugins else "",
        "GetStubbedLol": "true" if runtime_steamstub else "false",
        "EmulateTicket": "true",
    }
    with path.open("w", encoding="utf-8", newline="\n") as f:
        parser.write(f, space_around_delimiters=False)


class UCOnlineInstaller:
    def __init__(self, package: UCOnlinePackage, log: Callable[[str], None] | None = None) -> None:
        self.package = package
        self.log = log or (lambda _m: None)

    def install(
        self,
        game_root: Path,
        targets: Iterable[SteamApiTarget],
        main_exe: Path,
        *,
        appid: int,
        spoof_appid: int = 480,
        plugins: Iterable[str] = (),
        runtime_steamstub: bool = False,
        backup_file: Callable[[Path], None] | None = None,
    ) -> list[Path]:
        game_root = Path(game_root).resolve()
        main_exe = Path(main_exe).resolve()
        deployed: list[Path] = []
        for target in targets:
            target_path = Path(target.path).resolve()
            backup = ensure_adjacent_original_backup(target_path, self.log)
            if backup_file:
                backup_file(target_path)
                backup_file(backup)
            shutil.copy2(self.package.dll(target.arch), target_path)
            deployed.append(target_path)
            self.log(f"UC Online2 {target.arch}: {target_path}")

        requested_plugins = list(dict.fromkeys(str(x).strip().lower() for x in plugins if str(x).strip()))
        plugins_dir = main_exe.parent / "plugins"
        if requested_plugins:
            if backup_file:
                backup_file(plugins_dir)
            plugins_dir.mkdir(parents=True, exist_ok=True)
            for key in requested_plugins:
                src = self.package.plugin(key)
                dst = plugins_dir / src.name
                shutil.copy2(src, dst)
                deployed.append(dst)
                self.log(f"UC plugin: {src.name}")

        ini = main_exe.parent / "union-crax.ini"
        if backup_file:
            backup_file(ini)
        _write_ini(ini, appid, spoof_appid, runtime_steamstub, bool(requested_plugins))
        deployed.append(ini)
        return deployed
