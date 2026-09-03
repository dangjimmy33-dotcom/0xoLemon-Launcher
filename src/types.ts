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
  wireBytesDone?: number
  currentFile?: string
  pipelineVersion?: string
  transportPlans?: PackTransportPlan[]
  currentTransport?: DownloadTransportKind
  stallReason?: string
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
    xetBytes?: number
    rawRangeBytes?: number
    wireBytes?: number
    decodeWaitMs?: number
    writerWaitMs?: number
    checkpointWaitMs?: number
    ttfbP50Ms?: number
    ttfbP95Ms?: number
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

export type DownloadProfile = 'eco' | 'balanced' | 'auto' | 'turbo'
export type DownloadTransportKind = 'xetPack' | 'httpRange'

export type DownloadTelemetry = {
  jobId: string
  wireBytesDone: number
  applyBytesDone: number
  durableBytesDone: number
  wireBytesPerSecond: number
  applyBytesPerSecond: number
  activeConnections: number
  queueBytes: number
  ttfbMs: number
  retryWaitMs: number
  rateLimitWaitMs: number
  currentTransport: DownloadTransportKind
  stallReason: string
}

export type PackTransportPlan = {
  packId: string
  sourceIdentity: string
  requiredBytes: number
  totalPackBytes: number
  selectedTransport: DownloadTransportKind
  estimatedOverfetch: number
}
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
  localWinsOnceState: 'prepared' | 'running' | 'conflict' | null
  localWinsOnceSnapshotId: string | null
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

export type SteamRuntimeMode = 'none' | 'managedGse'

export type SaveProviderKind = 'gse' | 'goldbergSteamEmu' | 'goldbergUplayEmu' | 'legacyVersioned'

export type SaveProvider = {
  provider: SaveProviderKind
  saveId: string
}

export type LocalRuntimeIntegration = {
  steamRuntime: SteamRuntimeMode
  achievementsEnabled: boolean
  saveProviders: SaveProvider[]
}

export type GameSummary = LocalRuntimeIntegration & {
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

export type GameDetail = LocalRuntimeIntegration & {
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
  activeSha256: string | null
  hasUserOverrides: boolean
  canSwitchLive: boolean
  canSwitchLocked: boolean
}

export type LuaDriftResolution =
  | 'captureExternalAndApply'
  | 'keepExternal'
  | 'restoreManagedAndApply'

export type LuaFileDriftReport = {
  appId: number
  path: string
  fileExists: boolean
  baselineAvailable: boolean
  expectedSha256: string | null
  actualSha256: string | null
  drifted: boolean
  encodingStatus: 'utf8' | 'utf8Bom' | 'nonUtf8' | 'missing' | string
  validLua: boolean
  managedRevision: string | null
}

export type LuaRuntimePackage =
  | 'gseRegular'
  | 'gseExperimental'
  | 'gseColdClient'
  | 'gseColdClientV1'
  | 'ucOnline2'
  | 'runeRegular'
  | 'runeSteakClient'
  | 'runeSteamClient'

export type LuaOverlayRenderer =
  | 'gseNative'
  | 'reshadeCompatibility'
  | 'desktopFallback'
  | 'disabled'

export type LuaSteamStubMode = 'disabled' | 'autoSteamless' | 'steamless' | 'runeProxy' | 'ucRuntime'
export type LuaSaveMode = 'global' | 'portable' | 'custom'
export type LuaNetworkMode = 'offline' | 'lan' | 'ucOnline2'
export type LuaAccountMode = 'localEmulated' | 'steamClientSpacewar'

export type LuaRuntimeSettings = {
  schemaVersion: number
  defaultPackage: LuaRuntimePackage
  renderer: LuaOverlayRenderer
  overlayHotkey: 'Shift+Tab'
  desktopFallbackHotkey: 'Shift+F1'
  theme: 'launcher' | 'dark' | 'high-contrast'
  scale: number
  opacity: number
  soundEnabled: boolean
  notificationDurationMs: number
  telemetryEnabled: boolean
  reducedMotion: boolean
  saveMode: LuaSaveMode
  customSaveRoot: string | null
  networkMode: LuaNetworkMode
  accountMode: LuaAccountMode
  steamStubMode: LuaSteamStubMode
  resourceChannel: 'stable' | 'pinned'
  advancedFeatures: boolean
}

export type LuaRuntimeComponentStatus = 'available' | 'onDemand' | 'blocked' | 'unavailable'

export type LuaRuntimeComponentHealth = {
  id: string
  label: string
  status: LuaRuntimeComponentStatus
  version: string | null
  canonicalSource: string | null
  immutableCommit: string | null
  integrityVerified: boolean
  provenanceVerified: boolean
  artifactSha256: string | null
  license: string | null
  detail: string
}

export type LuaRuntimeSettingsState = {
  contractVersion: string
  settings: LuaRuntimeSettings
  components: LuaRuntimeComponentHealth[]
  activationAllowed: boolean
  activationBlockedReason: string
}

export type LuaRuntimeScannedFile = {
  relativePath: string
  architecture: 'x86' | 'x64' | null
  sha256: string
  sizeBytes: number
}

export type LuaRuntimeTargetScan = {
  schemaVersion: number
  appId: number
  installRoot: string
  runtimeTargets: LuaRuntimeScannedFile[]
  executables: LuaRuntimeScannedFile[]
  antiCheatSignals: string[]
  reparsePoints: string[]
  warnings: string[]
  blockedReasons: string[]
  approvalFingerprint: string
  approvalEligible: boolean
  locallyApproved: boolean
  approvedAt: string | null
  canApply: boolean
  applyBlockedReason: string
}

export type GseUcComponentHealth = {
  id: string
  label: string
  status: 'available' | 'blocked' | 'unavailable' | string
  version: string | null
  source: string | null
  integrityVerified: boolean
  provenanceVerified: boolean
  fileCount: number
  checkedFiles: number
  missingFiles: string[]
  corruptFiles: string[]
  license: string | null
  detail: string
}

export type GseUcActionKind = 'create' | 'replace'

export type GseUcFileAction = {
  kind: GseUcActionKind
  componentId: string
  architecture: 'x86' | 'x64' | null
  targetRelativePath: string
  sourceRelativePath: string
  beforeSha256: string | null
  afterSha256: string
  generated: boolean
}

export type GseUcPlan = {
  schemaVersion: number
  appId: number
  package: LuaRuntimePackage
  installRoot: string
  approvalFingerprint: string
  locallyApproved: boolean
  canApply: boolean
  blockedReasons: string[]
  warnings: string[]
  actions: GseUcFileAction[]
  componentHealth: GseUcComponentHealth[]
  steamStubMode: LuaSteamStubMode
}

export type GseUcOwnedFileReceipt = {
  targetRelativePath: string
  managedSha256: string
  originalSha256: string | null
  originalBackupRelativePath: string | null
}

export type GseUcReceiptState = {
  schemaVersion: number
  appId: number
  package: LuaRuntimePackage | null
  status: 'notInstalled' | 'installed' | 'repairRequired' | 'restored' | string
  transactionId: string | null
  approvalFingerprint: string | null
  ownedFiles: GseUcOwnedFileReceipt[]
  message: string
}

export type LuaDriftResolutionResult = {
  resolution: LuaDriftResolution
  before: LuaFileDriftReport
  captured: LuaVariantEntry | null
  applied: boolean
  game: LuaGameState
}

export type LuaVariantOrigin = 'active' | 'providerLive' | 'providerRaw' | 'imported'
export type LuaVariantCaptureReason =
  | 'manual'
  | 'beforeUpdate'
  | 'beforeProviderSwitch'
  | 'beforeChannelSwitch'
  | 'beforeImport'
  | 'beforeRestore'
  | 'legacyMigration'
export type LuaVariantValidationStatus = 'valid' | 'recoveryOnly'

export type LuaVariantEntry = {
  id: string
  appId: number
  sha256: string
  byteLength: number
  origin: LuaVariantOrigin
  captureReason: LuaVariantCaptureReason
  provider: string | null
  source: string | null
  channel: LuaGameChannel | null
  buildId: string | null
  revision: string | null
  capturedAt: string
  encodingStatus: 'utf8' | 'utf8Bom' | 'nonUtf8' | 'unknown'
  validationStatus: LuaVariantValidationStatus
  manifestSnapshotIdentity: string | null
  pinned: boolean
}

export type LuaVariantRestoreRequest = {
  appId: number
  sha256: string
}

export type SteamAppInfoFormat = 'v27' | 'v28' | 'v29'
export type SteamLaunchDriftState = 'staged' | 'applied' | 'drifted' | 'rebaseReviewRequired'

export type SteamLaunchOption = {
  index: string
  sourceIndex: string
  executable: string
  arguments: string
  workingDir: string
  description: string
  launchType: string
  osList: string
  osArch: string
  betaKey: string
  ownsDlc: string
}

export type SteamLaunchState = {
  appId: number
  format: SteamAppInfoFormat
  changeNumber: number
  current: SteamLaunchOption[]
  installDir: string | null
  isModded: boolean
  steamRunning: boolean
  driftState: SteamLaunchDriftState | null
}

export type SteamLaunchMod = {
  appId: number
  changeNumber: number
  original: SteamLaunchOption[]
  desired: SteamLaunchOption[]
  sourceFileHash: string
  savedAt: string
  appliedAt: string | null
  driftState: SteamLaunchDriftState
  initialBaseline: SteamLaunchBaseline | null
  latestSteamBaseline: SteamLaunchBaseline | null
}

export type SteamLaunchBaseline = {
  changeNumber: number
  sourceFileHash: string
  options: SteamLaunchOption[]
  capturedAt: string
}

export type SteamLaunchAuditReceipt = {
  receiptId: string
  appIds: number[]
  operation: 'apply' | 'reapply' | 'restore' | string
  transactionId: string
  beforeFileHash: string
  afterFileHash: string
  retainedBackupSha256: string
  initialBaselines: Record<string, SteamLaunchBaseline>
  latestSteamBaselines: Record<string, SteamLaunchBaseline>
  createdAt: string
}

export type SteamLaunchApplyRequest = { appIds: number[] }

export type SteamLaunchApplyResult = {
  ok: boolean
  appliedAppIds: number[]
  backupSha256: string | null
  transactionId: string | null
  auditReceipt: SteamLaunchAuditReceipt | null
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
  manifesthubKey: string | null
  manifesthubConfigured: boolean
  ryuuKey: string | null
  ryuuConfigured: boolean
  depotboxKey: string | null
  depotboxConfigured: boolean
  sushiEnabled: boolean
  githubMirrorsEnabled: boolean
  openluaEnabled: boolean
  steamtoolsEnabled: boolean
  ryuuEnabled: boolean
  luieEnabled: boolean
  twentyTwoCloudEnabled: boolean
  skyflareEnabled: boolean
}

export type LuaPackageProvider = 'curated' | 'community' | 'hubcap' | 'sushi' | 'githubMirrors' | 'openLua' | 'steamTools' | 'ryuu' | 'luie' | 'twentyTwoCloud' | 'skyflare' | 'none'
export type LuaSourceProvider = 'huggingFace' | 'hubcap' | 'sushi' | 'githubMirrors' | 'openLua' | 'steamTools' | 'ryuu' | 'luie' | 'twentyTwoCloud' | 'skyflare'
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

export type GameToolsCatalogKind = 'bypass' | 'onlineFix' | 'store'

export type GameToolsCatalogItem = {
  kind: GameToolsCatalogKind
  appId: number
  category: string | null
  name: string
  packageName: string | null
  imageUrl: string | null
  backgroundUrl: string | null
  logoUrl: string | null
  dependencies: string[]
  instructions: string[]
  note: string | null
  launchWithSteam: boolean
  launchExecutable: boolean
  active: boolean
  regularPrice: string | null
  supporterPrice: string | null
  discount: string | null
  sourceRepository: string
}

export type GameToolsCatalogResponse = {
  kind: GameToolsCatalogKind
  revision: string
  categories: string[]
  items: GameToolsCatalogItem[]
}

export type GameToolsStatus = {
  referenceVersion: string
  bypassCount: number
  onlineFixCount: number
  storeCount: number
  packageApplyMode: string
  capabilities: string[]
}

export type GameToolsPackageRequest = {
  kind: 'bypass' | 'onlineFix'
  appId: number
  requestId: string
  installDir: string
  revision: string
  packageSha256: string
}

export type GameToolsProvider = 'ubisoft' | 'ea' | 'rockstar' | 'denuvo' | 'playstation' | 'other'

export type GameToolsRouteState = {
  section: 'store' | 'tools' | 'bypass' | 'onlineFix'
  provider?: GameToolsProvider
  appId?: number
}

export type GameToolsSourceIdentity = {
  repository: string
  revision: string
  packageSha256: string
}

export type GameToolsImportResult = {
  transactionId: string
  installedFiles: number
  appIds: number[]
  depotIds: number[]
  requiresSteamRestart: boolean
}

export type GameToolsLibraryItem = {
  gameId: string
  appId: number | null
  title: string
  subtitle: string
  imageUrl: string | null
  installed: boolean
}

export type HomeWallpaperPreference =
  | { kind: 'featured'; assetId?: string }
  | { kind: 'pinned'; assetId: string }
  | { kind: 'custom'; assetId: string }

export type ManagedGseStatus = 'notManaged' | 'missing' | 'installed' | 'restored' | 'repairRequired' | 'conflict'

export type RuntimeComponentV2 = 'steamApi'

export type ManagedRuntimeFileStateV2 = {
  component: RuntimeComponentV2
  architecture: 'x86' | 'x64'
  targetPath: string
  managedSha256: string
  originalSha256: string | null
  originalBackupPath: string | null
}

export type ManagedGseState = {
  schemaVersion: number
  gameId: string
  appId: number | null
  steamRuntime: SteamRuntimeMode
  achievementsEnabled: boolean
  status: ManagedGseStatus
  catalogRevision: string
  runtimeVersion: string | null
  architecture: 'x86' | 'x64' | null
  dllPath: string | null
  managedSha256: string | null
  originalSha256: string | null
  originalBackupPath: string | null
  profileId?: string | null
  transactionId?: string | null
  ownedFiles?: ManagedRuntimeFileStateV2[]
  message: string
}

export type OverlayRenderer = 'gseNative' | 'reshadeCompatibility' | 'desktopFallback' | 'disabled'

export type ManagedRuntimeTargetSpecV2 = {
  relativePath: string
  architecture: 'x86' | 'x64'
  component: RuntimeComponentV2
  allowedOriginalSha256: string[]
  managedSha256: string
}

export type ManagedRuntimeProfileV2 = {
  schemaVersion: 2
  profileId: string
  gameId: string
  appId: number
  canonicalUpstream: string
  immutableCommit: string
  buildId: string
  patchSetHash: string
  license: string
  provenanceVerified: boolean
  executableAllowlist: string[]
  runtimeProcessAllowlist: string[]
  antiCheatPolicy: 'blockProtectedRuntime'
  targets: ManagedRuntimeTargetSpecV2[]
  generatedSettings: string[]
  generatedInterfaces: string[]
  generatedAssets: string[]
  renderer: OverlayRenderer
  hotkey: string
  achievementProtocolVersion: number
  saveProvider: string | null
  saveRoot: string | null
  cloudPolicy: string
  requiredComponents: string[]
  onDemandComponents: string[]
  restoreConstraints: string[]
}

export type ManagedRuntimePlanV2 = {
  schemaVersion: 2
  profile: ManagedRuntimeProfileV2
  state: ManagedGseState
  canApply: boolean
  blockedReason: string | null
  changes: Array<{
    targetRelativePath: string
    action: 'verify' | 'create' | 'replace' | 'blocked' | 'none' | string
    beforeSha256: string | null
    afterSha256: string
  }>
}

export type AchievementSchema = {
  id: string
  name: string
  description: string
  hidden: boolean
  target: number
}

export type AchievementRecord = {
  id: string
  unlocked: boolean
  unlockedAt: string | null
  progress: number
  target: number
}

export type AchievementState = {
  schemaVersion: number
  gameId: string
  appId: number
  sessionId: string | null
  connected: boolean
  transport: 'namedPipe' | 'fileFallback' | 'readOnly' | string
  schema: AchievementSchema[]
  achievements: Record<string, AchievementRecord>
  stats: Record<string, number>
  lastEventId: number
  updatedAt: string
}

export type AchievementEventType = 'schemaReady' | 'unlocked' | 'cleared' | 'progress' | 'statChanged' | 'flushed' | 'runtimeStopped'

export type AchievementEvent = {
  protocolVersion: number
  sessionId: string
  gameId: string
  appId: number
  pid: number
  messageId: number
  source: string
  eventType: AchievementEventType
  achievementId: string | null
  statId: string | null
  payload: Record<string, unknown>
  occurredAt: string
}

export type AchievementTransport = 'connecting' | 'namedPipe' | 'scopedFallback' | 'closed'

export type OverlayMetricsSummary = {
  sampleCount: number
  frameIntervalP95Ms: number | null
  overlayCallbackP95Ms: number | null
  privateBytes: number | null
  commitBytes: number | null
  vramBytes: number | null
  handleCount: number | null
  queueDepth: number
  sampledAt: number | null
}

export type GameSessionStateV1 = {
  schemaVersion: 1
  sessionId: string
  gameId: string
  appId: number
  lifecycle: 'starting' | 'running' | 'exiting' | 'closed' | 'failed'
  rootPid: number
  runtimePid: number | null
  renderer: OverlayRenderer
  achievementTransport: AchievementTransport
  droppedEventCount: number
  latestAchievementSequence: number
  metrics: OverlayMetricsSummary
}

export type AchievementEventV2 = {
  schemaVersion: 2
  eventId: string
  sequence: number
  sessionId: string
  gameId: string
  appId: number
  achievementId: string
  kind: 'schema' | 'unlock' | 'progress' | 'clear' | 'stat' | 'flush' | 'runtimeStopped'
  name?: string | null
  description?: string | null
  iconPath?: string | null
  current?: number | null
  maximum?: number | null
  occurredAt: number
  source: 'namedPipe' | 'scopedFallback'
}

export type SaveSnapshotPurpose = 'automatic' | 'preRestore'

export type SaveSnapshotRoot = {
  index: number
  provider: SaveProviderKind
  providerSaveId: string
  originalPath: string
  rootFingerprint: string
}

export type SaveSnapshotV2 = {
  schemaVersion: 2
  id: string
  gameId: string
  gameVersion: string
  runtimeVersion: string | null
  appId: number | null
  createdAt: string
  purpose: SaveSnapshotPurpose
  roots: SaveSnapshotRoot[]
  sourcePaths: string[]
  files: Array<{ relativePath: string; sizeBytes: number; sha256: string }>
  totalBytes: number
}

export type RestoreTransaction = {
  transactionId: string
  gameId: string
  snapshotId: string
  preRestoreSnapshotId: string
  filesReplaced: number
  status: 'committed'
}

export type RestoreAndRelaunchResult = {
  restore: RestoreTransaction
  launch: LaunchReport | null
  launchError: string | null
  cloudPolicyState: 'localWinsOncePrepared' | 'localWinsOnceRunning' | 'blocked' | string
}

export type GameToolsPackageProgress = {
  requestId: string
  appId: number
  phase: string
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
  currentFile: string | null
}

export type GameToolsPackageResult = {
  requestId: string
  kind: string
  appId: number
  gameName: string
  installDir: string
  sourceRepository: string
  sourceReference: string
  downloadedBytes: number
  appliedFiles: number
  backupFiles: number
  receiptPath: string
}

export type GameToolsAppliedPackageStatus = {
  requestId: string
  kind: string
  appId: number
  sourceRepository: string
  sourceReference: string
  installedAt: string
  committed: boolean
  restoredAt: string | null
  appliedFiles: number
  backupFiles: number
}

export type GameToolsGameStatus = {
  appId: number
  installed: boolean
  installDir: string | null
  latestPackage: GameToolsAppliedPackageStatus | null
}

export type GameToolsExecutable = {
  relativePath: string
  size: number
  patched: boolean
}

// Compatibility aliases for one desktop release while older UI bundles migrate.
export type LightningCatalogKind = GameToolsCatalogKind
export type LightningCatalogItem = GameToolsCatalogItem
export type LightningCatalogResponse = GameToolsCatalogResponse
export type LightningIntegrationStatus = GameToolsStatus
export type LightningPackageRequest = GameToolsPackageRequest
export type LightningPackageProgress = GameToolsPackageProgress
export type LightningPackageResult = GameToolsPackageResult
export type LightningAppliedPackageStatus = GameToolsAppliedPackageStatus
export type LightningGameStatus = GameToolsGameStatus
export type LightningExecutable = GameToolsExecutable

export type SteamlessResult = {
  success: boolean
  message: string
  outputPath: string | null
  variant: string | null
  steamAppId: number | null
}

export type FeaturePackageStatus = {
  id: string
  displayName: string
  capability: string
  source: string
  installed: boolean
  installedVersion: string | null
  entrypoint: string | null
  builtIn: boolean
  integration: 'builtIn' | 'automatic' | 'dependency' | 'component' | string
  usedBy: string | null
}

export type TabId =
  | 'Home'
  | 'Social'
  | 'What\'s New!'
  | 'Store'
  | 'Library'
  | 'Offline Activation'
  | 'Updates'
  | 'Downloads'
  | 'CloudRedirect'
  | 'Lua Installer'
  | 'Lua Shop'
  | 'Depot Downloader'
  | 'GSE / UC Setup'
  | 'Tools'
  | 'Translations'
  | 'Cache'
  | 'Settings'

export interface DepotGameItem {
  appid: number
  title: string
  folderName: string
  bannerUrl?: string
}

export interface DepotManifestInfo {
  depotId: number
  manifestGid: string
  manifestFile: string
}

export interface DepotBuildOption {
  buildId: string
  version?: string
  buildDate?: string
  manifests: DepotManifestInfo[]
}

export interface DepotGameDetail {
  appid: number
  title: string
  folderName: string
  builds: DepotBuildOption[]
  hasKey: boolean
}

export interface DepotDownloadProgressEvent {
  eventType: 'start' | 'depot-start' | 'progress' | 'log' | 'depot-done' | 'paused' | 'resumed' | 'complete' | 'error' | 'cancelled'
  appid: number
  buildId: string
  depotId?: string
  message?: string
  currentDepotIndex: number
  totalDepots: number
  progressPercent?: number
  speedMbps?: number
  transferredBytes?: number
  totalBytes?: number
  success?: boolean
}

export interface DepotDownloaderStatus {
  isDownloading: boolean
  isPaused: boolean
  canResume: boolean
  activeAppid?: number
  activeBuildId?: string
  destinationDir?: string
}

export interface DepotInstallState {
  appid: number
  installedBuildId?: string
  manifests: Record<string, string>
  completedUnix?: number
  hasDepotState: boolean
}

export type LauncherRoute = {
  tab: TabId
  selectedGameId?: string | null
  query?: Record<string, string>
}

export type LauncherNavigationSnapshot = {
  schemaVersion: 2
  currentRoute: LauncherRoute
  backStack: LauncherRoute[]
  forwardStack: LauncherRoute[]
  updatedAt: string
}

export type LauncherCollection = {
  id: string
  name: string
  gameIds: string[]
  createdAt: string
  updatedAt: string
}

export type LauncherShelf = {
  id: string
  title: string
  kind: 'recent' | 'installed' | 'favorites' | 'collection' | 'custom'
  collectionId?: string | null
  gameIds: string[]
}

export type XmclInstanceGroup = {
  id: string
  name: string
  gameIds: string[]
  collapsed: boolean
}

export type LauncherLibraryLayout = {
  schemaVersion: number
  libraryGameIds: string[]
  favoriteGameIds: string[]
  collections: LauncherCollection[]
  shelves: LauncherShelf[]
  xmclInstanceGroups: XmclInstanceGroup[]
  migratedLegacyAt?: string | null
  updatedAt: string
}

export { }
