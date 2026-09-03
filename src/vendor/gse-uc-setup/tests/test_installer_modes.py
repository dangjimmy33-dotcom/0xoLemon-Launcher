from pathlib import Path

import gse_autosetup.core.installer as installer_mod
from gse_autosetup.core.installer import Installer, restore_manifest
from gse_autosetup.core.models import SteamApiTarget
from gse_autosetup.core.package import GSEPackage


def _fake_package(tmp_path: Path) -> GSEPackage:
    root = tmp_path / "pkg"
    (root / "regular" / "x64").mkdir(parents=True)
    (root / "regular" / "x64" / "steam_api64.dll").write_bytes(b"GSE-API")
    (root / "experimental" / "x64").mkdir(parents=True)
    (root / "experimental" / "x64" / "steam_api64.dll").write_bytes(b"GSE-API-EXP")
    (root / "experimental" / "x64" / "steamclient64.dll").write_bytes(b"GSE-STEAMCLIENT")
    (root / "tools" / "generate_interfaces").mkdir(parents=True)
    (root / "tools" / "generate_interfaces" / "generate_interfaces_x64.exe").write_bytes(b"tool")
    return GSEPackage(root, "test")


def test_replace_mode_keeps_original_beside_replacement_and_restore_removes_backup(tmp_path, monkeypatch):
    package = _fake_package(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    api = game / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    monkeypatch.setattr(installer_mod, "_generate_interfaces", lambda *_: "SteamClient020\n")

    inst = Installer(package, tmp_path / "backup")
    manifest = inst.install(game, [SteamApiTarget(api, "x64")], 123, {}, "0xoLemon")

    assert api.read_bytes() == b"GSE-API"
    assert (game / "steam_api64.dll.bak").read_bytes() == b"ORIGINAL"
    restore_manifest(manifest)
    assert api.read_bytes() == b"ORIGINAL"
    assert not (game / "steam_api64.dll.bak").exists()


def test_preserve_mode_leaves_original_api_byte_identical(tmp_path, monkeypatch):
    package = _fake_package(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    api = game / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    exe = game / "game.exe"
    exe.write_bytes(b"MZ-not-used")
    monkeypatch.setattr(installer_mod, "_generate_interfaces", lambda *_: "SteamClient020\n")

    inst = Installer(package, tmp_path / "backup")
    manifest = inst.install_preserve(
        game, [SteamApiTarget(api, "x64")], exe, 123, {}, "0xoLemon", steamstub_enabled=False
    )

    assert api.read_bytes() == b"ORIGINAL"
    assert (game / "version.dll").is_file()
    assert (game / "coldloader.asi").is_file()
    assert (game / "gse_steamclient64.dll").read_bytes() == b"GSE-STEAMCLIENT"
    ini = (game / "coldloader.ini").read_text(encoding="utf-8")
    assert "appid = 123" in ini
    assert 'gse_steamclient64.dll' in ini
    restore_manifest(manifest)
    assert api.read_bytes() == b"ORIGINAL"
    assert not (game / "version.dll").exists()


def test_replace_rerun_generates_interfaces_from_adjacent_original_backup(tmp_path, monkeypatch):
    package = _fake_package(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    api = game / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")

    seen = []
    def fake_generate(_generator, original_dll):
        seen.append(Path(original_dll))
        return "SteamClient020\n"

    monkeypatch.setattr(installer_mod, "_generate_interfaces", fake_generate)
    inst = Installer(package, tmp_path / "backup1")
    inst.install(game, [SteamApiTarget(api, "x64")], 123, {}, "0xoLemon")

    assert api.read_bytes() == b"GSE-API"
    assert (game / "steam_api64.dll.bak").read_bytes() == b"ORIGINAL"

    seen.clear()
    inst2 = Installer(package, tmp_path / "backup2")
    inst2.install(game, [SteamApiTarget(api, "x64")], 123, {}, "0xoLemon")

    assert seen == [game / "steam_api64.dll.bak"]
    assert (game / "steam_api64.dll.bak").read_bytes() == b"ORIGINAL"


def test_replace_mode_preserves_official_generated_achievement_schema(tmp_path, monkeypatch):
    package = _fake_package(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    api = game / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    monkeypatch.setattr(installer_mod, "_generate_interfaces", lambda *_: "SteamClient020\n")

    generated = tmp_path / "official" / "steam_settings"
    generated.mkdir(parents=True)
    canonical = '[\n  {\n    "hidden": 0,\n    "displayName": {"english": "Winner", "japanese": "勝者"},\n    "token": "ACH_TOKEN",\n    "name": "ACH_ONE"\n  }\n]\n'
    (generated / "achievements.json").write_text(canonical, encoding="utf-8")

    web_schema = {"game": {"availableGameStats": {"achievements": [{
        "name": "ACH_ONE", "displayName": "Winner", "description": "Win once",
        "hidden": 0,
    }]}}}

    inst = Installer(package, tmp_path / "backup")
    inst.install(
        game, [SteamApiTarget(api, "x64")], 123, web_schema, "0xoLemon",
        generated_settings=generated,
    )

    deployed = game / "steam_settings" / "achievements.json"
    assert deployed.read_text(encoding="utf-8") == canonical


def test_generated_config_without_achievement_schema_does_not_preserve_stale_game_schema(tmp_path, monkeypatch):
    package = _fake_package(tmp_path)
    game = tmp_path / "game"
    game.mkdir()
    api = game / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL")
    settings = game / "steam_settings"
    settings.mkdir()
    (settings / "achievements.json").write_text('[{"name":"STALE"}]', encoding="utf-8")
    monkeypatch.setattr(installer_mod, "_generate_interfaces", lambda *_: "SteamClient020\n")

    generated = tmp_path / "official" / "steam_settings"
    generated.mkdir(parents=True)
    (generated / "configs.app.ini").write_text("[app::general]\n", encoding="utf-8")

    web_schema = {"game": {"availableGameStats": {"achievements": [{
        "name": "ACH_ONE", "displayName": "Winner", "description": "Win once", "hidden": 0,
    }]}}}

    inst = Installer(package, tmp_path / "backup")
    inst.install(
        game, [SteamApiTarget(api, "x64")], 123, web_schema, "0xoLemon",
        generated_settings=generated,
    )

    import json
    deployed = json.loads((settings / "achievements.json").read_text(encoding="utf-8"))
    assert deployed[0]["name"] == "ACH_ONE"
