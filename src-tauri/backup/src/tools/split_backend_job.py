#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Auto split/refactor backend job.rs for 0xoLemon Launcher.

What it does safely:
- Backup src-tauri/src/job.rs
- Create src-tauri/src/job/paths.rs
- Create src-tauri/src/job/session_cleanup.rs
- Move small path/session helper functions out of job.rs
- Fix downloading root so common/<Game> maps to downloading/<Game>

The script is intentionally conservative: if a function is not found it skips it,
and keeps a backup so you can restore instantly.
"""
from __future__ import annotations

import argparse
import datetime as _dt
import re
import shutil
from pathlib import Path

HELPER_FUNCTIONS_PATHS = [
    "staged_chunk_dir",
    "staged_chunk_path_from",
]

HELPER_FUNCTIONS_CLEANUP = [
    "cleanup_committed_download_session",
    "cleanup_empty_owned_download_dirs",
]

DOWNLOAD_FN_NAME = "downloading_dir_for_install"


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig")


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def find_fn_block(text: str, name: str) -> tuple[int, int] | None:
    # Match normal Rust free functions. Signature may span multiple lines.
    pat = re.compile(rf"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+{re.escape(name)}\s*\(")
    m = pat.search(text)
    if not m:
        return None
    start = m.start()
    brace = text.find("{", m.end())
    if brace == -1:
        return None

    depth = 0
    i = brace
    n = len(text)
    state = "code"
    raw_hashes = 0
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""

        if state == "code":
            if ch == '/' and nxt == '/':
                state = "line_comment"; i += 2; continue
            if ch == '/' and nxt == '*':
                state = "block_comment"; i += 2; continue
            if ch == 'r' and nxt == '"':
                state = "raw_string"; raw_hashes = 0; i += 2; continue
            if ch == 'r' and nxt == '#':
                j = i + 1
                hashes = 0
                while j < n and text[j] == '#':
                    hashes += 1; j += 1
                if j < n and text[j] == '"':
                    state = "raw_string"; raw_hashes = hashes; i = j + 1; continue
            if ch == '"':
                state = "string"; i += 1; continue
            if ch == "'":
                state = "char"; i += 1; continue
            if ch == '{':
                depth += 1
            elif ch == '}':
                depth -= 1
                if depth == 0:
                    end = i + 1
                    while end < n and text[end] in " \t\r\n":
                        end += 1
                    return start, end
            i += 1
            continue

        if state == "line_comment":
            if ch == '\n': state = "code"
            i += 1; continue

        if state == "block_comment":
            if ch == '*' and nxt == '/':
                state = "code"; i += 2; continue
            i += 1; continue

        if state == "string":
            if ch == '\\':
                i += 2; continue
            if ch == '"': state = "code"
            i += 1; continue

        if state == "char":
            if ch == '\\':
                i += 2; continue
            if ch == "'": state = "code"
            i += 1; continue

        if state == "raw_string":
            if ch == '"' and text.startswith('#' * raw_hashes, i + 1):
                state = "code"
                i += 1 + raw_hashes
                continue
            i += 1; continue

    return None


def remove_fn(text: str, name: str) -> tuple[str, str | None]:
    block = find_fn_block(text, name)
    if not block:
        return text, None
    start, end = block
    fn_text = text[start:end].strip() + "\n"
    # Leave exactly one blank line at the removal point.
    new_text = text[:start].rstrip() + "\n\n" + text[end:].lstrip()
    return new_text, fn_text


def make_pub_super(fn_text: str) -> str:
    # Functions moved into child modules are used by the parent job.rs.
    return re.sub(r"(?m)^(\s*)(?:pub\s+)?fn\s+", r"\1pub(super) fn ", fn_text, count=1)


def insert_module_block(text: str) -> str:
    if "mod paths;" in text and "mod session_cleanup;" in text:
        return text

    block = (
        "\nmod paths;\n"
        "mod session_cleanup;\n\n"
        "pub use paths::downloading_dir_for_install;\n"
        "use paths::{staged_chunk_dir, staged_chunk_path_from};\n"
        "use session_cleanup::{cleanup_committed_download_session, cleanup_empty_owned_download_dirs};\n"
    )

    # Insert after top-level use block if possible.
    matches = list(re.finditer(r"(?m)^use [^;]+;\s*", text))
    if matches:
        pos = matches[-1].end()
        return text[:pos] + block + text[pos:]

    # Fallback: after inner attributes or at top.
    return block + "\n" + text


def fixed_downloading_fn() -> str:
    return r'''pub(super) fn downloading_dir_for_install(install_root: &Path, source: &DepotSource) -> PathBuf {
    let game_folder = install_root
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "unknown-game".into());

    if let Some(common_dir) = install_root.parent() {
        let is_common_dir = common_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("common"))
            .unwrap_or(false);

        if is_common_dir {
            if let Some(store_root) = common_dir.parent() {
                return store_root.join("downloading").join(game_folder);
            }
        }
    }

    if source.is_default_source() {
        source.default_downloading_game_dir()
    } else {
        install_root.join(INSTALL_MARKER_DIR).join("downloading")
    }
}
'''


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--project-root", required=True)
    args = ap.parse_args()

    root = Path(args.project_root).resolve()
    src = root / "src-tauri" / "src"
    job_rs = src / "job.rs"
    job_dir = src / "job"

    if not job_rs.exists():
        raise SystemExit(f"Không tìm thấy {job_rs}")

    ts = _dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    backup = job_rs.with_name(f"job.rs.bak_split_{ts}")
    shutil.copy2(job_rs, backup)

    text = read_text(job_rs)

    # Remove the old buggy download-dir function; we will provide fixed version in job/paths.rs.
    text, old_download_fn = remove_fn(text, DOWNLOAD_FN_NAME)

    extracted_paths: list[str] = []
    for name in HELPER_FUNCTIONS_PATHS:
        text, fn_text = remove_fn(text, name)
        if fn_text:
            extracted_paths.append(make_pub_super(fn_text))

    extracted_cleanup: list[str] = []
    for name in HELPER_FUNCTIONS_CLEANUP:
        text, fn_text = remove_fn(text, name)
        if fn_text:
            extracted_cleanup.append(make_pub_super(fn_text))

    text = insert_module_block(text)
    write_text(job_rs, text)

    # Compose new child modules. `use super::*` intentionally gives access to private parent types.
    paths_content = """// Auto-generated by tools/split_backend_job.py\n// Path/staging helpers for job.rs.\n\nuse super::*;\nuse std::path::{Path, PathBuf};\n\n"""
    if extracted_paths:
        paths_content += "\n".join(extracted_paths).strip() + "\n\n"
    else:
        # Fallback definitions if old functions were already moved/missing.
        paths_content += """pub(super) fn staged_chunk_dir(downloading_root: &Path) -> PathBuf {\n    downloading_root.join(\"chunks\")\n}\n\npub(super) fn staged_chunk_path_from(staged_chunks_root: &Path, hash: &str) -> PathBuf {\n    staged_chunks_root.join(hash)\n}\n\n"""
    paths_content += fixed_downloading_fn()
    write_text(job_dir / "paths.rs", paths_content)

    cleanup_content = """// Auto-generated by tools/split_backend_job.py\n// Download-session cleanup helpers for job.rs.\n\nuse super::*;\n"""
    if extracted_cleanup:
        cleanup_content += "\n".join(extracted_cleanup).strip() + "\n"
    else:
        cleanup_content += "// No cleanup functions were extracted; your job.rs may already be split.\n"
    write_text(job_dir / "session_cleanup.rs", cleanup_content)

    # Also create a small note file inside source tree for maintainers.
    note = f"""# job.rs split note\n\nCreated by `tools/split_backend_job.py` at {ts}.\n\nBackup: `{backup.name}`\n\nNew files:\n- `job/paths.rs` — staged/chunk/download path helpers.\n- `job/session_cleanup.rs` — cleanup helpers for completed session markers and empty transient directories.\n\nImportant behavior fixed:\n- `E:\\0xoLemon store\\common\\<Game>` now stages to `E:\\0xoLemon store\\downloading\\<Game>`.\n- It no longer defaults to `common\\<Game>\\.0xolemon\\downloading` for normal store installs.\n"""
    write_text(job_dir / "README_SPLIT.md", note)

    print("OK: đã tách job.rs thành module nhỏ hơn.")
    print(f"Backup: {backup}")
    print(f"Created: {job_dir / 'paths.rs'}")
    print(f"Created: {job_dir / 'session_cleanup.rs'}")
    print("Next: cargo check --release")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
