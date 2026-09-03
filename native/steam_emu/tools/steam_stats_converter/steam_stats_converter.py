#!/usr/bin/env python3
"""
Steam Stats Converter  -  bidirectional Steam <-> GSE achievements/stats converter

WHAT IT READS
  UserGameStatsSchema_<appid>.bin  - achievement & stat definitions (per-game, shared)
  UserGameStats_<steamid3>_<appid>.bin  - per-user earned data (one file holds
      achievements, stat-linked progress counters AND standalone stats)

USAGE
  # Show schema + your current progress for a game:
  python steam_stats_converter.py --appid 1234 --info

  # Convert YOUR Steam data  ->  GSE save files  (auto-detects all paths):
  python steam_stats_converter.py --appid 1234 --st2gse

  # Convert GSE save files  ->  Steam binary  (auto-detects all paths):
  python steam_stats_converter.py --appid 1234 --gse2st

  # Explicit paths / non-default output:
  python steam_stats_converter.py --appid 1234 --st2gse --out-dir ./my_save
  python steam_stats_converter.py --appid 1234 --gse2st --gse-dir ./my_save --steamid 76561198000000000

GSE SAVE LAYOUT expected by --gse2st:
  <gse-dir>/achievements.json    {"ACH_NAME": {"earned": true/false, "earned_time": 1234567}}
  <gse-dir>/stats.json           [{"name": "stat", "type": "int|float|avgrate", "value": 42}, ...]
  <gse-dir>/stats/<stat_name>    legacy: raw 4-byte little-endian int32 or float32 (fallback only)
"""

import argparse
import datetime
import json
import os
import struct
import sys
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Binary VDF (KeyValues) parser / writer
# ---------------------------------------------------------------------------
TYPE_SUBKEY = 0x00
TYPE_STRING = 0x01
TYPE_INT32  = 0x02
TYPE_FLOAT  = 0x03
TYPE_UINT64 = 0x07
TYPE_END    = 0x08


def _read_cstr(data: bytes, pos: int) -> tuple[str, int]:
    end = data.index(0, pos)
    return data[pos:end].decode("utf-8", errors="replace"), end + 1


def parse_vdf(data: bytes, pos: int = 0) -> tuple[dict, int]:
    """Parse binary VDF starting at *pos*.  Returns (dict, new_pos)."""
    result: dict[str, Any] = {}
    while pos < len(data):
        t = data[pos]; pos += 1
        if t == TYPE_END:
            return result, pos
        key, pos = _read_cstr(data, pos)
        if t == TYPE_SUBKEY:
            val, pos = parse_vdf(data, pos)
        elif t == TYPE_STRING:
            val, pos = _read_cstr(data, pos)
        elif t == TYPE_INT32:
            val = struct.unpack_from("<I", data, pos)[0]; pos += 4
        elif t == TYPE_FLOAT:
            val = struct.unpack_from("<f", data, pos)[0]; pos += 4
        elif t == TYPE_UINT64:
            val = struct.unpack_from("<Q", data, pos)[0]; pos += 8
        else:
            sys.stderr.write("[warn] Unknown VDF type 0x%02X at offset %d\n" % (t, pos - 1))
            break
        result[key] = val
    return result, pos


def _vdf_type_of(val: Any) -> int:
    if isinstance(val, dict):  return TYPE_SUBKEY
    if isinstance(val, str):   return TYPE_STRING
    if isinstance(val, float): return TYPE_FLOAT
    if isinstance(val, int):   return TYPE_INT32
    return TYPE_INT32


def _write_vdf(d: dict) -> bytes:
    buf = bytearray()
    for key, val in d.items():
        t = _vdf_type_of(val)
        buf.append(t)
        buf += key.encode("utf-8") + b"\x00"
        if t == TYPE_SUBKEY:
            buf += _write_vdf(val)
        elif t == TYPE_STRING:
            buf += val.encode("utf-8") + b"\x00"
        elif t == TYPE_INT32:
            buf += struct.pack("<I", val & 0xFFFFFFFF)
        elif t == TYPE_FLOAT:
            buf += struct.pack("<f", val)
        elif t == TYPE_UINT64:
            buf += struct.pack("<Q", val)
    buf.append(TYPE_END)
    return bytes(buf)


def _write_vdf_root(root_key: str, d: dict) -> bytes:
    buf = bytearray()
    buf.append(TYPE_SUBKEY)
    buf += root_key.encode("utf-8") + b"\x00"
    buf += _write_vdf(d)
    buf.append(TYPE_END)
    return bytes(buf)


# ---------------------------------------------------------------------------
# Rich schema parser
# ---------------------------------------------------------------------------

class AchievementDef:
    __slots__ = ("api", "name", "desc", "hidden", "icon", "icon_gray",
                 "prog_stat_name", "prog_min", "prog_max", "_group", "_bit")

    def __init__(self, api, name, desc, hidden, icon, icon_gray,
                 prog_stat_name, prog_min, prog_max, group, bit):
        self.api            = api
        self.name           = name
        self.desc           = desc
        self.hidden         = hidden
        self.icon           = icon
        self.icon_gray      = icon_gray
        self.prog_stat_name = prog_stat_name   # API name of linked stat (or None)
        self.prog_min       = prog_min
        self.prog_max       = prog_max
        self._group         = group    # str  (VDF group id)
        self._bit           = bit      # str  (bit index within group)


class StatDef:
    __slots__ = ("name", "gtype", "default", "max_val", "_group")

    def __init__(self, name, gtype, default, max_val, group):
        self.name    = name
        self.gtype   = gtype     # "INT" | "FLOAT" | "AVGRATE"
        self.default = default
        self.max_val = max_val
        self._group  = group     # str


def parse_schema(schema_path: str, lang: str = "english") -> tuple[int, list, list]:
    """
    Parse UserGameStatsSchema_<appid>.bin.

    Returns:
        appid         : int
        achievements  : list[AchievementDef]  (ordered as in schema)
        stats         : list[StatDef]         (ordered as in schema)
    """
    data = Path(schema_path).read_bytes()
    raw, _ = parse_vdf(data)

    app_id_str = next(iter(raw))
    app_node   = raw[app_id_str]
    stats_node = app_node.get("stats", {})

    try:
        appid = int(app_id_str)
    except ValueError:
        appid = 0

    achievements: list[AchievementDef] = []
    stats: list[StatDef]               = []

    for gid, gval in stats_node.items():
        if not isinstance(gval, dict):
            continue
        gtype = gval.get("type", "").upper()

        if gtype == "ACHIEVEMENTS":
            bits = gval.get("bits", {})
            for bit, bval in bits.items():
                if not isinstance(bval, dict):
                    continue
                api  = bval.get("name", "ACH_%s_%s" % (gid, bit))
                disp = bval.get("display", {})
                name = desc = icon = icon_gray = ""
                hidden = False
                if isinstance(disp, dict):
                    n = disp.get("name", {})
                    d = disp.get("desc", {})
                    name      = (n.get(lang) or n.get("english", "")) if isinstance(n, dict) else str(n)
                    desc      = (d.get(lang) or d.get("english", "")) if isinstance(d, dict) else str(d)
                    hidden    = bool(disp.get("hidden", 0))
                    icon      = disp.get("icon", "")
                    icon_gray = disp.get("icon_gray", "")

                prog = bval.get("progress", {})
                prog_stat = prog_min = prog_max = None
                if isinstance(prog, dict) and prog:
                    val_node = prog.get("value", {})
                    if isinstance(val_node, dict):
                        prog_stat = val_node.get("operand1")
                    prog_min = prog.get("min_val", 0)
                    prog_max = prog.get("max_val")

                achievements.append(AchievementDef(
                    api=api, name=name, desc=desc, hidden=hidden,
                    icon=icon, icon_gray=icon_gray,
                    prog_stat_name=prog_stat,
                    prog_min=prog_min, prog_max=prog_max,
                    group=gid, bit=bit,
                ))
        else:
            name    = gval.get("name", "stat_%s" % gid)
            default = gval.get("default", 0)
            max_val = gval.get("max", None)
            stats.append(StatDef(name=name, gtype=gtype, default=default,
                                 max_val=max_val, group=gid))

    return appid, achievements, stats


# ---------------------------------------------------------------------------
# UserGameStats parser
# ---------------------------------------------------------------------------

class UGSData:
    """Parsed UserGameStats_<steamid>_<appid>.bin"""

    def __init__(self):
        # (group_str, bit_str) -> unix timestamp
        self.ach_times: dict[tuple[str, str], int] = {}
        # group_str -> raw int value (stat groups only)
        self.stat_vals: dict[str, int] = {}


def parse_ugs(ugs_path: str) -> UGSData:
    data = Path(ugs_path).read_bytes()
    raw, _ = parse_vdf(data)
    cache  = raw.get("cache", {})

    result = UGSData()
    for gid, gval in cache.items():
        if gid in ("crc", "PendingChanges") or not isinstance(gval, dict):
            continue
        raw_data    = gval.get("data", 0)
        ach_section = gval.get("AchievementTimes")

        if ach_section is not None:
            if isinstance(ach_section, dict):
                for bit_str, ts in ach_section.items():
                    result.ach_times[(gid, bit_str)] = int(ts)
        else:
            result.stat_vals[gid] = int(raw_data) if isinstance(raw_data, (int, float)) else 0

    return result


# ---------------------------------------------------------------------------
# Path / steamid helpers
# ---------------------------------------------------------------------------

def _find_steam_root() -> Path | None:
    candidates = [
        Path(r"C:\Program Files (x86)\Steam"),
        Path(r"C:\Program Files\Steam"),
        Path.home() / ".steam" / "steam",
        Path.home() / ".local" / "share" / "Steam",
    ]
    for c in candidates:
        if c.is_dir():
            return c
    return None


def _find_steamid3(steam_root: Path) -> int | None:
    userdata = steam_root / "userdata"
    if not userdata.is_dir():
        return None
    for entry in sorted(userdata.iterdir()):
        if entry.is_dir() and entry.name.isdigit() and entry.name != "0":
            return int(entry.name)
    return None


def _sid3_to_sid64(sid3: int) -> int:
    return 0x110000100000000 + sid3


def _sid64_to_sid3(sid64: int) -> int:
    return sid64 - 0x110000100000000


def resolve_steamid(raw: str, steam_root: Path | None) -> int | None:
    if raw.lower() == "auto":
        if steam_root is None:
            return None
        sid3 = _find_steamid3(steam_root)
        return _sid3_to_sid64(sid3) if sid3 else None
    try:
        return int(raw)
    except ValueError:
        return None


def find_schema(appid: int, steam_root: Path | None) -> Path | None:
    if steam_root:
        p = steam_root / "appcache" / "stats" / ("UserGameStatsSchema_%d.bin" % appid)
        if p.is_file():
            return p
    return None


def find_ugs(appid: int, steamid64: int | None, steam_root: Path | None) -> Path | None:
    if steam_root is None or steamid64 is None:
        return None
    base = steam_root / "appcache" / "stats"
    sid3 = _sid64_to_sid3(steamid64)
    for uid in (sid3, steamid64):
        p = base / ("UserGameStats_%d_%d.bin" % (uid, appid))
        if p.is_file():
            return p
    return None


def _find_gse_save_dir(appid: int) -> Path | None:
    candidates = []
    appdata = os.environ.get("APPDATA")
    if appdata:
        candidates += [
            Path(appdata) / "GSE Saves" / str(appid),
            Path(appdata) / "Goldberg SteamEmu Saves" / str(appid),
        ]
    candidates += [
        Path("gse_save") / str(appid),
        Path("steam_saves") / str(appid),
    ]
    for c in candidates:
        if c.is_dir() and any((c / f).exists() for f in ("achievements.json", "stats.json", "stats")):
            return c
    return None


# ---------------------------------------------------------------------------
# --info  :  show schema + current progress
# ---------------------------------------------------------------------------

def _bar(cur: int, mx: int, width: int = 20) -> str:
    pct    = min(cur, mx) / mx if mx else 0
    filled = int(pct * width)
    return "#" * filled + "-" * (width - filled)


def cmd_info(appid: int, steamid64: int | None,
             steam_root: Path | None, lang: str) -> None:

    schema_path = find_schema(appid, steam_root)
    if not schema_path:
        sys.exit("[error] Schema file not found for appid %d" % appid)

    _, achievements, stats = parse_schema(str(schema_path), lang)

    ugs: UGSData | None = None
    ugs_path = find_ugs(appid, steamid64, steam_root)
    if ugs_path:
        ugs = parse_ugs(str(ugs_path))

    # stat API name -> current int value
    stat_val_by_name: dict[str, int] = {}
    if ugs:
        for s in stats:
            raw = ugs.stat_vals.get(s._group)
            if raw is not None:
                stat_val_by_name[s.name] = raw

    total  = len(achievements)
    earned = sum(1 for a in achievements
                 if ugs and (a._group, a._bit) in ugs.ach_times)

    print("App %d  |  %d / %d achievements (%.0f%%)" % (
        appid, earned, total, earned / total * 100 if total else 0))
    print("Schema   : %s" % schema_path)
    if ugs_path:
        print("User data: %s" % ugs_path)
    else:
        print("User data: NOT FOUND (schema only)")
    print()

    # achievements table
    print("%-38s %-40s %-9s  %-24s  %s" % (
        "Achievement", "Description", "Status", "Progress", "Unlocked"))
    print("-" * 122)

    for a in achievements:
        ts     = ugs.ach_times.get((a._group, a._bit)) if ugs else None
        status = "EARNED" if ts else ("LOCKED[H]" if a.hidden else "LOCKED")
        date   = datetime.datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M") if ts else ""

        prog_str = ""
        if a.prog_stat_name and not ts and ugs:
            cur = stat_val_by_name.get(a.prog_stat_name, 0)
            try:
                mx   = int(a.prog_max)
                pct  = min(cur, mx) / mx * 100 if mx else 0
                prog_str = "%d/%d [%s] %.0f%%" % (cur, mx, _bar(cur, mx), pct)
            except (TypeError, ValueError):
                prog_str = "cur=%s" % cur
        elif ts:
            prog_str = "100%"

        print("%-38s %-40s %-9s  %-24s  %s" % (
            (a.name or a.api)[:38],
            (a.desc or "-")[:40],
            status, prog_str, date))

    # stats table
    if stats:
        print()
        print("=== STATS  (%d total) ===" % len(stats))
        print("%-40s %-8s  %s" % ("Name", "Type", "Value"))
        print("-" * 65)
        for s in stats:
            raw = ugs.stat_vals.get(s._group) if ugs else None
            if raw is None:
                val_str = "(no data)"
            elif s.gtype in ("FLOAT", "AVGRATE"):
                val_str = "%.4f" % struct.unpack("<f", struct.pack("<I", raw & 0xFFFFFFFF))[0]
            else:
                iv = raw if raw < 0x80000000 else raw - 0x100000000
                val_str = str(iv)
            print("%-40s %-8s  %s" % (s.name, s.gtype, val_str))


# ---------------------------------------------------------------------------
# --st2gse  :  Steam binary -> GSE save files
# ---------------------------------------------------------------------------

def cmd_st2gse(appid: int, steamid64: int | None,
               steam_root: Path | None,
               out_dir: Path, verbose: bool, lang: str) -> None:

    schema_path = find_schema(appid, steam_root)
    if not schema_path:
        sys.exit("[error] Schema not found for appid %d" % appid)

    ugs_path = find_ugs(appid, steamid64, steam_root)
    if not ugs_path:
        sys.exit("[error] UserGameStats file not found for appid %d (steamid=%s)" % (
            appid, steamid64))

    _, achievements, stats = parse_schema(str(schema_path), lang)
    ugs = parse_ugs(str(ugs_path))

    print("Schema  : %s" % schema_path)
    print("UGS     : %s" % ugs_path)
    print("Out dir : %s" % out_dir)
    print()

    out_dir.mkdir(parents=True, exist_ok=True)

    # achievements.json — complete manifest sorted by (_group int, _bit int)
    # all defined achievements present; earned ones get their timestamp, others get earned=false
    sorted_achs = sorted(achievements, key=lambda a: (int(a._group), int(a._bit)))
    user_achs: dict[str, dict] = {}
    earned_count = 0
    for a in sorted_achs:
        ts = ugs.ach_times.get((a._group, a._bit))
        if ts is not None:
            user_achs[a.api] = {"earned": True, "earned_time": ts, "_group": a._group, "_bit": a._bit}
            earned_count += 1
            if verbose:
                dt = datetime.datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M:%S")
                print("  [ach] %-35s  %s  %s" % (a.api, dt, ("(%s)" % a.name) if a.name else ""))
        else:
            user_achs[a.api] = {"earned": False, "earned_time": 0, "_group": a._group, "_bit": a._bit}

    if sorted_achs:
        (out_dir / "achievements.json").write_text(
            json.dumps(user_achs, indent=2), encoding="utf-8")
        print("[+] achievements.json  ->  %d / %d earned" % (earned_count, len(sorted_achs)))
    else:
        print("[i] achievements.json  ->  skipped (no achievements in schema)")

    # stats.json — all stats sorted by integer group ID; avgrate includes count/sessionlength
    stats_arr = []
    for s in sorted(stats, key=lambda x: int(x._group)):
        raw = ugs.stat_vals.get(s._group)
        if s.gtype == "INT":
            val = (raw if raw < 0x80000000 else raw - 0x100000000) if raw is not None else (
                int(s.default) if isinstance(s.default, (int, float)) else 0)
            stats_arr.append({"name": s.name, "type": "int", "value": val, "_group": s._group})
            if verbose and raw is not None:
                print("  [stat] %-35s  int=%d" % (s.name, val))
        elif s.gtype == "FLOAT":
            val = struct.unpack("<f", struct.pack("<I", raw & 0xFFFFFFFF))[0] if raw is not None else (
                float(s.default) if s.default is not None else 0.0)
            stats_arr.append({"name": s.name, "type": "float", "value": val, "_group": s._group})
            if verbose and raw is not None:
                print("  [stat] %-35s  float=%g" % (s.name, val))
        elif s.gtype == "AVGRATE":
            val = struct.unpack("<f", struct.pack("<I", raw & 0xFFFFFFFF))[0] if raw is not None else (
                float(s.default) if s.default is not None else 0.0)
            stats_arr.append({"name": s.name, "type": "avgrate", "value": val,
                               "count": 0.0, "sessionlength": 0.0, "_group": s._group})
            if verbose and raw is not None:
                print("  [stat] %-35s  avgrate=%g" % (s.name, val))

    if stats_arr:
        (out_dir / "stats.json").write_text(
            json.dumps(stats_arr, indent=2), encoding="utf-8")
        print("[+] stats.json         ->  %d stats" % len(stats_arr))
    else:
        print("[i] stats.json         ->  skipped (no stats in schema)")

    # UserGameStats bin — build from the JSON files we just created (scratch mode)
    if steamid64 is not None:
        sid3 = _sid64_to_sid3(steamid64)
        bin_name = "UserGameStats_%d_%d.bin" % (sid3, appid)
        cmd_gse2st(appid=appid, steamid64=steamid64,
                   schema_path=schema_path, gse_dir=out_dir,
                   out_path=out_dir / bin_name, verbose=False,
                   ugs_path=None)


# ---------------------------------------------------------------------------
# --gse2st  :  GSE save files -> Steam binary
# ---------------------------------------------------------------------------

def cmd_gse2st(appid: int, steamid64: int,
               schema_path: Path, gse_dir: Path,
               out_path: Path, verbose: bool,
               ugs_path: Path | None = None) -> None:

    _, achievements, stats = parse_schema(str(schema_path))

    ach_json  = gse_dir / "achievements.json"
    stats_dir = gse_dir / "stats"

    user_achs: dict[str, dict] = {}
    if ach_json.is_file():
        user_achs = json.loads(ach_json.read_text(encoding="utf-8"))

    # Read stats from stats.json (current format) with fallback to individual stats/ binary files
    user_stats_raw: dict[str, int] = {}   # name_lower -> raw uint32
    user_stats_group: dict[str, str] = {} # name_lower -> group id string (from _group field)
    stats_json_path = gse_dir / "stats.json"
    if stats_json_path.is_file():
        try:
            arr = json.loads(stats_json_path.read_text(encoding="utf-8"))
            if isinstance(arr, list):
                for entry in arr:
                    if not isinstance(entry, dict) or "name" not in entry or "value" not in entry:
                        continue
                    t = entry.get("type", "int").lower()
                    try:
                        if t == "int":
                            raw_u = struct.unpack("<I", struct.pack("<i", int(entry["value"])))[0]
                        else:
                            raw_u = struct.unpack("<I", struct.pack("<f", float(entry["value"])))[0]
                        name_lo = entry["name"].lower()
                        user_stats_raw[name_lo] = raw_u
                        grp = entry.get("_group")
                        if grp is not None:
                            user_stats_group[name_lo] = str(grp)
                    except (struct.error, ValueError, OverflowError):
                        pass
        except (json.JSONDecodeError, OSError):
            pass
    elif stats_dir.is_dir():
        # Legacy fallback: individual stats/<name> binary files
        for f in stats_dir.iterdir():
            if f.is_file() and f.stat().st_size >= 4:
                user_stats_raw[f.name.lower()] = struct.unpack_from("<I", f.read_bytes())[0]

    ach_by_api:   dict[str, AchievementDef] = {a.api: a for a in achievements}
    stat_by_name: dict[str, StatDef]        = {s.name.lower(): s for s in stats}

    # Carry forward CRC from original bin if available (for no-change detection).
    # All group data is built purely from the JSON files regardless.
    orig_crc = 0
    if ugs_path is not None and ugs_path.is_file():
        raw_bytes = ugs_path.read_bytes()
        if len(raw_bytes) >= 16:
            orig_crc = struct.unpack_from("<I", raw_bytes, 12)[0]

    # Build groups from JSON data (schema + user saves)
    groups: dict[str, dict] = {}

    for api, ach_data in user_achs.items():
        if not ach_data.get("earned", False):
            continue
        grp = ach_data.get("_group"); bit_s = ach_data.get("_bit")
        if grp is not None and bit_s is not None:
            gid, bit_str = str(grp), str(bit_s)
        else:
            a = ach_by_api.get(api)
            if a is None:
                if verbose:
                    print("  [warn] achievement '%s' not in schema/json, skipping" % api)
                continue
            gid, bit_str = a._group, a._bit
        ts  = int(ach_data.get("earned_time", 0))
        bit = int(bit_str)
        if gid not in groups:
            groups[gid] = {"data": 0, "AchievementTimes": {}}
        groups[gid]["data"] |= (1 << bit)
        groups[gid]["AchievementTimes"][bit_str] = ts
        if verbose:
            print("  [ach] %-35s  group=%s bit=%s ts=%d" % (api, gid, bit_str, ts))

    for name_lower, raw_u in user_stats_raw.items():
        grp = user_stats_group.get(name_lower)
        if grp is None:
            s = stat_by_name.get(name_lower)
            if s is None:
                if verbose:
                    print("  [warn] stat '%s' not in schema/json, skipping" % name_lower)
                continue
            grp = s._group
        if grp not in groups:
            groups[grp] = {}
        groups[grp]["data"] = raw_u
        if verbose:
            s = stat_by_name.get(name_lower)
            print("  [stat] %-35s  raw=0x%08X" % (s.name if s else name_lower, raw_u))

    cache: dict[str, Any] = {"crc": orig_crc, "PendingChanges": 0}
    for gid in sorted(groups, key=lambda x: int(x)):
        g = groups[gid]
        data_val = g.get("data", 0)
        if data_val == 0:
            continue  # skip zero groups to match Steam / write_ugs_bin behaviour
        node: dict[str, Any] = {"data": data_val}
        ach_times = g.get("AchievementTimes")
        if ach_times is not None:
            node["pendingbits"] = data_val  # all unlocked bits are pending
            node["AchievementTimes"] = {
                k: ach_times[k]
                for k in sorted(ach_times, key=lambda x: int(x))
            }
        cache[gid] = node

    # Recompute PendingChanges: count all groups (dicts) with non-zero data
    cache["PendingChanges"] = sum(
        1 for v in cache.values() if isinstance(v, dict) and v.get("data", 0) != 0
    )

    out_bytes = _write_vdf_root("cache", cache)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(out_bytes)
    print("[+] Written %d bytes -> %s" % (len(out_bytes), out_path))


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        prog="steam_stats_converter",
        description="Bidirectional Steam <-> GSE achievements/stats converter",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )

    p.add_argument("--appid", type=int, required=True,
                   help="Steam App ID  (required)")

    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--info",   action="store_true",
                      help="Show schema + your current progress/stats")
    mode.add_argument("--st2gse", action="store_true",
                      help="Convert Steam binary -> GSE save files")
    mode.add_argument("--gse2st", action="store_true",
                      help="Convert GSE save files -> Steam binary")

    p.add_argument("--steamid", default="auto",
                   help="64-bit SteamID or 'auto'  [default: auto]")
    p.add_argument("--out-dir",
                   help="Output dir for --st2gse  [default: gse_save/<appid>]")
    p.add_argument("--gse-dir",
                   help="GSE save dir for --gse2st  (contains achievements.json "
                        "and/or stats/)  [default: auto-detect]")
    p.add_argument("--out",
                   help="Output .bin for --gse2st  "
                        "[default: UserGameStats_<steamid3>_<appid>.bin]")
    p.add_argument("--lang", default="english",
                   help="Language for achievement names in --info  [default: english]")
    p.add_argument("-v", "--verbose", action="store_true",
                   help="Print every achievement/stat as it is processed")

    args = p.parse_args()

    steam_root = _find_steam_root()
    steamid64  = resolve_steamid(args.steamid, steam_root)
    steamid3   = _sid64_to_sid3(steamid64) if steamid64 else None

    if args.info:
        cmd_info(args.appid, steamid64, steam_root, args.lang)

    elif args.st2gse:
        out_dir = Path(args.out_dir) if args.out_dir else Path("gse_save") / str(args.appid)
        cmd_st2gse(
            appid=args.appid, steamid64=steamid64,
            steam_root=steam_root, out_dir=out_dir,
            verbose=args.verbose, lang=args.lang,
        )

    elif args.gse2st:
        schema_path = find_schema(args.appid, steam_root)
        if not schema_path:
            sys.exit("[error] Schema not found for appid %d" % args.appid)
        if not steamid64:
            sys.exit("[error] Cannot determine SteamID. Provide --steamid <id>")

        if args.gse_dir:
            gse_dir = Path(args.gse_dir)
        else:
            gse_dir = _find_gse_save_dir(args.appid)
            if gse_dir is None:
                gse_dir = Path("gse_save") / str(args.appid)
                if not gse_dir.exists():
                    sys.exit(
                        "[error] GSE save dir not found. Use --gse-dir or run --st2gse first.\n"
                        "  Tried: %%APPDATA%%\\GSE Saves\\%d  and  gse_save\\%d" % (
                            args.appid, args.appid))

        out_path = Path(args.out) if args.out else \
            Path("UserGameStats_%d_%d.bin" % (steamid3, args.appid))

        ugs_path = find_ugs(args.appid, steamid64, steam_root)

        print("Schema  : %s" % schema_path)
        print("GSE dir : %s" % gse_dir)
        if ugs_path:
            print("Base UGS: %s" % ugs_path)
        print("SteamID : %d  (id3=%d)" % (steamid64, steamid3))
        print("Out     : %s" % out_path)
        print()
        cmd_gse2st(
            appid=args.appid, steamid64=steamid64,
            schema_path=schema_path, gse_dir=gse_dir,
            out_path=out_path, verbose=args.verbose,
            ugs_path=ugs_path,
        )


if __name__ == "__main__":
    main()
