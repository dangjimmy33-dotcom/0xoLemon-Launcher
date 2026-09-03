from pathlib import Path
from gse_autosetup.core.official_generator import find_generator_executable, find_generated_settings

def test_finds_official_generator_and_appid_output(tmp_path):
    exe = tmp_path / "bundle" / "generate_emu_config.exe"
    exe.parent.mkdir(parents=True)
    exe.write_bytes(b"MZ")
    settings = exe.parent / "_OUTPUT" / "480" / "steam_settings"
    settings.mkdir(parents=True)
    assert find_generator_executable(tmp_path) == exe
    assert find_generated_settings(exe.parent, 480) == settings

from gse_autosetup.core.official_generator import merge_settings_tree


def test_merge_settings_tree_does_not_deploy_markdown_or_docs(tmp_path):
    src = tmp_path / "src"
    dst = tmp_path / "dst"
    src.mkdir()
    (src / "configs.user.ini").write_text("[x]\na=1\n", encoding="utf-8")
    (src / "README.md").write_text("docs", encoding="utf-8")
    (src / "CHANGELOG.md").write_text("docs", encoding="utf-8")
    nested = src / "sounds"
    nested.mkdir()
    (nested / "overlay.wav").write_bytes(b"wav")
    (nested / "README.md").write_text("docs", encoding="utf-8")
    merge_settings_tree(src, dst)
    assert (dst / "configs.user.ini").is_file()
    assert (dst / "sounds" / "overlay.wav").is_file()
    assert not (dst / "README.md").exists()
    assert not (dst / "CHANGELOG.md").exists()
    assert not (dst / "sounds" / "README.md").exists()

from gse_autosetup.core.official_generator import build_generator_command


def test_official_generator_keeps_achievement_generation_enabled():
    cmd = build_generator_command(Path('generate_emu_config.exe'), 480)
    assert '-skip_ach' not in cmd
    assert cmd[-1] == '480'


def test_generator_can_skip_redundant_achievement_downloads_for_fast_retry():
    from gse_autosetup.core.official_generator import build_generator_command
    cmd = build_generator_command(Path('generate_emu_config.exe'), 480, skip_achievements=True)
    assert '-def1' in cmd
    assert '-skip_ach' in cmd
    assert cmd[-1] == '480'


def test_official_generator_idle_timeout_prevents_infinite_hang(tmp_path, monkeypatch):
    import sys
    import pytest
    import gse_autosetup.core.official_generator as official_generator

    # Use the current Python interpreter as a real cross-platform child process.
    # A text file merely named *.exe is executable via shebang on POSIX, but on
    # Windows CreateProcess rejects it with WinError 216 before timeout logic is
    # exercised. Monkeypatching the command keeps this test about the timeout.
    monkeypatch.setattr(
        official_generator,
        "find_generator_executable",
        lambda _root: Path(sys.executable),
    )
    monkeypatch.setattr(
        official_generator,
        "build_generator_command",
        lambda _exe, _appid, **_kwargs: [
            sys.executable,
            "-c",
            "import time; time.sleep(30)",
        ],
    )

    with pytest.raises(TimeoutError, match="no output"):
        official_generator.run_official_generator(
            tmp_path, 480, timeout=5, idle_timeout=1
        )


def test_clean_subprocess_env_resets_nested_pyinstaller_state(monkeypatch):
    import gse_autosetup.core.official_generator as official_generator

    monkeypatch.setenv('_PYI_ARCHIVE_FILE', 'parent.pkg')
    monkeypatch.setenv('_PYI_PARENT_PROCESS_LEVEL', '2')
    monkeypatch.setenv('_MEIPASS', 'C:/parent/_MEI')
    monkeypatch.setenv('PYTHONHOME', 'C:/parent/python')
    env = official_generator.clean_subprocess_env()

    assert env['PYINSTALLER_RESET_ENVIRONMENT'] == '1'
    assert env['PYTHONUNBUFFERED'] == '1'
    assert '_PYI_ARCHIVE_FILE' not in env
    assert '_PYI_PARENT_PROCESS_LEVEL' not in env
    assert '_MEIPASS' not in env
    assert 'PYTHONHOME' not in env
