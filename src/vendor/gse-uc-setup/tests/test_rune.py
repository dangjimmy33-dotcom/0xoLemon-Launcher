from pathlib import Path
import hashlib
import shutil
from unittest.mock import MagicMock, patch

from gse_autosetup.core.rune import RuneInstaller, RuneResourceManager, SteamInterfaceExtractor, patch_shell32_string
from gse_autosetup.core.scanner import scan_steam_api_targets
from gse_autosetup.service import Inputs, SetupService
from gse_autosetup.core.models import GameMetadata


def test_rune_interface_extractor(tmp_path: Path):
    dummy_dll = tmp_path / "steam_api64.dll"
    dummy_dll.write_bytes(
        b"MZ" + b"\x00"*60 + b"PE\x00\x00\x64\x86" + b"\x00"*100
        + b"SteamClient017\x00SteamUser019\x00SteamFriends015\x00STEAMAPPS_INTERFACE_VERSION008\x00"
    )
    extractor = SteamInterfaceExtractor(dummy_dll)
    ifaces = extractor.extract_interfaces()
    assert ifaces.get("SteamClient") == "SteamClient017"
    assert ifaces.get("SteamUser") == "SteamUser019"
    assert ifaces.get("SteamFriends") == "SteamFriends015"
    assert ifaces.get("SteamApps") == "STEAMAPPS_INTERFACE_VERSION008"


def test_rune_patch_shell32_string(tmp_path: Path):
    dummy_dll = tmp_path / "steam_api64.dll"
    dummy_dll.write_bytes(b"DATA_BEFORE_SHELL32.dll_DATA_AFTER")
    ok = patch_shell32_string(dummy_dll, is64=True)
    assert ok is True
    patched_data = dummy_dll.read_bytes()
    assert b"SHELL32.dll" not in patched_data
    assert b"RUNE64\x00WUS\x00" in patched_data


def test_rune_resource_manager():
    rm = RuneResourceManager()
    assert rm.emu_root().is_dir()
    assert rm.steakclient_root().is_dir()
    assert rm.steamclient_root().is_dir()
    assert rm.steamstub_root().is_dir()


def test_rune_multi_mode_switch_and_restore(tmp_path: Path):
    game_dir = tmp_path / "game"
    game_dir.mkdir()
    exe_file = game_dir / "Game.exe"
    api_dll = game_dir / "steam_api64.dll"

    real_gse_dll = Path(r"E:\GSE_UC_Setup_V1_8_2\GSE_UC_Setup_V1_8_2\resources\embedded\gse\regular\x64\steam_api64.dll")
    assert real_gse_dll.is_file()
    shutil.copy2(real_gse_dll, api_dll)
    exe_file.write_bytes(b"MZ" + b"\x00"*58 + b"\x80\x00\x00\x00" + b"\x00"*60 + b"PE\x00\x00\x64\x86" + b"\x00"*200)

    original_sha256 = hashlib.sha256(api_dll.read_bytes()).hexdigest()
    service = SetupService()

    mock_steam = MagicMock()
    mock_steam.get_schema.return_value = {}
    mock_steam.get_localized_schemas.return_value = {}
    mock_steam.download_file.return_value = True
    mock_steam.get_store_metadata.return_value = GameMetadata(480, "Spacewar")
    mock_steam.get_dlcs.return_value = [(481, "DLC 1"), (482, "DLC 2")]

    with patch.object(SetupService, "_steam_context", return_value=(mock_steam, {}, GameMetadata(480, "Spacewar"))):
        # 1. GSE Regular
        service._run_gse(Inputs(appid=480, game_folder=game_dir, api_key="", engine="gse", gse_variant="regular", use_official_generator=False))
        assert (game_dir / "steam_settings").is_dir()

        # 2. RUNE Regular
        service._run_rune(Inputs(appid=480, game_folder=game_dir, api_key="", engine="rune", rune_profile="regular"))
        assert not (game_dir / "steam_settings").exists()
        assert (game_dir / "steam_emu.ini").is_file()

        # 3. RUNE Steakclient
        service._run_rune(Inputs(appid=480, game_folder=game_dir, api_key="", engine="rune", rune_profile="steakclient"))
        assert not (game_dir / "steam_emu.ini").exists()
        assert (game_dir / "steak_emu.ini").is_file()
        assert (game_dir / "steakclient64.dll").is_file()

        # 4. RUNE Steamclient
        service._run_rune(Inputs(appid=480, game_folder=game_dir, api_key="", engine="rune", rune_profile="steamclient"))
        assert not (game_dir / "steak_emu.ini").exists()
        assert not (game_dir / "steakclient64.dll").exists()
        assert (game_dir / "rune64.dll").is_file()

        # 5. Restore
        service.restore(game_dir)
        assert not (game_dir / ".gse_auto_backup").exists()
        assert not (game_dir / ".gse_auto_setup.json").exists()
        assert not (game_dir / "rune64.dll").exists()
        assert not (game_dir / "steamclient64.dll").exists()
        assert not (game_dir / "steam_emu.ini").exists()
        assert not (game_dir / "steam_appid.txt").exists()

        restored_sha256 = hashlib.sha256(api_dll.read_bytes()).hexdigest()
        assert restored_sha256 == original_sha256
