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
  // Merge asset URLs from assets_override (SteamGridDB fixed links) 
  const assetOverride = globalAssetsOverride[gameId] ?? {}

  
  const rawLatestVersion = (raw.latestVersion as string) || ''
  // Clean latestVersion: remove "- Uploaded YYYY-MM-DD" suffix that uploader may have appended
  const cleanedLatestVersion = rawLatestVersion.replace(/\s*-\s*Uploaded\s+\d{4}-\d{2}-\d{2}.*$/, '').trim()
  // Read version tags from window.globalVersionTags (same reliable pattern as useBackendCatalog.ts)
  // This avoids ES module live binding issues in Vite production bundles.
  const allVersionTags = (typeof window !== 'undefined' && window.globalVersionTags) || {}
  const versionTags = allVersionTags[gameId] ?? {}
  
  return {
    id: gameId,
    title,
    subtitle: (raw.subtitle as string) || '',
    developer: (raw.developer as string) || '',
    publisher: (raw.publisher as string) || '',
    latestVersion: cleanedLatestVersion,
    availableVersions: normalizeGameVersions(raw.availableVersions, versionTags),
    // Prefer assets_override CDN links (fixed SteamGridDB URLs) over catalog values
    gridAssetId: assetOverride.grid || (raw.gridAssetId as string) || '',
    heroAssetId: assetOverride.hero || (raw.heroAssetId as string) || '',
    logoAssetId: assetOverride.logo || (raw.logoAssetId as string) || '',
    iconAssetId: assetOverride.icon || (raw.iconAssetId as string) || '',
    install: buildInstall(gameId, title, raw.install as Record<string, unknown>),
    cloudSave: (raw.cloudSave as CloudSaveMetadata) || DEFAULT_CLOUD_SAVE,
    assetPackPath: (raw.assetPackPath as string) || `assets/games/${gameId}/core.0xo`,
  }
}

/**
 * Listens to `config/gameCatalog` in Firestore and returns a normalized
 * GameCatalog merged with SteamGridDB asset URLs from `globalAssetsOverride`.
 *
 * Re-normalizes whenever `assetOverrideVersion` bumps (assets loaded/changed),
 * preventing the race condition where catalog loads before assets override.
 */
export function useFirestoreCatalog(assetOverrideVersion?: number): GameCatalog | null {
  // Store raw Firestore data so we can re-normalize when assets change
  const [rawGames, setRawGames] = useState<Record<string, unknown>[]>([])
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
        const data = await response.json()
        setLocale(data.defaultLocale || 'en-US')
        setRawGames(data.games || [])
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
