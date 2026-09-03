# UI theme engine

The launcher keeps one behavior/data model. An interface theme is a restart-gated structural package applied to the whole client, not a recolored Library and not a duplicated app.

## Runtime contract

`applyLauncherTheme()` writes the active `data-ui-theme` to `<html>`. The interface theme selected in Settings is persisted immediately, but the running session keeps the theme that was active at process start. Applying a different interface theme therefore requires `restart_launcher`; this prevents the shell, dialogs, Settings, Library, and overlays from briefly existing in mixed layouts.

Default uses Color Studio. Lightning, Steam and XMCL start with their reference palette and allow an explicit Custom Accent override.

## Renderer ownership rule

Shared behavior stays in the existing React view models and actions. Each package
owns one dynamically loaded shell and route frame; shells from inactive themes
must not remain mounted or be hidden with CSS. Theme-specific Settings or Library
renderers may be added when the reference interaction cannot be expressed by the
shared view, but they must call the same typed launcher actions and Rust commands.

## Semantic surface tokens

Theme CSS should consume the runtime semantic variables instead of hard-coded brand colors:

- `--launcher-page-bg` / `--launcher-sidebar-bg`
- `--theme-card-bg` / `--theme-card-bg-soft`
- `--theme-accent` / `--theme-accent-strong`
- `--text` / `--text-strong` / `--muted`
- `--line` / `--line-strong`
- `--ui-page-bg` / `--ui-frame-bg` / `--ui-panel-bg` / `--ui-popup-bg`
- `--ui-selection-bg` / `--ui-hover-bg`

## Adding another built-in theme

1. Add a profile to `lib/uiThemes.ts`.
2. Add a package folder with a shell, route frame and scoped styles.
3. Register dynamic imports in `themes/contracts.ts`.
4. Add only the route renderers the package genuinely owns.
5. Choose a native palette and keep Custom Accent opt-in.
6. Keep theme selection restart-gated.
7. Add interaction, contract and visual tests for shell, Settings, Library, and shared overlays.

Never patch individual popups one by one with unrelated colors. The theme contract covers the complete launcher shell and its shared surfaces.

## Built-in reference skins

- `lightning/`: recommended full-launcher package adapted from the Project Lightning partner source. Project feature names route to the hardened 0xoLemon actions rather than copying old direct-download code.
- `steam.css`: Steam-inspired workbench shell.
- `xmcl/`: X Minecraft Launcher-inspired package. It adapts the reference project's compact 80px navigation, frosted shared surfaces, rounded card tiers, and calm route/surface motion to 0xoLemon's existing React component tree. It does not import XMCL Vue/runtime code; Color Wheel remains the accent source.
