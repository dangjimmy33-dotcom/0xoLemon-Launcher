import { useCallback, useEffect, useState } from 'react'
import type { LauncherNavigationSnapshot, LauncherRoute, TabId } from '../types'

const NAVIGATION_STORAGE_KEY = '0xo_launcher_navigation_v2'
const LEGACY_TAB_KEY = '0xo_activeTab'
const LEGACY_GAME_KEY = '0xo_selectedGameId'
const MAX_HISTORY = 40

type NavigationState = {
  currentRoute: LauncherRoute
  backStack: LauncherRoute[]
  forwardStack: LauncherRoute[]
}

function sameRoute(left: LauncherRoute, right: LauncherRoute) {
  return left.tab === right.tab
    && left.selectedGameId === right.selectedGameId
    && JSON.stringify(left.query ?? {}) === JSON.stringify(right.query ?? {})
}

function migrateRoute(value: unknown, isTabId: (value: string) => value is TabId): LauncherRoute | null {
  if (!value || typeof value !== 'object') return null
  const candidate = value as {
    tab?: unknown
    selectedGameId?: unknown
    query?: Record<string, string>
  }
  if (typeof candidate.tab !== 'string') return null
  const migratedTab = candidate.tab === 'Lightning Hub' ? 'Tools' : candidate.tab
  if (!isTabId(migratedTab) || (candidate.selectedGameId != null && typeof candidate.selectedGameId !== 'string')) return null
  return {
    tab: migratedTab,
    selectedGameId: candidate.selectedGameId ?? null,
    query: candidate.query,
  }
}

export function loadLauncherNavigation(
  fallbackTab: TabId,
  isTabId: (value: string) => value is TabId,
): NavigationState {
  if (typeof window === 'undefined') {
    return { currentRoute: { tab: fallbackTab, selectedGameId: null }, backStack: [], forwardStack: [] }
  }
  try {
    const raw = window.localStorage.getItem(NAVIGATION_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<LauncherNavigationSnapshot>
      const currentRoute = migrateRoute(parsed.currentRoute, isTabId)
      if (parsed.schemaVersion === 2 && currentRoute) {
        return {
          currentRoute,
          backStack: Array.isArray(parsed.backStack)
            ? parsed.backStack.map((route) => migrateRoute(route, isTabId)).filter((route): route is LauncherRoute => route !== null).slice(-MAX_HISTORY)
            : [],
          forwardStack: Array.isArray(parsed.forwardStack)
            ? parsed.forwardStack.map((route) => migrateRoute(route, isTabId)).filter((route): route is LauncherRoute => route !== null).slice(-MAX_HISTORY)
            : [],
        }
      }
    }
  } catch {
    // A corrupt navigation snapshot must never prevent the launcher from opening.
  }

  const legacyTab = window.localStorage.getItem(LEGACY_TAB_KEY)
  const migratedLegacyTab = legacyTab === 'Lightning Hub' ? 'Tools' : legacyTab
  const tab = migratedLegacyTab && isTabId(migratedLegacyTab) ? migratedLegacyTab : fallbackTab
  return {
    currentRoute: { tab, selectedGameId: window.localStorage.getItem(LEGACY_GAME_KEY) || null },
    backStack: [],
    forwardStack: [],
  }
}

export function useLauncherNavigation(initialState: NavigationState) {
  const [state, setState] = useState(initialState)

  useEffect(() => {
    const snapshot: LauncherNavigationSnapshot = {
      schemaVersion: 2,
      currentRoute: state.currentRoute,
      backStack: state.backStack,
      forwardStack: state.forwardStack,
      updatedAt: new Date().toISOString(),
    }
    window.localStorage.setItem(NAVIGATION_STORAGE_KEY, JSON.stringify(snapshot))
    window.localStorage.setItem(LEGACY_TAB_KEY, state.currentRoute.tab)
    if (state.currentRoute.selectedGameId) {
      window.localStorage.setItem(LEGACY_GAME_KEY, state.currentRoute.selectedGameId)
    } else {
      window.localStorage.removeItem(LEGACY_GAME_KEY)
    }
  }, [state])

  const navigate = useCallback((tab: TabId, query?: Record<string, string>) => {
    setState((current) => {
      const nextRoute: LauncherRoute = {
        tab,
        selectedGameId: current.currentRoute.selectedGameId,
        query,
      }
      if (sameRoute(current.currentRoute, nextRoute)) return current
      return {
        currentRoute: nextRoute,
        backStack: [...current.backStack, current.currentRoute].slice(-MAX_HISTORY),
        forwardStack: [],
      }
    })
  }, [])

  const setSelectedGameId = useCallback((selectedGameId: string | null) => {
    setState((current) => {
      if (current.currentRoute.selectedGameId === selectedGameId) return current
      return {
        ...current,
        currentRoute: { ...current.currentRoute, selectedGameId },
      }
    })
  }, [])

  const goBack = useCallback(() => {
    setState((current) => {
      const previous = current.backStack.at(-1)
      if (!previous) return current
      return {
        currentRoute: previous,
        backStack: current.backStack.slice(0, -1),
        forwardStack: [current.currentRoute, ...current.forwardStack].slice(0, MAX_HISTORY),
      }
    })
  }, [])

  const goForward = useCallback(() => {
    setState((current) => {
      const next = current.forwardStack[0]
      if (!next) return current
      return {
        currentRoute: next,
        backStack: [...current.backStack, current.currentRoute].slice(-MAX_HISTORY),
        forwardStack: current.forwardStack.slice(1),
      }
    })
  }, [])

  return {
    activeTab: state.currentRoute.tab,
    selectedGameId: state.currentRoute.selectedGameId ?? null,
    navigate,
    setSelectedGameId,
    goBack,
    goForward,
    canGoBack: state.backStack.length > 0,
    canGoForward: state.forwardStack.length > 0,
  }
}
