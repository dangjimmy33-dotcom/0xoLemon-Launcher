import fs from 'node:fs'
import assert from 'node:assert/strict'

const appCss = fs.readFileSync(new URL('../App.css', import.meta.url), 'utf8')
const premiumCss = fs.readFileSync(new URL('../premium.css', import.meta.url), 'utf8')
const offline = fs.readFileSync(new URL('../components/OfflineActivation.tsx', import.meta.url), 'utf8')
const luaShop = fs.readFileSync(new URL('../components/LuaShop.tsx', import.meta.url), 'utf8')
const luaCss = fs.readFileSync(new URL('../components/LuaShop.css', import.meta.url), 'utf8')

assert.match(appCss, /\.workspace\s*\{[\s\S]*background:\s*var\(--launcher-page-bg/, 'workspace must use semantic themed background')
assert.match(premiumCss, /\.premium-workspace\s*\{[\s\S]*backdrop-filter:/, 'premium workspace must blur ambient backdrop')
assert.match(appCss, /ambient-drift/, 'ambient animation must remain present')

assert.match(offline, /onClick=\{\(\) => handleSelectGame\(game/, 'offline card must be clickable')
assert.match(offline, /get_game_install_state/, 'offline detail must resolve launcher install state')
assert.match(offline, /DenuvoActivationButton/, 'offline detail must expose existing activation control')

assert.match(luaShop, /build-radio-indicator/, 'build dropdown must use radio indicator')
assert.doesNotMatch(luaShop, /M13\.485 1\.431/, 'legacy giant checkmark icon must be removed from build options')
assert.match(luaCss, /\.build-radio-indicator/, 'radio indicator styles must exist')
assert.match(luaCss, /\.version-status-badge/, 'compact version status badges must exist')

console.log('polish fixes contract PASS')
