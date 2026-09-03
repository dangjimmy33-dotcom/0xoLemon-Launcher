from __future__ import annotations

import os
import sys
import tempfile
import zipfile
from dataclasses import fields
from pathlib import Path

# Build script adds vendor root to PYTHONPATH / sys.path.
from gse_autosetup.service import Inputs
from gse_autosetup.save_manager.backup import MANIFEST_NAME, SaveBackupManager
import gse_core_bridge


def test_bridge_maps_complete_setup_contract() -> None:
    with tempfile.TemporaryDirectory() as td:
        game = Path(td) / "game"
        game.mkdir()
        os.environ["GSE_STEAM_WEB_API_KEY"] = "X" * 32
        config = {
            "appId": 945360,
            "gameFolder": str(game),
            "engine": "gse",
            "gseVariant": "experimental",
            "networkMode": "singleplayer",
            "steamstubMode": "auto",
            "accountName": "0xoLemon",
            "saveMode": "gse",
            "customSavePath": "",
            "ucSpoofAppid": 480,
            "ucPlugins": ["auto"],
            "coldclientRenderer": True,
            "coldclientExtra": False,
            "overlay": True,
            "overlayAchievementNotifications": True,
            "overlayAchievementProgress": True,
            "overlayFriendNotifications": True,
            "overlayIcons": True,
            "overlayUserInfo": False,
            "overlayWarnings": True,
            "overlayFps": True,
            "overlayFrametime": True,
            "overlayShowPlaytime": True,
            "overlayPlaytime": True,
            "overlayPosition": "bot_right",
            "overlayHotkey": "shift + tab",
            "overlayFontSize": 20,
            "overlayIconSize": 64,
            "overlayRounding": 10,
            "overlayAnimation": 0.35,
            "overlayAchievementDuration": 7,
            "overlayHookDelay": 0,
            "overlayRendererTimeout": 15,
            "overlayDinputBridge": False,
            "officialGenerator": True,
            "runeProfile": "regular",
            "runeUsername": "RUNE",
            "runeLanguage": "english",
            "runeUnlockAllDlcs": False,
            "runeLobby": True,
            "runeOverlays": True,
            "runeOffline": False,
        }
        mapped = gse_core_bridge._to_inputs(config)
        assert isinstance(mapped, Inputs)
        assert mapped.appid == 945360
        assert mapped.effective_experimental is True
        assert mapped.use_official_generator is True
        assert mapped.overlay_achievement_progress is True
        assert mapped.overlay_show_playtime is True
        assert mapped.uc_plugins == ("auto",)
        assert len(fields(Inputs)) >= 46


def test_original_save_manager_transaction() -> None:
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        live_root = root / "live"
        save = live_root / "945360"
        save.mkdir(parents=True)
        (save / "save.dat").write_bytes(b"original-save")
        backup_root = root / "backups"
        manager = SaveBackupManager(backup_root)

        archive = manager.create_backup(945360, save, "Among Us")
        assert archive.is_file()
        assert archive.with_suffix(".json").is_file()
        assert not archive.with_suffix(".zip.part").exists()
        with zipfile.ZipFile(archive, "r") as zf:
            assert zf.testzip() is None
            assert MANIFEST_NAME in zf.namelist()

        (save / "save.dat").write_bytes(b"changed-save")
        result = manager.restore_backup(archive, live_root, expected_appid=945360)
        assert (result.restored_folder / "save.dat").read_bytes() == b"original-save"
        assert result.safety_backup is not None and result.safety_backup.is_file()


if __name__ == "__main__":
    test_bridge_maps_complete_setup_contract()
    test_original_save_manager_transaction()
    print("GSE original-core parity contracts: PASS")
