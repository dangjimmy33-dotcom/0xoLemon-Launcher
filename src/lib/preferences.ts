import { DEFAULT_STORE_ROOT } from './installPaths'

export type StartupPage = 'Home' | 'Store' | 'Library' | 'Updates' | 'Downloads' | 'CloudRedirect'
export type CloseBehavior = 'exit' | 'minimize'
export type MotionMode = 'full' | 'system' | 'reduced'
export type ClockFormat = 'system' | '12h' | '24h'
export type NotificationCategory =
  | 'launcher'
  | 'installs'
  | 'downloads'
  | 'cloudSaves'
  | 'storage'
  | 'achievements'
  | 'errors'

export type NotificationCategoryPreferences = Record<NotificationCategory, boolean>

export type LauncherPreferences = {
  startupPage: StartupPage
  closeBehavior: CloseBehavior
  autoCheckLauncherUpdates: boolean
  confirmBeforeUninstall: boolean
  confirmBeforeCancelCleanup: boolean
  confirmBeforeClearCache: boolean
  confirmBeforeCloudRestore: boolean
  motionMode: MotionMode
  glassEffects: boolean
  scrollEffects: boolean
  hoverHints: boolean
  showContinuePlaying: boolean
  showRecentGames: boolean
  showActiveTasks: boolean
  showDiscordCard: boolean
  showDonateCard: boolean
  carouselAutoplay: boolean
  showClock: boolean
  showDate: boolean
  showNetworkStatus: boolean
  showDownloadIndicator: boolean
  showNotificationBell: boolean
  clockFormat: ClockFormat
  inAppNotifications: boolean
  windowsNotifications: boolean
  notificationSound: boolean
  doNotDisturbWhilePlaying: boolean
  notificationCategories: NotificationCategoryPreferences
  onboardingCompleted: boolean
  openDownloadsOnJobStart: boolean
  pauseDownloadsBeforeLaunch: boolean
  playInstallCompleteSound: boolean
  defaultLibraryRoot: string
}

export const DEFAULT_NOTIFICATION_CATEGORIES: NotificationCategoryPreferences = {
  launcher: true,
  installs: true,
  downloads: true,
  cloudSaves: true,
  storage: true,
  achievements: true,
  errors: true,
}

export const DEFAULT_LAUNCHER_PREFERENCES: LauncherPreferences = {
  startupPage: 'Home',
  closeBehavior: 'exit',
  autoCheckLauncherUpdates: true,
  confirmBeforeUninstall: true,
  confirmBeforeCancelCleanup: true,
  confirmBeforeClearCache: true,
  confirmBeforeCloudRestore: true,
  motionMode: 'system',
  glassEffects: true,
  scrollEffects: true,
  hoverHints: true,
  showContinuePlaying: true,
  showRecentGames: true,
  showActiveTasks: true,
  showDiscordCard: true,
  showDonateCard: true,
  carouselAutoplay: true,
  showClock: true,
  showDate: true,
  showNetworkStatus: true,
  showDownloadIndicator: true,
  showNotificationBell: true,
  clockFormat: 'system',
  inAppNotifications: true,
  windowsNotifications: false,
  notificationSound: true,
  doNotDisturbWhilePlaying: true,
  notificationCategories: DEFAULT_NOTIFICATION_CATEGORIES,
  onboardingCompleted: false,
  openDownloadsOnJobStart: true,
  pauseDownloadsBeforeLaunch: false,
  playInstallCompleteSound: true,
  defaultLibraryRoot: DEFAULT_STORE_ROOT,
}

const STORAGE_KEY = '0xo_launcher_preferences_v3'
const PREVIOUS_STORAGE_KEY = '0xo_launcher_preferences_v2'
const LEGACY_STORAGE_KEY = '0xo_launcher_preferences_v1'

function isStartupPage(value: unknown): value is StartupPage {
  return (
    value === 'Home' ||
    value === 'Store' ||
    value === 'Library' ||
    value === 'Updates' ||
    value === 'Downloads' ||
    value === 'Cloud Saves'
  )
}

function isCloseBehavior(value: unknown): value is CloseBehavior {
  return value === 'exit' || value === 'minimize'
}

function isMotionMode(value: unknown): value is MotionMode {
  return value === 'full' || value === 'system' || value === 'reduced'
}

function isClockFormat(value: unknown): value is ClockFormat {
  return value === 'system' || value === '12h' || value === '24h'
}

function normalizeRoot(value: unknown) {
  if (typeof value !== 'string') return DEFAULT_STORE_ROOT
  const trimmed = value.trim().replace(/[\\/]+$/, '')
  return trimmed || DEFAULT_STORE_ROOT
}

function booleanValue(value: unknown, fallback: boolean) {
  return typeof value === 'boolean' ? value : fallback
}

function normalizeCategories(value: unknown): NotificationCategoryPreferences {
  const parsed =
    value && typeof value === 'object'
      ? (value as Partial<NotificationCategoryPreferences>)
      : {}
  return {
    launcher: booleanValue(parsed.launcher, true),
    installs: booleanValue(parsed.installs, true),
    downloads: booleanValue(parsed.downloads, true),
    cloudSaves: booleanValue(parsed.cloudSaves, true),
    storage: booleanValue(parsed.storage, true),
    achievements: booleanValue(parsed.achievements, true),
    errors: booleanValue(parsed.errors, true),
  }
}

export function loadLauncherPreferences(): LauncherPreferences {
  if (typeof window === 'undefined') return DEFAULT_LAUNCHER_PREFERENCES
  try {
    const currentRaw = window.localStorage.getItem(STORAGE_KEY)
    const previousRaw = currentRaw ? null : window.localStorage.getItem(PREVIOUS_STORAGE_KEY)
    const legacyRaw = currentRaw || previousRaw ? null : window.localStorage.getItem(LEGACY_STORAGE_KEY)
    const raw = currentRaw ?? previousRaw ?? legacyRaw
    if (!raw) return DEFAULT_LAUNCHER_PREFERENCES
    const parsed = JSON.parse(raw) as Partial<LauncherPreferences> & { reduceMotion?: boolean }
    const migratedStartupPage =
      legacyRaw && parsed.startupPage === 'Library'
        ? 'Store'
        : isStartupPage(parsed.startupPage)
          ? parsed.startupPage
          : previousRaw || legacyRaw
            ? 'Store'
            : DEFAULT_LAUNCHER_PREFERENCES.startupPage
    const migratedMotionMode = isMotionMode(parsed.motionMode)
      ? parsed.motionMode
      : parsed.reduceMotion
        ? 'reduced'
        : 'system'

    return {
      startupPage: migratedStartupPage,
      closeBehavior: isCloseBehavior(parsed.closeBehavior)
        ? parsed.closeBehavior
        : DEFAULT_LAUNCHER_PREFERENCES.closeBehavior,
      autoCheckLauncherUpdates: booleanValue(parsed.autoCheckLauncherUpdates, true),
      confirmBeforeUninstall: booleanValue(parsed.confirmBeforeUninstall, true),
      confirmBeforeCancelCleanup: booleanValue(parsed.confirmBeforeCancelCleanup, true),
      confirmBeforeClearCache: booleanValue(parsed.confirmBeforeClearCache, true),
      confirmBeforeCloudRestore: booleanValue(parsed.confirmBeforeCloudRestore, true),
      motionMode: migratedMotionMode,
      glassEffects: booleanValue(parsed.glassEffects, true),
      scrollEffects: booleanValue(parsed.scrollEffects, true),
      hoverHints: booleanValue(parsed.hoverHints, true),
      showContinuePlaying: booleanValue(parsed.showContinuePlaying, true),
      showRecentGames: booleanValue(parsed.showRecentGames, true),
      showActiveTasks: booleanValue(parsed.showActiveTasks, true),
      showDiscordCard: booleanValue(parsed.showDiscordCard, true),
      showDonateCard: booleanValue(parsed.showDonateCard, true),
      carouselAutoplay: booleanValue(parsed.carouselAutoplay, true),
      showClock: booleanValue(parsed.showClock, true),
      showDate: booleanValue(parsed.showDate, true),
      showNetworkStatus: booleanValue(parsed.showNetworkStatus, true),
      showDownloadIndicator: booleanValue(parsed.showDownloadIndicator, true),
      showNotificationBell: booleanValue(parsed.showNotificationBell, true),
      clockFormat: isClockFormat(parsed.clockFormat) ? parsed.clockFormat : 'system',
      inAppNotifications: booleanValue(parsed.inAppNotifications, true),
      windowsNotifications: booleanValue(parsed.windowsNotifications, false),
      notificationSound: booleanValue(parsed.notificationSound, true),
      doNotDisturbWhilePlaying: booleanValue(parsed.doNotDisturbWhilePlaying, true),
      notificationCategories: normalizeCategories(parsed.notificationCategories),
      onboardingCompleted: booleanValue(parsed.onboardingCompleted, false),
      openDownloadsOnJobStart: booleanValue(parsed.openDownloadsOnJobStart, true),
      pauseDownloadsBeforeLaunch: booleanValue(parsed.pauseDownloadsBeforeLaunch, false),
      playInstallCompleteSound: booleanValue(parsed.playInstallCompleteSound, true),
      defaultLibraryRoot: normalizeRoot(parsed.defaultLibraryRoot),
    }
  } catch {
    return DEFAULT_LAUNCHER_PREFERENCES
  }
}

export function saveLauncherPreferences(preferences: LauncherPreferences) {
  if (typeof window === 'undefined') return
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences))
}
