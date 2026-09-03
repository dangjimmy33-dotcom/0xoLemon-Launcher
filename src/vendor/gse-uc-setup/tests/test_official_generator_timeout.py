import io
import subprocess
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from gse_autosetup.core import official_generator as og


def _make_generator(tmp_path: Path) -> Path:
    exe = tmp_path / "generate_emu_config.exe"
    exe.write_bytes(b"MZ")
    return exe


def test_generator_command_keeps_official_achievement_generation_enabled(tmp_path):
    exe = _make_generator(tmp_path)
    cmd = og.build_generator_command(exe, 1940340)
    assert cmd[0] == str(exe)
    assert "-anon" in cmd
    assert "-skip_ach" not in cmd
    assert cmd[-1] == "1940340"


def _make_fake_popen(stdout_lines: list[str], returncode: int = 0):
    """Return a context-manager-compatible Popen mock."""
    proc = MagicMock()
    proc.stdout = iter(line + "\n" for line in stdout_lines)
    proc.returncode = returncode
    proc.wait.return_value = None
    return proc


def test_generator_streams_output_and_calls_progress(tmp_path, monkeypatch):
    """run_official_generator must stream stdout and invoke the progress callback."""
    _make_generator(tmp_path)

    fake_proc = _make_fake_popen(
        ["Connecting to Steam...", "Getting app info...", "Fetching achievements...", "Done."],
        returncode=0,
    )

    # Patch find_generated_settings so we don't need real output on disk
    fake_settings = tmp_path / "steam_settings"
    fake_settings.mkdir()
    monkeypatch.setattr(og, "find_generated_settings", lambda *_a, **_kw: fake_settings)

    with patch.object(og.subprocess, "Popen", return_value=fake_proc):
        progress_calls: list[tuple[int, str]] = []
        result = og.run_official_generator(
            tmp_path, 1940340, progress=lambda pct, msg: progress_calls.append((pct, msg))
        )

    assert result == fake_settings
    assert len(progress_calls) >= 4, "progress must be called for every non-empty output line"
    pcts = [p for p, _ in progress_calls]
    # Progress should be monotonically non-decreasing
    assert pcts == sorted(pcts)
    # Final call should be 99 %
    assert progress_calls[-1][0] == 99


def test_generator_raises_on_nonzero_exit(tmp_path, monkeypatch):
    """A non-zero exit code must raise RuntimeError (no longer a timeout error)."""
    _make_generator(tmp_path)

    fake_proc = _make_fake_popen(["Something went wrong"], returncode=1)
    monkeypatch.setattr(og, "find_generated_settings", lambda *_a, **_kw: tmp_path)

    with patch.object(og.subprocess, "Popen", return_value=fake_proc):
        with pytest.raises(RuntimeError, match="exit code 1"):
            og.run_official_generator(tmp_path, 1940340)

