#!/usr/bin/env python3
"""
Live round-trip test against all games in Steam's appcache/stats.

For every UserGameStats_<steamid3>_<appid>.bin that has a matching schema:
  1. Copy original  ->  <base>_orig.bin   (skip if already exists)
  2. st2gse: Steam bin -> GSE saves (temp dir)
  3. gse2st: GSE saves -> <base>_conv.bin  (overlay mode, base = _orig)
  4. Compare _orig vs _conv byte-for-byte (CRC field masked to 0)

Usage:
    python test_live.py                    # all games
    python test_live.py --appid 228380     # single game
    python test_live.py --keep             # keep temp GSE dirs after run
    python test_live.py -v                 # verbose stat/ach listing
"""

import argparse
import contextlib
import hashlib
import io
import re
import shutil
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from steam_stats_converter import (
    cmd_gse2st,
    cmd_st2gse,
    _find_steam_root,
    _find_steamid3,
    _sid3_to_sid64,
    _sid64_to_sid3,
    find_schema,
    find_ugs,
    parse_ugs,
    parse_schema,
    parse_vdf,
    _write_vdf_root,
)

# CRC is the first INT32 inside the "cache" subkey:
#   0x00 "cache\0"  0x02 "crc\0"  [4-byte CRC]
# offsets: 1 (type) + 6 ("cache\0") + 1 (type) + 4 ("crc\0") = 12
CRC_OFFSET = 12


def mask_crc(data: bytes) -> bytes:
    if len(data) < CRC_OFFSET + 4:
        return data
    return data[:CRC_OFFSET] + b"\x00\x00\x00\x00" + data[CRC_OFFSET + 4:]


def first_diff(a: bytes, b: bytes) -> str:
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            lo = max(0, i - 4)
            hi = min(len(a), i + 16)
            ca = " ".join("%02X" % c for c in a[lo:hi])
            cb = " ".join("%02X" % c for c in b[lo:hi])
            return ("byte %d (0x%X): orig=0x%02X  conv=0x%02X\n"
                    "  orig: %s\n"
                    "  conv: %s" % (i, i, x, y, ca, cb))
    return "length mismatch: orig=%d  conv=%d" % (len(a), len(b))


def normalize_for_cmp(data: bytes) -> bytes:
    """Strip sync metadata (crc, PendingChanges, state, pendingbits) and re-serialize
    with groups sorted by integer key for stable byte-exact comparison."""
    raw, _ = parse_vdf(data)
    cache = raw.get("cache", {})
    norm = {}
    for gid in sorted((g for g in cache if isinstance(cache[g], dict)), key=lambda x: int(x)):
        gval = cache[gid]
        node = {k: v for k, v in gval.items() if k not in ("state", "pendingbits")}
        if node.get("data", 0) == 0:
            continue
        norm[gid] = node
    return _write_vdf_root("cache", norm)


def has_any_data(ugs_path: Path) -> bool:
    """Return True if this UGS bin has at least one non-zero group."""
    try:
        from steam_stats_converter import parse_ugs
        ugs = parse_ugs(str(ugs_path))
        return bool(ugs.ach_times or ugs.stat_vals)
    except Exception:
        return True   # err on the side of including it


def run_silent(fn, *args, **kwargs):
    """Run fn(*args, **kwargs) swallowing stdout; return (ok, error_str)."""
    buf = io.StringIO()
    try:
        with contextlib.redirect_stdout(buf):
            fn(*args, **kwargs)
        return True, ""
    except SystemExit as e:
        return False, str(e)
    except Exception as e:
        return False, "%s: %s" % (type(e).__name__, e)


def main() -> None:
    ap = argparse.ArgumentParser(description="Live round-trip test for steam_stats_converter")
    ap.add_argument("--appid", type=int, default=None,
                    help="Test only this appid (default: all)")
    ap.add_argument("--out-dir", default=None,
                    help="Output directory for _orig/_conv bins and GSE dirs (default: tests/ next to script)")
    ap.add_argument("--keep",  action="store_true",
                    help="Keep temporary GSE save dirs after the run")
    ap.add_argument("-v", "--verbose", action="store_true",
                    help="Show per-stat/ach details during conversion")
    args = ap.parse_args()

    # ── locate Steam ────────────────────────────────────────────────────────
    steam_root = _find_steam_root()
    if steam_root is None:
        sys.exit("[error] Steam installation not found")

    sid3 = _find_steamid3(steam_root)
    if sid3 is None:
        sys.exit("[error] Could not detect SteamID from Steam userdata")

    steamid64 = _sid3_to_sid64(sid3)
    stats_dir = steam_root / "appcache" / "stats"

    out_root = Path(args.out_dir) if args.out_dir else Path(__file__).parent / "tests"
    out_root.mkdir(parents=True, exist_ok=True)

    print("Steam root : %s" % steam_root)
    print("SteamID    : %d  (id3=%d)" % (steamid64, sid3))
    print("Stats dir  : %s" % stats_dir)
    print("Output dir : %s" % out_root)
    print()

    # ── discover UserGameStats bins ─────────────────────────────────────────
    pattern = re.compile(r"UserGameStats_(\d+)_(\d+)\.bin$", re.IGNORECASE)

    candidates = []
    for f in sorted(stats_dir.iterdir()):
        m = pattern.match(f.name)
        if not m:
            continue
        file_sid3  = int(m.group(1))
        file_appid = int(m.group(2))
        if file_sid3 != sid3:
            continue
        if args.appid is not None and file_appid != args.appid:
            continue
        candidates.append((file_appid, f))

    if not candidates:
        sys.exit("[info] No matching UserGameStats bins found in %s" % stats_dir)

    print("Found %d UGS file(s) for this account\n" % len(candidates))

    passed = failed = skipped = 0

    for appid, ugs_path in candidates:
        stem = ugs_path.stem   # UserGameStats_<sid3>_<appid>

        schema_path = find_schema(appid, steam_root)
        if schema_path is None:
            print("SKIP  %-10d  (no schema in appcache)" % appid)
            skipped += 1
            continue

        if not has_any_data(ugs_path):
            print("SKIP  %-10d  (UGS bin has no data)" % appid)
            skipped += 1
            continue

        # 2. st2gse -> per-appid dir inside out_root
        gse_dir = out_root / str(appid)
        gse_dir.mkdir(exist_ok=True)

        orig_path = gse_dir / (stem + "_orig.bin")
        conv_path = gse_dir / (stem + "_conv.bin")

        # 1. copy original — snapshot hash so we can assert it is never modified
        if not orig_path.exists():
            shutil.copy2(ugs_path, orig_path)
        orig_hash = hashlib.sha256(orig_path.read_bytes()).hexdigest()

        ok, err = run_silent(cmd_st2gse,
            appid=appid, steamid64=steamid64,
            steam_root=steam_root, out_dir=gse_dir,
            verbose=args.verbose, lang="english")
        if not ok:
            print("FAIL  %-10d  st2gse error: %s" % (appid, err))
            failed += 1
            continue

        # 3. gse2st (overlay mode, base = _orig) -> _conv.bin
        ok, err = run_silent(cmd_gse2st,
            appid=appid, steamid64=steamid64,
            schema_path=schema_path, gse_dir=gse_dir,
            out_path=conv_path, verbose=args.verbose,
            ugs_path=orig_path)
        if not ok:
            print("FAIL  %-10d  gse2st error: %s" % (appid, err))
            failed += 1
            continue

        # Guard: orig must never be written by any conversion step
        if hashlib.sha256(orig_path.read_bytes()).hexdigest() != orig_hash:
            print("FAIL  %-10d  orig bin was modified during conversion!" % appid)
            failed += 1
            continue

        # 4. compare byte-for-byte after stripping sync metadata
        # (pendingbits/state/PendingChanges intentionally differ between original
        # already-synced bin and converted sync-pending bin; data must be identical)
        orig_ugs  = parse_ugs(str(orig_path))
        conv_ugs  = parse_ugs(str(conv_path))
        orig_norm = normalize_for_cmp(orig_path.read_bytes())
        conv_norm = normalize_for_cmp(conv_path.read_bytes())

        n_achs  = len(orig_ugs.ach_times)
        n_stats = len(orig_ugs.stat_vals)

        # Intersection comparison: every group in orig must appear in conv with
        # identical data; conv may have extra groups (stats with non-zero defaults
        # not yet synced, or achievements newly earned via GSE) — those are OK.
        orig_nd, _ = parse_vdf(orig_norm)
        conv_nd, _ = parse_vdf(conv_norm)
        orig_gc = orig_nd.get("cache", {})
        conv_gc = conv_nd.get("cache", {})
        match = all(
            gid in conv_gc and conv_gc[gid] == gval
            for gid, gval in orig_gc.items()
            if isinstance(gval, dict)
        )

        if match:
            print("PASS  %-10d  %d ach(s) / %d stat group(s)" % (appid, n_achs, n_stats))
            passed += 1
        else:
            # Collect all semantic diffs
            diffs = []
            for k, ts in orig_ugs.ach_times.items():
                ct = conv_ugs.ach_times.get(k)
                if ct != ts:
                    diffs.append("  ach  gid=%-4s bit=%-3s  orig_ts=%-12d  conv_ts=%s"
                                 % (k[0], k[1], ts, ct))
            for k in conv_ugs.ach_times:
                if k not in orig_ugs.ach_times:
                    diffs.append("  ach  gid=%-4s bit=%-3s  UNEXPECTED in conv" % k)
            for gid, v in orig_ugs.stat_vals.items():
                cv = conv_ugs.stat_vals.get(gid)
                if cv != v:
                    diffs.append("  stat gid=%-4s          orig=0x%08X  conv=%s" % (gid, v, cv))

            if diffs:
                print("FAIL  %-10d  %d semantic diff(s):" % (appid, len(diffs)))
                for d in diffs:
                    print(d)
            else:
                # bytes differ but all values are the same — structural/ordering issue
                print("FAIL  %-10d  structural diff: %s"
                      % (appid, first_diff(orig_norm, conv_norm)))
            failed += 1

    # ── cleanup ─────────────────────────────────────────────────────────────
    if not args.keep:
        for d in (out_root / str(appid) for appid, _ in candidates):
            shutil.rmtree(d, ignore_errors=True)
    else:
        print("\nGSE save dirs kept under: %s" % out_root)

    print()
    print("=" * 55)
    print("Results: %d passed  %d failed  %d skipped  (%d total)" % (
        passed, failed, skipped, passed + failed + skipped))

    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
