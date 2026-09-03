# Theme Packages

The authenticated launcher UI is rendered by one active `ThemePackage`. Default,
Lightning, Steam and XMCL share launcher data, commands and domain actions, but own their
shell, route frame, motion profile and scoped styles.

## Reference Baselines

- `default`: `0xolemon-v2`
- `lightning`: `project-lightning-partner-2026.08`
- `steam`: `steam-desktop-2026.08`
- `xmcl`: `xmcl-source-2026.08`

Reference versions are intentionally pinned. A newer Steam client or XMCL source
does not silently alter the launcher. Rebaseline the reference version together
with updated visual snapshots and interaction tests.

## Runtime Rules

- Intro, Discord verification and onboarding always use Default.
- Only the selected authenticated theme package is dynamically imported.
- A theme load/render failure falls back to Default for the current session and
  does not overwrite the saved preference.
- Lightning, Steam and XMCL use their native palette by default. Custom Accent is opt-in.
- Temporary dialogs are never restored after a theme-change restart.
- Library collections, shelves and XMCL groups are persisted by Rust in AppData.

## Source And Licensing

The Lightning package is ported from the partner source supplied at
`E:\Project-Lightning-main\Project-Lightning-main`. The Project Lightning name is
replaced by 0xoLemon branding in the shipped shell. Its WPF navigation model,
motion and approved media are adapted to React, while downloads, archives,
SteamLess, Lua, OnlineFix and activation actions are delegated to 0xoLemon's
validated Rust services instead of copying the older direct-download routines.

The Steam package is a clean-room reconstruction from the installed client and
Valve's public interface documentation. It does not bundle Valve logos, client
JavaScript or other proprietary client assets. Steam-specific operation names,
such as Add to Steam and Restart Steam, remain unchanged where they describe a
real Steam action.

XMCL interaction and navigation references come from the MIT-licensed source at
`E:\Compressed\x-minecraft-launcher-master`. The launcher ports only the design
language that applies to 0xoLemon games-as-instances. Minecraft-only runtime,
modpack, Java and resource-pack features are not included.

The directories under `E:\Compressed\steam\_UI` are supplemental community
references only and are not shipped as source or runtime assets.

## Visual Fixtures

Development-only fixtures provide deterministic package validation without
Discord authorization:

- `?fixture=theme-steam`
- `?fixture=theme-xmcl`
- `?fixture=theme-lightning`

Use a dedicated test port, not the Vercel preview ports `1425` or `5174`.
