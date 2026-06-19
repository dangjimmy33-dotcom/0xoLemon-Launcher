import type { GameCatalog, GameInstallMetadata, GameSummary, Snapshot } from '../types'

export const DEFAULT_GAME_ID = '007-first-light'
export const DEFAULT_STORE_ROOT = 'E:\\0xoLemon store'
export const DEFAULT_COMMON_GAME = `${DEFAULT_STORE_ROOT}\\common\\007 First Light`
export const DEFAULT_DOWNLOADING_GAME = `${DEFAULT_STORE_ROOT}\\downloading\\007 First Light`
export const CUSTOM_DOWNLOADING_RELATIVE = '.0xolemon\\downloading'

export const fallbackSnapshot: Snapshot = {
  currentVersion: 'not scanned',
  latestVersion: 'unknown',
  availableVersions: [],
  detectedInstallPath: null,
  updateSize: 0,
  installSize: 0,
  temporarySpace: 0,
  requiredFreeSpace: 0,
  proxyStatus: 'Depot not checked',
  cache: {
    cacheSize: 0,
    cachePath: `${DEFAULT_DOWNLOADING_GAME}\\chunks`,
    freeSpace: 0,
    healthPercent: 0,
    rollbackReady: false,
    rollbackMissingBytes: 0,
  },
  changedFiles: [],
  lastJob: null,
}

export const fallbackInstall: GameInstallMetadata = {
  defaultStoreRoot: DEFAULT_STORE_ROOT,
  defaultInstallFolder: DEFAULT_COMMON_GAME,
  defaultDownloadingFolder: DEFAULT_DOWNLOADING_GAME,
  storageLabel: 'SSD',
  supportsResume: true,
  launchExecutable: 'Retail\\007FirstLight.exe',
}

export function gameFolderName(game: Pick<GameSummary, 'id' | 'title'>) {
  if (game.id === DEFAULT_GAME_ID) return '007 First Light'
  const cleaned = game.title
    .replace(/[<>:"/\\|?*]/g, ' ')
    .split('')
    .filter((char) => char.charCodeAt(0) >= 32)
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
  return cleaned || game.id
}

export function normalizeInstallMetadata(
  game: Pick<GameSummary, 'id' | 'title'> | null | undefined,
  install: GameInstallMetadata = fallbackInstall,
) {
  if (!game) return install
  const folderName = gameFolderName(game)
  const launchExecutable =
    game.id === DEFAULT_GAME_ID
      ? 'Retail\\007FirstLight.exe'
      : install.launchExecutable && install.launchExecutable !== fallbackInstall.launchExecutable
        ? install.launchExecutable
        : `${folderName}.exe`

  return {
    ...install,
    defaultStoreRoot: DEFAULT_STORE_ROOT,
    defaultInstallFolder: `${DEFAULT_STORE_ROOT}\\common\\${folderName}`,
    defaultDownloadingFolder: `${DEFAULT_STORE_ROOT}\\downloading\\${folderName}`,
    launchExecutable,
  }
}

export function installMetadataForStoreRoot(
  game: Pick<GameSummary, 'id' | 'title'> | null | undefined,
  install: GameInstallMetadata = fallbackInstall,
  storeRoot = DEFAULT_STORE_ROOT,
) {
  const normalized = normalizeInstallMetadata(game, install)
  if (!game) return { ...normalized, defaultStoreRoot: storeRoot }
  const folderName = gameFolderName(game)
  const root = storeRoot.trim().replace(/[\\/]+$/, '') || DEFAULT_STORE_ROOT
  return {
    ...normalized,
    defaultStoreRoot: root,
    defaultInstallFolder: `${root}\\common\\${folderName}`,
    defaultDownloadingFolder: `${root}\\downloading\\${folderName}`,
  }
}

export const fallbackCatalog: GameCatalog = {
  defaultLocale: 'en-US',
  games: [
    {
      id: DEFAULT_GAME_ID,
      title: '007 First Light',
      subtitle: 'IO Interactive A/S',
      developer: 'IO Interactive A/S',
      publisher: 'IO Interactive A/S',
      latestVersion: 'v1.2',
      availableVersions: [
        { version: 'v1.0', label: 'Release Patch', buildId: '2338871', sizeBytes: 49_690_000_000, latest: false },
        { version: 'v1.1', label: 'Update 1.1', buildId: '23531465', sizeBytes: 49_690_000_000, latest: false },
        { version: 'v1.2', label: 'Update 1.2', buildId: '23600000', sizeBytes: 49_690_000_000, latest: true },
      ],
      gridAssetId: '',
      heroAssetId: '',
      logoAssetId: '',
      iconAssetId: '',
      install: fallbackInstall,
      cloudSave: { enabled: false, saveRoots: [], include: [], exclude: [] },
      assetPackPath: 'assets/games/007-first-light/core.0xo',
    },
  ],
}
