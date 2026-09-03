from __future__ import annotations

import os
import shutil
import subprocess
import sys
import zipfile
from dataclasses import dataclass
from pathlib import Path

from .resources import ResourceManager


@dataclass(frozen=True)
class GSEPackage:
    root: Path
    version: str

    def dll(self, arch: str, experimental: bool = False) -> Path:
        group = "experimental" if experimental else "regular"
        name = "steam_api64.dll" if arch == "x64" else "steam_api.dll"
        path = self.root / group / arch / name
        if not path.is_file():
            raise FileNotFoundError(f"Missing GSE {group} {arch} DLL: {path}")
        return path

    def generator(self, arch: str) -> Path:
        name = "generate_interfaces_x64.exe" if arch == "x64" else "generate_interfaces_x86.exe"
        path = self.root / "tools" / "generate_interfaces" / name
        if not path.is_file():
            raise FileNotFoundError(f"Missing GSE generate_interfaces tool: {path}")
        return path

    def steamclient(self, arch: str) -> Path:
        name = "steamclient64.dll" if arch == "x64" else "steamclient.dll"
        candidates = [
            self.root / "steamclient_experimental" / name,
            self.root / "experimental" / arch / name,
        ]
        for path in candidates:
            if path.is_file():
                return path
        raise FileNotFoundError(f"Missing GSE experimental {name} required for preserve-original mode.")

    def coldclient_root(self) -> Path:
        root = self.root / "steamclient_experimental"
        if not root.is_dir():
            raise FileNotFoundError(f"Missing GSE steamclient_experimental folder: {root}")
        return root

    def coldclient_loader(self, arch: str) -> Path:
        name = "steamclient_loader_x64.exe" if arch == "x64" else "steamclient_loader_x86.exe"
        path = self.coldclient_root() / name
        if not path.is_file():
            raise FileNotFoundError(f"Missing GSE ColdClient loader: {path}")
        return path

    def overlay_renderer(self, arch: str) -> Path:
        name = "GameOverlayRenderer64.dll" if arch == "x64" else "GameOverlayRenderer.dll"
        path = self.coldclient_root() / name
        if not path.is_file():
            raise FileNotFoundError(f"Missing GSE overlay renderer stub: {path}")
        return path

    def coldclient_extra(self, arch: str) -> Path:
        name = "steamclient_extra_x64.dll" if arch == "x64" else "steamclient_extra_x86.dll"
        path = self.coldclient_root() / "extra_dlls" / name
        if not path.is_file():
            raise FileNotFoundError(f"Missing GSE ColdClient extra DLL: {path}")
        return path


def _runtime_root() -> Path:
    frozen = getattr(sys, "_MEIPASS", None)
    if frozen:
        return Path(frozen)
    return Path(__file__).resolve().parents[2]


def resource_path(*parts: str) -> Path:
    return _runtime_root().joinpath("resources", *parts)


def find_native_7zip() -> Path:
    """Return the bundled native 7-Zip command line executable.

    V1.7 deliberately uses native 7-Zip rather than py7zr because official
    GSE release archives can contain BCJ2-filtered streams, which py7zr cannot
    decode with Python's stdlib lzma backend.
    """
    bundled = resource_path("7zip", "7za.exe")
    if bundled.is_file():
        return bundled

    for name in ("7z.exe", "7za.exe", "7zr.exe", "7z", "7za", "7zr"):
        found = shutil.which(name)
        if found:
            return Path(found)

    raise RuntimeError(
        "Native 7-Zip extractor is missing. Rebuild GSE Auto Setup V1.7 so "
        "the bundled resources/7zip/7za.exe is included."
    )


def _extract_7z_native(archive: Path, destination: Path) -> None:
    tool = find_native_7zip()
    destination.mkdir(parents=True, exist_ok=True)
    process = subprocess.run(
        [
            str(tool),
            "x",
            "-y",
            "-aoa",
            f"-o{destination}",
            str(archive),
        ],
        cwd=destination,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=300,
        creationflags=(subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0),
    )
    if process.returncode != 0:
        detail = (process.stderr or process.stdout or "").strip()
        if len(detail) > 1000:
            detail = detail[-1000:]
        raise RuntimeError(
            f"7-Zip extraction failed (exit {process.returncode})"
            + (f": {detail}" if detail else ".")
        )


def extract_archive(archive: Path, destination: Path) -> Path:
    archive = Path(archive)
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    low = archive.name.lower()
    if low.endswith(".zip"):
        with zipfile.ZipFile(archive) as zf:
            zf.extractall(destination)
    elif low.endswith(".7z"):
        _extract_7z_native(archive, destination)
    else:
        raise RuntimeError(f"Unsupported archive: {archive.name}")
    return destination


def verify_package_root(root: Path) -> Path:
    root = Path(root)
    required = [
        root / "regular" / "x64" / "steam_api64.dll",
        root / "regular" / "x86" / "steam_api.dll",
        root / "tools" / "generate_interfaces" / "generate_interfaces_x64.exe",
        root / "tools" / "generate_interfaces" / "generate_interfaces_x86.exe",
    ]
    missing = [p for p in required if not p.is_file()]
    if missing:
        short = ", ".join(str(p.relative_to(root)) if root in p.parents else str(p) for p in missing)
        raise RuntimeError(f"GSE package is incomplete; missing: {short}")
    return root


def locate_package_root(extract_root: Path) -> Path:
    extract_root = Path(extract_root)
    if not extract_root.is_dir():
        raise RuntimeError(f"GSE package folder does not exist: {extract_root}")
    checks = [extract_root] + [p for p in extract_root.iterdir() if p.is_dir()]
    for candidate in checks:
        try:
            return verify_package_root(candidate)
        except Exception:
            pass
    for candidate in extract_root.rglob("steam_api64.dll"):
        if tuple(candidate.parts[-3:]) == ("regular", "x64", "steam_api64.dll"):
            root = candidate.parents[2]
            try:
                return verify_package_root(root)
            except Exception:
                pass
    raise RuntimeError("Extracted archive does not look like an official GSE Windows release package.")


def bundled_gse_root() -> Path:
    # V1.8 full seed. Keep the legacy gse_seed fallback for source archives
    # created by older versions of the tool.
    rm = ResourceManager()
    root = rm.embedded_root("gse")
    if root.is_dir():
        try:
            return verify_package_root(root)
        except Exception:
            pass
    return verify_package_root(resource_path("gse_seed"))


def extract_release(archive: Path, destination: Path) -> Path:
    archive = Path(archive)
    destination = Path(destination)
    marker = destination / ".complete"
    if marker.is_file():
        try:
            return locate_package_root(destination)
        except Exception:
            shutil.rmtree(destination, ignore_errors=True)

    # Avoid mixing a partial extraction from a previous failed run with a new one.
    if destination.exists():
        shutil.rmtree(destination, ignore_errors=True)
    destination.mkdir(parents=True, exist_ok=True)

    extract_archive(archive, destination)
    root = locate_package_root(destination)
    marker.write_text("ok", encoding="ascii")
    return root
