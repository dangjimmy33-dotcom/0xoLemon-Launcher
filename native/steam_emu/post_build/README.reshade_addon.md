# GSE Overlay — ReShade Addon

This is an alternative overlay renderer that uses [ReShade](https://reshade.me/) instead of the built-in ingame_overlay library.

## Files

| File | Description |
|------|-------------|
| `gse_overlay.addon64` | 64-bit ReShade addon |
| `gse_overlay.addon` | 32-bit ReShade addon |

## Installation

1. Install ReShade into your game (use the ReShade installer).
2. Copy the appropriate `.addon64` (64-bit) or `.addon` (32-bit) file into the same directory as the game executable (next to ReShade's `dxgi.dll` / `d3d11.dll` / etc.).
3. The addon automatically connects to the emu DLL (`steam_api64.dll` / `steam_api.dll`) via the GSE bridge.

## Usage

- **Shift+Tab** — Toggle the main overlay (same as native overlay).
- The addon renders notifications, achievement icons, stats HUD, friends list, settings, and the SCE asset browser — all through ReShade's ImGui integration.
- A settings tab ("GSE Overlay Settings") also appears inside ReShade's own overlay panel.

## Requirements

- ReShade v5.0+ (API v18 or newer recommended).
- The emu DLL must be the **experimental** build (`api_experimental`) which exports the `GSE_OverlayBridge_*` functions.

## Notes

- The addon is Windows-only.
- It does **not** link against any emu internals — communication happens entirely through the C ABI bridge.
- If ReShade is not installed, this file is simply ignored by the game.
