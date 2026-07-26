import { useEffect, useMemo, useState } from 'react'
import type { GameCatalog, GameSummary, GameInstallMetadata, CloudSaveMetadata } from '../types'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

// Access global assets override (set by useBackendAssets)
declare global {
  interface Window {
    globalAssetsOverride?: Record<string, { grid?: string; hero?: string; logo?: string; icon?: string }>
    globalVersionTags?: Record<string, Record<string, string[]>>
  }
}

const DEFAULT_CLOUD_SAVE: CloudSaveMetadata = {
  enabled: false,
  saveRoots: [],
  include: [],
  exclude: [],
}

function buildInstall(gameId: string, title: string, raw?: Record<string, unknown>): GameInstallMetadata {
  const storeRoot = 'E:\\0xoLemon store'
  const folderName = title.replace(/[<>:"/\\|?*]/g, ' ').replace(/\s+/g, ' ').trim() || gameId
  return {
    defaultStoreRoot: storeRoot,
    defaultInstallFolder: `${storeRoot}\\common\\${folderName}`,
    defaultDownloadingFolder: `${storeRoot}\\downloading\\${folderName}`,
    storageLabel: (raw?.storageLabel as string) || 'SSD',
    supportsResume: (raw?.supportsResume as boolean) ?? true,
    launchExecutable: (raw?.launchExecutable as string) || `${folderName}.exe`,
  }
}

function normalizeSummary(raw: Record<string, unknown>): GameSummary {
  const gameId = (raw.id as string) || ''
  const title = (raw.title as string) || gameId

  // Access globalAssetsOverride set by useBackendAssets
  const globalAssetsOverride = (typeof window !== 'undefined' && window.globalAssetsOverride) || {}
  const globalVersionTags = (typeof window !== 'undefined' && window.globalVersionTags) || {}

  // Build asset keys
  const assetOverride = {
    grid: globalAssetsOverride[`${gameId}-grid`],
    hero: globalAssetsOverride[`${gameId}-hero`],
    logo: globalAssetsOverride[`${gameId}-logo`],
    icon: globalAssetsOverride[`${gameId}-icon`]
  }

  const versionTags = globalVersionTags[gameId] ?? {}

  const rawAvailableVersions = (raw.availableVersions as GameSummary['availableVersions']) || []

  return {
    id: gameId,
    title,
    subtitle: (raw.subtitle as string) || '',
    developer: (raw.developer as string) || '',
    publisher: (raw.publisher as string) || '',
    latestVersion: (raw.latestVersion as string) || '',
    availableVersions: Array.isArray(rawAvailableVersions) ? rawAvailableVersions.map(v => {
      if (!v) return v as any
      let normalized = v
      if (typeof v === 'string') {
        normalized = { version: v, label: v, buildId: v, sizeBytes: 0, latest: false }
      }
      return {
        ...normalized,
        tags: versionTags[normalized.version] || versionTags[normalized.label] || versionTags[normalized.buildId] || normalized.tags
      } as any
    }).filter(Boolean) : [],
    // Prefer assets_override CDN links over catalog values
    gridAssetId: (assetOverride.grid as string) || (raw.gridAssetId as string) || '',
    heroAssetId: (assetOverride.hero as string) || (raw.heroAssetId as string) || '',
    logoAssetId: (assetOverride.logo as string) || (raw.logoAssetId as string) || '',
    iconAssetId: (assetOverride.icon as string) || (raw.iconAssetId as string) || '',
    install: buildInstall(gameId, title, raw.install as Record<string, unknown>),
    cloudSave: (raw.cloudSave as CloudSaveMetadata) || DEFAULT_CLOUD_SAVE,
    assetPackPath: (raw.assetPackPath as string) || `assets/games/${gameId}/core.0xo`,
  }
}

/**
 * Fetches game catalog from backend API and normalizes with assets_override.
 * Re-normalizes when assetOverrideVersion changes (assets loaded/updated).
 */
export function useBackendCatalog(assetOverrideVersion?: number): GameCatalog | null {
  const [rawGames, setRawGames] = useState<Record<string, unknown>[]>([])
  const [locale, setLocale] = useState('en-US')
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    let mounted = true

    async function fetchCatalog() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/catalog`, {
          headers: { 'Accept': 'application/json' }
        })

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()

        if (!mounted) return

        setLocale(data.defaultLocale || 'en-US')
        setRawGames(data.games || [])
        setLoaded(true)
      } catch (error) {
        console.error('[useBackendCatalog] Failed to fetch:', error)
        if (!mounted) return
        setLoaded(true)
      }
    }

    fetchCatalog()

    // Poll every 5 minutes to get updates
    const interval = setInterval(fetchCatalog, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])

  // Re-compute when raw games OR asset override changes (assetOverrideVersion dependency)
  const catalog = useMemo<GameCatalog | null>(() => {
    if (!loaded) return null
    return {
      defaultLocale: locale,
      games: rawGames.map(normalizeSummary),
    }
    // assetOverrideVersion is intentionally used as cache-busting dep
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawGames, locale, loaded, assetOverrideVersion])

  return catalog
}
