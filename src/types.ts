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
  metrics?: {
    pipeline: string
    payloadBytes: number
    networkBytes: number
    overfetchBytes: number
    retryWaitMs: number
    rateLimitWaitMs: number
    peakInFlightBytes: number
    throughputP50BytesPerSecond: number
    throughputP95BytesPerSecond: number
  }
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
  installSize: number
  temporarySpace: number
  requiredFreeSpace: number
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

export type DownloadProfile = 'eco' | 'balanced' | 'turbo'
export type GameUpdateMode = 'automatic' | 'scheduled' | 'manual'

export type LauncherSettings = {
  defaultLibrary: string
  downloadWorkers: number
  downloadRetries: number
  packRangeMb: number
  keepChunkCache: boolean
  notificationsEnabled: boolean
  autoVerifyAfterInstall: boolean
  downloadProfile: DownloadProfile
  downloadQueueMb: number
  directToStaging: boolean
  cloudSaveRoot: string
  gameUpdateMode: GameUpdateMode
  gameUpdateScheduleStart: string
  gameUpdateScheduleEnd: string
}

export type CloudSaveMetadata = {
  enabled: boolean
  saveRoots: string[]
  include: string[]
  exclude: string[]
}

export type CloudSaveRoot = {
  path: string
  label: string
}

export type CloudSaveConflict = {
  id: string
  createdAt: string
  localFileCount: number
  cloudFileCount: number
  localBytes: number
  cloudBytes: number
}

export type CloudSaveSnapshot = {
  id: string
  createdAt: string
  source: string
  fileCount: number
  bytes: number
}

export type CloudSaveStatus = {
  gameId: string
  enabled: boolean
  syncRoot: string
  saveRoots: CloudSaveRoot[]
  include: string[]
  exclude: string[]
  state: 'disabled' | 'ready' | 'conflict' | string
  lastSyncAt: string | null
  lastMessage: string
  conflicts: CloudSaveConflict[]
  snapshots: CloudSaveSnapshot[]
  canSync: boolean
  gameRunning: boolean
  googleDriveConfigured: boolean
  googleDriveConnected: boolean
  googleDriveLastBackupAt: string | null
  googleDriveLastRestoreCount: number
  googleDriveMessage: string
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
  cloudSave: CloudSaveMetadata
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
  cloudSave: CloudSaveMetadata
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
  version: string
  phase: 'checking' | 'downloading' | 'verifying' | 'installing' | 'restarting' | 'failed' | string
  downloadedBytes: number
  totalBytes: number | null
  timestamp: string
  error: string | null
}

export type GameRuntimeState = {
  gameId: string
  running: boolean
  pid: number | null
  totalPlaytimeSeconds: number
  currentSessionStartedAt: string | null
  lastPlayedAt: string | null
  launchCount: number
}

export type NotificationCategory =
  | 'launcher'
  | 'installs'
  | 'downloads'
  | 'cloudSaves'
  | 'storage'
  | 'achievements'
  | 'errors'

export type NotificationSeverity = 'info' | 'success' | 'warning' | 'error'

export type NotificationAction = {
  kind: string
  tab: TabId | null
  gameId: string | null
}

export type NotificationRecord = {
  id: string
  category: NotificationCategory
  severity: NotificationSeverity
  title: string
  message: string
  timestamp: string
  read: boolean
  dedupeKey: string
  entity: { kind: string; id: string } | null
  action: NotificationAction | null
}

export type NewNotification = Omit<NotificationRecord, 'id' | 'timestamp' | 'read'>

export type PushNotificationResult = {
  record: NotificationRecord
  inserted: boolean
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

export type ClearCacheReport = {
  removedFiles: number
  removedBytes: number
  cachePath: string
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

export type TabId =
  | 'Home'
  | 'Store'
  | 'Library'
  | 'Updates'
  | 'Downloads'
  | 'Cloud Saves'
  | 'Cache'
  | 'Settings'

export {}
