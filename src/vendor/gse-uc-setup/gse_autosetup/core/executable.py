from __future__ import annotations

from pathlib import Path

_SKIP_TOKENS = (
    "unitycrashhandler", "crashreportclient", "ueprereq", "unins", "uninstall",
    "setup", "installer", "vc_redist", "dxsetup", "launcher", "reporter",
)


def find_main_executable(game_root: Path) -> Path:
    root = Path(game_root).resolve()
    if not root.is_dir():
        raise ValueError("Game folder does not exist.")

    candidates: list[tuple[int, int, Path]] = []
    for exe in root.rglob("*.exe"):
        try:
            rel = exe.relative_to(root)
        except ValueError:
            continue
        low = str(rel).lower()
        if any(part in low for part in ("_commonredist", "redist", "redistributable", "backup")):
            continue
        name = exe.name.lower()
        if any(token in name for token in _SKIP_TOKENS):
            continue
        try:
            size = exe.stat().st_size
        except OSError:
            size = 0
        depth_score = max(0, 8 - len(rel.parts)) * 1_000_000_000
        candidates.append((depth_score + min(size, 999_999_999), size, exe))

    if not candidates:
        raise RuntimeError("Could not locate a likely main game executable for the loader/SteamStub stage.")
    candidates.sort(key=lambda x: (x[0], x[1], str(x[2]).lower()), reverse=True)
    return candidates[0][2]
