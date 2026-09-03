from __future__ import annotations

import json
import os
import re
from pathlib import Path


def app_data_root() -> Path:
    base = os.environ.get("LOCALAPPDATA")
    if base:
        return Path(base) / "GSE Auto Setup"
    return Path.home() / ".gse-auto-setup"


def _safe(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._") or "unknown"


class CacheManager:
    """Tiny compatibility metadata store.

    V1.8 deliberately stops storing release archives, extracted packages, or
    game backups in LocalAppData.  Large resources live beside the executable
    through :class:`ResourceManager`.  Only the optional state JSON remains.
    """

    def __init__(self, root: Path | None = None):
        self.root = Path(root) if root else app_data_root()
        self.state_path = self.root / "state.json"
        # Kept as attributes for older callers, but never created automatically.
        self.downloads = self.root / "downloads"
        self.extracted = self.root / "packages"
        # Clean legacy heavy caches owned by old tool versions.
        import shutil
        for legacy in (self.root / "backups", self.downloads, self.extracted, self.root / "components"):
            if legacy.is_dir():
                shutil.rmtree(legacy, ignore_errors=True)

    def archive_path(self, tag: str, asset_name: str) -> Path:
        return self.downloads / f"{_safe(tag)}__{Path(asset_name).name}"

    def package_dir(self, tag: str) -> Path:
        return self.extracted / _safe(tag)

    def load_state(self) -> dict:
        try:
            return json.loads(self.state_path.read_text(encoding="utf-8"))
        except Exception:
            return {}

    def save_state(self, state: dict) -> None:
        self.root.mkdir(parents=True, exist_ok=True)
        tmp = self.state_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(state, ensure_ascii=False, indent=2), encoding="utf-8")
        tmp.replace(self.state_path)
