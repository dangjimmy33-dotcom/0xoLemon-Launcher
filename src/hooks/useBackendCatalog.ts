import { useEffect, useMemo, useState } from 'react'
import type { GameCatalog, GameSummary, GameInstallMetadata, CloudSaveMetadata } from '../types'
import { fetchWithRetry } from '../lib/fetchWithRetry'
import { normalizeGameVersions } from '../lib/catalogVersions'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

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

  const globalAssetsOverride = (typeof window !== 'undefined' && window.globalAssetsOverride) || {}
  const globalVersionTags = (typeof window !== 'undefined' && window.globalVersionTags) || {}
  const assetOverride = {
    grid: globalAssetsOverride[`${gameId}-grid`],
    hero: globalAssetsOverride[`${gameId}-hero`],
    logo: globalAssetsOverride[`${gameId}-logo`],
    icon: globalAssetsOverride[`${gameId}-icon`],
  }
  const versionTags = globalVersionTags[gameId] ?? {}

  return {
    id: gameId,
    title,
    subtitle: stringValue(raw.subtitle),
    developer: stringValue(raw.developer),
    publisher: stringValue(raw.publisher),
    latestVersion: stringValue(raw.latestVersion),
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
    console.error(`[useBackendCatalog] Skipping invalid game at index ${index}: expected object`, raw)
    return null
  }
  try {
    const game = normalizeSummary(record)
    if (!game.id) {
      console.error(`[useBackendCatalog] Skipping invalid game at index ${index}: missing id`, record)
      return null
    }
    return game
  } catch (error) {
    console.error(`[useBackendCatalog] Skipping malformed game at index ${index}:`, error, record)
    return null
  }
}

export function useBackendCatalog(assetOverrideVersion?: number): GameCatalog | null {
  const [rawGames, setRawGames] = useState<unknown[]>([])
  const [locale, setLocale] = useState('en-US')
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    let mounted = true

    async function fetchCatalog() {
      try {
        const response = await fetchWithRetry(
          `${BACKEND_URL}/api/${TENANT_ID}/catalog`,
          { headers: { 'Accept': 'application/json' } },
          {
            maxRetries: 4,
            baseDelay: 2000,
            onRetry: (attempt, err) =>
              console.warn(`[useBackendCatalog] Retry ${attempt}: ${err.message}`),
          },
        )

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data: unknown = await response.json()
        if (!mounted) return
        const payload = recordValue(data)
        const games = Array.isArray(payload?.games) ? payload.games : []
        if (!Array.isArray(payload?.games)) {
          console.error('[useBackendCatalog] Invalid catalog payload: games must be an array', data)
        }
        setLocale(stringValue(payload?.defaultLocale) || 'en-US')
        setRawGames(games)
        setLoaded(true)
      } catch (error) {
        console.error('[useBackendCatalog] Failed to fetch:', error)
        if (!mounted) return
        setLoaded(true)
      }
    }

    fetchCatalog()
    const interval = setInterval(fetchCatalog, 60 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
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
