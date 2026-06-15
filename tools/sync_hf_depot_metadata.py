#!/usr/bin/env python
"""Download only depot metadata from Hugging Face.

This intentionally skips transport packs. The local builder only needs catalog
and manifests to reuse old chunks and continue numbering new packs safely.
"""

from __future__ import annotations

import argparse
import shutil
import sys
import os
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Sync HF depot metadata without packs")
    parser.add_argument("--repo", required=True, help="Repository id, for example CatManga/Cat-Manga")
    parser.add_argument("--repo-type", default="dataset", help="HF repo type")
    parser.add_argument("--prefix", required=True, help="Depot prefix inside the repo")
    parser.add_argument("--out", required=True, help="Local depot folder")
    return parser.parse_args()


def is_metadata_file(relative_path: str, prefix: str) -> bool:
    if not relative_path.startswith(f"{prefix}/"):
        return False
    inner = relative_path[len(prefix) + 1 :]
    if inner == "catalog.json":
        return True
    if inner.startswith("manifests/") and inner.endswith(".json"):
        return True
    if inner.startswith("versions/") and (
        inner.endswith("/manifest.json") or inner.endswith("/build-info.json")
    ):
        return True
    return False


def main() -> int:
    args = parse_args()
    try:
        from huggingface_hub import hf_hub_download, list_repo_files
    except Exception as exc:  # pragma: no cover - depends on local Python env
        print(f"Missing huggingface_hub package: {exc}", file=sys.stderr)
        return 2

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    token = os.environ.get("HF_TOKEN")
    files = list_repo_files(args.repo, repo_type=args.repo_type, token=token)
    metadata_files = sorted(path for path in files if is_metadata_file(path, args.prefix))
    if not metadata_files:
        print(f"No metadata found for prefix {args.prefix}; starting from empty local catalog.")
        return 0

    for remote_path in metadata_files:
        cached_path = hf_hub_download(
            repo_id=args.repo,
            repo_type=args.repo_type,
            filename=remote_path,
            token=token,
        )
        relative = remote_path[len(args.prefix) + 1 :]
        target = out_dir / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(cached_path, target)
        print(f"synced {remote_path} -> {target}")

    print(f"synced {len(metadata_files)} metadata file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
