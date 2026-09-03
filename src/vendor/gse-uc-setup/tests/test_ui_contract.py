from pathlib import Path


def test_v18_ui_exposes_engine_network_drm_resource_and_migration_controls():
    source = (Path(__file__).parents[1] / 'gse_autosetup' / 'ui' / 'main_window.py').read_text(encoding='utf-8')

    required = [
        'UC Online',
        'ColdClient',
        'Spacewar 480',
        'Steamless',
        'RUNE SteamStub Patcher',
        'UC Runtime SteamStub',
        'Reduced motion',
        'Migrate old Goldberg settings',
        'Resources & updates',
        'GameOverlayRenderer',
        'FPS counter',
        'Frametime',
        'Record playtime',
        'self.engine_gse',
        'self.engine_uc',
    ]
    for needle in required:
        assert needle in source

    assert 'RUNE SteamStub runtime' not in source


def test_v18_theme_has_native_glass_hierarchy_and_motion_friendly_controls():
    theme = (Path(__file__).parents[1] / 'gse_autosetup' / 'ui' / 'theme.py').read_text(encoding='utf-8')
    for needle in ['QWidget#TopBar', 'QFrame#GlassCard', 'QFrame#GlassCardStrong', 'rgba(', 'QPushButton#ModeCard']:
        assert needle in theme


def test_v18_ui_calls_validation_with_text_fields_not_parsed_values():
    source = (Path(__file__).parents[1] / 'gse_autosetup' / 'ui' / 'main_window.py').read_text(encoding='utf-8')
    assert 'validate_inputs(self.appid.text(), self.game_folder.text(), api_key, engine=engine)' in source


def test_v184_ui_exposes_full_overlay_controls_and_clear_master_switch():
    source = (Path(__file__).parents[1] / 'gse_autosetup' / 'ui' / 'main_window.py').read_text(encoding='utf-8')
    for needle in [
        'Enable GSE overlay',
        'Achievement popup',
        'Achievement progress',
        'Friend notifications',
        'Achievement icons',
        'Show user info',
        'Show playtime',
        'Achievement position',
        'Overlay hotkey',
        'Font size',
        'Icon size',
        'Popup rounding',
        'Popup animation',
        'Achievement duration',
        'Hook delay',
        'Renderer timeout',
        'Overlay warnings',
    ]:
        assert needle in source


def test_gse_connectivity_ui_exposes_singleplayer_strict_offline_and_lan():
    from pathlib import Path
    source = (Path(__file__).parents[1] / "gse_autosetup" / "ui" / "main_window.py").read_text(encoding="utf-8")
    assert '("Single-player", "singleplayer")' in source
    assert '("Strict offline", "strict_offline")' in source
    assert '("LAN", "lan")' in source
    assert 'Steam reports logged-on' in source
