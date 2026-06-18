import { DEFAULT_STORE_ROOT } from './installPaths'

export type StartupPage = 'Library' | 'Updates' | 'Downloads'
export type CloseBehavior = 'exit' | 'minimize'

export type LauncherPreferences = {
  startupPage: StartupPage
  closeBehavior: CloseBehavior
  autoCheckLauncherUpdates: boolean
  confirmBeforeUninstall: boolean
  reduceMotion: boolean
  openDownloadsOnJobStart: boolean
  pauseDownloadsBeforeLaunch: boolean
  defaultLibraryRoot: string
}

export const DEFAULT_LAUNCHER_PREFERENCES: LauncherPreferences = {
  startupPage: 'Library',
  closeBehavior: 'exit',
  autoCheckLauncherUpdates: true,
  confirmBeforeUninstall: true,
  reduceMotion: false,
  openDownloadsOnJobStart: true,
  pauseDownloadsBeforeLaunch: false,
  defaultLibraryRoot: DEFAULT_STORE_ROOT,
}

const STORAGE_KEY = '0xo_launcher_preferences_v1'

function isStartupPage(value: unknown): value is StartupPage {
  return value === 'Library' || value === 'Updates' || value === 'Downloads'
}

function isCloseBehavior(value: unknown): value is CloseBehavior {
  return value === 'exit' || value === 'minimize'
}

function normalizeRoot(value: unknown) {
  if (typeof value !== 'string') return DEFAULT_STORE_ROOT
  const trimmed = value.trim().replace(/[\\/]+$/, '')
  return trimmed || DEFAULT_STORE_ROOT
}

export function loadLauncherPreferences(): LauncherPreferences {
  if (typeof window === 'undefined') return DEFAULT_LAUNCHER_PREFERENCES
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULT_LAUNCHER_PREFERENCES
    const parsed = JSON.parse(raw) as Partial<LauncherPreferences>
    return {
      startupPage: isStartupPage(parsed.startupPage) ? parsed.startupPage : DEFAULT_LAUNCHER_PREFERENCES.startupPage,
      closeBehavior: isCloseBehavior(parsed.closeBehavior) ? parsed.closeBehavior : DEFAULT_LAUNCHER_PREFERENCES.closeBehavior,
      autoCheckLauncherUpdates:
        typeof parsed.autoCheckLauncherUpdates === 'boolean'
          ? parsed.autoCheckLauncherUpdates
          : DEFAULT_LAUNCHER_PREFERENCES.autoCheckLauncherUpdates,
      confirmBeforeUninstall:
        typeof parsed.confirmBeforeUninstall === 'boolean'
          ? parsed.confirmBeforeUninstall
          : DEFAULT_LAUNCHER_PREFERENCES.confirmBeforeUninstall,
      reduceMotion:
        typeof parsed.reduceMotion === 'boolean' ? parsed.reduceMotion : DEFAULT_LAUNCHER_PREFERENCES.reduceMotion,
      openDownloadsOnJobStart:
        typeof parsed.openDownloadsOnJobStart === 'boolean'
          ? parsed.openDownloadsOnJobStart
          : DEFAULT_LAUNCHER_PREFERENCES.openDownloadsOnJobStart,
      pauseDownloadsBeforeLaunch:
        typeof parsed.pauseDownloadsBeforeLaunch === 'boolean'
          ? parsed.pauseDownloadsBeforeLaunch
          : DEFAULT_LAUNCHER_PREFERENCES.pauseDownloadsBeforeLaunch,
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
