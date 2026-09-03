import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { LauncherCollection, LauncherLibraryLayout, LauncherShelf, XmclInstanceGroup } from '../types'
import { isTauriRuntime } from '../lib/gameMeta'

const LEGACY_LIBRARY_KEY = '0xo_launcher_library_game_ids_v1'
const LEGACY_FAVORITES_KEY = 'libraryLikedGames'
const LAYOUT_CHANGED_EVENT = '0xo-library-layout-changed'

function legacyIds(key: string) {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(key) || '[]')
    return Array.isArray(parsed) ? parsed.filter((value): value is string => typeof value === 'string') : []
  } catch {
    return []
  }
}

const legacyGameIds = () => legacyIds(LEGACY_LIBRARY_KEY)
const legacyFavoriteIds = () => legacyIds(LEGACY_FAVORITES_KEY)

function initialLayout(): LauncherLibraryLayout {
  const now = new Date().toISOString()
  return {
    schemaVersion: 1,
    libraryGameIds: legacyGameIds(),
    favoriteGameIds: legacyFavoriteIds(),
    collections: [],
    shelves: [
      { id: 'recent', title: 'Recent', kind: 'recent', gameIds: [] },
      { id: 'installed', title: 'Installed', kind: 'installed', gameIds: [] },
    ],
    xmclInstanceGroups: [{ id: 'all-instances', name: 'All instances', gameIds: [], collapsed: false }],
    migratedLegacyAt: null,
    updatedAt: now,
  }
}

export function useLauncherLibraryLayout() {
  const [layout, setLayout] = useState<LauncherLibraryLayout>(initialLayout)
  const [persistError, setPersistError] = useState<string | null>(null)
  const latestLayoutRef = useRef(layout)
  const persistQueue = useRef<Promise<unknown>>(Promise.resolve())

  const publishLayout = useCallback((next: LauncherLibraryLayout) => {
    window.localStorage.setItem(LEGACY_LIBRARY_KEY, JSON.stringify(next.libraryGameIds))
    window.localStorage.setItem(LEGACY_FAVORITES_KEY, JSON.stringify(next.favoriteGameIds))
    window.dispatchEvent(new CustomEvent<LauncherLibraryLayout>(LAYOUT_CHANGED_EVENT, { detail: next }))
  }, [])

  const persist = useCallback((previous: LauncherLibraryLayout, next: LauncherLibraryLayout) => {
    setPersistError(null)
    publishLayout(next)
    if (!isTauriRuntime()) return
    persistQueue.current = persistQueue.current
      .catch(() => undefined)
      .then(() => invoke<LauncherLibraryLayout>('save_launcher_library_layout', { layout: next }))
      .catch((error) => {
        if (latestLayoutRef.current === next) {
          latestLayoutRef.current = previous
          setLayout(previous)
          publishLayout(previous)
        }
        setPersistError(error instanceof Error ? error.message : String(error))
        if (import.meta.env.DEV) console.warn('[LibraryLayout] persist failed', error)
      })
  }, [publishLayout])

  useEffect(() => {
    const syncLayout = (event: Event) => {
      const next = (event as CustomEvent<LauncherLibraryLayout>).detail
      if (next?.schemaVersion === 1) {
        latestLayoutRef.current = next
        setLayout(next)
      }
    }
    window.addEventListener(LAYOUT_CHANGED_EVENT, syncLayout)
    return () => window.removeEventListener(LAYOUT_CHANGED_EVENT, syncLayout)
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let canceled = false
    void invoke<LauncherLibraryLayout>('get_launcher_library_layout').then((remote) => {
      if (canceled) return
      const legacy = legacyGameIds()
      const legacyFavorites = legacyFavoriteIds()
      const merged = [...new Set([...remote.libraryGameIds, ...legacy])]
      const mergedFavorites = [...new Set([...remote.favoriteGameIds, ...legacyFavorites])]
      const next = {
        ...remote,
        libraryGameIds: merged,
        favoriteGameIds: mergedFavorites,
        migratedLegacyAt: remote.migratedLegacyAt
          ?? (legacy.length > 0 || legacyFavorites.length > 0 ? new Date().toISOString() : null),
      }
      latestLayoutRef.current = next
      setLayout(next)
      if (
        merged.length !== remote.libraryGameIds.length
        || mergedFavorites.length !== remote.favoriteGameIds.length
        || next.migratedLegacyAt !== remote.migratedLegacyAt
      ) persist(remote, next)
    }).catch((error) => {
      if (import.meta.env.DEV) console.warn('[LibraryLayout] load failed', error)
    })
    return () => { canceled = true }
  }, [persist])

  const addGameIds = useCallback((gameIds: Iterable<string>) => {
    const current = latestLayoutRef.current
    const merged = [...new Set([...current.libraryGameIds, ...gameIds])]
    if (merged.length === current.libraryGameIds.length) return
    const next = { ...current, libraryGameIds: merged, updatedAt: new Date().toISOString() }
    latestLayoutRef.current = next
    setLayout(next)
    persist(current, next)
  }, [persist])

  const removeGameIds = useCallback((gameIds: Iterable<string>) => {
    const toRemove = new Set(gameIds)
    const current = latestLayoutRef.current
    const filtered = current.libraryGameIds.filter((id) => !toRemove.has(id))
    if (filtered.length === current.libraryGameIds.length) return
    const next = { ...current, libraryGameIds: filtered, updatedAt: new Date().toISOString() }
    latestLayoutRef.current = next
    setLayout(next)
    persist(current, next)
  }, [persist])

  const updateLayout = useCallback((update: (current: LauncherLibraryLayout) => LauncherLibraryLayout) => {
    const current = latestLayoutRef.current
    const candidate = update(current)
    if (candidate === current) return
    const next = { ...candidate, schemaVersion: 1, updatedAt: new Date().toISOString() }
    latestLayoutRef.current = next
    setLayout(next)
    persist(current, next)
  }, [persist])

  const setFavorite = useCallback((gameId: string, favorite: boolean) => {
    updateLayout((current) => {
      const ids = new Set(current.favoriteGameIds)
      if (favorite) ids.add(gameId)
      else ids.delete(gameId)
      return { ...current, favoriteGameIds: [...ids] }
    })
  }, [updateLayout])

  const saveCollection = useCallback((collection: LauncherCollection) => {
    updateLayout((current) => ({
      ...current,
      collections: [...current.collections.filter((entry) => entry.id !== collection.id), collection],
    }))
  }, [updateLayout])

  const saveShelf = useCallback((shelf: LauncherShelf) => {
    updateLayout((current) => ({
      ...current,
      shelves: [...current.shelves.filter((entry) => entry.id !== shelf.id), shelf],
    }))
  }, [updateLayout])

  const saveXmclInstanceGroup = useCallback((group: XmclInstanceGroup) => {
    updateLayout((current) => ({
      ...current,
      xmclInstanceGroups: [
        ...current.xmclInstanceGroups.filter((entry) => entry.id !== group.id),
        group,
      ],
    }))
  }, [updateLayout])

  const libraryGameIds = useMemo(() => new Set(layout.libraryGameIds), [layout.libraryGameIds])
  const favoriteGameIds = useMemo(() => new Set(layout.favoriteGameIds), [layout.favoriteGameIds])

  return {
    layout,
    libraryGameIds,
    favoriteGameIds,
    addGameIds,
    removeGameIds,
    setFavorite,
    saveCollection,
    saveShelf,
    saveXmclInstanceGroup,
    updateLayout,
    persistError,
  }
}
