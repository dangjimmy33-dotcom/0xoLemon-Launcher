from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from .core.cache import CacheManager
from .core.config_builder import achievement_image_count, schema_to_achievements, schema_to_stats
from .core.executable import find_main_executable
from .core.github_client import GitHubClient
from .core.installer import (
    Installer,
    adjacent_backup_path,
    backup_files,
    clean_previous_emulator_state,
    fallback_adjacent_backup_path,
    restore_manifest,
)
from .core.models import GameMetadata, ReleaseInfo, SetupResult
from .core.official_generator import (
    GSE_TOOLS_REPO,
    cleanup_official_generator_output,
    extract_tools_package,
    find_generator_executable,
    run_official_generator,
)
from .core.package import GSEPackage, extract_release, locate_package_root, verify_package_root
from .core.resources import ResourceManager
from .core.rune import RuneInstaller, RuneResourceManager
from .core.scanner import detect_pe_architecture, scan_steam_api_targets
from .core.steam_api import STEAM_PLATFORM_LANGUAGES, SteamApiClient
from .core.steamless import SteamlessManager, steamless_backup_path
from .core.steamstub import SteamStubManager
from .core.uc_online import UCOnlineInstaller, UCOnlineResourceManager, detect_uc_backends


@dataclass(frozen=True)
class Inputs:
    appid: int
    game_folder: Path
    api_key: str
    # Legacy V1.x compatibility knobs.
    experimental: bool = False
    use_official_generator: bool = True
    account_name: str = "0xoLemon"
    save_mode: str = "gse"
    custom_save_path: str = ""
    enable_overlay: bool = False
    deployment_mode: str = "replace"
    enable_steamstub: bool = True
    # V1.8 engine model.
    engine: str = "gse"                    # gse | uc | rune
    gse_variant: str = "regular"           # regular | experimental | coldclient | coldclient_simple | preserve
    network_mode: str = "singleplayer"     # singleplayer | strict_offline | lan
    steamstub_mode: str = "auto"           # auto | steamless | rune | uc_runtime | disabled
    uc_spoof_appid: int = 480
    uc_plugins: tuple[str, ...] = ()
    coldclient_renderer: bool = True
    coldclient_extra: bool = False
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
    # RUNE engine options.
    rune_profile: str = "regular"          # regular | steakclient | steamclient
    rune_username: str = "RUNE"
    rune_language: str = "english"
    rune_unlock_all_dlcs: bool = False
    rune_lobby: bool = True
    rune_overlays: bool = True
    rune_offline: bool = False

    @property
    def effective_experimental(self) -> bool:
        # The native GSE overlay is compiled only into api_experimental.
        return bool(self.experimental or self.enable_overlay or self.gse_variant == "experimental")

    @property
    def effective_steamstub_mode(self) -> str:
        mode = (self.steamstub_mode or "").strip().lower()
        if mode:
            return mode
        return "auto" if self.enable_steamstub else "disabled"


def validate_inputs(appid_text: str, game_folder: str, api_key: str, *, engine: str = "gse") -> Inputs:
    try:
        appid = int(appid_text.strip())
    except Exception as exc:
        raise ValueError("AppID must be a positive number.") from exc
    if appid <= 0:
        raise ValueError("AppID must be a positive number.")
    folder = Path(game_folder.strip().strip('"')).expanduser()
    if not folder.is_dir():
        raise ValueError("Select a valid game folder.")
    engine = (engine or "gse").strip().lower()
    key = api_key.strip()
    if engine == "gse" and len(key) < 8:
        raise ValueError("Enter a valid Steam Web API key for GSE achievement enrichment.")
    if engine not in {"gse", "uc"}:
        raise ValueError(f"Unknown engine: {engine}")
    return Inputs(appid=appid, game_folder=folder.resolve(), api_key=key, engine=engine)


class SetupService:
    def __init__(
        self,
        log: Callable[[str], None] | None = None,
        progress: Callable[[int, str], None] | None = None,
        cache: CacheManager | None = None,
        resources: ResourceManager | None = None,
    ):
        self.log = log or (lambda _m: None)
        self.progress = progress or (lambda _p, _m: None)
        self.resources = resources or ResourceManager()
        # Metadata is portable too; V1.8 never needs a heavy LocalAppData cache.
        self.cache = cache or CacheManager(root=self.resources.app_dir / "resources" / "state")
        self.github = GitHubClient()
        self.tools_github = GitHubClient(repo=GSE_TOOLS_REPO)
        self.uc_resources = UCOnlineResourceManager(self.resources, self.log)

    def check_latest_release(self) -> ReleaseInfo:
        return self.github.latest_release()

    def _set_progress(self, value: int, message: str) -> None:
        self.progress(max(0, min(100, int(value))), message)
        self.log(message)

    @staticmethod
    def _component_tag(root: Path, fallback: str = "embedded") -> str:
        try:
            return str(json.loads((root / "component.json").read_text(encoding="utf-8-sig")).get("tag") or fallback)
        except Exception:
            return fallback

    def _bundled_package(self) -> GSEPackage:
        root = self.resources.component_root("gse")
        root = verify_package_root(root)
        return GSEPackage(root, self._component_tag(root, "embedded"))

    def _ensure_package(self, release: ReleaseInfo) -> GSEPackage:
        local_root = self.resources.component_root("gse", require=False)
        if local_root.is_dir():
            try:
                verify_package_root(local_root)
                if self._component_tag(local_root, "") == release.tag:
                    self.log(f"Using local GSE {release.tag}.")
                    return GSEPackage(local_root, release.tag)
            except Exception:
                pass

        with self.resources.temp_dir("gse") as temp:
            archive = temp / release.asset.name
            self._set_progress(18, f"Downloading official GSE {release.tag}...")

            def dl(done: int, total: int):
                if total > 0:
                    pct = 18 + int((done / total) * 18)
                    self.progress(pct, f"Downloading GSE... {done * 100 // total}%")

            self.github.download_asset(release.asset, archive, dl)
            self._set_progress(38, "Extracting GSE release into portable update staging...")
            extract_root = temp / "unpack"
            root = extract_release(archive, extract_root)
            (root / "component.json").write_text(
                json.dumps({"tag": release.tag, "asset": release.asset.name}, indent=2), encoding="utf-8"
            )
            installed = self.resources.install_update_tree("gse", root)
            return GSEPackage(verify_package_root(installed), release.tag)

    def _prepare_gse_package(self) -> GSEPackage:
        self._set_progress(3, "Checking official GSE release...")
        try:
            release = self.github.latest_release()
            return self._ensure_package(release)
        except Exception as exc:
            self.log(f"GSE update unavailable ({exc}); using local embedded/portable resources.")
            return self._bundled_package()

    def _local_tools(self) -> tuple[Path, str] | None:
        # Do not let a stale/partial portable update shadow a valid baseline.
        # Validate each candidate and fall through to the next one when needed.
        for root in self.resources.component_candidates("gse_tools"):
            if not root.is_dir():
                continue
            try:
                exe = find_generator_executable(root)
                return exe.parent, self._component_tag(root, "embedded")
            except Exception:
                continue
        return None

    def _ensure_tools(self) -> tuple[Path, str]:
        # Setup should be deterministic/offline-first. Resource updates are a
        # separate operation; never redownload a valid generator during Setup.
        local = self._local_tools()
        if local is not None:
            self.log(f"Using local official config generator {local[1]}.")
            return local

        try:
            release = self.tools_github.latest_release()
        except Exception as exc:
            raise RuntimeError(
                "No bundled gse_fork_tools baseline is available and the update check failed."
            ) from exc
        with self.resources.temp_dir("gse_tools") as temp:
            archive = temp / release.asset.name
            self.log(f"Downloading official gse_fork_tools {release.tag} into portable resources...")
            self.tools_github.download_asset(release.asset, archive)
            unpack = temp / "unpack"
            generator_dir = extract_tools_package(archive, unpack)
            # Install the package root that contains the generator, not only the exe parent if nested.
            package_root = unpack
            (package_root / "component.json").write_text(json.dumps({"tag": release.tag}, indent=2), encoding="utf-8")
            installed = self.resources.install_update_tree("gse_tools", package_root)
            exe = find_generator_executable(installed)
            return exe.parent, release.tag

    def _steam_context(self, inputs: Inputs) -> tuple[SteamApiClient, dict, GameMetadata]:
        self._set_progress(45, "Steam Web API: fetching achievement/stat schema...")
        steam = SteamApiClient(inputs.api_key)
        schema = steam.get_schema(inputs.appid, "english")
        achievement_count = len(schema_to_achievements(schema))
        stat_count = len(schema_to_stats(schema))
        icon_total = achievement_image_count(schema)
        self.log(
            f"Steam Web API schema: {achievement_count} achievements, {stat_count} stats, "
            f"{icon_total} achievement images."
        )
        try:
            metadata = steam.get_store_metadata(inputs.appid)
        except Exception:
            metadata = GameMetadata(inputs.appid, f"Steam App {inputs.appid}")
        return steam, schema, metadata

    def _main_exe_and_arch(self, game_folder: Path, targets) -> tuple[Path, str]:
        main_exe = find_main_executable(game_folder)
        try:
            arch = detect_pe_architecture(main_exe)
        except Exception:
            arch = "x64" if any(t.arch == "x64" for t in targets) else "x86"
        return main_exe, arch

    def _drm_plan(self, inputs: Inputs, main_exe: Path, arch: str):
        """Return (backup paths, hook, mutable state).

        Auto intentionally tries Steamless only. It never silently falls back to
        a proxy DLL; the user must explicitly choose RUNE or UC runtime mode.
        """
        mode = inputs.effective_steamstub_mode
        state: dict = {"mode": mode, "status": "disabled" if mode == "disabled" else "pending"}
        if mode == "disabled":
            return [], None, state

        if mode in {"steamless", "auto"}:
            backup_paths = [
                main_exe,
                steamless_backup_path(main_exe),
                main_exe.with_name(main_exe.name + ".unpacked.exe"),
            ]
            manager = SteamlessManager(self.resources, self.log)

            def hook():
                try:
                    manager.patch(main_exe, check_updates=True)
                    state.update({"status": "patched", "method": "steamless"})
                except Exception as exc:
                    if mode == "auto":
                        state.update({"status": "not-patched", "method": "steamless", "error": str(exc)})
                        self.log(
                            f"Steamless Auto could not unpack this EXE ({exc}). "
                            "No proxy DLL fallback was deployed; choose RUNE/UC runtime explicitly if needed."
                        )
                        return
                    raise
            return backup_paths, hook, state

        if mode == "rune":
            backup_paths = [main_exe.parent / "winmm.dll", main_exe.parent / ".gse_steamstub.json"]
            manager = SteamStubManager(self.resources, self.log)

            def hook():
                deployed = manager.deploy(main_exe, arch)
                state.update({"status": "deployed", "method": "rune", "path": str(deployed)})
            return backup_paths, hook, state

        if mode == "uc_runtime":
            # UC runtime patching is activated in union-crax.ini; no extra file is written here.
            state.update({"status": "configured", "method": "uc_runtime"})
            return [], None, state

        raise ValueError(f"Unknown SteamStub mode: {mode}")

    def _run_gse(self, inputs: Inputs) -> SetupResult:
        self._set_progress(10, "Checking previous installation state...")
        clean_previous_emulator_state(inputs.game_folder, log=self.log)

        package = self._prepare_gse_package()
        steam, schema, metadata = self._steam_context(inputs)
        self.log(f"Game: {metadata.name} ({inputs.appid})")

        self._set_progress(53, "Scanning game folder for Steam API DLLs...")
        targets = scan_steam_api_targets(inputs.game_folder)
        if not targets:
            raise RuntimeError("No eligible original steam_api.dll / steam_api64.dll was found in the selected folder.")
        self.log("Found: " + ", ".join(f"{t.arch} {t.path.relative_to(inputs.game_folder)}" for t in targets))

        generated_settings = None
        if inputs.use_official_generator:
            self._set_progress(57, "Preparing official GSE config...")
            try:
                tools_root, tools_version = self._ensure_tools()
                self.log(f"Official generator version: {tools_version}")
                self.progress(58, "Starting official GSE config generator…")

                # Map the generator's internal 0-100 scale onto the 58-66 % slot
                # of the overall progress bar so the bar moves visibly while the
                # generator is running (which can take several minutes on slow
                # connections).
                _GEN_LO, _GEN_HI = 58, 66

                def _gen_progress(gen_pct: int, label: str) -> None:
                    mapped = _GEN_LO + int(gen_pct / 100 * (_GEN_HI - _GEN_LO))
                    self.progress(mapped, label)

                # A previous interrupted run may have left _OUTPUT/<appid> behind.
                # Clear it first so this run's official generator output is the
                # canonical source of truth, including achievements/localization.
                cleanup_official_generator_output(tools_root, inputs.appid)
                try:
                    generated_settings = run_official_generator(
                        tools_root, inputs.appid, log=self.log, progress=_gen_progress,
                        skip_achievements=False,
                    )
                    self.log(f"Official config generated: {generated_settings}")
                except Exception as full_exc:
                    detail = str(full_exc)
                    if "PyCryptodome native module" in detail or "Cryptodome.Hash._MD5" in detail:
                        self.log(
                            "Official generator runtime dependency failure detected; not retrying -skip_ach "
                            "because the generator fails before argument parsing."
                        )
                        raise RuntimeError(
                            "Official generator could not start its Python crypto runtime. "
                            "No incomplete steam_settings tree was deployed. "
                            f"Details: {detail}"
                        ) from full_exc
                    self.log(
                        "Full official generator failed after startup; retrying compatibility mode with "
                        f"-skip_ach. Details: {full_exc}"
                    )
                    cleanup_official_generator_output(tools_root, inputs.appid)
                    generated_settings = run_official_generator(
                        tools_root, inputs.appid, log=self.log, progress=_gen_progress,
                        skip_achievements=True,
                    )
                    self.log(
                        "Official config generated in compatibility mode (-skip_ach); "
                        f"achievement localization will be supplied by Steam Web API: {generated_settings}"
                    )
            except Exception as exc:
                raise RuntimeError(
                    "Official GSE config generation failed in both full and compatibility modes. "
                    "No incomplete steam_settings tree was deployed. Refresh gse_fork_tools and retry. "
                    f"Details: {exc}"
                ) from exc
            finally:
                self.progress(66, "Preparing local GSE configuration…")

        generated_has_achievements = bool(
            generated_settings is not None
            and (Path(generated_settings) / "achievements.json").is_file()
        )
        if generated_has_achievements:
            localized_schemas = {}
            self._set_progress(61, "Using canonical achievement localization from official generator...")
            self.log("Official generator supplied achievements.json; Steam Web API localization fallback skipped.")
        else:
            requested_languages = list(STEAM_PLATFORM_LANGUAGES)
            if generated_settings is not None:
                lang_file = next(iter(Path(generated_settings).rglob("supported_languages.txt")), None)
                if lang_file and lang_file.is_file():
                    parsed: list[str] = []
                    for raw in lang_file.read_text(encoding="utf-8", errors="replace").splitlines():
                        lang = raw.strip().lower()
                        if lang and not lang.startswith(("#", ";")) and lang not in parsed:
                            parsed.append(lang)
                    if parsed:
                        requested_languages = parsed
            if "english" not in requested_languages:
                requested_languages.insert(0, "english")

            self._set_progress(61, f"Steam Web API: fetching {len(requested_languages)} achievement localizations...")

            def loc_progress(done: int, total: int, _lang: str) -> None:
                self.progress(61 + int((done / max(total, 1)) * 5), f"Achievement localization {done}/{total}")

            localized_schemas = steam.get_localized_schemas(
                inputs.appid, requested_languages, progress=loc_progress, max_workers=6
            )
            self.log(f"Achievement localization fallback: {len(localized_schemas)} Steam language schema(s).")

        icon_total = achievement_image_count(schema)
        icon_done = 0

        def download_icon(url: str, path: Path) -> bool:
            nonlocal icon_done
            ok = steam.download_file(url, path)
            icon_done += 1
            if icon_total:
                self.progress(min(89, 74 + int((icon_done / icon_total) * 15)), f"Achievement images {icon_done}/{icon_total}")
            return ok

        main_exe, main_arch = self._main_exe_and_arch(inputs.game_folder, targets)
        self.log(f"Main executable: {main_exe.relative_to(inputs.game_folder)}")
        drm_paths, drm_hook, drm_state = self._drm_plan(inputs, main_exe, main_arch)
        if inputs.effective_steamstub_mode == "uc_runtime":
            raise ValueError("UC Runtime SteamStub is only available with the UC Online engine.")

        backup_dir = inputs.game_folder / ".gse_auto_backup"
        installer = Installer(package, backup_dir, log=self.log)
        marker_extra = {"engine": "gse", "steamstub": drm_state, "network_mode": inputs.network_mode}
        variant = (inputs.gse_variant or "regular").strip().lower()
        if inputs.deployment_mode == "preserve" and variant == "regular":
            variant = "preserve"

        extra_targets = list(drm_paths)
        if inputs.overlay_dinput_bridge:
            extra_targets.extend([
                inputs.game_folder / "dinput8.dll",
                inputs.game_folder / "dinput8.ini",
            ])

        common = dict(
            account_name=inputs.account_name or "0xoLemon",
            image_downloader=download_icon,
            localized_schemas=localized_schemas,
            generated_settings=generated_settings,
            save_mode=inputs.save_mode,
            custom_save_path=inputs.custom_save_path,
            enable_overlay=inputs.enable_overlay,
            network_mode=inputs.network_mode,
            overlay_fps=inputs.overlay_fps,
            overlay_frametime=inputs.overlay_frametime,
            overlay_playtime=inputs.overlay_playtime,
            overlay_achievement_notifications=inputs.overlay_achievement_notifications,
            overlay_friend_notifications=inputs.overlay_friend_notifications,
            overlay_achievement_progress=inputs.overlay_achievement_progress,
            overlay_icons=inputs.overlay_icons,
            overlay_user_info=inputs.overlay_user_info,
            overlay_show_playtime=inputs.overlay_show_playtime,
            overlay_position=inputs.overlay_position,
            overlay_hotkey=inputs.overlay_hotkey,
            overlay_font_size=inputs.overlay_font_size,
            overlay_icon_size=inputs.overlay_icon_size,
            overlay_rounding=inputs.overlay_rounding,
            overlay_animation=inputs.overlay_animation,
            overlay_achievement_duration=inputs.overlay_achievement_duration,
            overlay_hook_delay=inputs.overlay_hook_delay,
            overlay_renderer_timeout=inputs.overlay_renderer_timeout,
            overlay_warnings=inputs.overlay_warnings,
            extra_backup_targets=extra_targets,
            after_backup_hook=drm_hook,
            marker_extra=marker_extra,
        )

        self._set_progress(67, f"Deploying GSE {variant}...")
        if variant == "coldclient":
            manifest = installer.install_coldclient(
                inputs.game_folder, targets, main_exe, inputs.appid, schema,
                include_renderer=inputs.coldclient_renderer,
                include_extra=inputs.coldclient_extra,
                **common,
            )
            settings_dirs = [main_exe.parent / "steam_settings"]
        elif variant == "coldclient_simple":
            manifest = installer.install_coldclient_simple(
                inputs.game_folder, targets, main_exe, inputs.appid, schema,
                **common,
            )
            settings_dirs = sorted({t.path.parent / "steam_settings" for t in targets}, key=lambda p: str(p))
        elif variant == "preserve":
            manifest = installer.install_preserve(
                inputs.game_folder, targets, main_exe, inputs.appid, schema,
                steamstub_enabled=(inputs.effective_steamstub_mode == "rune"),
                **common,
            )
            settings_dirs = [main_exe.parent / "steam_settings"]
        elif variant in {"regular", "experimental"}:
            manifest = installer.install(
                inputs.game_folder, targets, inputs.appid, schema,
                experimental=(variant == "experimental" or inputs.effective_experimental),
                **common,
            )
            settings_dirs = sorted({t.path.parent / "steam_settings" for t in targets}, key=lambda p: str(p))
        else:
            raise ValueError(f"Unknown GSE variant: {variant}")

        self._set_progress(100, f"Setup complete — GSE {package.version}")

        # Deploy DInput8 bridge if requested
        if inputs.overlay_dinput_bridge:
            res = ResourceManager()
            dinput_dir = res.component_root("dinput", require=False)
            dll_src = dinput_dir / "dinput8.dll"
            ini_src = dinput_dir / "dinput8.ini"
            if dll_src.is_file():
                import shutil as _shutil
                _shutil.copy2(dll_src, inputs.game_folder / "dinput8.dll")
                self.log("Deployed DInput8 Overlay Bridge: dinput8.dll")
            if ini_src.is_file():
                import shutil as _shutil
                ini_dst = inputs.game_folder / "dinput8.ini"
                if not ini_dst.exists():
                    _shutil.copy2(ini_src, ini_dst)
                    self.log("Deployed DInput8 config: dinput8.ini")

        # Clean up generator temporary _OUTPUT folder
        if generated_settings is not None:
            try:
                tools_root, _ = self._ensure_tools()
                cleanup_official_generator_output(tools_root, inputs.appid)
                self.log("Cleaned up official generator temporary _OUTPUT folder.")
            except Exception:
                pass

        return SetupResult(
            appid=inputs.appid,
            game_name=metadata.name,
            gse_version=package.version,
            installed_targets=[t.path for t in targets],
            backup_manifest=manifest,
            settings_dirs=settings_dirs,
        )

    def _run_uc(self, inputs: Inputs) -> SetupResult:
        self._set_progress(5, "Checking previous installation state...")
        clean_previous_emulator_state(inputs.game_folder, log=self.log)

        self._set_progress(10, "Preparing UC Online2 runtime...")
        package = self.uc_resources.ensure_package(check_updates=True)
        self._set_progress(25, "Scanning game folder for Steam API DLLs...")
        targets = scan_steam_api_targets(inputs.game_folder)
        if not targets:
            raise RuntimeError("No eligible original steam_api.dll / steam_api64.dll was found in the selected folder.")
        main_exe, main_arch = self._main_exe_and_arch(inputs.game_folder, targets)

        requested = list(inputs.uc_plugins)
        if "auto" in requested:
            detected = detect_uc_backends(inputs.game_folder)
            requested = sorted(detected)
            self.log("UC backend detection: " + (", ".join(requested) if requested else "none"))

        drm_paths, drm_hook, drm_state = self._drm_plan(inputs, main_exe, main_arch)
        runtime_stub = inputs.effective_steamstub_mode == "uc_runtime"

        backup_root = inputs.game_folder / ".gse_auto_backup"
        marker = inputs.game_folder / ".gse_auto_setup.json"
        planned: list[Path] = [marker, main_exe.parent / "union-crax.ini", main_exe.parent / "plugins"]
        planned.extend(drm_paths)
        for target in targets:
            planned.extend([target.path, adjacent_backup_path(target.path), fallback_adjacent_backup_path(target.path)])
        manifest = backup_files(inputs.game_folder, planned, backup_root)
        try:
            if drm_hook is not None:
                drm_hook()
            deployed = UCOnlineInstaller(package, self.log).install(
                inputs.game_folder,
                targets,
                main_exe,
                appid=inputs.appid,
                spoof_appid=int(inputs.uc_spoof_appid or 480),
                plugins=requested,
                runtime_steamstub=runtime_stub,
            )
            marker.write_text(json.dumps({
                "engine": "uc",
                "version": package.version,
                "appid": inputs.appid,
                "spoof_appid": int(inputs.uc_spoof_appid or 480),
                "backup_manifest": str(manifest),
                "targets": [str(t.path) for t in targets],
                "main_exe": str(main_exe),
                "plugins": requested,
                "steamstub": drm_state,
                "deployed": [str(p) for p in deployed],
            }, indent=2), encoding="utf-8")
        except Exception:
            self.log("UC Online deployment failed; restoring original files...")
            restore_manifest(manifest)
            raise

        # Metadata request does not require a Web API key.
        try:
            metadata = SteamApiClient(inputs.api_key).get_store_metadata(inputs.appid)
        except Exception:
            metadata = GameMetadata(inputs.appid, f"Steam App {inputs.appid}")
        self._set_progress(100, f"Setup complete — UC Online2 {package.version}")
        return SetupResult(
            appid=inputs.appid,
            game_name=metadata.name,
            gse_version=f"UC Online2 {package.version}",
            installed_targets=[t.path for t in targets],
            backup_manifest=manifest,
            settings_dirs=[],
        )

    def _run_rune(self, inputs: Inputs) -> SetupResult:
        self._set_progress(5, "Checking previous installation state...")
        clean_previous_emulator_state(inputs.game_folder, log=self.log)

        self._set_progress(10, "Preparing RUNE AutoCracker runtime...")
        rune_resources = RuneResourceManager(self.resources)
        # Verify RUNE suite files exist
        try:
            rune_root = rune_resources.rune_root()
            if not rune_root.is_dir():
                raise FileNotFoundError(f"RUNE emulator root directory not found: {rune_root}")
        except Exception as exc:
            raise RuntimeError(f"RUNE AutoCracker resources unavailable: {exc}") from exc

        self._set_progress(20, "Scanning game folder for Steam API DLLs...")
        targets = scan_steam_api_targets(inputs.game_folder)
        if not targets:
            raise RuntimeError("No eligible original steam_api.dll / steam_api64.dll was found in the selected folder.")
        main_exe, main_arch = self._main_exe_and_arch(inputs.game_folder, targets)

        if inputs.rune_profile == "steakclient" and main_arch != "x64":
            raise RuntimeError("Steakclient profile only supports 64-bit (x64) executables.")

        self._set_progress(35, "Checking SteamStub DRM plan...")
        drm_paths, drm_hook, drm_state = self._drm_plan(inputs, main_exe, main_arch)

        self._set_progress(50, "Querying Steam Store metadata & DLC list...")
        steam_client = SteamApiClient(inputs.api_key)
        try:
            metadata = steam_client.get_store_metadata(inputs.appid)
        except Exception:
            metadata = GameMetadata(inputs.appid, f"Steam App {inputs.appid}")

        dlcs: list[tuple[int, str]] = []
        try:
            dlcs = steam_client.get_dlcs(inputs.appid)
            if dlcs:
                self.log(f"Found {len(dlcs)} DLC(s) for AppID {inputs.appid}")
        except Exception as exc:
            self.log(f"DLC query notice: {exc}")

        self._set_progress(70, f"Deploying RUNE ({inputs.rune_profile})...")
        installer = RuneInstaller(
            package_manager=rune_resources,
            log=self.log,
            backup_root=inputs.game_folder / ".gse_auto_backup",
        )

        manifest = installer.install(
            game_root=inputs.game_folder,
            targets=targets,
            main_exe=main_exe,
            appid=inputs.appid,
            profile=inputs.rune_profile,
            username=inputs.rune_username or "RUNE",
            language=inputs.rune_language or "english",
            lobby_enabled=inputs.rune_lobby,
            overlays_enabled=inputs.rune_overlays,
            offline=inputs.rune_offline,
            unlock_all_dlcs=inputs.rune_unlock_all_dlcs,
            dlcs=dlcs,
            extra_backup_targets=drm_paths,
            after_backup_hook=drm_hook,
            marker_extra={"engine": "rune", "steamstub": drm_state, "profile": inputs.rune_profile},
        )

        settings_dirs = [t.path.parent for t in targets]
        if inputs.rune_profile == "steakclient":
            settings_dirs = [main_exe.parent]

        self._set_progress(100, f"Setup complete — RUNE AutoCracker ({inputs.rune_profile})")
        return SetupResult(
            appid=inputs.appid,
            game_name=metadata.name,
            gse_version=f"RUNE AutoCracker ({inputs.rune_profile})",
            installed_targets=[t.path for t in targets],
            backup_manifest=manifest,
            settings_dirs=settings_dirs,
        )

    def run(self, inputs: Inputs) -> SetupResult:
        engine = (inputs.engine or "gse").strip().lower()
        if engine == "gse":
            return self._run_gse(inputs)
        if engine == "uc":
            return self._run_uc(inputs)
        if engine == "rune":
            return self._run_rune(inputs)
        raise ValueError(f"Unknown engine: {inputs.engine}")

    def restore(self, game_folder: Path) -> Path:
        self._set_progress(15, "Reading game-local backup manifest...")
        manifest = Installer.restore_latest(game_folder)
        self._set_progress(100, "Original game files restored.")
        return manifest
