from __future__ import annotations

import json
import re
import shutil
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse


def _stats_node(schema: dict) -> dict:
    return schema.get("game", {}).get("availableGameStats", {}) or {}


def safe_filename(value: str) -> str:
    value = re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._")
    return value[:120] or "achievement"


def _achievement_index(schema: dict) -> dict[str, dict]:
    return {
        str(item.get("name")): item
        for item in (_stats_node(schema).get("achievements", []) or [])
        if item.get("name")
    }


def schema_to_achievements(schema: dict, localized_schemas: dict[str, dict] | None = None) -> list[dict]:
    """Convert Steam schema to current GSE achievement JSON.

    With localized_schemas, displayName/description are language maps. The base
    English schema remains the source of hidden/icon metadata.
    """
    localized_schemas = dict(localized_schemas or {})
    if "english" not in localized_schemas:
        localized_schemas["english"] = schema

    indexes = {lang: _achievement_index(loc_schema) for lang, loc_schema in localized_schemas.items()}
    result = []
    for item in _stats_node(schema).get("achievements", []) or []:
        name = str(item.get("name") or "")
        if not name:
            continue

        display_names: dict[str, str] = {}
        descriptions: dict[str, str] = {}
        for lang, index in indexes.items():
            localized = index.get(name)
            if not localized:
                continue
            display = str(localized.get("displayName") or "").strip()
            description = str(localized.get("description") or "").strip()
            if display:
                display_names[lang] = display
            # Keep empty descriptions out of the map; GSE falls back to English/empty.
            if description:
                descriptions[lang] = description

        english_display = str(item.get("displayName") or name)
        english_description = str(item.get("description") or "")
        display_names.setdefault("english", english_display)
        if english_description:
            descriptions.setdefault("english", english_description)

        result.append({
            "name": name,
            "displayName": display_names,
            "description": descriptions if descriptions else {"english": ""},
            "hidden": 1 if str(item.get("hidden", "0")) in ("1", "True", "true") else 0,
            "_icon_url": str(item.get("icon") or ""),
            "_icon_gray_url": str(item.get("icongray") or item.get("icon_gray") or ""),
        })
    return result


def schema_to_stats(schema: dict) -> list[dict]:
    mapping = {"INT": "int", "FLOAT": "float", "AVGRATE": "avgrate"}
    result = []
    for item in _stats_node(schema).get("stats", []) or []:
        name = str(item.get("name") or "")
        if not name:
            continue
        raw_type = str(item.get("type") or "INT").upper()
        stat_type = mapping.get(raw_type, "int")
        default = item.get("defaultvalue", 0)
        if isinstance(default, float) and default.is_integer():
            default = int(default)
        result.append({
            "name": name,
            "type": stat_type,
            "default": str(default),
            "global": "0",
        })
    return result


def achievement_image_count(schema: dict) -> int:
    total = 0
    for item in _stats_node(schema).get("achievements", []) or []:
        if item.get("icon"):
            total += 1
        if item.get("icongray"):
            total += 1
    return total

def _runtime_example_name(name: str) -> str:
    """Convert official *.EXAMPLE names to the runtime name GSE reads."""
    return name.replace(".EXAMPLE", "")


def _is_runtime_default_file(path: Path) -> bool:
    low = path.name.lower()
    stem = path.stem.lower()
    if path.suffix.lower() in {".md", ".markdown"}:
        return False
    if stem.startswith(("readme", "credits", "changelog", "copying")) or "license" in stem:
        return False
    return True


def materialize_canonical_defaults(example_root: Path | None, settings_dir: Path) -> list[str]:
    """Seed canonical static GSE runtime defaults without inventing game data.

    Only official static config/assets are copied. Dynamic per-game files such as
    branches.json, depots.txt, achievements.json and inventory are left to the
    official generator (or explicit API fallback). Existing/generated files win.
    """
    if example_root is None:
        return []
    root = Path(example_root)
    settings_dir = Path(settings_dir)
    if not root.is_dir():
        return []

    copied: list[str] = []
    static_files = (
        "configs.main.EXAMPLE.ini",
        "configs.user.EXAMPLE.ini",
        "configs.app.EXAMPLE.ini",
        "configs.overlay.EXAMPLE.ini",
        "account_avatar.EXAMPLE.jpg",
        "account_avatar_default.EXAMPLE.jpg",
    )
    for name in static_files:
        src = root / name
        if not src.is_file():
            continue
        rel = Path(_runtime_example_name(name))
        dst = settings_dir / rel
        if dst.exists():
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        copied.append(rel.as_posix())

    for example_dir in ("controller.EXAMPLE", "fonts.EXAMPLE", "sounds.EXAMPLE"):
        src_root = root / example_dir
        if not src_root.is_dir():
            continue
        out_root = settings_dir / _runtime_example_name(example_dir)
        for src in src_root.rglob("*"):
            if not src.is_file() or not _is_runtime_default_file(src):
                continue
            rel_inside = src.relative_to(src_root)
            dst = out_root / rel_inside
            if dst.exists():
                continue
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            copied.append((Path(_runtime_example_name(example_dir)) / rel_inside).as_posix())
    return copied



def _upsert_ini_values(path: Path, section: str, values: dict[str, str]) -> None:
    """Patch selected INI keys while preserving the generator's other content/comments."""
    path = Path(path)
    if path.is_file():
        text = path.read_text(encoding="utf-8", errors="replace")
    else:
        text = ""

    lines = text.splitlines()
    header = f"[{section}]"
    start = None
    end = len(lines)
    for i, line in enumerate(lines):
        if line.strip().lower() == header.lower():
            start = i
            for j in range(i + 1, len(lines)):
                stripped = lines[j].strip()
                if stripped.startswith("[") and stripped.endswith("]"):
                    end = j
                    break
            break

    if start is None:
        if lines and lines[-1].strip():
            lines.append("")
        lines.append(header)
        start = len(lines) - 1
        end = len(lines)

    pending = dict(values)
    key_re = re.compile(r"^\s*([^#;][^=]*?)\s*=.*$")
    for i in range(start + 1, end):
        m = key_re.match(lines[i])
        if not m:
            continue
        key = m.group(1).strip()
        for wanted in list(pending):
            if key.lower() == wanted.lower():
                lines[i] = f"{wanted}={pending.pop(wanted)}"
                break

    if pending:
        insert_at = end
        for key, value in pending.items():
            lines.insert(insert_at, f"{key}={value}")
            insert_at += 1

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def _resolve_save_path(save_mode: str, custom_save_path: str) -> str:
    mode = (save_mode or "gse").strip().lower()
    if mode == "gse":
        return ""
    if mode == "portable":
        return "./steam_settings/saves"
    if mode == "custom":
        value = (custom_save_path or "").strip().strip('"')
        if not value:
            raise ValueError("Custom save location is selected but no folder was provided.")
        return value
    raise ValueError(f"Unknown save mode: {save_mode}")


def _find_overlay_asset(root: Path, category: str, filename: str) -> Path | None:
    root = Path(root)
    candidates = [
        root / category / filename,
        root / f"{category}.EXAMPLE" / filename,
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    matches = list(root.rglob(filename)) if root.is_dir() else []
    return matches[0] if matches else None


def _copy_overlay_assets(settings_dir: Path, overlay_assets_root: Path | None) -> None:
    if overlay_assets_root is None:
        return
    root = Path(overlay_assets_root)
    mapping = [
        ("fonts", "Roboto-Medium.ttf"),
        ("sounds", "overlay_achievement_notification.wav"),
        ("sounds", "overlay_friend_notification.wav"),
    ]
    for category, filename in mapping:
        source = _find_overlay_asset(root, category, filename)
        if not source:
            continue
        dest = settings_dir / category / filename
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, dest)


def _steam_asset_filename(url: str, fallback: str) -> str:
    """Keep Steam's original content-hash filename when possible."""
    try:
        name = Path(urlparse(url).path).name
    except Exception:
        name = ""
    name = safe_filename(name)
    stem = Path(name).stem if name else ""
    # Steam achievement artwork normally uses a SHA-1-like hex content name.
    # Preserve that upstream filename; otherwise use a readable API-name fallback.
    if name and "." in name and re.fullmatch(r"[0-9a-fA-F]{32,64}", stem):
        return name
    return f"{safe_filename(fallback)}.jpg"


def write_basic_settings(
    settings_dir: Path,
    appid: int,
    schema: dict,
    account_name: str,
    image_downloader: Callable[[str, Path], bool] | None = None,
    *,
    save_mode: str = "gse",
    custom_save_path: str = "",
    enable_overlay: bool = False,
    overlay_assets_root: Path | None = None,
    network_mode: str = "singleplayer",
    overlay_fps: bool = False,
    overlay_frametime: bool = False,
    overlay_playtime: bool = False,
    overlay_achievement_notifications: bool = True,
    overlay_friend_notifications: bool = True,
    overlay_achievement_progress: bool = False,
    overlay_icons: bool = True,
    overlay_user_info: bool = False,
    overlay_show_playtime: bool = False,
    overlay_position: str = "bot_right",
    overlay_hotkey: str = "shift + tab",
    overlay_font_size: float = 20.0,
    overlay_icon_size: float = 64.0,
    overlay_rounding: float = 10.0,
    overlay_animation: float = 0.35,
    overlay_achievement_duration: float = 7.0,
    overlay_hook_delay: int = 0,
    overlay_renderer_timeout: int = 15,
    overlay_warnings: bool = True,
    localized_schemas: dict[str, dict] | None = None,
    preserve_existing_schema: bool = False,
    preserve_existing_achievements: bool | None = None,
    preserve_existing_stats: bool | None = None,
) -> dict:
    settings_dir = Path(settings_dir)
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "steam_appid.txt").write_text(str(int(appid)), encoding="utf-8")

    # configs.app.ini is one of the four canonical GSE config files. Official
    # _DEFAULT/1 normally supplies it; keep a safe public-branch minimum for
    # explicit limited-fallback/test packages that do not ship the examples.
    _upsert_ini_values(settings_dir / "configs.app.ini", "app::general", {
        "branch_name": "public",
    })

    account = (account_name or "0xoLemon").strip() or "0xoLemon"
    local_save_path = _resolve_save_path(save_mode, custom_save_path)
    user_ini = settings_dir / "configs.user.ini"
    _upsert_ini_values(user_ini, "user::general", {
        "account_name": account,
        "language": "english",
    })
    _upsert_ini_values(user_ini, "user::saves", {
        "local_save_path": local_save_path,
        "saves_folder_name": "GSE Saves",
    })

    mode = (network_mode or "singleplayer").strip().lower()
    # V1.8.4 migration: the legacy UI value "offline" used to force
    # ISteamUser::BLoggedOn() false. Treat it as the safer single-player
    # preset so existing config.ini files stop triggering false
    # "lost network connection" dialogs in games that only query login state.
    if mode == "offline":
        mode = "singleplayer"
    if mode not in {"singleplayer", "strict_offline", "lan"}:
        raise ValueError(f"Unknown GSE network mode: {network_mode}")

    connectivity = {
        # singleplayer: disable Steam networking APIs but still report the
        # emulated user as logged on. strict_offline explicitly reports
        # Steam offline. LAN keeps GSE networking enabled.
        "singleplayer": {"offline": "0", "disable_networking": "1"},
        "strict_offline": {"offline": "1", "disable_networking": "1"},
        "lan": {"offline": "0", "disable_networking": "0"},
    }[mode]
    main_ini = settings_dir / "configs.main.ini"
    _upsert_ini_values(main_ini, "main::connectivity", {
        **connectivity,
        "disable_lan_only": "0",
    })
    _upsert_ini_values(main_ini, "main::stats", {
        "record_playtime": "1" if overlay_playtime else "0",
    })

    overlay_ini = settings_dir / "configs.overlay.ini"
    allowed_positions = {"top_left", "top_center", "top_right", "bot_left", "bot_center", "bot_right"}
    position = (overlay_position or "bot_right").strip().lower()
    if position not in allowed_positions:
        position = "bot_right"
    hotkey = (overlay_hotkey or "shift + tab").strip() or "shift + tab"
    _upsert_ini_values(overlay_ini, "overlay::general", {
        "enable_experimental_overlay": "1" if enable_overlay else "0",
        "hook_delay_sec": str(max(0, int(overlay_hook_delay))),
        "renderer_detector_timeout_sec": str(max(1, int(overlay_renderer_timeout))),
        "disable_achievement_notification": "0" if overlay_achievement_notifications else "1",
        "disable_friend_notification": "0" if overlay_friend_notifications else "1",
        "disable_achievement_progress": "0" if overlay_achievement_progress else "1",
        "disable_warning_any": "0" if overlay_warnings else "1",
        "upload_achievements_icons_to_gpu": "1" if overlay_icons else "0",
        "overlay_always_show_user_info": "1" if overlay_user_info else "0",
        "overlay_always_show_fps": "1" if overlay_fps else "0",
        "overlay_always_show_frametime": "1" if overlay_frametime else "0",
        "overlay_always_show_playtime": "1" if (overlay_show_playtime or overlay_playtime) else "0",
    })
    _upsert_ini_values(overlay_ini, "overlay::appearance", {
        "Font_Override": "Roboto-Medium.ttf",
        "Font_Size": str(float(overlay_font_size)),
        "Icon_Size": str(float(overlay_icon_size)),
        "Notification_Rounding": str(float(overlay_rounding)),
        "Notification_Animation": str(float(overlay_animation)),
        "Notification_Duration_Achievement": str(float(overlay_achievement_duration)),
        "PosAchievement": position,
    })
    _upsert_ini_values(overlay_ini, "overlay::hotkeys", {
        "key_combo": hotkey,
    })
    if enable_overlay:
        _copy_overlay_assets(settings_dir, overlay_assets_root)

    # Canonical generator output wins.  Only create supported_languages.txt
    # when the generator did not provide one; never rewrite an existing file
    # from fallback Web API data.
    languages_path = settings_dir / "supported_languages.txt"
    if localized_schemas and not languages_path.is_file():
        languages: list[str] = []
        for lang in localized_schemas:
            value = str(lang).strip().lower()
            if value and value not in languages:
                languages.append(value)
        if "english" in languages:
            languages.remove("english")
        languages.insert(0, "english")

        languages_path.write_text("\n".join(languages) + "\n", encoding="utf-8")

    achievements_path = settings_dir / "achievements.json"
    stats_path = settings_dir / "stats.json"

    def existing_list_count(path: Path) -> int | None:
        if not path.is_file():
            return None
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return None
        return len(data) if isinstance(data, list) else None

    if preserve_existing_achievements is None:
        preserve_existing_achievements = preserve_existing_schema
    if preserve_existing_stats is None:
        preserve_existing_stats = preserve_existing_schema

    def existing_achievement_languages(path: Path) -> set[str]:
        if not path.is_file():
            return set()
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return set()
        if not isinstance(data, list) or not data:
            return set()
        languages: set[str] = set()
        for item in data:
            if not isinstance(item, dict):
                continue
            for key in ("displayName", "description"):
                value = item.get(key)
                if isinstance(value, dict):
                    languages.update(str(lang).strip().lower() for lang in value if str(lang).strip())
        return languages

    # Canonical official-generator output wins.  Web API localization is a
    # fallback only when the generator did not produce achievements.json.
    # Never rewrite a valid generator file merely because an external schema
    # currently exposes a different language set.

    preserved_achievements = existing_list_count(achievements_path) if preserve_existing_achievements else None
    if preserved_achievements is None:
        achievements = schema_to_achievements(schema, localized_schemas=localized_schemas)
        images_dir = settings_dir / "img"
        for achievement in achievements:
            icon_url = achievement.pop("_icon_url", "")
            gray_url = achievement.pop("_icon_gray_url", "")
            if icon_url:
                filename = _steam_asset_filename(icon_url, achievement["name"])
                path = images_dir / filename
                if path.is_file() or (image_downloader and image_downloader(icon_url, path)):
                    achievement["icon"] = f"img/{filename}"
            if gray_url:
                filename = _steam_asset_filename(gray_url, achievement["name"] + "_gray")
                path = images_dir / filename
                if path.is_file() or (image_downloader and image_downloader(gray_url, path)):
                    # Current GSE format. Older builds also accept icongray as a fallback.
                    achievement["icon_gray"] = f"img/{filename}"
        if achievements:
            # Match the current official gse_fork_tools serializer: UTF-8 JSON with
            # ensure_ascii=False. Do not rewrite a valid official achievements.json.
            achievements_path.write_text(
                json.dumps(achievements, ensure_ascii=False, indent=2), encoding="utf-8"
            )
        achievement_count = len(achievements)
    else:
        achievement_count = preserved_achievements

    preserved_stats = existing_list_count(stats_path) if preserve_existing_stats else None
    if preserved_stats is None:
        stats = schema_to_stats(schema)
        if stats:
            stats_path.write_text(
                json.dumps(stats, ensure_ascii=False, indent=2), encoding="utf-8"
            )
        stat_count = len(stats)
    else:
        stat_count = preserved_stats

    return {"achievements": achievement_count, "stats": stat_count}
