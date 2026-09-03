from pathlib import Path
from types import SimpleNamespace

import pytest

from gse_autosetup.core import package


def test_7z_archive_uses_native_bundled_extractor(tmp_path, monkeypatch):
    archive = tmp_path / "emu-win-release.7z"
    archive.write_bytes(b"7z")
    out = tmp_path / "out"
    tool = tmp_path / "7za.exe"
    tool.write_bytes(b"MZ")

    calls = []

    monkeypatch.setattr(package, "find_native_7zip", lambda: tool)

    def fake_run(args, **kwargs):
        calls.append((args, kwargs))
        return SimpleNamespace(returncode=0, stdout="Everything is Ok", stderr="")

    monkeypatch.setattr(package.subprocess, "run", fake_run)

    package.extract_archive(archive, out)

    assert calls
    args, kwargs = calls[0]
    assert args[0] == str(tool)
    assert args[1] == "x"
    assert "-y" in args
    assert "-aoa" in args
    assert f"-o{out}" in args
    assert str(archive) in args
    assert kwargs["timeout"] >= 60


def test_native_extractor_failure_includes_safe_error(tmp_path, monkeypatch):
    archive = tmp_path / "bad.7z"
    archive.write_bytes(b"7z")
    tool = tmp_path / "7za.exe"
    tool.write_bytes(b"MZ")
    monkeypatch.setattr(package, "find_native_7zip", lambda: tool)
    monkeypatch.setattr(
        package.subprocess,
        "run",
        lambda *a, **k: SimpleNamespace(returncode=2, stdout="", stderr="Data Error"),
    )

    with pytest.raises(RuntimeError, match="7-Zip extraction failed"):
        package.extract_archive(archive, tmp_path / "out")


def test_bundled_gse_seed_is_a_valid_runtime_package():
    root = package.bundled_gse_root()
    package.verify_package_root(root)
    assert (root / "regular" / "x64" / "steam_api64.dll").is_file()
    assert (root / "regular" / "x86" / "steam_api.dll").is_file()
    assert (root / "tools" / "generate_interfaces" / "generate_interfaces_x64.exe").is_file()
    assert (root / "tools" / "generate_interfaces" / "generate_interfaces_x86.exe").is_file()
