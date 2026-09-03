from __future__ import annotations

import os
import shutil
import subprocess
import queue
import threading
import time
from pathlib import Path

from .package import extract_archive

GSE_TOOLS_REPO = "alex47exe/gse_fork_tools"


def find_generator_executable(root: Path) -> Path:
    root = Path(root)
    preferred = [
        root / "generate_emu_config.exe",
        root / "generate_emu_config_old" / "generate_emu_config.exe",
    ]
    for path in preferred:
        if path.is_file():
            return path
    matches = sorted(root.rglob("generate_emu_config.exe"), key=lambda p: (len(p.parts), str(p).lower()))
    if not matches:
        raise RuntimeError("Official gse_fork_tools package does not contain generate_emu_config.exe.")
    return matches[0]


def find_generated_settings(generator_dir: Path, appid: int) -> Path:
    generator_dir = Path(generator_dir)
    direct = generator_dir / "_OUTPUT" / str(int(appid)) / "steam_settings"
    if direct.is_dir():
        return direct
    candidates = [p for p in generator_dir.rglob("steam_settings") if p.is_dir() and str(int(appid)) in p.parts]
    if not candidates:
        candidates = [p for p in generator_dir.rglob("steam_settings") if p.is_dir()]
    if not candidates:
        raise RuntimeError("Official generator completed but no steam_settings output was found.")
    candidates.sort(key=lambda p: (len(p.parts), str(p).lower()))
    return candidates[0]


def extract_tools_package(archive: Path, destination: Path) -> Path:
    destination = Path(destination)
    marker = destination / ".tools-complete"
    if marker.is_file():
        try:
            return find_generator_executable(destination).parent
        except Exception:
            shutil.rmtree(destination, ignore_errors=True)
    destination.mkdir(parents=True, exist_ok=True)
    extract_archive(archive, destination)
    exe = find_generator_executable(destination)
    marker.write_text("ok", encoding="ascii")
    return exe.parent


def build_generator_command(
    exe: Path,
    appid: int,
    *,
    skip_achievements: bool = False,
) -> list[str]:
    """Build the maintainer-compatible anonymous command.

    ``-def1`` keeps the official complete GSE preset.  The normal Setup path
    keeps ``skip_achievements=False`` because official generator output is the
    canonical source of achievement localization/artwork.  ``-skip_ach`` remains
    available only for explicit limited/fallback callers.
    """
    cmd = [str(exe), "-def1", "-clr", "-anon"]
    if skip_achievements:
        cmd.append("-skip_ach")
    cmd.append(str(int(appid)))
    return cmd


def clean_subprocess_env(extra: dict[str, str] | None = None) -> dict[str, str]:
    """Build an isolated environment for a nested PyInstaller generator."""
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("_PYI_") or key in {
            "_MEIPASS",
            "_MEIPASS2",
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONEXECUTABLE",
            "PYINSTALLER_STRICT_UNPACK_MODE",
        }:
            env.pop(key, None)
    env["PYINSTALLER_RESET_ENVIRONMENT"] = "1"
    env["PYTHONUNBUFFERED"] = "1"
    if extra:
        env.update(extra)
    return env


def _terminate_process(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    if os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            return
        except Exception:
            pass
    try:
        proc.kill()
    except Exception:
        pass


def run_official_generator(
    generator_root: Path,
    appid: int,
    log=None,
    progress=None,
    timeout: int = 600,
    idle_timeout: int = 120,
    *,
    skip_achievements: bool = False,
) -> Path:
    """Run ``generate_emu_config`` without allowing the UI to hang forever.

    stdout is consumed by a reader thread so the main loop can emit heartbeats
    and enforce both an overall timeout and an output-idle timeout. Stdin is
    closed to prevent an unexpected prompt from blocking a windowed build.
    """
    log = log or (lambda _m: None)
    progress = progress or (lambda _pct, _msg: None)

    exe = find_generator_executable(generator_root)
    work_dir = exe.parent
    cmd = build_generator_command(exe, appid, skip_achievements=skip_achievements)
    mode = "fast complete config; achievements via Steam Web API" if skip_achievements else "full official schema"
    log(f"Running official GSE config generator ({mode})...")
    creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0) if os.name == "nt" else 0

    milestones: list[tuple[str, int]] = [
        ("connecting", 2), ("authenticating", 4), ("getting app info", 6),
        ("app info", 8), ("achievement", 20), ("stat", 35), ("dlc", 45),
        ("depot", 55), ("branch", 62), ("controller", 70), ("inventory", 76),
        ("language", 82), ("tag", 86), ("writing", 91), ("generating", 91),
        ("done", 97), ("finish", 97), ("complete", 97),
    ]

    proc = subprocess.Popen(
        cmd,
        cwd=work_dir,
        env=clean_subprocess_env(),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
        creationflags=creationflags,
        bufsize=1,
    )
    assert proc.stdout is not None

    lines: queue.Queue[str | None] = queue.Queue()

    def reader() -> None:
        try:
            for raw in proc.stdout:
                lines.put(raw)
        finally:
            lines.put(None)

    threading.Thread(target=reader, name="gse-generator-output", daemon=True).start()

    started = time.monotonic()
    last_output = started
    last_heartbeat = started
    current_pct = 0
    reader_done = False

    try:
        while True:
            now = time.monotonic()
            elapsed = int(now - started)
            if timeout and now - started > timeout:
                raise TimeoutError(f"Official generator exceeded {timeout}s total runtime.")
            if idle_timeout and now - last_output > idle_timeout:
                raise TimeoutError(f"Official generator produced no output for {idle_timeout}s.")

            try:
                raw = lines.get(timeout=0.5)
            except queue.Empty:
                raw = ""

            if raw is None:
                reader_done = True
            elif raw:
                line = raw.rstrip()
                if line:
                    last_output = time.monotonic()
                    log("generator: " + line)
                    low = line.lower()
                    # Some distributed PyInstaller builds of generate_emu_config
                    # are missing PyCryptodome native hash modules. Do not sit on
                    # the 120 s idle timeout after the child has already printed
                    # its fatal loader error; fail immediately so Setup can retry
                    # with -skip_ach and still obtain depots/branches/controllers.
                    if (
                        "cannot load native module 'cryptodome.hash." in low
                        or ("failed to execute script" in low and "generate_emu_config" in low)
                    ):
                        raise RuntimeError(
                            "Official generator runtime is missing a PyCryptodome native module: " + line
                        )
                    milestone = next((pct for kw, pct in milestones if kw in low), None)
                    if milestone is not None:
                        current_pct = max(current_pct, milestone)
                    progress(current_pct, f"Official generator… {elapsed}s — {line[:60]}")

            now = time.monotonic()
            if now - last_heartbeat >= 10 and proc.poll() is None:
                last_heartbeat = now
                time_pct = min(90, max(current_pct, int((now - started) / 6)))
                current_pct = time_pct
                progress(current_pct, f"Official generator still working… {int(now-started)}s")
                log(f"Official generator still working… {int(now-started)}s")

            if proc.poll() is not None and reader_done:
                break
    except Exception:
        _terminate_process(proc)
        proc.wait(timeout=10)
        raise

    elapsed = int(time.monotonic() - started)
    if proc.returncode != 0:
        raise RuntimeError(f"Official config generator failed with exit code {proc.returncode}.")

    progress(99, f"Generator finished in {elapsed}s — locating output…")
    log(f"Official generator completed in {elapsed}s.")
    return find_generated_settings(work_dir, appid)


_DOC_NAMES = {"readme", "changelog", "credits", "license", "copying"}


def _is_deployable_settings_file(path: Path) -> bool:
    low = path.name.lower()
    stem = path.stem.lower()
    if path.suffix.lower() in {".md", ".markdown"}:
        return False
    if any(stem.startswith(prefix) for prefix in _DOC_NAMES) or "license" in stem:
        return False
    return True


def merge_settings_tree(source: Path, destination: Path) -> None:
    """Merge generated runtime settings while excluding documentation-only files."""
    source = Path(source)
    destination = Path(destination)
    if not source.is_dir():
        raise ValueError(f"Generated steam_settings folder does not exist: {source}")
    destination.mkdir(parents=True, exist_ok=True)
    for path in source.rglob("*"):
        if path.is_dir():
            continue
        rel = path.relative_to(source)
        if not _is_deployable_settings_file(path):
            continue
        target = destination / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)


def _deployable_relative_files(root: Path) -> set[Path]:
    root = Path(root)
    result: set[Path] = set()
    if not root.is_dir():
        return result
    for path in root.rglob("*"):
        if path.is_file() and _is_deployable_settings_file(path):
            result.add(path.relative_to(root))
    return result


def validate_settings_mirror(source: Path, destination: Path) -> None:
    """Ensure every runtime file produced by gse_fork_tools reached the game.

    Documentation files are intentionally excluded, but generated runtime data
    such as branches.json, depots.txt, controllers, images, inventory files and
    supported_languages.txt must never disappear during deployment.
    """
    source = Path(source)
    destination = Path(destination)
    expected = _deployable_relative_files(source)
    missing = sorted(rel for rel in expected if not (destination / rel).is_file())
    if missing:
        preview = ", ".join(str(p) for p in missing[:12])
        more = "" if len(missing) <= 12 else f" (+{len(missing) - 12} more)"
        raise RuntimeError(f"GSE generated settings mirror is incomplete: {preview}{more}")


def cleanup_official_generator_output(tools_root: Path, appid: int | None = None) -> None:
    """Clean up generated _OUTPUT directory or specific AppID output in generator tools folder."""
    try:
        tools_root = Path(tools_root)
        output_dirs: list[Path] = []
        direct = tools_root / "_OUTPUT"
        if direct.is_dir():
            output_dirs.append(direct)
        for cand in tools_root.rglob("_OUTPUT"):
            if cand.is_dir() and cand not in output_dirs:
                output_dirs.append(cand)

        for out_dir in output_dirs:
            if appid is not None:
                target = out_dir / str(int(appid))
                if target.is_dir():
                    shutil.rmtree(target, ignore_errors=True)
                # If _OUTPUT is now empty, remove _OUTPUT as well
                try:
                    if not any(out_dir.iterdir()):
                        shutil.rmtree(out_dir, ignore_errors=True)
                except Exception:
                    pass
            else:
                shutil.rmtree(out_dir, ignore_errors=True)
    except Exception:
        pass
