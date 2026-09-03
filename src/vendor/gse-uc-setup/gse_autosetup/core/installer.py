
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from datetime import datetime
from pathlib import Path

from .config_builder import materialize_canonical_defaults, write_basic_settings
from .models import SteamApiTarget
from .package import GSEPackage
from .official_generator import merge_settings_tree, validate_settings_mirror
from .preserve_loader import choose_proxy_name, deploy_preserve_loader, sha256_file
from .coldclient import ColdClientInstaller


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def adjacent_backup_path(api_path: Path) -> Path:
    return Path(str(Path(api_path)) + ".bak")


def fallback_adjacent_backup_path(api_path: Path) -> Path:
    return Path(str(Path(api_path)) + ".gseauto.bak")


def choose_adjacent_backup_path(api_path: Path, known_backup: Path | None = None) -> Path:
    """Choose a visible sibling backup path without clobbering unrelated user files."""
    api_path = Path(api_path)
    preferred = adjacent_backup_path(api_path)
    if not preferred.exists():
        return preferred
    if known_backup is not None and Path(known_backup).resolve() == preferred.resolve():
        return preferred
    try:
        if preferred.is_file() and api_path.is_file() and _sha256(preferred) == _sha256(api_path):
            return preferred
    except OSError:
        pass
    fallback = fallback_adjacent_backup_path(api_path)
    if not fallback.exists():
        return fallback
    if known_backup is not None and Path(known_backup).resolve() == fallback.resolve():
        return fallback
    try:
        if fallback.is_file() and api_path.is_file() and _sha256(fallback) == _sha256(api_path):
            return fallback
    except OSError:
        pass
    raise RuntimeError(
        f"Both {preferred.name} and {fallback.name} already exist and neither is a known GSE Auto Setup backup. "
        "Move or rename the unrelated backup before continuing."
    )


def ensure_adjacent_original_backup(
    api_path: Path, log=None, *, destination: Path | None = None, known_backup: Path | None = None
) -> Path:
    """Create a sibling backup once and never overwrite an unrelated backup."""
    log = log or (lambda _m: None)
    api_path = Path(api_path)
    backup = Path(destination) if destination is not None else choose_adjacent_backup_path(api_path, known_backup)
    if backup.exists():
        log(f"Adjacent Steam API backup already exists; preserving it: {backup}")
        return backup
    shutil.copy2(api_path, backup)
    log(f"Protected original Steam API beside replacement: {backup.name}")
    return backup


def backup_files(game_root: Path, paths: list[Path], backup_root: Path) -> Path:
    """Snapshot original files into one stable game-local backup.

    Existing manifest entries are never overwritten. This preserves the state
    from before the first GSE Auto Setup deployment even when Setup is run again.
    """
    game_root = Path(game_root).resolve()
    backup_root = Path(backup_root).resolve()
    backup_root.mkdir(parents=True, exist_ok=True)
    manifest = backup_root / "manifest.json"
    if manifest.is_file():
        try:
            data = json.loads(manifest.read_text(encoding="utf-8"))
        except Exception:
            data = {}
    else:
        data = {}

    entries = list(data.get("entries") or [])
    known = {str(Path(entry.get("original", "")).resolve()) for entry in entries if entry.get("original")}

    for path in paths:
        path = Path(path).resolve()
        if str(path) in known:
            continue
        try:
            rel = path.relative_to(game_root)
        except ValueError as exc:
            raise ValueError(f"Backup target is outside game folder: {path}") from exc
        # Never recursively snapshot the backup folder itself.
        if path == backup_root or backup_root in path.parents:
            continue

        backup_path = backup_root / "files" / rel
        existed = path.exists()
        kind = "dir" if path.is_dir() else "file"
        if existed:
            backup_path.parent.mkdir(parents=True, exist_ok=True)
            if path.is_dir():
                shutil.copytree(path, backup_path, dirs_exist_ok=True)
            else:
                shutil.copy2(path, backup_path)
        entries.append({
            "original": str(path),
            "backup": str(backup_path),
            "existed": existed,
            "kind": kind,
            "sha256": _sha256(path) if existed and path.is_file() else None,
        })
        known.add(str(path))

    manifest.write_text(json.dumps({
        "game_root": str(game_root),
        "created_at": data.get("created_at") or datetime.now().isoformat(timespec="seconds"),
        "storage": "game-local",
        "entries": entries,
    }, indent=2), encoding="utf-8")
    return manifest


def restore_manifest(manifest_path: Path) -> None:
    data = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    for entry in reversed(data.get("entries", [])):
        original = Path(entry["original"])
        backup = Path(entry["backup"])
        existed = bool(entry.get("existed"))
        kind = entry.get("kind", "file")
        if original.is_dir():
            shutil.rmtree(original, ignore_errors=True)
        elif original.exists():
            try:
                original.unlink()
            except OSError:
                pass
        if existed:
            original.parent.mkdir(parents=True, exist_ok=True)
            if kind == "dir":
                shutil.copytree(backup, original, dirs_exist_ok=True)
            else:
                shutil.copy2(backup, original)


def clean_previous_emulator_state(game_root: Path, log=None) -> None:
    """Cleanly revert any previous emulator artifacts and restore original game DLLs before a new deployment.

    This ensures switching between engines (GSE Regular/Experimental, ColdClient v1/v2,
    Preserve, UC Online2, RUNE) leaves no leftover proxy DLLs, conflicting configs, or
    corrupted DLLs. The persistent baseline snapshot in .gse_auto_backup is preserved.
    """
    log = log or (lambda _m: None)
    game_root = Path(game_root).resolve()
    manifest_path = game_root / ".gse_auto_backup" / "manifest.json"
    if not manifest_path.is_file():
        return

    try:
        data = json.loads(manifest_path.read_text(encoding="utf-8"))
        for entry in reversed(data.get("entries", [])):
            original = Path(entry.get("original", ""))
            backup = Path(entry.get("backup", ""))
            existed = bool(entry.get("existed"))
            kind = entry.get("kind", "file")

            # Never delete the backup directory itself during intermediate cleanup.
            if original == game_root / ".gse_auto_backup" or (game_root / ".gse_auto_backup") in original.parents:
                continue

            if existed and backup.exists():
                if kind == "dir":
                    if original.is_dir():
                        shutil.rmtree(original, ignore_errors=True)
                    original.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copytree(backup, original, dirs_exist_ok=True)
                else:
                    original.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(backup, original)
            elif not existed and original.exists():
                if original.is_dir():
                    shutil.rmtree(original, ignore_errors=True)
                else:
                    try:
                        original.unlink()
                    except OSError:
                        pass
        log("Cleaned up previous emulator deployment; restored original binaries for clean reconfiguration.")
    except Exception as exc:
        log(f"Notice during previous state cleanup: {exc}")


def _generate_interfaces(generator: Path, original_dll: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="gse-iface-") as temp:
        work = Path(temp)
        dll_copy = work / original_dll.name
        shutil.copy2(original_dll, dll_copy)
        process = subprocess.run(
            [str(generator), str(dll_copy)],
            cwd=work,
            capture_output=True,
            text=True,
            timeout=90,
        )
        if process.returncode != 0:
            raise RuntimeError(
                "generate_interfaces failed: " + (process.stderr.strip() or process.stdout.strip() or str(process.returncode))
            )
        output = work / "steam_interfaces.txt"
        if not output.is_file():
            matches = list(work.rglob("steam_interfaces.txt"))
            if matches:
                output = matches[0]
        if not output.is_file():
            raise RuntimeError("generate_interfaces completed but steam_interfaces.txt was not produced.")
        return output.read_text(encoding="utf-8", errors="replace")


class Installer:
    def __init__(self, package: GSEPackage, backup_root: Path, log=None):
        self.package = package
        self.backup_root = Path(backup_root)
        self.log = log or (lambda _msg: None)

    def install(
        self,
        game_root: Path,
        targets: list[SteamApiTarget],
        appid: int,
        schema: dict,
        account_name: str,
        image_downloader=None,
        localized_schemas: dict[str, dict] | None = None,
        experimental: bool = False,
        generated_settings: Path | None = None,
        save_mode: str = "gse",
        custom_save_path: str = "",
        enable_overlay: bool = False,
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
        extra_backup_targets: list[Path] | None = None,
        after_backup_hook=None,
        marker_extra: dict | None = None,
    ) -> Path:
        game_root = Path(game_root).resolve()
        if not targets:
            raise RuntimeError("No eligible steam_api DLL was found.")

        settings_dirs = sorted({t.path.parent / "steam_settings" for t in targets}, key=lambda p: str(p).lower())
        marker = game_root / ".gse_auto_setup.json"
        known_backup_map: dict[str, Path] = {}
        if marker.is_file():
            try:
                previous = json.loads(marker.read_text(encoding="utf-8"))
                for original, backup in (previous.get("adjacent_backups") or {}).items():
                    known_backup_map[str(Path(original).resolve())] = Path(backup).resolve()
            except Exception:
                pass

        selected_backups: dict[str, Path] = {}
        for target in targets:
            key = str(target.path.resolve())
            selected_backups[key] = choose_adjacent_backup_path(target.path, known_backup_map.get(key))

        backup_targets: list[Path] = [t.path for t in targets]
        backup_targets.extend(selected_backups.values())
        backup_targets.extend(settings_dirs)
        if extra_backup_targets:
            backup_targets.extend(Path(p) for p in extra_backup_targets)
        backup_targets.append(marker)
        manifest = backup_files(game_root, backup_targets, self.backup_root)

        try:
            for target in targets:
                key = str(target.path.resolve())
                ensure_adjacent_original_backup(
                    target.path, self.log, destination=selected_backups[key], known_backup=known_backup_map.get(key)
                )
            if after_backup_hook is not None:
                after_backup_hook()
            interfaces_by_arch: dict[str, str] = {}
            for target in targets:
                if target.arch not in interfaces_by_arch:
                    original_dll = selected_backups.get(str(target.path.resolve()), target.path)
                    if not original_dll.is_file():
                        original_dll = target.path
                    self.log(f"Generating Steam interfaces ({target.arch}) from original DLL backup...")
                    interfaces_by_arch[target.arch] = _generate_interfaces(
                        self.package.generator(target.arch), original_dll
                    )

            stats = None
            for settings in settings_dirs:
                overlay_assets = self.package.root / "steam_settings.EXAMPLE"
                seeded = materialize_canonical_defaults(
                    overlay_assets if overlay_assets.is_dir() else None, settings
                )
                if seeded:
                    self.log(f"Seeded {len(seeded)} canonical GSE runtime default file(s).")
                if generated_settings is not None:
                    self.log(f"Mirroring official generated config into {settings}...")
                    merge_settings_tree(generated_settings, settings)
                stats = write_basic_settings(
                    settings,
                    appid,
                    schema,
                    account_name,
                    image_downloader=image_downloader,
                    save_mode=save_mode,
                    custom_save_path=custom_save_path,
                    enable_overlay=enable_overlay,
                    overlay_assets_root=overlay_assets if overlay_assets.is_dir() else None,
                    network_mode=network_mode,
                    overlay_fps=overlay_fps,
                    overlay_frametime=overlay_frametime,
                    overlay_playtime=overlay_playtime,
                    overlay_achievement_notifications=overlay_achievement_notifications,
                    overlay_friend_notifications=overlay_friend_notifications,
                    overlay_achievement_progress=overlay_achievement_progress,
                    overlay_icons=overlay_icons,
                    overlay_user_info=overlay_user_info,
                    overlay_show_playtime=overlay_show_playtime,
                    overlay_position=overlay_position,
                    overlay_hotkey=overlay_hotkey,
                    overlay_font_size=overlay_font_size,
                    overlay_icon_size=overlay_icon_size,
                    overlay_rounding=overlay_rounding,
                    overlay_animation=overlay_animation,
                    overlay_achievement_duration=overlay_achievement_duration,
                    overlay_hook_delay=overlay_hook_delay,
                    overlay_renderer_timeout=overlay_renderer_timeout,
                    overlay_warnings=overlay_warnings,
                    localized_schemas=localized_schemas,
                    preserve_existing_achievements=(
                        generated_settings is not None and (Path(generated_settings) / "achievements.json").is_file()
                    ),
                    preserve_existing_stats=(
                        generated_settings is not None and (Path(generated_settings) / "stats.json").is_file()
                    ),
                )
                if generated_settings is not None:
                    validate_settings_mirror(generated_settings, settings)
                required_core = ("configs.main.ini", "configs.user.ini", "configs.app.ini", "configs.overlay.ini")
                missing_core = [name for name in required_core if not (settings / name).is_file()]
                if missing_core:
                    raise RuntimeError("GSE core config is incomplete: " + ", ".join(missing_core))

                # If both x86/x64 exist in one folder, their interface sets are normally compatible;
                # prefer the architecture of the DLL located in that exact parent, x64 first.
                arch_targets = [t for t in targets if t.path.parent == settings.parent]
                arch_targets.sort(key=lambda t: 0 if t.arch == "x64" else 1)
                if arch_targets:
                    (settings / "steam_interfaces.txt").write_text(
                        interfaces_by_arch[arch_targets[0].arch], encoding="utf-8"
                    )

            for target in targets:
                source = self.package.dll(target.arch, experimental=experimental)
                shutil.copy2(source, target.path)
                self.log(f"Installed GSE {'Experimental' if experimental else 'Regular'} {target.arch}: {target.path}")

            marker.write_text(json.dumps({
                "version": self.package.version,
                "appid": appid,
                "backup_manifest": str(manifest),
                "targets": [str(t.path) for t in targets],
                "settings_dirs": [str(p) for p in settings_dirs],
                "achievements": (stats or {}).get("achievements", 0),
                "stats": (stats or {}).get("stats", 0),
                "account_name": account_name,
                "save_mode": save_mode,
                "custom_save_path": custom_save_path if save_mode == "custom" else "",
                "overlay_enabled": bool(enable_overlay),
                "deployment_mode": "replace",
                "adjacent_backups": {
                    str(t.path.resolve()): str(selected_backups[str(t.path.resolve())]) for t in targets
                },
                **(marker_extra or {}),
            }, indent=2), encoding="utf-8")
            return manifest
        except Exception:
            self.log("Install failed; restoring original files...")
            restore_manifest(manifest)
            raise

    def install_preserve(
        self,
        game_root: Path,
        targets: list[SteamApiTarget],
        main_exe: Path,
        appid: int,
        schema: dict,
        account_name: str,
        image_downloader=None,
        localized_schemas: dict[str, dict] | None = None,
        generated_settings: Path | None = None,
        save_mode: str = "gse",
        custom_save_path: str = "",
        enable_overlay: bool = False,
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
        steamstub_enabled: bool = True,
        extra_backup_targets: list[Path] | None = None,
        after_backup_hook=None,
        marker_extra: dict | None = None,
    ) -> Path:
        game_root = Path(game_root).resolve()
        main_exe = Path(main_exe).resolve()
        x64_targets = [t for t in targets if t.arch == "x64"]
        if not x64_targets or len(x64_targets) != len(targets):
            raise RuntimeError("Preserve-original loader mode currently supports x64 Steam API targets only.")

        exe_dir = main_exe.parent
        proxy_name = choose_proxy_name(exe_dir, steamstub_enabled=steamstub_enabled)
        proxy_path = exe_dir / proxy_name
        proxy_ini = exe_dir / Path(proxy_name).with_suffix(".ini").name
        settings_dir = exe_dir / "steam_settings"
        marker = game_root / ".gse_auto_setup.json"
        loader_targets = [
            proxy_path, proxy_ini, exe_dir / "coldloader.asi", exe_dir / "coldloader.ini",
            exe_dir / "gse_steamclient64.dll", settings_dir, marker,
        ]
        if extra_backup_targets:
            loader_targets.extend(Path(p) for p in extra_backup_targets)
        manifest = backup_files(game_root, loader_targets, self.backup_root)
        api_hashes_before = {str(t.path): sha256_file(t.path) for t in x64_targets}

        try:
            if after_backup_hook is not None:
                after_backup_hook()
            deployment = deploy_preserve_loader(
                self.package, exe_dir, appid, steamstub_enabled=steamstub_enabled,
                proxy_name=proxy_name, log=self.log,
            )
            overlay_assets = self.package.root / "steam_settings.EXAMPLE"
            materialize_canonical_defaults(
                overlay_assets if overlay_assets.is_dir() else None, deployment.settings_dir
            )
            if generated_settings is not None:
                self.log(f"Mirroring official generated config into {deployment.settings_dir}...")
                merge_settings_tree(generated_settings, deployment.settings_dir)
            stats = write_basic_settings(
                deployment.settings_dir, appid, schema, account_name,
                image_downloader=image_downloader, save_mode=save_mode,
                custom_save_path=custom_save_path, enable_overlay=enable_overlay,
                overlay_assets_root=overlay_assets if overlay_assets.is_dir() else None,
                network_mode=network_mode, overlay_fps=overlay_fps,
                overlay_frametime=overlay_frametime, overlay_playtime=overlay_playtime,
                overlay_achievement_notifications=overlay_achievement_notifications,
                overlay_friend_notifications=overlay_friend_notifications,
                overlay_achievement_progress=overlay_achievement_progress, overlay_icons=overlay_icons,
                overlay_user_info=overlay_user_info, overlay_show_playtime=overlay_show_playtime,
                overlay_position=overlay_position, overlay_hotkey=overlay_hotkey,
                overlay_font_size=overlay_font_size, overlay_icon_size=overlay_icon_size,
                overlay_rounding=overlay_rounding, overlay_animation=overlay_animation,
                overlay_achievement_duration=overlay_achievement_duration,
                overlay_hook_delay=overlay_hook_delay, overlay_renderer_timeout=overlay_renderer_timeout,
                overlay_warnings=overlay_warnings,
                localized_schemas=localized_schemas,
                preserve_existing_achievements=(
                    generated_settings is not None and (Path(generated_settings) / "achievements.json").is_file()
                ),
                preserve_existing_stats=(
                    generated_settings is not None and (Path(generated_settings) / "stats.json").is_file()
                ),
            )
            if generated_settings is not None:
                validate_settings_mirror(generated_settings, deployment.settings_dir)
            required_core = ("configs.main.ini", "configs.user.ini", "configs.app.ini", "configs.overlay.ini")
            missing_core = [name for name in required_core if not (deployment.settings_dir / name).is_file()]
            if missing_core:
                raise RuntimeError("GSE core config is incomplete: " + ", ".join(missing_core))
            self.log("Generating Steam interfaces from untouched original x64 Steam API...")
            interfaces = _generate_interfaces(self.package.generator("x64"), x64_targets[0].path)
            (deployment.settings_dir / "steam_interfaces.txt").write_text(interfaces, encoding="utf-8")

            api_hashes_after = {str(t.path): sha256_file(t.path) for t in x64_targets}
            if api_hashes_before != api_hashes_after:
                raise RuntimeError("Preserve-original invariant failed: a Steam API DLL changed during deployment.")

            marker.write_text(json.dumps({
                "version": self.package.version,
                "appid": appid,
                "backup_manifest": str(manifest),
                "targets": [str(t.path) for t in x64_targets],
                "settings_dirs": [str(deployment.settings_dir)],
                "account_name": account_name,
                "save_mode": save_mode,
                "overlay_enabled": bool(enable_overlay),
                "deployment_mode": "preserve",
                "proxy": str(deployment.proxy_path),
                "original_api_sha256": api_hashes_before,
                "achievements": stats.get("achievements", 0),
                "stats": stats.get("stats", 0),
                **(marker_extra or {}),
            }, indent=2), encoding="utf-8")
            self.log("Preserve-original verification: Steam API hash unchanged.")
            return manifest
        except Exception:
            self.log("Preserve install failed; restoring loader/settings files...")
            restore_manifest(manifest)
            raise

    def install_coldclient(
        self,
        game_root: Path,
        targets: list[SteamApiTarget],
        main_exe: Path,
        appid: int,
        schema: dict,
        account_name: str,
        image_downloader=None,
        localized_schemas: dict[str, dict] | None = None,
        generated_settings: Path | None = None,
        save_mode: str = "gse",
        custom_save_path: str = "",
        enable_overlay: bool = False,
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
        include_renderer: bool = True,
        include_extra: bool = False,
        extra_backup_targets: list[Path] | None = None,
        after_backup_hook=None,
        marker_extra: dict | None = None,
    ) -> Path:
        game_root = Path(game_root).resolve()
        main_exe = Path(main_exe).resolve()
        if not targets:
            raise RuntimeError("ColdClient mode requires at least one Steam API target for interface discovery.")
        arch = "x64" if any(t.arch == "x64" for t in targets) else "x86"
        if any(t.arch != arch for t in targets):
            raise RuntimeError("ColdClient mode does not support mixed x86/x64 Steam API targets in one setup run.")
        exe_dir = main_exe.parent
        settings_dir = exe_dir / "steam_settings"
        cc = self.package.coldclient_root()
        loader = self.package.coldclient_loader(arch)
        steamclient = self.package.steamclient(arch)
        renderer = self.package.overlay_renderer(arch) if include_renderer else None
        extra = self.package.coldclient_extra(arch) if include_extra else None
        planned = [
            exe_dir / loader.name, exe_dir / steamclient.name, exe_dir / "ColdClientLoader.ini",
            settings_dir, game_root / ".gse_auto_setup.json",
        ]
        if renderer is not None:
            planned.append(exe_dir / renderer.name)
        if extra is not None:
            planned.append(exe_dir / extra.name)
        if extra_backup_targets:
            planned.extend(Path(p) for p in extra_backup_targets)
        manifest = backup_files(game_root, planned, self.backup_root)
        api_hashes_before = {str(t.path): sha256_file(t.path) for t in targets}
        try:
            if after_backup_hook is not None:
                after_backup_hook()
            deployed = ColdClientInstaller(self.package, self.log).install(
                exe_dir, appid, arch, main_exe=main_exe, include_renderer=include_renderer, include_extra=include_extra
            )
            overlay_assets = self.package.root / "steam_settings.EXAMPLE"
            materialize_canonical_defaults(
                overlay_assets if overlay_assets.is_dir() else None, settings_dir
            )
            if generated_settings is not None:
                self.log(f"Mirroring official generated config into {settings_dir}...")
                merge_settings_tree(generated_settings, settings_dir)
            stats = write_basic_settings(
                settings_dir, appid, schema, account_name, image_downloader=image_downloader,
                save_mode=save_mode, custom_save_path=custom_save_path, enable_overlay=enable_overlay,
                overlay_assets_root=overlay_assets if overlay_assets.is_dir() else None,
                network_mode=network_mode, overlay_fps=overlay_fps,
                overlay_frametime=overlay_frametime, overlay_playtime=overlay_playtime,
                overlay_achievement_notifications=overlay_achievement_notifications,
                overlay_friend_notifications=overlay_friend_notifications,
                overlay_achievement_progress=overlay_achievement_progress, overlay_icons=overlay_icons,
                overlay_user_info=overlay_user_info, overlay_show_playtime=overlay_show_playtime,
                overlay_position=overlay_position, overlay_hotkey=overlay_hotkey,
                overlay_font_size=overlay_font_size, overlay_icon_size=overlay_icon_size,
                overlay_rounding=overlay_rounding, overlay_animation=overlay_animation,
                overlay_achievement_duration=overlay_achievement_duration,
                overlay_hook_delay=overlay_hook_delay, overlay_renderer_timeout=overlay_renderer_timeout,
                overlay_warnings=overlay_warnings,
                localized_schemas=localized_schemas,
                preserve_existing_achievements=(generated_settings is not None and (Path(generated_settings) / "achievements.json").is_file()),
                preserve_existing_stats=(generated_settings is not None and (Path(generated_settings) / "stats.json").is_file()),
            )
            if generated_settings is not None:
                validate_settings_mirror(generated_settings, settings_dir)
            required_core = ("configs.main.ini", "configs.user.ini", "configs.app.ini", "configs.overlay.ini")
            missing_core = [name for name in required_core if not (settings_dir / name).is_file()]
            if missing_core:
                raise RuntimeError("GSE core config is incomplete: " + ", ".join(missing_core))
            original = next((t.path for t in targets if t.arch == arch), targets[0].path)
            interfaces = _generate_interfaces(self.package.generator(arch), original)
            (settings_dir / "steam_interfaces.txt").write_text(interfaces, encoding="utf-8")
            api_hashes_after = {str(t.path): sha256_file(t.path) for t in targets}
            if api_hashes_before != api_hashes_after:
                raise RuntimeError("ColdClient invariant failed: Steam API DLL changed during deployment.")
            marker_path = game_root / ".gse_auto_setup.json"
            marker_path.write_text(json.dumps({
                "version": self.package.version, "appid": appid, "backup_manifest": str(manifest),
                "targets": [str(t.path) for t in targets], "settings_dirs": [str(settings_dir)],
                "deployment_mode": "coldclient", "coldclient_files": [str(p) for p in deployed],
                "overlay_enabled": bool(enable_overlay), "original_api_sha256": api_hashes_before,
                "achievements": stats.get("achievements", 0), "stats": stats.get("stats", 0),
                **(marker_extra or {}),
            }, indent=2), encoding="utf-8")
            return manifest
        except Exception:
            self.log("ColdClient install failed; restoring original files...")
            restore_manifest(manifest)
            raise

    def install_coldclient_simple(
        self,
        game_root: Path,
        targets: list[SteamApiTarget],
        main_exe: Path,
        appid: int,
        schema: dict,
        account_name: str,
        image_downloader=None,
        localized_schemas: dict[str, dict] | None = None,
        generated_settings: Path | None = None,
        save_mode: str = "gse",
        custom_save_path: str = "",
        enable_overlay: bool = False,
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
        extra_backup_targets: list[Path] | None = None,
        after_backup_hook=None,
        marker_extra: dict | None = None,
    ) -> Path:
        """ColdClient v1 (simple/direct) mode.

        Unlike the full ColdClient loader mode, this variant:
        - Replaces steam_api*.dll with GSE Experimental (same as regular Experimental install).
        - Drops ``steamclient64.dll`` + ``steamclient.dll`` from the ColdClient package
          directly into the *game root* folder next to the main executable so the OS
          DLL search order finds them without requiring ColdClientLoader.exe.
        - Creates ``steam_settings/`` next to the main executable (same location as
          the regular Experimental variant).

        This is useful for Unity IL2CPP games, DX12-only titles and other games that
        ``LoadLibrary("steamclient64.dll")`` directly at startup and would otherwise
        show "Unable to load library steamclient64.dll".
        """
        game_root = Path(game_root).resolve()
        main_exe = Path(main_exe).resolve()
        if not targets:
            raise RuntimeError("ColdClient Simple mode requires at least one Steam API target.")
        arch = "x64" if any(t.arch == "x64" for t in targets) else "x86"
        exe_dir = main_exe.parent

        # Resolve steamclient and renderer DLL sources from the ColdClient package.
        cc = self.package.coldclient_root()
        steamclient64_src = cc / "steamclient64.dll"
        steamclient_src = cc / "steamclient.dll"
        renderer64_src = cc / "GameOverlayRenderer64.dll"
        renderer_src = cc / "GameOverlayRenderer.dll"
        if not steamclient64_src.is_file():
            raise FileNotFoundError(f"steamclient64.dll not found in ColdClient package: {cc}")
        if not steamclient_src.is_file():
            raise FileNotFoundError(f"steamclient.dll not found in ColdClient package: {cc}")

        # In ColdClient mode, configs MUST be placed beside steamclient(64).dll (exe_dir),
        # as well as beside any subfolder steam_api targets (e.g. Unity Plugins folder).
        target_settings_dirs = {t.path.parent / "steam_settings" for t in targets}
        target_settings_dirs.add(exe_dir / "steam_settings")
        settings_dirs = sorted(target_settings_dirs, key=lambda p: str(p).lower())

        # Files that will be created/modified — backup them all.
        planned: list[Path] = [t.path for t in targets]
        planned += [exe_dir / "steamclient64.dll", exe_dir / "steamclient.dll", exe_dir / "steam_appid.txt"]
        if renderer64_src.is_file():
            planned.append(exe_dir / "GameOverlayRenderer64.dll")
        if renderer_src.is_file():
            planned.append(exe_dir / "GameOverlayRenderer.dll")
        for sd in settings_dirs:
            planned.append(sd)
        planned.append(game_root / ".gse_auto_setup.json")
        if extra_backup_targets:
            planned.extend(Path(p) for p in extra_backup_targets)

        manifest = backup_files(game_root, planned, self.backup_root)
        try:
            if after_backup_hook is not None:
                after_backup_hook()

            # 1. Detect interfaces from original DLL before replacing.
            interfaces_by_arch: dict[str, str] = {}
            for target in targets:
                if target.arch not in interfaces_by_arch:
                    self.log(f"Generating Steam interfaces ({target.arch}) from original DLL...")
                    interfaces_by_arch[target.arch] = _generate_interfaces(
                        self.package.generator(target.arch), target.path
                    )

            # 2. Replace steam_api*.dll with GSE Experimental.
            for target in targets:
                source = self.package.dll(target.arch, experimental=True)
                shutil.copy2(source, target.path)
                self.log(f"Installed GSE Experimental {target.arch}: {target.path}")

            # 3. Drop steamclient DLLs & GameOverlayRenderer into exe dir (game searches here at startup).
            shutil.copy2(steamclient64_src, exe_dir / "steamclient64.dll")
            self.log(f"Deployed steamclient64.dll -> {exe_dir}")
            shutil.copy2(steamclient_src, exe_dir / "steamclient.dll")
            self.log(f"Deployed steamclient.dll -> {exe_dir}")
            if renderer64_src.is_file():
                shutil.copy2(renderer64_src, exe_dir / "GameOverlayRenderer64.dll")
            if renderer_src.is_file():
                shutil.copy2(renderer_src, exe_dir / "GameOverlayRenderer.dll")
            (exe_dir / "steam_appid.txt").write_text(str(int(appid)), encoding="ascii")

            # 4. Write steam_settings config next to each steam_api target (standard location).
            overlay_assets = self.package.root / "steam_settings.EXAMPLE"
            stats = None
            for settings_dir in settings_dirs:
                seeded = materialize_canonical_defaults(
                    overlay_assets if overlay_assets.is_dir() else None, settings_dir
                )
                if seeded:
                    self.log(f"Seeded {len(seeded)} canonical GSE default file(s).")
                if generated_settings is not None:
                    self.log(f"Mirroring official generated config into {settings_dir}...")
                    merge_settings_tree(generated_settings, settings_dir)
                stats = write_basic_settings(
                    settings_dir, appid, schema, account_name,
                    image_downloader=image_downloader,
                    save_mode=save_mode, custom_save_path=custom_save_path,
                    enable_overlay=enable_overlay,
                    overlay_assets_root=overlay_assets if overlay_assets.is_dir() else None,
                    network_mode=network_mode, overlay_fps=overlay_fps,
                    overlay_frametime=overlay_frametime, overlay_playtime=overlay_playtime,
                    overlay_achievement_notifications=overlay_achievement_notifications,
                    overlay_friend_notifications=overlay_friend_notifications,
                    overlay_achievement_progress=overlay_achievement_progress,
                    overlay_icons=overlay_icons, overlay_user_info=overlay_user_info,
                    overlay_show_playtime=overlay_show_playtime,
                    overlay_position=overlay_position, overlay_hotkey=overlay_hotkey,
                    overlay_font_size=overlay_font_size, overlay_icon_size=overlay_icon_size,
                    overlay_rounding=overlay_rounding, overlay_animation=overlay_animation,
                    overlay_achievement_duration=overlay_achievement_duration,
                    overlay_hook_delay=overlay_hook_delay,
                    overlay_renderer_timeout=overlay_renderer_timeout,
                    overlay_warnings=overlay_warnings,
                    localized_schemas=localized_schemas,
                    preserve_existing_achievements=(
                        generated_settings is not None and (Path(generated_settings) / "achievements.json").is_file()
                    ),
                    preserve_existing_stats=(
                        generated_settings is not None and (Path(generated_settings) / "stats.json").is_file()
                    ),
                )
                if generated_settings is not None:
                    validate_settings_mirror(generated_settings, settings_dir)
                required_core = ("configs.main.ini", "configs.user.ini", "configs.app.ini", "configs.overlay.ini")
                missing_core = [n for n in required_core if not (settings_dir / n).is_file()]
                if missing_core:
                    raise RuntimeError("GSE core config is incomplete: " + ", ".join(missing_core))

                arch_targets = [t for t in targets if t.path.parent == settings_dir.parent]
                arch_targets.sort(key=lambda t: 0 if t.arch == "x64" else 1)
                if arch_targets:
                    (settings_dir / "steam_interfaces.txt").write_text(
                        interfaces_by_arch[arch_targets[0].arch], encoding="utf-8"
                    )

            marker_path = game_root / ".gse_auto_setup.json"
            marker_path.write_text(json.dumps({
                "version": self.package.version, "appid": appid,
                "backup_manifest": str(manifest),
                "targets": [str(t.path) for t in targets],
                "settings_dirs": [str(sd) for sd in settings_dirs],
                "deployment_mode": "coldclient_simple",
                "steamclient64": str(exe_dir / "steamclient64.dll"),
                "overlay_enabled": bool(enable_overlay),
                "achievements": (stats or {}).get("achievements", 0),
                "stats": (stats or {}).get("stats", 0),
                **(marker_extra or {}),
            }, indent=2), encoding="utf-8")
            return manifest
        except Exception:
            self.log("ColdClient Simple install failed; restoring original files...")
            restore_manifest(manifest)
            raise

    @staticmethod
    def restore_latest(game_root: Path) -> Path:
        game_root = Path(game_root).resolve()
        marker = game_root / ".gse_auto_setup.json"
        manifest: Path | None = None

        if marker.is_file():
            try:
                data = json.loads(marker.read_text(encoding="utf-8"))
                m_path = Path(data.get("backup_manifest", ""))
                if m_path.is_file():
                    manifest = m_path
            except Exception:
                pass

        if manifest is None or not manifest.is_file():
            fallback_manifest = game_root / ".gse_auto_backup" / "manifest.json"
            if fallback_manifest.is_file():
                manifest = fallback_manifest

        if manifest is None or not manifest.is_file():
            # Check adjacent backups (.bak / .gseauto.bak) as emergency fallback
            from .scanner import scan_steam_api_targets
            try:
                targets = scan_steam_api_targets(game_root)
            except Exception:
                targets = []
            recovered = False
            for t in targets:
                for bak_func in (adjacent_backup_path, fallback_adjacent_backup_path):
                    bak = bak_func(t.path)
                    if bak.is_file():
                        shutil.copy2(bak, t.path)
                        try:
                            bak.unlink()
                        except OSError:
                            pass
                        recovered = True
            if marker.is_file():
                try:
                    marker.unlink()
                except OSError:
                    pass
            if recovered:
                return game_root
            raise RuntimeError("No backup manifest (.gse_auto_backup/manifest.json) was found in this game folder.")

        restore_manifest(manifest)
        backup_root = manifest.parent
        # The manifest is only needed until restore succeeds; leave no backup tree behind.
        if backup_root.name == ".gse_auto_backup" and backup_root.parent == game_root:
            shutil.rmtree(backup_root, ignore_errors=True)
        if marker.is_file():
            try:
                marker.unlink()
            except OSError:
                pass
        return game_root

