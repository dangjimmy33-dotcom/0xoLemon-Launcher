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

console.log('UI contract tests passed')
