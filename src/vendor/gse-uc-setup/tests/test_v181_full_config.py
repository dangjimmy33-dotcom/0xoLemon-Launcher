from pathlib import Path

from gse_autosetup.core.config_builder import materialize_canonical_defaults
from gse_autosetup.core.official_generator import (
    build_generator_command,
    merge_settings_tree,
    validate_settings_mirror,
)


def test_generator_uses_gse_default_preset_without_unrelated_emu_exports():
    cmd = build_generator_command(Path('generate_emu_config.exe'), 1940340)
    assert '-def1' in cmd
    assert '-anon' in cmd
    assert '-clr' in cmd
    assert '-cdx' not in cmd
    assert '-rne' not in cmd
    assert '-acw' not in cmd
    assert cmd[-1] == '1940340'


def test_canonical_defaults_materialize_runtime_examples_without_docs(tmp_path):
    examples = tmp_path / 'steam_settings.EXAMPLE'
    examples.mkdir()
    (examples / 'configs.main.EXAMPLE.ini').write_text('[main::connectivity]\noffline=0\n', encoding='utf-8')
    (examples / 'configs.user.EXAMPLE.ini').write_text('[user::general]\naccount_name=x\n', encoding='utf-8')
    (examples / 'configs.app.EXAMPLE.ini').write_text('[app::general]\nbranch_name=public\n', encoding='utf-8')
    (examples / 'configs.overlay.EXAMPLE.ini').write_text('[overlay::general]\nenable_experimental_overlay=0\n', encoding='utf-8')
    controller = examples / 'controller.EXAMPLE'
    controller.mkdir()
    (controller / 'MenuControls.txt').write_text('menu', encoding='utf-8')
    fonts = examples / 'fonts.EXAMPLE'
    fonts.mkdir()
    (fonts / 'Roboto-Medium.ttf').write_bytes(b'font')
    (fonts / 'README.md').write_text('docs', encoding='utf-8')
    sounds = examples / 'sounds.EXAMPLE'
    sounds.mkdir()
    (sounds / 'overlay_achievement_notification.wav').write_bytes(b'wav')
    (sounds / 'LICENSE.md').write_text('docs', encoding='utf-8')

    dst = tmp_path / 'steam_settings'
    copied = materialize_canonical_defaults(examples, dst)

    assert (dst / 'configs.main.ini').is_file()
    assert (dst / 'configs.user.ini').is_file()
    assert (dst / 'configs.app.ini').is_file()
    assert (dst / 'configs.overlay.ini').is_file()
    assert (dst / 'controller' / 'MenuControls.txt').is_file()
    assert (dst / 'fonts' / 'Roboto-Medium.ttf').is_file()
    assert (dst / 'sounds' / 'overlay_achievement_notification.wav').is_file()
    assert not (dst / 'fonts' / 'README.md').exists()
    assert not (dst / 'sounds' / 'LICENSE.md').exists()
    assert 'configs.app.ini' in copied


def test_generated_runtime_tree_is_mirrored_completely(tmp_path):
    src = tmp_path / 'generated'
    dst = tmp_path / 'installed'
    (src / 'controller').mkdir(parents=True)
    (src / 'img').mkdir()
    (src / 'configs.app.ini').write_text('[x]\n', encoding='utf-8')
    (src / 'configs.overlay.ini').write_text('[o]\n', encoding='utf-8')
    (src / 'branches.json').write_text('{}', encoding='utf-8')
    (src / 'depots.txt').write_text('123\n', encoding='utf-8')
    (src / 'supported_languages.txt').write_text('english\n', encoding='utf-8')
    (src / 'controller' / 'InGameControls.txt').write_text('controls', encoding='utf-8')
    (src / 'img' / 'abc.jpg').write_bytes(b'jpg')
    (src / 'README.md').write_text('docs', encoding='utf-8')

    merge_settings_tree(src, dst)
    validate_settings_mirror(src, dst)

    for rel in [
        'configs.app.ini', 'configs.overlay.ini', 'branches.json', 'depots.txt',
        'supported_languages.txt', 'controller/InGameControls.txt', 'img/abc.jpg'
    ]:
        assert (dst / rel).is_file(), rel
    assert not (dst / 'README.md').exists()


def test_limited_fallback_writes_supported_languages_without_overwriting_generator_file(tmp_path):
    from gse_autosetup.core.config_builder import write_basic_settings
    settings = tmp_path / 'steam_settings'
    schemas = {'english': {}, 'japanese': {}, 'german': {}}
    write_basic_settings(settings, 480, {}, 'Player', localized_schemas=schemas)
    assert (settings / 'supported_languages.txt').read_text(encoding='utf-8').splitlines() == [
        'english', 'japanese', 'german'
    ]
    (settings / 'supported_languages.txt').write_text('english\nfrench\n', encoding='utf-8')
    write_basic_settings(settings, 480, {}, 'Player', localized_schemas=schemas)
    assert (settings / 'supported_languages.txt').read_text(encoding='utf-8') == 'english\nfrench\n'
