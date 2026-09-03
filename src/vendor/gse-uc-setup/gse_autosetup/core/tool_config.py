from __future__ import annotations

import base64
import configparser
import ctypes
import os
import sys
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path


@dataclass
class ToolConfig:
    last_appid: str = ""
    last_game_folder: str = ""
    account_name: str = "0xoLemon"
    save_mode: str = "gse"
    custom_save_path: str = ""
    gse_build: str = "regular"
    overlay: bool = False
    official_generator: bool = True
    deployment_mode: str = "replace"
    steamstub: bool = True
    engine: str = "gse"
    gse_variant: str = "regular"
    network_mode: str = "singleplayer"
    steamstub_mode: str = "auto"
    uc_spoof_appid: int = 480
    uc_plugins: str = ""
    coldclient_renderer: bool = True
    coldclient_extra: bool = False
    reduced_motion: bool = False
    overlay_fps: bool = False
    overlay_frametime: bool = False
    overlay_playtime: bool = False
    overlay_achievement_notifications: bool = True
    overlay_friend_notifications: bool = True
    overlay_achievement_progress: bool = False
    overlay_icons: bool = True
    overlay_user_info: bool = False
    overlay_show_playtime: bool = False
    overlay_position: str = "bot_right"
    overlay_hotkey: str = "shift + tab"
    overlay_font_size: float = 20.0
    overlay_icon_size: float = 64.0
    overlay_rounding: float = 10.0
    overlay_animation: float = 0.35
    overlay_achievement_duration: float = 7.0
    overlay_hook_delay: int = 0
    overlay_renderer_timeout: int = 15
    overlay_warnings: bool = True
    overlay_dinput_bridge: bool = False
    rune_profile: str = "regular"
    rune_username: str = "RUNE"
    rune_language: str = "english"
    rune_unlock_all_dlcs: bool = False
    rune_lobby: bool = True
    rune_overlays: bool = True
    rune_offline: bool = False
    remember_web_api_key: bool = True
    steam_web_api_key: str = ""
    last_top_tab: str = "setup"
    save_manager_root: str = ""


def app_directory() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[2]


def default_config_path() -> Path:
    return app_directory() / "config.ini"


class _DATA_BLOB(ctypes.Structure):
    _fields_ = [("cbData", wintypes.DWORD), ("pbData", ctypes.POINTER(ctypes.c_byte))]


def _blob_from_bytes(data: bytes):
    buf = ctypes.create_string_buffer(data)
    return _DATA_BLOB(len(data), ctypes.cast(buf, ctypes.POINTER(ctypes.c_byte))), buf


def _dpapi_functions():
    crypt32 = ctypes.WinDLL("crypt32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    blob_ptr = ctypes.POINTER(_DATA_BLOB)
    crypt32.CryptProtectData.argtypes = [
        blob_ptr, wintypes.LPCWSTR, blob_ptr, ctypes.c_void_p, ctypes.c_void_p,
        wintypes.DWORD, blob_ptr,
    ]
    crypt32.CryptProtectData.restype = wintypes.BOOL
    crypt32.CryptUnprotectData.argtypes = [
        blob_ptr, ctypes.POINTER(wintypes.LPWSTR), blob_ptr, ctypes.c_void_p, ctypes.c_void_p,
        wintypes.DWORD, blob_ptr,
    ]
    crypt32.CryptUnprotectData.restype = wintypes.BOOL
    kernel32.LocalFree.argtypes = [ctypes.c_void_p]
    kernel32.LocalFree.restype = ctypes.c_void_p
    return crypt32, kernel32


def _protect_dpapi(value: str) -> str:
    raw = value.encode("utf-8")
    if os.name != "nt":
        # Development/test fallback only. Production Windows builds always use DPAPI.
        return "dev:" + base64.b64encode(raw).decode("ascii")
    in_blob, in_buf = _blob_from_bytes(raw)
    out_blob = _DATA_BLOB()
    CRYPTPROTECT_UI_FORBIDDEN = 0x1
    crypt32, kernel32 = _dpapi_functions()
    ok = crypt32.CryptProtectData(
        ctypes.byref(in_blob), "GSE Auto Setup", None, None, None,
        CRYPTPROTECT_UI_FORBIDDEN, ctypes.byref(out_blob)
    )
    _ = in_buf
    if not ok:
        raise ctypes.WinError()
    try:
        protected = ctypes.string_at(out_blob.pbData, out_blob.cbData)
        return "dpapi:" + base64.b64encode(protected).decode("ascii")
    finally:
        kernel32.LocalFree(ctypes.cast(out_blob.pbData, ctypes.c_void_p))


def _unprotect_dpapi(value: str) -> str:
    if not value:
        return ""
    if value.startswith("dev:"):
        return base64.b64decode(value[4:]).decode("utf-8")
    if not value.startswith("dpapi:"):
        return ""
    if os.name != "nt":
        return ""
    encrypted = base64.b64decode(value[6:])
    in_blob, in_buf = _blob_from_bytes(encrypted)
    out_blob = _DATA_BLOB()
    CRYPTPROTECT_UI_FORBIDDEN = 0x1
    crypt32, kernel32 = _dpapi_functions()
    ok = crypt32.CryptUnprotectData(
        ctypes.byref(in_blob), None, None, None, None,
        CRYPTPROTECT_UI_FORBIDDEN, ctypes.byref(out_blob)
    )
    _ = in_buf
    if not ok:
        return ""
    try:
        return ctypes.string_at(out_blob.pbData, out_blob.cbData).decode("utf-8")
    finally:
        kernel32.LocalFree(ctypes.cast(out_blob.pbData, ctypes.c_void_p))


def _bool(section, key: str, default: bool) -> bool:
    try:
        return section.getboolean(key, fallback=default)
    except Exception:
        return default


def _float_value(section, key: str, default: float) -> float:
    try:
        return float(section.get(key, default))
    except Exception:
        return default


def _int_value(section, key: str, default: int) -> int:
    try:
        return int(float(section.get(key, default)))
    except Exception:
        return default


def _normalize_network_mode(value: str) -> str:
    mode = (value or "singleplayer").strip().lower()
    # Legacy V1.8.x "offline" mapped to strict Steam-offline behavior.
    # Migrate it to the safer single-player preset.
    if mode == "offline":
        return "singleplayer"
    if mode in {"singleplayer", "strict_offline", "lan"}:
        return mode
    return "singleplayer"


def load_config(path: Path | None = None) -> ToolConfig:
    path = Path(path) if path else default_config_path()
    parser = configparser.ConfigParser(interpolation=None)
    if path.is_file():
        parser.read(path, encoding="utf-8")
    tool = parser["tool"] if parser.has_section("tool") else {}
    auth = parser["auth"] if parser.has_section("auth") else {}
    encrypted = str(auth.get("steam_web_api_key_dpapi", "")) if auth else ""
    remember = _bool(auth, "remember_web_api_key", True) if auth else True
    api_key = _unprotect_dpapi(encrypted) if remember and encrypted else ""
    return ToolConfig(
        last_appid=str(tool.get("last_appid", "")),
        last_game_folder=str(tool.get("last_game_folder", "")),
        account_name=str(tool.get("account_name", "0xoLemon")) or "0xoLemon",
        save_mode=str(tool.get("save_mode", "gse")),
        custom_save_path=str(tool.get("custom_save_path", "")),
        gse_build=str(tool.get("gse_build", "regular")),
        overlay=_bool(tool, "overlay", False) if tool else False,
        official_generator=_bool(tool, "official_generator", True) if tool else True,
        deployment_mode=str(tool.get("deployment_mode", "replace")),
        steamstub=_bool(tool, "steamstub", True) if tool else True,
        engine=str(tool.get("engine", "gse")),
        gse_variant=str(tool.get("gse_variant", tool.get("gse_build", "regular"))),
        network_mode=_normalize_network_mode(str(tool.get("network_mode", "singleplayer"))),
        steamstub_mode=str(tool.get("steamstub_mode", "auto")),
        uc_spoof_appid=int(str(tool.get("uc_spoof_appid", "480")) or "480"),
        uc_plugins=str(tool.get("uc_plugins", "")),
        coldclient_renderer=_bool(tool, "coldclient_renderer", True) if tool else True,
        coldclient_extra=_bool(tool, "coldclient_extra", False) if tool else False,
        reduced_motion=_bool(tool, "reduced_motion", False) if tool else False,
        overlay_fps=_bool(tool, "overlay_fps", False) if tool else False,
        overlay_frametime=_bool(tool, "overlay_frametime", False) if tool else False,
        overlay_playtime=_bool(tool, "overlay_playtime", False) if tool else False,
        overlay_achievement_notifications=_bool(tool, "overlay_achievement_notifications", True) if tool else True,
        overlay_friend_notifications=_bool(tool, "overlay_friend_notifications", True) if tool else True,
        overlay_achievement_progress=_bool(tool, "overlay_achievement_progress", False) if tool else False,
        overlay_icons=_bool(tool, "overlay_icons", True) if tool else True,
        overlay_user_info=_bool(tool, "overlay_user_info", False) if tool else False,
        overlay_show_playtime=(_bool(tool, "overlay_show_playtime", _bool(tool, "overlay_playtime", False)) if tool else False),
        overlay_position=str(tool.get("overlay_position", "bot_right")),
        overlay_hotkey=str(tool.get("overlay_hotkey", "shift + tab")),
        overlay_font_size=_float_value(tool, "overlay_font_size", 20.0) if tool else 20.0,
        overlay_icon_size=_float_value(tool, "overlay_icon_size", 64.0) if tool else 64.0,
        overlay_rounding=_float_value(tool, "overlay_rounding", 10.0) if tool else 10.0,
        overlay_animation=_float_value(tool, "overlay_animation", 0.35) if tool else 0.35,
        overlay_achievement_duration=_float_value(tool, "overlay_achievement_duration", 7.0) if tool else 7.0,
        overlay_hook_delay=_int_value(tool, "overlay_hook_delay", 0) if tool else 0,
        overlay_renderer_timeout=_int_value(tool, "overlay_renderer_timeout", 15) if tool else 15,
        overlay_warnings=_bool(tool, "overlay_warnings", True) if tool else True,
        overlay_dinput_bridge=_bool(tool, "overlay_dinput_bridge", False) if tool else False,
        rune_profile=str(tool.get("rune_profile", "regular")),
        rune_username=str(tool.get("rune_username", "RUNE")) or "RUNE",
        rune_language=str(tool.get("rune_language", "english")) or "english",
        rune_unlock_all_dlcs=_bool(tool, "rune_unlock_all_dlcs", False) if tool else False,
        rune_lobby=_bool(tool, "rune_lobby", True) if tool else True,
        rune_overlays=_bool(tool, "rune_overlays", True) if tool else True,
        rune_offline=_bool(tool, "rune_offline", False) if tool else False,
        remember_web_api_key=remember,
        steam_web_api_key=api_key,
        last_top_tab=str(tool.get("last_top_tab", "setup")),
        save_manager_root=str(tool.get("save_manager_root", "")),
    )


def save_config(config: ToolConfig, path: Path | None = None) -> Path:
    path = Path(path) if path else default_config_path()
    parser = configparser.ConfigParser(interpolation=None)
    parser["tool"] = {
        "last_appid": config.last_appid,
        "last_game_folder": config.last_game_folder,
        "account_name": config.account_name or "0xoLemon",
        "save_mode": config.save_mode,
        "custom_save_path": config.custom_save_path,
        "gse_build": config.gse_build,
        "overlay": "true" if config.overlay else "false",
        "official_generator": "true" if config.official_generator else "false",
        "deployment_mode": config.deployment_mode,
        "steamstub": "true" if config.steamstub else "false",
        "engine": config.engine,
        "gse_variant": config.gse_variant,
        "network_mode": config.network_mode,
        "steamstub_mode": config.steamstub_mode,
        "uc_spoof_appid": str(int(config.uc_spoof_appid or 480)),
        "uc_plugins": config.uc_plugins,
        "coldclient_renderer": "true" if config.coldclient_renderer else "false",
        "coldclient_extra": "true" if config.coldclient_extra else "false",
        "reduced_motion": "true" if config.reduced_motion else "false",
        "overlay_fps": "true" if config.overlay_fps else "false",
        "overlay_frametime": "true" if config.overlay_frametime else "false",
        "overlay_playtime": "true" if config.overlay_playtime else "false",
        "overlay_achievement_notifications": "true" if config.overlay_achievement_notifications else "false",
        "overlay_friend_notifications": "true" if config.overlay_friend_notifications else "false",
        "overlay_achievement_progress": "true" if config.overlay_achievement_progress else "false",
        "overlay_icons": "true" if config.overlay_icons else "false",
        "overlay_user_info": "true" if config.overlay_user_info else "false",
        "overlay_show_playtime": "true" if config.overlay_show_playtime else "false",
        "overlay_position": config.overlay_position or "bot_right",
        "overlay_hotkey": config.overlay_hotkey or "shift + tab",
        "overlay_font_size": str(float(config.overlay_font_size)),
        "overlay_icon_size": str(float(config.overlay_icon_size)),
        "overlay_rounding": str(float(config.overlay_rounding)),
        "overlay_animation": str(float(config.overlay_animation)),
        "overlay_achievement_duration": str(float(config.overlay_achievement_duration)),
        "overlay_hook_delay": str(int(config.overlay_hook_delay)),
        "overlay_renderer_timeout": str(int(config.overlay_renderer_timeout)),
        "overlay_warnings": "true" if config.overlay_warnings else "false",
        "overlay_dinput_bridge": "true" if config.overlay_dinput_bridge else "false",
        "rune_profile": config.rune_profile or "regular",
        "rune_username": config.rune_username or "RUNE",
        "rune_language": config.rune_language or "english",
        "rune_unlock_all_dlcs": "true" if config.rune_unlock_all_dlcs else "false",
        "rune_lobby": "true" if config.rune_lobby else "false",
        "rune_overlays": "true" if config.rune_overlays else "false",
        "rune_offline": "true" if config.rune_offline else "false",
        "last_top_tab": config.last_top_tab or "setup",
        "save_manager_root": config.save_manager_root,
    }
    protected = ""
    if config.remember_web_api_key and config.steam_web_api_key:
        protected = _protect_dpapi(config.steam_web_api_key)
    parser["auth"] = {
        "remember_web_api_key": "true" if config.remember_web_api_key else "false",
        "steam_web_api_key_dpapi": protected,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8", newline="\n") as f:
        parser.write(f)
        f.flush()
        os.fsync(f.fileno())
    tmp.replace(path)
    return path
