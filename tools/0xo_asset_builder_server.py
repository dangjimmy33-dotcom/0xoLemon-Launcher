#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
0xo Asset Builder - Local GUI Server
- Serves the GUI from http://127.0.0.1:8765
- Proxies Steam API calls server-side to avoid browser CORS
- Runs steam-fetch-game-assets.ps1 and streams output to the GUI

No external Python packages required.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import signal
import subprocess
import sys
import threading
import time
import traceback
import urllib.parse
import urllib.request
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List, Optional

HOST = "127.0.0.1"
PORT = int(os.environ.get("OXO_ASSET_BUILDER_PORT", "8765"))
BASE_DIR = Path(__file__).resolve().parent
GUI_FILE = BASE_DIR / "asset-builder-gui-local.html"
PS_SCRIPT = BASE_DIR / "steam-fetch-game-assets.ps1"

current_process: Optional[subprocess.Popen[str]] = None
current_lock = threading.Lock()


def json_bytes(payload: Any) -> bytes:
    return json.dumps(payload, ensure_ascii=False, indent=None).encode("utf-8")


def find_powershell() -> Optional[str]:
    # Prefer PowerShell 7 if available, then Windows PowerShell.
    for exe in ("pwsh", "pwsh.exe", "powershell", "powershell.exe"):
        p = shutil.which(exe)
        if p:
            return p
    return None


def safe_int(value: Any, default: int, minimum: Optional[int] = None, maximum: Optional[int] = None) -> int:
    try:
        n = int(value)
    except Exception:
        n = default
    if minimum is not None:
        n = max(minimum, n)
    if maximum is not None:
        n = min(maximum, n)
    return n


def safe_str(value: Any, default: str = "") -> str:
    if value is None:
        return default
    return str(value)


def bool_flag(value: Any) -> bool:
    return bool(value) and str(value).lower() not in ("0", "false", "no", "off", "")


def process_creation_kwargs() -> Dict[str, Any]:
    if os.name == "nt":
        return {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
    return {"preexec_fn": os.setsid}



def steam_country(value: Any, default: str = "us") -> str:
    cc = safe_str(value, default).strip().lower()
    if len(cc) != 2 or not cc.isalpha():
        return default
    return cc


def fetch_steam_appdetails_with_fallback(app_id: str, language: str = "english", preferred_country: str = "us") -> Dict[str, Any]:
    preferred_country = steam_country(preferred_country, "us")
    countries: List[str] = []
    for cc in [preferred_country, "us", "sg", "gb", "jp", "kr", "tw", "hk", "th", "vn", "de", "fr", "ca", "au"]:
        cc = steam_country(cc, "us")
        if cc not in countries:
            countries.append(cc)
    languages: List[str] = []
    if language:
        languages.append(language)
    if "english" not in languages:
        languages.append("english")

    last_error = ""
    for lang in languages:
        for cc in countries:
            try:
                url = "https://store.steampowered.com/api/appdetails?" + urllib.parse.urlencode(
                    {"appids": app_id, "l": lang, "cc": cc}
                )
                req = urllib.request.Request(
                    url,
                    headers={
                        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) 0xoAssetBuilderLocal/1.0",
                        "Accept": "application/json,text/plain,*/*",
                        "Accept-Language": "en-US,en;q=0.9",
                        "Cookie": "birthtime=568022401; lastagecheckage=1-January-1988; mature_content=1",
                    },
                )
                with urllib.request.urlopen(req, timeout=25) as resp:
                    payload = json.loads(resp.read().decode("utf-8", errors="replace"))
                entry = payload.get(app_id)
                if entry and entry.get("success") and entry.get("data"):
                    return {"ok": True, "raw": payload, "data": entry.get("data", {}), "country": cc, "language": lang}
                last_error = f"cc={cc} l={lang}: success=false or no data"
            except Exception as exc:
                last_error = f"cc={cc} l={lang}: {exc}"
    return {"ok": False, "error": last_error or f"Không tìm thấy App ID {app_id}."}





def _nul_device() -> str:
    return "NUL" if os.name == "nt" else "/dev/null"


def _run_capture(args: List[str], cwd: Optional[str] = None, timeout: Optional[int] = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        stdin=subprocess.DEVNULL,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        check=False,
        **process_creation_kwargs(),
    )


def _video_decode_errors(ffmpeg: str, video_path: Path) -> str:
    """Return ffmpeg decode error text. Empty string means clean enough."""
    try:
        cp = _run_capture(
            [ffmpeg, "-hide_banner", "-v", "error", "-i", str(video_path), "-map", "0:v:0", "-f", "null", _nul_device()],
            timeout=180,
        )
    except subprocess.TimeoutExpired:
        return "ffmpeg decode check timeout"
    out = "\n".join(x for x in (cp.stdout.strip(), cp.stderr.strip()) if x)
    if cp.returncode != 0 and not out:
        out = f"ffmpeg exited with code {cp.returncode}"
    return out.strip()


def _repair_video(ffmpeg: str, video_path: Path, writer) -> bool:
    tmp = video_path.with_name(video_path.stem + ".repaired.tmp.mp4")
    try:
        if tmp.exists():
            tmp.unlink()
    except Exception:
        pass

    writer(f"[VIDEO] Repair MP4: {video_path}")
    args = [
        ffmpeg,
        "-hide_banner",
        "-y",
        "-v", "error",
        "-fflags", "+genpts+discardcorrupt",
        "-err_detect", "ignore_err",
        "-i", str(video_path),
        "-map", "0:v:0",
        "-map", "0:a?",
        "-sn", "-dn",
        "-c:v", "libx264",
        "-preset", "veryfast",
        "-crf", "32",
        "-pix_fmt", "yuv420p",
        "-profile:v", "main",
        "-level", "4.0",
        "-movflags", "+faststart",
        "-c:a", "aac",
        "-b:a", "96k",
        str(tmp),
    ]
    try:
        cp = _run_capture(args, timeout=600)
    except subprocess.TimeoutExpired:
        writer("[VIDEO] Repair timeout, giữ nguyên file cũ.")
        try:
            if tmp.exists(): tmp.unlink()
        except Exception: pass
        return False
    if cp.returncode != 0 or not tmp.exists() or tmp.stat().st_size < 16_384:
        err = (cp.stderr or cp.stdout or "").strip().splitlines()
        writer("[VIDEO] Repair thất bại: " + (err[-1] if err else f"exit {cp.returncode}"))
        try:
            if tmp.exists(): tmp.unlink()
        except Exception: pass
        return False

    err_after = _video_decode_errors(ffmpeg, tmp)
    if err_after:
        writer("[VIDEO] File sau repair vẫn báo lỗi decode, bỏ thay thế.")
        writer("[VIDEO] " + err_after.splitlines()[0][:220])
        try:
            tmp.unlink()
        except Exception: pass
        return False

    bak = video_path.with_name(video_path.name + ".bak")
    try:
        if bak.exists():
            bak.unlink()
        video_path.replace(bak)
        tmp.replace(video_path)
        try:
            bak.unlink()
        except Exception:
            pass
        writer(f"[VIDEO] Đã làm sạch MP4: {video_path.name}")
        return True
    except Exception as exc:
        writer(f"[VIDEO] Không thay được file MP4: {exc}")
        try:
            if tmp.exists(): tmp.unlink()
        except Exception: pass
        return False


def preflight_videos_before_build(assets_root: str, writer) -> None:
    """Remove stale temp video files and repair MP4 files that ffmpeg would warn about during build."""
    root = Path(assets_root).expanduser()
    if not assets_root or not root.exists():
        writer(f"[VIDEO] Bỏ qua kiểm tra video: assetsRoot không tồn tại ({assets_root}).")
        return

    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        writer("[VIDEO] Không tìm thấy ffmpeg, bỏ qua kiểm tra/sửa MP4 trước build.")
        return

    writer(f"[VIDEO] Kiểm tra MP4 trước build: {root}")

    # Dọn rác từ các lần tải/chuyển mã bị dừng giữa chừng. Một số packer quét *.mp4 đệ quy nên file *.source.mp4/*.tmp.mp4 cũng có thể bị đem đi đóng gói.
    trash_patterns = ["*.source.mp4", "*.tmp.mp4", "*.repaired.tmp.mp4", "*.part", "*.download"]
    removed = 0
    for pat in trash_patterns:
        for f in root.rglob(pat):
            try:
                f.unlink()
                removed += 1
            except Exception:
                pass
    if removed:
        writer(f"[VIDEO] Đã xóa {removed} file video tạm/source cũ.")

    mp4s = [p for p in root.rglob("*.mp4") if p.is_file()]
    if not mp4s:
        writer("[VIDEO] Không có MP4 nào trong assetsRoot.")
        return

    repaired = 0
    bad = 0
    for i, video in enumerate(mp4s, 1):
        writer(f"[VIDEO] Check {i}/{len(mp4s)}: {video.name}")
        errors = _video_decode_errors(ffmpeg, video)
        if not errors:
            continue
        bad += 1
        first = errors.splitlines()[0][:220]
        writer(f"[VIDEO] Phát hiện MP4 có cảnh báo decode: {first}")
        if _repair_video(ffmpeg, video, writer):
            repaired += 1

    if bad == 0:
        writer(f"[VIDEO] MP4 sạch: {len(mp4s)} file.")
    else:
        writer(f"[VIDEO] Kết quả preflight: {bad} file có cảnh báo, sửa được {repaired} file.")



def _read_json_file(path: Path) -> Optional[Dict[str, Any]]:
    try:
        if path.exists() and path.is_file():
            return json.loads(path.read_text(encoding="utf-8-sig", errors="replace"))
    except Exception:
        return None
    return None



def _count_files(path: Path, pattern: str) -> int:
    try:
        if path.exists() and path.is_dir():
            return sum(1 for x in path.rglob(pattern) if x.is_file())
    except Exception:
        pass
    return 0


def list_game_asset_dirs(assets_root: str) -> List[Dict[str, Any]]:
    """Return top-level fetched game folders under AssetsRoot for the GUI picker."""
    root = Path(assets_root).expanduser()
    if not assets_root or not root.exists() or not root.is_dir():
        return []

    rows: List[Dict[str, Any]] = []
    for child in sorted([x for x in root.iterdir() if x.is_dir()], key=lambda x: x.name.casefold()):
        # Ignore temporary hold directories created by single-game build isolation.
        if ".__0xo_build_hold_" in child.name or child.name.startswith("."):
            continue

        meta_dir = child / "details" / "metadata"
        meta = _read_json_file(meta_dir / "game-detail.normalized.json") or {}
        media = _read_json_file(meta_dir / "media-manifest.json") or []

        title = str(meta.get("title") or child.name)
        app_id = meta.get("appId", "")
        try:
            app_id = str(app_id) if app_id not in (None, "") else ""
        except Exception:
            app_id = ""

        screenshots = _count_files(child / "details" / "screenshots", "*.webp")
        videos = _count_files(child / "details" / "videos", "*.mp4")
        achievements = _count_files(child / "achievement_images", "*.*")
        media_count = len(media) if isinstance(media, list) else 0
        try:
            mtime = child.stat().st_mtime
        except Exception:
            mtime = 0

        rows.append(
            {
                "folder": child.name,
                "title": title,
                "appId": app_id,
                "path": str(child.resolve()),
                "modified": mtime,
                "screenshots": screenshots,
                "videos": videos,
                "achievements": achievements,
                "mediaEntries": media_count,
                "hasMetadata": bool(meta),
            }
        )

    rows.sort(key=lambda r: (r.get("modified") or 0), reverse=True)
    return rows


def find_game_asset_dir(assets_root: str, game_name: str, app_id: str = "", asset_folder: str = "") -> Optional[Path]:
    """Find the selected game's asset folder under assets_root.

    Fetch script writes to: <AssetsRoot>/<GameName>. We still support case-insensitive
    lookup and metadata lookup so renames do not break single-game build.
    """
    root = Path(assets_root).expanduser()
    if not root.exists() or not root.is_dir():
        return None

    folder = Path((asset_folder or "").strip()).name
    if folder:
        exact_folder = root / folder
        if exact_folder.exists() and exact_folder.is_dir():
            return exact_folder.resolve()

    name = (game_name or "").strip()
    if name:
        exact = root / name
        if exact.exists() and exact.is_dir():
            return exact.resolve()
        lower = name.casefold()
        for child in root.iterdir():
            if child.is_dir() and child.name.casefold() == lower:
                return child.resolve()

    # Metadata fallback by appId/title.
    for child in root.iterdir():
        if not child.is_dir():
            continue
        meta = _read_json_file(child / "details" / "metadata" / "game-detail.normalized.json")
        if not meta:
            meta = _read_json_file(child / "meta" / "game-detail.normalized.json")
        if not meta:
            meta = _read_json_file(child / "details" / "game-detail.normalized.json")
        if not meta:
            continue
        if app_id and str(meta.get("appId", "")).strip() == str(app_id).strip():
            return child.resolve()
        title = str(meta.get("title", "")).strip()
        if name and title.casefold() == name.casefold():
            return child.resolve()
    return None




def _media_manifest_has_remote_entries(path: Path) -> bool:
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig", errors="replace"))
        return isinstance(data, list) and any(isinstance(x, dict) and (x.get("sourceUrl") or x.get("url")) for x in data)
    except Exception:
        return False


def _metadata_needs_refresh(game_dir: Path) -> bool:
    meta_dir = game_dir / "details" / "metadata"
    detail = meta_dir / "game-detail.normalized.json"
    media = meta_dir / "media-manifest.json"
    if not detail.exists() or not media.exists():
        return True
    meta = _read_json_file(detail) or {}
    if not str(meta.get("shortDescription") or meta.get("detailedDescriptionHtml") or "").strip():
        return True
    return not _media_manifest_has_remote_entries(media)


def ensure_remote_metadata_before_build(
    assets_root: str,
    target: Path,
    game_name: str,
    app_id: str,
    writer,
    options: Optional[Dict[str, Any]] = None,
) -> None:
    if not _metadata_needs_refresh(target):
        writer("[META] Metadata remote đã có, bỏ qua fetch lại.")
        return

    if not app_id or not str(app_id).strip().isdigit():
        writer("[META] Thiếu Steam AppID nên không thể tự fetch metadata; build sẽ dùng fallback tối thiểu.")
        return

    pwsh = find_powershell()
    if not pwsh or not PS_SCRIPT.exists():
        writer("[META] Thiếu PowerShell hoặc steam-fetch-game-assets.ps1; build sẽ dùng fallback tối thiểu.")
        return

    options = options or {}
    title = target.name or game_name or str(app_id)
    writer(f"[META] Metadata thiếu/không đủ, tự fetch remote metadata cho: {title} ({app_id})")
    args = [
        pwsh,
        "-NoLogo",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        str(PS_SCRIPT),
        "-AppId",
        str(app_id).strip(),
        "-GameName",
        title,
        "-AssetsRoot",
        assets_root,
        "-Language",
        safe_str(options.get("language"), "english"),
        "-SteamCountry",
        steam_country(options.get("steamCountry") or options.get("country") or options.get("cc"), "us"),
        "-MaxScreenshots",
        str(safe_int(options.get("maxScreenshots"), 12, 1, 100)),
        "-MaxVideos",
        str(safe_int(options.get("maxVideos"), 4, 0, 50)),
        "-ScreenshotQuality",
        str(safe_int(options.get("screenshotQuality"), 55, 1, 100)),
        "-ImageQuality",
        str(safe_int(options.get("imageQuality"), 75, 1, 100)),
        "-VideoCrf",
        str(safe_int(options.get("videoCrf"), 35, 0, 63)),
        "-VideoScale",
        safe_str(options.get("videoScale"), "720"),
        "-SkipRootImages",
    ]
    if bool_flag(options.get("noAchievements")):
        args.append("-NoAchievementImages")
    steam_api_key = safe_str(options.get("steamApiKey")).strip()
    if steam_api_key:
        args.extend(["-SteamApiKey", steam_api_key])
    sgdb_key = safe_str(options.get("steamGridDbKey")).strip()
    if sgdb_key:
        args.extend(["-SteamGridDbKey", sgdb_key])

    cp = _run_capture(args, cwd=str(BASE_DIR), timeout=600)
    for line in (cp.stdout or "").splitlines():
        if line.strip():
            writer(line)
    if cp.returncode != 0:
        err = "\n".join((cp.stderr or cp.stdout or "").splitlines()[-8:])
        writer(f"[META] Fetch metadata lỗi exit {cp.returncode}; build sẽ dùng fallback tối thiểu.")
        if err.strip():
            writer(err.strip())
    else:
        writer("[META] Đã bảo đảm metadata remote-only trước khi build.")

def preflight_single_game_build(assets_root: str, game_name: str, app_id: str, writer, asset_folder: str = "", options: Optional[Dict[str, Any]] = None):
    """Temporarily isolate assetsRoot so asset_pack_builder sees only one game.

    The Rust builder currently scans the whole assets directory and has no per-game
    argument here. To build one pack safely, we move the other top-level asset folders
    to a sibling hold directory, run the builder, then restore everything in finally.
    """
    root = Path(assets_root).expanduser().resolve()
    if not assets_root or not root.exists() or not root.is_dir():
        writer(f"[BUILD] Không tìm thấy Assets Root: {assets_root}")
        return None

    target = find_game_asset_dir(str(root), game_name, app_id, asset_folder)
    if not target:
        writer(f"[BUILD] Không tìm thấy thư mục asset cho game: {game_name or app_id}")
        writer("[BUILD] Sẽ build toàn bộ để tránh làm sai dữ liệu.")
        preflight_videos_before_build(str(root), writer)
        return None

    target = target.resolve()
    writer(f"[BUILD] Chế độ build 1 game: {target.name}")
    ensure_remote_metadata_before_build(str(root), target, game_name, app_id, writer, options)
    preflight_videos_before_build(str(target), writer)

    # Note: We now pass target dir directly to asset_pack_builder via --source.
    # We no longer move/hide other directories which caused WinError 5 locks.
    writer("[BUILD] Đã truyền trực tiếp thư mục game cho builder.")

    def cleanup(write):
        pass

    return cleanup


def stop_process() -> bool:
    global current_process
    with current_lock:
        proc = current_process
    if not proc or proc.poll() is not None:
        return False

    try:
        if os.name == "nt":
            subprocess.run(
                ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        else:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        return True
    except Exception:
        try:
            proc.terminate()
            return True
        except Exception:
            return False


class Handler(BaseHTTPRequestHandler):
    server_version = "0xoAssetBuilderLocal/1.0"
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args: Any) -> None:
        # Keep the terminal clean; GUI has its own log.
        return

    def _send_bytes(self, status: int, body: bytes, content_type: str = "application/octet-stream") -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _send_json(self, payload: Any, status: int = 200) -> None:
        self._send_bytes(status, json_bytes(payload), "application/json; charset=utf-8")

    def _read_json_body(self) -> Dict[str, Any]:
        try:
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length) if length else b"{}"
            if not raw:
                return {}
            return json.loads(raw.decode("utf-8"))
        except Exception as exc:
            raise ValueError(f"Invalid JSON body: {exc}")

    def do_GET(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path in ("/", "/index.html"):
            if not GUI_FILE.exists():
                self._send_json({"ok": False, "error": f"Missing GUI file: {GUI_FILE}"}, 500)
                return
            self._send_bytes(200, GUI_FILE.read_bytes(), "text/html; charset=utf-8")
            return

        if path == "/api/status":
            self._send_json(
                {
                    "ok": True,
                    "server": "0xo Asset Builder Local",
                    "baseDir": str(BASE_DIR),
                    "psScriptExists": PS_SCRIPT.exists(),
                    "psScript": str(PS_SCRIPT),
                    "powershell": find_powershell(),
                    "ffmpeg": shutil.which("ffmpeg"),
                    "cargo": shutil.which("cargo"),
                    "platform": platform.platform(),
                }
            )
            return

        if path == "/api/steam/validate":
            qs = urllib.parse.parse_qs(parsed.query)
            app_id = (qs.get("appId") or qs.get("appid") or [""])[0].strip()
            language = (qs.get("language") or qs.get("l") or ["english"])[0].strip() or "english"
            if not app_id.isdigit():
                self._send_json({"ok": False, "error": "App ID phải là số."}, 400)
                return
            try:
                country = (qs.get("country") or qs.get("cc") or ["us"])[0].strip() or "us"
                result = fetch_steam_appdetails_with_fallback(app_id, language, country)
                if not result.get("ok"):
                    self._send_json({"ok": False, "error": result.get("error") or f"Không tìm thấy App ID {app_id}."})
                    return
                payload = result.get("raw", {})
                data = result.get("data", {})
                self._send_json(
                    {
                        "ok": True,
                        "appId": app_id,
                        "name": data.get("name", ""),
                        "headerImage": data.get("header_image", ""),
                        "type": data.get("type", ""),
                        "isFree": data.get("is_free", False),
                        "releaseDate": data.get("release_date", {}),
                        "country": result.get("country", ""),
                        "language": result.get("language", ""),
                        "raw": payload,
                    }
                )
            except Exception as exc:
                self._send_json(
                    {
                        "ok": False,
                        "error": "Không thể gọi Steam API từ backend local.",
                        "detail": str(exc),
                    },
                    502,
                )
            return

        if path == "/api/assets/list":
            qs = urllib.parse.parse_qs(parsed.query)
            assets_root = (qs.get("assetsRoot") or qs.get("root") or [r"E:\007Launcher\src\assets"])[0].strip()
            try:
                rows = list_game_asset_dirs(assets_root)
                self._send_json({"ok": True, "assetsRoot": assets_root, "games": rows})
            except Exception as exc:
                self._send_json({"ok": False, "error": f"Không đọc được Assets Root: {exc}"}, 500)
            return

        self._send_json({"ok": False, "error": "Not found"}, 404)

    def do_POST(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path == "/api/stop":
            stopped = stop_process()
            self._send_json({"ok": True, "stopped": stopped})
            return

        if path == "/api/run/fetch":
            try:
                body = self._read_json_body()
            except ValueError as exc:
                self._send_json({"ok": False, "error": str(exc)}, 400)
                return
            self._stream_fetch(body)
            return

        if path == "/api/run/build":
            try:
                body = self._read_json_body()
            except ValueError as exc:
                self._send_json({"ok": False, "error": str(exc)}, 400)
                return
            self._stream_build(body)
            return

        self._send_json({"ok": False, "error": "Not found"}, 404)

    def _start_stream(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Cache-Control", "no-cache, no-store")
        self.send_header("Connection", "close")
        self.send_header("X-Accel-Buffering", "no")
        self.end_headers()
        self.close_connection = True

    def _write_line(self, line: str) -> None:
        try:
            self.wfile.write((line.rstrip("\r\n") + "\n").encode("utf-8", errors="replace"))
            self.wfile.flush()
        except BrokenPipeError:
            raise

    def _run_and_stream(self, args: List[str], cwd: Optional[str] = None, title: str = "Task", preflight=None) -> None:
        global current_process
        self._start_stream()

        with current_lock:
            if current_process and current_process.poll() is None:
                self._write_line("[ERROR] Đang có tác vụ khác chạy. Hãy bấm Dừng hoặc chờ xong.")
                return

        self._write_line(f"[INFO] ▶ {title}")
        self._write_line(f"[INFO] Thư mục tool: {BASE_DIR}")
        if cwd:
            self._write_line(f"[INFO] Working directory: {cwd}")
        self._write_line("[INFO] Bắt đầu chạy tiến trình local...")

        env = os.environ.copy()
        env["PYTHONIOENCODING"] = "utf-8"
        env["PYTHONUTF8"] = "1"

        start = time.time()
        cleanup = None
        try:
            if preflight is not None:
                maybe_cleanup = preflight(self._write_line)
                if callable(maybe_cleanup):
                    cleanup = maybe_cleanup

            proc = subprocess.Popen(
                args,
                cwd=cwd or str(BASE_DIR),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                env=env,
                **process_creation_kwargs(),
            )
            with current_lock:
                current_process = proc

            assert proc.stdout is not None
            for out_line in proc.stdout:
                self._write_line(out_line.rstrip("\r\n"))

            rc = proc.wait()
            elapsed = time.time() - start
            if rc == 0:
                self._write_line(f"[DONE] Hoàn tất trong {elapsed:.1f}s.")
            else:
                self._write_line(f"[ERROR] Tiến trình kết thúc với mã lỗi {rc} sau {elapsed:.1f}s.")
        except BrokenPipeError:
            # Browser disconnected; leave process running unless user clicks Stop.
            pass
        except Exception:
            self._write_line("[ERROR] Lỗi backend local:")
            self._write_line(traceback.format_exc())
        finally:
            if cleanup is not None:
                try:
                    cleanup(self._write_line)
                except Exception:
                    try:
                        self._write_line("[BUILD] Lỗi khi restore asset sau build:")
                        self._write_line(traceback.format_exc())
                    except Exception:
                        pass
            with current_lock:
                if current_process and current_process.poll() is not None:
                    current_process = None

    def _stream_fetch(self, body: Dict[str, Any]) -> None:
        pwsh = find_powershell()
        if not pwsh:
            self._send_json({"ok": False, "error": "Không tìm thấy PowerShell/pwsh trong PATH."}, 500)
            return
        if not PS_SCRIPT.exists():
            self._send_json({"ok": False, "error": f"Không thấy file script: {PS_SCRIPT}"}, 500)
            return

        app_id = safe_str(body.get("appId")).strip()
        game_name = safe_str(body.get("gameName")).strip()
        if not app_id.isdigit() or not game_name:
            self._send_json({"ok": False, "error": "Thiếu App ID hoặc tên game."}, 400)
            return

        args = [
            pwsh,
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(PS_SCRIPT),
            "-AppId",
            app_id,
            "-GameName",
            game_name,
            "-AssetsRoot",
            safe_str(body.get("assetsRoot"), r"E:\007Launcher\src\assets"),
            "-Language",
            safe_str(body.get("language"), "english"),
            "-SteamCountry",
            steam_country(body.get("steamCountry") or body.get("country") or body.get("cc"), "us"),
            "-MaxScreenshots",
            str(safe_int(body.get("maxScreenshots"), 10, 1, 100)),
            "-MaxVideos",
            str(safe_int(body.get("maxVideos"), 3, 0, 50)),
            "-ScreenshotQuality",
            str(safe_int(body.get("screenshotQuality"), 55, 1, 100)),
            "-ImageQuality",
            str(safe_int(body.get("imageQuality"), 75, 1, 100)),
            "-VideoCrf",
            str(safe_int(body.get("videoCrf"), 35, 0, 63)),
            "-VideoScale",
            safe_str(body.get("videoScale"), "720"),
            "-PreserveRootAssets",
        ]
        if bool_flag(body.get("noAchievements")):
            args.append("-NoAchievementImages")
        if bool_flag(body.get("overwrite")):
            args.append("-Overwrite")
        if bool_flag(body.get("cookMetadataAssets")):
            args.append("-CookMetadataAssets")
        steam_api_key = safe_str(body.get("steamApiKey")).strip()
        if steam_api_key:
            args.extend(["-SteamApiKey", steam_api_key])
        sgdb_key = safe_str(body.get("steamGridDbKey")).strip()
        if sgdb_key:
            args.extend(["-SteamGridDbKey", sgdb_key])

        self._run_and_stream(args, cwd=str(BASE_DIR), title=f"Tải tài sản: {game_name} ({app_id})")

    def _stream_build(self, body: Dict[str, Any]) -> None:
        project_root = safe_str(body.get("projectRoot"), r"E:\007Launcher\src-tauri").strip()
        cargo = shutil.which("cargo")
        if not cargo:
            self._send_json({"ok": False, "error": "Không tìm thấy cargo trong PATH."}, 500)
            return
        if not project_root:
            self._send_json({"ok": False, "error": "Thiếu thư mục src-tauri/project root."}, 400)
            return
        if not Path(project_root).exists():
            self._send_json({"ok": False, "error": f"Thư mục không tồn tại: {project_root}"}, 400)
            return
        assets_root = safe_str(body.get("assetsRoot"), r"E:\007Launcher\src\assets").strip()
        build_mode = safe_str(body.get("buildMode"), "single").strip().lower()
        game_name = safe_str(body.get("gameName"), "").strip()
        app_id = safe_str(body.get("appId"), "").strip()
        asset_folder = safe_str(body.get("assetFolder"), "").strip()
        output_path = str(Path(project_root) / "assets" / "catalog.0xo")

        if build_mode == "all":
            title = "Xây dựng tất cả gói asset"
            args = [cargo, "run", "--bin", "asset_pack_builder", "--", "--source", assets_root, "--output", output_path]
            preflight = lambda write: preflight_videos_before_build(assets_root, write)
        else:
            title = f"Xây dựng gói asset: {asset_folder or game_name or app_id or 'game hiện tại'}"
            target_dir = find_game_asset_dir(assets_root, game_name, app_id, asset_folder)
            if target_dir:
                args = [cargo, "run", "--bin", "asset_pack_builder", "--", "--source", str(target_dir), "--output", output_path]
            else:
                args = [cargo, "run", "--bin", "asset_pack_builder", "--", "--source", assets_root, "--output", output_path]
            preflight = lambda write: preflight_single_game_build(assets_root, game_name, app_id, write, asset_folder, body)

        self._run_and_stream(
            args,
            cwd=project_root,
            title=title,
            preflight=preflight,
        )


def main() -> int:
    if not GUI_FILE.exists():
        print(f"Missing GUI file: {GUI_FILE}", file=sys.stderr)
        return 1
    httpd = ThreadingHTTPServer((HOST, PORT), Handler)
    url = f"http://{HOST}:{PORT}/"
    print("=" * 70)
    print("0xo Asset Builder Local GUI")
    print(f"URL       : {url}")
    print(f"Tool dir  : {BASE_DIR}")
    print(f"PS script : {PS_SCRIPT} ({'OK' if PS_SCRIPT.exists() else 'MISSING'})")
    print(f"PowerShell: {find_powershell() or 'MISSING'}")
    print(f"ffmpeg    : {shutil.which('ffmpeg') or 'MISSING'}")
    print("Nhấn Ctrl+C để tắt server.")
    print("=" * 70)
    try:
        threading.Timer(0.7, lambda: webbrowser.open(url)).start()
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nĐang tắt server...")
        stop_process()
        httpd.shutdown()
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
