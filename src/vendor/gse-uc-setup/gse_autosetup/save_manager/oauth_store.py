from __future__ import annotations

import base64
import ctypes
import hashlib
import json
import os
from ctypes import wintypes
from pathlib import Path

_MAGIC = b"GSEGOAUTH1\0"
_ENTROPY = b"GSE.UC.Setup.GoogleOAuth.v1"


class _DATA_BLOB(ctypes.Structure):
    _fields_ = [("cbData", wintypes.DWORD), ("pbData", ctypes.POINTER(ctypes.c_byte))]


def _blob(data: bytes):
    buf = ctypes.create_string_buffer(data)
    return _DATA_BLOB(len(data), ctypes.cast(buf, ctypes.POINTER(ctypes.c_byte))), buf


def _xor_for_tests(data: bytes) -> bytes:
    key = hashlib.sha256(os.environ["GSE_OAUTH_TEST_KEY"].encode("utf-8")).digest()
    return bytes(value ^ key[i % len(key)] for i, value in enumerate(data))


def _protect(data: bytes) -> bytes:
    if os.environ.get("GSE_OAUTH_TEST_KEY"):
        return _xor_for_tests(data)
    if os.name != "nt":
        raise RuntimeError("Google OAuth credential storage requires Windows DPAPI.")
    crypt32 = ctypes.windll.crypt32
    kernel32 = ctypes.windll.kernel32
    in_blob, in_buf = _blob(data)
    entropy_blob, entropy_buf = _blob(_ENTROPY)
    out_blob = _DATA_BLOB()
    CRYPTPROTECT_UI_FORBIDDEN = 0x1
    ok = crypt32.CryptProtectData(
        ctypes.byref(in_blob), "GSE / UC Setup Google Drive", ctypes.byref(entropy_blob),
        None, None, CRYPTPROTECT_UI_FORBIDDEN, ctypes.byref(out_blob),
    )
    _ = (in_buf, entropy_buf)
    if not ok:
        raise ctypes.WinError()
    try:
        return ctypes.string_at(out_blob.pbData, out_blob.cbData)
    finally:
        kernel32.LocalFree(out_blob.pbData)


def _unprotect(data: bytes) -> bytes:
    if os.environ.get("GSE_OAUTH_TEST_KEY"):
        return _xor_for_tests(data)
    if os.name != "nt":
        raise RuntimeError("Google OAuth credential storage requires Windows DPAPI.")
    crypt32 = ctypes.windll.crypt32
    kernel32 = ctypes.windll.kernel32
    in_blob, in_buf = _blob(data)
    entropy_blob, entropy_buf = _blob(_ENTROPY)
    out_blob = _DATA_BLOB()
    CRYPTPROTECT_UI_FORBIDDEN = 0x1
    ok = crypt32.CryptUnprotectData(
        ctypes.byref(in_blob), None, ctypes.byref(entropy_blob), None, None,
        CRYPTPROTECT_UI_FORBIDDEN, ctypes.byref(out_blob),
    )
    _ = (in_buf, entropy_buf)
    if not ok:
        raise ctypes.WinError()
    try:
        return ctypes.string_at(out_blob.pbData, out_blob.cbData)
    finally:
        kernel32.LocalFree(out_blob.pbData)


class OAuthCredentialStore:
    def __init__(self, path: Path):
        self.path = Path(path)

    def save(self, payload: dict) -> None:
        raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        encoded = _MAGIC + base64.b64encode(_protect(raw))
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_bytes(encoded)
        tmp.replace(self.path)

    def load(self) -> dict:
        if not self.path.is_file():
            return {}
        blob = self.path.read_bytes()
        if not blob.startswith(_MAGIC):
            raise RuntimeError("Unknown Google OAuth credential format.")
        protected = base64.b64decode(blob[len(_MAGIC):])
        return json.loads(_unprotect(protected).decode("utf-8"))

    def clear(self) -> None:
        self.path.unlink(missing_ok=True)
