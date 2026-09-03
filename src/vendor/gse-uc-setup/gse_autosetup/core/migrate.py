from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Callable

from .resources import ResourceManager


class MigrateGSEManager:
    def __init__(
        self,
        resources: ResourceManager | None = None,
        log: Callable[[str], None] | None = None,
    ) -> None:
        self.resources = resources or ResourceManager()
        self.log = log or (lambda _m: None)

    def executable(self) -> Path:
        root = self.resources.component_root("migrate_gse")
        direct = root / "migrate_gse.exe"
        if direct.is_file():
            return direct
        matches = list(root.rglob("migrate_gse.exe"))
        if not matches:
            raise FileNotFoundError("migrate_gse.exe is missing from embedded/updated resources.")
        return matches[0]

    def launch(self, *, cwd: Path | None = None) -> subprocess.Popen:
        exe = self.executable()
        self.log(f"Launching GSE migration utility: {exe}")
        return subprocess.Popen([str(exe)], cwd=str(cwd or exe.parent))
