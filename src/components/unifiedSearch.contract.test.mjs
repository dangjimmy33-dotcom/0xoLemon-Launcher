import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'

const read = (name) => fs.readFileSync(new URL(name, import.meta.url), 'utf8')
const unified = read('./UnifiedSearchOverlay.tsx')
const luaShop = read('./LuaShop.tsx')
const luaInstaller = read('./LuaInstaller.tsx')
const depot = read('./DepotDownloaderView.tsx')
const gse = read('./GseUcStandaloneView.tsx')
const layout = read('./layout.tsx')
const translations = read('./TranslationsView.tsx')
const library = read('./library.tsx')

const expectUnified = (source, label) => {
  assert.ok(source.includes('UnifiedSearchOverlay'), `${label} must use the shared Store-style fullscreen search shell`)
}

test('shared search shell is visually identical to Store search primitives', () => {
  for (const token of [
    'store-search-overlay',
    'store-search-backdrop',
    'store-search-surface',
    'store-search-command',
    'store-search-filters',
    'store-search-results',
    'store-search-result',
    'store-search-overlay-open',
  ]) {
    assert.ok(unified.includes(token), `Unified search shell must reuse ${token}`)
  }
  assert.ok(unified.includes("event.key === 'Escape'"), 'unified overlay must close on Escape')
  assert.ok(unified.includes('Ctrl K'), 'unified overlay must advertise Ctrl K')
})

test('all primary game searches use the shared fullscreen shell', () => {
  expectUnified(luaShop, 'Lua Shop')
  expectUnified(luaInstaller, 'Lua Installer')
  expectUnified(depot, 'Depot Downloader')
  expectUnified(gse, 'GSE Save Manager')
  expectUnified(layout, 'Updates/catalog empty state')

  // These predate the shared component but must keep the exact same Store shell.
  assert.ok(library.includes('store-search-overlay'), 'Store/Library search must keep Store fullscreen shell')
  assert.ok(translations.includes('store-search-overlay'), 'Translations search must keep Store fullscreen shell')
})

test('Ctrl/Cmd+K opens overlays instead of only focusing compact inputs', () => {
  for (const [source, label] of [
    [luaShop, 'Lua Shop'],
    [luaInstaller, 'Lua Installer'],
    [depot, 'Depot Downloader'],
    [gse, 'GSE Save Manager'],
  ]) {
    assert.match(source, /set[A-Za-z]*SearchOverlayOpen\(true\)/, `${label} must open a fullscreen overlay`) 
  }
})
