import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { collection, addDoc, serverTimestamp } from 'firebase/firestore'
import { db } from './firebase'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getVersion } from '@tauri-apps/api/app'
import { open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  isPermissionGranted,
  onAction as onNativeNotificationAction,
  registerActionTypes,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import { MotionConfig, AnimatePresence } from 'motion/react'
import { CircleAlert, Download, Heart, X, Cloud } from 'lucide-react'
import packageMetadata from '../package.json'
import './App.css'
import './premium.css'
import type {
  AssetBlob,
  ClearCacheReport,
  CloudSaveRoot,
  CloudSaveStatus,
  DiscordAuthStatus,
  GameCatalog,
  GameDetail,
  GameInstallState,
  GameRuntimeState,
  GameSummary,
  JobJournal,
  LaunchReport,
  LauncherUpdateInfo,
  LauncherUpdateProgress,
  LauncherSettings,
  LaunchSplashState,
  NewNotification,
  NotificationAction,
  NotificationRecord,
  PushNotificationResult,
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
import donateImage from './assets/donate/donate.png'
import { DEFAULT_GAME_ID, DEFAULT_STORE_ROOT, fallbackCatalog, fallbackInstall, fallbackSnapshot, gameFolderName, installMetadataForStoreRoot } from './lib/installPaths'
import { collectAssetIds, contentServiceLabel, downloadPathForInstallRoot, fallbackDetailFromSummary, firstMediaUrl, isTauriRuntime, versionOptions } from './lib/gameMeta'
import { createIdleJob, getPhaseProgress } from './lib/jobProgress'
import { formatBytes } from './lib/format'
import { gameHasTag } from './lib/gameTags'
import { DEFAULT_LAUNCHER_PREFERENCES, loadLauncherPreferences, saveLauncherPreferences, type LauncherPreferences } from './lib/preferences'
import {
  ActiveView,
  CloudRedirectSettings,
  CustomTitleBar,
  DriveLibraryPickerModal,
  DiscordAccessGate,
  HomeView,
  InstallOptionsDialog,
  LaunchOptionsModal,
  LaunchSplash,
  IntroScreen,
  NotificationToasts,
  NvidiaToast,
  Onboarding,
  OperationHero,
  SettingsView,
  Sidebar,
  UpdateBanner,
  UpdateCenter,
  FirebaseRemoteControl,
  ChangelogModal,
  BigPictureView,
  DefenderExclusionDialog,
} from './components'
import { useLocale } from './context/LocaleContext'

const initialLauncherPreferences = loadLauncherPreferences()
const emptyCatalog: GameCatalog = { defaultLocale: 'en-US', games: [] }
const initialDiscordAuthStatus: DiscordAuthStatus = {
  state: isTauriRuntime() ? 'checking' : 'notConfigured',
  configured: false,
  message: isTauriRuntime()
    ? 'Checking your Discord access...'
    : 'Discord access verification requires the desktop launcher.',
  user: null,
  guildId: '1492076309323714570',
  guildName: null,
  guildInvite: 'https://discord.gg/7ZXdTUVsJE',
  eligibleAt: null,
}
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
  directToStaging: true,
  cloudSaveRoot: '',
  gameUpdateMode: 'automatic',
  gameUpdateScheduleStart: '02:00',
  gameUpdateScheduleEnd: '06:00',
  depotHfRepoId: '',
}

import { useRealtimeGameTags } from './hooks/useRealtimeGameTags'
import { useFirestoreCatalog } from './hooks/useFirestoreCatalog'
import { useSteamAppIds } from './hooks/useSteamAppIds'
import { useFirestoreDetail } from './hooks/useFirestoreDetail'
import { useRealtimeAssets } from './hooks/useRealtimeAssets'
import { useScrollReveal } from './hooks/useScrollReveal'
import { useDefenderExclusion } from './hooks/useDefenderExclusion'
import { GlobalChatSync } from './components/GlobalChatSync'
import { NoInternetView } from './components/NoInternetView'

export default function App() {
  const { t } = useLocale()
  useEffect(() => {
    // Smooth scrolling is handled by CSS scroll-behavior: smooth on .workspace.
    // We keep a minimal __lenis stub so modal code (lenis.stop/start) doesn't crash.
    const workspace = document.querySelector<HTMLElement>('.workspace')
    const stub = {
      stop: () => { if (workspace) workspace.style.overflow = 'hidden' },
      start: () => { if (workspace) workspace.style.overflow = '' },
    }
      ; (window as unknown as Record<string, unknown>).__lenis = stub
    return () => {
      ; (window as unknown as Record<string, unknown>).__lenis = null
    }
  }, [])



  // Google Antigravity–style scroll reveal (fade+slide on scroll into view)
  useScrollReveal()

  useRealtimeGameTags()
  const assetOverrideVersion = useRealtimeAssets()
  const firestoreCatalog = useFirestoreCatalog(assetOverrideVersion)
  const defenderExclusion = useDefenderExclusion()
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot)
  const [job, setJob] = useState<JobJournal | null>(fallbackSnapshot.lastJob)
  const [installPath, setInstallPath] = useState('')
  const [scanStatus, setScanStatus] = useState('No install found')
  const [, setHasScanned] = useState(false)
  const [preferences, setPreferences] = useState<LauncherPreferences>(initialLauncherPreferences)
  const [launcherSettings, setLauncherSettings] = useState<LauncherSettings>(defaultLauncherSettings)
  const [activeTab, setActiveTab] = useState<TabId>(initialLauncherPreferences.startupPage)
  const [isOnline, setIsOnline] = useState(navigator.onLine)
  const [offlineModeEnabled, setOfflineModeEnabled] = useState(false)
  const [selectedVersion, setSelectedVersion] = useState('')
  const [showInstallOptions, setShowInstallOptions] = useState(false)
  const [isStartingDownload, setIsStartingDownload] = useState(false)
  const [installRoot, setInstallRoot] = useState(`${initialLauncherPreferences.defaultLibraryRoot}\\common\\007 First Light`)
  const [catalog, setCatalog] = useState<GameCatalog>(() => (isTauriRuntime() ? emptyCatalog : fallbackCatalog))
  const [catalogLoadState, setCatalogLoadState] = useState<CatalogLoadState>(
    isTauriRuntime() ? 'loading' : 'ready',
  )
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)
  const [detail, setDetail] = useState<GameDetail | null>(null)
  const [isBigPictureMode, setIsBigPictureMode] = useState(false)
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({})
  const assetUrlsRef = useRef<Record<string, string>>({})
  const catalogRef = useRef<GameCatalog>(catalog)
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
  const autoResumeInFlightRef = useRef(false)
  const autoResumeJobIdRef = useRef<string | null>(null)
  const selectedGameIdRef = useRef<string | null>(selectedGameId)
  const versionPlanSequenceRef = useRef(0)
  const [downloadRate, setDownloadRate] = useState(0)
  const [verifyStatus, setVerifyStatus] = useState<VerifyUiStatus | null>(null)
  const [launchSplash, setLaunchSplash] = useState<LaunchSplashState | null>(null)
  const [showIntro, setShowIntro] = useState(true)
  // introExiting: true when exit animation starts (gate should be visible)
  const [introExiting, setIntroExiting] = useState(false)
  const [launchOptions, setLaunchOptions] = useState<ResolvedGameLaunchConfig | null>(null)
  const [launcherUpdate, setLauncherUpdate] = useState<LauncherUpdateInfo | null>(null)
  const [launcherUpdateProgress, setLauncherUpdateProgress] = useState<LauncherUpdateProgress | null>(null)
  const [launcherUpdateSpeed, setLauncherUpdateSpeed] = useState(0)
  const [launcherUpdateEta, setLauncherUpdateEta] = useState<number | null>(null)
  const [showUpdateCenter, setShowUpdateCenter] = useState(false)
  const [settingsUpdateStatus, setSettingsUpdateStatus] = useState<string | null>(null)
  const [showDrivePicker, setShowDrivePicker] = useState(false)
  const [showUninstallConfirm, setShowUninstallConfirm] = useState(false)
  const [playingGames, setPlayingGames] = useState<Record<string, boolean>>({})
  const [showNvidiaToast, setShowNvidiaToast] = useState(false)
  const nvidiaToastTimersRef = useRef<number[]>([])
  const [showSpacewarPrompt, setShowSpacewarPrompt] = useState(false)
  const [spacewarDownloading, setSpacewarDownloading] = useState(false)
  const [showSteamRecommendation, setShowSteamRecommendation] = useState(false)
  const [steamOpening, setSteamOpening] = useState(false)
  const [steamEnvironment, setSteamEnvironment] = useState<SteamEnvironmentInfo | null>(null)
  const [steamSettingsStatus, setSteamSettingsStatus] = useState<string | null>(null)
  const [cloudSaveStatus, setCloudSaveStatus] = useState<CloudSaveStatus | null>(null)
  const [cloudSaveBusy, setCloudSaveBusy] = useState(false)
  const [cloudLaunchBlocked, setCloudLaunchBlocked] = useState(false)
  const [runtimeStates, setRuntimeStates] = useState<GameRuntimeState[]>([])
  const [notifications, setNotifications] = useState<NotificationRecord[]>([])
  const [toastNotifications, setToastNotifications] = useState<NotificationRecord[]>([])
  const [notificationOpen, setNotificationOpen] = useState(false)
  const [showDonate, setShowDonate] = useState(false)
  const [showLogoutConfirm, setShowLogoutConfirm] = useState(false)
  const [discordAuth, setDiscordAuth] = useState<DiscordAuthStatus>(initialDiscordAuthStatus)
  const [discordAuthBusy, setDiscordAuthBusy] = useState(false)
  const [luaModeEnabled, setLuaModeEnabled] = useState(false)
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false)

  // Block notifications & big picture during intro or Discord verification
  const isBlockedState = showIntro || discordAuth.state === 'checking'
  const [cacheBusy, setCacheBusy] = useState(false)
  const [appVersion, setAppVersion] = useState(packageMetadata.version)
  const [showWhatsNewModal, setShowWhatsNewModal] = useState(false)
  const launcherUpdateRateRef = useRef<Array<{ bytes: number; at: number }>>([])
  const pendingHomeLaunchRef = useRef<string | null>(null)
  const playingGamesRef = useRef<Record<string, boolean>>({})
  const lastDiscordCheckRef = useRef(0)
  const [systemReducedMotion, setSystemReducedMotion] = useState(
    () => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches,
  )
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

  // Show window (small, centered) as soon as app mounts — before auth check
  useEffect(() => {
    if (!isTauriRuntime()) return
    import('@tauri-apps/api/window').then((m) => {
      m.getCurrentWindow().show().catch(() => { })
    })
  }, [])

  useEffect(() => {
    catalogRef.current = catalog
  }, [catalog])

  useEffect(() => {
    selectedGameIdRef.current = selectedGameId
  }, [selectedGameId])

  useEffect(() => {
    preferencesRef.current = preferences
    saveLauncherPreferences(preferences)
  }, [preferences])

  useEffect(() => {
    playingGamesRef.current = playingGames
  }, [playingGames])

  // game-started / game-exited merged into launcher://game-started/launcher://game-exited below

  useEffect(() => {
    if (!import.meta.env.DEV || isTauriRuntime()) return
    const preview = new URLSearchParams(window.location.search).get('preview')
    if (preview !== 'update') return
    const info: LauncherUpdateInfo = {
      version: `${packageMetadata.version}-preview`,
      notes: 'Premium Home dashboard\nAccurate Update Center phases\nNotification history and Windows notifications',
      publishedAt: new Date().toISOString(),
    }
    queueMicrotask(() => {
      setLauncherUpdate(info)
      setLauncherUpdateProgress({
        version: info.version,
        phase: 'downloading',
        downloadedBytes: 128 * 1024 * 1024,
        totalBytes: 272 * 1024 * 1024,
        timestamp: new Date().toISOString(),
        error: null,
      })
      setLauncherUpdateSpeed(12.4 * 1024 * 1024)
      setLauncherUpdateEta(12)
      setShowUpdateCenter(true)
    })
  }, [])

  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => setSystemReducedMotion(media.matches)
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  // Listen for navigation events from CloudRedirect
  useEffect(() => {
    const handleNavigation = (e: Event) => {
      const customEvent = e as CustomEvent<string>
      const tabId = customEvent.detail as TabId
      setActiveTab(tabId)
    }
    window.addEventListener('navigate-to-tab', handleNavigation)
    return () => window.removeEventListener('navigate-to-tab', handleNavigation)
  }, [])

  const reducedMotion =
    preferences.motionMode === 'reduced' ||
    (preferences.motionMode === 'system' && systemReducedMotion)

  const refreshDiscordAccess = useCallback(async (force = false) => {
    if (!isTauriRuntime()) {
      setDiscordAuth((prev) => ({ ...prev }))
      return
    }
    const now = Date.now()
    if (!force && now - lastDiscordCheckRef.current < 60_000) return
    lastDiscordCheckRef.current = now
    setDiscordAuthBusy(true)
    try {
      const next = await invoke<DiscordAuthStatus>('get_discord_auth_status')
      setDiscordAuth(next)
      if (next.state === 'authorized') {
        import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().maximize().catch(() => { }))
      }
    } catch (error) {
      setDiscordAuth((current) => ({
        ...current,
        state: 'error',
        message: `Discord access check failed: ${String(error)}`,
      }))
    } finally {
      setDiscordAuthBusy(false)
    }
  }, [])

  const loginDiscord = useCallback(async () => {
    if (!isTauriRuntime()) return
    setDiscordAuthBusy(true)
    try {
      const next = await invoke<DiscordAuthStatus>('login_discord')
      lastDiscordCheckRef.current = Date.now()
      setDiscordAuth(next)
      if (next.state === 'authorized') {
        import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().maximize().catch(() => { }))
      }
    } catch (error) {
      setDiscordAuth((current) => ({
        ...current,
        state: 'error',
        message: String(error),
      }))
    } finally {
      setDiscordAuthBusy(false)
    }
  }, [])

  const logoutDiscord = useCallback(() => {
    if (!isTauriRuntime()) return
    setShowLogoutConfirm(true)
  }, [])

  const executeLogoutDiscord = useCallback(async () => {
    setShowLogoutConfirm(false)
    setDiscordAuthBusy(true)
    try {
      const next = await invoke<DiscordAuthStatus>('logout_discord')
      lastDiscordCheckRef.current = 0
      setDiscordAuth(next)
    } catch (error) {
      setDiscordAuth((current) => ({ ...current, state: 'error', message: String(error) }))
    } finally {
      setDiscordAuthBusy(false)
    }
  }, [])

  // Only run Discord auth check after intro starts exiting (2400ms)
  useEffect(() => {
    if (!introExiting) return
    void refreshDiscordAccess(true)
    if (!isTauriRuntime()) return
    const interval = window.setInterval(() => void refreshDiscordAccess(true), 10 * 60_000)
    const handleFocus = () => void refreshDiscordAccess()
    window.addEventListener('focus', handleFocus)
    return () => {
      window.clearInterval(interval)
      window.removeEventListener('focus', handleFocus)
    }
  }, [introExiting, refreshDiscordAccess])

  const upsertNotification = useCallback((record: NotificationRecord) => {
    setNotifications((current) => {
      const without = current.filter((item) => item.id !== record.id)
      return [record, ...without].slice(0, 200)
    })
  }, [])

  const publishNotification = useCallback(async (notification: NewNotification) => {
    // Block notifications during intro or Discord verification
    if (isBlockedState) {
      console.log('[0xoToast] Blocked: intro or Discord check in progress')
      return
    }

    const currentPreferences = preferencesRef.current

    // Debug logging
    console.log('[0xoToast] Publishing notification:', notification)
    console.log('[0xoToast] Category enabled:', currentPreferences.notificationCategories[notification.category])
    console.log('[0xoToast] In-app enabled:', currentPreferences.inAppNotifications)

    if (!currentPreferences.notificationCategories[notification.category]) return
    const result = isTauriRuntime()
      ? await invoke<PushNotificationResult>('push_notification', { notification }).catch(() => null)
      : {
        inserted: true,
        record: {
          ...notification,
          id: `preview-${Date.now()}`,
          timestamp: new Date().toISOString(),
          read: false,
        },
      }
    if (!result?.inserted) return
    upsertNotification(result.record)

    const gameRunning = Object.values(playingGamesRef.current).some(Boolean)
    const suppressPopup = currentPreferences.doNotDisturbWhilePlaying && gameRunning

    console.log('[0xoToast] Game running:', gameRunning, 'Suppress:', suppressPopup)

    if (currentPreferences.inAppNotifications && !suppressPopup) {
      console.log('[0xoToast] Showing toast notification')
      setToastNotifications((current) => [result.record, ...current].slice(0, 3))
      window.setTimeout(() => {
        setToastNotifications((current) => current.filter((item) => item.id !== result.record.id))
      }, result.record.severity === 'error' ? 9000 : 6000)
    }

    if (
      isTauriRuntime() &&
      currentPreferences.windowsNotifications &&
      !suppressPopup &&
      (!document.hasFocus() || document.visibilityState !== 'visible')
    ) {
      let granted = await isPermissionGranted().catch(() => false)
      if (!granted) granted = (await requestPermission().catch(() => 'denied')) === 'granted'
      if (granted) {
        sendNotification({
          id: notificationIdToNumber(result.record.id),
          title: result.record.title,
          body: result.record.message,
          silent: !currentPreferences.notificationSound,
          actionTypeId: '0xolemon-open',
          extra: { notificationId: result.record.id },
        })
      }
    }
  }, [upsertNotification, isBlockedState])

  useEffect(() => {
    const handleCustomToast = (e: Event) => {
      const customEvent = e as CustomEvent<NewNotification>
      void publishNotification(customEvent.detail)
    }
    window.addEventListener('0xo-toast', handleCustomToast)
    return () => window.removeEventListener('0xo-toast', handleCustomToast)
  }, [publishNotification])

  // Listen for navigation requests
  useEffect(() => {
    const handleNavigateToSettings = (e: Event) => {
      const customEvent = e as CustomEvent<{ section?: string }>
      setActiveTab('Settings')
      // Optionally scroll to specific section
      if (customEvent.detail?.section) {
        setTimeout(() => {
          const element = document.getElementById(customEvent.detail.section!)
          element?.scrollIntoView({ behavior: 'smooth', block: 'start' })
        }, 100)
      }
    }
    window.addEventListener('navigate-to-settings', handleNavigateToSettings)
    return () => window.removeEventListener('navigate-to-settings', handleNavigateToSettings)
  }, [])

  const routeNotificationAction = useCallback((action: NotificationAction | null) => {
    if (!action) return
    if (action.gameId) setSelectedGameId(action.gameId)
    if (action.kind === 'update-center') setShowUpdateCenter(true)
    if (action.tab) setActiveTab(action.tab)
    setNotificationOpen(false)
  }, [])

  const openNotificationRecord = useCallback((notification: NotificationRecord) => {
    setNotifications((current) =>
      current.map((item) => (item.id === notification.id ? { ...item, read: true } : item)),
    )
    routeNotificationAction(notification.action)
    if (isTauriRuntime()) {
      void invoke('open_notification_action', { notificationId: notification.id }).catch(() => undefined)
    }
  }, [routeNotificationAction])

  const enableWindowsNotifications = useCallback(async () => {
    if (!isTauriRuntime()) {
      setPreferences((current) => ({ ...current, windowsNotifications: false }))
      return false
    }
    let granted = await isPermissionGranted().catch(() => false)
    if (!granted) granted = (await requestPermission().catch(() => 'denied')) === 'granted'
    setPreferences((current) => ({ ...current, windowsNotifications: granted }))
    return granted
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    void getVersion()
      .then((version) => {
        setAppVersion(version)
        const pendingVersion = window.localStorage.getItem('0xo_pending_launcher_update')
        if (pendingVersion === version) {
          window.localStorage.removeItem('0xo_pending_launcher_update')
          setShowWhatsNewModal(true)
          void publishNotification({
            category: 'launcher',
            severity: 'success',
            title: `Launcher updated to ${version}`,
            message: 'The signed launcher update was installed successfully.',
            dedupeKey: `launcher-update:${version}:completed`,
            entity: { kind: 'launcher-update', id: version },
            action: { kind: 'open-home', tab: 'Home', gameId: null },
          })
        }
      })
      .catch(() => undefined)
    void invoke<NotificationRecord[]>('list_notifications').then(setNotifications).catch(() => undefined)
    void registerActionTypes([
      {
        id: '0xolemon-open',
        actions: [{ id: 'open', title: 'Open launcher', foreground: true }],
      },
    ]).catch(() => undefined)

    let disposeNotification: (() => void) | undefined
    let disposeAction: (() => void) | undefined
    let disposeNativeAction: (() => void) | undefined
    listen<NotificationRecord>('launcher://notification', (event) => upsertNotification(event.payload))
      .then((dispose) => {
        disposeNotification = dispose
      })
      .catch(() => undefined)
    listen<NotificationAction>('launcher://notification-action', (event) => routeNotificationAction(event.payload))
      .then((dispose) => {
        disposeAction = dispose
      })
      .catch(() => undefined)
    onNativeNotificationAction((notification) => {
      const notificationId = notification.extra?.notificationId
      if (typeof notificationId === 'string') {
        void invoke('open_notification_action', { notificationId }).catch(() => undefined)
      }
    })
      .then((dispose) => {
        disposeNativeAction = dispose.unregister
      })
      .catch(() => undefined)

    return () => {
      disposeNotification?.()
      disposeAction?.()
      disposeNativeAction?.()
    }
  }, [publishNotification, routeNotificationAction, upsertNotification])

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

  const refreshRuntimeStates = useCallback(() => {
    if (!isTauriRuntime()) return Promise.resolve()
    return invoke<GameRuntimeState[]>('get_game_runtime_states')
      .then(setRuntimeStates)
      .catch(() => undefined)
  }, [])

  useEffect(() => {
    void refreshRuntimeStates()
    if (!isTauriRuntime()) return
    let startedDispose: (() => void) | undefined
    let exitedDispose: (() => void) | undefined
    let achievementDispose: (() => void) | undefined
    let errorDispose: (() => void) | undefined

    const clearNvidiaToastTimers = () => {
      for (const timer of nvidiaToastTimersRef.current) {
        window.clearTimeout(timer)
      }
      nvidiaToastTimersRef.current = []
    }

    const scheduleNvidiaToast = () => {
      clearNvidiaToastTimers()
      const showTimer = window.setTimeout(() => {
        setShowNvidiaToast(true)
        nvidiaToastTimersRef.current = nvidiaToastTimersRef.current.filter((timer) => timer !== showTimer)
        const hideTimer = window.setTimeout(() => {
          setShowNvidiaToast(false)
          nvidiaToastTimersRef.current = nvidiaToastTimersRef.current.filter((timer) => timer !== hideTimer)
        }, 8000)
        nvidiaToastTimersRef.current.push(hideTimer)
      }, 25000)
      nvidiaToastTimersRef.current.push(showTimer)
    }

    listen<{ gameId: string }>('launcher://game-started', (event) => {
      setPlayingGames((current) => ({ ...current, [event.payload.gameId]: true }))
      void refreshRuntimeStates()
      scheduleNvidiaToast()
    }).then((dispose) => {
      startedDispose = dispose
    })
    listen<{ gameId: string; exitCode: number | null; sessionSeconds: number }>('launcher://game-exited', (event) => {
      setPlayingGames((current) => ({ ...current, [event.payload.gameId]: false }))
      clearNvidiaToastTimers()
      setShowNvidiaToast(false)
      void refreshRuntimeStates()
    }).then((dispose) => {
      exitedDispose = dispose
    })
    listen<{ gameId: string; id: string; name: string; description: string }>('launcher://achievement-unlocked', (event) => {
      void publishNotification({
        category: 'achievements',
        severity: 'success',
        title: `Achievement unlocked: ${event.payload.name}`,
        message: event.payload.description || 'A new achievement was recorded.',
        dedupeKey: `achievement:${event.payload.gameId}:${event.payload.id}`,
        entity: { kind: 'game', id: event.payload.gameId },
        action: { kind: 'open-game', tab: 'Library', gameId: event.payload.gameId },
      })
    }).then((dispose) => {
      achievementDispose = dispose
    })
    listen<string>('launcher://runtime-error', (event) => {
      void publishNotification({
        category: 'errors',
        severity: 'error',
        title: 'Game runtime error',
        message: event.payload,
        dedupeKey: `runtime-error:${event.payload}`,
        entity: null,
        action: { kind: 'open-library', tab: 'Library', gameId: null },
      })
    }).then((dispose) => {
      errorDispose = dispose
    })

    return () => {
      clearNvidiaToastTimers()
      startedDispose?.()
      exitedDispose?.()
      achievementDispose?.()
      errorDispose?.()
    }
  }, [publishNotification, refreshRuntimeStates])

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
    if (!game || !assetId) {
      return
    }
    if (assetUrlsRef.current[assetId] || assetRequestRef.current.has(assetId)) {
      return
    }
    assetRequestRef.current.add(assetId)

    if (!isTauriRuntime()) {
      import('./lib/remoteAssets').then(({ fetchRemoteAssetUrl }) => {
        fetchRemoteAssetUrl(assetId, game).then((remoteUrl) => {
          if (remoteUrl) {
            setAssetUrls((current) => {
              if (current[assetId]) return current
              return { ...current, [assetId]: remoteUrl }
            })
          } else {
            // Fallback to huggingface / web asset url
            import('./lib/gameMeta').then(({ fetchWebAssetUrl }) => {
              fetchWebAssetUrl(assetId).then((url) => {
                if (url) {
                  setAssetUrls((current) => {
                    if (current[assetId]) return current
                    return { ...current, [assetId]: url }
                  })
                }
              })
            })
          }
        })
      })
      return
    }
    const delay = urgent ? 0 : Math.min(1200, assetDelaySlotRef.current++ * 90)
    window.setTimeout(async () => {
      // If assetId is already a remote URL (e.g. from assets_override), use it directly
      if (assetId.startsWith('http://') || assetId.startsWith('https://')) {
        // When offline: try reading from offline_cache first
        if (!navigator.onLine) {
          try {
            const blob = await invoke<AssetBlob>('get_cached_asset', { gameId: game.id, assetId })
            const url = `data:${blob.mimeType};base64,${blob.dataBase64}`
            setAssetUrls((current) => {
              if (current[assetId]) return current
              return { ...current, [assetId]: url }
            })
            return
          } catch {
            // Not cached - no image available offline
            return
          }
        }
        setAssetUrls((current) => {
          if (current[assetId]) return current
          return { ...current, [assetId]: assetId }
        })
        return
      }

      try {
        const blob = await invoke<AssetBlob>('get_game_asset', { gameId: game.id, assetId })
        const url = `data:${blob.mimeType};base64,${blob.dataBase64}`
        setAssetUrls((current) => {
          if (current[assetId]) return current
          return { ...current, [assetId]: url }
        })
      } catch {
        // Fallback: if local asset fails (or doesn't exist), try SteamGridDB via remoteAssets.ts
        import('./lib/remoteAssets').then(({ fetchRemoteAssetUrl }) => {
          fetchRemoteAssetUrl(assetId, game).then((remoteUrl) => {
            if (remoteUrl) {
              setAssetUrls((current) => {
                if (current[assetId]) return current
                return { ...current, [assetId]: remoteUrl }
              })
            }
          })
        })
      }
    }, delay)
  }, [])

  const loadCatalog = useCallback(async () => {
    if (!isTauriRuntime()) {
      setCatalogLoadState('ready')
      return
    }

    queueMicrotask(() => setCatalogLoadState('loading'))
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

  // Sync Firestore catalog into local state when it arrives
  useEffect(() => {
    if (firestoreCatalog && firestoreCatalog.games.length > 0) {
      setCatalog(firestoreCatalog)
      setCatalogLoadState('ready')
    }
  }, [firestoreCatalog])

  useEffect(() => {
    queueMicrotask(() => void loadCatalog())
  }, [loadCatalog])

  useEffect(() => {
    if (activeTab === 'Settings') {
      queueMicrotask(() => void refreshSteamEnvironment())
    }
  }, [activeTab, refreshSteamEnvironment])

  // Check lua mode status on mount
  useEffect(() => {
    if (!isTauriRuntime()) return
    invoke<boolean>('is_lua_game_mode_enabled')
      .then(setLuaModeEnabled)
      .catch(() => setLuaModeEnabled(false))
  }, [])

  // Cache images for existing installed games for offline use
  useEffect(() => {
    if (!isTauriRuntime() || !isOnline) return
    const installedGameIds = Object.keys(installStates).filter((id) => installStates[id]?.installed)
    if (installedGameIds.length === 0) return

    const cacheTimeout = setTimeout(() => {
      for (const gameId of installedGameIds) {
        const game = catalogRef.current.games.find(g => g.id === gameId)
        if (game) {
          const assetIdsToCache = [
            game.gridAssetId,
            game.heroAssetId,
            game.logoAssetId,
            game.iconAssetId,
          ].filter((id): id is string => Boolean(id) && (id.startsWith('http://') || id.startsWith('https://')))
          for (const assetId of assetIdsToCache) {
            invoke('cache_remote_asset', { url: assetId, gameId, assetId }).catch(() => undefined)
          }
        }
      }
    }, 5000)

    return () => clearTimeout(cacheTimeout)
  }, [installStates, isOnline])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    listen<LauncherUpdateProgress>('launcher://update-progress', (event) => {
      const progress = event.payload
      setLauncherUpdateProgress(progress)
      if (progress.phase === 'downloading') {
        const now = Date.now()
        const points = [...launcherUpdateRateRef.current, { bytes: progress.downloadedBytes, at: now }]
          .filter((point) => now - point.at <= 6000)
          .slice(-8)
        launcherUpdateRateRef.current = points
        if (points.length >= 2) {
          const first = points[0]
          const last = points[points.length - 1]
          const seconds = Math.max((last.at - first.at) / 1000, 0.001)
          const sampleRate = Math.max(0, (last.bytes - first.bytes) / seconds)
          setLauncherUpdateSpeed((current) => (current > 0 ? current * 0.65 + sampleRate * 0.35 : sampleRate))
          if (progress.totalBytes && sampleRate > 1) {
            setLauncherUpdateEta(Math.max(0, (progress.totalBytes - progress.downloadedBytes) / sampleRate))
          }
        }
      } else {
        setLauncherUpdateEta(null)
      }
      const total = progress.totalBytes ?? 0
      const percent = total > 0 ? Math.min(100, Math.round((progress.downloadedBytes / total) * 100)) : null
      const labels: Record<string, string> = {
        checking: 'Checking for updates...',
        downloading: percent === null ? 'Downloading update...' : `Downloading update... ${percent}%`,
        verifying: 'Download complete. Verifying signature...',
        installing: 'Signature verified. Installing update...',
        restarting: 'Update installed. Restarting launcher...',
        failed: progress.error ? `Update failed: ${progress.error}` : 'Update failed.',
      }
      setSettingsUpdateStatus(labels[progress.phase] ?? progress.phase)
      if (progress.phase === 'failed') {
        void publishNotification({
          category: 'errors',
          severity: 'error',
          title: 'Launcher update failed',
          message: progress.error || 'The signed launcher update could not be applied.',
          dedupeKey: `launcher-update:${progress.version}:failed:${progress.error ?? 'unknown'}`,
          entity: { kind: 'launcher-update', id: progress.version || 'unknown' },
          action: { kind: 'update-center', tab: null, gameId: null },
        })
        setShowUpdateCenter(true)
      }
    })
      .then((dispose) => {
        unlisten = dispose
      })
      .catch(console.error)
    return () => unlisten?.()
  }, [publishNotification])

  useEffect(() => {
    if (!isTauriRuntime() || !preferences.autoCheckLauncherUpdates) {
      return
    }

    const updateTimer = window.setTimeout(() => {
      invoke<LauncherUpdateInfo | null>('check_launcher_update')
        .then((info) => {
          if (info) {
            setLauncherUpdate(info)
            void publishNotification({
              category: 'launcher',
              severity: 'info',
              title: `Launcher ${info.version} is available`,
              message: 'A signed launcher update is ready to download.',
              dedupeKey: `launcher-update:${info.version}:available`,
              entity: { kind: 'launcher-update', id: info.version },
              action: { kind: 'update-center', tab: null, gameId: null },
            })
          }
        })
        .catch(console.error)
    }, 1800)

    return () => window.clearTimeout(updateTimer)
  }, [preferences.autoCheckLauncherUpdates, publishNotification])

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
  const { mapping } = useSteamAppIds()
  const [steamInstalledAppIds, setSteamInstalledAppIds] = useState<number[]>([])
  const [steamBuildIds, setSteamBuildIds] = useState<Record<number, string>>({})

  useEffect(() => {
    const fetchSteamApps = () => {
      invoke<number[]>('get_installed_steam_apps')
        .then(async (appIds) => {
          setSteamInstalledAppIds(appIds)
          const buildIds: Record<number, string> = {}
          await Promise.all(
            appIds.map(async (appId) => {
              try {
                const buildId = await invoke<string | null>('get_steam_game_buildid', { appid: appId })
                buildIds[appId] = buildId || 'Unknown'
              } catch (e) {
                buildIds[appId] = 'Unknown'
              }
            })
          )
          setSteamBuildIds(buildIds)
        })
        .catch(() => undefined)
    }
    fetchSteamApps()

    const handleLuaGameModeChange = () => fetchSteamApps()
    window.addEventListener('lua-game-mode-changed' as any, handleLuaGameModeChange)
    return () => window.removeEventListener('lua-game-mode-changed' as any, handleLuaGameModeChange)
  }, [])

  const libraryCatalog = useMemo(
    () => ({
      ...catalog,
      games: catalog.games.filter((game) => {
        const isLocal = installStates[game.id]?.installed
        const appId = mapping[game.id]
        const isSteam = appId && steamInstalledAppIds.includes(appId)
        return isLocal || isSteam
      }),
    }),
    [catalog, installStates, mapping, steamInstalledAppIds],
  )

  const effectiveGameId = useMemo(() => {
    if (activeTab === 'Home' || activeTab === 'CloudRedirect' || activeTab === 'Settings') {
      return null
    }
    if (activeTab === 'Store') {
      return selectedGameId
    }
    if (activeTab === 'Library') {
      if (!selectedGameId) return null
      const isLocal = installStates[selectedGameId]?.installed
      const appId = mapping[selectedGameId]
      const isSteam = appId && steamInstalledAppIds.includes(appId)
      return (isLocal || isSteam) ? selectedGameId : null
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

  const requestHomeAsset = useCallback(
    (gameId: string, assetId: string, urgent = false) => {
      const game = catalogRef.current.games.find((candidate) => candidate.id === gameId)
      requestGameAsset(game, assetId, urgent)
    },
    [requestGameAsset],
  )

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
  // Firestore detail — used when local .0xo pack is absent (metadataSource === 'preview')
  const firestoreDetail = useFirestoreDetail(effectiveGameId)

  const activeDetail = useMemo(() => {
    const local = detail?.gameId === effectiveGameId ? detail : null
    const firestore = firestoreDetail?.gameId === effectiveGameId ? firestoreDetail : null

    // If local is missing or is the web preview stub → use Firestore entirely
    if (!local || local.metadataSource === 'preview') {
      return firestore ?? local
    }

    // Local is a full pack — but Firestore may have richer fields (achievements, media, genres).
    // Deep-merge: fill in empty arrays from Firestore so the UI is always as complete as possible.
    if (firestore) {
      return {
        ...local,
        achievements: local.achievements?.length ? local.achievements : (firestore.achievements ?? []),
        media: local.media?.length ? local.media : (firestore.media ?? []),
        genres: local.genres?.length ? local.genres : (firestore.genres ?? []),
        categories: local.categories?.length ? local.categories : (firestore.categories ?? []),
        ratings: local.ratings?.length ? local.ratings : (firestore.ratings ?? []),
        shortDescription: local.shortDescription || firestore.shortDescription,
        detailedDescription: local.detailedDescription || firestore.detailedDescription,
        releaseDate: local.releaseDate || firestore.releaseDate,
      }
    }

    return local
  }, [detail, firestoreDetail, effectiveGameId])


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
        const gameTitle =
          catalogRef.current.games.find((game) => game.id === nextJob.gameId)?.title ??
          nextJob.gameId
        void publishNotification({
          category: 'installs',
          severity: 'success',
          title:
            nextJob.kind === 'install'
              ? `${gameTitle} installed`
              : nextJob.kind === 'repair'
                ? `${gameTitle} repaired`
                : `${gameTitle} updated`,
          message: `Version ${nextJob.toVersion} committed successfully.`,
          dedupeKey: `job:${nextJob.id}:committed`,
          entity: { kind: 'game', id: nextJob.gameId },
          action: { kind: 'open-game', tab: 'Library', gameId: nextJob.gameId },
        })
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
        // Auto-cache the 4 key image assets for offline use (only remote URLs)
        window.setTimeout(() => {
          const installedGame = catalogRef.current.games.find((g) => g.id === nextJob.gameId)
          if (installedGame && isTauriRuntime()) {
            const assetIdsToCache = [
              installedGame.gridAssetId,
              installedGame.heroAssetId,
              installedGame.logoAssetId,
              installedGame.iconAssetId,
            ].filter((id): id is string => Boolean(id) && (id.startsWith('http://') || id.startsWith('https://')))
            for (const assetId of assetIdsToCache) {
              invoke('cache_remote_asset', { url: assetId, gameId: nextJob.gameId, assetId }).catch(() => undefined)
            }
          }
        }, 2000)
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
        const gameTitle =
          catalogRef.current.games.find((game) => game.id === nextJob.gameId)?.title ??
          nextJob.gameId
        void publishNotification({
          category: nextJob.status === 'failed' ? 'errors' : 'downloads',
          severity: nextJob.status === 'failed' ? 'error' : 'warning',
          title: `${titleCase(nextJob.kind)} ${nextJob.status}`,
          message: `${gameTitle} ${nextJob.kind} job ${nextJob.status}.`,
          dedupeKey: `job:${nextJob.id}:${nextJob.status}`,
          entity: { kind: 'job', id: nextJob.id },
          action: { kind: 'open-downloads', tab: 'Downloads', gameId: nextJob.gameId },
        })
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
  }, [playInstallCompleteSound, publishNotification])

  useEffect(() => {
    if (!isTauriRuntime()) return

    const canAutoResume = (current: JobJournal | null) => {
      if (!current) return false
      if (current.kind !== 'install' && current.kind !== 'update' && current.kind !== 'repair') {
        return false
      }
      if (current.status === 'paused') return true
      if (current.status !== 'failed') return false
      const stepPhase = current.steps.map((step) => `${step.name} ${step.detail}`).join(' ')
      const phase = `${current.phase ?? ''} ${stepPhase}`.toLowerCase()
      return phase.includes('download') || phase.includes('assembling') || phase.includes('verify')
    }

    const resumeInterruptedJob = async () => {
      const current = latestJobRef.current
      if (!canAutoResume(current)) return
      if (autoResumeInFlightRef.current && autoResumeJobIdRef.current === current?.id) return

      autoResumeInFlightRef.current = true
      autoResumeJobIdRef.current = current?.id ?? null
      try {
        await invoke('resume_job')
        setJob((state) => (state ? { ...state, status: 'running', phase: state.phase || 'Download packs' } : state))
        setScanStatus('Network restored, resuming download...')
      } catch (error) {
        setScanStatus(`Network restored, but auto-resume failed: ${String(error)}`)
      } finally {
        autoResumeInFlightRef.current = false
      }
    }

    const handleOnline = () => {
      setIsOnline(true)
      setOfflineModeEnabled(false)
      void resumeInterruptedJob()
      // Re-check Discord auth when network comes back
      void refreshDiscordAccess(true)
    }

    const handleOffline = () => {
      setIsOnline(false)
      if (canAutoResume(latestJobRef.current)) {
        setScanStatus('Network lost; launcher will retry when the connection is back.')
      }
    }

    window.addEventListener('online', handleOnline)
    window.addEventListener('offline', handleOffline)

    if (navigator.onLine) {
      void resumeInterruptedJob()
    }

    return () => {
      window.removeEventListener('online', handleOnline)
      window.removeEventListener('offline', handleOffline)
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
  const showVersionAction = selectedInstalled && hasVersionChoices
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
    if ((activeTab !== 'Library' && activeTab !== 'Store') || !selectedGame || !selectedInstalled) {
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
      if (disposed) return
      if (event.payload.gameId === selectedGameIdRef.current) {
        setCloudSaveStatus(event.payload.status)
        setCloudLaunchBlocked(event.payload.status.conflicts.length > 0)
      }
      if (event.payload.status.conflicts.length > 0) {
        void publishNotification({
          category: 'cloudSaves',
          severity: 'warning',
          title: 'Cloud save conflict detected',
          message: `${event.payload.status.conflicts.length} conflict${event.payload.status.conflicts.length === 1 ? '' : 's'} require a decision.`,
          dedupeKey: `cloud-conflict:${event.payload.gameId}:${event.payload.status.conflicts.map((item) => item.id).join(',')}`,
          entity: { kind: 'game', id: event.payload.gameId },
          action: { kind: 'open-cloud-save', tab: 'Library', gameId: event.payload.gameId },
        })
      }
    }).then((dispose) => {
      if (disposed) dispose()
      else unlistenStatus = dispose
    })

    listen<{ gameId: string; message: string }>('launcher://cloud-save-error', (event) => {
      if (disposed) return
      if (event.payload.gameId === selectedGameIdRef.current) setScanStatus(event.payload.message)
      void publishNotification({
        category: 'errors',
        severity: 'error',
        title: 'Cloud save failed',
        message: event.payload.message,
        dedupeKey: `cloud-error:${event.payload.gameId}:${event.payload.message}`,
        entity: { kind: 'game', id: event.payload.gameId },
        action: { kind: 'open-cloud-save', tab: 'Library', gameId: event.payload.gameId },
      })
    }).then((dispose) => {
      if (disposed) dispose()
      else unlistenError = dispose
    })

    return () => {
      disposed = true
      unlistenStatus?.()
      unlistenError?.()
    }
  }, [publishNotification])

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
    if (key === 'windowsNotifications' && value === true) {
      void enableWindowsNotifications()
      return
    }
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
    if (
      resolution === 'cloud' &&
      preferences.confirmBeforeCloudRestore &&
      !window.confirm('Use the cloud copy? The current local save will be preserved as a conflict copy before replacement.')
    ) {
      return
    }
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
      void publishNotification({
        category: 'cloudSaves',
        severity: 'success',
        title: 'Cloud save conflict resolved',
        message: resolution === 'local' ? 'The local save was kept and uploaded.' : 'The cloud save was restored locally.',
        dedupeKey: `cloud-conflict:${selectedGame.id}:${conflictId}:resolved:${resolution}`,
        entity: { kind: 'game', id: selectedGame.id },
        action: { kind: 'open-cloud-save', tab: 'Library', gameId: selectedGame.id },
      })
    } catch (error) {
      setScanStatus(`Could not resolve cloud save conflict: ${String(error)}`)
    } finally {
      setCloudSaveBusy(false)
    }
  }

  async function restoreCloudSnapshot(snapshotId: string) {
    if (!selectedGame || !isTauriRuntime()) return
    if (
      preferences.confirmBeforeCloudRestore &&
      !window.confirm('Restore this cloud-save snapshot? Current local files will be preserved before replacement.')
    ) {
      return
    }
    setCloudSaveBusy(true)
    try {
      const status = await invoke<CloudSaveStatus>('restore_cloud_save_snapshot', {
        gameId: selectedGame.id,
        snapshotId,
      })
      setCloudSaveStatus(status)
      setCloudLaunchBlocked(status.conflicts.length > 0)
      setScanStatus(status.lastMessage)
      void publishNotification({
        category: 'cloudSaves',
        severity: 'success',
        title: 'Cloud-save snapshot restored',
        message: status.lastMessage || 'The selected snapshot was restored successfully.',
        dedupeKey: `cloud-snapshot:${selectedGame.id}:${snapshotId}:restored`,
        entity: { kind: 'game', id: selectedGame.id },
        action: { kind: 'open-cloud-save', tab: 'Library', gameId: selectedGame.id },
      })
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
      if (info) {
        void publishNotification({
          category: 'launcher',
          severity: 'info',
          title: `Launcher ${info.version} is available`,
          message: 'A signed launcher update is ready to download.',
          dedupeKey: `launcher-update:${info.version}:available`,
          entity: { kind: 'launcher-update', id: info.version },
          action: { kind: 'update-center', tab: null, gameId: null },
        })
        setShowUpdateCenter(true)
      }
    } catch (error) {
      setSettingsUpdateStatus(`Update check failed: ${String(error)}`)
    }
  }

  async function applyLauncherUpdate() {
    if (!launcherUpdate || !isTauriRuntime()) return
    launcherUpdateRateRef.current = []
    setLauncherUpdateSpeed(0)
    setLauncherUpdateEta(null)
    setSettingsUpdateStatus('Preparing signed update...')
    setLauncherUpdateProgress({
      version: launcherUpdate.version,
      phase: 'downloading',
      downloadedBytes: 0,
      totalBytes: null,
      timestamp: new Date().toISOString(),
      error: null,
    })
    setShowUpdateCenter(true)
    window.localStorage.setItem('0xo_pending_launcher_update', launcherUpdate.version)
    try {
      await invoke('apply_launcher_update')
    } catch (error) {
      const message = String(error)
      setSettingsUpdateStatus(`Update failed: ${message}`)
      setLauncherUpdateProgress((current) => ({
        version: launcherUpdate.version,
        phase: 'failed',
        downloadedBytes: current?.downloadedBytes ?? 0,
        totalBytes: current?.totalBytes ?? null,
        timestamp: new Date().toISOString(),
        error: message,
      }))
    }
  }

  async function openSteamFromSettings(command: 'open_steam' | 'open_steam_big_picture' | 'restart_steam') {
    if (!isTauriRuntime()) {
      setSteamSettingsStatus('Steam actions require the desktop launcher.')
      return
    }
    setSteamSettingsStatus(
      command === 'open_steam' ? 'Opening Steam...' :
        command === 'restart_steam' ? 'Restarting Steam...' :
          'Opening Steam Big Picture...'
    )
    try {
      if (command === 'restart_steam') {
        const report = await invoke<{ wasRunning: boolean; forced: boolean; running: boolean; message: string }>('restart_steam')
        setSteamSettingsStatus(report.message)
      } else {
        await invoke(command)
      }
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

    const isSteamGame = steamInstalledAppIds.includes(mapping[selectedGame.id])
    if (isSteamGame && !selectedInstalled) {
      const proceed = window.confirm(t.library.steamDuplicateWarning)
      if (!proceed) return
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

    // Show loading state immediately to prevent spam clicks
    setIsStartingDownload(true)

    // Web app: send remote install command to PC launcher via Firebase
    if (!isTauriRuntime()) {
      if (!discordAuth.user) {
        setScanStatus('Please login with Discord to remote install.')
        setIsStartingDownload(false)
        return
      }
      try {
        await addDoc(collection(db, 'users', discordAuth.user.id, 'commands'), {
          action: 'install',
          game_id: selectedGame.id,
          timestamp: serverTimestamp()
        })
        setShowInstallOptions(false)
        void publishNotification({
          category: 'launcher',
          severity: 'info',
          title: 'Remote Command Sent',
          message: `Installation for ${selectedGame.title} will start on your PC shortly.`,
          dedupeKey: `remote-install-${selectedGame.id}`,
          entity: null,
          action: null
        })
      } catch (err) {
        setScanStatus('Failed to send remote command. Is your PC online?')
        console.error('Remote install failed:', err)
      } finally {
        setIsStartingDownload(false)
      }
      return
    }

    try {
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
    } finally {
      setIsStartingDownload(false)
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

  function openHomeGame(gameId: string) {
    setSelectedGameId(gameId)
    const isInstalled = installStates[gameId] && installStates[gameId].currentVersion !== 'not installed' && installStates[gameId].currentVersion !== 'unknown'
    setActiveTab(isInstalled ? 'Library' : 'Store')
  }

  function playHomeGame(gameId: string) {
    pendingHomeLaunchRef.current = gameId
    setSelectedGameId(gameId)
    setActiveTab('Library')
  }

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlistenNavigate: (() => void) | undefined
    listen<string>('navigate', (event) => {
      setActiveTab(event.payload as any)
      // We don't need to manually show() the window because the Rust side already calls window.show()
    }).then((dispose) => {
      unlistenNavigate = dispose
    })
    return () => {
      unlistenNavigate?.()
    }
  }, [])

  useEffect(() => {
    if (
      pendingHomeLaunchRef.current &&
      pendingHomeLaunchRef.current === selectedGame?.id &&
      activeDetail?.gameId === selectedGame.id &&
      selectedInstalled
    ) {
      pendingHomeLaunchRef.current = null
      void playSelectedGame()
    }
    // The launch is deliberately keyed to the selected game/detail transition.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDetail?.gameId, selectedGame?.id, selectedInstalled])

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

    if (!isTauriRuntime()) {
      if (!discordAuth.user) return
      await addDoc(collection(db, 'users', discordAuth.user.id, 'commands'), {
        action: 'launch',
        game_id: selectedGame.id,
        timestamp: serverTimestamp()
      })
      void publishNotification({
        category: 'launcher',
        severity: 'info',
        title: 'Remote Command Sent',
        message: `Command to launch ${selectedGame.title} sent to PC.`,
        dedupeKey: `remote-launch-${selectedGame.id}`,
        entity: null,
        action: null
      })
      return
    }

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

  async function stopSelectedGame() {
    if (!selectedGame) return
    try {
      await invoke('kill_game', { gameId: selectedGame.id })
      setPlayingGames((prev) => ({ ...prev, [selectedGame.id]: false }))
    } catch (error) {
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
      void publishNotification({
        category: 'storage',
        severity: 'success',
        title: `${selectedGame.title} uninstalled`,
        message: `Removed ${report.removedFiles} files${desktopShortcutText}${steamShortcutText}.`,
        dedupeKey: `uninstall:${selectedGame.id}:${Date.now()}`,
        entity: { kind: 'game', id: selectedGame.id },
        action: { kind: 'open-store', tab: 'Store', gameId: selectedGame.id },
      })
    } catch (error) {
      setScanStatus(String(error))
      void publishNotification({
        category: 'errors',
        severity: 'error',
        title: 'Uninstall failed',
        message: String(error),
        dedupeKey: `uninstall:${selectedGame.id}:failed:${String(error)}`,
        entity: { kind: 'game', id: selectedGame.id },
        action: { kind: 'open-game', tab: 'Library', gameId: selectedGame.id },
      })
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

    if (
      preferences.confirmBeforeCancelCleanup &&
      !window.confirm('Are you sure you want to cancel the download? This will delete temporary downloaded data for this job.')
    ) {
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
      void publishNotification({
        category: 'downloads',
        severity: 'success',
        title: 'Download canceled',
        message: 'Temporary data cleanup completed for the canceled job.',
        dedupeKey: `job:${job?.id ?? selectedGameId ?? 'active'}:cleanup-complete`,
        entity: job ? { kind: 'job', id: job.id } : null,
        action: { kind: 'open-downloads', tab: 'Downloads', gameId: selectedGameId },
      })
    } catch (error) {
      setScanStatus(`Download canceled, but cleanup reported: ${String(error)}`)
      void publishNotification({
        category: 'errors',
        severity: 'error',
        title: 'Temporary cleanup failed',
        message: String(error),
        dedupeKey: `job:${job?.id ?? selectedGameId ?? 'active'}:cleanup-failed:${String(error)}`,
        entity: job ? { kind: 'job', id: job.id } : null,
        action: { kind: 'open-downloads', tab: 'Downloads', gameId: selectedGameId },
      })
    } finally {
      // Clear immediately; the backend job-cleared event repeats this after the
      // journal is removed and again when the worker has fully exited.
      setJob(null)
      setDownloadRate(0)
      downloadRateWindowRef.current = null
      setVerifyStatus(null)
    }
  }

  async function clearLauncherCache() {
    if (!isTauriRuntime()) {
      setScanStatus('Cache cleanup requires the desktop launcher.')
      return
    }
    if (isRunning) {
      setScanStatus('Pause or finish the active download before clearing cache.')
      return
    }
    if (
      preferences.confirmBeforeClearCache &&
      !window.confirm(`Clear ${snapshot.cache.cacheSize > 0 ? 'the reusable chunk cache' : 'this cache'}? Installed game files are not affected.`)
    ) {
      return
    }
    setCacheBusy(true)
    try {
      const report = await invoke<ClearCacheReport>('clear_chunk_cache', {
        cachePath: snapshot.cache.cachePath,
      })
      const nextSnapshot = await invoke<Snapshot>('get_launcher_snapshot')
      setSnapshot(nextSnapshot)
      setScanStatus(`Cleared ${report.removedFiles} cached files (${formatBytes(report.removedBytes)}).`)
      void publishNotification({
        category: 'storage',
        severity: 'success',
        title: 'Chunk cache cleared',
        message: `Removed ${report.removedFiles} files and freed ${formatBytes(report.removedBytes)}.`,
        dedupeKey: `cache-clear:${report.cachePath}:${Date.now()}`,
        entity: { kind: 'cache', id: report.cachePath },
        action: { kind: 'open-cache', tab: 'Cache', gameId: selectedGameId },
      })
    } catch (error) {
      setScanStatus(`Cache cleanup failed: ${String(error)}`)
      void publishNotification({
        category: 'errors',
        severity: 'error',
        title: 'Cache cleanup failed',
        message: String(error),
        dedupeKey: `cache-clear:failed:${snapshot.cache.cachePath}:${String(error)}`,
        entity: { kind: 'cache', id: snapshot.cache.cachePath },
        action: { kind: 'open-cache', tab: 'Cache', gameId: selectedGameId },
      })
    } finally {
      setCacheBusy(false)
    }
  }

  const enterBigPicture = () => {
    // Block big picture during intro or Discord verification
    if (isBlockedState) {
      console.log('[BigPicture] Blocked: intro or Discord check in progress')
      return
    }
    setIsBigPictureMode(true)
  }

  const exitBigPicture = () => {
    setIsBigPictureMode(false)
  }

  return (
    <MotionConfig reducedMotion={reducedMotion ? 'always' : 'never'}>
      {isBigPictureMode ? (
        <AnimatePresence>
          <BigPictureView
            games={catalog.games}
            assetUrls={assetUrls}
            onExit={exitBigPicture}
            notifications={notifications}
            notificationOpen={notificationOpen}
            onToggleNotifications={() => setNotificationOpen((current) => !current)}
            onCloseNotifications={() => setNotificationOpen(false)}
            onOpenNotification={openNotificationRecord}
            onMarkAllNotificationsRead={() => {
              setNotifications((current) => current.map((item) => ({ ...item, read: true })))
              if (isTauriRuntime()) {
                void invoke<NotificationRecord[]>('mark_all_notifications_read').then(setNotifications).catch(() => undefined)
              }
            }}
            onClearNotifications={() => {
              setNotifications([])
              if (isTauriRuntime()) {
                void invoke<NotificationRecord[]>('clear_notifications').then(setNotifications).catch(() => undefined)
              }
            }}
            onOpenNotificationSettings={() => {
              setNotificationOpen(false)
              setIsBigPictureMode(false)
              setActiveTab('Settings')
              window.setTimeout(() => {
                document.getElementById('notification-settings')?.scrollIntoView({ behavior: reducedMotion ? 'auto' : 'smooth' })
              }, 80)
            }}
          />
        </AnimatePresence>
      ) : (
        <div
          className={[
            'app-root',
            reducedMotion ? 'reduce-motion' : '',
            preferences.glassEffects ? 'glass-effects' : 'no-glass-effects',
            preferences.scrollEffects ? '' : 'no-scroll-effects',
          ].filter(Boolean).join(' ')}
        >
          {showIntro && (
            <IntroScreen
              onExiting={() => setIntroExiting(true)}
              onDone={() => setShowIntro(false)}
            />
          )}
          <GlobalChatSync catalog={catalog} />
          <CustomTitleBar
            closeBehavior={preferences.closeBehavior}
            serviceOnline={!contentServiceLabel(snapshot.proxyStatus).toLowerCase().includes('unavailable')}
            job={job}
            updateProgress={launcherUpdateProgress}
            notifications={notifications}
            notificationOpen={notificationOpen}
            discordUser={discordAuth.state === 'authorized' ? discordAuth.user : null}
            statusPreferences={preferences}
            isBlockedState={isBlockedState}
            onToggleNotifications={() => setNotificationOpen((current) => !current)}
            onCloseNotifications={() => setNotificationOpen(false)}
            onOpenNotification={openNotificationRecord}
            onMarkAllNotificationsRead={() => {
              setNotifications((current) => current.map((item) => ({ ...item, read: true })))
              if (isTauriRuntime()) {
                void invoke<NotificationRecord[]>('mark_all_notifications_read').then(setNotifications).catch(() => undefined)
              }
            }}
            onClearNotifications={() => {
              setNotifications([])
              if (isTauriRuntime()) {
                void invoke<NotificationRecord[]>('clear_notifications').then(setNotifications).catch(() => undefined)
              }
            }}
            onOpenNotificationSettings={() => {
              setNotificationOpen(false)
              setActiveTab('Settings')
              window.setTimeout(() => {
                setNotificationOpen(false)
                document.getElementById('notification-settings')?.scrollIntoView({ behavior: reducedMotion ? 'auto' : 'smooth' })
              }, 80)
            }}
            onDiscordLogout={() => void logoutDiscord()}
            onToggleBigPicture={enterBigPicture}
            onToggleSidebar={() => setIsSidebarCollapsed((prev) => !prev)}
            isSidebarCollapsed={isSidebarCollapsed}
          />
          {launcherUpdate ? (
            <UpdateBanner
              update={launcherUpdate}
              progress={launcherUpdateProgress}
              onOpen={() => setShowUpdateCenter(true)}
              onStart={() => void applyLauncherUpdate()}
            />
          ) : null}
        <main className={`launcher-shell premium-shell ${isSidebarCollapsed ? 'sidebar-collapsed-shell' : ''}`}>
            <Sidebar
              serviceStatus={contentServiceLabel(snapshot.proxyStatus)}
              activeTab={activeTab}
              onSelect={setActiveTab}
              updateCount={updateReadyGameIds.length}
              downloadCount={hasVisibleJob ? 1 : 0}
              luaModeEnabled={luaModeEnabled}
              isSidebarCollapsed={isSidebarCollapsed}
              onToggleSidebar={() => setIsSidebarCollapsed((prev) => !prev)}
            />
            <section className="workspace premium-workspace">
              {['Updates', 'Downloads', 'Cache'].includes(activeTab) && selectedGame && activeDetail ? (
                <OperationHero
                  game={selectedGame}
                  detail={activeDetail}
                  assets={assetUrls}
                  currentVersion={selectedCurrentVersion}
                  latestVersion={latestCatalogVersion}
                  updateReady={updateReady}
                  showVersionAction={showVersionAction}
                  updateSize={effectiveDownloadSize}
                  onUpdate={openVersionOptions}
                  onPlay={playSelectedGame}
                  onStop={stopSelectedGame}
                  isJobRunning={isRunning}
                  isGameRunning={playingGames[selectedGame.id] || false}
                  canUpdate={canUpdate}
                  installMode={installMode}
                  selectedVersion={targetVersion}
                />
              ) : null}

              <div key={activeTab} className={reducedMotion ? undefined : 'tab-enter'}>
                {/* Offline gate: tabs requiring internet show NoInternetView when offline */}
                {!isOnline && !['Library', 'Settings'].includes(activeTab) ? (
                  <NoInternetView tabName={activeTab === 'Home' ? 'Home' : activeTab === 'Store' ? 'Store' : activeTab === "What's New!" ? "What's New" : activeTab === 'Downloads' ? 'Downloads' : activeTab === 'Updates' ? 'Updates' : activeTab === 'CloudRedirect' ? 'CloudRedirect' : activeTab === 'Translations' ? 'Translations' : undefined} />
                ) : activeTab === 'Home' ? (
                  <HomeView
                    catalog={catalog}
                    installStates={installStates}
                    runtimeStates={runtimeStates}
                    assets={assetUrls}
                    job={job}
                    launcherUpdate={launcherUpdate}
                    launcherUpdateProgress={launcherUpdateProgress}
                    preferences={preferences}
                    reducedMotion={reducedMotion}
                    onRequestAsset={requestHomeAsset}
                    onOpenGame={openHomeGame}
                    onPlayGame={playHomeGame}
                    onOpenTab={setActiveTab}
                    onOpenDiscord={() => void openUrl('https://discord.gg/7ZXdTUVsJE')}
                    onOpenDonate={() => setShowDonate(true)}
                  />
                ) : activeTab === 'CloudRedirect' ? (
                  <div className="settings-view settings-view-global" style={{ padding: '40px' }}>
                    <header className="settings-page-header">
                      <div>
                        <span className="settings-page-icon">
                          <Cloud size={21} />
                        </span>
                        <div>
                          <h1>CloudRedirect</h1>
                          <p>Cloud saves for lua games using Google Drive, OneDrive, or local folder.</p>
                        </div>
                      </div>
                    </header>
                    <CloudRedirectSettings />
                  </div>
                ) : activeTab === 'Settings' ? (
                  <SettingsView
                    preferences={preferences}
                    launcherSettings={launcherSettings}
                    onChange={updatePreference}
                    onLauncherSettingChange={<K extends keyof LauncherSettings>(key: K, value: LauncherSettings[K]) => void updateLauncherSetting(key, value)}
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
                    onRestartSteam={() => void openSteamFromSettings('restart_steam')}
                    onOpenBigPicture={() => void openSteamFromSettings('open_steam_big_picture')}
                    onReset={resetLauncherPreferences}
                    onResetOnboarding={() => {
                      updatePreference('onboardingCompleted', false)
                      setActiveTab('Home')
                    }}
                    onManageNotifications={() => setNotificationOpen(true)}
                    appVersion={appVersion}
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
                    installStates={installStates}
                    steamInstalledAppIds={steamInstalledAppIds}
                    steamBuildIds={steamBuildIds}
                    selectedInstallState={selectedInstallState}
                    verifyStatus={selectedVerifyStatus}
                    installMode={installMode}
                    updateReady={updateReady}
                    showVersionAction={showVersionAction}
                    canUpdate={canUpdate}
                    isJobRunning={isRunning}
                    isGameRunning={selectedGame ? playingGames[selectedGame.id] || false : false}
                    onBrowse={chooseInstallFolder}
                    onScan={() => scanFolder()}
                    onPrimaryAction={openVersionOptions}
                    onPlay={playSelectedGame}
                    onStop={stopSelectedGame}
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
                    cacheBusy={cacheBusy}
                    onClearCache={() => void clearLauncherCache()}
                    discordUser={discordAuth.state === 'authorized' ? discordAuth.user : null}
                  />
                )}
              </div>
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
                  isStarting={isStartingDownload}
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
              {showNvidiaToast && <NvidiaToast onDismiss={() => setShowNvidiaToast(false)} />}

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

              {showLogoutConfirm && (
                <div className="dialog-backdrop" role="presentation" onClick={() => setShowLogoutConfirm(false)}>
                  <section
                    className="logout-modal"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="logout-title"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <div className="logout-modal-icon">
                      <svg width="36" height="36" viewBox="0 0 127.14 96.36" xmlns="http://www.w3.org/2000/svg">
                        <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1A105.25,105.25,0,0,0,126.6,80.22h0C129.24,52.84,122.09,29.11,107.7,8.07ZM42.45,65.69C36.18,65.69,31,60,31,53s5-12.74,11.43-12.74S54,46,53.89,53,48.84,65.69,42.45,65.69Zm42.24,0C78.41,65.69,73.25,60,73.25,53s5-12.74,11.44-12.74S96.23,46,96.12,53,91.08,65.69,84.69,65.69Z" fill="#5865f2" />
                      </svg>
                    </div>
                    <h3 id="logout-title" className="logout-modal-title">Sign Out of Discord</h3>
                    <p className="logout-modal-desc">Are you sure you want to sign out?<br />You will need to re-authorize to use online features.</p>
                    <div className="logout-modal-actions">
                      <button type="button" className="logout-modal-btn cancel" onClick={() => setShowLogoutConfirm(false)}>
                        Cancel
                      </button>
                      <button type="button" className="logout-modal-btn confirm" onClick={() => void executeLogoutDiscord()}>
                        Sign Out
                      </button>
                    </div>
                  </section>
                </div>
              )}
            </section>
          </main>
          <UpdateCenter
            open={showUpdateCenter}
            update={launcherUpdate}
            progress={launcherUpdateProgress}
            speed={launcherUpdateSpeed}
            eta={launcherUpdateEta}
            onClose={() => setShowUpdateCenter(false)}
            onStart={() => void applyLauncherUpdate()}
            onRetry={() => void applyLauncherUpdate()}
          />
          <NotificationToasts
            notifications={toastNotifications}
            onOpen={openNotificationRecord}
            onDismiss={(notificationId) =>
              setToastNotifications((current) => current.filter((item) => item.id !== notificationId))
            }
          />
          {!preferences.onboardingCompleted ? (
            <Onboarding
              onComplete={() => updatePreference('onboardingCompleted', true)}
              onEnableWindowsNotifications={() => void enableWindowsNotifications()}
            />
          ) : null}
          {showDonate ? (
            <div className="donate-modal-backdrop" role="presentation" onMouseDown={() => setShowDonate(false)}>
              <section
                className="donate-modal"
                role="dialog"
                aria-modal="true"
                aria-labelledby="donate-title"
                onMouseDown={(event) => event.stopPropagation()}
              >
                <button type="button" className="donate-modal-close" onClick={() => setShowDonate(false)} aria-label="Close">
                  <X size={18} />
                </button>
                <div className="donate-modal-copy">
                  <span><Heart size={18} /></span>
                  <h2 id="donate-title">Support 0xoLemon</h2>
                  <p>Scan the QR code with your banking app. Donation is optional and does not unlock launcher features.</p>
                </div>
                <img src={donateImage} alt="0xoLemon donation QR code" />
              </section>
            </div>
          ) : null}
          {/* Gate shown when intro starts exiting (introExiting=true at 2400ms) so no gap */}
          <div style={!introExiting ? { visibility: 'hidden', pointerEvents: 'none' } : undefined}>
            <DiscordAccessGate
              status={offlineModeEnabled ? { ...discordAuth, state: 'authorized' } : discordAuth}
              busy={discordAuthBusy}
              onLogin={() => void loginDiscord()}
              onRefresh={() => void refreshDiscordAccess(true)}
              onJoinServer={() => void openUrl(discordAuth.guildInvite)}
              onLogout={() => void executeLogoutDiscord()}
              onEnterOfflineMode={() => setOfflineModeEnabled(true)}
            />
          </div>
          {discordAuth.state === 'authorized' && discordAuth.user ? (
            <FirebaseRemoteControl
              user={discordAuth.user}
              catalog={catalog}
              installStates={installStates}
              runtimeStates={runtimeStates}
              setCatalog={setCatalog}
              setInstallStates={setInstallStates}
              setRuntimeStates={setRuntimeStates}
            />
          ) : null}

          {showWhatsNewModal && (
            <ChangelogModal onClose={() => setShowWhatsNewModal(false)} />
          )}

          <DefenderExclusionDialog
            isOpen={defenderExclusion.isDialogOpen}
            path={defenderExclusion.exclusionPath}
            onClose={defenderExclusion.handleClose}
            onAccept={defenderExclusion.handleAccept}
          />
        </div>
      )}
    </MotionConfig>
  )
}

function notificationIdToNumber(value: string) {
  let hash = 0
  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) | 0
  }
  return Math.abs(hash || 1)
}

function titleCase(value: string) {
  return value
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}
