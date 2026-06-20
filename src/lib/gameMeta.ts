import type { GameDetail, GameInstallMetadata, GameMedia, GameSummary, Snapshot } from '../types'
import { CUSTOM_DOWNLOADING_RELATIVE, fallbackInstall } from './installPaths'

export function isRemoteAssetId(assetId: string | null | undefined) {
  return Boolean(assetId?.startsWith('remote64:') || assetId?.startsWith('remote:'))
}

export function decodeRemoteAssetId(assetId: string) {
  if (assetId.startsWith('remote:')) return assetId.slice('remote:'.length)
  if (!assetId.startsWith('remote64:')) return undefined
  const raw = assetId.slice('remote64:'.length)
  const padded = raw.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (raw.length % 4)) % 4)
  try {
    const binary = atob(padded)
    const bytes = Uint8Array.from(binary, (ch) => ch.charCodeAt(0))
    return new TextDecoder().decode(bytes)
  } catch {
    return undefined
  }
}

export function assetUrlForId(assetId: string | null | undefined, assets: Record<string, string>) {
  if (!assetId) return undefined
  if (assetId.startsWith('http://') || assetId.startsWith('https://')) return assetId
  if (isRemoteAssetId(assetId)) return decodeRemoteAssetId(assetId)
  return assets[assetId]
}

export function isCarouselMedia(item: GameMedia) {
  return item.role === 'video' || item.role === 'video-preview' || item.role === 'screenshot' || item.role === 'gif'
}

export function mediaPriority(item: GameMedia) {
  switch (item.role) {
    case 'video':
      return 0
    case 'video-preview':
      return 1
    case 'screenshot':
      return 2
    case 'gif':
      return 3
    default:
      return 99
  }
}

export function versionOptions(snapshot: Snapshot, game: GameSummary, useSnapshot: boolean) {
  if (useSnapshot && snapshot.availableVersions.length > 0) {
    return snapshot.availableVersions
  }
  if (game.availableVersions.length > 0) {
    return game.availableVersions.map((version) => version.version)
  }
  return snapshot.latestVersion === 'unknown' ? [] : [snapshot.latestVersion]
}

export function collectAssetIds(game: GameSummary) {
  return Array.from(
    new Set(
      [game.gridAssetId, game.heroAssetId, game.logoAssetId, game.iconAssetId].filter(
        (id): id is string => Boolean(id) && !isRemoteAssetId(id),
      ),
    ),
  )
}

export function fallbackDetailFromSummary(game: GameSummary): GameDetail {
  return {
    gameId: game.id,
    locale: 'en-US',
    title: game.title,
    shortDescription: game.subtitle || 'Game details are packaged for the desktop launcher.',
    detailedDescription:
      'Open the desktop launcher build to load the cooked .0xo media pack, Steam detail metadata, achievements, version data, and install workflow.',
    developers: [game.developer].filter(Boolean),
    publishers: [game.publisher].filter(Boolean),
    releaseDate: 'Pack metadata required',
    genres: [],
    categories: [],
    ratings: [],
    media: [],
    achievements: [],
    sounds: [],
    cloudSave: game.cloudSave ?? { enabled: false, saveRoots: [], include: [], exclude: [] },
    install: game.install,
    descriptionImages: [],
    versions: game.availableVersions,
    metadataSource: 'preview',
  }
}

export function firstMediaUrl(detail: GameDetail, assets: Record<string, string>) {
  const first = detail.media.find((item) => isCarouselMedia(item) && item.mimeType.startsWith('image/') && assetUrlForId(item.assetId, assets))
  return first ? assetUrlForId(first.assetId, assets) : undefined
}

export function processDescriptionHtml(html: string, assets: Record<string, string>) {
  if (!html) return '<p>No description available.</p>'
  let processed = html

  processed = processed.replace(/asset:([a-zA-Z0-9_:/-]+)/g, (_match, assetId) => {
    return assetUrlForId(assetId, assets) ?? ''
  })

  processed = processed
    .replace(/\[h[123]\](.*?)\[\/h[123]\]/gi, '<h3>$1</h3>')
    .replace(/\[b\](.*?)\[\/b\]/gi, '<strong>$1</strong>')
    .replace(/\[i\](.*?)\[\/i\]/gi, '<em>$1</em>')
    .replace(/\[u\](.*?)\[\/u\]/gi, '<u>$1</u>')
    .replace(/\[url=([^]]+)\](.*?)\[\/url\]/gi, '<a href="$1" target="_blank" rel="noreferrer">$2</a>')
    .replace(/\[list\]([\s\S]*?)\[\/list\]/gi, '<ul>$1</ul>')
    .replace(/\[\*\]/gi, '<li>')
    .replace(/\[img\](https?:.*?)\[\/img\]/gi, '<img src="$1" alt="" loading="lazy" />')
    .replace(/\[img\].*?\[\/img\]/gi, '')
    .replace(/<img\b(?![^>]*\bloading=)/gi, '<img loading="lazy"')
    .replace(/<img[^>]+src="(?!data:|https?:)[^"]*"[^>]*>/gi, '')

  return processed
}

function normalizeWindowsPath(path: string) {
  return path.replace(/\\/g, '\\').replace(/\\+$/g, '')
}

export function downloadPathForInstallRoot(root: string, install: GameInstallMetadata = fallbackInstall) {
  const normalizedRoot = normalizeWindowsPath(root)
  const normalizedDefault = normalizeWindowsPath(install.defaultInstallFolder)

  if (normalizedRoot.toLowerCase() === normalizedDefault.toLowerCase()) {
    return install.defaultDownloadingFolder
  }

  const marker = '\\common\\'
  const markerIndex = normalizedRoot.toLowerCase().lastIndexOf(marker)
  if (markerIndex >= 0) {
    const storeRoot = normalizedRoot.slice(0, markerIndex)
    const gameFolder = normalizedRoot.slice(markerIndex + marker.length)
    return `${storeRoot}\\downloading\\${gameFolder}`
  }

  return `${normalizedRoot}\\${CUSTOM_DOWNLOADING_RELATIVE}`
}

export function isTauriRuntime() {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__)
}

export function contentServiceLabel(value: string) {
  const normalized = value.toLowerCase()
  if (normalized.includes('missing') || normalized.includes('failed') || normalized.includes('error')) {
    return 'Content service unavailable'
  }
  if (normalized.includes('ready') || normalized.includes('local') || normalized.includes('auth')) {
    return 'Content service ready'
  }
  return 'Content service checking'
}

export function catalogStatusLabel(value: string) {
  const normalized = value.toLowerCase()
  if (normalized.includes('loaded')) return 'Asset pack loaded'
  if (normalized.includes('fallback') || normalized.includes('preview')) return 'Preview metadata'
  if (normalized.includes('failed') || normalized.includes('error') || normalized.includes('invalid')) return 'Asset pack unavailable'
  return 'Asset pack checking'
}

export function rollbackVersionFor(detail: GameDetail, selectedVersion: string) {
  const versions = detail.versions.map((item) => item.version)
  const selectedIndex = versions.indexOf(selectedVersion)
  if (selectedIndex > 0) return versions[selectedIndex - 1]
  return detail.versions.find((item) => !item.latest)?.version ?? 'previous version'
}
