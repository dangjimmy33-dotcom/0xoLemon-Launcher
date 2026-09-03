from gse_autosetup.core.config_builder import schema_to_achievements, schema_to_stats

def test_schema_conversion_maps_achievements_and_stats():
    schema = {"game": {"availableGameStats": {
        "achievements": [{"name":"ACH_WIN","displayName":"Winner","description":"Win once","hidden":1,"icon":"https://x/a.jpg","icongray":"https://x/b.jpg"}],
        "stats": [{"name":"kills","type":"INT","defaultvalue":3,"displayName":"Kills"}]
    }}}
    achievements = schema_to_achievements(schema)
    stats = schema_to_stats(schema)
    assert achievements[0]["name"] == "ACH_WIN"
    assert achievements[0]["hidden"] == 1
    assert achievements[0]["displayName"]["english"] == "Winner"
    assert stats == [{"name":"kills","type":"int","default":"3","global":"0"}]


def test_v18_network_and_overlay_metrics_are_written(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {"game": {"availableGameStats": {}}}, "0xoLemon",
        network_mode="singleplayer", enable_overlay=True,
        overlay_fps=True, overlay_frametime=True, overlay_playtime=True,
    )
    main = (settings / "configs.main.ini").read_text(encoding="utf-8")
    overlay = (settings / "configs.overlay.ini").read_text(encoding="utf-8")
    assert "offline=0" in main
    assert "disable_networking=1" in main
    assert "record_playtime=1" in main
    assert "overlay_always_show_fps=1" in overlay
    assert "overlay_always_show_frametime=1" in overlay
    assert "overlay_always_show_playtime=1" in overlay


def test_gse_strict_offline_reports_steam_offline(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {"game": {"availableGameStats": {}}}, "0xoLemon",
        network_mode="strict_offline",
    )
    main = (settings / "configs.main.ini").read_text(encoding="utf-8")
    assert "offline=1" in main
    assert "disable_networking=1" in main


def test_gse_lan_keeps_networking_and_logged_on_state(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {"game": {"availableGameStats": {}}}, "0xoLemon",
        network_mode="lan",
    )
    main = (settings / "configs.main.ini").read_text(encoding="utf-8")
    assert "offline=0" in main
    assert "disable_networking=0" in main


def test_legacy_offline_alias_uses_safe_singleplayer_semantics(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {"game": {"availableGameStats": {}}}, "0xoLemon",
        network_mode="offline",
    )
    main = (settings / "configs.main.ini").read_text(encoding="utf-8")
    assert "offline=0" in main
    assert "disable_networking=1" in main


def test_v184_overlay_full_preferences_are_written(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / "steam_settings"
    write_basic_settings(
        settings, 480, {"game": {"availableGameStats": {}}}, "0xoLemon",
        enable_overlay=True,
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
    overlay = (settings / "configs.overlay.ini").read_text(encoding="utf-8")
    assert "enable_experimental_overlay=1" in overlay
    assert "disable_achievement_notification=1" in overlay
    assert "disable_friend_notification=1" in overlay
    assert "disable_achievement_progress=0" in overlay
    assert "upload_achievements_icons_to_gpu=0" in overlay
    assert "overlay_always_show_user_info=1" in overlay
    assert "overlay_always_show_playtime=1" in overlay
    assert "disable_warning_any=1" in overlay
    assert "hook_delay_sec=2" in overlay
    assert "renderer_detector_timeout_sec=20" in overlay
    assert "PosAchievement=top_right" in overlay
    assert "Font_Size=22.0" in overlay
    assert "Icon_Size=72.0" in overlay
    assert "Notification_Rounding=14.0" in overlay
    assert "Notification_Animation=0.25" in overlay
    assert "Notification_Duration_Achievement=5.5" in overlay
    assert "key_combo=ctrl + shift + tab" in overlay
