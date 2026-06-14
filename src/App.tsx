import { useEffect, useMemo, useState, useRef, type ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { getCurrentWindow } from '@tauri-apps/api/window'

function CustomTitleBar() {
  const win = isTauriRuntime() ? getCurrentWindow() : null

  function handleMinimize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.minimize()
  }
  function handleMaximize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.toggleMaximize()
  }
  function handleClose(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.close()
  }

  return (
    <div data-tauri-drag-region className="custom-titlebar">
      <div className="titlebar-drag-area" data-tauri-drag-region>
        <span className="titlebar-label">0xoLemon Launcher</span>
      </div>
      <div className="titlebar-actions">
        <button
          className="titlebar-btn minimize-btn"
          title="Thu nhỏ"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMinimize}
        >
          <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
        </button>
        <button
          className="titlebar-btn maximize-btn"
          title="Phóng to"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMaximize}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor"/></svg>
        </button>
        <button
          className="titlebar-btn close-btn"
          title="Đóng"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" strokeWidth="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" strokeWidth="1.2"/></svg>
        </button>
      </div>
    </div>
  )
}
import {
  Archive,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Check,
  CheckCircle2,
  CircleAlert,
  Database,
  Download,
  FolderOpen,
  Gauge,
  HardDrive,
  Image as ImageIcon,
  Library,
  Pause,
  Play,
  RefreshCcw,
  RotateCcw,
  Search,
  Settings,
  ShieldCheck,
  Square,
  TerminalSquare,
  Trophy,
  Plus,
  Wifi,
  X,
} from 'lucide-react'
import { enUS as t } from './i18n/en-US'
import './App.css'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

type JobStatus =
  | 'planned'
  | 'running'
  | 'paused'
  | 'downloading'
  | 'assembling'
  | 'verified'
  | 'committed'
  | 'canceled'
  | 'failed'

type StepStatus = 'waiting' | 'running' | 'completed' | 'paused' | 'failed'

type JobStep = {
  name: string
  detail: string
  status: StepStatus
  progress: number
  retryCount: number
}

type JobLog = {
  at: string
  level: string
  message: string
}

type PhaseProgress = {
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

type JobJournal = {
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

type ChangedFile = {
  path: string
  oldSize: number
  newSize: number
}

type Snapshot = {
  currentVersion: string
  latestVersion: string
  availableVersions: string[]
  detectedInstallPath: string | null
  updateSize: number
  proxyStatus: string
  cache: {
    cacheSize: number
    freeSpace: number
    healthPercent: number
    rollbackReady: boolean
    rollbackMissingBytes: number
  }
  changedFiles: ChangedFile[]
  lastJob: JobJournal | null
}

type GameCatalog = {
  defaultLocale: string
  games: GameSummary[]
}

type GameSummary = {
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

type GameVersionInfo = {
  version: string
  label: string
  buildId: string
  sizeBytes: number
  latest: boolean
}

type GameInstallMetadata = {
  defaultStoreRoot: string
  defaultInstallFolder: string
  defaultDownloadingFolder: string
  storageLabel: string
  supportsResume: boolean
  launchExecutable: string
}

type GameDetail = {
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

type GameRating = {
  source: string
  score: string
}

type GameMedia = {
  id: string
  role: string
  title: string
  mimeType: string
  assetId: string
}

type LauncherUpdateInfo = {
  version: string
  notes: string
  downloadUrl: string
  publishedAt: string
}

type GameAchievement = {
  id: string
  name: string
  description: string
  iconAssetId: string
  hidden: boolean
}

type GameSound = {
  id: string
  role: string
  mimeType: string
  assetId: string
}

type AssetBlob = {
  mimeType: string
  dataBase64: string
}

type GameInstallState = {
  gameId: string
  installed: boolean
  currentVersion: string
  installPath: string
  launchExecutable: string
}

type VerifyInstallReport = {
  ok: boolean
  checkedFiles: number
  missingFiles: string[]
  mismatchedFiles: string[]
}

type VerifyUiStatus = {
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

type VerifyProgressPayload = {
  gameId: string
  phase: string
  currentFile: string | null
  checkedFiles: number
  totalFiles: number
  checkedBytes: number
  totalBytes: number
  percent: number
}

type UninstallReport = {
  gameId: string
  removedFiles: number
  removedDirs: number
  installPath: string
}

type LaunchReport = {
  gameId: string
  executable: string
  shortcutPath: string | null
  dependenciesInstalled: string[]
}

type LaunchSplashState = {
  title: string
  heroUrl?: string
  iconUrl?: string
}

type ShortcutLaunchPayload = {
  gameId: string
  installPath: string
  launchExecutable?: string | null
}

type TabId = 'Library' | 'Updates' | 'Downloads' | 'Cache' | 'Settings'

const DEFAULT_GAME_ID = '007-first-light'
const DEFAULT_STORE_ROOT = 'E:\\0xoLemon store'
const DEFAULT_COMMON_GAME = `${DEFAULT_STORE_ROOT}\\common\\007 First Light`
const DEFAULT_DOWNLOADING_GAME = `${DEFAULT_STORE_ROOT}\\downloading\\007 First Light`
const CUSTOM_DOWNLOADING_RELATIVE = '.0xolemon\\downloading'

const fallbackSnapshot: Snapshot = {
  currentVersion: 'not scanned',
  latestVersion: 'unknown',
  availableVersions: [],
  detectedInstallPath: null,
  updateSize: 0,
  proxyStatus: 'Depot not checked',
  cache: {
    cacheSize: 0,
    freeSpace: 0,
    healthPercent: 0,
    rollbackReady: false,
    rollbackMissingBytes: 0,
  },
  changedFiles: [],
  lastJob: null,
}

const fallbackInstall: GameInstallMetadata = {
  defaultStoreRoot: DEFAULT_STORE_ROOT,
  defaultInstallFolder: DEFAULT_COMMON_GAME,
  defaultDownloadingFolder: DEFAULT_DOWNLOADING_GAME,
  storageLabel: 'SSD',
  supportsResume: true,
  launchExecutable: 'Retail\\007FirstLight.exe',
}

function gameFolderName(game: Pick<GameSummary, 'id' | 'title'>) {
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

function normalizeInstallMetadata(
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

const fallbackCatalog: GameCatalog = {
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
      assetPackPath: 'assets/games/007-first-light/core.0xo',
    },
  ],
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot)
  const [job, setJob] = useState<JobJournal | null>(fallbackSnapshot.lastJob)
  const [installPath, setInstallPath] = useState('')
  const [scanStatus, setScanStatus] = useState('No install found')
  const [hasScanned, setHasScanned] = useState(false)
  const [activeTab, setActiveTab] = useState<TabId>('Library')
  const [selectedVersion, setSelectedVersion] = useState('')
  const [showInstallOptions, setShowInstallOptions] = useState(false)
  const [installRoot, setInstallRoot] = useState(DEFAULT_COMMON_GAME)
  const [catalog, setCatalog] = useState<GameCatalog>(fallbackCatalog)
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)
  const [detail, setDetail] = useState<GameDetail | null>(null)
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({})
  const [installStates, setInstallStates] = useState<Record<string, GameInstallState>>({})
  const [catalogStatus, setCatalogStatus] = useState(
    isTauriRuntime() ? 'Loading asset pack' : 'Browser preview uses fallback metadata',
  )
  const downloadSampleRef = useRef<{ jobId: string; bytesDone: number; at: number } | null>(null)
  const [downloadRate, setDownloadRate] = useState(0)
  const [verifyStatus, setVerifyStatus] = useState<VerifyUiStatus | null>(null)
  const [launchSplash, setLaunchSplash] = useState<LaunchSplashState | null>(null)
  const [launcherUpdate, setLauncherUpdate] = useState<LauncherUpdateInfo | null>(null)
  const [launcherUpdateStatus, setLauncherUpdateStatus] = useState<string | null>(null)
  const [showDrivePicker, setShowDrivePicker] = useState(false)
  // Library locations the user has added (persisted to localStorage)
  const [libraries, setLibraries] = useState<string[]>(() => {
    try {
      const saved = localStorage.getItem('0xo_libraries')
      return saved ? JSON.parse(saved) : ['E:\\', 'C:\\']
    } catch {
      return ['E:\\', 'C:\\']
    }
  })

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    invoke<GameCatalog>('get_game_catalog')
      .then((next) => {
        setCatalog(next)
        setCatalogStatus('Asset pack loaded')
      })
      .catch((error) => {
        setCatalogStatus(String(error))
        setCatalog(fallbackCatalog)
      })

    invoke<LauncherUpdateInfo | null>('check_launcher_update')
      .then((info) => {
        if (info) setLauncherUpdate(info)
      })
      .catch(console.error)
  }, [])

  async function refreshInstallState(gameId: string) {
    if (!isTauriRuntime()) {
      return
    }
    const state = await invoke<GameInstallState>('get_game_install_state', { gameId })
    setInstallStates((current) => ({ ...current, [gameId]: state }))
  }

  useEffect(() => {
    if (!isTauriRuntime() || catalog.games.length === 0) {
      return
    }

    let disposed = false
    Promise.all(
      catalog.games.map((game) =>
        invoke<GameInstallState>('get_game_install_state', { gameId: game.id }).catch(() => null),
      ),
    ).then((states) => {
      if (disposed) return
      const next: Record<string, GameInstallState> = {}
      for (const state of states) {
        if (state) next[state.gameId] = state
      }
      setInstallStates(next)
    })

    return () => {
      disposed = true
    }
  }, [catalog.games])

  useEffect(() => {
    if (!selectedGameId) {
      return
    }

    if (!isTauriRuntime()) {
      const game = catalog.games.find((candidate) => candidate.id === selectedGameId)
      let disposed = false
      Promise.resolve().then(() => {
        if (!disposed && game) {
          setDetail(fallbackDetailFromSummary(game))
        }
      })
      return () => {
        disposed = true
      }
    }

    invoke<GameDetail>('get_game_detail', { gameId: selectedGameId, locale: 'en-US' })
      .then(setDetail)
      .catch((error) => {
        setCatalogStatus(String(error))
        const game = catalog.games.find((candidate) => candidate.id === selectedGameId)
        setDetail(game ? fallbackDetailFromSummary(game) : null)
      })
  }, [catalog.games, selectedGameId])

  const selectedGame = useMemo(
    () => (selectedGameId ? catalog.games.find((game) => game.id === selectedGameId) ?? null : null),
    [catalog.games, selectedGameId],
  )

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let disposed = false
    let unlistenLaunch: (() => void) | undefined
    let unlistenError: (() => void) | undefined

    listen<ShortcutLaunchPayload>('launcher://shortcut-launch', (event) => {
      const payload = event.payload
      const game = catalog.games.find((candidate) => candidate.id === payload.gameId)
      setSelectedGameId(payload.gameId)
      setInstallPath(payload.installPath)
      setInstallRoot(payload.installPath)
      setActiveTab('Library')
      setScanStatus(`Starting ${game?.title ?? payload.gameId}`)
      setLaunchSplash({
        title: game?.title ?? payload.gameId,
        heroUrl: game ? assetUrls[game.heroAssetId] : undefined,
        iconUrl: game ? assetUrls[game.iconAssetId] || assetUrls[game.gridAssetId] : undefined,
      })
      window.setTimeout(() => setLaunchSplash(null), 4200)
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unlistenLaunch = fn
      }
    })

    listen<string>('launcher://shortcut-launch-error', (event) => {
      setLaunchSplash(null)
      setScanStatus(event.payload)
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unlistenError = fn
      }
    })

    return () => {
      disposed = true
      unlistenLaunch?.()
      unlistenError?.()
    }
  }, [assetUrls, catalog.games])

  useEffect(() => {
    if (!selectedGame) {
      return
    }
    const state = installStates[selectedGame.id]
    let disposed = false
    queueMicrotask(() => {
      if (disposed) {
        return
      }
      if (state?.installed) {
        setInstallPath(state.installPath)
        setInstallRoot(state.installPath)
        setHasScanned(true)
        setScanStatus(`Installed ${state.currentVersion}`)
      } else {
        setInstallPath('')
        setInstallRoot(normalizeInstallMetadata(selectedGame, selectedGame.install).defaultInstallFolder)
        setHasScanned(false)
        setScanStatus('No install found')
      }
    })
    return () => {
      disposed = true
    }
  }, [installStates, selectedGame])

  useEffect(() => {
    const ids = catalog.games.flatMap((game) => [game.gridAssetId, game.logoAssetId, game.iconAssetId]).filter(Boolean)
    const missing = ids.filter((id) => !assetUrls[id])
    if (!isTauriRuntime() || missing.length === 0) {
      return
    }

    let disposed = false
    Promise.all(
      missing.map(async (assetId) => {
        const game = catalog.games.find((candidate) =>
          [candidate.gridAssetId, candidate.logoAssetId, candidate.iconAssetId].includes(assetId),
        )
        if (!game) return null
        const blob = await invoke<AssetBlob>('get_game_asset', { gameId: game.id, assetId })
        return [assetId, `data:${blob.mimeType};base64,${blob.dataBase64}`] as const
      }),
    )
      .then((loaded) => {
        if (disposed) return
        setAssetUrls((current) => {
          const next = { ...current }
          for (const item of loaded) {
            if (item) {
              const [assetId, url] = item
              next[assetId] = url
            }
          }
          return next
        })
      })
      .catch((error) => setCatalogStatus(String(error)))

    return () => {
      disposed = true
    }
  }, [assetUrls, catalog.games])

  useEffect(() => {
    if (!selectedGame || !detail) {
      return
    }
    const ids = collectAssetIds(selectedGame, detail)
    const missing = ids.filter((id) => id && !assetUrls[id])
    if (!isTauriRuntime() || missing.length === 0) {
      return
    }

    let disposed = false
    Promise.all(
      missing.map(async (assetId) => {
        const blob = await invoke<AssetBlob>('get_game_asset', { gameId: selectedGame.id, assetId })
        return [assetId, `data:${blob.mimeType};base64,${blob.dataBase64}`] as const
      }),
    )
      .then((loaded) => {
        if (disposed) return
        setAssetUrls((current) => {
          const next = { ...current }
          for (const [assetId, url] of loaded) {
            next[assetId] = url
          }
          return next
        })
      })
      .catch((error) => setCatalogStatus(String(error)))

    return () => {
      disposed = true
    }
  }, [assetUrls, detail, selectedGame])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    invoke<Snapshot>('get_launcher_snapshot')
      .then((next) => {
        setSnapshot(next)
        setJob(next.lastJob)
        if (next.detectedInstallPath) {
          setInstallPath(next.detectedInstallPath)
          setInstallRoot(next.detectedInstallPath)
          setHasScanned(next.currentVersion !== 'unknown' && next.currentVersion !== 'not installed')
          setScanStatus(`0xoLemon store install recognized (${next.currentVersion})`)
        }
      })
      .catch(() => {
        setSnapshot(fallbackSnapshot)
      })

    let disposed = false
    let unsubscribe: (() => void) | undefined
    listen<JobJournal>('launcher://job', (event) => {
      const nextJob = event.payload
      setJob(nextJob)
      if (nextJob.status === 'committed') {
        setInstallPath(nextJob.installPath)
        setInstallRoot(nextJob.installPath)
        setHasScanned(true)
        setScanStatus(`${nextJob.kind === 'install' ? 'Installed' : 'Updated'} ${nextJob.toVersion}`)
        setShowInstallOptions(false)
        setInstallStates((current) => ({
          ...current,
          [nextJob.gameId]: {
            gameId: nextJob.gameId,
            installed: true,
            currentVersion: nextJob.toVersion,
            installPath: nextJob.installPath,
            launchExecutable: current[nextJob.gameId]?.launchExecutable ?? '',
          },
        }))
        void refreshInstallState(nextJob.gameId)
        setSnapshot((current) => ({
          ...current,
          currentVersion: nextJob.toVersion,
          detectedInstallPath: nextJob.installPath,
          updateSize: 0,
          changedFiles: [],
        }))
        setVerifyStatus((current) => {
          if (current?.gameId === nextJob.gameId) {
            return nextJob.kind === 'repair'
              ? { ...current, state: 'ok', message: 'Repair completed successfully.', percent: 1 }
              : null
          }
          return current
        })
      } else if (nextJob.status === 'failed' || nextJob.status === 'canceled') {
        setVerifyStatus((current) => {
          if (current?.gameId === nextJob.gameId && current.state === 'running') {
            return {
              ...current,
              state: 'failed',
              message: `Repair ${nextJob.status}.`,
            }
          }
          return current
        })
      }
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unsubscribe = fn
      }
    })

    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let disposed = false
    let unsubscribe: (() => void) | undefined
    listen<VerifyProgressPayload>('launcher://verify-progress', (event) => {
      const progress = event.payload
      setVerifyStatus((current) => {
        const finalState =
          progress.phase === 'Verified' ? 'ok' : progress.phase === 'Verify failed' ? 'failed' : null
        const message = finalState
          ? progress.phase
          : `${progress.phase}: ${progress.checkedFiles}/${progress.totalFiles} files`
        return {
          gameId: progress.gameId,
          state: finalState ?? 'running',
          message:
            finalState && current?.gameId === progress.gameId && current.state !== 'running'
              ? current.message
              : message,
          percent: progress.percent,
          currentFile: progress.currentFile,
          checkedFiles: progress.checkedFiles,
          totalFiles: progress.totalFiles,
          checkedBytes: progress.checkedBytes,
          totalBytes: progress.totalBytes,
          missingFiles: current?.gameId === progress.gameId ? current.missingFiles : undefined,
          mismatchedFiles: current?.gameId === progress.gameId ? current.mismatchedFiles : undefined,
        }
      })
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unsubscribe = fn
      }
    })

    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [])

  const activeJob = job ?? createIdleJob(snapshot)
  useEffect(() => {
    if (activeJob.status !== 'downloading') {
      downloadSampleRef.current = null
      return
    }

    const now = performance.now()
    const previous = downloadSampleRef.current
    if (previous && previous.jobId === activeJob.id && activeJob.bytesDone >= previous.bytesDone) {
      const deltaBytes = activeJob.bytesDone - previous.bytesDone
      const deltaMs = now - previous.at
      if (deltaBytes > 0 && deltaMs > 0) {
        const instantRate = (deltaBytes * 1000) / deltaMs
        setDownloadRate((current) => (current > 0 ? current * 0.65 + instantRate * 0.35 : instantRate))
      }
    }
    downloadSampleRef.current = { jobId: activeJob.id, bytesDone: activeJob.bytesDone, at: now }
  }, [activeJob.id, activeJob.status, activeJob.bytesDone])

  const phaseProgress = getPhaseProgress(activeJob, activeJob.status === 'downloading' ? downloadRate : 0)
  const progress = phaseProgress.percent
  const hasVisibleJob = job !== null && activeJob.status !== 'committed'
  const isDefaultGame = selectedGame?.id === DEFAULT_GAME_ID
  const selectedInstallState = selectedGame ? installStates[selectedGame.id] : undefined
  const selectedInstalled = Boolean(selectedInstallState?.installed)
  const gameInstall = useMemo(
    () => normalizeInstallMetadata(selectedGame, detail?.install ?? selectedGame?.install ?? fallbackInstall),
    [detail?.install, selectedGame],
  )
  const selectedInstallPath = selectedInstalled
    ? selectedInstallState?.installPath || gameInstall.defaultInstallFolder
    : gameInstall.defaultInstallFolder
  const selectedCurrentVersion = selectedInstalled ? selectedInstallState?.currentVersion ?? 'installed' : 'not installed'
  const selectedVerifyStatus = selectedGame && verifyStatus?.gameId === selectedGame.id ? verifyStatus : null
  const availableVersions = selectedGame ? versionOptions(snapshot, selectedGame, isDefaultGame) : []
  const fallbackTargetVersion = isDefaultGame && availableVersions.includes(snapshot.latestVersion)
    ? snapshot.latestVersion
    : selectedGame?.latestVersion || availableVersions[availableVersions.length - 1] || 'select game'
  const targetVersion = selectedVersion && availableVersions.includes(selectedVersion) ? selectedVersion : fallbackTargetVersion
  const selectedVersionInfo =
    selectedGame?.availableVersions.find((version) => version.version === targetVersion) ??
    detail?.versions.find((version) => version.version === targetVersion)
  const installMode = !selectedInstalled
  const updateReady =
    selectedInstalled &&
    selectedCurrentVersion !== 'unknown' &&
    targetVersion !== 'unknown' &&
    selectedCurrentVersion !== targetVersion
  const isPaused = activeJob.status === 'paused'
  const isRunning = job !== null && ['running', 'downloading', 'assembling', 'paused'].includes(activeJob.status)
  const canUpdate = Boolean(selectedGame && detail) && !isRunning && targetVersion !== 'unknown' && targetVersion !== 'select game' && (installMode || updateReady)
  const effectiveDownloadSize =
    isDefaultGame && updateReady && snapshot.updateSize > 0
      ? snapshot.updateSize
      : selectedVersionInfo?.sizeBytes ?? detail?.versions[0]?.sizeBytes ?? 0
  const displayedInstallTarget =
    selectedInstalled
      ? selectedInstallPath
      : hasVisibleJob && activeJob.installPath
        ? activeJob.installPath
        : installRoot || gameInstall.defaultInstallFolder

  async function chooseInstallFolder() {
    if (!isTauriRuntime()) {
      setScanStatus('Folder picker requires desktop shell')
      return
    }

    try {
      const selected = await open({ directory: true, multiple: false, title: `Select ${selectedGame?.title ?? 'game'} folder` })
      if (typeof selected === 'string') {
        setInstallPath(selected)
        await scanFolder(selected)
      }
    } catch {
      setScanStatus('Folder picker unavailable')
    }
  }

  async function scanFolder(path = installPath) {
    if (!selectedGame) {
      setScanStatus('Select a game first')
      setHasScanned(false)
      return
    }
    if (!path) {
      setScanStatus('Choose the game folder first')
      setHasScanned(false)
      return
    }

    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot scan local game files')
      return
    }

    try {
      const report = await invoke<{ fileCount: number; detectedVersion?: string | null; warnings: string[] }>('scan_install', {
        path,
      })
      const version = report.detectedVersion ? `detected ${report.detectedVersion}` : 'version needs manifest verify'
      setScanStatus(`${report.fileCount} files, ${version}`)
      const planned = await invoke<Snapshot>('plan_install_update', { path, targetVersion, gameId: selectedGame.id })
      setSnapshot(planned)
      setJob(planned.lastJob)
      setHasScanned(Boolean(report.detectedVersion))
    } catch (error) {
      setScanStatus(String(error))
      setHasScanned(false)
    }
  }

  async function changeTargetVersion(version: string) {
    if (!selectedGame) {
      setScanStatus('Select a game first')
      return
    }
    setSelectedVersion(version)
    if (!isTauriRuntime()) {
      return
    }

    try {
      const planned =
        installMode || !installPath || !hasScanned
          ? await invoke<Snapshot>('plan_fresh_install', { targetVersion: version, gameId: selectedGame.id })
          : await invoke<Snapshot>('plan_install_update', { path: installPath, targetVersion: version, gameId: selectedGame.id })
      setSnapshot(planned)
      setJob(planned.lastJob)
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  function chooseInstallTarget() {
    setShowDrivePicker(true)
  }

  function applyLibraryDrive(driveLetter: string) {
    // driveLetter is e.g. "E:" or "C:"
    const drivePath = driveLetter.replace(/\\+$/, '')
    const gameName = selectedGame ? gameFolderName(selectedGame) : '007 First Light'
    const newRoot = `${drivePath}\\0xoLemon store\\common\\${gameName}`
    setInstallRoot(newRoot)
    setShowDrivePicker(false)
  }

  async function addLibraryDrive() {
    try {
      const selected = await open({ directory: true, multiple: false, title: 'Select a drive or folder to add as library' })
      if (typeof selected === 'string') {
        // Normalise to drive root if user selected root
        const drive = selected.match(/^([A-Za-z]:)/)?.[1] ?? selected
        const driveLetter = `${drive.toUpperCase().charAt(0)}:`
        if (!libraries.includes(driveLetter)) {
          const next = [...libraries, driveLetter]
          setLibraries(next)
          localStorage.setItem('0xo_libraries', JSON.stringify(next))
        }
      }
    } catch {
      // ignore
    }
  }

  async function startUpdate() {
    if (!selectedGame || !detail) {
      setScanStatus('Select a game first')
      return
    }
    if (installMode && !showInstallOptions) {
      setShowInstallOptions(true)
      return
    }

    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot write local game files')
      return
    }

    // Disk space check
    try {
      const targetPath = installMode ? installRoot : selectedInstallPath
      const freeSpace = await invoke<number>('get_disk_free_space', { path: targetPath })
      const requiredSpace = snapshot ? snapshot.updateSize : 0
      if (freeSpace < requiredSpace) {
        const freeGB = (freeSpace / 1024 / 1024 / 1024).toFixed(2)
        const reqGB = (requiredSpace / 1024 / 1024 / 1024).toFixed(2)
        setScanStatus(`Not enough disk space! Need ${reqGB} GB, but only ${freeGB} GB available.`)
        return
      }
    } catch (e) {
      console.warn('Disk space check failed:', e)
      // Continue anyway if the check fails (e.g. path doesn't exist yet)
    }

    try {
      const next = installMode
        ? await invoke<JobJournal>('start_install_job', { gameId: selectedGame.id, targetVersion, installPath: installRoot })
        : await invoke<JobJournal>('start_update_job', { gameId: selectedGame.id, installPath: selectedInstallPath, targetVersion })
      setJob(next)
      setActiveTab('Downloads')
      setShowInstallOptions(false)
      if (installMode) {
        setInstallPath(installRoot)
        setScanStatus(`Installing ${targetVersion}`)
      }
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  async function playSelectedGame() {
    if (!selectedGame || !detail) {
      setScanStatus('Select a game first')
      return
    }
    if (!selectedInstalled) {
      setShowInstallOptions(true)
      return
    }
    if (!isTauriRuntime()) {
      setScanStatus('Desktop launcher required to start the game')
      return
    }

    const splashStartedAt = performance.now()
    setLaunchSplash({
      title: selectedGame.title,
      heroUrl: assetUrls[selectedGame.heroAssetId] || firstMediaUrl(detail, assetUrls),
      iconUrl: assetUrls[selectedGame.iconAssetId] || assetUrls[selectedGame.gridAssetId],
    })

    try {
      const report = await invoke<LaunchReport>('launch_game', {
        gameId: selectedGame.id,
        installPath: selectedInstallPath,
        launchExecutable: selectedInstallState?.launchExecutable || gameInstall.launchExecutable,
      })
      const dependencyText =
        report.dependenciesInstalled.length > 0
          ? `Installed ${report.dependenciesInstalled.length} dependency package(s), then started`
          : 'Started'
      setScanStatus(`${dependencyText} ${selectedGame.title}`)
      const remainingMs = Math.max(1200, 2600 - (performance.now() - splashStartedAt))
      window.setTimeout(() => setLaunchSplash(null), remainingMs)
    } catch (error) {
      setLaunchSplash(null)
      setScanStatus(String(error))
    }
  }

  async function verifySelectedGame() {
    if (!selectedGame) {
      setScanStatus('Select a game first')
      return
    }
    if (!selectedInstalled) {
      setScanStatus('Install the game before verify')
      return
    }
    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot verify local game files')
      return
    }

    const gameId = selectedGame.id
    const verifyInstallPath = selectedInstallPath
    const verifyTargetVersion = selectedCurrentVersion
    setVerifyStatus({
      gameId,
      state: 'running',
      message: 'Verifying local files...',
      percent: 0,
      currentFile: null,
      checkedFiles: 0,
      totalFiles: 0,
      checkedBytes: 0,
      totalBytes: 0,
    })
    setScanStatus('Verifying local files...')

    try {
      const report = await invoke<VerifyInstallReport>('verify_install_integrity', {
        gameId,
        installPath: verifyInstallPath,
        targetVersion: verifyTargetVersion,
      })
      const filePaths = [...report.missingFiles, ...report.mismatchedFiles]
      const message = report.ok
        ? `Verified ${report.checkedFiles} files`
        : `Verify found ${report.missingFiles.length} missing and ${report.mismatchedFiles.length} changed files. Repair starts automatically.`
      setVerifyStatus({
        gameId,
        state: report.ok ? 'ok' : 'running',
        message,
        percent: 1,
        checkedFiles: report.checkedFiles,
        missingFiles: report.missingFiles,
        mismatchedFiles: report.mismatchedFiles,
      })
      setScanStatus(message)

      if (filePaths.length > 0) {
        try {
          const planned = await invoke<JobJournal>('start_repair_job', {
            gameId,
            installPath: verifyInstallPath,
            targetVersion: verifyTargetVersion,
            filePaths,
          })
          setJob(planned)
          setActiveTab('Downloads')
          setScanStatus(`Repairing ${filePaths.length} file${filePaths.length === 1 ? '' : 's'} after verify`)
        } catch (repairError) {
          const repairMessage = `Verify found ${filePaths.length} file${filePaths.length === 1 ? '' : 's'} to repair, but repair could not start: ${String(repairError)}`
          setVerifyStatus({
            gameId,
            state: 'failed',
            message: repairMessage,
            percent: 1,
            checkedFiles: report.checkedFiles,
            missingFiles: report.missingFiles,
            mismatchedFiles: report.mismatchedFiles,
          })
          setScanStatus(repairMessage)
        }
      }
    } catch (error) {
      const message = String(error)
      setVerifyStatus({
        gameId,
        state: 'failed',
        message,
        percent: 1,
      })
      setScanStatus(message)
    }
  }

  async function uninstallSelectedGame() {
    if (!selectedGame || !selectedInstalled) {
      return
    }
    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot uninstall local game files')
      return
    }

    try {
      const report = await invoke<UninstallReport>('uninstall_game', {
        gameId: selectedGame.id,
        installPath: selectedInstallPath,
      })
      setInstallStates((current) => ({
        ...current,
        [selectedGame.id]: {
          gameId: selectedGame.id,
          installed: false,
          currentVersion: 'not installed',
          installPath: gameInstall.defaultInstallFolder,
          launchExecutable: gameInstall.launchExecutable,
        },
      }))
      setInstallPath('')
      setInstallRoot(gameInstall.defaultInstallFolder)
      setHasScanned(false)
      setScanStatus(`Uninstalled ${selectedGame.title}: removed ${report.removedFiles} files`)
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  async function pauseOrResume() {
    if (!isTauriRuntime()) {
      setJob((current) => (current ? { ...current, status: isPaused ? 'running' : 'paused' } : current))
      return
    }

    if (isPaused) {
      await invoke('resume_job').catch(() => undefined)
      setJob((current) => (current ? { ...current, status: 'running' } : current))
    } else {
      await invoke('pause_job').catch(() => undefined)
      setJob((current) => (current ? { ...current, status: 'paused' } : current))
    }
  }

  async function cancelJob() {
    if (!isTauriRuntime()) {
      setJob((current) => (current ? { ...current, status: 'canceled', phase: 'Canceled' } : current))
      return
    }

    await invoke('cancel_job').catch(() => undefined)
    setJob((current) => (current ? { ...current, status: 'canceled', phase: 'Canceled' } : current))
  }

  return (
    <div className="app-root">
      <CustomTitleBar />
      <main className="launcher-shell">
        {launcherUpdate ? (
          <div className="launcher-update-banner">
            <div className="banner-content">
              <strong>Launcher Update Available (v{launcherUpdate.version})</strong>
              {launcherUpdateStatus ? (
                <span>{launcherUpdateStatus}</span>
              ) : (
                <span>Restart to apply.</span>
              )}
            </div>
            <button
              className="primary-control small"
              onClick={() => {
                setLauncherUpdateStatus('Downloading...')
                invoke('apply_launcher_update', { downloadUrl: launcherUpdate.downloadUrl })
                  .catch((e) => setLauncherUpdateStatus(`Failed: ${e}`))
              }}
              disabled={!!launcherUpdateStatus}
            >
              Update Now
            </button>
          </div>
        ) : null}
        <Sidebar
        serviceStatus={contentServiceLabel(snapshot.proxyStatus)}
        activeTab={activeTab}
        onSelect={setActiveTab}
        updateCount={updateReady ? 1 : 0}
        downloadCount={hasVisibleJob ? 1 : 0}
      />
      <section className="workspace">
        {activeTab !== 'Library' && selectedGame && detail ? (
          <OperationHero
            game={selectedGame}
            detail={detail}
            assets={assetUrls}
            currentVersion={snapshot.currentVersion}
            latestVersion={snapshot.latestVersion}
            updateReady={updateReady}
            updateSize={effectiveDownloadSize}
            onUpdate={startUpdate}
            onPlay={playSelectedGame}
            isRunning={isRunning}
            canUpdate={canUpdate}
            installMode={installMode}
            selectedInstalled={selectedInstalled}
            selectedVersion={targetVersion}
          />
        ) : null}

        <ActiveView
          activeTab={activeTab}
          catalog={catalog}
          catalogStatus={catalogStatus}
          selectedGame={selectedGame}
          selectedGameId={selectedGameId}
          onSelectGame={(gameId) => {
            setDetail(null)
            setSelectedGameId(gameId)
            setShowInstallOptions(false)
            const game = catalog.games.find((candidate) => candidate.id === gameId)
            if (game) {
              const latest = game.availableVersions.find((version) => version.latest)?.version ?? game.latestVersion
              setSelectedVersion(latest)
              setInstallRoot(normalizeInstallMetadata(game, game.install).defaultInstallFolder)
              if (game.id !== DEFAULT_GAME_ID) {
                setInstallPath('')
                setScanStatus('No install found')
              }
            }
          }}
          detail={detail}
          assets={assetUrls}
          snapshot={snapshot}
          installPath={installPath}
          installTarget={displayedInstallTarget}
          scanStatus={scanStatus}
          selectedVersion={targetVersion}
          selectedCurrentVersion={selectedCurrentVersion}
          selectedVersionInfo={selectedVersionInfo}
          selectedInstallState={selectedInstallState}
          verifyStatus={selectedVerifyStatus}
          installMode={installMode}
          updateReady={updateReady}
          canUpdate={canUpdate}
          onBrowse={chooseInstallFolder}
          onScan={() => scanFolder()}
          onPrimaryAction={startUpdate}
          onPlay={playSelectedGame}
          onVerify={verifySelectedGame}
          onUninstall={uninstallSelectedGame}
          job={activeJob}
          hasJob={hasVisibleJob}
          progress={progress}
          phaseProgress={phaseProgress}
          updateSize={effectiveDownloadSize}
          isRunning={isRunning}
          onOpenInstallOptions={() => {
            setShowInstallOptions(true)
          }}
          onPause={pauseOrResume}
          onCancel={cancelJob}
          isPaused={isPaused}
          logs={activeJob.logs}
        />
        {showInstallOptions && installMode && selectedGame && detail ? (
          <InstallOptionsDialog
            detail={detail}
            selectedVersion={targetVersion}
            availableVersions={availableVersions}
            versionInfos={selectedGame.availableVersions.length > 0 ? selectedGame.availableVersions : detail.versions}
            downloadSize={effectiveDownloadSize}
            installRoot={installRoot}
            downloadingRoot={downloadPathForInstallRoot(installRoot, gameInstall)}
            onVersionChange={changeTargetVersion}
            onChangeInstallRoot={chooseInstallTarget}
            onStart={startUpdate}
            onClose={() => setShowInstallOptions(false)}
          />
        ) : null}
        {showDrivePicker ? (
          <DriveLibraryPickerModal
            libraries={libraries}
            gameName={selectedGame ? gameFolderName(selectedGame) : '007 First Light'}
            currentRoot={installRoot}
            onSelect={applyLibraryDrive}
            onAddDrive={addLibraryDrive}
            onClose={() => setShowDrivePicker(false)}
          />
        ) : null}
        {launchSplash ? <LaunchSplash splash={launchSplash} /> : null}
      </section>
    </main>
    </div>
  )
}

function LaunchSplash({ splash }: { splash: LaunchSplashState }) {
  return (
    <div className="launch-splash" role="status" aria-live="polite">
      <section className="launch-splash-card">
        {splash.heroUrl ? <img className="launch-splash-hero" src={splash.heroUrl} alt="" /> : null}
        <div className="launch-splash-shade" />
        <div className="launch-splash-content">
          {splash.iconUrl ? <img className="launch-splash-icon" src={splash.iconUrl} alt="" /> : null}
          <div>
            <strong>{splash.title}</strong>
            <span>Chúc bạn chơi game vui vẻ</span>
          </div>
        </div>
      </section>
    </div>
  )
}

function useSmoothNumber(target: number, durationMs = 420) {
  const [value, setValue] = useState(target)
  const valueRef = useRef(target)
  const safeTarget = Number.isFinite(target) ? target : 0

  useEffect(() => {
    const start = valueRef.current
    const end = safeTarget
    if (Math.abs(end - start) < 0.08) {
      valueRef.current = end
      const raf = requestAnimationFrame(() => setValue(end))
      return () => cancelAnimationFrame(raf)
    }

    let raf = 0
    const startedAt = performance.now()
    const tick = (now: number) => {
      const elapsed = Math.min((now - startedAt) / durationMs, 1)
      const eased = 1 - Math.pow(1 - elapsed, 3)
      const next = start + (end - start) * eased
      valueRef.current = next
      setValue(next)
      if (elapsed < 1) {
        raf = requestAnimationFrame(tick)
      } else {
        valueRef.current = end
        setValue(end)
      }
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [durationMs, safeTarget])

  return value
}

function ActiveView({
  activeTab,
  catalog,
  catalogStatus,
  selectedGame,
  selectedGameId,
  onSelectGame,
  detail,
  assets,
  snapshot,
  installPath,
  installTarget,
  scanStatus,
  selectedVersion,
  selectedCurrentVersion,
  selectedVersionInfo,
  selectedInstallState,
  verifyStatus,
  installMode,
  updateReady,
  canUpdate,
  onBrowse,
  onScan,
  onPrimaryAction,
  onPlay,
  onVerify,
  onUninstall,
  job,
  hasJob,
  progress,
  phaseProgress,
  updateSize,
  isRunning,
  onOpenInstallOptions,
  onPause,
  onCancel,
  isPaused,
  logs,
}: {
  activeTab: TabId
  catalog: GameCatalog
  catalogStatus: string
  selectedGame: GameSummary | null
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
  detail: GameDetail | null
  assets: Record<string, string>
  snapshot: Snapshot
  installPath: string
  installTarget: string
  scanStatus: string
  selectedVersion: string
  selectedCurrentVersion: string
  selectedVersionInfo?: GameVersionInfo
  selectedInstallState?: GameInstallState
  verifyStatus: VerifyUiStatus | null
  installMode: boolean
  updateReady: boolean
  canUpdate: boolean
  onBrowse: () => void
  onScan: () => void
  onPrimaryAction: () => void
  onPlay: () => void
  onVerify: () => void
  onUninstall: () => void
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  updateSize: number
  isRunning: boolean
  onOpenInstallOptions: () => void
  onPause: () => void
  onCancel: () => void
  isPaused: boolean
  logs: JobLog[]
}) {
  const hasSelectedDetail = Boolean(selectedGame && detail)

  if (activeTab === 'Library') {
    return (
      <StoreLibraryView
        catalog={catalog}
        catalogStatus={catalogStatus}
        selectedGame={selectedGame}
        selectedGameId={selectedGameId}
        onSelectGame={onSelectGame}
        detail={detail}
        assets={assets}
        selectedVersion={selectedVersion}
        selectedCurrentVersion={selectedCurrentVersion}
        selectedVersionInfo={selectedVersionInfo}
        selectedInstallState={selectedInstallState}
        verifyStatus={verifyStatus}
        updateReady={updateReady}
        canUpdate={canUpdate}
        updateSize={updateSize}
        onPrimaryAction={onPrimaryAction}
        onPlay={onPlay}
        onVerify={onVerify}
        onUninstall={onUninstall}
        onOpenInstallOptions={onOpenInstallOptions}
      />
    )
  }

  if (activeTab === 'Cache') {
    return (
      <section className="single-view cache-tab-view">
        <CachePanel snapshot={snapshot} />
        {selectedGame && detail ? (
          <>
            <RollbackPanel snapshot={snapshot} rollbackVersion={rollbackVersionFor(detail, selectedVersion)} />
            {installMode ? (
              <InstallSummaryPanel selectedVersion={selectedVersion} downloadSize={updateSize} />
            ) : (
              <ChangedFiles files={snapshot.changedFiles} />
            )}
          </>
        ) : (
          <ScopedTabEmptyState
            icon={<Database size={34} />}
            title="No game selected"
            body="Choose a game in Library to inspect rollback and changed-file cache state."
          />
        )}
      </section>
    )
  }

  if (activeTab === 'Downloads' || activeTab === 'Updates') {
    if (!hasSelectedDetail) {
      return (
        <TabEmptyState
          activeTab={activeTab}
          catalog={catalog}
          catalogStatus={catalogStatus}
          onSelectGame={onSelectGame}
          assets={assets}
        />
      )
    }

    return (
      <section className="content-grid single-main">
        <div className="main-column">
          {!installMode || hasJob ? (
              <InstallBar
                installPath={installPath}
                installTarget={installTarget}
                scanStatus={scanStatus}
                installMode={installMode}
              onBrowse={onBrowse}
              onScan={onScan}
            />
          ) : null}
          {activeTab === 'Downloads' ? (
            <DownloadQueuePanel
              gameTitle={selectedGame?.title ?? 'Selected game'}
              job={job}
              hasJob={hasJob}
              progress={progress}
              phaseProgress={phaseProgress}
              selectedVersion={selectedVersion}
              downloadSize={updateSize}
              isRunning={isRunning}
              onOpenOptions={onOpenInstallOptions}
            />
          ) : null}
          {hasJob || activeTab === 'Updates' ? (
            <>
              <JobCenter
                job={job}
                hasJob={hasJob}
                progress={progress}
                phaseProgress={phaseProgress}
                onPause={onPause}
                onCancel={onCancel}
                isPaused={isPaused}
              />
              {hasJob ? <JobLogPanel logs={logs} /> : null}
            </>
          ) : null}
        </div>
      </section>
    )
  }

  return (
    <section className="settings-view">
      <section className="panel settings-panel">
        <header className="side-header">
          <Settings size={17} />
          <strong>SETTINGS</strong>
        </header>
        <div className="settings-card-grid">
          <StatusTile label="Content service" value={contentServiceLabel(snapshot.proxyStatus)} />
          <StatusTile label="Asset catalog" value={catalogStatusLabel(catalogStatus)} />
          <StatusTile label="Selected game" value={selectedGame?.title ?? 'None'} />
          <StatusTile label="Install state" value={snapshot.detectedInstallPath ? 'Installed game detected' : 'Not configured'} />
          <StatusTile label="Target version" value={hasSelectedDetail ? selectedVersion : 'Choose game first'} />
          <StatusTile label="Downloads" value={hasJob ? 'Active job' : 'Idle'} />
        </div>
        <div className="settings-actions">
          <button type="button" onClick={onBrowse} disabled={!hasSelectedDetail}>
            <FolderOpen size={16} />
            LOCATE EXISTING INSTALL
          </button>
          <button type="button" onClick={onScan} disabled={!installPath}>
            <Gauge size={16} />
            SCAN SELECTED FOLDER
          </button>
        </div>
      </section>
    </section>
  )
}

function Sidebar({
  serviceStatus,
  activeTab,
  onSelect,
  updateCount,
  downloadCount,
}: {
  serviceStatus: string
  activeTab: TabId
  onSelect: (tab: TabId) => void
  updateCount: number
  downloadCount: number
}) {
  const items = [
    [t.nav.library, Library],
    [t.nav.updates, RefreshCcw],
    [t.nav.downloads, Download],
    [t.nav.cache, Database],
    [t.nav.settings, Settings],
  ] as const

  return (
    <aside className="sidebar">
      <nav>
        {items.map(([label, Icon]) => (
          <button
            className={activeTab === label ? 'nav-item active' : 'nav-item'}
            key={label}
            type="button"
            onClick={() => onSelect(label)}
          >
            <Icon size={20} />
            <span>{label}</span>
            {label === 'Updates' && updateCount > 0 ? <span className="nav-badge">{updateCount}</span> : null}
            {label === 'Downloads' && downloadCount > 0 ? <span className="nav-badge">{downloadCount}</span> : null}
          </button>
        ))}
      </nav>
      <div className="sidebar-status">
        <div className="status-line">
          <Wifi size={16} />
          <span>Online</span>
        </div>
        <div className="status-line proxy">
          <ShieldCheck size={16} />
          <span>{serviceStatus}</span>
        </div>
        <small>Launcher 0.1.0</small>
      </div>
    </aside>
  )
}

function TabEmptyState({
  activeTab,
  catalog,
  catalogStatus,
  onSelectGame,
  assets,
}: {
  activeTab: TabId
  catalog: GameCatalog
  catalogStatus: string
  onSelectGame: (gameId: string | null) => void
  assets: Record<string, string>
}) {
  return (
    <section className="tab-empty-view">
      <header className="tab-empty-header">
        <div>
          <strong>{activeTab}</strong>
          <span>Select a game to load its jobs and version state.</span>
        </div>
        <small>{catalogStatusLabel(catalogStatus)}</small>
      </header>
      <div className="tab-game-list">
        {catalog.games.map((game) => (
          <button className="tab-game-row" key={game.id} type="button" onClick={() => onSelectGame(game.id)}>
            {assets[game.gridAssetId] ? (
              <img src={assets[game.gridAssetId]} alt="" />
            ) : (
              <div className="tab-game-art">
                <ImageIcon size={22} />
              </div>
            )}
            <span>
              <strong>{game.title}</strong>
              <small>{game.developer}</small>
            </span>
            <Download size={16} />
          </button>
        ))}
      </div>
    </section>
  )
}

function ScopedTabEmptyState({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return (
    <section className="panel scoped-empty-state">
      <div>{icon}</div>
      <strong>{title}</strong>
      <span>{body}</span>
    </section>
  )
}

function StatusTile({ label, value }: { label: string; value: string }) {
  return (
    <article className="settings-tile">
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  )
}

function StoreLibraryView({
  catalog,
  catalogStatus,
  selectedGame,
  selectedGameId,
  onSelectGame,
  detail,
  assets,
  selectedVersion,
  selectedCurrentVersion,
  selectedVersionInfo,
  selectedInstallState,
  verifyStatus,
  updateReady,
  canUpdate,
  updateSize,
  onPrimaryAction,
  onPlay,
  onVerify,
  onUninstall,
  onOpenInstallOptions,
}: {
  catalog: GameCatalog
  catalogStatus: string
  selectedGame: GameSummary | null
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
  detail: GameDetail | null
  assets: Record<string, string>
  selectedVersion: string
  selectedCurrentVersion: string
  selectedVersionInfo?: GameVersionInfo
  selectedInstallState?: GameInstallState
  verifyStatus: VerifyUiStatus | null
  updateReady: boolean
  canUpdate: boolean
  updateSize: number
  onPrimaryAction: () => void
  onPlay: () => void
  onVerify: () => void
  onUninstall: () => void
  onOpenInstallOptions: () => void
}) {
  const [query, setQuery] = useState('')
  const visibleGames = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return catalog.games
    return catalog.games.filter((game) =>
      [game.title, game.subtitle, game.developer, game.publisher].some((value) => value.toLowerCase().includes(needle)),
    )
  }, [catalog.games, query])
  const visibleStatus = catalogStatusLabel(catalogStatus)
  const actionDockRef = useRef<HTMLDivElement>(null)
  const [stickyVisible, setStickyVisible] = useState(false)

  useEffect(() => {
    if (!selectedGameId || !detail?.gameId || typeof IntersectionObserver === 'undefined') {
      return
    }

    const el = actionDockRef.current
    if (!el) {
      return
    }

    const observer = new IntersectionObserver(([entry]) => setStickyVisible(!entry.isIntersecting), {
      threshold: 0,
      rootMargin: '-64px 0px 0px 0px',
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [selectedGameId, detail?.gameId])

  const renderGameCard = (game: GameSummary, variant: 'compact' | 'browse') => (
    <button
      className={[
        'store-game-card',
        variant === 'browse' ? 'browse-game-card' : '',
        game.id === selectedGameId ? 'active' : '',
      ]
        .filter(Boolean)
        .join(' ')}
      key={game.id}
      type="button"
      onClick={() => onSelectGame(game.id)}
    >
      {assets[game.gridAssetId] ? (
        <img src={assets[game.gridAssetId]} alt="" />
      ) : (
        <div className="asset-placeholder">
          <ImageIcon size={variant === 'browse' ? 34 : 26} />
        </div>
      )}
      <span>
        <strong>{game.title}</strong>
        <small>{game.developer}</small>
      </span>
    </button>
  )

  if (!selectedGame) {
    return (
      <section className="library-browse-view">
        <header className="library-browse-toolbar">
          <div className="library-browse-heading">
            <strong>{t.library.availableGames}</strong>
            <span>
              {visibleGames.length} game{visibleGames.length === 1 ? '' : 's'} - {visibleStatus}
            </span>
          </div>
          <label className="store-search">
            <Search size={16} />
            <input aria-label="Search games" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search..." />
          </label>
        </header>

        <div className="library-browse-grid">
          {visibleGames.map((game) => renderGameCard(game, 'browse'))}
          {visibleGames.length === 0 ? (
            <div className="library-empty-inline">
              <Search size={24} />
              <strong>No matching games</strong>
            </div>
          ) : null}
        </div>
      </section>
    )
  }

  if (!detail) {
    return (
      <section className="game-detail-loading-view">
        <button className="back-to-library" type="button" onClick={() => onSelectGame(null)}>
          <Library size={16} />
          Library
        </button>
        <div>
          <ImageIcon size={34} />
          <h1>Loading game details</h1>
          <p>Opening cooked media and version metadata.</p>
        </div>
      </section>
    )
  }

  const hero = firstMediaUrl(detail, assets) || assets[selectedGame.heroAssetId]
  const logo = assets[selectedGame.logoAssetId]
  const installed = Boolean(selectedInstallState?.installed)
  const actionLabel = !installed ? t.library.chooseInstall : updateReady ? t.library.update : t.library.play
  const stateLabel = !installed ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay
  const primaryAction = !installed ? onOpenInstallOptions : updateReady ? onPrimaryAction : onPlay
  const primaryDisabled = !installed && !canUpdate
  const primaryIcon = installed && !updateReady ? <Play size={17} /> : <Download size={17} />
  const downloadSize = updateSize || selectedVersionInfo?.sizeBytes || 0
  const isVerifying = verifyStatus?.state === 'running'
  const verifyLabel = isVerifying ? 'Verifying...' : t.library.verifyIntegrity
  const VerifyIcon = verifyStatus?.state === 'failed' ? CircleAlert : ShieldCheck
  const missingCount = verifyStatus?.missingFiles?.length ?? 0
  const changedCount = verifyStatus?.mismatchedFiles?.length ?? 0

  const gridAsset = assets[selectedGame.gridAssetId]
  const iconAsset = assets[selectedGame.iconAssetId]

  return (
    <section className="game-detail-view">
      {/* ── Sticky Floating Bar ── */}
      <div className={`sticky-action-bar${stickyVisible ? ' visible' : ''}`}>
        {(iconAsset || gridAsset) && (
          <img
            className="sticky-bar-icon"
            src={iconAsset || gridAsset}
            alt=""
          />
        )}
        <div className="sticky-bar-info">
          <strong>{detail.title}</strong>
          <span>v{selectedVersion}</span>
        </div>
        <div className="sticky-bar-actions">
          <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
            <VerifyIcon size={15} />
            {verifyLabel}
          </button>
          <button
            className="primary-control"
            type="button"
            onClick={primaryAction}
            disabled={primaryDisabled}
          >
            {installed && !updateReady ? <Play size={15} /> : <Download size={15} />}
            {actionLabel}
          </button>
          {installed ? (
            <button className="danger-control" type="button" onClick={onUninstall}>
              <X size={15} />
              {t.library.uninstall}
            </button>
          ) : null}
        </div>
      </div>

      <section className="game-detail-main">
        <button className="back-to-library" type="button" onClick={() => onSelectGame(null)}>
          <Library size={16} />
          Library
        </button>
        <div className="detail-hero">
          {hero ? <img src={hero} alt="" /> : <div className="detail-placeholder"><ImageIcon size={40} /></div>}
          <div className="detail-hero-shade" />
          <div className="detail-copy">
            <span className="storage-pill">
              <HardDrive size={14} />
              {detail.install.storageLabel}
            </span>
            {logo ? <img className="detail-logo" src={logo} alt={detail.title} /> : <h1>{detail.title}</h1>}
            <p>{detail.shortDescription}</p>
            <div className="library-meta-row">
              <span>Version {selectedVersion}</span>
              <span>{formatBytes(downloadSize)}</span>
              {detail.install.supportsResume ? <span>{t.library.resumeSupported}</span> : null}
            </div>
          </div>
          <div className="store-action-dock" ref={actionDockRef}>
            <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
              <VerifyIcon size={17} />
              {verifyLabel}
            </button>
            <button className="primary-control" type="button" onClick={primaryAction} disabled={primaryDisabled}>
              {primaryIcon}
              {actionLabel}
            </button>
            {installed ? (
              <button className="danger-control" type="button" onClick={onUninstall}>
                <X size={17} />
                {t.library.uninstall}
              </button>
            ) : null}
          </div>
        </div>
        <MediaRail detail={detail} assets={assets} />

        <section className="detail-body">
          <div className="detail-description">
            <h2>{detail.title}</h2>
            <div
              className="description-html"
              dangerouslySetInnerHTML={{ __html: processDescriptionHtml(detail.detailedDescription, assets) }}
            />
          </div>
        </section>
      </section>

      <aside className="store-info-column">
        <section className="panel status-card">
          <header className="side-header">
            <CheckCircle2 size={17} />
            <strong>{stateLabel}</strong>
          </header>
          <dl className="metric-list">
            <div>
              <dt>{t.library.currentVersion}</dt>
              <dd>{installed ? selectedCurrentVersion : t.library.notInstalled}</dd>
            </div>
            <div>
              <dt>{t.library.latestVersion}</dt>
              <dd>{selectedGame.latestVersion}</dd>
            </div>
            <div>
              <dt>{t.library.targetVersion}</dt>
              <dd>{selectedVersion}</dd>
            </div>
            <div>
              <dt>Install size</dt>
              <dd>{formatBytes(downloadSize)}</dd>
            </div>
          </dl>
        </section>
        <InstallSummaryPanel selectedVersion={selectedVersion} downloadSize={downloadSize} />
        {verifyStatus ? (
          <section className={`panel verify-feedback ${verifyStatus.state}`}>
            <header className="side-header">
              <VerifyIcon size={17} />
              <strong>{isVerifying ? 'Verifying install' : 'Verify result'}</strong>
            </header>
            <p>{verifyStatus.message}</p>
            {verifyStatus.state === 'failed' ? (
              <div className="verify-count-summary">
                <span>
                  <strong>{missingCount}</strong>
                  missing
                </span>
                <span>
                  <strong>{changedCount}</strong>
                  changed
                </span>
              </div>
            ) : null}
            <div className="verify-progress">
              <div className="mini-track">
                <span style={{ width: `${Math.round((verifyStatus.percent ?? 0) * 100)}%` }} />
              </div>
              <small>
                {Math.round((verifyStatus.percent ?? 0) * 100)}%
                {verifyStatus.totalBytes ? ` - ${formatBytes(verifyStatus.checkedBytes ?? 0)} / ${formatBytes(verifyStatus.totalBytes)}` : ''}
              </small>
            </div>
            {verifyStatus.currentFile ? <small className="verify-current-file">{verifyStatus.currentFile}</small> : null}
          </section>
        ) : null}
        <GameDetailsPanel detail={detail} />
        <AchievementPreview achievements={detail.achievements} assets={assets} />
      </aside>
    </section>
  )
}

function OperationHero({
  game,
  detail,
  assets,
  currentVersion,
  latestVersion,
  updateReady,
  updateSize,
  onUpdate,
  onPlay,
  isRunning,
  canUpdate,
  installMode,
  selectedInstalled,
  selectedVersion,
}: {
  game: GameSummary
  detail: GameDetail
  assets: Record<string, string>
  currentVersion: string
  latestVersion: string
  updateReady: boolean
  updateSize: number
  onUpdate: () => void
  onPlay: () => void
  isRunning: boolean
  canUpdate: boolean
  installMode: boolean
  selectedInstalled: boolean
  selectedVersion: string
}) {
  const hero = firstMediaUrl(detail, assets) || assets[game.heroAssetId]
  const stateLabel = installMode ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay
  const buttonLabel = isRunning ? 'RUNNING' : installMode ? t.library.chooseInstall : updateReady ? t.library.update : t.library.play
  const buttonAction = selectedInstalled && !updateReady ? onPlay : onUpdate
  const buttonDisabled = isRunning || (!selectedInstalled && !canUpdate)

  return (
    <section className="hero-panel">
      {hero ? <img src={hero} alt="" /> : null}
      <div className="game-strip">
        <div className="game-emblem">
          {assets[game.iconAssetId] ? <img src={assets[game.iconAssetId]} alt="" /> : <ImageIcon size={28} />}
        </div>
        <div>
          <h1>{game.title}</h1>
          <div className="version-row">
            <VersionStat label={t.library.currentVersion} value={currentVersion} />
            <VersionStat label={t.library.latestVersion} value={latestVersion} highlight />
            <VersionStat label={t.library.targetVersion} value={selectedVersion} />
            <div className="ready-state">
              <CheckCircle2 size={20} />
              <span>{stateLabel}</span>
              <small>{formatBytes(updateSize)}</small>
            </div>
          </div>
        </div>
        <button className="update-button" type="button" onClick={buttonAction} disabled={buttonDisabled}>
          <span>{buttonLabel}</span>
          {selectedInstalled && !updateReady ? <Play size={18} /> : <Download size={18} />}
        </button>
      </div>
    </section>
  )
}

function VersionStat({ label, value, highlight = false }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="version-stat">
      <small>{label}</small>
      <strong className={highlight ? 'gold-text' : ''}>{value}</strong>
    </div>
  )
}

function MediaRail({ detail, assets }: { detail: GameDetail; assets: Record<string, string> }) {
  // Build a thumb map: video item id -> thumbnail URL
  // e.g. "movie-00" -> URL from item with id "movie-thumb-00"
  const videoThumbMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (const item of detail.media) {
      if (item.role === 'video-thumb' && assets[item.assetId]) {
        // item.id is like "movie-thumb-00", derive video id "movie-00"
        const videoId = item.id.replace('movie-thumb-', 'movie-')
        map[videoId] = assets[item.assetId]
      }
    }
    return map
  }, [detail.media, assets])

  const media = detail.media
    .filter((item) => isCarouselMedia(item) && assets[item.assetId])
    .sort((left, right) => mediaPriority(left) - mediaPriority(right))
    .map((item) => ({ ...item, url: assets[item.assetId] }))
  const [activeIndex, setActiveIndex] = useState(0)

  if (media.length === 0) {
    return null
  }
  const safeActiveIndex = Math.min(activeIndex, media.length - 1)
  const active = media[safeActiveIndex]
  const go = (direction: -1 | 1) => {
    setActiveIndex((current) => (current + direction + media.length) % media.length)
  }

  return (
    <section className="media-section media-carousel-section">
      <header>
        <strong>{t.library.media}</strong>
        <small>
          {media.length} items - {detail.metadataSource}
        </small>
      </header>
      <div className="media-carousel">
        <div className="media-stage">
          {active.mimeType.startsWith('video/') ? (
            <video src={active.url} controls muted preload="metadata" />
          ) : (
            <>
              <img src={active.url} alt="" />
              {active.role === 'video-preview' ? (
                <span className="media-play-badge" aria-hidden="true">
                  <Play size={22} />
                </span>
              ) : null}
            </>
          )}
          <button className="media-nav prev" type="button" onClick={() => go(-1)} aria-label="Previous media">
            <ChevronLeft size={22} />
          </button>
          <button className="media-nav next" type="button" onClick={() => go(1)} aria-label="Next media">
            <ChevronRight size={22} />
          </button>
          <div className="media-stage-caption">
            <strong>{active.title}</strong>
            <span>{active.role}</span>
          </div>
        </div>
        <div className="media-thumb-rail">
          {media.map((item, index) => {
            const isVideo = item.mimeType.startsWith('video/')
            const thumbUrl = isVideo ? (videoThumbMap[item.id] ?? null) : null

            return (
              <button
                className={index === safeActiveIndex ? 'media-thumb active' : 'media-thumb'}
                key={item.id}
                type="button"
                onClick={() => setActiveIndex(index)}
              >
                {isVideo ? (
                  <span className="image-video-thumb">
                    {thumbUrl ? (
                      <img src={thumbUrl} alt="" />
                    ) : (
                      <span className="video-thumb-placeholder"><Play size={24} /></span>
                    )}
                    <Play size={16} className="video-thumb-overlay" />
                  </span>
                ) : (
                  <img src={item.url} alt="" />
                )}
              </button>
            )
          })}
        </div>
      </div>
      <div className="media-rail legacy-hidden">
        {media.map((item) => (
          <article key={item.id}>
            {item.mimeType.startsWith('video/') ? (
              <video src={item.url} muted controls />
            ) : (
              <img src={item.url} alt="" />
            )}
            <span>{item.title}</span>
          </article>
        ))}
      </div>
    </section>
  )
}

function isCarouselMedia(item: GameMedia) {
  return item.role === 'video' || item.role === 'video-preview' || item.role === 'screenshot' || item.role === 'gif'
}

function mediaPriority(item: GameMedia) {
  switch (item.role) {
    case 'video':
      return 0
    case 'video-preview':
      return 1
    case 'screenshot':
      return 2
    case 'gif':
      return 3
    default:
      return 99
  }
}

function AchievementPreview({
  achievements,
  assets,
}: {
  achievements: GameAchievement[]
  assets: Record<string, string>
}) {
  const [showAll, setShowAll] = useState(false)
  const available = achievements.filter((achievement) => assets[achievement.iconAssetId])
  const preview = available.slice(0, 10)
  if (available.length === 0) {
    return null
  }

  return (
    <section className="achievement-section">
      <header>
        <strong>{t.library.achievements}</strong>
        <div className="achievement-header-actions">
          <small>{achievements.length} total</small>
          <button type="button" onClick={() => setShowAll(true)}>
            <Trophy size={15} />
            See all
          </button>
        </div>
      </header>
      <div className="achievement-grid">
        {preview.map((achievement) => (
          <article key={achievement.id}>
            <img src={assets[achievement.iconAssetId]} alt="" />
            <div>
              <strong>{achievement.name}</strong>
              <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
            </div>
          </article>
        ))}
      </div>
      {showAll ? (
        <div className="dialog-backdrop" role="presentation" onClick={() => setShowAll(false)}>
          <section className="achievement-modal" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <header>
              <div>
                <strong>{t.library.achievements}</strong>
                <span>{available.length} unlocked-image entries packed locally</span>
              </div>
              <button type="button" onClick={() => setShowAll(false)}>
                <X size={17} />
              </button>
            </header>
            <div className="achievement-all-grid">
              {available.map((achievement) => (
                <article key={achievement.id}>
                  <img src={assets[achievement.iconAssetId]} alt="" />
                  <div>
                    <strong>{achievement.name}</strong>
                    <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
                  </div>
                </article>
              ))}
            </div>
          </section>
        </div>
      ) : null}
    </section>
  )
}

function InstallBar({
  installPath,
  installTarget,
  scanStatus,
  installMode,
  onBrowse,
  onScan,
}: {
  installPath: string
  installTarget: string
  scanStatus: string
  installMode: boolean
  onBrowse: () => void
  onScan: () => void
}) {
  const label = installMode ? 'Install target' : 'Installed folder'
  const path = installMode ? installTarget : installPath || 'No installed folder selected'

  return (
    <section className={installMode ? 'install-bar install-mode' : 'install-bar'}>
      <div className="install-path">
        <FolderOpen size={18} />
        <div>
          <small>{label}</small>
          <span>{path}</span>
        </div>
      </div>
      <span className="scan-status">{scanStatus}</span>
      {!installMode ? (
        <>
          <button type="button" onClick={onScan} disabled={!installPath}>
            <Gauge size={16} />
            Scan
          </button>
          <button type="button" onClick={onBrowse}>
            <FolderOpen size={16} />
            Browse
          </button>
        </>
      ) : null}
    </section>
  )
}

function InstallOptionsDialog({
  detail,
  selectedVersion,
  availableVersions,
  versionInfos,
  downloadSize,
  installRoot,
  downloadingRoot,
  onVersionChange,
  onChangeInstallRoot,
  onStart,
  onClose,
}: {
  detail: GameDetail
  selectedVersion: string
  availableVersions: string[]
  versionInfos: GameVersionInfo[]
  downloadSize: number
  installRoot: string
  downloadingRoot: string
  onVersionChange: (version: string) => void
  onChangeInstallRoot: () => void
  onStart: () => void
  onClose: () => void
}) {
  const [versionMenuOpen, setVersionMenuOpen] = useState(false)
  const infos =
    versionInfos.length > 0
      ? versionInfos
      : availableVersions.map((version) => ({
          version,
          label: version,
          buildId: version,
          sizeBytes: downloadSize,
          latest: version === availableVersions[availableVersions.length - 1],
        }))
  const selectedInfo = infos.find((info) => info.version === selectedVersion) ?? infos[0]

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="install-modal" role="dialog" aria-modal="true" aria-labelledby="install-options-title">
        <div className="modal-handle" />
        <header>
          <button type="button" onClick={onClose} aria-label="Close install options">
            <X size={17} />
          </button>
          <h2 id="install-options-title">{t.install.title}</h2>
          <p>{t.install.subtitle}</p>
        </header>
        <div className="install-modal-body">
          <div className={versionMenuOpen ? 'version-dropdown open' : 'version-dropdown'}>
            <small>{t.install.version}</small>
            <button
              className="version-dropdown-trigger"
              type="button"
              aria-haspopup="listbox"
              aria-expanded={versionMenuOpen}
              onClick={() => setVersionMenuOpen((open) => !open)}
            >
              <span>
                <strong>{selectedInfo?.label || selectedVersion}</strong>
                <small>Build {selectedInfo?.buildId || selectedVersion}</small>
              </span>
              {selectedInfo?.latest ? <em>{t.install.latest}</em> : null}
              <ChevronDown size={17} />
            </button>
            {versionMenuOpen ? (
              <div className="version-dropdown-menu" role="listbox" aria-label="Choose install version">
                {infos.map((info) => (
                  <button
                    className={info.version === selectedVersion ? 'version-dropdown-option active' : 'version-dropdown-option'}
                    key={info.version}
                    type="button"
                    role="option"
                    aria-selected={info.version === selectedVersion}
                    onClick={() => {
                      onVersionChange(info.version)
                      setVersionMenuOpen(false)
                    }}
                  >
                    <CheckCircle2 size={17} />
                    <span>
                      <strong>{info.label || info.version}</strong>
                      <small>Build {info.buildId}</small>
                    </span>
                    {info.latest ? <em>{t.install.latest}</em> : null}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          <div className="install-options-grid">
            <div>
              <small>{t.install.version}</small>
              <strong>{selectedVersion}</strong>
            </div>
            <div>
              <small>{t.install.downloadSize}</small>
              <strong>{formatBytes(downloadSize)}</strong>
            </div>
            <div>
              <small>{t.install.resumeBehavior}</small>
              <strong>{t.install.journalCache}</strong>
            </div>
            <div>
              <small>Game</small>
              <strong>{detail.title}</strong>
            </div>
            <div className="wide-option">
              <small>{t.install.installFolder}</small>
              <strong>{installRoot}</strong>
              <button type="button" onClick={onChangeInstallRoot}>
                <FolderOpen size={16} />
                {t.install.change}
              </button>
            </div>
            <div className="wide-option">
              <small>{t.install.downloadingFolder}</small>
              <strong>{downloadingRoot}</strong>
            </div>
          </div>
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t.install.cancel}
          </button>
          <button className="primary-control" type="button" onClick={onStart}>
            <Download size={17} />
            {t.install.startDownload}
          </button>
        </footer>
      </section>
    </div>
  )
}

function DownloadQueuePanel({
  gameTitle,
  job,
  hasJob,
  progress,
  phaseProgress,
  selectedVersion,
  downloadSize,
  isRunning,
  onOpenOptions,
}: {
  gameTitle: string
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  selectedVersion: string
  downloadSize: number
  isRunning: boolean
  onOpenOptions: () => void
}) {
  const displayProgress = useSmoothNumber(progress)

  if (!hasJob) {
    return (
      <section className="panel download-queue-panel">
        <header className="panel-header compact">
          <strong>DOWNLOADS</strong>
          <span>No queued downloads</span>
        </header>
        <div className="downloads-empty">
          <div className="queue-art">
            <Download size={19} />
          </div>
          <div>
            <strong>No active download</strong>
            <span>
              {gameTitle} {selectedVersion} is available, {formatBytes(downloadSize)} required.
            </span>
          </div>
          <button type="button" onClick={onOpenOptions}>
            {t.library.chooseInstall}
          </button>
        </div>
      </section>
    )
  }

  const queuedLabel = `${job.kind === 'install' ? 'Install' : 'Update'} ${job.toVersion}`
  const failed = job.status === 'failed'
  const canceled = job.status === 'canceled'

  return (
    <section className="panel download-queue-panel">
      <header className="panel-header compact">
        <strong>DOWNLOADS</strong>
        <span>{failed ? 'Download failed' : canceled ? 'Download canceled' : 'Active queue'}</span>
      </header>
      <article className={failed ? 'queue-row failed' : 'queue-row active'}>
        <div className="queue-art">
          {failed ? <CircleAlert size={19} /> : <Download size={19} />}
        </div>
        <div className="queue-copy">
          <strong>{gameTitle}</strong>
          <span>{queuedLabel}</span>
          <small>
            {failed ? 'Network error. Resume will reuse staged chunks.' : `${phaseProgress.name} - ${phaseProgress.detail}`}
          </small>
        </div>
        <div className="queue-progress">
          <div className="mini-track">
            <span style={{ width: `${displayProgress}%` }} />
          </div>
          <div className="queue-transfer">
            <span>{displayProgress.toFixed(1)}%</span>
            <span>
              {formatBytes(phaseProgress.bytesDone)} / {formatBytes(phaseProgress.bytesTotal)}
            </span>
            <span>{phaseProgress.isDownloading ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : 'Phase progress'}</span>
          </div>
        </div>
        {failed || canceled ? (
          <button type="button" onClick={onOpenOptions}>
            {failed ? 'Resume' : t.library.chooseInstall}
          </button>
        ) : (
          <span className={isRunning ? 'queue-pill running' : 'queue-pill'}>{isRunning ? 'DOWNLOADING' : job.status}</span>
        )}
      </article>
    </section>
  )
}

function JobCenter({
  job,
  hasJob,
  progress,
  phaseProgress,
  onPause,
  onCancel,
  isPaused,
}: {
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  onPause: () => void
  onCancel: () => void
  isPaused: boolean
}) {
  const displayProgress = useSmoothNumber(progress)
  const displayOverall = useSmoothNumber(phaseProgress.overallPercent)
  const canControl = hasJob && ['running', 'downloading', 'assembling', 'paused'].includes(job.status)
  const jobTitle = job.kind === 'install' ? `INSTALL JOB: ${job.toVersion}` : `UPDATE JOB: ${job.fromVersion} -> ${job.toVersion}`

  return (
    <section className="panel job-panel">
      <header className="panel-header">
        <div>
          <strong>{hasJob ? jobTitle : t.jobs.noActiveJob}</strong>
          <span>{hasJob ? `${phaseProgress.name} - ${phaseProgress.detail}` : t.jobs.chooseVersion}</span>
        </div>
        <div className="progress-summary">
          <span>Current phase</span>
          <strong>{displayProgress.toFixed(1)}%</strong>
          <span>Overall {displayOverall.toFixed(1)}%</span>
        </div>
      </header>
      <div className="track">
        <span style={{ width: `${displayProgress}%` }} />
      </div>
      <div className="phase-transfer-row">
        <span>
          Downloaded <strong>{formatBytes(phaseProgress.bytesDone)}</strong> / {formatBytes(phaseProgress.bytesTotal)}
        </span>
        <span>
          Speed <strong>{phaseProgress.isDownloading ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : '--'}</strong>
        </span>
        <span>
          ETA <strong>{formatDuration(phaseProgress.etaSeconds)}</strong>
        </span>
      </div>
      <div className="steps">
        {job.steps.map((step, index) => (
          <StepRow key={step.name} index={index + 1} step={step} />
        ))}
      </div>
      <footer className="job-actions">
        {canControl ? (
          <>
            <button className="primary-control" type="button" onClick={onPause}>
              {isPaused ? <Play size={17} /> : <Pause size={17} />}
              {isPaused ? t.jobs.resume : t.jobs.pause}
            </button>
            <button type="button" onClick={onCancel}>
              <X size={17} />
              {t.jobs.cancel}
            </button>
            <span className="resume-state">{t.jobs.resumable}</span>
          </>
        ) : (
          <span className="resume-state idle">No running download, assemble, or repair job.</span>
        )}
      </footer>
    </section>
  )
}

function StepRow({ index, step }: { index: number; step: JobStep }) {
  const displayProgress = useSmoothNumber(step.progress * 100)
  const Icon = useMemo(() => {
    if (step.status === 'completed') return CheckCircle2
    if (step.status === 'failed') return CircleAlert
    if (step.name.includes('Download')) return Download
    if (step.name.includes('Verify')) return ShieldCheck
    if (step.name.includes('Assemble')) return Archive
    return TerminalSquare
  }, [step.name, step.status])

  return (
    <article className={`step-row ${step.status}`}>
      <div className="step-icon">
        <Icon size={21} />
      </div>
      <span className="step-index">{index}</span>
      <div className="step-copy">
        <strong>{step.name}</strong>
        <small>{step.detail}</small>
      </div>
      <div className="mini-track">
        <span style={{ width: `${displayProgress}%` }} />
      </div>
      <strong className="step-percent">{Math.round(displayProgress)}%</strong>
      <span className="retry-count">{step.retryCount} retry</span>
    </article>
  )
}

function CachePanel({ snapshot }: { snapshot: Snapshot }) {
  const radius = 38
  const circumference = 2 * Math.PI * radius
  const offset = circumference - (circumference * snapshot.cache.healthPercent) / 100

  return (
    <section className="panel metric-panel">
      <header className="side-header">
        <HardDrive size={17} />
        <strong>CACHE HEALTH</strong>
      </header>
      <div className="cache-meter">
        <svg viewBox="0 0 96 96" aria-hidden="true">
          <circle cx="48" cy="48" r={radius} />
          <circle cx="48" cy="48" r={radius} style={{ strokeDasharray: circumference, strokeDashoffset: offset }} />
        </svg>
        <strong>{snapshot.cache.healthPercent}%</strong>
      </div>
      <dl className="metric-list">
        <div>
          <dt>Cache size</dt>
          <dd>{formatBytes(snapshot.cache.cacheSize)}</dd>
        </div>
        <div>
          <dt>Free space</dt>
          <dd>{formatBytes(snapshot.cache.freeSpace)}</dd>
        </div>
      </dl>
      <button type="button" disabled={snapshot.cache.cacheSize === 0}>
        {snapshot.cache.cacheSize === 0 ? 'NO CACHE ITEMS' : 'MANAGE CACHE'}
      </button>
    </section>
  )
}

function RollbackPanel({ snapshot, rollbackVersion }: { snapshot: Snapshot; rollbackVersion: string }) {
  const rollbackKnown = snapshot.cache.rollbackReady || snapshot.cache.rollbackMissingBytes > 0

  return (
    <section className="panel rollback-panel">
      <header className="side-header">
        <RotateCcw size={17} />
        <strong>ROLLBACK READINESS</strong>
      </header>
      <div className="rollback-state">
        <span className={snapshot.cache.rollbackReady ? 'ready-pill' : 'warn-pill'}>
          {snapshot.cache.rollbackReady ? 'READY' : rollbackKnown ? 'NEEDS DOWNLOAD' : 'NOT PREPARED'}
        </span>
        <div>
          <strong>{snapshot.cache.rollbackReady ? `${rollbackVersion} rollback ready` : 'Rollback not staged'}</strong>
          <small>
            {snapshot.cache.rollbackReady
              ? 'All required chunks are cached.'
              : rollbackKnown
                ? `${formatBytes(snapshot.cache.rollbackMissingBytes)} required from proxy.`
                : 'Run verify/cache analysis before rollback.'}
          </small>
        </div>
      </div>
      <button type="button" disabled={!snapshot.cache.rollbackReady}>
        ROLLBACK TO {rollbackVersion}
      </button>
    </section>
  )
}

function GameDetailsPanel({ detail }: { detail: GameDetail }) {
  return (
    <section className="panel game-info-panel">
      <header className="side-header">
        <ShieldCheck size={17} />
        <strong>{t.library.details}</strong>
      </header>
      <dl className="game-info-list">
        <div>
          <dt>Developer</dt>
          <dd>{detail.developers.join(', ')}</dd>
        </div>
        <div>
          <dt>Publisher</dt>
          <dd>{detail.publishers.join(', ')}</dd>
        </div>
        <div>
          <dt>Release date</dt>
          <dd>{detail.releaseDate}</dd>
        </div>
        <div>
          <dt>Genres</dt>
          <dd>
            {detail.genres.map((genre) => (
              <span key={genre}>{genre}</span>
            ))}
          </dd>
        </div>
      </dl>
      {detail.ratings.map((rating) => (
        <div className="rating-strip" key={rating.source}>
          <strong>{rating.score}</strong>
          <span>{rating.source}</span>
        </div>
      ))}
    </section>
  )
}

function InstallSummaryPanel({
  selectedVersion,
  downloadSize,
}: {
  selectedVersion: string
  downloadSize: number
}) {
  return (
    <section className="panel install-summary-panel">
      <header className="side-header">
        <Download size={17} />
        <strong>{t.library.install}</strong>
      </header>
      <dl className="metric-list">
        <div>
          <dt>Version</dt>
          <dd>{selectedVersion}</dd>
        </div>
        <div>
          <dt>Install size</dt>
          <dd>{formatBytes(downloadSize)}</dd>
        </div>
      </dl>
    </section>
  )
}

function ChangedFiles({ files }: { files: ChangedFile[] }) {
  return (
    <section className="panel changed-panel">
      <header className="side-header">
        <Square size={17} />
        <strong>CHANGED FILES ({files.length})</strong>
      </header>
      <div className="changed-list">
        {files.map((file) => (
          <article key={file.path}>
            <Archive size={18} />
            <div>
              <strong>{file.path}</strong>
              <small>
                {formatBytes(file.oldSize)} {'->'} {formatBytes(file.newSize)}
              </small>
            </div>
            <span>{formatDelta(file.newSize - file.oldSize)}</span>
          </article>
        ))}
      </div>
      <button type="button" disabled={files.length === 0}>
        {files.length === 0 ? 'NO CHANGED FILES' : 'VIEW ALL FILES'}
      </button>
    </section>
  )
}

function JobLogPanel({ logs }: { logs: JobLog[] }) {
  return (
    <section className="panel log-panel">
      <header className="panel-header compact">
        <strong>JOB LOG</strong>
        <button type="button">CLEAR</button>
      </header>
      <div className="log-list">
        {logs.slice(-7).map((log, index) => (
          <div className={`log-row ${log.level}`} key={`${log.at}-${index}`}>
            <span>[{log.at}]</span>
            <CheckCircle2 size={15} />
            <p>{log.message}</p>
          </div>
        ))}
      </div>
    </section>
  )
}

function createIdleJob(snapshot: Snapshot): JobJournal {
  return {
    id: 'idle',
    gameId: DEFAULT_GAME_ID,
    kind: 'update',
    status: 'planned',
    installPath: '',
    fromVersion: snapshot.currentVersion,
    toVersion: snapshot.latestVersion,
    phase: 'Ready',
    overallProgress: 0,
    bytesDone: 0,
    bytesTotal: snapshot.updateSize,
    retryCount: 0,
    resumable: true,
    updatedAt: new Date().toISOString(),
    steps: [
      { name: 'Scan', detail: 'Find local files and detect version', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Verify', detail: 'Hash manifest-owned files', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Download packs', detail: 'Resume missing byte ranges from proxy', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Assemble files', detail: 'Rebuild files into verified temp outputs', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Finalize', detail: 'Replace only after full-file hash match', status: 'waiting', progress: 0, retryCount: 0 },
    ],
    logs: [
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'No launcher job is running.' },
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'Select a target version or scan an existing install.' },
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'Install uses 0xoLemon store; update uses the selected game folder.' },
    ],
  }
}

function versionOptions(snapshot: Snapshot, game: GameSummary, useSnapshot: boolean) {
  if (useSnapshot && snapshot.availableVersions.length > 0) {
    return snapshot.availableVersions
  }
  if (game.availableVersions.length > 0) {
    return game.availableVersions.map((version) => version.version)
  }
  return snapshot.latestVersion === 'unknown' ? [] : [snapshot.latestVersion]
}

function collectAssetIds(game: GameSummary, detail: GameDetail) {
  return Array.from(
    new Set(
      [
        game.gridAssetId,
        game.heroAssetId,
        game.logoAssetId,
        game.iconAssetId,
        ...detail.media.slice(0, 24).map((item) => item.assetId),
        ...detail.achievements.map((achievement) => achievement.iconAssetId),
        ...(detail.descriptionImages || []),
      ].filter(Boolean),
    ),
  )
}

function fallbackDetailFromSummary(game: GameSummary): GameDetail {
  return {
    gameId: game.id,
    locale: 'en-US',
    title: game.title,
    shortDescription: game.subtitle || 'Game details are packaged for the desktop launcher.',
    detailedDescription:
      'Open the desktop launcher build to load the cooked .0xo media pack, Steam detail metadata, achievements, version data, and install workflow.',
    developers: [game.developer].filter(Boolean),
    publishers: [game.publisher].filter(Boolean),
    releaseDate: 'Pack metadata required',
    genres: [],
    categories: [],
    ratings: [],
    media: [],
    achievements: [],
    sounds: [],
    install: game.install,
    descriptionImages: [],
    versions: game.availableVersions,
    metadataSource: 'preview',
  }
}

function firstMediaUrl(detail: GameDetail, assets: Record<string, string>) {
  const first = detail.media.find((item) => isCarouselMedia(item) && item.mimeType.startsWith('image/') && assets[item.assetId])
  return first ? assets[first.assetId] : undefined
}

function processDescriptionHtml(html: string, assets: Record<string, string>) {
  if (!html) return '<p>No description available.</p>';
  let processed = html;

  // Replace asset tokens with base64 data URLs - asset IDs contain colons e.g. "007-first-light:desc-img-0"
  processed = processed.replace(/asset:([a-zA-Z0-9_:/-]+)/g, (_match, assetId) => {
    return assets[assetId] ? assets[assetId] : '';
  });

  // Clean up Steam-specific tags into readable HTML
  processed = processed
    .replace(/\[h[123]\](.*?)\[\/h[123]\]/gi, '<h3>$1</h3>')
    .replace(/\[b\](.*?)\[\/b\]/gi, '<strong>$1</strong>')
    .replace(/\[i\](.*?)\[\/i\]/gi, '<em>$1</em>')
    .replace(/\[u\](.*?)\[\/u\]/gi, '<u>$1</u>')
    .replace(/\[url=([^\]]+)\](.*?)\[\/url\]/gi, '<a href="$1" target="_blank">$2</a>')
    .replace(/\[list\]([\s\S]*?)\[\/list\]/gi, '<ul>$1</ul>')
    .replace(/\[\*\]/gi, '<li>')
    .replace(/\[img\](.*?)\[\/img\]/gi, '')  // remove raw steam image tags
    .replace(/<img[^>]+src="(?!data:)[^"]*"[^>]*>/gi, ''); // strip unresolved external img

  return processed;
}

function downloadPathForInstallRoot(root: string, install: GameInstallMetadata = fallbackInstall) {
  return root === install.defaultInstallFolder ? install.defaultDownloadingFolder : `${root}\\${CUSTOM_DOWNLOADING_RELATIVE}`
}

function isTauriRuntime() {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__)
}

function contentServiceLabel(value: string) {
  const normalized = value.toLowerCase()
  if (normalized.includes('missing') || normalized.includes('failed') || normalized.includes('error')) {
    return 'Content service unavailable'
  }
  if (normalized.includes('ready') || normalized.includes('local') || normalized.includes('auth')) {
    return 'Content service ready'
  }
  return 'Content service checking'
}

function catalogStatusLabel(value: string) {
  const normalized = value.toLowerCase()
  if (normalized.includes('loaded')) return 'Asset pack loaded'
  if (normalized.includes('fallback') || normalized.includes('preview')) return 'Preview metadata'
  if (normalized.includes('failed') || normalized.includes('error') || normalized.includes('invalid')) return 'Asset pack unavailable'
  return 'Asset pack checking'
}

function rollbackVersionFor(detail: GameDetail, selectedVersion: string) {
  const versions = detail.versions.map((item) => item.version)
  const selectedIndex = versions.indexOf(selectedVersion)
  if (selectedIndex > 0) return versions[selectedIndex - 1]
  return detail.versions.find((item) => !item.latest)?.version ?? 'previous version'
}

function getPhaseProgress(job: JobJournal, rateBytesPerSecond: number): PhaseProgress {
  const runningStep =
    job.steps.find((step) => step.status === 'running' || step.status === 'paused') ??
    job.steps.find((step) => step.status !== 'completed') ??
    job.steps[job.steps.length - 1]
  const isDownloading = job.status === 'downloading'
  const phasePercent = isDownloading
    ? bytePercent(job.bytesDone, job.bytesTotal)
    : clampPercent((runningStep?.progress ?? job.overallProgress) * 100)
  const remainingBytes = Math.max(job.bytesTotal - job.bytesDone, 0)

  return {
    name: runningStep?.name ?? job.phase,
    detail: job.phase,
    percent: job.status === 'committed' ? 100 : phasePercent,
    overallPercent: clampPercent(job.overallProgress * 100),
    bytesDone: job.bytesDone,
    bytesTotal: job.bytesTotal,
    rateBytesPerSecond,
    etaSeconds: isDownloading && rateBytesPerSecond > 1 ? remainingBytes / rateBytesPerSecond : null,
    isDownloading,
  }
}

function bytePercent(done: number, total: number) {
  if (total <= 0) return 0
  return clampPercent((done / total) * 100)
}

function clampPercent(value: number) {
  return Math.min(Math.max(value, 0), 100)
}

function formatBytes(value: number) {
  if (value <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  return `${(value / 1024 ** index).toFixed(index === 0 ? 0 : 2)} ${units[index]}`
}

function formatDuration(seconds: number | null) {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return '--'
  const rounded = Math.max(1, Math.round(seconds))
  const hours = Math.floor(rounded / 3600)
  const minutes = Math.floor((rounded % 3600) / 60)
  const secs = rounded % 60
  if (hours > 0) return `${hours}h ${minutes}m`
  if (minutes > 0) return `${minutes}m ${secs}s`
  return `${secs}s`
}

function formatDelta(value: number) {
  const sign = value >= 0 ? '+' : '-'
  return `${sign}${formatBytes(Math.abs(value))}`
}

function DriveLibraryPickerModal({
  libraries,
  gameName,
  currentRoot,
  onSelect,
  onAddDrive,
  onClose,
}: {
  libraries: string[]
  gameName: string
  currentRoot: string
  onSelect: (driveLetter: string) => void
  onAddDrive: () => void
  onClose: () => void
}) {
  type DriveInfo = { letter: string; label: string; free_bytes: number; total_bytes: number }
  const [driveInfos, setDriveInfos] = useState<Record<string, DriveInfo>>({})

  useEffect(() => {
    invoke<DriveInfo[]>('list_system_drives')
      .then((drives) => {
        const map: Record<string, DriveInfo> = {}
        for (const d of drives) map[d.letter] = d
        setDriveInfos(map)
      })
      .catch(() => {/* ignore if not in tauri */})
  }, [])

  return (
    <div className="dialog-backdrop" role="presentation" onClick={(e) => { if (e.target === e.currentTarget) onClose() }}>
      <section className="drive-picker-modal" role="dialog" aria-modal="true" aria-label="Choose install library">
        <header>
          <h2>Choose Install Location</h2>
          <button type="button" onClick={onClose} aria-label="Close"><X size={17} /></button>
        </header>
        <p className="drive-picker-hint">
          Game will be installed to: <code>Drive:\0xoLemon store\common\{gameName}</code>
        </p>
        <div className="drive-list">
          {libraries.map((lib) => {
            const info = driveInfos[lib]
            const isSelected = currentRoot.toUpperCase().startsWith(lib.toUpperCase())
            const freeGB = info ? (info.free_bytes / 1024 / 1024 / 1024).toFixed(1) : null
            const totalGB = info ? (info.total_bytes / 1024 / 1024 / 1024).toFixed(0) : null
            const usedPct = info ? Math.round(((info.total_bytes - info.free_bytes) / info.total_bytes) * 100) : 0

            return (
              <button
                key={lib}
                className={`drive-entry${isSelected ? ' selected' : ''}`}
                type="button"
                onClick={() => onSelect(lib)}
              >
                <div className="drive-icon">
                  <HardDrive size={28} />
                </div>
                <div className="drive-details">
                  <div className="drive-label">
                    <strong>{lib}</strong>
                    {info ? <span>{info.label}</span> : null}
                  </div>
                  {info ? (
                    <>
                      <div className="drive-space-bar">
                        <div className="drive-space-fill" style={{ width: `${usedPct}%` }} />
                      </div>
                      <div className="drive-space-text">
                        {freeGB} GB free of {totalGB} GB
                      </div>
                    </>
                  ) : (
                    <div className="drive-space-text muted">Checking…</div>
                  )}
                  <div className="drive-path-preview">
                    {lib}\0xoLemon store\common\{gameName}
                  </div>
                </div>
                {isSelected && <div className="drive-check"><Check size={16} /></div>}
              </button>
            )
          })}
        </div>
        <footer>
          <button type="button" className="add-drive-btn" onClick={onAddDrive}>
            <Plus size={15} /> Add Drive
          </button>
        </footer>
      </section>
    </div>
  )
}

export default App
