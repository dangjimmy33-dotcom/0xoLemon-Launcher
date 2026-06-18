declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export type JobStatus =
  | 'planned'
  | 'running'
  | 'paused'
  | 'downloading'
  | 'assembling'
  | 'verified'
  | 'committed'
  | 'canceled'
  | 'failed'

export type StepStatus = 'waiting' | 'running' | 'completed' | 'paused' | 'failed'

export type JobStep = {
  name: string
  detail: string
  status: StepStatus
  progress: number
  retryCount: number
}

export type JobLog = {
  at: string
  level: string
  message: string
}

export type PhaseProgress = {
  name: string
  detail: string
  percent: number
  overallPercent: number
  bytesDone: number
  bytesTotal: number
  rateBytesPerSecond: number
  etaSeconds: number | null
  isDownloading: boolean
}

export type JobJournal = {
  id: string
  gameId: string
  kind: string
  status: JobStatus
  installPath: string
  fromVersion: string
  toVersion: string
  phase: string
  overallProgress: number
  bytesDone: number
  bytesTotal: number
  retryCount: number
  resumable: boolean
  updatedAt: string
  steps: JobStep[]
  logs: JobLog[]
}

export type ChangedFile = {
  path: string
  oldSize: number
  newSize: number
}

export type Snapshot = {
  currentVersion: string
  latestVersion: string
  availableVersions: string[]
  detectedInstallPath: string | null
  updateSize: number
  proxyStatus: string
  cache: {
    cacheSize: number
    cachePath: string
    freeSpace: number
    healthPercent: number
    rollbackReady: boolean
    rollbackMissingBytes: number
  }
  changedFiles: ChangedFile[]
  lastJob: JobJournal | null
}

export type GameCatalog = {
  defaultLocale: string
  games: GameSummary[]
}

export type GameSummary = {
  id: string
  title: string
  subtitle: string
  developer: string
  publisher: string
  latestVersion: string
  availableVersions: GameVersionInfo[]
  gridAssetId: string
  heroAssetId: string
  logoAssetId: string
  iconAssetId: string
  install: GameInstallMetadata
  assetPackPath: string
}

export type GameVersionInfo = {
  version: string
  label: string
  buildId: string
  sizeBytes: number
  latest: boolean
}

export type GameInstallMetadata = {
  defaultStoreRoot: string
  defaultInstallFolder: string
  defaultDownloadingFolder: string
  storageLabel: string
  supportsResume: boolean
  launchExecutable: string
}

export type GameDetail = {
  gameId: string
  locale: string
  title: string
  shortDescription: string
  detailedDescription: string
  developers: string[]
  publishers: string[]
  releaseDate: string
  genres: string[]
  categories: string[]
  ratings: GameRating[]
  media: GameMedia[]
  achievements: GameAchievement[]
  sounds: GameSound[]
  install: GameInstallMetadata
  descriptionImages: string[]
  versions: GameVersionInfo[]
  metadataSource: string
}

export type GameRating = {
  source: string
  score: string
}

export type GameMedia = {
  id: string
  role: string
  title: string
  mimeType: string
  assetId: string
}

export type LauncherUpdateInfo = {
  version: string
  notes: string
  publishedAt: string
}

export type LauncherUpdateProgress = {
  phase: 'downloading' | 'installing' | string
  downloadedBytes: number
  totalBytes: number | null
}

export type SteamEnvironmentInfo = {
  installed: boolean
  running: boolean
  rootPath: string | null
  uiLanguage: string | null
  activeAccountId: string | null
  libraryPaths: string[]
  shortcutsPath: string | null
  spacewarInstalled: boolean
  pendingShortcutActions: number
}

export type GameAchievement = {
  id: string
  name: string
  description: string
  iconAssetId: string
  hidden: boolean
}

export type GameSound = {
  id: string
  role: string
  mimeType: string
  assetId: string
}

export type AssetBlob = {
  mimeType: string
  dataBase64: string
}

export type GameInstallState = {
  gameId: string
  installed: boolean
  currentVersion: string
  installPath: string
  launchExecutable: string
}

export type VerifyInstallReport = {
  ok: boolean
  checkedFiles: number
  missingFiles: string[]
  mismatchedFiles: string[]
}

export type VerifyUiStatus = {
  gameId: string
  state: 'running' | 'ok' | 'failed'
  message: string
  percent: number
  currentFile?: string | null
  checkedFiles?: number
  totalFiles?: number
  checkedBytes?: number
  totalBytes?: number
  missingFiles?: string[]
  mismatchedFiles?: string[]
}

export type VerifyProgressPayload = {
  gameId: string
  phase: string
  currentFile: string | null
  checkedFiles: number
  totalFiles: number
  checkedBytes: number
  totalBytes: number
  percent: number
}

export type UninstallReport = {
  gameId: string
  removedFiles: number
  removedDirs: number
  removedShortcuts: number
  steamShortcutRemoved: boolean
  installPath: string
}

export type ResolvedGameLaunchConfig = {
  schemaVersion: number
  gameId: string
  pickerMode: 'auto' | 'always' | 'never' | string
  defaultOptionId: string
  source: string
  options: ResolvedGameLaunchOption[]
}

export type ResolvedGameLaunchOption = {
  id: string
  title: string
  description: string
  recommended: boolean
  available: boolean
  unavailableReason: string | null
}

export type LaunchReport = {
  gameId: string
  executable: string
  shortcutPath: string | null
  dependenciesInstalled: string[]
  launchOptionId: string
  launchOptionTitle: string
  launchedProcesses: string[]
}

export type LaunchSplashState = {
  title: string
  heroUrl?: string
  iconUrl?: string
}

export type ShortcutLaunchPayload = {
  gameId: string
  installPath: string
  launchExecutable?: string | null
}

export type TabId = 'Library' | 'Updates' | 'Downloads' | 'Cache' | 'Settings'

export {}
