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
  logicalBytesDone: number
  logicalBytesTotal: number
  sessionBytesDone: number
  sessionBytesTotal: number
  sessionBaseBytes: number
  remainingBytes: number
  rateBytesPerSecond: number
  applyRateBytesPerSecond: number
  etaSeconds: number | null
  applyEtaSeconds: number | null
  networkPercent: number
  applyPercent: number
  applyBytesDone: number
  applyBytesTotal: number
  durableBytes: number
  currentFile: string
  pipelineVersion: string
  commitState: string
  isCommitting: boolean
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
  /** Monotonic user-facing transfer progress across resume/replanning. */
  logicalBytesDone?: number
  logicalBytesTotal?: number
  /** Bytes already available before the current remaining-work session. */
  sessionBaseBytes?: number
  /** Bytes durably written into verified staging. */
  applyBytesDone?: number
  applyBytesTotal?: number
  durableBytes?: number
  currentFile?: string
  pipelineVersion?: string
  commitState?: string
  plannedFiles?: string[]
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
    diskReadBytes?: number
    diskWriteBytes?: number
    resumeRehashBytes?: number
    syncWaitMs?: number
    commitWaitMs?: number
    allocationReservedBytes?: number
    allocationFallbackReason?: string
  }
  /** Set when a patch job commits – lets the UI immediately clear the pending-patch badge. */
  appliedPatchId?: string
}

export type ChangedFile = {
  path: string
  oldSize: number
  newSize: number
}

export type Snapshot = {
  gameId?: string | null
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
  appliedPatchId?: string
}

export type DownloadProfile = 'eco' | 'balanced' | 'turbo'
export type GameUpdateMode = 'automatic' | 'scheduled' | 'manual'
export type GameTurboPreference = 'always' | 'never' | 'ask'

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
  /** HuggingFace dataset repo ID hosting depot manifests and keys. Format: "owner/repo-name" */
  depotHfRepoId: string
  gameTurbo: GameTurboPreference
}

export type CloudSaveMetadata = {
  enabled: boolean
  saveRoots: string[]
  include: string[]
  exclude: string[]
}

export type CloudSaveRoot = {
  id?: string
  path: string
  label: string
  purpose?: 'save' | 'profile' | 'progress' | 'settings-portable' | string
  include?: string[]
  exclude?: string[]
  fingerprint?: string
  legacy?: boolean
  legacyExpiresAt?: string | null
}

export type CloudSaveConflict = {
  id: string
  createdAt: string
  localFileCount: number
  cloudFileCount: number
  localBytes: number
  cloudBytes: number
  recommended?: 'local' | 'cloud' | string
  localDevice?: string
  cloudDevice?: string
  recommendationReason?: string
  recommendationConfidence?: 'low' | 'medium' | 'high' | string
  localLatestWriteAtMs?: number
  cloudLatestWriteAtMs?: number
}

export type CloudSaveSnapshot = {
  id: string
  createdAt: string
  source: string
  fileCount: number
  bytes: number
  pinned?: boolean
  snapshotClass?: 'automatic' | 'conflict' | 'manual' | string
}

export type CloudSaveQuota = {
  limitBytes: number | null
  usageBytes: number
  availableBytes: number | null
  checkedAt: string
  state: 'healthy' | 'low' | 'full' | string
}

export type CloudSaveMapStatus = {
  version: string
  source: string
  healthy: boolean
  message: string
  warnings: string[]
}

export type CloudSaveStatus = {
  gameId: string
  enabled: boolean
  automaticProtection: boolean
  syncRoot: string
  saveRoots: CloudSaveRoot[]
  include: string[]
  exclude: string[]
  state:
    | 'disabled'
    | 'ready'
    | 'synced'
    | 'syncing'
    | 'offline'
    | 'rate_limited'
    | 'storage_full'
    | 'auth_required'
    | 'waiting_for_first_save'
    | 'waiting_for_save'
    | 'conflict_check_required'
    | 'permission_denied'
    | 'remote_damaged'
    | 'conflict'
    | string
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
  pendingOperationCount: number
  pendingUploadBytes: number
  quota: CloudSaveQuota | null
  mapStatus: CloudSaveMapStatus
  remoteNewerKnown: boolean
}


export type CloudRedirectStatus = {
  steamPath: string | null
  steamVersion: number | null
  steamVersionSupported: boolean
  steamRunning: boolean
  coreDllPresent: boolean
  cloudRedirectDllPresent: boolean
  stfixerApplied: boolean
  supportedVersions: number[]
}

export type StfixerResult = {
  succeeded: boolean
  log: string[]
  error?: string
}

export type CloudProviderConfig = {
  provider: string       // "gdrive" | "onedrive" | "folder" | ""
  tokenPath: string      // path to token/sync folder, empty if not set
  authenticated: boolean // token file exists and is valid
  configFound: boolean   // config.json was found at all
}

export type GameCatalog = {
  defaultLocale: string
  games: GameSummary[]
  newestGameIds?: string[]
}

export type GameSummary = {
  id: string
  appid?: string | number
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
  tags?: string[]
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
  appid?: string | number
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
  /** Optional low-resolution asset for thumbnail rails. Existing media keeps assetId. */
  thumbnailAssetId?: string
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

export type RestartSteamReport = {
  wasRunning: boolean
  forced: boolean
  running: boolean
  message: string
}

export type LuaGameChannel = 'live' | 'locked'

export type LuaSyncStatus =
  | 'idle'
  | 'checking'
  | 'upToDate'
  | 'updated'
  | 'updateAvailable'
  | 'conflict'
  | 'error'

export type LuaMigrationState = 'managed' | 'reviewRequired'
export type LuaRuntimeState = 'active' | 'missing' | 'conflict' | 'unknown'
export type LuaRemoteSourceState = 'available' | 'unavailable' | 'updateAvailable' | 'error' | 'unknown'

export type LuaGameState = {
  appid: number
  gameName: string
  channel: LuaGameChannel
  pinnedBuildId: string | null
  sourceRevision: string | null
  lastSyncAt: string | null
  nextSyncAt: string | null
  lastError: string | null
  syncStatus: LuaSyncStatus
  migrationState: LuaMigrationState
  requiresSteamRestart: boolean
  sharedDepotConflicts: number[]
  runtimeState: LuaRuntimeState
  sourceState: LuaRemoteSourceState
  sourceErrorCode: string | null
  sourceProvider: LuaPackageProvider | null
  selectedSource: LuaSourceProvider | null
  selectedVariant: LuaPackageProvider | null
  installedRevision: string | null
  installedModifiedAt: string | null
  availableRevision: string | null
  availableModifiedAt: string | null
  lastCheckedAt: string | null
  updateAvailable: boolean
}

export type LuaGameManagerState = {
  game: LuaGameState
  luaPath: string
  fileExists: boolean
  hasUserOverrides: boolean
  canSwitchLive: boolean
}

export type HubcapUsageBucket = {
  usage: number | null
  limit: number | null
  remaining: number | null
}

export type HubcapKeyState = {
  configured: boolean
  valid: boolean
  maskedKey: string | null
  expiresAt: string | null
  expiryEstimated: boolean
  expiringSoon: boolean
  expired: boolean
  serviceReady: boolean
  daily: HubcapUsageBucket
  single: HubcapUsageBucket
  bundle: HubcapUsageBucket
  workshop: HubcapUsageBucket
  lastCheckedAt: string | null
  lastError: string | null
}

export type LuaSourceSettingsState = {
  hubcap: HubcapKeyState
  sushiEnabled: boolean
  ryuuEnabled: boolean
}

export type LuaPackageProvider = 'curated' | 'community' | 'hubcap' | 'sushi' | 'ryuu' | 'none'
export type LuaSourceProvider = 'huggingFace' | 'hubcap' | 'sushi' | 'ryuu'
export type LuaSourceOperation = 'add' | 'update' | 'sync'

export type LuaSourceCandidate = {
  provider: LuaSourceProvider
  available: boolean
  enabled: boolean
  onDemand: boolean
  requiresKey: boolean
  keyReady: boolean
  recommended: boolean
  variant: LuaPackageProvider | null
  revision: string | null
  modifiedAt: string | null
  errorCode: string | null
}

export type LuaSourceScanResult = {
  appid: number
  operation: LuaSourceOperation
  sources: LuaSourceCandidate[]
}

export type LuaSourceAvailability = {
  appid: number
  curatedAvailable: boolean
  communityAvailable: boolean
  hubcapAvailable: boolean
  sushiAvailable: boolean
  ryuuAvailable: boolean
  preferredProvider: LuaPackageProvider
  revision: string | null
  sourceModifiedAt: string | null
  errorCode: string | null
}

export type LuaCatalogItem = {
  appid: number
  name: string
  headerImage: string
  installed: boolean
  availability: LuaSourceAvailability
}

export type LuaCatalogSearchPage = {
  items: LuaCatalogItem[]
  nextCursor: string | null
  totalEstimate: number | null
}

export type LuaAddQuotaState = {
  limit: number
  used: number
  remaining: number
  resetAt: string | null
  serverTime: string | null
  timezone: string | null
  available: boolean
  lastError: string | null
}

export type NativeCoreSettings = {
  statsApiEnabled: boolean
  configExists: boolean
}

export type DiscordAuthState =
  | 'checking'
  | 'notConfigured'
  | 'signedOut'
  | 'authorized'
  | 'notMember'
  | 'noRole'
  | 'accountTooNew'
  | 'expired'
  | 'error'
  | 'networkError'

export type DiscordAuthUser = {
  id: string
  username: string
  displayName: string
  avatarUrl: string
  accountCreatedAt: string
  accountAgeDays: number
}

export type DiscordAuthStatus = {
  state: DiscordAuthState
  configured: boolean
  message: string
  user: DiscordAuthUser | null
  guildId: string
  guildName: string | null
  guildInvite: string
  eligibleAt: string | null
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
  appliedPatchId?: string
  discoveryStatus?: 'recovering' | 'registered' | 'recovered' | 'conflict' | 'unavailable' | 'notFound' | string
  candidatePaths?: string[]
  libraryId?: string
  unavailableReason?: string
}

export type LibraryRecoveryIndex = {
  schemaVersion: number
  libraryId: string
  createdAt: string
}

export type DiscoveredInstall = {
  gameId: string
  installPath: string
  version: string
  launchExecutable: string
  appliedPatchId?: string
  libraryId?: string
}

export type InstallDiscoveryConflict = {
  gameId: string
  candidatePaths: string[]
}

export type InstallDiscoveryReport = {
  recovered: DiscoveredInstall[]
  conflicts: InstallDiscoveryConflict[]
  rootsScanned: string[]
  unavailableRoots: string[]
  invalidCandidates: number
  requiresLocateLibrary: boolean
  durationMs: number
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
  | 'What\'s New!'
  | 'Store'
  | 'Library'
  | 'Offline Activation'
  | 'Updates'
  | 'Downloads'
  | 'CloudRedirect'
  | 'Lua Installer'
  | 'Lua Shop'
  | 'Translations'
  | 'Cache'
  | 'Settings'

export { }
