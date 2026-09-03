from __future__ import annotations

import json
import os
import re
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Dict, List, Optional, Set, Tuple

from .installer import (
    adjacent_backup_path,
    backup_files,
    ensure_adjacent_original_backup,
    fallback_adjacent_backup_path,
    restore_manifest,
)
from .models import ReleaseInfo
from .package import GSEPackage
from .resources import ResourceManager
from .scanner import SteamApiTarget


class SteamInterfaceExtractor:
    """Extract Steam interface version strings directly from original binary DLLs."""

    def __init__(self, dll_path: Path | str) -> None:
        path = Path(dll_path)
        # Prefer adjacent backup if available (to get original game interface versions)
        bak = adjacent_backup_path(path)
        fbak = fallback_adjacent_backup_path(path)
        if bak.is_file():
            self.dll_path = bak
        elif fbak.is_file():
            self.dll_path = fbak
        else:
            self.dll_path = path

    def extract_strings(self) -> List[str]:
        strings: List[str] = []
        if not self.dll_path.is_file():
            return strings
        try:
            data = self.dll_path.read_bytes()
            current_string = bytearray()
            for byte in data:
                if byte == 0:
                    if len(current_string) > 3:
                        try:
                            decoded = current_string.decode("ascii", errors="ignore")
                            if decoded.isprintable() and decoded.strip():
                                strings.append(decoded.strip())
                        except Exception:
                            pass
                    current_string = bytearray()
                else:
                    current_string.append(byte)
            if len(current_string) > 3:
                try:
                    decoded = current_string.decode("ascii", errors="ignore")
                    if decoded.isprintable() and decoded.strip():
                        strings.append(decoded.strip())
                except Exception:
                    pass
        except Exception:
            pass
        return strings

    @staticmethod
    def is_steam_interface(name: str) -> bool:
        return (
            ("Steam" in name or "STEAM" in name)
            and bool(name)
            and name[-1].isdigit()
            and 8 <= len(name) <= 50
            and not name.endswith((".dll", ".exe"))
        )

    def find_steam_interfaces(self, strings: List[str]) -> Set[str]:
        interfaces: Set[str] = set()
        for s in strings:
            if self.is_steam_interface(s):
                interfaces.add(s)
            elif " " in s or "\t" in s:
                parts = s.replace("\t", " ").split(" ")
                for part in parts:
                    clean = part.strip("(),;:'\"")
                    if self.is_steam_interface(clean):
                        interfaces.add(clean)
        return interfaces

    @staticmethod
    def normalize_name(full_interface: str) -> str:
        if full_interface.startswith("STEAM") and "_INTERFACE_" in full_interface:
            base = full_interface.split("_INTERFACE_")[0][5:]
            mappings = {
                "GAMESERVER": "SteamGameServer",
                "GAMESERVERSTATS": "SteamGameServerStats",
                "HTMLSURFACE": "SteamHTMLSurface",
                "MUSICREMOTE": "SteamMusicRemote",
                "MATCHMAKING": "SteamMatchMaking",
                "MATCHMAKINGSERVERS": "SteamMatchMakingServers",
                "MATCHGAMESEARCH": "SteamMatchGameSearch",
                "PARENTALSETTINGS": "SteamParentalSettings",
                "REMOTEPLAY": "SteamRemotePlay",
                "REMOTESTORAGE": "SteamRemoteStorage",
                "USERSTATS": "SteamUserStats",
                "HTTP": "SteamHTTP",
                "UGC": "SteamUGC",
                "UNIFIEDMESSAGES": "SteamUnifiedMessages",
            }
            return mappings.get(base, "Steam" + base.capitalize())
        elif full_interface.startswith("Steam"):
            result = full_interface
            while result and result[-1].isdigit():
                result = result[:-1]
            return result
        elif full_interface.startswith("ISteam"):
            result = full_interface[1:]
            while result and result[-1].isdigit():
                result = result[:-1]
            return result
        return full_interface

    def extract_interfaces(self) -> Dict[str, str]:
        strings = self.extract_strings()
        interfaces = self.find_steam_interfaces(strings)
        mapping: Dict[str, str] = {}
        for full_interface in sorted(interfaces):
            simple_name = self.normalize_name(full_interface)
            if simple_name:
                mapping[simple_name] = full_interface
        return mapping


def patch_shell32_string(file_path: Path | str, is64: bool = True) -> bool:
    """Patch the SHELL32.dll import string in steam_api(64).dll to load RUNE loader."""
    path = Path(file_path)
    if not path.is_file():
        return False
    data = bytearray(path.read_bytes())
    target = b"SHELL32.dll"
    if is64:
        replacement = bytes.fromhex("52554e4536340057555300")
    else:
        replacement = bytes.fromhex("52554e4500215755532100")

    idx = data.lower().find(target.lower())
    if idx == -1:
        return False
    data[idx : idx + len(target)] = replacement
    path.write_bytes(data)
    return True


class RuneResourceManager:
    """Resolve RUNE emulator suite resources."""

    def __init__(self, resources: Optional[ResourceManager] = None) -> None:
        self.resources = resources or ResourceManager()

    def rune_root(self) -> Path:
        return self.resources.component_root("rune")

    def emu_root(self) -> Path:
        return self.rune_root() / "emu"

    def steakclient_root(self) -> Path:
        return self.rune_root() / "steakclient"

    def steamclient_root(self) -> Path:
        return self.rune_root() / "steamclient"

    def steamstub_root(self) -> Path:
        return self.rune_root() / "Steam stub patcher"

    def latest_release(self) -> ReleaseInfo:
        root = self.rune_root()
        meta = root / "component.json"
        tag = "v1.2.2"
        if meta.is_file():
            try:
                data = json.loads(meta.read_text(encoding="utf-8"))
                tag = data.get("tag", tag)
            except Exception:
                pass
        return ReleaseInfo(tag=tag, name="RUNE Emulator Suite", body="", published_at="", assets=())


class RuneInstaller:
    """Installs RUNE emulator profiles (Regular, Steakclient, Steamclient)."""

    def __init__(
        self,
        package_manager: Optional[RuneResourceManager] = None,
        log: Optional[Callable[[str], None]] = None,
        backup_root: Optional[Path] = None,
    ) -> None:
        self.manager = package_manager or RuneResourceManager()
        self.log = log or (lambda _m: None)
        self.backup_root = Path(backup_root) if backup_root else Path(".gse_auto_backup")

    def _render_ini_content(
        self,
        template_text: str,
        appid: int,
        username: str = "RUNE",
        language: str = "english",
        lobby_enabled: bool = True,
        overlays_enabled: bool = True,
        offline: bool = False,
        unlock_all_dlcs: bool = False,
        dlcs: Optional[List[Tuple[int, str]]] = None,
        interfaces: Optional[Dict[str, str]] = None,
    ) -> str:
        content = template_text
        content = content.replace("SteamID", str(appid))
        content = content.replace("UserName=RUNE", f"UserName={username or 'RUNE'}")
        content = content.replace("Language=english", f"Language={language or 'english'}")
        content = content.replace("LobbyEnabled=1", f"LobbyEnabled={'1' if lobby_enabled else '0'}")
        content = content.replace("Overlays=1", f"Overlays={'1' if overlays_enabled else '0'}")
        content = content.replace("Offline=0", f"Offline={'1' if offline else '0'}")
        content = content.replace("DLCUnlockall=0", f"DLCUnlockall={'1' if unlock_all_dlcs else '0'}")

        # Interfaces
        if interfaces:
            iface_lines = [f"{k}={v}" for k, v in sorted(interfaces.items())]
            content = content.replace("RUNE_Interfaces", "\n".join(iface_lines))
        else:
            content = content.replace("RUNE_Interfaces", "")

        # DLCs
        if dlcs:
            dlc_lines = "".join(f"{d_id}={d_name}\n" for d_id, d_name in dlcs)
            content = content.replace("DLCs", dlc_lines.strip())
            content = content.replace("RUNE_DLC", "".join(f"{d_id} = {d_name}\n" for d_id, d_name in dlcs).strip())
        else:
            content = re.sub(r"^.*DLCs.*\n?", "", content, flags=re.MULTILINE)
            content = re.sub(r"^.*RUNE_DLC.*\n?", "", content, flags=re.MULTILINE)

        return content

    def install(
        self,
        game_root: Path,
        targets: List[SteamApiTarget],
        main_exe: Path,
        appid: int,
        *,
        profile: str = "regular",
        username: str = "RUNE",
        language: str = "english",
        lobby_enabled: bool = True,
        overlays_enabled: bool = True,
        offline: bool = False,
        unlock_all_dlcs: bool = False,
        dlcs: Optional[List[Tuple[int, str]]] = None,
        extra_backup_targets: Optional[List[Path]] = None,
        after_backup_hook: Optional[Callable[[], None]] = None,
        marker_extra: Optional[dict] = None,
    ) -> Path:
        game_root = Path(game_root).resolve()
        main_exe = Path(main_exe).resolve()
        profile = (profile or "regular").strip().lower()

        if not targets:
            raise RuntimeError("No Steam API targets found in game folder.")

        exe_dir = main_exe.parent
        target_dirs = sorted({t.path.parent for t in targets}, key=lambda p: str(p).lower())

        # Extract interfaces from original target DLLs before modifying anything.
        interface_mapping: Dict[str, str] = {}
        for t in targets:
            if t.path.is_file():
                extractor = SteamInterfaceExtractor(t.path)
                mapping = extractor.extract_interfaces()
                if mapping:
                    interface_mapping.update(mapping)
                    self.log(f"Extracted {len(mapping)} Steam interfaces from {t.path.name}")

        # Plan files for backup
        planned: List[Path] = [t.path for t in targets]
        planned.append(game_root / ".gse_auto_setup.json")
        planned.append(exe_dir / "steam_appid.txt")
        planned.append(game_root / "steam_appid.txt")
        for t in targets:
            planned.extend([
                adjacent_backup_path(t.path),
                fallback_adjacent_backup_path(t.path),
            ])

        if profile == "regular":
            for td in target_dirs:
                planned.append(td / "steam_emu.ini")
        elif profile == "steakclient":
            planned.extend([
                exe_dir / "winmm.dll",
                exe_dir / "steakclient64.dll",
                exe_dir / "steak_emu.ini",
            ])
        elif profile == "steamclient":
            for t in targets:
                td = t.path.parent
                if t.arch == "x64":
                    planned.extend([
                        td / "steamclient64.dll",
                        td / "rune64.dll",
                        td / "GameOverlayRenderer64.dll",
                        td / "steam_emu.ini",
                    ])
                else:
                    planned.extend([
                        td / "steamclient.dll",
                        td / "rune.dll",
                        td / "GameOverlayRenderer.dll",
                        td / "steam_emu.ini",
                    ])

        if extra_backup_targets:
            planned.extend(Path(p) for p in extra_backup_targets)

        manifest = backup_files(game_root, planned, self.backup_root)

        try:
            if after_backup_hook is not None:
                after_backup_hook()

            # Execute profile deployment
            if profile == "regular":
                emu_root = self.manager.emu_root()
                template_ini = (emu_root / "steam_emu.ini").read_text(encoding="utf-8", errors="replace")

                for target in targets:
                    ensure_adjacent_original_backup(target.path, self.log)
                    src_dll = emu_root / ("steam_api64.dll" if target.arch == "x64" else "steam_api.dll")
                    if not src_dll.is_file():
                        raise FileNotFoundError(f"RUNE emulator DLL not found: {src_dll}")
                    shutil.copy2(src_dll, target.path)
                    self.log(f"Deployed RUNE Regular {target.arch} -> {target.path}")

                    ini_content = self._render_ini_content(
                        template_ini,
                        appid=appid,
                        username=username,
                        language=language,
                        lobby_enabled=lobby_enabled,
                        overlays_enabled=overlays_enabled,
                        offline=offline,
                        unlock_all_dlcs=unlock_all_dlcs,
                        dlcs=dlcs,
                        interfaces=interface_mapping,
                    )
                    ini_path = target.path.parent / "steam_emu.ini"
                    ini_path.write_text(ini_content, encoding="utf-8")
                    self.log(f"Generated RUNE config: {ini_path}")

            elif profile == "steakclient":
                sc_root = self.manager.steakclient_root()
                winmm_src = sc_root / "winmm.dll"
                sc_dll_src = sc_root / "steakclient64.dll"
                template_ini = (sc_root / "steak_emu.ini").read_text(encoding="utf-8", errors="replace")

                shutil.copy2(winmm_src, exe_dir / "winmm.dll")
                shutil.copy2(sc_dll_src, exe_dir / "steakclient64.dll")
                self.log(f"Deployed Steakclient loader proxy: {exe_dir / 'winmm.dll'}")
                self.log(f"Deployed Steakclient backend: {exe_dir / 'steakclient64.dll'}")

                ini_content = self._render_ini_content(
                    template_ini,
                    appid=appid,
                    username=username,
                    language=language,
                    lobby_enabled=lobby_enabled,
                    overlays_enabled=overlays_enabled,
                    offline=offline,
                    unlock_all_dlcs=unlock_all_dlcs,
                    dlcs=dlcs,
                    interfaces=interface_mapping,
                )
                ini_path = exe_dir / "steak_emu.ini"
                ini_path.write_text(ini_content, encoding="utf-8")
                self.log(f"Generated Steakclient config: {ini_path}")

            elif profile == "steamclient":
                sc_base = self.manager.steamclient_root()
                for target in targets:
                    td = target.path.parent
                    arch_dir = sc_base / ("x64" if target.arch == "x64" else "x86")
                    template_ini = (arch_dir / "steam_emu.ini").read_text(encoding="utf-8", errors="replace")

                    # Copy support files
                    for file_item in arch_dir.iterdir():
                        if file_item.is_file() and file_item.name != "steam_emu.ini":
                            shutil.copy2(file_item, td / file_item.name)
                            self.log(f"Deployed {file_item.name} -> {td}")

                    # Binary patch SHELL32.dll string in steam_api(64).dll
                    patched = patch_shell32_string(target.path, is64=(target.arch == "x64"))
                    if patched:
                        self.log(f"Patched SHELL32.dll hook in {target.path.name}")
                    else:
                        self.log(f"Warning: SHELL32.dll string not found in {target.path.name}")

                    # Write steam_emu.ini
                    ini_content = self._render_ini_content(
                        template_ini,
                        appid=appid,
                        username=username,
                        language=language,
                        lobby_enabled=lobby_enabled,
                        overlays_enabled=overlays_enabled,
                        offline=offline,
                        unlock_all_dlcs=unlock_all_dlcs,
                        dlcs=dlcs,
                        interfaces=interface_mapping,
                    )
                    ini_path = td / "steam_emu.ini"
                    ini_path.write_text(ini_content, encoding="utf-8")
                    self.log(f"Generated RUNE steamclient config: {ini_path}")

            # Write steam_appid.txt at game root & exe folder
            (exe_dir / "steam_appid.txt").write_text(str(int(appid)), encoding="ascii")
            (game_root / "steam_appid.txt").write_text(str(int(appid)), encoding="ascii")

            # Write installation marker
            marker_path = game_root / ".gse_auto_setup.json"
            marker_path.write_text(
                json.dumps(
                    {
                        "engine": "rune",
                        "profile": profile,
                        "appid": appid,
                        "username": username,
                        "language": language,
                        "backup_manifest": str(manifest),
                        "targets": [str(t.path) for t in targets],
                        "interfaces_count": len(interface_mapping),
                        "dlcs_count": len(dlcs or []),
                        **(marker_extra or {}),
                    },
                    indent=2,
                ),
                encoding="utf-8",
            )
            return manifest

        except Exception:
            self.log("RUNE installation failed; restoring original files...")
            restore_manifest(manifest)
            raise
