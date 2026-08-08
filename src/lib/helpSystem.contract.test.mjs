import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const registry = await readFile(new URL('./helpRegistry.ts', import.meta.url), 'utf8').catch(() => '')
const helpUi = await readFile(new URL('../components/HelpSystem.tsx', import.meta.url), 'utf8').catch(() => '')
const onboarding = await readFile(new URL('../components/Onboarding.tsx', import.meta.url), 'utf8')
const layout = await readFile(new URL('../components/layout.tsx', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const settings = await readFile(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const luaShop = await readFile(new URL('../components/LuaShop.tsx', import.meta.url), 'utf8')
const titleBar = await readFile(new URL('../components/CustomTitleBar.tsx', import.meta.url), 'utf8')
const en = await readFile(new URL('../i18n/en-US.ts', import.meta.url), 'utf8')
const vi = await readFile(new URL('../i18n/vi-VN.ts', import.meta.url), 'utf8')

const tabs = [
  "What's New!", 'Home', 'Store', 'Lua Shop', 'Lua Installer', 'Library',
  'Offline Activation', 'Updates', 'Downloads', 'CloudRedirect', 'Translations', 'Cache', 'Settings',
]
for (const tab of tabs) {
  assert.ok(registry.includes(`'${tab}'`) || registry.includes(`\"${tab}\"`), `help registry must cover ${tab}`)
}

for (const [name, source] of [['en-US', en], ['vi-VN', vi]]) {
  assert.ok(/\bhelp\s*:\s*\{/.test(source), `${name} locale must define help copy`)
  assert.ok(/\bonboarding\s*:\s*\{/.test(source), `${name} locale must define onboarding copy`)
}

assert.ok(helpUi.includes('export function HelpButton'), 'shared HelpButton must exist')
assert.ok(helpUi.includes('export function PageHelpButton'), 'page-level help button must exist')
assert.ok(helpUi.includes('export function HelpCenter'), 'replayable Help Center must exist')
assert.ok(onboarding.includes('data-tour') || onboarding.includes('querySelector'), 'onboarding must target real UI elements')
assert.ok(layout.includes('data-tour="sidebar"'), 'sidebar must expose a stable tour target')
assert.ok(!app.includes('<PageHelpButton'), 'page help must not float over workspace content')
assert.ok(titleBar.includes('<PageHelpButton'), 'global page help must live in the title bar')
assert.ok(titleBar.indexOf('<PageHelpButton') > titleBar.indexOf('titlebar-notification-anchor'), 'help button must be grouped beside notifications')
assert.ok(app.includes('<HelpCenter'), 'App must mount the Help Center')
assert.ok(app.includes('onboardingCompleted'), 'App must preserve first-run completion state')
assert.ok(!settings.includes('<HelpButton'), 'settings rows must not render per-row help buttons')
assert.ok(settings.includes('settings-row-title-line') && settings.includes('<span>{description}</span>'), 'settings rows must keep their inline explanatory descriptions')
assert.ok(luaShop.includes('BuildID') && luaShop.includes('<HelpButton'), 'Lua Shop build selector must explain BuildID/version behavior')

console.log('Help/onboarding contract tests passed')
