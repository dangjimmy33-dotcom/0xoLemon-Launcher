import type { GameCatalog, GameInstallState } from '../types'

export type LibraryOwnershipInput = {
  catalog: GameCatalog
  explicitLibraryGameIds: readonly string[]
  installStates: Readonly<Record<string, GameInstallState>>
  steamMapping: Readonly<Record<string, number>>
  steamInstalledAppIds: readonly number[]
}

export function collectOwnedGameIds({
  catalog,
  explicitLibraryGameIds,
  installStates,
  steamMapping,
  steamInstalledAppIds,
}: LibraryOwnershipInput): Set<string> {
  const owned = new Set(explicitLibraryGameIds)
  const installedSteamApps = new Set(steamInstalledAppIds)

  for (const game of catalog.games) {
    if (installStates[game.id]?.installed) {
      owned.add(game.id)
      continue
    }

    const appId = steamMapping[game.id]
    if (appId && installedSteamApps.has(appId)) owned.add(game.id)
  }

  return owned
}

export function filterCatalogByOwnedGameIds(catalog: GameCatalog, ownedGameIds: ReadonlySet<string>): GameCatalog {
  return {
    ...catalog,
    games: catalog.games.filter((game) => ownedGameIds.has(game.id)),
  }
}
