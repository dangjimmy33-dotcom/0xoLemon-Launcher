#!/usr/bin/env python3
"""Build a signed 0xoLemon Cloud Save map repository.

The script is intentionally offline-first. Private Ed25519 keys are written only
to the developer-selected key directory. The public repository contains
root/timestamp/snapshot/targets metadata and immutable map targets.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import shutil
import sys
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

try:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import (
        Ed25519PrivateKey,
        Ed25519PublicKey,
    )
except ImportError as exc:  # pragma: no cover - environment error
    raise SystemExit("Install the 'cryptography' Python package first.") from exc

try:
    import jsonschema
except ImportError:  # Optional; the launcher still performs its own validation.
    jsonschema = None

ROLES = ("timestamp", "snapshot", "targets")
SPEC_VERSION = "1.0.0"


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def expiry(days: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(days=days)).replace(microsecond=0).isoformat().replace("+00:00", "Z")


@dataclass(frozen=True)
class KeyMaterial:
    key_id: str
    private: Ed25519PrivateKey
    public_b64: str


def private_key_path(keys_dir: Path, role: str) -> Path:
    return keys_dir / f"{role}.ed25519.pem"


def load_or_create_key(keys_dir: Path, role: str) -> KeyMaterial:
    path = private_key_path(keys_dir, role)
    if path.exists():
        private = serialization.load_pem_private_key(path.read_bytes(), password=None)
        if not isinstance(private, Ed25519PrivateKey):
            raise ValueError(f"{path} is not an Ed25519 private key")
    else:
        private = Ed25519PrivateKey.generate()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(
            private.private_bytes(
                encoding=serialization.Encoding.PEM,
                format=serialization.PrivateFormat.PKCS8,
                encryption_algorithm=serialization.NoEncryption(),
            )
        )
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass
    public = private.public_key().public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )
    return KeyMaterial(
        key_id=sha256_hex(public),
        private=private,
        public_b64=base64.b64encode(public).decode("ascii"),
    )


def sign_envelope(signed: dict[str, Any], keys: list[KeyMaterial] | tuple[KeyMaterial, ...]) -> dict[str, Any]:
    payload = canonical_json(signed)
    return {
        "signed": signed,
        "signatures": [
            {
                "keyid": key.key_id,
                "sig": base64.b64encode(key.private.sign(payload)).decode("ascii"),
            }
            for key in keys
        ],
    }


def metadata_description(version: int, data: bytes) -> dict[str, Any]:
    return {
        "version": version,
        "length": len(data),
        "hashes": {"sha256": sha256_hex(data)},
    }


def validate_map(map_path: Path, schema_path: Path | None) -> dict[str, Any]:
    data = json.loads(map_path.read_text(encoding="utf-8"))
    if data.get("schemaVersion") != 1:
        raise ValueError("Cloud Save map must use schemaVersion=1")
    if data.get("platform") != "windows":
        raise ValueError("Cloud Save map must target platform='windows'")
    if not str(data.get("mapVersion", "")).strip():
        raise ValueError("Cloud Save map needs a non-empty mapVersion")
    if schema_path and schema_path.exists() and jsonschema is not None:
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        jsonschema.validate(instance=data, schema=schema)
    return data


def build_repository(args: argparse.Namespace) -> None:
    repo = args.repo.resolve()
    keys_dir = args.keys.resolve()
    map_path = args.map.resolve()
    schema_path = args.schema.resolve() if args.schema else None
    map_json = validate_map(map_path, schema_path)
    map_version = str(map_json["mapVersion"])

    if args.root_threshold < 1 or args.root_threshold > args.root_key_count:
        raise ValueError("root threshold must be between 1 and root key count")
    root_keys = [
        load_or_create_key(keys_dir, f"root-{index}")
        for index in range(1, args.root_key_count + 1)
    ]
    role_keys = {role: load_or_create_key(keys_dir, role) for role in ROLES}
    all_keys = [*root_keys, *role_keys.values()]
    repo.mkdir(parents=True, exist_ok=True)
    maps_dir = repo / "maps"
    maps_dir.mkdir(parents=True, exist_ok=True)

    safe_version = "".join(ch if ch.isalnum() or ch in ".-_" else "-" for ch in map_version)
    target_name = f"maps/cloud-save-map-{safe_version}.json"
    target_path = repo / target_name
    shutil.copyfile(map_path, target_path)
    target_bytes = target_path.read_bytes()

    root_signed = {
        "_type": "root",
        "spec_version": SPEC_VERSION,
        "version": args.root_version,
        "expires": expiry(args.root_days),
        "consistent_snapshot": True,
        "keys": {
            key.key_id: {
                "keytype": "ed25519",
                "scheme": "ed25519",
                "keyval": {"public": key.public_b64},
            }
            for key in all_keys
        },
        "roles": {
            "root": {
                "keyids": [key.key_id for key in root_keys],
                "threshold": args.root_threshold,
            },
            **{
                role: {"keyids": [role_keys[role].key_id], "threshold": 1}
                for role in ROLES
            },
        },
    }
    root_envelope = sign_envelope(root_signed, root_keys)
    (repo / "root.json").write_bytes(json_bytes(root_envelope))

    targets_signed = {
        "_type": "targets",
        "spec_version": SPEC_VERSION,
        "version": args.targets_version,
        "expires": expiry(args.targets_days),
        "targets": {
            target_name: {
                "length": len(target_bytes),
                "hashes": {"sha256": sha256_hex(target_bytes)},
                "custom": {
                    "mapVersion": map_version,
                    "rollout": {
                        "percentage": args.rollout,
                        "seed": args.rollout_seed,
                    },
                },
            }
        },
    }
    targets_bytes = json_bytes(sign_envelope(targets_signed, [role_keys["targets"]]))
    (repo / "targets.json").write_bytes(targets_bytes)

    snapshot_signed = {
        "_type": "snapshot",
        "spec_version": SPEC_VERSION,
        "version": args.snapshot_version,
        "expires": expiry(args.snapshot_days),
        "meta": {
            "targets.json": metadata_description(args.targets_version, targets_bytes),
        },
    }
    snapshot_bytes = json_bytes(sign_envelope(snapshot_signed, [role_keys["snapshot"]]))
    (repo / "snapshot.json").write_bytes(snapshot_bytes)

    timestamp_signed = {
        "_type": "timestamp",
        "spec_version": SPEC_VERSION,
        "version": args.timestamp_version,
        "expires": expiry(args.timestamp_days),
        "meta": {
            "snapshot.json": metadata_description(args.snapshot_version, snapshot_bytes),
        },
    }
    timestamp_bytes = json_bytes(sign_envelope(timestamp_signed, [role_keys["timestamp"]]))
    (repo / "timestamp.json").write_bytes(timestamp_bytes)

    print(f"Signed repository written to: {repo}")
    print(f"Private keys kept in: {keys_dir}")
    print("Copy repo/root.json into launcher resources only for the first trusted release.")
    print("Never upload or commit the private key directory.")


def verify_role_envelope(
    envelope: dict[str, Any],
    root_signed: dict[str, Any],
    role: str,
) -> None:
    role_config = root_signed["roles"][role]
    allowed = set(role_config["keyids"])
    threshold = int(role_config["threshold"])
    payload = canonical_json(envelope["signed"])
    verified: set[str] = set()
    for signature_record in envelope.get("signatures") or []:
        key_id = signature_record.get("keyid", "")
        if key_id not in allowed or key_id in verified:
            continue
        key = root_signed["keys"].get(key_id)
        if not key:
            continue
        try:
            public = base64.b64decode(key["keyval"]["public"], validate=True)
            signature = base64.b64decode(signature_record["sig"], validate=True)
            Ed25519PublicKey.from_public_bytes(public).verify(signature, payload)
        except (ValueError, KeyError):
            continue
        verified.add(key_id)
    if len(verified) < threshold:
        raise ValueError(
            f"{role} metadata has {len(verified)} valid signature(s), needs {threshold}"
        )


def verify_repository(args: argparse.Namespace) -> None:
    repo = args.repo.resolve()
    root = json.loads((repo / "root.json").read_text(encoding="utf-8"))
    root_signed = root["signed"]
    verify_role_envelope(root, root_signed, "root")
    for filename, role in (("timestamp.json", "timestamp"), ("snapshot.json", "snapshot"), ("targets.json", "targets")):
        env = json.loads((repo / filename).read_text(encoding="utf-8"))
        verify_role_envelope(env, root_signed, role)

    timestamp = json.loads((repo / "timestamp.json").read_text(encoding="utf-8"))["signed"]
    snapshot_bytes = (repo / "snapshot.json").read_bytes()
    snapshot_desc = timestamp["meta"]["snapshot.json"]
    if snapshot_desc["length"] != len(snapshot_bytes) or snapshot_desc["hashes"]["sha256"] != sha256_hex(snapshot_bytes):
        raise ValueError("snapshot.json description mismatch")

    snapshot = json.loads(snapshot_bytes)["signed"]
    targets_bytes = (repo / "targets.json").read_bytes()
    targets_desc = snapshot["meta"]["targets.json"]
    if targets_desc["length"] != len(targets_bytes) or targets_desc["hashes"]["sha256"] != sha256_hex(targets_bytes):
        raise ValueError("targets.json description mismatch")

    targets = json.loads(targets_bytes)["signed"]["targets"]
    for name, description in targets.items():
        data = (repo / name).read_bytes()
        if description["length"] != len(data) or description["hashes"]["sha256"] != sha256_hex(data):
            raise ValueError(f"target mismatch: {name}")
    print(f"Repository verification PASS: {repo}")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    sub = result.add_subparsers(dest="command", required=True)
    build = sub.add_parser("build", help="create/update a signed repository")
    build.add_argument("--map", type=Path, required=True)
    build.add_argument("--schema", type=Path)
    build.add_argument("--repo", type=Path, required=True)
    build.add_argument("--keys", type=Path, required=True)
    build.add_argument("--rollout", type=int, default=100, choices=range(0, 101), metavar="0..100")
    build.add_argument("--rollout-seed", default="cloud-save-map-v1")
    build.add_argument("--root-key-count", type=int, default=3, choices=range(1, 6), metavar="1..5")
    build.add_argument("--root-threshold", type=int, default=2)
    build.add_argument("--root-version", type=int, default=1)
    build.add_argument("--timestamp-version", type=int, default=1)
    build.add_argument("--snapshot-version", type=int, default=1)
    build.add_argument("--targets-version", type=int, default=1)
    build.add_argument("--root-days", type=int, default=3650)
    build.add_argument("--timestamp-days", type=int, default=2)
    build.add_argument("--snapshot-days", type=int, default=14)
    build.add_argument("--targets-days", type=int, default=31)
    build.set_defaults(func=build_repository)

    verify = sub.add_parser("verify", help="verify signatures, hashes, and lengths")
    verify.add_argument("--repo", type=Path, required=True)
    verify.set_defaults(func=verify_repository)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.func(args)
        return 0
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
