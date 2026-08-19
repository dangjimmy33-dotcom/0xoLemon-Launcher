import { useEffect, useMemo, useState } from 'react'
import type { GameCatalog, GameSummary, GameInstallMetadata, CloudSaveMetadata } from '../types'
import { globalAssetsOverride } from './useRealtimeAssets'
import { normalizeGameVersions } from '../lib/catalogVersions'
import { fetchWithRetry } from '../lib/fetchWithRetry'

const DEFAULT_CLOUD_SAVE: CloudSaveMetadata = {
  enabled: false,
  saveRoots: [],
  include: [],
  exclude: [],
}

function stringValue(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value === 'number' && Number.isFinite(value)) return String(value)
  return ''
}

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : []
}

function normalizeCloudSave(value: unknown): CloudSaveMetadata {
  const raw = recordValue(value)
  if (!raw) return DEFAULT_CLOUD_SAVE
  return {
    enabled: raw.enabled === true,
    saveRoots: stringArray(raw.saveRoots),
    include: stringArray(raw.include),
    exclude: stringArray(raw.exclude),
  }
}

function buildInstall(gameId: string, title: string, raw?: Record<string, unknown>): GameInstallMetadata {
  const storeRoot = 'E:\\0xoLemon store'
  const folderName = title.replace(/[<>:"/\\|?*]/g, ' ').replace(/\s+/g, ' ').trim() || gameId
  return {
    defaultStoreRoot: storeRoot,
    defaultInstallFolder: `${storeRoot}\\common\\${folderName}`,
    defaultDownloadingFolder: `${storeRoot}\\downloading\\${folderName}`,
    storageLabel: stringValue(raw?.storageLabel) || 'SSD',
    supportsResume: typeof raw?.supportsResume === 'boolean' ? raw.supportsResume : true,
    launchExecutable: stringValue(raw?.launchExecutable) || `${folderName}.exe`,
  }
}

function normalizeSummary(raw: Record<string, unknown>): GameSummary {
  const gameId = stringValue(raw.id).trim()
  const title = stringValue(raw.title).trim() || gameId
  const assetOverride = globalAssetsOverride[gameId] ?? {}

  const rawLatestVersion = stringValue(raw.latestVersion)
  const cleanedLatestVersion = rawLatestVersion.replace(/\s*-\s*Uploaded\s+\d{4}-\d{2}-\d{2}.*$/, '').trim()
  const allVersionTags = (typeof window !== 'undefined' && window.globalVersionTags) || {}
  const versionTags = allVersionTags[gameId] ?? {}

  return {
    id: gameId,
    title,
    subtitle: stringValue(raw.subtitle),
    developer: stringValue(raw.developer),
    publisher: stringValue(raw.publisher),
    latestVersion: cleanedLatestVersion,
    availableVersions: normalizeGameVersions(raw.availableVersions, versionTags),
    gridAssetId: stringValue(assetOverride.grid) || stringValue(raw.gridAssetId),
    heroAssetId: stringValue(assetOverride.hero) || stringValue(raw.heroAssetId),
    logoAssetId: stringValue(assetOverride.logo) || stringValue(raw.logoAssetId),
    iconAssetId: stringValue(assetOverride.icon) || stringValue(raw.iconAssetId),
    install: buildInstall(gameId, title, recordValue(raw.install)),
    cloudSave: normalizeCloudSave(raw.cloudSave),
    assetPackPath: stringValue(raw.assetPackPath) || `assets/games/${gameId}/core.0xo`,
  }
}

function safeNormalizeSummary(raw: unknown, index: number): GameSummary | null {
  const record = recordValue(raw)
  if (!record) {
    console.error(`[useFirestoreCatalog] Skipping invalid game at index ${index}: expected object`, raw)
    return null
  }
  try {
    const game = normalizeSummary(record)
    if (!game.id) {
      console.error(`[useFirestoreCatalog] Skipping invalid game at index ${index}: missing id`, record)
      return null
    }
    return game
  } catch (error) {
    console.error(`[useFirestoreCatalog] Skipping malformed game at index ${index}:`, error, record)
    return null
  }
}

export function useFirestoreCatalog(assetOverrideVersion?: number): GameCatalog | null {
  const [rawGames, setRawGames] = useState<unknown[]>([])
  const [locale, setLocale] = useState('en-US')
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    let mounted = true
    const fetchCatalog = async () => {
      try {
        const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
        const response = await fetchWithRetry(`${BACKEND_URL}/api/0xolemon/catalog`)
        if (!mounted) return
        if (!response.ok) {
          console.error('[useFirestoreCatalog] Backend error:', response.status)
          setRawGames([])
          setLoaded(true)
          return
        }
        const data: unknown = await response.json()
        const payload = recordValue(data)
        const games = Array.isArray(payload?.games) ? payload.games : []
        if (!Array.isArray(payload?.games)) {
          console.error('[useFirestoreCatalog] Invalid catalog payload: games must be an array', data)
        }
        setLocale(stringValue(payload?.defaultLocale) || 'en-US')
        setRawGames(games)
        setLoaded(true)
      } catch (error) {
        if (!mounted) return
        console.error('[useFirestoreCatalog] Fetch error:', error)
        setLoaded(true)
      }
    }

    fetchCatalog()

    return () => {
      mounted = false
    }
  }, [])

  const catalog = useMemo<GameCatalog | null>(() => {
    if (!loaded) return null
    const games = rawGames
      .map((raw, index) => safeNormalizeSummary(raw, index))
      .filter((game): game is GameSummary => game !== null)
    return {
      defaultLocale: locale,
      games,
    }
    // assetOverrideVersion is intentionally used as cache-busting dep
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawGames, locale, loaded, assetOverrideVersion])

  return catalog
}
