from __future__ import annotations

import os
import shutil
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator


def app_directory() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[2]


def runtime_resources_root() -> Path:
    frozen = getattr(sys, "_MEIPASS", None)
    if frozen:
        return Path(frozen) / "resources"
    return Path(__file__).resolve().parents[2] / "resources"


class ResourceManager:
    """Resolve portable component resources.

    Runtime priority is intentionally simple and deterministic:
    1. ``<app>/resources/updates/<component>``
    2. ``<runtime resources>/embedded/<component>``

    ``runtime_resources`` is the directory that contains the ``embedded``
    directory.  In a PyInstaller build this is ``sys._MEIPASS/resources``.
    """

    def __init__(
        self,
        app_dir: Path | None = None,
        runtime_resources: Path | None = None,
    ) -> None:
        self.app_dir = Path(app_dir) if app_dir is not None else app_directory()
        self.runtime_resources = (
            Path(runtime_resources) if runtime_resources is not None else runtime_resources_root()
        )
        self.updates_root = self.app_dir / "resources" / "updates"

    def update_root(self, component: str) -> Path:
        return self.updates_root / component

    def external_embedded_root(self, component: str) -> Path:
        """Baseline resource shipped visibly beside the executable."""
        return self.app_dir / "resources" / "embedded" / component

    def embedded_root(self, component: str) -> Path:
        """Baseline embedded inside the PyInstaller bundle (or source tree)."""
        return self.runtime_resources / "embedded" / component

    def component_candidates(self, component: str) -> list[Path]:
        """Return resource roots in runtime priority order.

        Portable updates win, then the visible baseline beside the EXE, then the
        one-file embedded fallback. Duplicate paths are removed for source runs.
        """
        raw = [
            self.update_root(component),
            self.external_embedded_root(component),
            self.embedded_root(component),
        ]
        result: list[Path] = []
        seen: set[str] = set()
        for path in raw:
            key = str(path.resolve(strict=False)).casefold()
            if key not in seen:
                seen.add(key)
                result.append(path)
        return result

    def component_root(self, component: str, *, require: bool = True) -> Path:
        for candidate in self.component_candidates(component):
            if candidate.is_dir() and any(candidate.iterdir()):
                return candidate
        fallback = self.embedded_root(component)
        if require:
            raise FileNotFoundError(
                f"Component '{component}' is unavailable in portable updates, "
                "external embedded resources, or the one-file embedded fallback."
            )
        return fallback

    def ensure_update_root(self, component: str) -> Path:
        root = self.update_root(component)
        root.mkdir(parents=True, exist_ok=True)
        return root

    @contextmanager
    def temp_dir(self, component: str) -> Iterator[Path]:
        base = self.updates_root / ".tmp"
        base.mkdir(parents=True, exist_ok=True)
        temp = Path(tempfile.mkdtemp(prefix=f"{component}-", dir=base))
        try:
            yield temp
        finally:
            shutil.rmtree(temp, ignore_errors=True)
            try:
                if base.is_dir() and not any(base.iterdir()):
                    base.rmdir()
            except OSError:
                pass

    def install_update_tree(self, component: str, source: Path) -> Path:
        """Atomically replace a component update tree with ``source``."""
        source = Path(source)
        if not source.is_dir():
            raise FileNotFoundError(source)
        target = self.update_root(component)
        target.parent.mkdir(parents=True, exist_ok=True)
        staged = target.with_name(target.name + ".new")
        old = target.with_name(target.name + ".old")
        shutil.rmtree(staged, ignore_errors=True)
        shutil.rmtree(old, ignore_errors=True)
        shutil.copytree(source, staged)
        if target.exists():
            target.replace(old)
        staged.replace(target)
        shutil.rmtree(old, ignore_errors=True)
        return target

    def clear_update(self, component: str) -> None:
        shutil.rmtree(self.update_root(component), ignore_errors=True)

    def clear_temp(self) -> None:
        shutil.rmtree(self.updates_root / ".tmp", ignore_errors=True)
