import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const downloads = await readFile(new URL('./downloads.tsx', import.meta.url), 'utf8')
const translations = await readFile(new URL('./TranslationsView.tsx', import.meta.url), 'utf8')
const activeView = await readFile(new URL('./ActiveView.tsx', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const dock = await readFile(new URL('./TransferDock.tsx', import.meta.url), 'utf8')
const luaShop = await readFile(new URL('./LuaShop.tsx', import.meta.url), 'utf8')
const luaShopCss = await readFile(new URL('./LuaShop.css', import.meta.url), 'utf8')
const luaManager = await readFile(new URL('./LuaGameManagerDialog.tsx', import.meta.url), 'utf8')
const preferences = await readFile(new URL('../lib/preferences.ts', import.meta.url), 'utf8')
const settingsView = await readFile(new URL('./SettingsView.tsx', import.meta.url), 'utf8')
const picker = await readFile(new URL('./LuaSourcePickerDialog.tsx', import.meta.url), 'utf8')
const theme = await readFile(new URL('../lib/theme.ts', import.meta.url), 'utf8').catch(() => '')

for (const label of ['Already available', 'This session', 'Remaining']) {
  assert.ok(downloads.includes(label), `downloads view must include ${label}`)
}
assert.ok(downloads.includes('aria-valuenow'), 'downloads progress must expose determinate accessibility values')
assert.ok(downloads.includes('details className="job-log-disclosure"'), 'job log must be progressively disclosed')

assert.ok(dock.includes('transfer-dock'), 'floating transfer dock component must exist')
assert.ok(dock.includes('Open transfer details'), 'dock must expose an accessible navigation action')
assert.ok(app.includes('<TransferDock'), 'App must mount the global transfer dock')

assert.ok(translations.includes('Translation catalog'), 'translations must have a catalog mode')
assert.ok(translations.includes('Search games or translations'), 'translations must support search')
assert.ok(translations.includes('translation-catalog-card'), 'translations must render catalog cards')
assert.ok(translations.includes('Available archives'), 'translation detail must list archives')
assert.ok(!translations.includes("disabled={state?.status === 'ready' && !available && !state.installed}"), 'unavailable games must still open their detail page')
assert.ok(activeView.includes('catalog={catalog}'), 'ActiveView must pass the whole catalog to TranslationsView')

assert.ok(!luaShop.includes("filterTab === 'verified'"), 'Lua Shop must not restore the legacy Verified tab')
assert.ok(!luaShop.includes('lua-migration-review'), 'legacy Lua review must live in per-game management, not above the shop')
assert.ok(!luaShop.includes('verified-detail-overlay'), 'version management must not render a second shop-level overlay')
assert.ok(luaShop.includes('className="lua-shop-results" ref={gridRef}'), 'the result list and pagination must share one scroll owner')
assert.ok(luaShopCss.includes('.lua-shop-card-actions { grid-row: 5; }'), 'Lua card actions must remain in a stable footer row')
assert.ok(luaManager.includes('role="listbox"'), 'the Lua manager must use the styled build picker')
assert.ok(!luaManager.includes('<select'), 'the native BuildID select must not return')



assert.ok(luaShop.includes('Ctrl K'), 'Lua Shop search must advertise Ctrl K')
assert.ok(luaShop.includes("event.key.toLowerCase() === 'k'"), 'Lua Shop must support Ctrl/Cmd+K focus')
assert.ok(luaShop.includes('luaShopSort'), 'Lua Shop must persist independent sort state')
assert.ok(luaShop.includes('luaShopGridCols'), 'Lua Shop must persist grid density')
assert.ok(luaShop.includes('lua-shop-transition-overlay'), 'Lua Shop must show a loading transition when query state changes')
assert.ok(luaShop.includes("'grid' | 'list'"), 'Lua Shop must support grid and list layouts')
assert.ok(preferences.includes('accentHue'), 'launcher preferences must persist accent hue')
assert.ok(preferences.includes('accentChroma'), 'launcher preferences must persist color-wheel chroma')
assert.ok(settingsView.includes('AccentTonePicker'), 'Settings must expose the custom accent picker')
assert.ok(settingsView.includes('accent-color-wheel'), 'Settings must expose a circular color wheel')
assert.ok(settingsView.includes('onPointerDown'), 'color wheel must support pointer dragging')
assert.ok(settingsView.includes('Default'), 'color wheel must expose a Default reset action')
assert.ok(settingsView.includes('accent-color-hex'), 'color wheel must show a HEX preview')
assert.ok(theme.includes('applyLauncherTheme'), 'launcher must derive and apply theme CSS variables centrally')
assert.ok(theme.includes('oklchToHex'), 'theme helper must expose HEX conversion for the picker')
assert.ok(theme.includes('--launcher-sidebar-bg'), 'theme helper must tint launcher chrome, not only buttons')


const sourceMeta = await readFile(new URL('../lib/luaSourcesMeta.ts', import.meta.url), 'utf8')
const types = await readFile(new URL('../types.ts', import.meta.url), 'utf8')
assert.ok(types.includes("'luie'"), 'Lua source types must include Luie')
assert.ok(types.includes("'twentyTwoCloud'"), 'Lua source types must include TwentyTwo Cloud')
assert.ok(types.includes("'skyflare'"), 'Lua source types must include Skyflare')
assert.ok(sourceMeta.includes("displayName: 'Luie'"), 'source metadata must describe Luie')
assert.ok(sourceMeta.includes("displayName: 'DepotBox'"), 'source metadata must describe DepotBox')
assert.ok(sourceMeta.includes("displayName: 'Skyflare'"), 'source metadata must describe Skyflare')
assert.match(sourceMeta, /twentyTwoCloud:\s*\{[\s\S]*?displayName:\s*'DepotBox'[\s\S]*?sourceType:\s*'hybrid'/, 'DepotBox must expose LIVE & LOCKED')
assert.match(sourceMeta, /skyflare:\s*\{[\s\S]*?sourceType:\s*'live'/, 'Skyflare must be LIVE-only')
assert.ok(!picker.includes("AUTO · LIVE / LOCKED"), 'hybrid source pill should say LIVE & LOCKED, not AUTO')
assert.ok(sourceMeta.includes("sourceType: 'live'"), 'source metadata must expose Live classification')
assert.ok(sourceMeta.includes("sourceType: 'hybrid'"), 'dynamic ZIP/Lua sources must expose adaptive classification')


assert.ok(sourceMeta.includes("displayName: 'Ryuu'"), 'source metadata must describe official Ryuu')
assert.match(sourceMeta, /ryuu:\s*\{[\s\S]*?sourceType:\s*'hybrid'/, 'official Ryuu must expose LIVE & LOCKED')

assert.ok(picker.includes('useState<LuaGameChannel | null>'), 'source picker must keep an explicit nullable selected channel')
assert.ok(picker.includes("useState<'source' | 'channel'>('source')"), 'source picker must use a two-step source/channel flow')
assert.ok(picker.includes('lua-source-channel-step'), 'hybrid sources must show channel selection in a separate step')
assert.ok(!picker.includes("selectedMeta.sourceType === 'hybrid' && ("), 'channel choice must not render inline under source cards')
assert.ok(picker.includes('onConfirm(selected, statSteamId.trim() || null, selectedChannel)'), 'picker must return the chosen channel')
assert.ok(luaShop.includes('channel,'), 'Lua Shop source confirmation must receive the selected channel')
assert.ok(!luaShop.includes("channel: 'live',\n          buildId: null,\n          accessToken: null,\n          statSteamId: null,\n          conflictResolution: null,\n          provider,"), 'Lua Shop add must not hard-code Live after source selection')


assert.ok(types.includes('depotboxConfigured'), 'source settings state must expose optional DepotBox API configuration')
assert.ok(settingsView.includes('save_depotbox_api_key'), 'Settings must save a DepotBox API key through Tauri')
assert.ok(settingsView.includes('clear_depotbox_api_key'), 'Settings must clear a DepotBox API key through Tauri')
assert.ok(settingsView.includes('depotboxApiKey'), 'Settings must expose the DepotBox API panel')
assert.ok(sourceMeta.includes('Free Web'), 'DepotBox metadata must describe free web mode')
assert.ok(sourceMeta.includes('Direct API'), 'DepotBox metadata must describe optional direct API mode')

console.log('UI contract tests passed')
