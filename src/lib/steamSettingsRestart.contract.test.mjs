import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const settings = await readFile(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const steamSettingsCss = await readFile(new URL('../themes/steam/SteamSettingsView.css', import.meta.url), 'utf8')
const rust = await readFile(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8')

test('changing UI theme persists first and restarts the launcher automatically', () => {
  const updatePreference = app.match(/function updatePreference[\s\S]*?\n  }\n\n  async function updateLauncherSetting/)?.[0] ?? ''
  assert.match(updatePreference, /key === 'uiTheme'/)
  assert.match(updatePreference, /saveLauncherPreferences\(next\)/)
  assert.match(updatePreference, /invoke\('restart_launcher'\)/)
  assert.ok(!settings.includes('ui-theme-restart-required'), 'theme picker must not wait for a manual Restart now action')
  assert.match(rust, /fn restart_launcher[\s\S]*?request_restart\(\)/)
})

test('Steam settings owns a bounded internal workbench and cannot overlap the workspace', () => {
  assert.match(app, /activeTab === 'Settings' \? 'settings-tab-content' : ''/)
  assert.match(steamSettingsCss, /\.tab-content\.settings-tab-content\s*\{[\s\S]*?overflow:\s*hidden/)
  assert.match(steamSettingsCss, /\.steam-settings-view\s*\{[\s\S]*?height:\s*100%[\s\S]*?max-height:\s*100%/)
  assert.match(steamSettingsCss, /\.steam-settings-view\s*>\s*\.settings-pane-nav\s*\{[\s\S]*?overflow:\s*hidden/)
  assert.match(steamSettingsCss, /\.steam-settings-view\s*>\s*\.settings-sections\s*\{[\s\S]*?min-height:\s*0[\s\S]*?overflow-y:\s*auto/)
})
