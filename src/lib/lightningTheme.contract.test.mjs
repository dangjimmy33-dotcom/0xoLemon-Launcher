import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const themes = readFileSync(new URL('./uiThemes.ts', import.meta.url), 'utf8')
const preferences = readFileSync(new URL('./preferences.ts', import.meta.url), 'utf8')
const contracts = readFileSync(new URL('../themes/contracts.ts', import.meta.url), 'utf8')
const shell = readFileSync(new URL('../themes/lightning/LightningShell.tsx', import.meta.url), 'utf8')
const shellCss = readFileSync(new URL('../themes/lightning/shell.css', import.meta.url), 'utf8')
const pickerCss = readFileSync(new URL('../themes/theme-picker.css', import.meta.url), 'utf8')
const settings = readFileSync(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const desktopBootstrap = readFileSync(new URL('../desktop/bootstrap.tsx', import.meta.url), 'utf8')
const app = readFileSync(new URL('../App.tsx', import.meta.url), 'utf8')
const activeView = readFileSync(new URL('../components/ActiveView.tsx', import.meta.url), 'utf8')
const types = readFileSync(new URL('../types.ts', import.meta.url), 'utf8')

assert.ok(themes.includes("id: 'lightning'"), 'Lightning must be registered as a built-in theme')
assert.ok(themes.includes("label: '0xoLemon Cinematic'"), 'the migrated theme must use the 0xoLemon Cinematic display name')
assert.ok(themes.includes("recommended: true"), 'Lightning must be the recommended standard theme')
assert.ok(preferences.includes("uiTheme: 'lightning'"), 'new users must start with Lightning')
assert.ok(preferences.includes('themeEngineVersion: 3'), 'theme migration must be versioned')
assert.ok(contracts.includes("import('./lightning/LightningShell')"), 'Lightning must stay out of the default module graph')
assert.ok(contracts.includes("motionProfile: 'lightning-cinematic'"), 'Lightning must own its motion profile')
assert.ok(shell.includes('data-theme-reference="project-lightning-v5.0.8-installed"'), 'the installed v5.0.8 renderer must be pinned')
assert.ok(contracts.includes('onOpenSelfProfile: () => void'), 'theme shells must retain the real social profile bridge')
assert.ok(shell.includes('0xoLemon'), 'visible product branding must be 0xoLemon')
assert.ok(!shell.includes('Partner Edition'), 'the replacement renderer must not expose the old dashboard branding')

for (const label of ['Home', 'Nexus', 'Library', 'Instant Gaming', 'Tools', 'Bypass', 'OnlineFix', 'Settings']) {
  assert.ok(shell.includes(`label: '${label}'`), `${label} must be available in primary navigation`)
}
for (const route of ["'Offline Activation'", "'Lua Shop'", "'Lua Installer'", "'CloudRedirect'", "'Downloads'"]) {
  assert.ok((app + activeView + types).includes(route), `${route} must remain a real launcher route outside the installed v5 primary nav`)
}

assert.ok(shellCss.includes("html[data-ui-theme='lightning']"), 'Lightning shell CSS must be scoped')
assert.match(shellCss, /grid-template-rows:\s*30px 64px minmax\(0, 1fr\) 24px/, 'the renderer must preserve the installed title, horizontal nav, content and status tracks')
assert.doesNotMatch(shellCss, /\.lightning-primary-nav:hover[\s\S]*?width:\s*226px/, 'the obsolete expanding side rail must stay removed')
assert.ok(pickerCss.includes('.theme-preview-lightning'), 'Settings must preview Lightning geometry')
assert.ok(settings.includes("theme.tier === 'standard'"), 'Settings must separate standard and additional themes')
assert.ok(desktopBootstrap.includes("fixture === 'theme-lightning'"), 'Lightning must provide a desktop-only visual development fixture')

console.log('lightningTheme.contract: PASS')
