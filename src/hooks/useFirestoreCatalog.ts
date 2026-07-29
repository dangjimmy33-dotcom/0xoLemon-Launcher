import { useEffect, useMemo, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { contentDb as db } from '../firebase'
import type { GameCatalog, GameSummary, GameInstallMetadata, CloudSaveMetadata } from '../types'
import { globalAssetsOverride, globalVersionTags } from './useRealtimeAssets'

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
      const entry = v as any
      let normalized: any
      if (typeof entry === 'string') {
        // Trích buildId từ pattern "X.X.X (Build 24298527)" nếu có
        const buildMatch = (entry as string).match(/\(Build ([^)]+)\)/)
        const extractedBuildId = buildMatch ? buildMatch[1].trim() : entry
        // Label sạch: bỏ phần "- Uploaded ..." nếu có
        const cleanLabel = (entry as string).replace(/\s*-\s*Uploaded\s+\d{4}-\d{2}-\d{2}.*$/, '').trim()
        normalized = { version: entry, label: cleanLabel, buildId: extractedBuildId, sizeBytes: 0, latest: false }
      } else {
        // Object từ catalog: extract buildId nếu chưa có hoặc giống version string
        if (!entry.buildId || entry.buildId === entry.version) {
          const buildMatch = (entry.version || '').match(/\(Build ([^)]+)\)/)
          if (buildMatch) entry.buildId = buildMatch[1].trim()
        }
        // Clean label: bỏ "- Uploaded ..." suffix
        if (entry.label && entry.label.includes('- Uploaded')) {
          entry.label = entry.label.replace(/\s*-\s*Uploaded\s+\d{4}-\d{2}-\d{2}.*$/, '').trim()
        }
        normalized = entry
      }
      // Lookup tags: thử full version string trước, sau đó thử semver prefix (phần trước space/ngoặc)
      // Vd: "1.1.0 (Build 24298527) - Uploaded 2026-07-29" → thử "1.1.0" nếu full không match
      function semverPrefix(s: string): string { return s ? s.split(/[ (]/)[0].trim() : s }
      const ver = normalized.version || ''
      const lbl = normalized.label || ''
      const bid = normalized.buildId || ''
      const foundTags =
        versionTags[ver] ||
        versionTags[lbl] ||
        versionTags[bid] ||
        versionTags[semverPrefix(ver)] ||
        versionTags[semverPrefix(lbl)] ||
        versionTags[semverPrefix(bid)] ||
        normalized.tags
      return {
        ...normalized,
        tags: foundTags
      } as any
    }).filter(Boolean) : [],
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
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'gameCatalog'),
      (snap) => {
        if (!mounted) return
        if (!snap.exists()) {
          setRawGames([])
          setLoaded(true)
          return
        }
        const data = snap.data() as Record<string, unknown>
        setLocale((data.defaultLocale as string) || 'en-US')
        setRawGames((data.games as Record<string, unknown>[]) || [])
        setLoaded(true)
      },
      (error) => {
        if (!mounted) return
        console.error('[useFirestoreCatalog] Firestore error:', error)
        setLoaded(true)
      },
    )

    return () => {
      mounted = false
      unsubscribe()
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
