from pathlib import Path

from gse_autosetup.core.config_builder import write_basic_settings


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_default_account_and_gse_saves_mode(tmp_path):
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {}, "0xoLemon",
        save_mode="gse", custom_save_path="", enable_overlay=False,
    )
    text = _read(settings / "configs.user.ini")
    assert "account_name=0xoLemon" in text
    assert "local_save_path=" in text
    assert "saves_folder_name=GSE Saves" in text


def test_portable_save_mode_uses_game_relative_path(tmp_path):
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {}, "0xoLemon",
        save_mode="portable", custom_save_path="", enable_overlay=False,
    )
    text = _read(settings / "configs.user.ini")
    assert "local_save_path=./steam_settings/saves" in text


def test_custom_save_mode_writes_absolute_path(tmp_path):
    settings = tmp_path / "steam_settings"
    custom = r"D:\\My Saves\\Game 480"
    write_basic_settings(
        settings, 480, {}, "0xoLemon",
        save_mode="custom", custom_save_path=custom, enable_overlay=False,
    )
    text = _read(settings / "configs.user.ini")
    assert f"local_save_path={custom}" in text


def test_overlay_toggle_writes_config_and_copies_sound_assets(tmp_path):
    settings = tmp_path / "steam_settings"
    assets = tmp_path / "assets"
    (assets / "fonts").mkdir(parents=True)
    (assets / "sounds").mkdir(parents=True)
    (assets / "fonts" / "Roboto-Medium.ttf").write_bytes(b"FONT")
    (assets / "sounds" / "overlay_achievement_notification.wav").write_bytes(b"ACH")
    (assets / "sounds" / "overlay_friend_notification.wav").write_bytes(b"FRIEND")

    write_basic_settings(
        settings, 480, {}, "0xoLemon",
        save_mode="gse", custom_save_path="", enable_overlay=True,
        overlay_assets_root=assets,
    )
    overlay = _read(settings / "configs.overlay.ini")
    assert "enable_experimental_overlay=1" in overlay
    assert "key_combo=shift + tab" in overlay
    assert "Font_Override=Roboto-Medium.ttf" in overlay
    assert (settings / "fonts" / "Roboto-Medium.ttf").read_bytes() == b"FONT"
    assert (settings / "sounds" / "overlay_achievement_notification.wav").read_bytes() == b"ACH"
    assert (settings / "sounds" / "overlay_friend_notification.wav").read_bytes() == b"FRIEND"


def test_user_config_patch_preserves_official_generator_values(tmp_path):
    settings = tmp_path / "steam_settings"
    settings.mkdir()
    (settings / "configs.user.ini").write_text(
        "# generator comment\n[user::general]\naccount_name=old\nip_country=VN\n\n[user::saves]\nlocal_save_path=old\n",
        encoding="utf-8",
    )
    write_basic_settings(
        settings, 480, {}, "0xoLemon",
        save_mode="gse", custom_save_path="", enable_overlay=False,
    )
    text = _read(settings / "configs.user.ini")
    assert "# generator comment" in text
    assert "ip_country=VN" in text
    assert "account_name=0xoLemon" in text
    assert "local_save_path=" in text


def test_overlay_requires_experimental_gse_build(tmp_path):
    from gse_autosetup.service import Inputs
    base = dict(appid=480, game_folder=tmp_path, api_key="12345678")
    assert Inputs(**base, experimental=False, enable_overlay=False).effective_experimental is False
    assert Inputs(**base, experimental=True, enable_overlay=False).effective_experimental is True
    assert Inputs(**base, experimental=False, enable_overlay=True).effective_experimental is True
