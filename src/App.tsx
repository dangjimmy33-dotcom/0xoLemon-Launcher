import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { CircleAlert, Download, X } from 'lucide-react'
import './App.css'
import type {
  AssetBlob,
  CloudSaveRoot,
  CloudSaveStatus,
  GameCatalog,
  GameDetail,
  GameInstallState,
  GameSummary,
  JobJournal,
  LaunchReport,
  LauncherUpdateInfo,
  LauncherUpdateProgress,
  LauncherSettings,
  LaunchSplashState,
  ResolvedGameLaunchConfig,
  ShortcutLaunchPayload,
  Snapshot,
  SteamEnvironmentInfo,
  TabId,
  UninstallReport,
  VerifyInstallReport,
  VerifyProgressPayload,
  VerifyUiStatus,
} from './types'
import installCompleteSoundUrl from './assets/sounds/desktop_toast_default.wav?url'
import { DEFAULT_GAME_ID, DEFAULT_STORE_ROOT, fallbackCatalog, fallbackInstall, fallbackSnapshot, gameFolderName, installMetadataForStoreRoot } from './lib/installPaths'
import { collectAssetIds, contentServiceLabel, downloadPathForInstallRoot, fallbackDetailFromSummary, firstMediaUrl, isTauriRuntime, versionOptions } from './lib/gameMeta'
import { createIdleJob, getPhaseProgress } from './lib/jobProgress'
import { gameHasTag } from './lib/gameTags'
import { DEFAULT_LAUNCHER_PREFERENCES, loadLauncherPreferences, saveLauncherPreferences, type LauncherPreferences } from './lib/preferences'
import { ActiveView, CustomTitleBar, DriveLibraryPickerModal, InstallOptionsDialog, LaunchOptionsModal, LaunchSplash, OperationHero, SettingsView, Sidebar } from './components'

const initialLauncherPreferences = loadLauncherPreferences()
const emptyCatalog: GameCatalog = { defaultLocale: 'en-US', games: [] }
type CatalogLoadState = 'loading' | 'ready' | 'error'
const defaultLauncherSettings: LauncherSettings = {
  defaultLibrary: DEFAULT_STORE_ROOT,
  downloadWorkers: 8,
  downloadRetries: 5,
  packRangeMb: 16,
  keepChunkCache: true,
  notificationsEnabled: true,
  autoVerifyAfterInstall: false,
  downloadProfile: 'balanced',
  downloadQueueMb: 128,
  directToStaging: false,
  cloudSaveRoot: '',
  gameUpdateMode: 'automatic',
  gameUpdateScheduleStart: '02:00',
  gameUpdateScheduleEnd: '06:00',
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot)
  const [job, setJob] = useState<JobJournal | null>(fallbackSnapshot.lastJob)
  const [installPath, setInstallPath] = useState('')
  const [scanStatus, setScanStatus] = useState('No install found')
  const [, setHasScanned] = useState(false)
  const [preferences, setPreferences] = useState<LauncherPreferences>(initialLauncherPreferences)
  const [launcherSettings, setLauncherSettings] = useState<LauncherSettings>(defaultLauncherSettings)
  const [activeTab, setActiveTab] = useState<TabId>(initialLauncherPreferences.startupPage)
  const [selectedVersion, setSelectedVersion] = useState('')
  const [showInstallOptions, setShowInstallOptions] = useState(false)
  const [installRoot, setInstallRoot] = useState(`${initialLauncherPreferences.defaultLibraryRoot}\\common\\007 First Light`)
  const [catalog, setCatalog] = useState<GameCatalog>(() => (isTauriRuntime() ? emptyCatalog : fallbackCatalog))
  const [catalogLoadState, setCatalogLoadState] = useState<CatalogLoadState>(
    isTauriRuntime() ? 'loading' : 'ready',
  )
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)
  const [detail, setDetail] = useState<GameDetail | null>(null)
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({})
  const assetUrlsRef = useRef<Record<string, string>>({})
  const assetRequestRef = useRef<Set<string>>(new Set())
  const assetDelaySlotRef = useRef(0)
  const [installStates, setInstallStates] = useState<Record<string, GameInstallState>>({})
  const latestJobRef = useRef<JobJournal | null>(job)
  const preferencesRef = useRef<LauncherPreferences>(preferences)
  const installCompleteAudioRef = useRef<HTMLAudioElement | null>(null)
  const installCompleteSoundJobsRef = useRef<Set<string>>(new Set())
  const audibleInstallJobIdsRef = useRef<Set<string>>(new Set())
  const pendingCloudLaunchRef = useRef<{ optionId?: string; optionTitle?: string } | null>(null)
  const downloadRateWindowRef = useRef<{ jobId: string; points: Array<{ bytesDone: number; at: number }> } | null>(null)
  const canceledJobIdRef = useRef<string | null>(null)
  const selectedGameIdRef = useRef<string | null>(selectedGameId)
  const versionPlanSequenceRef = useRef(0)
  const [downloadRate, setDownloadRate] = useState(0)
  const [verifyStatus, setVerifyStatus] = useState<VerifyUiStatus | null>(null)
  const [launchSplash, setLaunchSplash] = useState<LaunchSplashState | null>(null)
  const [launchOptions, setLaunchOptions] = useState<ResolvedGameLaunchConfig | null>(null)
  const [launcherUpdate, setLauncherUpdate] = useState<LauncherUpdateInfo | null>(null)
  const [launcherUpdateStatus, setLauncherUpdateStatus] = useState<string | null>(null)
  const [settingsUpdateStatus, setSettingsUpdateStatus] = useState<string | null>(null)
  const [showDrivePicker, setShowDrivePicker] = useState(false)
  const [showUninstallConfirm, setShowUninstallConfirm] = useState(false)
  const [playingGames, setPlayingGames] = useState<Record<string, boolean>>({})
  const [showSpacewarPrompt, setShowSpacewarPrompt] = useState(false)
  const [spacewarDownloading, setSpacewarDownloading] = useState(false)
  const [showSteamRecommendation, setShowSteamRecommendation] = useState(false)
  const [steamOpening, setSteamOpening] = useState(false)
  const [steamEnvironment, setSteamEnvironment] = useState<SteamEnvironmentInfo | null>(null)
  const [steamSettingsStatus, setSteamSettingsStatus] = useState<string | null>(null)
  const [cloudSaveStatus, setCloudSaveStatus] = useState<CloudSaveStatus | null>(null)
  const [cloudSaveBusy, setCloudSaveBusy] = useState(false)
  const [cloudLaunchBlocked, setCloudLaunchBlocked] = useState(false)
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
    assetUrlsRef.current = assetUrls
  }, [assetUrls])

  useEffect(() => {
    selectedGameIdRef.current = selectedGameId
  }, [selectedGameId])

  useEffect(() => {
    preferencesRef.current = preferences
    saveLauncherPreferences(preferences)
  }, [preferences])

  useEffect(() => {
    const audio = new Audio(installCompleteSoundUrl)
    audio.preload = 'auto'
    installCompleteAudioRef.current = audio
    return () => {
      audio.pause()
      installCompleteAudioRef.current = null
    }
  }, [])

  const primeInstallCompleteSound = useCallback(() => {
    if (!preferencesRef.current.playInstallCompleteSound) return
    const audio = installCompleteAudioRef.current
    if (!audio) return
    const previousMuted = audio.muted
    audio.muted = true
    audio.currentTime = 0
    void audio.play()
      .then(() => {
        audio.pause()
        audio.currentTime = 0
        audio.muted = previousMuted
      })
      .catch(() => {
        audio.muted = previousMuted
      })
  }, [])

  const playInstallCompleteSound = useCallback((completedJob: JobJournal) => {
    if (
      completedJob.status !== 'committed' ||
      completedJob.kind !== 'install' ||
      !preferencesRef.current.playInstallCompleteSound ||
      !audibleInstallJobIdsRef.current.has(completedJob.id) ||
      installCompleteSoundJobsRef.current.has(completedJob.id)
    ) {
      return
    }
    installCompleteSoundJobsRef.current.add(completedJob.id)
    audibleInstallJobIdsRef.current.delete(completedJob.id)
    const audio = installCompleteAudioRef.current
    if (!audio) return
    audio.muted = false
    audio.currentTime = 0
    void audio.play().catch(() => undefined)
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    invoke<LauncherSettings>('get_launcher_settings')
      .then(setLauncherSettings)
      .catch((error) => setSettingsUpdateStatus(`Could not load downloader settings: ${String(error)}`))
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    listen<{ state: string; message: string; gameId: string | null }>('launcher://auto-update', (event) => {
      setSettingsUpdateStatus(event.payload.message)
    }).then((dispose) => {
      unlisten = dispose
    })
    return () => unlisten?.()
  }, [])

  const requestGameAsset = useCallback((game: GameSummary | null | undefined, assetId: string | undefined, urgent = false) => {
    if (!game || !assetId || !isTauriRuntime()) {
      return
    }
    if (assetUrlsRef.current[assetId] || assetRequestRef.current.has(assetId)) {
      return
    }
    assetRequestRef.current.add(assetId)
    const delay = urgent ? 0 : Math.min(1200, assetDelaySlotRef.current++ * 90)
    window.setTimeout(async () => {
      try {
        const blob = await invoke<AssetBlob>('get_game_asset', { gameId: game.id, assetId })
        const url = `data:${blob.mimeType};base64,${blob.dataBase64}`
        setAssetUrls((current) => {
          if (current[assetId]) return current
          return { ...current, [assetId]: url }
        })
      } catch {
        // Ignore individual image failures; placeholders stay visible.
      }
    }, delay)
  }, [])

  const loadCatalog = useCallback(async () => {
    if (!isTauriRuntime()) {
      setCatalogLoadState('ready')
      return
    }

    queueMicrotask(() => setCatalogLoadState('loading'))
    try {
      const next = await invoke<GameCatalog>('get_game_catalog')
      setCatalog(next)
      setCatalogLoadState('ready')
    } catch (error) {
      console.error('Unable to load the game catalog:', error)
      setCatalogLoadState('error')
    }
  }, [])

  const refreshSteamEnvironment = useCallback(async (announce = false) => {
    if (!isTauriRuntime()) {
      setSteamSettingsStatus('Steam integration diagnostics require the desktop launcher.')
      return
    }
    if (announce) setSteamSettingsStatus('Refreshing Steam status...')
    try {
      const environment = await invoke<SteamEnvironmentInfo>('get_steam_environment')
      setSteamEnvironment(environment)
      if (announce) {
        setSteamSettingsStatus(
          environment.installed
            ? environment.running
              ? 'Steam is installed and running.'
              : 'Steam is installed but not running.'
            : 'Steam installation was not detected.',
        )
      }
    } catch (error) {
      setSteamSettingsStatus(`Steam status failed: ${String(error)}`)
    }
  }, [])

  useEffect(() => {
    queueMicrotask(() => void loadCatalog())
  }, [loadCatalog])

  useEffect(() => {
    if (activeTab === 'Settings') {
      queueMicrotask(() => void refreshSteamEnvironment())
    }
  }, [activeTab, refreshSteamEnvironment])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    listen<LauncherUpdateProgress>('launcher://update-progress', (event) => {
      if (event.payload.phase === 'installing') {
        setLauncherUpdateStatus('Download verified. Installing and restarting...')
        return
      }
      const total = event.payload.totalBytes ?? 0
      const percent = total > 0 ? Math.min(100, Math.round((event.payload.downloadedBytes / total) * 100)) : null
      setLauncherUpdateStatus(percent === null ? 'Downloading update...' : `Downloading update... ${percent}%`)
    })
      .then((dispose) => {
        unlisten = dispose
      })
      .catch(console.error)
    return () => unlisten?.()
  }, [])

  useEffect(() => {
    if (!isTauriRuntime() || !preferences.autoCheckLauncherUpdates) {
      return
    }

    const updateTimer = window.setTimeout(() => {
      invoke<LauncherUpdateInfo | null>('check_launcher_update')
        .then((info) => {
          if (info) setLauncherUpdate(info)
        })
        .catch(console.error)
    }, 1800)

    return () => window.clearTimeout(updateTimer)
  }, [preferences.autoCheckLauncherUpdates])

  async function refreshInstallState(gameId: string, committedInstallPath?: string) {
    if (!isTauriRuntime()) {
      return
    }
    const state = await invoke<GameInstallState>('get_game_install_state', { gameId })
    setInstallStates((current) => {
      const existing = current[gameId]
      if (
        committedInstallPath &&
        !state.installed &&
        existing?.installed &&
        existing.installPath === committedInstallPath
      ) {
        return current
      }
      return { ...current, [gameId]: state }
    })
  }

  useEffect(() => {
    if (!isTauriRuntime() || catalog.games.length === 0) {
      return
    }

    let disposed = false
    const gameIds = catalog.games.map((game) => game.id)
    invoke<GameInstallState[]>('get_game_install_states', { gameIds })
      .then((states) => {
        if (disposed) return
        const next: Record<string, GameInstallState> = {}
        for (const state of states) {
          next[state.gameId] = state
        }
        setInstallStates(next)
      })
      .catch(() => {
        // Compatibility fallback for older backends: stagger single calls so the
        // WebView is not hammered by N simultaneous IPC requests at startup.
        gameIds.forEach((gameId, index) => {
          window.setTimeout(() => {
            if (!disposed) void refreshInstallState(gameId).catch(() => undefined)
          }, index * 120)
        })
      })

    return () => {
      disposed = true
    }
  }, [catalog.games])

  const updateReadyGameIds = useMemo(() => {
    return catalog.games
      .filter((game) => {
        const state = installStates[game.id]
        if (!state?.installed || state.currentVersion === 'unknown' || state.currentVersion === 'not installed') {
          return false
        }
        const latest = game.availableVersions.find((version) => version.latest)?.version ?? game.latestVersion
        return Boolean(latest && latest !== 'unknown' && state.currentVersion !== latest)
      })
      .map((game) => game.id)
  }, [catalog.games, installStates])
  const updatesCatalog = useMemo(
    () => ({ ...catalog, games: catalog.games.filter((game) => updateReadyGameIds.includes(game.id)) }),
    [catalog, updateReadyGameIds],
  )
  const libraryCatalog = useMemo(
    () => ({ ...catalog, games: catalog.games.filter((game) => installStates[game.id]?.installed) }),
    [catalog, installStates],
  )

  const effectiveGameId = useMemo(() => {
    if (activeTab === 'Settings') {
      return null
    }
    if (activeTab === 'Store') {
      return selectedGameId
    }
    if (activeTab === 'Library') {
      return selectedGameId && installStates[selectedGameId]?.installed ? selectedGameId : null
    }
    const activeJobGameId = job?.gameId || snapshot.lastJob?.gameId
    if (activeTab === 'Downloads') {
      return activeJobGameId ?? null
    }
    if (activeTab === 'Updates') {
      const activeUpdateGameId =
        job?.kind === 'update'
          ? job.gameId
          : snapshot.lastJob?.kind === 'update'
            ? snapshot.lastJob.gameId
            : null
      if (activeUpdateGameId) {
        return activeUpdateGameId
      }
      return selectedGameId && updateReadyGameIds.includes(selectedGameId) ? selectedGameId : null
    }
    return selectedGameId
  }, [activeTab, installStates, job?.gameId, job?.kind, snapshot.lastJob?.gameId, snapshot.lastJob?.kind, selectedGameId, updateReadyGameIds])

  useEffect(() => {
    if (!effectiveGameId) {
      return
    }

    if (!isTauriRuntime()) {
      const game = catalog.games.find((candidate) => candidate.id === effectiveGameId)
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

    let disposed = false
    invoke<GameDetail>('get_game_detail', { gameId: effectiveGameId, locale: 'en-US' })
      .then((nextDetail) => {
        if (!disposed) setDetail(nextDetail)
      })
      .catch((error) => {
        if (disposed) return
        console.error(`Unable to load details for ${effectiveGameId}:`, error)
        const game = catalog.games.find((candidate) => candidate.id === effectiveGameId)
        setDetail(game ? fallbackDetailFromSummary(game) : null)
      })
    return () => {
      disposed = true
    }
  }, [catalog.games, effectiveGameId])

  const selectedGame = useMemo(
    () => (effectiveGameId ? catalog.games.find((game) => game.id === effectiveGameId) ?? null : null),
    [catalog.games, effectiveGameId],
  )
  const activeDetail = detail?.gameId === effectiveGameId ? detail : null

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let disposed = false
    let unlistenLaunch: (() => void) | undefined
    let unlistenError: (() => void) | undefined
    let unlistenSteamRecommendation: (() => void) | undefined
    let unlistenSpacewarRequired: (() => void) | undefined

    listen<ShortcutLaunchPayload>('launcher://shortcut-launch', (event) => {
      const payload = event.payload
      const game = catalog.games.find((candidate) => candidate.id === payload.gameId)
      setSelectedGameId(payload.gameId)
      setInstallPath(payload.installPath)
      setInstallRoot(payload.installPath)
      setInstallStates((current) => ({
        ...current,
        [payload.gameId]: {
          gameId: payload.gameId,
          installed: true,
          currentVersion: current[payload.gameId]?.currentVersion ?? 'installed',
          installPath: payload.installPath,
          launchExecutable: payload.launchExecutable ?? current[payload.gameId]?.launchExecutable ?? '',
        },
      }))
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

    listen<ShortcutLaunchPayload>('launcher://steam-recommendation-required', () => {
      setShowSteamRecommendation(true)
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unlistenSteamRecommendation = fn
      }
    })

    listen<ShortcutLaunchPayload>('launcher://spacewar-required', () => {
      setShowSpacewarPrompt(true)
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unlistenSpacewarRequired = fn
      }
    })

    return () => {
      disposed = true
      unlistenLaunch?.()
      unlistenError?.()
      unlistenSteamRecommendation?.()
      unlistenSpacewarRequired?.()
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
        setInstallRoot(installMetadataForStoreRoot(selectedGame, selectedGame.install, preferences.defaultLibraryRoot).defaultInstallFolder)
        setHasScanned(false)
        setScanStatus('No install found')
      }
    })
    return () => {
      disposed = true
    }
  }, [installStates, preferences.defaultLibraryRoot, selectedGame])

  // Scale mode: selected game assets are urgent; browse cards request their
  // thumbnails only when they become visible. This avoids reading every .0xo
  // image at launcher startup.
  useEffect(() => {
    if (!isTauriRuntime() || catalog.games.length === 0 || !selectedGameId) return
    const selected = catalog.games.find((game) => game.id === selectedGameId)
    if (!selected) return
    ;[selected.heroAssetId, selected.logoAssetId, selected.iconAssetId, selected.gridAssetId].forEach((assetId) => {
      requestGameAsset(selected, assetId, true)
    })
  }, [catalog.games, requestGameAsset, selectedGameId])

  useEffect(() => {
    if (!selectedGame || !activeDetail) {
      return
    }
    const ids = collectAssetIds(selectedGame)
    for (const assetId of ids) {
      requestGameAsset(selectedGame, assetId, true)
    }
  }, [activeDetail, requestGameAsset, selectedGame])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let disposed = false
    const snapshotTimer = window.setTimeout(() => {
      invoke<Snapshot>('get_launcher_snapshot')
        .then((next) => {
          if (disposed) return
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
          if (!disposed) setSnapshot(fallbackSnapshot)
        })
    }, 250)

    let unsubscribe: (() => void) | undefined
    let unsubscribeJobCleared: (() => void) | undefined
    listen<JobJournal>('launcher://job', (event) => {
      const nextJob = event.payload
      if (canceledJobIdRef.current === nextJob.id) {
        return
      }
      if (canceledJobIdRef.current && canceledJobIdRef.current !== nextJob.id) {
        canceledJobIdRef.current = null
      }
      setJob(nextJob)
      if (
        selectedGameIdRef.current === nextJob.gameId &&
        nextJob.toVersion &&
        nextJob.toVersion !== 'unknown'
      ) {
        // The job target is authoritative while install/update/repair is active.
        // Keep the picker on that exact version instead of letting a delayed
        // planning response or an old marker make the UI jump backwards.
        setSelectedVersion(nextJob.toVersion)
      }
      if (nextJob.status === 'committed') {
        playInstallCompleteSound(nextJob)
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
        window.setTimeout(() => {
          void refreshInstallState(nextJob.gameId, nextJob.installPath).catch(() => undefined)
        }, 350)
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

    listen('launcher://job-cleared', () => {
      setJob(null)
      setDownloadRate(0)
      downloadRateWindowRef.current = null
      setVerifyStatus((current) => (current?.state === 'running' ? null : current))
    }).then((fn) => {
      if (disposed) {
        fn()
      } else {
        unsubscribeJobCleared = fn
      }
    })

    return () => {
      disposed = true
      window.clearTimeout(snapshotTimer)
      unsubscribe?.()
      unsubscribeJobCleared?.()
    }
  }, [playInstallCompleteSound])

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
  latestJobRef.current = activeJob

  useEffect(() => {
    if (activeJob.status !== 'downloading') {
      downloadRateWindowRef.current = null
      return
    }

    const sampleWindowMs = 10_000
    const tick = () => {
      const current = latestJobRef.current
      if (!current || current.status !== 'downloading' || current.id !== activeJob.id) {
        return
      }

      const now = performance.now()
      let windowState = downloadRateWindowRef.current
      if (!windowState || windowState.jobId !== current.id) {
        windowState = { jobId: current.id, points: [] }
      }

      const lastPoint = windowState.points[windowState.points.length - 1]
      if (!lastPoint || current.bytesDone !== lastPoint.bytesDone || now - lastPoint.at >= 900) {
        windowState.points.push({ bytesDone: current.bytesDone, at: now })
      }
      windowState.points = windowState.points.filter((point) => now - point.at <= sampleWindowMs)
      if (windowState.points.length > 12) {
        windowState.points.splice(0, windowState.points.length - 12)
      }
      downloadRateWindowRef.current = windowState

      const first = windowState.points[0]
      const last = windowState.points[windowState.points.length - 1]
      const elapsedMs = last && first ? last.at - first.at : 0
      const transferred = last && first ? Math.max(last.bytesDone - first.bytesDone, 0) : 0
      setDownloadRate(elapsedMs >= 900 ? (transferred * 1000) / elapsedMs : 0)
    }

    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [activeJob.id, activeJob.status])

  const phaseProgress = getPhaseProgress(activeJob, activeJob.status === 'downloading' ? downloadRate : 0)
  const progress = phaseProgress.percent
  const hasVisibleJob = job !== null && activeJob.status !== 'committed'
  const isDefaultGame = selectedGame?.id === DEFAULT_GAME_ID
  const selectedInstallState = selectedGame ? installStates[selectedGame.id] : undefined
  const selectedInstalled = Boolean(selectedInstallState?.installed)
  const gameInstall = useMemo(
    () => installMetadataForStoreRoot(selectedGame, activeDetail?.install ?? selectedGame?.install ?? fallbackInstall, preferences.defaultLibraryRoot),
    [activeDetail?.install, preferences.defaultLibraryRoot, selectedGame],
  )
  const selectedInstallPath = selectedInstalled
    ? selectedInstallState?.installPath || gameInstall.defaultInstallFolder
    : gameInstall.defaultInstallFolder
  const selectedCurrentVersion = selectedInstalled ? selectedInstallState?.currentVersion ?? 'installed' : 'not installed'
  const selectedVerifyStatus = selectedGame && verifyStatus?.gameId === selectedGame.id ? verifyStatus : null
  const availableVersions = selectedGame ? versionOptions(snapshot, selectedGame, isDefaultGame) : []
  const latestCatalogVersion =
    selectedGame?.availableVersions.find((version) => version.latest)?.version ??
    activeDetail?.versions.find((version) => version.latest)?.version ??
    selectedGame?.latestVersion ??
    availableVersions[availableVersions.length - 1] ??
    'unknown'
  const fallbackTargetVersion = isDefaultGame && availableVersions.includes(snapshot.latestVersion)
    ? snapshot.latestVersion
    : latestCatalogVersion !== 'unknown'
      ? latestCatalogVersion
      : availableVersions[availableVersions.length - 1] || 'select game'
  const requestedTargetVersion =
    selectedVersion && availableVersions.includes(selectedVersion) ? selectedVersion : fallbackTargetVersion
  // Keep the selected target for both fresh installs and installed games. This
  // allows the same version picker to perform upgrades, reinstalls and downgrades.
  const targetVersion = requestedTargetVersion
  const selectedVersionInfo =
    selectedGame?.availableVersions.find((version) => version.version === targetVersion) ??
    activeDetail?.versions.find((version) => version.version === targetVersion)
  const installMode = !selectedInstalled
  const updateReady =
    selectedInstalled &&
    selectedCurrentVersion !== 'unknown' &&
    selectedCurrentVersion !== 'not installed' &&
    latestCatalogVersion !== 'unknown' &&
    selectedCurrentVersion !== latestCatalogVersion
  const isPaused = activeJob.status === 'paused'
  const isRunning = job !== null && ['running', 'downloading', 'assembling', 'paused'].includes(activeJob.status)
  const hasVersionChoices = availableVersions.length > 1
  const canUpdate =
    Boolean(selectedGame && activeDetail) &&
    !isRunning &&
    availableVersions.length > 0 &&
    targetVersion !== 'unknown' &&
    targetVersion !== 'select game'
  const canApplySelectedVersion =
    canUpdate && (installMode || targetVersion !== selectedCurrentVersion)
  const effectiveDownloadSize =
    snapshot.updateSize > 0
      ? snapshot.updateSize
      : selectedVersionInfo?.sizeBytes ?? activeDetail?.versions[0]?.sizeBytes ?? 0
  const displayedInstallTarget =
    selectedInstalled
      ? selectedInstallPath
      : hasVisibleJob && activeJob.installPath
        ? activeJob.installPath
        : installRoot || gameInstall.defaultInstallFolder

  const refreshCloudSaveStatus = useCallback(async (gameId: string) => {
    if (!isTauriRuntime()) {
      setCloudSaveStatus(null)
      return
    }
    try {
      const status = await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
    } catch (error) {
      setCloudSaveStatus(null)
      setScanStatus(`Could not load cloud save status: ${String(error)}`)
    }
  }, [])

  useEffect(() => {
    if (activeTab !== 'Library' || !selectedGame || !selectedInstalled) {
      queueMicrotask(() => {
        setCloudSaveStatus(null)
        setCloudLaunchBlocked(false)
      })
      return
    }
    queueMicrotask(() => void refreshCloudSaveStatus(selectedGame.id))
  }, [activeTab, refreshCloudSaveStatus, selectedGame, selectedInstalled])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let disposed = false
    let unlistenStatus: (() => void) | undefined
    let unlistenError: (() => void) | undefined

    listen<{ gameId: string; status: CloudSaveStatus }>('launcher://cloud-save', (event) => {
      if (disposed || event.payload.gameId !== selectedGameIdRef.current) return
      setCloudSaveStatus(event.payload.status)
      setCloudLaunchBlocked(event.payload.status.conflicts.length > 0)
    }).then((dispose) => {
      if (disposed) dispose()
      else unlistenStatus = dispose
    })

    listen<{ gameId: string; message: string }>('launcher://cloud-save-error', (event) => {
      if (disposed || event.payload.gameId !== selectedGameIdRef.current) return
      setScanStatus(event.payload.message)
    }).then((dispose) => {
      if (disposed) dispose()
      else unlistenError = dispose
    })

    return () => {
      disposed = true
      unlistenStatus?.()
      unlistenError?.()
    }
  }, [])

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
      const gameId = selectedGame.id
      const [report, planned] = await Promise.all([
        invoke<{ fileCount: number; detectedVersion?: string | null; warnings: string[] }>('scan_install', {
          path,
        }),
        invoke<Snapshot>('plan_install_update', { path, targetVersion, gameId }),
      ])
      const plannedVersion =
        planned.currentVersion !== 'unknown' && planned.currentVersion !== 'not installed'
          ? planned.currentVersion
          : report.detectedVersion
      const versionLabel = plannedVersion ? `installed ${plannedVersion}` : 'version state not found'
      setScanStatus(`${report.fileCount} files, ${versionLabel}`)
      setSnapshot(planned)
      setJob(planned.lastJob)
      setHasScanned(Boolean(plannedVersion))
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
    const gameId = selectedGame.id
    const requestSequence = ++versionPlanSequenceRef.current
    setSelectedVersion(version)
    if (!isTauriRuntime()) {
      return
    }

    try {
      const planned = selectedInstalled
        ? await invoke<Snapshot>('plan_install_update', {
            path: selectedInstallPath,
            targetVersion: version,
            gameId,
          })
        : await invoke<Snapshot>('plan_fresh_install', { targetVersion: version, gameId })
      if (
        requestSequence !== versionPlanSequenceRef.current ||
        selectedGameIdRef.current !== gameId
      ) {
        return
      }
      setSnapshot(planned)
      setJob(planned.lastJob)
    } catch (error) {
      if (requestSequence === versionPlanSequenceRef.current) {
        setScanStatus(String(error))
      }
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

  function updatePreference<K extends keyof LauncherPreferences>(key: K, value: LauncherPreferences[K]) {
    setPreferences((current) => ({ ...current, [key]: value }))
  }

  async function updateLauncherSetting<K extends keyof LauncherSettings>(
    key: K,
    value: LauncherSettings[K],
  ) {
    const profilePreset =
      key === 'downloadProfile'
        ? value === 'eco'
          ? { downloadWorkers: 4, downloadQueueMb: 64 }
          : value === 'turbo'
            ? { downloadWorkers: 12, downloadQueueMb: 256 }
            : { downloadWorkers: 8, downloadQueueMb: 128 }
        : {}
    const next = { ...launcherSettings, [key]: value, ...profilePreset }
    setLauncherSettings(next)
    if (!isTauriRuntime()) return
    try {
      const saved = await invoke<LauncherSettings>('set_launcher_settings', { settings: next })
      setLauncherSettings(saved)
      setSettingsUpdateStatus('Launcher settings saved.')
    } catch (error) {
      setSettingsUpdateStatus(`Could not save downloader settings: ${String(error)}`)
    }
  }

  async function chooseDefaultLibraryRoot() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Choose default game library',
        defaultPath: preferences.defaultLibraryRoot,
      })
      if (typeof selected !== 'string') return
      const root = selected.trim().replace(/[\\/]+$/, '') || DEFAULT_STORE_ROOT
      updatePreference('defaultLibraryRoot', root)
      setSettingsUpdateStatus(`Default library changed to ${root}`)

      const drive = root.match(/^([A-Za-z]:)/)?.[1]?.toUpperCase()
      if (drive && !libraries.includes(drive)) {
        const next = [...libraries, drive]
        setLibraries(next)
        localStorage.setItem('0xo_libraries', JSON.stringify(next))
      }

      if (selectedGame && !selectedInstalled) {
        setInstallRoot(installMetadataForStoreRoot(selectedGame, activeDetail?.install ?? selectedGame.install, root).defaultInstallFolder)
      }
    } catch (error) {
      setSettingsUpdateStatus(`Could not change library: ${String(error)}`)
    }
  }

  async function openDefaultLibraryRoot() {
    if (!isTauriRuntime()) {
      setSettingsUpdateStatus(preferences.defaultLibraryRoot)
      return
    }
    try {
      await invoke('open_folder', { path: preferences.defaultLibraryRoot })
    } catch (error) {
      setSettingsUpdateStatus(`Could not open library: ${String(error)}`)
    }
  }

  async function chooseCloudSaveRoot() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Choose synchronized cloud save folder',
        defaultPath: launcherSettings.cloudSaveRoot || undefined,
      })
      if (typeof selected !== 'string') return
      await updateLauncherSetting('cloudSaveRoot', selected.trim().replace(/[\\/]+$/, ''))
      setSettingsUpdateStatus('Cloud save provider folder saved.')
      if (selectedGame && selectedInstalled) {
        await refreshCloudSaveStatus(selectedGame.id)
      }
    } catch (error) {
      setSettingsUpdateStatus(`Could not change cloud save folder: ${String(error)}`)
    }
  }

  async function openCloudSaveRoot() {
    if (!launcherSettings.cloudSaveRoot) return
    if (!isTauriRuntime()) {
      setSettingsUpdateStatus(launcherSettings.cloudSaveRoot)
      return
    }
    try {
      await invoke('open_folder', { path: launcherSettings.cloudSaveRoot })
    } catch (error) {
      setSettingsUpdateStatus(`Could not open cloud save folder: ${String(error)}`)
    }
  }

  async function saveCloudConfig(enabled: boolean, saveRoots: CloudSaveRoot[]) {
    if (!selectedGame || !selectedInstalled || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    try {
      const status = await invoke<CloudSaveStatus>('set_cloud_save_config', {
        gameId: selectedGame.id,
        enabled,
        saveRoots,
        include: cloudSaveStatus?.include ?? activeDetail?.cloudSave.include ?? [],
        exclude: cloudSaveStatus?.exclude ?? activeDetail?.cloudSave.exclude ?? [],
      })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
      setScanStatus(status.lastMessage)
    } catch (error) {
      setScanStatus(`Cloud save configuration failed: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function toggleCloudSave(enabled: boolean) {
    if (enabled && !launcherSettings.cloudSaveRoot) {
      setScanStatus('Choose a Cloud Save root in Settings before enabling sync.')
      return
    }
    await saveCloudConfig(enabled, cloudSaveStatus?.saveRoots ?? [])
  }

  async function addCloudSaveFolder() {
    if (!selectedGame || !selectedInstalled) return
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: `Choose a save folder for ${selectedGame.title}`,
      })
      if (typeof selected !== 'string') return
      const normalized = selected.trim().replace(/[\\/]+$/, '')
      if (!normalized) return
      const currentRoots = cloudSaveStatus?.saveRoots ?? []
      if (currentRoots.some((root) => root.path.toLowerCase() === normalized.toLowerCase())) {
        setScanStatus('That save folder is already configured.')
        return
      }
      const label = normalized.split(/[\\/]/).filter(Boolean).pop() ?? normalized
      await saveCloudConfig(cloudSaveStatus?.enabled ?? false, [...currentRoots, { path: normalized, label }])
    } catch (error) {
      setScanStatus(`Could not add save folder: ${String(error)}`)
    }
  }

  async function syncCloudSave() {
    if (!selectedGame || !selectedInstalled || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    try {
      const status = await invoke<CloudSaveStatus>('sync_cloud_save', {
        gameId: selectedGame.id,
        direction: null,
      })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
      setScanStatus(status.lastMessage)
    } catch (error) {
      setScanStatus(`Cloud save sync failed: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function resolveCloudConflict(conflictId: string, resolution: 'local' | 'cloud') {
    if (!selectedGame || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    try {
      const status = await invoke<CloudSaveStatus>('resolve_cloud_save_conflict', {
        gameId: selectedGame.id,
        conflictId,
        resolution,
      })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
      setScanStatus(status.lastMessage)
    } catch (error) {
      setScanStatus(`Could not resolve cloud save conflict: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function restoreCloudSnapshot(snapshotId: string) {
    if (!selectedGame || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    try {
      const status = await invoke<CloudSaveStatus>('restore_cloud_save_snapshot', {
        gameId: selectedGame.id,
        snapshotId,
      })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
      setScanStatus(status.lastMessage)
    } catch (error) {
      setScanStatus(`Could not restore cloud save snapshot: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function runGoogleDriveAction(
    command:
      | 'connect_google_drive'
      | 'disconnect_google_drive'
      | 'backup_save_game_to_google_drive'
      | 'restore_missing_save_files',
    pendingMessage: string,
  ) {
    if (!selectedGame || !selectedInstalled || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    setScanStatus(pendingMessage)
    try {
      const status = await invoke<CloudSaveStatus>(command, { gameId: selectedGame.id })
      setCloudSaveStatus(status)
      setScanStatus(status.googleDriveMessage || status.lastMessage)
    } catch (error) {
      setScanStatus(`Google Drive operation failed: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function connectAndBackupGoogleDrive() {
    if (!selectedGame || !selectedInstalled || !isTauriRuntime()) return
    setCloudSaveBusy(true)
    setScanStatus('Opening Google sign-in in your browser...')
    try {
      const connected = await invoke<CloudSaveStatus>('connect_google_drive', {
        gameId: selectedGame.id,
      })
      setCloudSaveStatus(connected)
      setScanStatus('Google Drive connected. Backing up save files...')

      const backedUp = await invoke<CloudSaveStatus>('backup_save_game_to_google_drive', {
        gameId: selectedGame.id,
      })
      setCloudSaveStatus(backedUp)
      setScanStatus(backedUp.googleDriveMessage || backedUp.lastMessage)
    } catch (error) {
      setScanStatus(`Google Drive backup failed: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function checkLauncherUpdateNow() {
    if (!isTauriRuntime()) {
      setSettingsUpdateStatus('Update checks require the desktop launcher.')
      return
    }
    setSettingsUpdateStatus('Checking for launcher updates...')
    try {
      const info = await invoke<LauncherUpdateInfo | null>('check_launcher_update')
      setLauncherUpdate(info)
      setSettingsUpdateStatus(info ? `Version ${info.version} is available.` : 'Launcher is up to date.')
    } catch (error) {
      setSettingsUpdateStatus(`Update check failed: ${String(error)}`)
    }
  }

  async function openSteamFromSettings(command: 'open_steam' | 'open_steam_big_picture') {
    if (!isTauriRuntime()) {
      setSteamSettingsStatus('Steam actions require the desktop launcher.')
      return
    }
    setSteamSettingsStatus(command === 'open_steam' ? 'Opening Steam...' : 'Opening Steam Big Picture...')
    try {
      await invoke(command)
      window.setTimeout(() => void refreshSteamEnvironment(), 1800)
    } catch (error) {
      setSteamSettingsStatus(`Steam action failed: ${String(error)}`)
    }
  }

  function resetLauncherPreferences() {
    const defaults = { ...DEFAULT_LAUNCHER_PREFERENCES }
    setPreferences(defaults)
    setLauncherSettings(defaultLauncherSettings)
    if (isTauriRuntime()) {
      void invoke<LauncherSettings>('set_launcher_settings', { settings: defaultLauncherSettings })
        .then(setLauncherSettings)
        .catch((error) => setSettingsUpdateStatus(`Could not reset downloader settings: ${String(error)}`))
    }
    setSettingsUpdateStatus('Default launcher settings restored.')
    if (selectedGame && !selectedInstalled) {
      setInstallRoot(
        installMetadataForStoreRoot(
          selectedGame,
          activeDetail?.install ?? selectedGame.install,
          defaults.defaultLibraryRoot,
        ).defaultInstallFolder,
      )
    }
  }

  async function openVersionOptions() {
    if (!selectedGame || !activeDetail) {
      setScanStatus('Select a game first')
      return
    }

    const preferredVersion = selectedInstalled
      ? updateReady && latestCatalogVersion !== 'unknown'
        ? latestCatalogVersion
        : availableVersions.includes(selectedCurrentVersion)
          ? selectedCurrentVersion
          : targetVersion
      : targetVersion

    setShowInstallOptions(true)
    if (preferredVersion && preferredVersion !== 'unknown' && preferredVersion !== 'select game') {
      await changeTargetVersion(preferredVersion)
    }
  }

  async function startUpdate() {
    if (!selectedGame || !activeDetail) {
      setScanStatus('Select a game first')
      return
    }
    if (!installMode && targetVersion === selectedCurrentVersion) {
      setScanStatus(`${targetVersion} is already installed. Choose another version to upgrade or downgrade.`)
      return
    }
    if (installMode) {
      primeInstallCompleteSound()
    }

    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot write local game files')
      return
    }

    // Disk space check
    try {
      const targetPath = installMode ? installRoot : selectedInstallPath
      const freeSpace = await invoke<number>('get_disk_free_space', { path: targetPath })
      const requiredSpace = snapshot?.requiredFreeSpace || snapshot?.updateSize || 0
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
      const versionToApply = targetVersion
      setSelectedVersion(versionToApply)
      const next = installMode
        ? await invoke<JobJournal>('start_install_job', {
            gameId: selectedGame.id,
            targetVersion: versionToApply,
            installPath: installRoot,
          })
        : await invoke<JobJournal>('start_update_job', {
            gameId: selectedGame.id,
            installPath: selectedInstallPath,
            targetVersion: versionToApply,
          })
      setJob(next)
      if (installMode) {
        audibleInstallJobIdsRef.current.add(next.id)
      }
      if (preferences.openDownloadsOnJobStart) {
        setActiveTab('Downloads')
      }
      setShowInstallOptions(false)
      if (installMode) {
        setInstallPath(installRoot)
        setScanStatus(`Installing ${versionToApply}`)
      }
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  async function playSelectedGame() {
    if (!selectedGame || !activeDetail) {
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

    if (gameHasTag(selectedGame.id, 'online')) {
      try {
        const spacewarOk = await invoke<boolean>('check_spacewar_installed')
        if (!spacewarOk) {
          setShowSpacewarPrompt(true)
          return
        }
      } catch {
        // If the Steam library probe itself fails, do not block the game.
      }

      try {
        const steamRunning = await invoke<boolean>('is_steam_running')
        if (!steamRunning) {
          setShowSteamRecommendation(true)
          return
        }
      } catch {
        // If process detection is unavailable, continue with normal launch.
      }
    }

    await continuePlaySelectedGame()
  }

  async function continuePlaySelectedGame() {
    if (!selectedGame || !activeDetail) return

    try {
      const config = await invoke<ResolvedGameLaunchConfig>('get_game_launch_config', {
        gameId: selectedGame.id,
        installPath: selectedInstallPath,
        launchExecutable: selectedInstallState?.launchExecutable || gameInstall.launchExecutable,
      })
      const availableOptions = config.options.filter((option) => option.available)
      if (availableOptions.length === 0) {
        const reason = config.options
          .map((option) => option.unavailableReason)
          .filter(Boolean)
          .join('; ')
        setScanStatus(reason || 'No launch option is available for this game')
        return
      }

      const shouldShowPicker =
        config.pickerMode === 'always' ||
        (config.pickerMode !== 'never' && config.options.length > 1)

      if (shouldShowPicker) {
        setLaunchOptions(config)
        return
      }

      const selectedOption =
        availableOptions.find((option) => option.id === config.defaultOptionId) ??
        availableOptions.find((option) => option.recommended) ??
        availableOptions[0]
      await doLaunchGame(selectedOption.id, selectedOption.title)
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  async function openSteamAndContinue() {
    setSteamOpening(true)
    try {
      await invoke('open_steam')
      const deadline = Date.now() + 20_000
      while (Date.now() < deadline) {
        const running = await invoke<boolean>('is_steam_running').catch(() => false)
        if (running) {
          setShowSteamRecommendation(false)
          await continuePlaySelectedGame()
          return
        }
        await new Promise((resolve) => window.setTimeout(resolve, 700))
      }
      setScanStatus("We're recommended you to open Steam to play online")
    } catch (error) {
      setScanStatus(`Could not open Steam: ${String(error)}`)
    } finally {
      setSteamOpening(false)
    }
  }

  async function doLaunchGame(launchOptionId?: string, launchOptionTitle?: string, skipCloudSync = false) {
    if (!selectedGame || !activeDetail) return

    if (preferences.pauseDownloadsBeforeLaunch && isRunning && !isPaused) {
      try {
        await invoke('pause_job')
        setJob((current) => (current ? { ...current, status: 'paused' } : current))
        setScanStatus('Active download paused before launching the game.')
      } catch (error) {
        setScanStatus(`Could not pause the active download: ${String(error)}`)
        return
      }
    }

    const splashStartedAt = performance.now()
    setLaunchSplash({
      title: launchOptionTitle ? `${selectedGame.title} — ${launchOptionTitle}` : selectedGame.title,
      heroUrl: assetUrls[selectedGame.heroAssetId] || firstMediaUrl(activeDetail, assetUrls),
      iconUrl: assetUrls[selectedGame.iconAssetId] || assetUrls[selectedGame.gridAssetId],
    })

    try {
      pendingCloudLaunchRef.current = { optionId: launchOptionId, optionTitle: launchOptionTitle }
      const report = await invoke<LaunchReport>('launch_game', {
        gameId: selectedGame.id,
        installPath: selectedInstallPath,
        launchExecutable: selectedInstallState?.launchExecutable || gameInstall.launchExecutable,
        launchOptionId: launchOptionId || null,
        skipCloudSync,
      })
      pendingCloudLaunchRef.current = null
      setCloudLaunchBlocked(false)
      const dependencyText =
        report.dependenciesInstalled.length > 0
          ? `Installed ${report.dependenciesInstalled.length} dependency package(s), then started`
          : 'Started'
      const optionText = report.launchOptionTitle ? ` (${report.launchOptionTitle})` : ''
      setScanStatus(`${dependencyText} ${selectedGame.title}${optionText}`)

      setPlayingGames((prev) => ({ ...prev, [selectedGame.id]: true }))
      setTimeout(() => {
        setPlayingGames((prev) => ({ ...prev, [selectedGame.id]: false }))
      }, 15000)

      const remainingMs = Math.max(0, 4200 - (performance.now() - splashStartedAt))
      window.setTimeout(() => setLaunchSplash(null), remainingMs)
    } catch (error) {
      setLaunchSplash(null)
      const message = String(error)
      if (message.includes('CLOUD_SAVE_CONFLICT:')) {
        setCloudLaunchBlocked(true)
        setScanStatus('Cloud save conflict detected. Resolve it or choose Launch without sync.')
        void refreshCloudSaveStatus(selectedGame.id)
      } else {
        pendingCloudLaunchRef.current = null
        setScanStatus(message)
      }
    }
  }

  function launchWithoutCloudSync() {
    const pending = pendingCloudLaunchRef.current
    if (!pending) {
      setScanStatus('Start the game again, then choose Launch without sync if the conflict remains.')
      return
    }
    setCloudLaunchBlocked(false)
    void doLaunchGame(pending.optionId, pending.optionTitle, true)
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

  function uninstallSelectedGame() {
    if (!selectedGame || !selectedInstalled) {
      return
    }
    if (preferences.confirmBeforeUninstall) {
      setShowUninstallConfirm(true)
      return
    }
    void executeUninstall()
  }

  async function executeUninstall() {
    setShowUninstallConfirm(false)
    if (!selectedGame || !selectedInstalled) {
      return
    }
    if (!isTauriRuntime()) {
      setScanStatus('Browser preview cannot uninstall local game files')
      return
    }
    try {
      await invoke('abort_and_clean_job', { gameId: selectedGame.id }).catch(() => undefined)
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
      const desktopShortcutText = report.removedShortcuts > 0 ? `, removed ${report.removedShortcuts} shortcut${report.removedShortcuts === 1 ? '' : 's'}` : ''
      const steamShortcutText = report.steamShortcutRemoved ? ', removed/queued Steam shortcut cleanup' : ''
      setScanStatus(`Uninstalled ${selectedGame.title}: removed ${report.removedFiles} files${desktopShortcutText}${steamShortcutText}`)
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

  async function resumeFailedJob() {
    if (!isTauriRuntime()) {
      setJob((current) => (current ? { ...current, status: 'downloading', phase: 'Download packs' } : current))
      return
    }

    try {
      await invoke('resume_job')
      setJob((current) => (current ? { ...current, status: 'downloading', phase: current.phase || 'Download packs' } : current))
      setScanStatus('Resuming download...')
    } catch (error) {
      setScanStatus(String(error))
    }
  }

  async function cancelJob() {
    if (!isTauriRuntime()) {
      setJob((current) => (current ? { ...current, status: 'canceled', phase: 'Canceled' } : current))
      return
    }

    if (!window.confirm('Are you sure you want to cancel the download? This will delete all downloaded data.')) {
      return
    }

    canceledJobIdRef.current = job?.id ?? null

    try {
      if (selectedGameId) {
        await invoke('abort_and_clean_job', { gameId: selectedGameId })
      } else {
        await invoke('cancel_job')
      }
      setScanStatus('Download canceled and temporary data cleanup started.')
    } catch (error) {
      setScanStatus(`Download canceled, but cleanup reported: ${String(error)}`)
    } finally {
      // Clear immediately; the backend job-cleared event repeats this after the
      // journal is removed and again when the worker has fully exited.
      setJob(null)
      setDownloadRate(0)
      downloadRateWindowRef.current = null
      setVerifyStatus(null)
    }
  }

  return (
    <div className={preferences.reduceMotion ? 'app-root reduce-motion' : 'app-root'}>
      <CustomTitleBar closeBehavior={preferences.closeBehavior} />
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
              invoke('apply_launcher_update')
                .catch((e) => setLauncherUpdateStatus(`Failed: ${e}`))
            }}
            disabled={!!launcherUpdateStatus}
          >
            Update Now
          </button>
        </div>
      ) : null}
      <main className="launcher-shell">
        <Sidebar
        serviceStatus={contentServiceLabel(snapshot.proxyStatus)}
        activeTab={activeTab}
        onSelect={setActiveTab}
        updateCount={updateReadyGameIds.length}
        downloadCount={hasVisibleJob ? 1 : 0}
      />
      <section className="workspace">
        {activeTab !== 'Store' && activeTab !== 'Library' && activeTab !== 'Settings' && selectedGame && activeDetail ? (
          <OperationHero
            game={selectedGame}
            detail={activeDetail}
            assets={assetUrls}
            currentVersion={selectedCurrentVersion}
            latestVersion={latestCatalogVersion}
            updateReady={updateReady}
            showVersionAction={selectedInstalled && hasVersionChoices}
            updateSize={effectiveDownloadSize}
            onUpdate={openVersionOptions}
            onPlay={playSelectedGame}
            isJobRunning={isRunning}
            isGameRunning={playingGames[selectedGame.id] || false}
            canUpdate={canUpdate}
            installMode={installMode}
            selectedVersion={targetVersion}
          />
        ) : null}

        {activeTab === 'Settings' ? (
          <SettingsView
            preferences={preferences}
            launcherSettings={launcherSettings}
            onChange={updatePreference}
            onLauncherSettingChange={(key, value) => void updateLauncherSetting(key, value)}
            onChooseLibrary={() => void chooseDefaultLibraryRoot()}
            onOpenLibrary={() => void openDefaultLibraryRoot()}
            onOpenCache={() => setActiveTab('Cache')}
            onChooseCloudRoot={() => void chooseCloudSaveRoot()}
            onOpenCloudRoot={() => void openCloudSaveRoot()}
            onCheckForUpdates={() => void checkLauncherUpdateNow()}
            steamEnvironment={steamEnvironment}
            steamStatus={steamSettingsStatus}
            onRefreshSteam={() => void refreshSteamEnvironment(true)}
            onOpenSteam={() => void openSteamFromSettings('open_steam')}
            onOpenBigPicture={() => void openSteamFromSettings('open_steam_big_picture')}
            onReset={resetLauncherPreferences}
            updateStatus={settingsUpdateStatus}
          />
        ) : (
        <ActiveView
          activeTab={activeTab}
          catalog={activeTab === 'Updates' ? updatesCatalog : activeTab === 'Library' ? libraryCatalog : catalog}
          catalogLoadState={catalogLoadState}
          onRetryCatalog={() => void loadCatalog()}
          selectedGame={selectedGame}
          selectedGameId={selectedGameId}
          onSelectGame={(gameId) => {
            versionPlanSequenceRef.current += 1
            setSelectedGameId(gameId)
            setShowInstallOptions(false)
            setLaunchOptions(null)
            const game = catalog.games.find((candidate) => candidate.id === gameId)
            if (game) {
              const latest = game.availableVersions.find((version) => version.latest)?.version ?? game.latestVersion
              setSelectedVersion(latest)
              setInstallRoot(installMetadataForStoreRoot(game, game.install, preferences.defaultLibraryRoot).defaultInstallFolder)
              if (game.id !== DEFAULT_GAME_ID) {
                setInstallPath('')
                setScanStatus('No install found')
              }
            }
          }}
          onRequestAsset={requestGameAsset}
          detail={activeDetail}
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
          showVersionAction={selectedInstalled && hasVersionChoices}
          canUpdate={canUpdate}
          isJobRunning={isRunning}
          isGameRunning={selectedGame ? playingGames[selectedGame.id] || false : false}
          onBrowse={chooseInstallFolder}
          onScan={() => scanFolder()}
          onPrimaryAction={openVersionOptions}
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
            void openVersionOptions()
          }}
          onPause={pauseOrResume}
          onCancel={cancelJob}
          onResume={resumeFailedJob}
          isPaused={isPaused}
          logs={activeJob.logs}
          onOpenStore={() => {
            setSelectedGameId(null)
            setActiveTab('Store')
          }}
          cloudSaveStatus={cloudSaveStatus}
          cloudSaveBusy={cloudSaveBusy}
          cloudLaunchBlocked={cloudLaunchBlocked}
          onToggleCloudSave={(enabled) => void toggleCloudSave(enabled)}
          onAddCloudSaveFolder={() => void addCloudSaveFolder()}
          onSyncCloudSave={() => void syncCloudSave()}
          onResolveCloudConflict={(conflictId, resolution) => void resolveCloudConflict(conflictId, resolution)}
          onRestoreCloudSnapshot={(snapshotId) => void restoreCloudSnapshot(snapshotId)}
          onLaunchWithoutCloudSync={launchWithoutCloudSync}
          onConnectGoogleDrive={() => void connectAndBackupGoogleDrive()}
          onDisconnectGoogleDrive={() =>
            void runGoogleDriveAction('disconnect_google_drive', 'Disconnecting Google Drive...')
          }
          onBackupGoogleDrive={() =>
            void runGoogleDriveAction('backup_save_game_to_google_drive', 'Backing up save files to Google Drive...')
          }
          onRestoreMissingSaveFiles={() =>
            void runGoogleDriveAction('restore_missing_save_files', 'Checking Google Drive for missing save files...')
          }
        />
        )}
        {showInstallOptions && selectedGame && activeDetail ? (
          <InstallOptionsDialog
            detail={activeDetail}
            mode={installMode ? 'install' : 'version'}
            currentVersion={selectedCurrentVersion}
            selectedVersion={targetVersion}
            availableVersions={availableVersions}
            versionInfos={selectedGame.availableVersions.length > 0 ? selectedGame.availableVersions : activeDetail.versions}
            downloadSize={effectiveDownloadSize}
            installRoot={installMode ? installRoot : selectedInstallPath}
            downloadingRoot={downloadPathForInstallRoot(installMode ? installRoot : selectedInstallPath, gameInstall)}
            canStart={canApplySelectedVersion}
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
        {launchOptions && selectedGame ? (
          <LaunchOptionsModal
            gameTitle={selectedGame.title}
            config={launchOptions}
            onClose={() => setLaunchOptions(null)}
            onLaunch={(optionId, optionTitle) => {
              setLaunchOptions(null)
              void doLaunchGame(optionId, optionTitle)
            }}
          />
        ) : null}
        {launchSplash ? <LaunchSplash splash={launchSplash} /> : null}

        {showUninstallConfirm && selectedGame ? (
          <div className="dialog-backdrop" role="presentation">
            <section className="install-modal" role="dialog" aria-modal="true" aria-labelledby="uninstall-title">
              <div className="modal-handle" />
              <header>
                <button type="button" onClick={() => setShowUninstallConfirm(false)} aria-label="Cancel">
                  <X size={17} />
                </button>
                <h2 id="uninstall-title">Confirm Uninstall</h2>
                <p>Are you sure you want to uninstall {selectedGame.title}?</p>
              </header>
              <div className="install-modal-body">
                <div className="warning-box" style={{ background: 'rgba(255, 60, 60, 0.1)', border: '1px solid rgba(255, 60, 60, 0.3)', padding: '16px', borderRadius: '8px', color: '#ffb3b3' }}>
                  <CircleAlert size={20} style={{ display: 'inline-block', verticalAlign: 'middle', marginRight: '10px' }} />
                  <span style={{ verticalAlign: 'middle' }}>This will permanently delete all local files for this game from your hard drive.</span>
                </div>
              </div>
              <footer>
                <button type="button" className="secondary" onClick={() => setShowUninstallConfirm(false)}>
                  Cancel
                </button>
                <button type="button" className="danger-control" onClick={executeUninstall} style={{ padding: '8px 24px', background: '#e53935', color: '#fff', border: 'none', borderRadius: '4px', fontWeight: 'bold' }}>
                  Uninstall
                </button>
              </footer>
            </section>
          </div>
        ) : null}

        {showSteamRecommendation && (
          <div className="dialog-backdrop" role="presentation">
            <section className="install-modal" role="dialog" aria-modal="true" aria-labelledby="steam-recommendation-title">
              <div className="modal-handle" />
              <header>
                <button type="button" onClick={() => setShowSteamRecommendation(false)} aria-label="Cancel">
                  <X size={17} />
                </button>
                <h2 id="steam-recommendation-title">Steam recommended</h2>
                <p>We're recommended you to open Steam to play online</p>
              </header>
              <div className="install-modal-body">
                <div style={{ background: 'rgba(77, 164, 255, 0.1)', border: '1px solid rgba(77, 164, 255, 0.35)', padding: '16px', borderRadius: '8px', color: '#b3d8ff', lineHeight: 1.7 }}>
                  Steam is not running. Opening it first improves compatibility for online play and Steam-based services.
                </div>
              </div>
              <footer>
                <button
                  type="button"
                  className="secondary"
                  disabled={steamOpening}
                  onClick={() => {
                    setShowSteamRecommendation(false)
                    void continuePlaySelectedGame()
                  }}
                >
                  Continue anyway
                </button>
                <button
                  type="button"
                  className="primary-control downloading-btn"
                  disabled={steamOpening}
                  onClick={() => void openSteamAndContinue()}
                >
                  {steamOpening ? 'Opening Steam...' : 'Open Steam and Play'}
                </button>
              </footer>
            </section>
          </div>
        )}

        {showSpacewarPrompt && (
          <div className="dialog-backdrop" role="presentation">
            <section className="install-modal" role="dialog" aria-modal="true" aria-labelledby="spacewar-title">
              <div className="modal-handle" />
              <header>
                <button type="button" onClick={() => setShowSpacewarPrompt(false)} aria-label="Cancel">
                  <X size={17} />
                </button>
                <h2 id="spacewar-title">⚙️ Yêu cầu: Spacewar (App 480)</h2>
                <p>Launcher cần <strong>Spacewar</strong> được cài trên Steam để khởi chạy game. Đây là game miễn phí, mọi tài khoản Steam đều có thể tải.</p>
              </header>
              <div className="install-modal-body">
                <div style={{ background: 'rgba(77, 164, 255, 0.1)', border: '1px solid rgba(77, 164, 255, 0.35)', padding: '16px', borderRadius: '8px', color: '#b3d8ff', lineHeight: 1.7 }}>
                  <p style={{ margin: 0 }}>
                    🎮 Nhấn <strong>"Tải Spacewar"</strong> → Steam sẽ mở và tự động tải về.<br />
                    Sau khi tải xong (chỉ ~15MB), nhấn <strong>Play</strong> lại trên Launcher.
                  </p>
                </div>
              </div>
              <footer>
                <button type="button" className="secondary" onClick={() => setShowSpacewarPrompt(false)}>
                  Hủy
                </button>
                <button
                  type="button"
                  className="primary-control downloading-btn"
                  style={{ padding: '8px 20px', borderRadius: '6px', fontWeight: 'bold', display: 'flex', alignItems: 'center', gap: '8px' }}
                  disabled={spacewarDownloading}
                  onClick={async () => {
                    setSpacewarDownloading(true)
                    try {
                      await invoke('install_spacewar')
                      setScanStatus('Steam đang tải Spacewar (app 480). Vui lòng chờ Steam xong rồi nhấn Play lại.')
                    } catch (e) {
                      setScanStatus('Không thể mở Steam: ' + String(e))
                    } finally {
                      setSpacewarDownloading(false)
                      setShowSpacewarPrompt(false)
                    }
                  }}
                >
                  <Download size={16} />
                  {spacewarDownloading ? 'Đang mở Steam...' : 'Tải Spacewar qua Steam'}
                </button>
              </footer>
            </section>
          </div>
        )}
      </section>
    </main>
    </div>
  )
}
