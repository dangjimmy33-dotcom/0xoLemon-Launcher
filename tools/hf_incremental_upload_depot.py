#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Incremental Hugging Face uploader for 0xo depot outputs.

Design goals for big Windows game depots:
- No .hf_upload_stage folder.
- No full 60GB copy/hardlink staging.
- Upload packs one-by-one directly from local_depot.
- After each pack upload succeeds, optionally delete that local pack immediately.
- Upload metadata/catalog only after all packs are present remotely.
- Re-run safe: if a pack was deleted locally but already exists remotely, skip it.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Iterable, Set

PACK_PATH_RE = re.compile(r"^packs/[A-Za-z0-9._-]+\.bin$")
INVALID_PACK_BASENAMES = {".bin", "pack-.bin", "pack.bin"}


def log(msg: str) -> None:
    print(msg, flush=True)


def repo_join(prefix: str, rel: str) -> str:
    prefix = prefix.strip("/\\")
    rel = rel.replace("\\", "/").strip("/")
    return f"{prefix}/{rel}" if prefix else rel


def is_valid_pack_path(value: str) -> bool:
    value = value.replace("\\", "/").strip("/")
    if not PACK_PATH_RE.match(value):
        return False
    if Path(value).name in INVALID_PACK_BASENAMES:
        return False
    return True


def walk_json_for_pack_paths(obj: Any, out: Set[str]) -> None:
    if isinstance(obj, dict):
        path = obj.get("path")
        if isinstance(path, str):
            p = path.replace("\\", "/").strip("/")
            if is_valid_pack_path(p):
                out.add(p)
        # Fallback for structures that have packId/id but no path.
        pack_id = obj.get("packId") or obj.get("pack_id")
        if not isinstance(pack_id, str) and isinstance(obj.get("id"), str):
            cand = obj.get("id")
            if str(cand).startswith("pack"):
                pack_id = cand
        if isinstance(pack_id, str):
            pack_id = pack_id.strip().replace("\\", "/").strip("/")
            if pack_id and not pack_id.endswith(".bin"):
                p = f"packs/{pack_id}.bin"
            else:
                p = f"packs/{pack_id}"
            if is_valid_pack_path(p):
                out.add(p)
        for v in obj.values():
            walk_json_for_pack_paths(v, out)
    elif isinstance(obj, list):
        for v in obj:
            walk_json_for_pack_paths(v, out)


def json_files(root: Path) -> Iterable[Path]:
    skip_parts = {".git", ".hg", ".svn", "__pycache__", ".hf_upload_stage", ".cache"}
    for p in root.rglob("*.json"):
        if any(part in skip_parts for part in p.parts):
            continue
        if p.is_file():
            yield p


def discover_pack_paths(local_depot: Path) -> Set[str]:
    expected: Set[str] = set()
    # 1) Manifest/catalog references.
    for jf in json_files(local_depot):
        try:
            data = json.loads(jf.read_text(encoding="utf-8"))
        except Exception:
            continue
        walk_json_for_pack_paths(data, expected)

    # 2) Local packs always count, even if metadata format changes.
    pack_dir = local_depot / "packs"
    if pack_dir.exists():
        for p in pack_dir.glob("*.bin"):
            rel = p.relative_to(local_depot).as_posix()
            if is_valid_pack_path(rel):
                expected.add(rel)

    return expected


def metadata_files(local_depot: Path) -> list[Path]:
    skip_dirs = {"packs", ".git", ".hg", ".svn", "__pycache__", ".hf_upload_stage", ".cache"}
    out: list[Path] = []
    for p in local_depot.rglob("*"):
        if not p.is_file():
            continue
        rel = p.relative_to(local_depot)
        if any(part in skip_dirs for part in rel.parts):
            continue
        # Avoid random temp/log/cache artifacts.
        if p.suffix.lower() in {".tmp", ".log", ".lock"}:
            continue
        out.append(p)
    return sorted(out, key=lambda x: x.relative_to(local_depot).as_posix())


def remote_file_set(api: Any, repo: str, repo_type: str, prefix: str) -> Set[str]:
    try:
        files = api.list_repo_files(repo_id=repo, repo_type=repo_type)
    except Exception as exc:
        log(f"[HF-UPLOAD][WARN] Could not list remote files; upload will continue without remote-skip check: {exc}")
        return set()
    pref = prefix.strip("/\\")
    if pref:
        pref += "/"
    return {f for f in files if not pref or f.startswith(pref)}


def upload_one(api: Any, local_path: Path, repo_path: str, repo: str, repo_type: str, token: str) -> None:
    log(f"[HF-UPLOAD] Upload: {local_path.name} -> {repo_path}")
    api.upload_file(
        path_or_fileobj=str(local_path),
        path_in_repo=repo_path,
        repo_id=repo,
        repo_type=repo_type,
        token=token,
    )


def main() -> int:
    ap = argparse.ArgumentParser(description="Incremental per-file HF uploader for 0xo depot outputs.")
    ap.add_argument("--local-depot", required=True, help="Local depot folder, e.g. E:/007Launcher/depot/stellar-blade")
    ap.add_argument("--repo", required=True, help="HF repo id, e.g. owner/repo")
    ap.add_argument("--repo-type", default="dataset", choices=["dataset", "model", "space"])
    ap.add_argument("--prefix", required=True, help="Path prefix in repo, e.g. stellar-blade")
    ap.add_argument("--delete-local-packs", action="store_true", help="Delete each local pack after it uploads successfully.")
    ap.add_argument("--no-remote-skip", action="store_true", help="Do not skip packs that are already present remotely.")
    args = ap.parse_args()

    local_depot = Path(args.local_depot).resolve()
    if not local_depot.exists():
        raise SystemExit(f"local depot does not exist: {local_depot}")

    os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")
    os.environ.setdefault("PYTHONUTF8", "1")
    # Keep safe default. Users can set HF_XET_HIGH_PERFORMANCE=1 manually before launching the tool.
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

    expected_packs = sorted(discover_pack_paths(local_depot))
    meta = metadata_files(local_depot)
    log("[HF-UPLOAD] Mode: incremental per-file, no .hf_upload_stage")
    log(f"[HF-UPLOAD] Repo: {args.repo} ({args.repo_type})")
    log(f"[HF-UPLOAD] Prefix: {args.prefix.strip('/\\')}")
    log(f"[HF-UPLOAD] Pack refs found: {len(expected_packs)}")
    log(f"[HF-UPLOAD] Metadata files found: {len(meta)}")
    log(f"[HF-UPLOAD] Delete local packs after upload: {args.delete_local_packs}")
    log(f"[HF-UPLOAD] HF_HUB_DISABLE_PROGRESS_BARS={os.environ.get('HF_HUB_DISABLE_PROGRESS_BARS')}")
    log(f"[HF-UPLOAD] HF_XET_HIGH_PERFORMANCE={os.environ.get('HF_XET_HIGH_PERFORMANCE', '')}")

    remote_files = set() if args.no_remote_skip else remote_file_set(api, args.repo, args.repo_type, args.prefix)

    missing: list[str] = []
    uploaded = 0
    skipped_remote = 0
    deleted = 0
    started = time.time()

    # Packs first. If metadata is uploaded last, users won't see a catalog pointing to missing packs.
    for rel in expected_packs:
        local_path = local_depot / rel
        repo_path = repo_join(args.prefix, rel)
        if local_path.exists():
            upload_one(api, local_path, repo_path, args.repo, args.repo_type, token)
            uploaded += 1
            if args.delete_local_packs:
                try:
                    local_path.unlink()
                    deleted += 1
                    log(f"[HF-UPLOAD] Deleted local pack after upload: {rel}")
                except OSError as exc:
                    log(f"[HF-UPLOAD][WARN] Uploaded but could not delete local pack {rel}: {exc}")
        elif repo_path in remote_files:
            skipped_remote += 1
            log(f"[HF-UPLOAD] Skip already remote: {repo_path}")
        else:
            missing.append(rel)

    if missing:
        show = ", ".join(missing[:20])
        if len(missing) > 20:
            show += f", ... (+{len(missing)-20} more)"
        raise SystemExit(
            "Some packs are missing locally and not found remotely; rebuild packs or disable local deletion. "
            f"Missing: {show}"
        )

    # Metadata after packs.
    for local_path in meta:
        rel = local_path.relative_to(local_depot).as_posix()
        repo_path = repo_join(args.prefix, rel)
        upload_one(api, local_path, repo_path, args.repo, args.repo_type, token)

    elapsed = time.time() - started
    log(
        f"[HF-UPLOAD] Done in {elapsed/60:.1f} min. "
        f"packs_uploaded={uploaded}, packs_remote_skipped={skipped_remote}, packs_deleted={deleted}, metadata_uploaded={len(meta)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
