#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
0xo Depot Uploader Studio - Local GUI Server

What this fixes compared with a plain HTML/BAT generator:
- Runs from http://127.0.0.1, not file://.
- Executes cargo/PowerShell directly through a local backend.
- Passes HF_TOKEN only to the child process environment, so you do not need to set it globally.
- Streams logs back to the GUI.
- Can preflight paths/tools/write permissions before building.

No external Python packages are required by this server.
The sync tool still needs huggingface_hub when you choose to sync/upload HF metadata.
"""

from __future__ import annotations

import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.parse
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

HOST = "127.0.0.1"
PORT = int(os.environ.get("OXO_DEPOT_UPLOADER_PORT", "8776"))
BASE_DIR = Path(__file__).resolve().parent
GUI_FILE = BASE_DIR / "depot-builder-gui-local.html"
PUBLISH_PS1 = BASE_DIR / "publish_depot_version.ps1"
SYNC_TOOL = BASE_DIR / "sync_hf_depot_metadata.py"
INCREMENTAL_UPLOAD_TOOL = BASE_DIR / "hf_incremental_upload_depot.py"

_current_process: Optional[subprocess.Popen[str]] = None
_current_lock = threading.Lock()

_logs: List[Dict[str, Any]] = []
_running = False
_exit_code: Optional[int] = None
_started_at: Optional[float] = None
_task_title = ""

ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def clean_log_text(value: Any) -> str:
    text = str(value)
    text = ANSI_RE.sub("", text)
    text = text.replace("\r", "\n")
    return text


def add_log(text: str, level: str = "info") -> None:
    global _logs
    text = clean_log_text(text)
    line = {"i": len(_logs), "t": time.time(), "level": level, "text": text}
    _logs.append(line)
    # Keep memory bounded, but usually logs are small enough.
    if len(_logs) > 20000:
        # Preserve index monotonicity by not trimming unless huge.
        _logs = _logs[-10000:]
        for idx, item in enumerate(_logs):
            item["i"] = idx


def reset_task(title: str) -> None:
    global _logs, _running, _exit_code, _started_at, _task_title
    _logs = []
    _running = True
    _exit_code = None
    _started_at = time.time()
    _task_title = title
    add_log(f"▶ {title}", "step")


def finish_task(exit_code: int) -> None:
    global _running, _exit_code
    _exit_code = exit_code
    _running = False
    if exit_code == 0:
        add_log("[DONE] Hoàn tất.", "success")
    else:
        add_log(f"[FAIL] Tiến trình kết thúc với mã lỗi {exit_code}.", "error")


def json_bytes(payload: Any) -> bytes:
    return json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def safe_str(value: Any, default: str = "") -> str:
    if value is None:
        return default
    return str(value)


def bool_value(value: Any) -> bool:
    return bool(value) and str(value).lower() not in ("0", "false", "no", "off", "")


def find_powershell() -> Optional[str]:
    for exe in ("pwsh", "pwsh.exe", "powershell", "powershell.exe"):
        p = shutil.which(exe)
        if p:
            return p
    return None


def find_python() -> str:
    # Use current interpreter first; it is guaranteed to exist.
    return sys.executable or "python"


def process_creation_kwargs() -> Dict[str, Any]:
    if os.name == "nt":
        return {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
    return {"preexec_fn": os.setsid}


def terminate_process(proc: subprocess.Popen[str]) -> None:
    try:
        if os.name == "nt":
            try:
                proc.send_signal(signal.CTRL_BREAK_EVENT)  # type: ignore[attr-defined]
                time.sleep(0.8)
            except Exception:
                pass
            if proc.poll() is None:
                proc.terminate()
        else:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            except Exception:
                proc.terminate()
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass


def run_process(args: List[str], cwd: Optional[Path], env: Dict[str, str]) -> int:
    """Run a command and stream stdout/stderr merged into the global log."""
    global _current_process
    add_log("[RUN] " + command_preview(args), "cmd")
    if cwd:
        add_log(f"[CWD] {cwd}", "info")

    try:
        with _current_lock:
            proc = subprocess.Popen(
                args,
                cwd=str(cwd) if cwd else None,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                universal_newlines=True,
                **process_creation_kwargs(),
            )
            _current_process = proc

        assert proc.stdout is not None
        for line in proc.stdout:
            line = line.rstrip("\r\n")
            if not line:
                add_log("", "info")
                continue
            level = classify_line(line)
            add_log(line, level)
        code = proc.wait()
        return int(code or 0)
    except FileNotFoundError as exc:
        add_log(f"Không tìm thấy executable: {exc}", "error")
        return 127
    except Exception as exc:
        add_log(f"Lỗi khi chạy tiến trình: {exc}", "error")
        return 1
    finally:
        with _current_lock:
            _current_process = None


def classify_line(line: str) -> str:
    low = line.lower()
    if any(x in low for x in ("error", "failed", "panic", "denied", "unauthorized", "forbidden", "traceback")):
        return "error"
    if any(x in low for x in ("warning", "warn", "cảnh báo")):
        return "warn"
    if any(x in low for x in ("done", "success", "verified", "uploaded", "synced", "built")):
        return "success"
    if line.startswith("[") or line.startswith("▶"):
        return "step"
    return "info"


def command_preview(args: List[str]) -> str:
    def quote(a: str) -> str:
        if not a:
            return '""'
        if any(c.isspace() for c in a) or any(c in a for c in '"&;()[]{}'):
            return '"' + a.replace('"', '\\"') + '"'
        return a
    # Hide tokens if someone accidentally passes one as arg.
    redacted: List[str] = []
    for a in args:
        if a.startswith("hf_") and len(a) > 8:
            redacted.append("hf_***")
        else:
            redacted.append(a)
    return " ".join(quote(a) for a in redacted)


def get_src_tauri_from_manifest(cargo_manifest: str) -> Path:
    manifest = Path(cargo_manifest).expanduser()
    if manifest.name.lower() == "cargo.toml":
        return manifest.parent
    return manifest



def int_value(value: Any, default: int = 0) -> int:
    try:
        if value is None or value == "":
            return default
        return int(str(value).strip())
    except Exception:
        return default

def pack_options(payload: Dict[str, Any]) -> Tuple[int, int, str]:
    size_mb = int_value(payload.get("packTargetMb", payload.get("packSizeMb")), 256)
    size_mb = max(4, min(size_mb, 4096))
    start_index = int_value(payload.get("packStartIndex"), 0)
    start_index = max(0, min(start_index, 999999))
    prefix = safe_str(payload.get("packIdPrefix"), "pack-").strip() or "pack-"
    return size_mb, start_index, prefix

def validate_payload(payload: Dict[str, Any], for_upload: bool = False) -> List[Tuple[str, str]]:
    """Return list of (level, message)."""
    msgs: List[Tuple[str, str]] = []
    mode = safe_str(payload.get("mode"), "build-version")
    game_dir = Path(safe_str(payload.get("gameDir"))).expanduser()
    old_dir = Path(safe_str(payload.get("oldDir"))).expanduser()
    game_id = safe_str(payload.get("gameId")).strip().lower()
    exe_name = safe_str(payload.get("exeName")).strip()
    depot_root = Path(safe_str(payload.get("depotRoot"))).expanduser()
    repo = safe_str(payload.get("repoId")).strip()
    token = safe_str(payload.get("hfToken")).strip()
    encrypt_packs = bool_value(payload.get("encryptPacks", True))
    pack_mb, pack_start, pack_prefix = pack_options(payload)
    depot_key = safe_str(payload.get("depotKey")).strip()
    cargo_manifest = Path(safe_str(payload.get("cargoManifest"))).expanduser()

    if not game_id:
        msgs.append(("error", "Thiếu Game ID."))
    if not game_dir.exists():
        msgs.append(("error", f"Game/New directory không tồn tại: {game_dir}"))
    elif not game_dir.is_dir():
        msgs.append(("error", f"Game/New directory không phải thư mục: {game_dir}"))
    else:
        msgs.append(("success", f"OK game folder: {game_dir}"))

    if mode == "build-pair":
        if not old_dir.exists():
            msgs.append(("error", f"Old directory không tồn tại: {old_dir}"))
        elif not old_dir.is_dir():
            msgs.append(("error", f"Old directory không phải thư mục: {old_dir}"))
        else:
            msgs.append(("success", f"OK old folder: {old_dir}"))

    if exe_name and game_dir.exists():
        exe_path = game_dir / exe_name
        if exe_path.exists():
            msgs.append(("success", f"OK launch executable: {exe_path}"))
        else:
            msgs.append(("warn", f"Không thấy launch executable trong game folder: {exe_path}"))

    if not cargo_manifest.exists():
        msgs.append(("error", f"Không thấy Cargo.toml/src-tauri: {cargo_manifest}"))
    else:
        msgs.append(("success", f"OK Cargo manifest: {cargo_manifest}"))

    cargo = shutil.which("cargo")
    if cargo:
        msgs.append(("success", f"OK cargo: {cargo}"))
    else:
        msgs.append(("error", "Không tìm thấy cargo trong PATH."))

    py = find_python()
    msgs.append(("success", f"OK python: {py}"))

    if not PUBLISH_PS1.exists():
        msgs.append(("warn", f"Không thấy publish_depot_version.ps1 cạnh server: {PUBLISH_PS1}"))
    if not SYNC_TOOL.exists():
        msgs.append(("warn", f"Không thấy sync_hf_depot_metadata.py cạnh server: {SYNC_TOOL}"))

    if bool_value(payload.get("uploadToHf", True)) or for_upload:
        if not repo or "/" not in repo:
            msgs.append(("error", "Repo HF phải dạng owner/repo."))
        if not token:
            msgs.append(("error", "Thiếu Hugging Face token. Token chỉ được truyền cho process con, không set global."))
        else:
            msgs.append(("success", "HF token đã nhập, sẽ truyền bằng biến môi trường cho process con."))

    if encrypt_packs:
        if depot_key:
            msgs.append(("success", "Pack encryption bật; key sẽ truyền qua biến môi trường OXO_DEPOT_KEY."))
        else:
            msgs.append(("warn", "Pack encryption bật nhưng chưa nhập Depot key; builder sẽ dùng fallback trong source. Nên đặt key riêng trước khi public."))
    else:
        msgs.append(("warn", "Pack encryption đang tắt; pack .bin mới sẽ để plain như format cũ."))

    pack_mb, pack_start, pack_prefix = pack_options(payload)
    msgs.append(("success", f"Pack target: {pack_mb} MiB; prefix: {pack_prefix}; start index: {pack_start}."))

    try:
        depot_root.mkdir(parents=True, exist_ok=True)
        test_file = depot_root / ".0xo_write_test.tmp"
        test_file.write_text("ok", encoding="utf-8")
        test_file.unlink(missing_ok=True)  # type: ignore[arg-type]
        msgs.append(("success", f"OK quyền ghi Depot Root: {depot_root}"))
    except Exception as exc:
        msgs.append(("error", f"Không ghi được Depot Root {depot_root}: {exc}"))

    try:
        src_tauri = get_src_tauri_from_manifest(str(cargo_manifest))
        target_dir = src_tauri / "target"
        target_dir.mkdir(parents=True, exist_ok=True)
        test_file = target_dir / ".0xo_write_test.tmp"
        test_file.write_text("ok", encoding="utf-8")
        test_file.unlink(missing_ok=True)  # type: ignore[arg-type]
        msgs.append(("success", f"OK quyền ghi Cargo target: {target_dir}"))
    except Exception as exc:
        msgs.append(("error", f"Không ghi được Cargo target: {exc}"))

    return msgs


def final_version(payload: Dict[str, Any]) -> str:
    explicit = safe_str(payload.get("finalVersion")).strip()
    if explicit:
        return explicit
    base = safe_str(payload.get("gameVersion"), "1.0.0").strip() or "1.0.0"
    build_id = safe_str(payload.get("buildId")).strip()
    date = time.strftime("%Y-%m-%d")
    out = base
    if build_id:
        out += f" (Build {build_id})"
    if bool_value(payload.get("appendUploadDate", True)):
        out += f" - Uploaded {date}"
    return out


def old_final_version(payload: Dict[str, Any]) -> str:
    explicit = safe_str(payload.get("oldFinalVersion")).strip()
    if explicit:
        return explicit
    base = safe_str(payload.get("oldVersion"), "1.0.0").strip() or "1.0.0"
    build_id = safe_str(payload.get("oldBuildId")).strip()
    out = base
    if build_id:
        out += f" (Build {build_id})"
    return out


def common_env(payload: Dict[str, Any]) -> Dict[str, str]:
    env = os.environ.copy()
    token = safe_str(payload.get("hfToken")).strip()
    if token:
        env["HF_TOKEN"] = token
    depot_key = safe_str(payload.get("depotKey")).strip()
    if depot_key:
        env["OXO_DEPOT_KEY"] = depot_key
    # Better UTF-8 logs on Windows.
    env.setdefault("PYTHONUTF8", "1")
    env.setdefault("RUST_BACKTRACE", "1")

    # Hugging Face upload behavior:
    # - Disable tqdm progress bars because they spam ANSI lines inside this GUI log panel.
    # - Do NOT enable Xet high-performance by default: it can spike CPU/disk/RAM on large depots.
    # These are only passed to child processes, not written globally.
    env.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")
    env.setdefault("HF_XET_HIGH_PERFORMANCE", "0")
    return env


def depot_out_path(payload: Dict[str, Any]) -> Path:
    root = Path(safe_str(payload.get("depotRoot"))).expanduser()
    game_id = safe_str(payload.get("gameId")).strip()
    return root / game_id


def build_direct_args(payload: Dict[str, Any]) -> List[str]:
    mode = safe_str(payload.get("mode"), "build-version")
    game_dir = safe_str(payload.get("gameDir")).strip()
    old_dir = safe_str(payload.get("oldDir")).strip()
    game_id = safe_str(payload.get("gameId")).strip()
    exe_name = safe_str(payload.get("exeName")).strip()
    repo_id = safe_str(payload.get("repoId")).strip()
    repo_type = safe_str(payload.get("repoType"), "dataset").strip() or "dataset"
    repo_prefix = safe_str(payload.get("repoPrefix")).strip() or game_id
    out_path = str(depot_out_path(payload))
    keep = bool_value(payload.get("keepLocalPacks", False))
    extend = bool_value(payload.get("extendExisting", True))
    encrypt_packs = bool_value(payload.get("encryptPacks", True))
    pack_mb, pack_start, pack_prefix = pack_options(payload)

    args: List[str] = []
    if mode == "build-pair":
        args += [
            "build-pair",
            "--old-input", old_dir,
            "--old-version", old_final_version(payload),
            "--new-input", game_dir,
            "--new-version", final_version(payload),
        ]
    else:
        args += [
            "build-version",
            "--input", game_dir,
            "--version", final_version(payload),
        ]
        if extend:
            args += ["--extend-existing"]

    args += ["--out", out_path, "--game-id", game_id, "--pack-target-mb", str(pack_mb), "--pack-start-index", str(pack_start), "--pack-id-prefix", pack_prefix]
    if exe_name:
        args += ["--launch-executable", exe_name]
    # Do not let depot_builder upload directly. Build local first, then upload incrementally per file.
    if keep:
        args += ["--keep-local-packs"]
    if not encrypt_packs:
        args += ["--no-encrypt-packs"]
    return args


def run_sync_metadata(payload: Dict[str, Any], env: Dict[str, str]) -> int:
    game_id = safe_str(payload.get("gameId")).strip().lower()
    repo_id = safe_str(payload.get("repoId")).strip()
    repo_type = safe_str(payload.get("repoType"), "dataset").strip() or "dataset"
    repo_prefix = safe_str(payload.get("repoPrefix")).strip().lower() or game_id
    out_path = depot_out_path(payload)
    out_path.mkdir(parents=True, exist_ok=True)
    if not SYNC_TOOL.exists():
        add_log(f"Không thấy sync tool: {SYNC_TOOL}", "error")
        return 2
    add_log("[SYNC] Chỉ đồng bộ metadata HF: catalog/manifests/build-info. Không tải pack lớn.", "step")
    return run_process([
        find_python(),
        str(SYNC_TOOL),
        "--repo", repo_id,
        "--repo-type", repo_type,
        "--prefix", repo_prefix,
        "--out", str(out_path),
    ], cwd=BASE_DIR, env=env)


def run_publish_script(payload: Dict[str, Any], env: Dict[str, str]) -> int:
    ps = find_powershell()
    if not ps:
        add_log("Không tìm thấy PowerShell/pwsh trong PATH.", "error")
        return 127
    if not PUBLISH_PS1.exists():
        add_log(f"Không thấy script: {PUBLISH_PS1}", "error")
        return 2
    game_id = safe_str(payload.get("gameId")).strip().lower()
    repo_prefix = safe_str(payload.get("repoPrefix")).strip().lower() or game_id
    depot_root = safe_str(payload.get("depotRoot")).strip()
    pack_mb, pack_start, pack_prefix = pack_options(payload)
    args = [
        ps,
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", str(PUBLISH_PS1),
        "-GameId", game_id,
        "-Version", final_version(payload),
        "-InputDir", safe_str(payload.get("gameDir")).strip(),
        "-Repo", safe_str(payload.get("repoId")).strip(),
        "-RepoType", safe_str(payload.get("repoType"), "dataset").strip() or "dataset",
        "-RepoPrefix", repo_prefix,
        "-DepotRoot", depot_root,
        "-CargoManifest", safe_str(payload.get("cargoManifest")).strip(),
        "-PackTargetMb", str(pack_mb),
        "-PackStartIndex", str(pack_start),
        "-PackIdPrefix", pack_prefix,
    ]
    exe_name = safe_str(payload.get("exeName")).strip()
    if exe_name:
        args += ["-LaunchExecutable", exe_name]
    if bool_value(payload.get("keepLocalPacks", False)):
        args += ["-KeepLocalPacks"]
    if not bool_value(payload.get("encryptPacks", True)):
        args += ["-NoEncryptPacks"]
    if bool_value(payload.get("forceRebuild", False)):
        args += ["-ForceRebuild"]
    if not bool_value(payload.get("syncBeforeBuild", True)):
        args += ["-NoSyncMetadata"]
    if not bool_value(payload.get("extendExisting", True)):
        args += ["-NoExtendExisting"]
    add_log("[SAFE] Chạy publish_depot_version.ps1 với đúng Sync/Extend/ForceRebuild/Encrypt flags từ GUI.", "step")
    return run_process(args, cwd=BASE_DIR, env=env)


def run_direct_builder(payload: Dict[str, Any], env: Dict[str, str]) -> int:
    cargo = shutil.which("cargo")
    if not cargo:
        add_log("Không tìm thấy cargo trong PATH.", "error")
        return 127
    cargo_manifest = Path(safe_str(payload.get("cargoManifest"))).expanduser()
    src_tauri = get_src_tauri_from_manifest(str(cargo_manifest))
    if not cargo_manifest.exists():
        add_log(f"Không thấy Cargo.toml: {cargo_manifest}", "error")
        return 2

    if bool_value(payload.get("syncBeforeBuild", True)) and bool_value(payload.get("uploadToHf", True)):
        code = run_sync_metadata(payload, env)
        if code != 0:
            add_log("[SYNC] Đồng bộ metadata thất bại, dừng để tránh ghi đè catalog/chunk numbering sai.", "error")
            return code

    exe = src_tauri / "target" / "release" / ("depot_builder.exe" if os.name == "nt" else "depot_builder")
    force_rebuild = bool_value(payload.get("forceRebuild", False))
    if force_rebuild or not exe.exists():
        add_log("[BUILD] Build release depot_builder...", "step")
        code = run_process([cargo, "build", "--release", "--manifest-path", str(cargo_manifest), "--bin", "depot_builder"], cwd=src_tauri, env=env)
        if code != 0:
            return code
    else:
        add_log(f"[BUILD] Dùng release builder sẵn có: {exe}", "success")

    args = [str(exe)] + build_direct_args(payload)
    add_log("[DEPOT] Chạy depot_builder local.", "step")
    code = run_process(args, cwd=src_tauri, env=env)
    if code != 0:
        return code

    if bool_value(payload.get("uploadToHf", True)):
        if not INCREMENTAL_UPLOAD_TOOL.exists():
            add_log(f"Không thấy incremental uploader: {INCREMENTAL_UPLOAD_TOOL}", "error")
            return 2
        game_id = safe_str(payload.get("gameId")).strip().lower()
        repo_prefix = safe_str(payload.get("repoPrefix")).strip().lower() or game_id
        add_log("[HF] Upload depot local từng file, xong pack nào xóa pack đó nếu không giữ local.", "step")
        upload_args = [
            find_python(),
            str(INCREMENTAL_UPLOAD_TOOL),
            "--local-depot", str(depot_out_path(payload)),
            "--repo", safe_str(payload.get("repoId")).strip(),
            "--repo-type", safe_str(payload.get("repoType"), "dataset").strip() or "dataset",
            "--prefix", repo_prefix,
        ]
        if not bool_value(payload.get("keepLocalPacks", False)):
            upload_args.append("--delete-local-packs")
        return run_process(upload_args, cwd=BASE_DIR, env=env)

    return 0


def run_preflight_task(payload: Dict[str, Any]) -> int:
    add_log("[CHECK] Kiểm tra môi trường, đường dẫn, quyền ghi...", "step")
    msgs = validate_payload(payload)
    has_error = False
    for level, msg in msgs:
        add_log(msg, level)
        if level == "error":
            has_error = True
    add_log("[CHECK] Lưu ý: token HF và Depot key không được ghi vào file, chỉ truyền qua env cho process con khi chạy.", "info")
    return 1 if has_error else 0


def run_task(action: str, payload: Dict[str, Any]) -> None:
    try:
        env = common_env(payload)
        if action == "preflight":
            code = run_preflight_task(payload)
        elif action == "sync":
            code = run_sync_metadata(payload, env)
        elif action == "publish":
            # Validate before running anything destructive/heavy.
            msgs = validate_payload(payload, for_upload=bool_value(payload.get("uploadToHf", True)))
            hard_errors = [m for lvl, m in msgs if lvl == "error"]
            for lvl, msg in msgs:
                add_log(msg, lvl)
            if hard_errors:
                add_log("Có lỗi preflight, chưa chạy build/upload.", "error")
                code = 1
            else:
                mode = safe_str(payload.get("mode"), "build-version")
                use_safe_script = bool_value(payload.get("usePublishScript", True))
                upload = bool_value(payload.get("uploadToHf", True))
                if mode == "build-version" and use_safe_script and upload:
                    code = run_publish_script(payload, env)
                else:
                    code = run_direct_builder(payload, env)
        else:
            add_log(f"Action không hợp lệ: {action}", "error")
            code = 1
    except Exception as exc:
        add_log(f"Lỗi ngoài ý muốn: {exc}", "error")
        code = 1
    finish_task(code)


def _parent_for_initial(initial: str) -> str:
    try:
        path = Path(initial).expanduser()
        if path.is_file():
            return str(path.parent)
        if path.exists():
            return str(path)
        if path.parent.exists():
            return str(path.parent)
    except Exception:
        pass
    return str(Path.home())


def browse_with_tkinter(kind: str, initial: str, title: str, file_kind: str) -> str:
    """Open a native path picker on the local machine and return selected path."""
    import tkinter as tk
    from tkinter import filedialog

    root = tk.Tk()
    root.withdraw()
    try:
        root.attributes("-topmost", True)
    except Exception:
        pass
    initial_dir = _parent_for_initial(initial)
    if kind == "directory":
        value = filedialog.askdirectory(parent=root, title=title or "Chọn thư mục", initialdir=initial_dir)
    else:
        if file_kind == "exe":
            filetypes = [("Executable files", "*.exe"), ("All files", "*.*")]
        elif file_kind == "cargo":
            filetypes = [("Cargo.toml", "Cargo.toml"), ("TOML files", "*.toml"), ("All files", "*.*")]
        else:
            filetypes = [("All files", "*.*")]
        value = filedialog.askopenfilename(parent=root, title=title or "Chọn file", initialdir=initial_dir, filetypes=filetypes)
    try:
        root.destroy()
    except Exception:
        pass
    return value or ""


def browse_with_powershell(kind: str, initial: str, title: str, file_kind: str) -> str:
    """Fallback Windows picker using PowerShell WinForms."""
    ps = find_powershell()
    if not ps:
        return ""
    initial_dir = _parent_for_initial(initial)
    if kind == "directory":
        script = f"""
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.FolderBrowserDialog
$dlg.Description = {json.dumps(title or 'Chọn thư mục')}
$dlg.SelectedPath = {json.dumps(initial_dir)}
$dlg.ShowNewFolderButton = $true
if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ [Console]::OutputEncoding=[Text.UTF8Encoding]::UTF8; Write-Output $dlg.SelectedPath }}
"""
    else:
        if file_kind == "exe":
            filt = "Executable files (*.exe)|*.exe|All files (*.*)|*.*"
        elif file_kind == "cargo":
            filt = "Cargo.toml|Cargo.toml|TOML files (*.toml)|*.toml|All files (*.*)|*.*"
        else:
            filt = "All files (*.*)|*.*"
        script = f"""
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.OpenFileDialog
$dlg.Title = {json.dumps(title or 'Chọn file')}
$dlg.InitialDirectory = {json.dumps(initial_dir)}
$dlg.Filter = {json.dumps(filt)}
if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ [Console]::OutputEncoding=[Text.UTF8Encoding]::UTF8; Write-Output $dlg.FileName }}
"""
    try:
        cp = subprocess.run([ps, "-NoProfile", "-STA", "-Command", script], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace", timeout=300)
        if cp.returncode == 0:
            return cp.stdout.strip().splitlines()[-1] if cp.stdout.strip() else ""
        add_log(f"[BROWSE] PowerShell picker lỗi: {cp.stderr.strip()}", "warn")
    except Exception as exc:
        add_log(f"[BROWSE] PowerShell picker lỗi: {exc}", "warn")
    return ""


def open_path_picker(payload: Dict[str, Any]) -> Dict[str, Any]:
    kind = safe_str(payload.get("kind"), "directory").strip().lower()
    if kind not in ("directory", "file"):
        return {"ok": False, "error": "kind phải là directory hoặc file."}
    initial = safe_str(payload.get("initial")).strip()
    title = safe_str(payload.get("title")).strip()
    file_kind = safe_str(payload.get("fileKind")).strip().lower()
    try:
        selected = browse_with_tkinter(kind, initial, title, file_kind)
    except Exception as exc:
        add_log(f"[BROWSE] tkinter picker lỗi, thử PowerShell fallback: {exc}", "warn")
        selected = browse_with_powershell(kind, initial, title, file_kind)
    if not selected:
        return {"ok": True, "cancelled": True, "path": ""}
    return {"ok": True, "cancelled": False, "path": selected}


class Handler(BaseHTTPRequestHandler):
    server_version = "0xoDepotUploader/1.0"

    def log_message(self, fmt: str, *args: Any) -> None:
        # Silence default HTTP logs.
        return

    def _send(self, status: int, body: bytes, ctype: str = "application/json; charset=utf-8") -> None:
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "http://127.0.0.1:%d" % PORT)
        self.end_headers()
        self.wfile.write(body)

    def _json(self, status: int, payload: Any) -> None:
        self._send(status, json_bytes(payload), "application/json; charset=utf-8")

    def _read_json(self) -> Dict[str, Any]:
        n = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(n) if n > 0 else b"{}"
        try:
            return json.loads(raw.decode("utf-8"))
        except Exception:
            return {}

    def do_OPTIONS(self) -> None:  # noqa: N802
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()

    def do_GET(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path
        if path in ("/", "/index.html"):
            if not GUI_FILE.exists():
                self._send(404, b"GUI file not found", "text/plain; charset=utf-8")
                return
            self._send(200, GUI_FILE.read_bytes(), "text/html; charset=utf-8")
            return
        if path == "/api/health":
            cargo = shutil.which("cargo")
            self._json(200, {
                "ok": True,
                "host": HOST,
                "port": PORT,
                "platform": platform.platform(),
                "baseDir": str(BASE_DIR),
                "python": find_python(),
                "cargo": cargo or "",
                "powershell": find_powershell() or "",
                "publishPs1": str(PUBLISH_PS1),
                "syncTool": str(SYNC_TOOL),
            })
            return
        if path == "/api/logs":
            qs = urllib.parse.parse_qs(parsed.query)
            try:
                since = int((qs.get("since") or ["0"])[0])
            except Exception:
                since = 0
            lines = [x for x in _logs if int(x.get("i", 0)) >= since]
            self._json(200, {
                "running": _running,
                "exitCode": _exit_code,
                "startedAt": _started_at,
                "taskTitle": _task_title,
                "next": (lines[-1]["i"] + 1) if lines else since,
                "lines": lines,
            })
            return
        self._send(404, b"Not found", "text/plain; charset=utf-8")

    def do_POST(self) -> None:  # noqa: N802
        global _running
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/browse":
            payload = self._read_json()
            self._json(200, open_path_picker(payload))
            return
        if parsed.path == "/api/run":
            payload = self._read_json()
            action = safe_str(payload.get("action"), "publish")
            title = safe_str(payload.get("title"), "Depot task")
            with _current_lock:
                if _running:
                    self._json(409, {"ok": False, "error": "Đang có task chạy. Hãy dừng hoặc chờ xong."})
                    return
                reset_task(title)
                t = threading.Thread(target=run_task, args=(action, payload), daemon=True)
                t.start()
            self._json(200, {"ok": True})
            return
        if parsed.path == "/api/stop":
            with _current_lock:
                proc = _current_process
            if proc and proc.poll() is None:
                add_log("[STOP] Đang dừng tiến trình...", "warn")
                terminate_process(proc)
                self._json(200, {"ok": True})
            else:
                self._json(200, {"ok": True, "message": "Không có tiến trình đang chạy."})
            return
        self._send(404, b"Not found", "text/plain; charset=utf-8")


def main() -> int:
    print(f"0xo Depot Uploader Studio")
    print(f"Serving: http://{HOST}:{PORT}")
    print(f"Base dir: {BASE_DIR}")
    if not GUI_FILE.exists():
        print(f"ERROR: GUI file not found: {GUI_FILE}", file=sys.stderr)
        return 2
    url = f"http://{HOST}:{PORT}"
    try:
        webbrowser.open(url)
    except Exception:
        pass
    httpd = ThreadingHTTPServer((HOST, PORT), Handler)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping...")
        with _current_lock:
            proc = _current_process
        if proc and proc.poll() is None:
            terminate_process(proc)
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
