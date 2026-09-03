from pathlib import Path
from gse_autosetup.core.scanner import detect_pe_architecture, scan_steam_api_targets

def _write_pe(path: Path, machine: int):
    data = bytearray(512)
    data[0:2] = b"MZ"
    data[0x3C:0x40] = (0x80).to_bytes(4, "little")
    data[0x80:0x84] = b"PE\0\0"
    data[0x84:0x86] = machine.to_bytes(2, "little")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)

def test_detects_x86_and_x64(tmp_path):
    x86 = tmp_path / "steam_api.dll"
    x64 = tmp_path / "steam_api64.dll"
    _write_pe(x86, 0x14C)
    _write_pe(x64, 0x8664)
    assert detect_pe_architecture(x86) == "x86"
    assert detect_pe_architecture(x64) == "x64"

def test_scan_excludes_common_redist(tmp_path):
    good = tmp_path / "bin" / "steam_api64.dll"
    bad = tmp_path / "_CommonRedist" / "steam_api64.dll"
    _write_pe(good, 0x8664)
    _write_pe(bad, 0x8664)
    targets = scan_steam_api_targets(tmp_path)
    assert [t.path for t in targets] == [good]
