import assert from 'node:assert/strict'
import test from 'node:test'
import { collectOwnedGameIds, filterCatalogByOwnedGameIds } from './libraryOwnership.ts'

const catalog = {
  games: [
    { id: 'explicit', title: 'Explicit library game' },
    { id: 'local', title: 'Local install' },
    { id: 'steam', title: 'Steam install' },
    { id: 'store-only', title: 'Store only' },
  ],
}

test('explicit additions remain owned before installation', () => {
  const owned = collectOwnedGameIds({
    catalog,
    explicitLibraryGameIds: ['explicit', 'explicit'],
    installStates: {},
    steamMapping: {},
    steamInstalledAppIds: [],
  })

  assert.deepEqual([...owned], ['explicit'])
  assert.deepEqual(filterCatalogByOwnedGameIds(catalog, owned).games.map((game) => game.id), ['explicit'])
})

test('ownership is the union of explicit, launcher-installed, and Steam-installed games', () => {
  const owned = collectOwnedGameIds({
    catalog,
    explicitLibraryGameIds: ['explicit'],
    installStates: { local: { installed: true } },
    steamMapping: { steam: 730 },
    steamInstalledAppIds: [730],
  })

  assert.deepEqual([...owned].sort(), ['explicit', 'local', 'steam'])
  assert.deepEqual(filterCatalogByOwnedGameIds(catalog, owned).games.map((game) => game.id), ['explicit', 'local', 'steam'])
})
