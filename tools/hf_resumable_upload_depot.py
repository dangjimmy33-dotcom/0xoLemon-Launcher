#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Incremental Hugging Face uploader for 0xo depot outputs.

This replaces the heavy staging/upload_large_folder path.

Why:
- Staging copied/hardlinked the whole depot into .hf_upload_stage and then hf-xet cached it again.
  With 60GB depots this can freeze Windows or fill the drive.
- The old Codex-style flow uploaded files one by one and deleted local packs after each successful
  upload. This script restores that behavior while keeping clean logs.

Behavior:
- Uploads packs first, metadata/catalog last.
- Does NOT create .hf_upload_stage.
- If --delete-local-packs is set, deletes each local packs/*.bin only after that file uploads OK.
- If a pack was already deleted locally but exists remotely, it is treated as already uploaded.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Iterable, List, Set, Tuple


SKIP_DIR_NAMES = {".git", ".hg", ".svn", "__pycache__", ".hf_upload_stage", ".cache", "cache"}
PACK_NAME_RE = re.compile(r"^pack[0-9A-Za-z_.\-]*$")


def log(msg: str) -> None:
    print(msg, flush=True)


def rel_posix(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def repo_path(prefix: str, rel: str) -> str:
    prefix = prefix.strip("/\\")
    rel = rel.replace("\\", "/").lstrip("/")
    return f"{prefix}/{rel}" if prefix else rel


def should_skip(path: Path) -> bool:
    parts = set(path.parts)
    if parts & SKIP_DIR_NAMES:
        return True
    name = path.name.lower()
    if name.endswith((".tmp", ".log", ".bak")):
        return True
    return False


def iter_local_files(local_depot: Path) -> List[Path]:
    files: List[Path] = []
    for path in local_depot.rglob("*"):
        if path.is_file() and not should_skip(path.relative_to(local_depot)):
            files.append(path)

    def sort_key(p: Path) -> Tuple[int, str]:
        rel = rel_posix(p, local_depot)
        # packs first, catalog last. The launcher should not see a catalog pointing to packs that
        # are not uploaded yet.
        if rel.startswith("packs/") and rel.endswith(".bin"):
            return (0, rel)
        if rel == "catalog.json":
            return (3, rel)
        if rel.endswith(".json"):
            return (2, rel)
        return (1, rel)

    return sorted(files, key=sort_key)


def walk_json_strings(value: Any) -> Iterable[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from walk_json_strings(item)
    elif isinstance(value, dict):
        for key, item in value.items():
            # Be generous: collect string values under pack-ish keys, and recurse normally.
            if isinstance(item, str) and "pack" in str(key).lower():
                yield item
            yield from walk_json_strings(item)


def collect_expected_pack_relpaths(local_depot: Path) -> Set[str]:
    """Best-effort: find pack filenames referenced by JSON manifests/catalogs."""
    expected: Set[str] = set()

    # Local pack files always count.
    pack_dir = local_depot / "packs"
    if pack_dir.exists():
        for p in pack_dir.glob("*.bin"):
            expected.add(f"packs/{p.name}")

    # Also parse JSON metadata in case some packs were already uploaded and deleted locally.
    for jf in local_depot.rglob("*.json"):
        try:
            rel = jf.relative_to(local_depot)
        except ValueError:
            continue
        if should_skip(rel):
            continue
        try:
            data = json.loads(jf.read_text(encoding="utf-8"))
        except Exception:
            continue
        for s in walk_json_strings(data):
            base = s.strip().replace("\\", "/").split("/")[-1]
            if not base:
                continue
            if base.endswith(".bin") and base.startswith("pack"):
                expected.add(f"packs/{base}")
            elif PACK_NAME_RE.match(base):
                expected.add(f"packs/{base}.bin")
    return expected


def list_remote_files(api: Any, repo: str, repo_type: str, token: str) -> Set[str]:
    try:
        return set(api.list_repo_files(repo_id=repo, repo_type=repo_type, token=token))
    except TypeError:
        return set(api.list_repo_files(repo_id=repo, repo_type=repo_type))
    except Exception as exc:
        log(f"[HF-UPLOAD][WARN] Could not list remote files; continuing without skip check: {exc}")
        return set()


def upload_one(api: Any, *, path: Path, path_in_repo: str, repo: str, repo_type: str, token: str) -> None:
    api.upload_file(
        path_or_fileobj=str(path),
        path_in_repo=path_in_repo,
        repo_id=repo,
        repo_type=repo_type,
        token=token,
    )


def main() -> int:
    ap = argparse.ArgumentParser(description="Incremental HF uploader for 0xo depot outputs.")
    ap.add_argument("--local-depot", required=True, help="Local depot folder, e.g. E:/007Launcher/depot/stellar-blade")
    ap.add_argument("--repo", required=True, help="HF repo id, e.g. owner/repo")
    ap.add_argument("--repo-type", default="dataset", choices=["dataset", "model", "space"])
    ap.add_argument("--prefix", required=True, help="Path prefix in repo, e.g. stellar-blade")
    ap.add_argument("--workers", type=int, default=1, help="Accepted for backward compatibility; upload is sequential.")
    ap.add_argument("--delete-local-packs", action="store_true", help="Delete local packs/*.bin after each successful upload.")
    ap.add_argument("--keep-local-packs", action="store_true", help="Keep local packs even if --delete-local-packs was passed.")
    ap.add_argument("--skip-existing-remote", action="store_true", default=True, help="Skip a local file if the same path already exists remotely.")
    ap.add_argument("--no-xet-high-performance", action="store_true", help="Do not enable HF_XET_HIGH_PERFORMANCE.")
    # Legacy flags accepted so old scripts do not break.
    ap.add_argument("--legacy-upload-folder", action="store_true", help=argparse.SUPPRESS)
    args = ap.parse_args()

    local_depot = Path(args.local_depot).resolve()
    if not local_depot.exists():
        raise SystemExit(f"local depot does not exist: {local_depot}")

    # Safe defaults for Windows machines. High performance mode can make disk/CPU/network spike.
    os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")
    os.environ.setdefault("PYTHONUTF8", "1")
    if not args.no_xet_high_performance:
        # Keep it OFF by default unless the user explicitly exported it before launching the tool.
        os.environ.setdefault("HF_XET_HIGH_PERFORMANCE", "0")

    token = os.environ.get("HF_TOKEN")
    if not token:
        raise SystemExit("HF_TOKEN is not set")

    try:
        from huggingface_hub import HfApi
    except Exception as exc:
        raise SystemExit(
            "Cannot import huggingface_hub. Install/update it with: "
            "python -m pip install -U huggingface_hub hf_xet\n"
            f"Import error: {exc}"
        )

    api = HfApi(token=token)
    api.create_repo(repo_id=args.repo, repo_type=args.repo_type, exist_ok=True)

    prefix = args.prefix.strip("/\\")
    if not prefix:
        raise SystemExit("--prefix must not be empty")

    all_files = iter_local_files(local_depot)
    remote_files = list_remote_files(api, args.repo, args.repo_type, token)
    expected_packs = collect_expected_pack_relpaths(local_depot)

    log(f"[HF-UPLOAD] Mode: incremental per-file, no .hf_upload_stage")
    log(f"[HF-UPLOAD] Repo: {args.repo} ({args.repo_type})")
    log(f"[HF-UPLOAD] Prefix: {prefix}")
    log(f"[HF-UPLOAD] Local files found: {len(all_files)}")
    log(f"[HF-UPLOAD] Delete local packs after upload: {bool(args.delete_local_packs and not args.keep_local_packs)}")
    log(f"[HF-UPLOAD] HF_HUB_DISABLE_PROGRESS_BARS={os.environ.get('HF_HUB_DISABLE_PROGRESS_BARS')}")
    log(f"[HF-UPLOAD] HF_XET_HIGH_PERFORMANCE={os.environ.get('HF_XET_HIGH_PERFORMANCE', '')}")

    # If a previous run deleted local packs, make sure they are present remotely before catalog upload.
    local_pack_rels = {rel_posix(p, local_depot) for p in all_files if rel_posix(p, local_depot).startswith("packs/")}
    missing_local_pack_rels = sorted(expected_packs - local_pack_rels)
    if missing_local_pack_rels and remote_files:
        missing_remote = [rel for rel in missing_local_pack_rels if repo_path(prefix, rel) not in remote_files]
        if missing_remote:
            raise SystemExit(
                "Some packs are missing locally and not found remotely; rebuild packs or disable local deletion. Missing: "
                + ", ".join(missing_remote[:20])
            )
        log(f"[HF-UPLOAD] {len(missing_local_pack_rels)} expected packs already absent locally but present remotely; OK.")

    uploaded = 0
    skipped = 0
    deleted = 0
    total_bytes_uploaded = 0
    started = time.time()

    for idx, path in enumerate(all_files, start=1):
        rel = rel_posix(path, local_depot)
        dst = repo_path(prefix, rel)
        size = path.stat().st_size
        is_pack = rel.startswith("packs/") and rel.endswith(".bin")

        if args.skip_existing_remote and dst in remote_files:
            skipped += 1
            log(f"[HF-UPLOAD] SKIP remote exists ({idx}/{len(all_files)}): {dst}")
            if is_pack and args.delete_local_packs and not args.keep_local_packs:
                try:
                    path.unlink()
                    deleted += 1
                    log(f"[HF-UPLOAD] Deleted local already-uploaded pack: {rel}")
                except OSError as exc:
                    log(f"[HF-UPLOAD][WARN] Could not delete local pack {rel}: {exc}")
            continue

        log(f"[HF-UPLOAD] Upload ({idx}/{len(all_files)}): {rel} -> {dst} ({size/1024/1024:.1f} MiB)")
        upload_one(api, path=path, path_in_repo=dst, repo=args.repo, repo_type=args.repo_type, token=token)
        uploaded += 1
        total_bytes_uploaded += size
        remote_files.add(dst)

        if is_pack and args.delete_local_packs and not args.keep_local_packs:
            try:
                path.unlink()
                deleted += 1
                log(f"[HF-UPLOAD] Deleted local pack after upload: {rel}")
            except OSError as exc:
                log(f"[HF-UPLOAD][WARN] Could not delete local pack {rel}: {exc}")

    elapsed = time.time() - started
    log(
        f"[HF-UPLOAD] Done. uploaded={uploaded}, skipped={skipped}, "
        f"local_packs_deleted={deleted}, uploaded_size={total_bytes_uploaded/1024/1024:.1f} MiB, "
        f"elapsed={elapsed/60:.1f} min."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
