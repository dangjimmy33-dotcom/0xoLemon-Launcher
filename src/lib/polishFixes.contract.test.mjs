import fs from 'node:fs'
import assert from 'node:assert/strict'

const appCss = fs.readFileSync(new URL('../App.css', import.meta.url), 'utf8')
const premiumCss = fs.readFileSync(new URL('../premium.css', import.meta.url), 'utf8')
const offline = fs.readFileSync(new URL('../components/OfflineActivation.tsx', import.meta.url), 'utf8')
const luaShop = fs.readFileSync(new URL('../components/LuaShop.tsx', import.meta.url), 'utf8')
const luaManager = fs.readFileSync(new URL('../components/LuaGameManagerDialog.tsx', import.meta.url), 'utf8')
const luaCss = fs.readFileSync(new URL('../components/LuaShop.css', import.meta.url), 'utf8')

assert.match(appCss, /\.workspace\s*\{[\s\S]*background:\s*var\(--launcher-page-bg/, 'workspace must use semantic themed background')
assert.match(premiumCss, /\.premium-workspace\s*\{[\s\S]*backdrop-filter:/, 'premium workspace must blur ambient backdrop')
assert.match(appCss, /ambient-drift/, 'ambient animation must remain present')

assert.match(offline, /onClick=\{\(\) => handleSelectGame\(game/, 'offline card must be clickable')
assert.match(offline, /get_game_install_state/, 'offline detail must resolve launcher install state')
assert.match(offline, /DenuvoActivationButton/, 'offline detail must expose existing activation control')

assert.match(luaShop, /LuaGameManagerDialog/, 'version management must live behind the installed-game settings action')
assert.match(luaManager, /role="listbox"/, 'build picker must use an accessible custom listbox')
assert.match(luaManager, /lua-manager-build-check/, 'build picker must reserve a stable selection indicator')
assert.doesNotMatch(luaManager, /<select/, 'native select dropdown must not be used for BuildID selection')
assert.match(luaCss, /\.lua-manager-build-check/, 'custom build selection indicator styles must exist')

console.log('polish fixes contract PASS')
