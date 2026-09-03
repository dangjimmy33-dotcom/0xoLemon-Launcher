from pathlib import Path

from gse_autosetup.core.migrate import MigrateGSEManager
from gse_autosetup.core.resources import ResourceManager


def test_migrate_manager_resolves_embedded_exe(tmp_path: Path):
    resources = tmp_path / "runtime" / "resources"
    exe = resources / "embedded" / "migrate_gse" / "migrate_gse.exe"
    exe.parent.mkdir(parents=True)
    exe.write_bytes(b"exe")
    rm = ResourceManager(app_dir=tmp_path / "portable", runtime_resources=resources)
    mgr = MigrateGSEManager(resources=rm)
    assert mgr.executable() == exe
