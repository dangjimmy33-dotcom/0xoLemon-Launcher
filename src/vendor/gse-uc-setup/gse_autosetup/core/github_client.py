
from __future__ import annotations

from pathlib import Path
from typing import Callable, Iterable

import requests

from .models import ReleaseAsset, ReleaseInfo

GSE_REPO = "alex47exe/gse_fork"


def select_windows_release_asset(assets: Iterable[dict]) -> ReleaseAsset:
    candidates: list[tuple[int, ReleaseAsset]] = []
    for raw in assets:
        name = str(raw.get("name") or "")
        low = name.lower()
        url = str(raw.get("browser_download_url") or "")
        if not name or not url:
            continue
        if not (low.endswith(".7z") or low.endswith(".zip")):
            continue

        score = 0
        if "win" in low or "windows" in low:
            score += 100
        if "release" in low:
            score += 30
        if "emu" in low or "gse" in low:
            score += 20
        if low.endswith(".7z"):
            score += 5
        if any(token in low for token in ("debug", "symbols", "pdb", "source", "src")):
            score -= 200
        if "linux" in low or "ubuntu" in low:
            score -= 200

        candidates.append((score, ReleaseAsset(name, url, int(raw.get("size") or 0))))

    if not candidates:
        raise RuntimeError("No Windows GSE release archive was found in the latest GitHub release.")

    candidates.sort(key=lambda item: (item[0], item[1].name.lower()), reverse=True)
    best_score, best = candidates[0]
    if best_score < 50:
        raise RuntimeError("GitHub release assets exist, but no unambiguous Windows release archive was found.")
    return best


class GitHubClient:
    def __init__(self, timeout: int = 20, repo: str = GSE_REPO):
        self.timeout = timeout
        self.repo = repo
        self.latest_release_url = f"https://api.github.com/repos/{repo}/releases/latest"
        self.session = requests.Session()
        self.session.headers.update({
            "Accept": "application/vnd.github+json",
            "User-Agent": "GSE-Auto-Setup/1.6",
        })

    def latest_release(self) -> ReleaseInfo:
        response = self.session.get(self.latest_release_url, timeout=self.timeout)
        response.raise_for_status()
        data = response.json()
        asset = select_windows_release_asset(data.get("assets", []))
        return ReleaseInfo(
            tag=str(data.get("tag_name") or data.get("name") or "unknown"),
            name=str(data.get("name") or data.get("tag_name") or "GSE release"),
            published_at=str(data.get("published_at") or ""),
            asset=asset,
        )

    def download_asset(
        self,
        asset: ReleaseAsset,
        destination: Path,
        progress: Callable[[int, int], None] | None = None,
    ) -> Path:
        destination.parent.mkdir(parents=True, exist_ok=True)
        tmp = destination.with_suffix(destination.suffix + ".part")
        with self.session.get(asset.download_url, stream=True, timeout=(15, 120)) as response:
            response.raise_for_status()
            total = int(response.headers.get("Content-Length") or asset.size or 0)
            done = 0
            with tmp.open("wb") as f:
                for chunk in response.iter_content(chunk_size=1024 * 512):
                    if not chunk:
                        continue
                    f.write(chunk)
                    done += len(chunk)
                    if progress:
                        progress(done, total)
        tmp.replace(destination)
        return destination
