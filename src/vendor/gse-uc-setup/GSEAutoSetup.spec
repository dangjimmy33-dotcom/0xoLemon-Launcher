# -*- mode: python ; coding: utf-8 -*-
from pathlib import Path

root = Path(SPECPATH)
icon = root / "assets" / "icon.ico"
resources = root / "resources"

resource_datas = []
# V1.8.3: the complete hybrid baseline is embedded in the one-file EXE.
# Runtime portable updates are deliberately NOT bundled; they live beside the EXE.
for folder, destination in (
    (resources / "embedded", "resources/embedded"),
    (resources / "7zip", "resources/7zip"),
    # Legacy resources retained only for backward compatibility with older restore paths.
    (resources / "gse_seed", "resources/gse_seed"),
    (resources / "preserve_seed", "resources/preserve_seed"),
    (resources / "google", "resources/google"),
):
    if folder.exists():
        resource_datas.append((str(folder), destination))


a = Analysis(
    [str(root / "app.py")],
    pathex=[str(root)],
    binaries=[],
    datas=[(str(icon), "assets"), *resource_datas],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["py7zr"],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="GSEAutoSetup",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=str(icon),
)
