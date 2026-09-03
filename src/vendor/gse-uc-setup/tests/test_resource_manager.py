from pathlib import Path

from gse_autosetup.core.cache import CacheManager
from gse_autosetup.core.resources import ResourceManager


def test_resource_manager_prefers_portable_update_over_embedded(tmp_path: Path):
    embedded = tmp_path / "embedded"
    portable = tmp_path / "portable"
    (embedded / "gse").mkdir(parents=True)
    (embedded / "gse" / "marker.txt").write_text("embedded", encoding="utf-8")
    (portable / "resources" / "updates" / "gse").mkdir(parents=True)
    (portable / "resources" / "updates" / "gse" / "marker.txt").write_text("update", encoding="utf-8")

    rm = ResourceManager(app_dir=portable, runtime_resources=embedded.parent)
    root = rm.component_root("gse")
    assert (root / "marker.txt").read_text(encoding="utf-8") == "update"


def test_resource_manager_falls_back_to_embedded(tmp_path: Path):
    runtime = tmp_path / "runtime"
    embedded = runtime / "resources" / "embedded" / "steamless"
    embedded.mkdir(parents=True)
    (embedded / "Steamless.CLI.exe").write_bytes(b"cli")

    rm = ResourceManager(app_dir=tmp_path / "portable", runtime_resources=runtime / "resources")
    assert rm.component_root("steamless") == embedded


def test_resource_manager_temp_dir_is_portable_and_cleaned(tmp_path: Path):
    rm = ResourceManager(app_dir=tmp_path / "portable", runtime_resources=tmp_path / "runtime")
    with rm.temp_dir("uc_online") as temp:
        assert temp.is_dir()
        assert str(temp).startswith(str(tmp_path / "portable" / "resources" / "updates"))
        (temp / "x").write_text("x", encoding="utf-8")
    assert not temp.exists()


def test_cache_manager_is_metadata_only_and_does_not_create_package_cache(tmp_path: Path):
    cm = CacheManager(root=tmp_path / "meta")
    assert cm.state_path.parent == tmp_path / "meta"
    assert not (tmp_path / "meta" / "downloads").exists()
    assert not (tmp_path / "meta" / "packages").exists()


def test_resource_manager_prefers_external_embedded_beside_exe(tmp_path: Path):
    app_dir = tmp_path / "portable"
    external = app_dir / "resources" / "embedded" / "gse_tools"
    internal = tmp_path / "runtime" / "embedded" / "gse_tools"
    external.mkdir(parents=True)
    internal.mkdir(parents=True)
    (external / "marker.txt").write_text("external", encoding="utf-8")
    (internal / "marker.txt").write_text("internal", encoding="utf-8")

    rm = ResourceManager(app_dir=app_dir, runtime_resources=tmp_path / "runtime")
    assert rm.component_root("gse_tools") == external


def test_component_candidates_include_update_external_and_internal_without_duplicates(tmp_path: Path):
    app_dir = tmp_path / "portable"
    runtime = tmp_path / "runtime"
    rm = ResourceManager(app_dir=app_dir, runtime_resources=runtime)
    expected = [
        app_dir / "resources" / "updates" / "gse_tools",
        app_dir / "resources" / "embedded" / "gse_tools",
        runtime / "embedded" / "gse_tools",
    ]
    assert rm.component_candidates("gse_tools") == expected
