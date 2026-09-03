from __future__ import annotations

import json
import os
import sys
import traceback
from dataclasses import asdict
from datetime import datetime
from pathlib import Path
from typing import Any

# IMPORTANT: this executable is a headless transport for the original
# GSE_UC_Setup Python core. It intentionally imports the original package
# instead of re-implementing its behavior in Rust.
from gse_autosetup.service import Inputs, SetupService
from gse_autosetup.core.resources import ResourceManager
from gse_autosetup.core.uc_online import UCOnlineResourceManager
from gse_autosetup.core.steamstub import SteamStubManager
from gse_autosetup.core.steamless import SteamlessManager
from gse_autosetup.save_manager.models import SaveEntry
from gse_autosetup.save_manager.service import SaveManagerService


_PROTOCOL_OUT = sys.__stdout__


def _emit(kind: str, **payload: Any) -> None:
    message = {"type": kind, **payload}
    _PROTOCOL_OUT.write(json.dumps(message, ensure_ascii=True, separators=(",", ":")) + "\n")
    _PROTOCOL_OUT.flush()


class _CapturedText:
    """Redirect accidental stdout/stderr into structured log events.

    Third-party libraries should not be able to corrupt the JSON-lines protocol.
    """

    def __init__(self, stream: str) -> None:
        self.stream = stream
        self._buffer = ""

    def write(self, value: str) -> int:
        text = str(value)
        self._buffer += text
        while "\n" in self._buffer:
            line, self._buffer = self._buffer.split("\n", 1)
            if line.strip():
                _emit("log", stream=self.stream, message=line.rstrip("\r"))
        return len(text)

    def flush(self) -> None:
        if self._buffer.strip():
            _emit("log", stream=self.stream, message=self._buffer.rstrip("\r\n"))
        self._buffer = ""


def _externalize_original_resource_helpers(resource_root: Path, state_root: Path) -> None:
    """Point original helpers at Tauri's bundled resource tree.

    SetupService already accepts ResourceManager explicitly, but a few original
    helpers (7-Zip, preserve loader, Google OAuth file discovery) use module-level
    resource helpers. Patch only their roots; the implementation remains the
    original GSE_UC_Setup code.
    """
    from gse_autosetup.core import package as package_mod
    from gse_autosetup.core import preserve_loader as preserve_mod
    from gse_autosetup.core import resources as resources_mod
    from gse_autosetup.core import tool_config as tool_config_mod
    from gse_autosetup.save_manager import google_auth as google_auth_mod

    def external_resource_path(*parts: str) -> Path:
        return resource_root.joinpath(*parts)

    package_mod.resource_path = external_resource_path
    preserve_mod.resource_path = external_resource_path
    resources_mod.app_directory = lambda: state_root
    resources_mod.runtime_resources_root = lambda: resource_root
    tool_config_mod.app_directory = lambda: state_root

    def client_secret_candidates() -> list[Path]:
        return [
            state_root / "client_secrets.json",
            state_root / "resources" / "google" / "client_secrets.json",
            resource_root / "google" / "client_secrets.json",
        ]

    google_auth_mod.client_secret_candidates = client_secret_candidates


def _resources(request: dict[str, Any]) -> tuple[ResourceManager, Path, Path]:
    resource_root = Path(str(request.get("resourceRoot") or "")).resolve()
    state_root = Path(str(request.get("stateRoot") or "")).resolve()
    if not resource_root.is_dir():
        raise FileNotFoundError(f"GSE resource root does not exist: {resource_root}")
    state_root.mkdir(parents=True, exist_ok=True)
    _externalize_original_resource_helpers(resource_root, state_root)
    manager = ResourceManager(app_dir=state_root, runtime_resources=resource_root)
    return manager, resource_root, state_root


def _steam_api_key(engine: str, config: dict[str, Any]) -> str:
    key = str(config.get("apiKey") or config.get("steamWebApiKey") or "").strip()
    if not key:
        key = os.environ.get("GSE_STEAM_WEB_API_KEY", "").strip()
    if engine == "gse" and len(key) < 8:
        raise RuntimeError("Steam Web API credential is required for GSE metadata enrichment.")
    return key


def _to_inputs(config: dict[str, Any]) -> Inputs:
    engine = str(config.get("engine") or "gse").strip().lower()
    variant = str(config.get("gseVariant") or "regular").strip().lower()
    steamstub_mode = str(config.get("steamstubMode") or "auto").strip().lower()
    return Inputs(
        appid=int(config.get("appId") or 0),
        game_folder=Path(str(config.get("gameFolder") or "")).expanduser().resolve(),
        api_key=_steam_api_key(engine, config),
        experimental=(variant == "experimental"),
        use_official_generator=bool(config.get("officialGenerator", True)),
        account_name=str(config.get("accountName") or "0xoLemon"),
        save_mode=str(config.get("saveMode") or "gse"),
        custom_save_path=str(config.get("customSavePath") or ""),
        enable_overlay=bool(config.get("overlay", False)),
        deployment_mode="preserve" if variant == "preserve" else "replace",
        enable_steamstub=(steamstub_mode != "disabled"),
        engine=engine,
        gse_variant=variant,
        network_mode=str(config.get("networkMode") or "singleplayer"),
        steamstub_mode=steamstub_mode,
        uc_spoof_appid=int(config.get("ucSpoofAppid") or 480),
        uc_plugins=tuple(str(v) for v in (config.get("ucPlugins") or [])),
        coldclient_renderer=bool(config.get("coldclientRenderer", True)),
        coldclient_extra=bool(config.get("coldclientExtra", False)),
        overlay_fps=bool(config.get("overlayFps", False)),
        overlay_frametime=bool(config.get("overlayFrametime", False)),
        overlay_playtime=bool(config.get("overlayPlaytime", False)),
        overlay_achievement_notifications=bool(config.get("overlayAchievementNotifications", True)),
        overlay_friend_notifications=bool(config.get("overlayFriendNotifications", True)),
        overlay_achievement_progress=bool(config.get("overlayAchievementProgress", False)),
        overlay_icons=bool(config.get("overlayIcons", True)),
        overlay_user_info=bool(config.get("overlayUserInfo", False)),
        overlay_show_playtime=bool(config.get("overlayShowPlaytime", False)),
        overlay_position=str(config.get("overlayPosition") or "bot_right"),
        overlay_hotkey=str(config.get("overlayHotkey") or "shift + tab"),
        overlay_font_size=float(config.get("overlayFontSize") or 20.0),
        overlay_icon_size=float(config.get("overlayIconSize") or 64.0),
        overlay_rounding=float(config.get("overlayRounding") or 10.0),
        overlay_animation=float(config.get("overlayAnimation") or 0.35),
        overlay_achievement_duration=float(config.get("overlayAchievementDuration") or 7.0),
        overlay_hook_delay=int(config.get("overlayHookDelay") or 0),
        overlay_renderer_timeout=int(config.get("overlayRendererTimeout") or 15),
        overlay_warnings=bool(config.get("overlayWarnings", True)),
        overlay_dinput_bridge=bool(config.get("overlayDinputBridge", False)),
        rune_profile=str(config.get("runeProfile") or "regular"),
        rune_username=str(config.get("runeUsername") or "RUNE"),
        rune_language=str(config.get("runeLanguage") or "english"),
        rune_unlock_all_dlcs=bool(config.get("runeUnlockAllDlcs", False)),
        rune_lobby=bool(config.get("runeLobby", True)),
        rune_overlays=bool(config.get("runeOverlays", True)),
        rune_offline=bool(config.get("runeOffline", False)),
    )


def _setup(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    resources, _resource_root, _state_root = _resources(request)
    config = dict(payload.get("config") or {})
    inputs = _to_inputs(config)
    if inputs.appid <= 0:
        raise ValueError("AppID must be a positive number.")
    if not inputs.game_folder.is_dir():
        raise ValueError("Select a valid game folder.")

    logs: list[str] = []

    def log(message: str) -> None:
        text = str(message)
        logs.append(text)
        _emit("log", message=text)

    def progress(percent: int, message: str) -> None:
        _emit("progress", percent=max(0, min(100, int(percent))), message=str(message))

    result = SetupService(log=log, progress=progress, resources=resources).run(inputs)
    return {
        "success": True,
        "gameName": result.game_name,
        "installedTargets": [str(path) for path in result.installed_targets],
        "backupManifest": str(result.backup_manifest),
        "settingsDirs": [str(path) for path in result.settings_dirs],
        "gseVersion": result.gse_version,
        "message": f"Setup complete — {result.gse_version}",
        "logs": logs,
    }


def _restore(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    resources, _resource_root, _state_root = _resources(request)
    game_folder = Path(str(payload.get("gameFolder") or "")).expanduser().resolve()
    logs: list[str] = []

    def log(message: str) -> None:
        text = str(message)
        logs.append(text)
        _emit("log", message=text)

    def progress(percent: int, message: str) -> None:
        _emit("progress", percent=max(0, min(100, int(percent))), message=str(message))

    manifest = SetupService(log=log, progress=progress, resources=resources).restore(game_folder)
    return {"message": "Original game files restored.", "manifest": str(manifest), "logs": logs}


def _save_service(state_root: Path) -> SaveManagerService:
    service = SaveManagerService(app_dir=state_root)
    # Original GoogleAuthManager derives its store from app_directory(). The
    # embedded core runs from Tauri resources, so bind the same credential store
    # semantics to the writable per-user core state directory.
    try:
        from gse_autosetup.save_manager.google_auth import GoogleAuthManager
        from gse_autosetup.save_manager.oauth_store import OAuthCredentialStore
        service.auth = GoogleAuthManager(OAuthCredentialStore(state_root / "data" / "google_oauth.bin"))
    except Exception:
        pass
    return service


def _serialize_save(entry: SaveEntry) -> dict[str, Any]:
    modified = entry.modified_at.timestamp() if isinstance(entry.modified_at, datetime) else 0
    return {
        "appId": str(entry.appid),
        "gameName": entry.game_name,
        "headerImageUrl": entry.header_image_url or "",
        "coverPath": str(entry.cover_path) if entry.cover_path else "",
        "path": str(entry.save_folder),
        "sourceRoot": str(entry.source_root),
        "sizeBytes": int(entry.size_bytes),
        "modifiedUnix": int(max(0, modified)),
        "localBackupCount": int(entry.local_backup_count),
        "cloudBackupCount": int(entry.cloud_backup_count),
    }


def _find_save_entry(service: SaveManagerService, save_path: Path, appid: int, game_name: str = "") -> SaveEntry:
    for entry in service.scan(save_path.parent, resolve_metadata=True):
        if entry.appid == appid and entry.save_folder.resolve() == save_path.resolve():
            return entry
    stat = save_path.stat()
    return SaveEntry(
        appid=appid,
        source_root=save_path.parent,
        save_folder=save_path,
        size_bytes=sum(p.stat().st_size for p in save_path.rglob("*") if p.is_file()),
        modified_at=datetime.fromtimestamp(stat.st_mtime),
        game_name=game_name or f"Steam App {appid}",
    )


def _list_saves(request: dict[str, Any], payload: dict[str, Any]) -> list[dict[str, Any]]:
    _resources_manager, _resource_root, state_root = _resources(request)
    root_raw = str(payload.get("root") or "").strip()
    root = Path(root_raw).expanduser() if root_raw else None
    return [_serialize_save(entry) for entry in _save_service(state_root).scan(root, resolve_metadata=True)]


def _create_snapshot(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    save_path = Path(str(payload.get("savePath") or "")).expanduser().resolve()
    appid = int(str(payload.get("appId") or "0"))
    if not save_path.is_dir() or appid <= 0:
        raise ValueError("A valid AppID save folder is required.")
    service = _save_service(state_root)
    entry = _find_save_entry(service, save_path, appid, str(payload.get("gameName") or ""))
    archive = service.backup(entry)
    return {"path": str(archive)}


def _list_backups(request: dict[str, Any], payload: dict[str, Any]) -> list[dict[str, Any]]:
    _resources_manager, _resource_root, state_root = _resources(request)
    appid = int(payload.get("appId") or 0)
    service = _save_service(state_root)
    rows: list[dict[str, Any]] = []
    for archive in service.backups.list_backups(appid):
        manifest = service.backups.read_manifest(archive)
        sidecar = archive.with_suffix(".json")
        meta = json.loads(sidecar.read_text(encoding="utf-8")) if sidecar.is_file() else {}
        rows.append({"path": str(archive), "manifest": manifest, "metadata": meta})
    return rows


def _read_backup(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    archive = Path(str(payload.get("archive") or "")).expanduser().resolve()
    service = _save_service(state_root)
    return service.backups.read_manifest(archive)


def _restore_save_backup(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    archive = Path(str(payload.get("archive") or "")).expanduser().resolve()
    target_root = Path(str(payload.get("targetRoot") or "")).expanduser().resolve()
    expected = payload.get("appId")
    result = _save_service(state_root).restore(archive, target_root, appid=int(expected) if expected is not None else None)
    return {
        "appId": result.appid,
        "restoredFolder": str(result.restored_folder),
        "safetyBackup": str(result.safety_backup) if result.safety_backup else "",
    }


def _drive_status(request: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    return _save_service(state_root).drive_status()


def _connect_drive(request: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    return _save_service(state_root).connect_drive()


def _disconnect_drive(request: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    _save_service(state_root).disconnect_drive()
    return {"connected": False}


def _backup_save_to_drive(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    save_path = Path(str(payload.get("savePath") or "")).expanduser().resolve()
    appid = int(payload.get("appId") or 0)
    service = _save_service(state_root)
    entry = _find_save_entry(service, save_path, appid, str(payload.get("gameName") or ""))
    archive, uploaded = service.backup_to_drive(entry, progress=lambda p: _emit("progress", percent=p, message=f"Google Drive upload {p}%"))
    return {"archive": str(archive), "remote": uploaded}


def _list_cloud_backups(request: dict[str, Any], payload: dict[str, Any]) -> list[dict[str, Any]]:
    _resources_manager, _resource_root, state_root = _resources(request)
    appid = payload.get("appId")
    return _save_service(state_root).cloud_backups(int(appid) if appid is not None else None)


def _restore_cloud_backup(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    _resources_manager, _resource_root, state_root = _resources(request)
    service = _save_service(state_root)
    result = service.restore_from_drive(
        str(payload.get("fileId") or ""),
        str(payload.get("fileName") or "backup.zip"),
        Path(str(payload.get("targetRoot") or "")).expanduser().resolve(),
        appid=int(payload.get("appId") or 0),
        progress=lambda p: _emit("progress", percent=p, message=f"Google Drive download {p}%"),
    )
    return {
        "appId": result.appid,
        "restoredFolder": str(result.restored_folder),
        "safetyBackup": str(result.safety_backup) if result.safety_backup else "",
    }


def _check_updates(request: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
    resources, _resource_root, _state_root = _resources(request)
    status: dict[str, str] = {}
    checks = [
        ("gse", lambda: SetupService(resources=resources).check_latest_release().tag),
        ("uc", lambda: UCOnlineResourceManager(resources=resources).latest_release().tag),
        ("rune", lambda: SteamStubManager(resources=resources).latest_release().tag),
        ("steamless", lambda: SteamlessManager(resources=resources).latest_release().tag),
    ]
    for name, callback in checks:
        try:
            status[name] = str(callback())
        except Exception as exc:
            status[name] = f"check failed: {exc}"
    status["migrate"] = "embedded / portable override"
    return status


def _clean_update_temp(request: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
    resources, _resource_root, _state_root = _resources(request)
    resources.clear_temp()
    return {"message": "Portable update temp cache cleared."}


def _clear_component_update(request: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    resources, _resource_root, _state_root = _resources(request)
    component = str(payload.get("component") or "").strip()
    if component not in {"gse", "gse_tools", "uc_online", "steamless", "rune_steamstub", "migrate_gse", "dinput"}:
        raise ValueError("Unknown GSE component.")
    resources.clear_update(component)
    return {"message": f"Cleared portable update for {component}."}


_HANDLERS = {
    "setup": _setup,
    "restore": _restore,
    "list_saves": _list_saves,
    "create_snapshot": _create_snapshot,
    "list_backups": _list_backups,
    "read_backup": _read_backup,
    "restore_save_backup": _restore_save_backup,
    "drive_status": _drive_status,
    "connect_drive": _connect_drive,
    "disconnect_drive": _disconnect_drive,
    "backup_save_to_drive": _backup_save_to_drive,
    "list_cloud_backups": _list_cloud_backups,
    "restore_cloud_backup": _restore_cloud_backup,
    "check_updates": _check_updates,
    "clean_update_temp": _clean_update_temp,
    "clear_component_update": _clear_component_update,
}


def main() -> int:
    sys.stdout = _CapturedText("stdout")
    sys.stderr = _CapturedText("stderr")
    try:
        raw = sys.stdin.read()
        request = json.loads(raw or "{}")
        command = str(request.get("command") or "")
        payload = dict(request.get("payload") or {})
        handler = _HANDLERS.get(command)
        if handler is None:
            raise ValueError(f"Unknown core command: {command}")
        result = handler(request, payload)
        _emit("result", payload=result)
        return 0
    except Exception as exc:
        _emit("error", message=str(exc), exceptionType=type(exc).__name__)
        # Full trace is a diagnostic event, not raw stderr, so the transport
        # remains parseable. It never includes the Steam API credential.
        trace = "".join(traceback.format_exception(type(exc), exc, exc.__traceback__))
        _emit("diagnostic", message=trace)
        return 1
    finally:
        try:
            sys.stdout.flush()
            sys.stderr.flush()
        except Exception:
            pass


if __name__ == "__main__":
    raise SystemExit(main())
