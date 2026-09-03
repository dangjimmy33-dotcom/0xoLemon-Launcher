import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const layout = await readFile(new URL('../components/layout.tsx', import.meta.url), 'utf8')
const titlebar = await readFile(new URL('../components/CustomTitleBar.tsx', import.meta.url), 'utf8')
const settings = await readFile(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const library = await readFile(new URL('../components/library.tsx', import.meta.url), 'utf8')
const prefs = await readFile(new URL('./preferences.ts', import.meta.url), 'utf8')
const theme = await readFile(new URL('./theme.ts', import.meta.url), 'utf8')
const host = await readFile(new URL('../themes/ThemeShellHost.tsx', import.meta.url), 'utf8')
const contracts = await readFile(new URL('../themes/contracts.ts', import.meta.url), 'utf8')
const steamShell = await readFile(new URL('../themes/steam/SteamShell.tsx', import.meta.url), 'utf8')
const steamCss = await readFile(new URL('../themes/steam.css', import.meta.url), 'utf8')
const steamSettings = await readFile(new URL('../themes/steam/SteamSettingsView.tsx', import.meta.url), 'utf8')
const steamSettingsCss = await readFile(new URL('../themes/steam/SteamSettingsView.css', import.meta.url), 'utf8')

test('Steam theme owns a horizontal shell instead of recoloring the default sidebar', () => {
  assert.doesNotMatch(layout, /SteamPrimaryNav/)
  assert.match(app, /ThemeShellHost/)
  assert.match(contracts, /loadShell:\s*\(\)\s*=>\s*import\('\.\/steam\/SteamShell'\)/)
  assert.match(host, /loadedShells/)
  assert.match(steamShell, /steam-shell-layout/)
  assert.match(steamCss, /\.steam-primary-nav/)
  assert.match(steamCss, /grid-template-rows:/)
  assert.doesNotMatch(app, /import\s+['"]\.\/themes\/steam\.css['"]/, 'Steam CSS must not enter the default bundle')
})

test('Steam theme defaults to its native palette and keeps custom accent explicit', () => {
  assert.match(prefs, /themeAccentMode:\s*ThemeAccentMode/)
  assert.match(prefs, /themeAccentMode:\s*'native'/)
  assert.match(theme, /preferences\.themeAccentMode === 'native'/)
  assert.match(theme, /profile\.nativePalette/)
})

test('changing interface theme persists and restarts automatically', () => {
  assert.ok(!settings.includes('ui-theme-restart-required'), 'theme selection must not wait for a manual restart action')
  assert.match(app, /key === 'uiTheme'/)
  assert.match(app, /saveLauncherPreferences\(next\)/)
  assert.match(app, /invoke\('restart_launcher'\)/)
  assert.match(app, /sessionUiTheme/)
})

test('Steam shell restyles Settings and shared overlays structurally', () => {
  assert.match(steamSettingsCss, /\.steam-settings-view/)
  assert.match(steamSettingsCss, /\.steam-settings-view > \.settings-pane-nav/)
  assert.doesNotMatch(steamCss, /Canonical Steam Settings workbench/)
  assert.match(steamCss, /\.install-modal/)
  assert.match(steamCss, /\.confirm-dialog/)
  assert.match(steamCss, /\.ctx-menu/)
  assert.match(steamCss, /\.library-/)
})

test('Steam title chrome exposes Steam-style menus while preserving window actions', () => {
  assert.match(titlebar, /steam-client-menu/)
  assert.match(titlebar, /uiTheme === 'steam'/)
  assert.match(titlebar, /0xoLemon/)
  assert.match(titlebar, /View/)
  assert.match(titlebar, /Games/)
  assert.match(titlebar, /Friends/)
  assert.match(titlebar, /Help/)
})


test('Steam skin gives Library a persistent Steam-style game rail in browse and detail', () => {
  assert.match(library, /steam-library-rail/)
  assert.match(library, /steam-library-detail-rail/)
  assert.match(steamCss, /\.steam-library-rail/)
  assert.match(steamCss, /\.steam-library-detail-rail/)
})


test('Steam settings becomes a Steam-style settings window with its own close action', () => {
  assert.match(contracts, /loadSettingsView:\s*\(\)\s*=>\s*import\('\.\/steam\/SteamSettingsView'\)/)
  assert.match(steamSettings, /presentation="steam"/)
  assert.match(settings, /data-settings-presentation=\{presentation\}/)
  assert.match(settings, /onClose/)
  assert.match(app, /onClose=\{\(\) => setActiveTab\('Home'\)\}/)
  assert.match(steamSettingsCss, /\.steam-settings-view > \.settings-pane-nav/)
  assert.match(steamSettingsCss, /overflow-y:\s*auto/)
})

test('Steam account tab opens the authenticated profile while the brand remains Home', () => {
  assert.match(steamShell, /steam-nav-home-mark[\s\S]*navigate\('Home'\)/)
  assert.match(steamShell, /onOpenSelfProfile\(\)/)
  assert.doesNotMatch(steamShell, /\['Home',\s*\(displayName/)
})
