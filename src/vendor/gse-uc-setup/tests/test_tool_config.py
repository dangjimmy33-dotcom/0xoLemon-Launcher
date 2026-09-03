from gse_autosetup.core.tool_config import ToolConfig, load_config, save_config


def test_config_round_trip_including_secret(tmp_path):
    path = tmp_path / "config.ini"
    cfg = ToolConfig(
        last_appid="1940340",
        last_game_folder=r"E:\\Games\\DD2",
        account_name="0xoLemon",
        deployment_mode="preserve",
        overlay=True,
        steamstub=True,
        remember_web_api_key=True,
        steam_web_api_key="ABCDEF0123456789",
    )
    save_config(cfg, path)
    raw = path.read_text(encoding="utf-8")
    assert "ABCDEF0123456789" not in raw
    loaded = load_config(path)
    assert loaded.last_appid == "1940340"
    assert loaded.deployment_mode == "preserve"
    assert loaded.overlay is True
    assert loaded.steam_web_api_key == "ABCDEF0123456789"


def test_config_can_forget_secret(tmp_path):
    path = tmp_path / "config.ini"
    save_config(ToolConfig(remember_web_api_key=False, steam_web_api_key="SECRET"), path)
    assert "SECRET" not in path.read_text(encoding="utf-8")
    assert load_config(path).steam_web_api_key == ""


def test_v18_engine_preferences_round_trip(tmp_path):
    path = tmp_path / "config.ini"
    cfg = ToolConfig(
        engine="uc",
        gse_variant="coldclient",
        network_mode="lan",
        steamstub_mode="steamless",
        uc_spoof_appid=480,
        uc_plugins="eos,photon",
        coldclient_renderer=True,
        coldclient_extra=True,
        reduced_motion=True,
        overlay_fps=True,
        overlay_frametime=True,
        overlay_playtime=True,
    )
    save_config(cfg, path)
    loaded = load_config(path)
    assert loaded.engine == "uc"
    assert loaded.gse_variant == "coldclient"
    assert loaded.network_mode == "lan"
    assert loaded.steamstub_mode == "steamless"
    assert loaded.uc_spoof_appid == 480
    assert loaded.uc_plugins == "eos,photon"
    assert loaded.coldclient_renderer is True
    assert loaded.coldclient_extra is True
    assert loaded.reduced_motion is True
    assert loaded.overlay_fps is True


def test_v184_overlay_preferences_round_trip(tmp_path):
    from gse_autosetup.core.tool_config import ToolConfig, save_config, load_config
    path = tmp_path / "config.ini"
    cfg = ToolConfig(
        overlay=True,
        overlay_achievement_notifications=False,
        overlay_friend_notifications=False,
        overlay_achievement_progress=True,
        overlay_icons=False,
        overlay_user_info=True,
        overlay_show_playtime=True,
        overlay_position="top_right",
        overlay_hotkey="ctrl + shift + tab",
        overlay_font_size=22.0,
        overlay_icon_size=72.0,
        overlay_rounding=14.0,
        overlay_animation=0.25,
        overlay_achievement_duration=5.5,
        overlay_hook_delay=2,
        overlay_renderer_timeout=20,
        overlay_warnings=False,
    )
    save_config(cfg, path)
    out = load_config(path)
    assert out.overlay is True
    assert out.overlay_achievement_notifications is False
    assert out.overlay_friend_notifications is False
    assert out.overlay_achievement_progress is True
    assert out.overlay_icons is False
    assert out.overlay_user_info is True
    assert out.overlay_show_playtime is True
    assert out.overlay_position == "top_right"
    assert out.overlay_hotkey == "ctrl + shift + tab"
    assert out.overlay_font_size == 22.0
    assert out.overlay_icon_size == 72.0
    assert out.overlay_rounding == 14.0
    assert out.overlay_animation == 0.25
    assert out.overlay_achievement_duration == 5.5
    assert out.overlay_hook_delay == 2
    assert out.overlay_renderer_timeout == 20
    assert out.overlay_warnings is False


def test_legacy_offline_network_mode_migrates_to_singleplayer(tmp_path):
    path = tmp_path / "config.ini"
    path.write_text("[tool]\nnetwork_mode=offline\n", encoding="utf-8")
    loaded = load_config(path)
    assert loaded.network_mode == "singleplayer"


def test_new_network_modes_round_trip(tmp_path):
    path = tmp_path / "config.ini"
    for mode in ("singleplayer", "strict_offline", "lan"):
        save_config(ToolConfig(network_mode=mode), path)
        assert load_config(path).network_mode == mode
