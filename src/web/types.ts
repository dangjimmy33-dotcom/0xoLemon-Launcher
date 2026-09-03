export type WebUser = {
  id: string
  username: string
  displayName: string
  avatarUrl: string
}

export type LegalAcceptance = {
  accepted: boolean
  termsVersion: string
  privacyVersion: string
  acceptedAt: string | null
  locale: 'en' | 'vi' | null
}

export type WebSession = {
  enabled: boolean
  authenticated: boolean
  user?: WebUser
  legal?: LegalAcceptance
}

export type DeviceLibrary = {
  id: string
  label: string
  freeBytes: number
  default: boolean
}

export type LauncherDevice = {
  id: string
  name: string
  os: string
  launcherVersion: string
  online: boolean
  lastSeen: string | null
  libraries: DeviceLibrary[]
  installedGameIds: string[]
}

export type RemoteJobAction = 'install' | 'update' | 'downgrade' | 'repair' | 'verify' | 'launch'
export type RemoteJobState = 'dispatching' | 'accepted' | 'running' | 'completed' | 'failed' | 'canceled'

export type RemoteJobProgress = {
  overallProgress: number
  phase: string
  bytesDone: number
  bytesTotal: number
  speedBytesPerSecond: number
}

export type RemoteJob = {
  id: string
  requestId: string
  action: RemoteJobAction
  gameId: string
  versionId: string
  deviceId: string
  libraryId: string
  state: RemoteJobState
  progress: RemoteJobProgress | null
  errorCode: string | null
  errorMessage: string | null
  createdAt: string | null
  updatedAt: string | null
}

export type WebCatalogVersion = {
  version: string
  label?: string
  buildId?: string
  sizeBytes?: number
  latest?: boolean
}

export type WebCatalogGame = {
  id: string
  title: string
  subtitle?: string
  developer?: string
  publisher?: string
  latestVersion?: string
  availableVersions?: WebCatalogVersion[]
  gridAssetUrl?: string
  heroAssetUrl?: string
}

export type WebCatalog = {
  defaultLocale: string
  games: WebCatalogGame[]
}

export type RemoteEvent = {
  id?: string
  type: string
  payload: unknown
}
