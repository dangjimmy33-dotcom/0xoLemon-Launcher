// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

// Registry of per-module log channels for LumaCore.
//
// Each LC_MOD(VarName, "filename") line registers one module logger.
// The file is included twice with different definitions of LC_MOD:
//   - Logger.h uses it to declare a shared_ptr<spdlog::logger> for each module.
//   - Logger.cpp uses it to create the spdlog file sinks.
// cmake/LogMacros.cmake reads it a third time to generate the LOG_<MOD>_* macros
// that hook code uses (e.g. LOG_IPCCH_INFO, LOG_MANIFESTCH_WARN).
//
// To add a new module: add one LC_MOD line here, then re-run CMake so the
// macro header gets regenerated. No other files need to change.
//
// Channel identifiers carry a `Ch` suffix so they stay LumaCore-specific and
// cannot collide with shared channel tokens like `IPC`, `NetPacket`, `Package`.

LC_MOD(IpcCh,           "ipc")
LC_MOD(WireCh,          "netpacket")
LC_MOD(ManifestCh,      "manifest")
LC_MOD(KeyValueCh,      "keyvalue")
LC_MOD(DecryptionKeyCh, "decryptionkey")
LC_MOD(MiscCh,          "misc")
LC_MOD(AchievementCh,   "achievement")
LC_MOD(PicsCh,          "pics")
LC_MOD(OnlineFixCh,     "onlinefix")
LC_MOD(PkgCh,           "package")
LC_MOD(LicenseCh,       "license")
LC_MOD(SteamUiCh,       "steamui")
LC_MOD(ManBndCh,        "manbnd")
LC_MOD(IpcRtrCh,        "ipcrtr")
LC_MOD(UsrCmdCh,        "usrcmd")
LC_MOD(PktRtCh,         "pktrt")
LC_MOD(AuthCh,          "auth")
LC_MOD(CoreInCh,        "corein")
LC_MOD(EticketCh,       "eticket")
