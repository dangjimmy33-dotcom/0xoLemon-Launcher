import pytest
from gse_autosetup.core.steamstub import parse_steamstub_release


def test_parse_steamstub_release_requires_sha256():
    data = {
        "tag_name": "v1",
        "assets": [{
            "name": "steamstub.zip",
            "browser_download_url": "https://example/steamstub.zip",
            "digest": "sha256:" + "a" * 64,
            "size": 12,
        }],
    }
    release = parse_steamstub_release(data)
    assert release.tag == "v1"
    assert release.sha256 == "a" * 64


def test_parse_steamstub_release_rejects_unverifiable_asset():
    with pytest.raises(RuntimeError):
        parse_steamstub_release({"assets": [{"name": "steamstub.zip", "browser_download_url": "x"}]})

from pathlib import Path
from gse_autosetup.core.resources import ResourceManager
from gse_autosetup.core.steamstub import SteamStubManager


def test_steamstub_uses_embedded_component_without_localappdata(tmp_path: Path):
    resources = tmp_path / "runtime" / "resources"
    root = resources / "embedded" / "rune_steamstub"
    root.mkdir(parents=True)
    (root / "steamstub_x64.dll").write_bytes(b"x64")
    (root / "steamstub_x32.dll").write_bytes(b"x86")
    rm = ResourceManager(app_dir=tmp_path / "portable", runtime_resources=resources)
    mgr = SteamStubManager(resources=rm)
    resolved, tag = mgr.ensure_component(check_updates=False)
    assert resolved == root
    assert tag == "embedded"
