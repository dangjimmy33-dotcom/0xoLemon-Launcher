import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const preferences = await readFile(new URL('./preferences.ts', import.meta.url), 'utf8')
const registry = await readFile(new URL('./uiThemes.ts', import.meta.url), 'utf8').catch(() => '')
const theme = await readFile(new URL('./theme.ts', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const settings = await readFile(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const onboarding = await readFile(new URL('../components/Onboarding.tsx', import.meta.url), 'utf8')
const contracts = await readFile(new URL('../themes/contracts.ts', import.meta.url), 'utf8')
const host = await readFile(new URL('../themes/ThemeShellHost.tsx', import.meta.url), 'utf8')
const steamCss = await readFile(new URL('../themes/steam.css', import.meta.url), 'utf8').catch(() => '')
const pickerCss = await readFile(new URL('../themes/theme-picker.css', import.meta.url), 'utf8').catch(() => '')

assert.ok(preferences.includes('uiTheme'), 'launcher preferences must persist selected UI theme')
assert.ok(preferences.includes('themeAccentMode'), 'Steam/XMCL must persist native versus explicit custom accent mode')
assert.ok(registry.includes("id: 'default'"), 'theme registry must expose the default launcher theme')
assert.ok(registry.includes("id: 'lightning'"), 'theme registry must expose the Lightning partner theme')
assert.ok(registry.includes("id: 'steam'"), 'theme registry must expose the Steam-inspired theme')
assert.ok(theme.includes("data-ui-theme"), 'theme engine must publish the active UI theme on the document root')
assert.ok(theme.includes("data-theme-accent-mode"), 'theme engine must publish theme/custom accent mode')
assert.ok(app.includes('ThemeShellHost'), 'App must delegate authenticated shell rendering to the theme host')
assert.ok(!app.includes("import './themes/steam.css'"), 'Steam styles must be absent from the default module graph')
assert.ok(contracts.includes("import('./steam/SteamShell')"), 'Steam shell must be dynamically imported')
assert.ok(contracts.includes("import('./lightning/LightningShell')"), 'Lightning shell must be dynamically imported')
assert.ok(contracts.includes("import('./xmcl/XmclShell')"), 'XMCL shell must be dynamically imported')
assert.ok(host.includes('loadedShells'), 'loaded theme packages must be cached without mounting inactive shells')
assert.ok(settings.includes('UI_THEME_PROFILES'), 'Settings must render theme choices from the theme registry')
assert.ok(settings.includes("onChange('uiTheme'"), 'theme selection must persist through LauncherPreferences')
assert.ok(settings.includes("onChange('themeAccentMode'"), 'native/custom accent selection must be explicit')
assert.ok(onboarding.includes('theme-quick-pick'), 'first-run onboarding must offer the UI theme choice')
assert.match(steamCss, /html\[data-ui-theme=['"]steam['"]\]/, 'Steam theme must be scoped to data-ui-theme=steam')
for (const selector of ['.install-modal', '.confirm-dialog', '.cs-dropdown', '.settings-group', '.sidebar', '.custom-titlebar']) {
  assert.ok(steamCss.includes(selector), `Steam overlay must theme ${selector}`)
}
for (const token of ['--ui-popup-bg', '--ui-panel-bg', '--ui-sidebar-bg', '--ui-selection-bg', '--ui-radius-md']) {
  assert.ok(steamCss.includes(token), `Steam overlay must define semantic theme token ${token}`)
}
assert.ok(pickerCss.includes('.ui-theme-picker'), 'theme picker must have dedicated styling')

console.log('UI theme package contract PASS')
