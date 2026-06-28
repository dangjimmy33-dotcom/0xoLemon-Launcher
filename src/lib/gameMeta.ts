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

let githubTreeCache: string[] | null = null
type GithubTreeEntry = { path?: string }

async function resolveGithubAssetUrl(gameId: string, role: string): Promise<string | undefined> {
  if (!githubTreeCache) {
    try {
      const res = await fetch('https://api.github.com/repos/dangjimmy33-dotcom/0xoLemon-Launcher/git/trees/main?recursive=1')
      if (!res.ok) return undefined
      const data = (await res.json()) as { tree?: GithubTreeEntry[] }
      githubTreeCache = (data.tree ?? [])
        .map((entry) => entry.path)
        .filter((path): path is string => Boolean(path?.startsWith('src/assets/')))
    } catch {
      return undefined
    }
  }

  if (!githubTreeCache) return undefined

  const targetSlug = gameId.toLowerCase().replace(/[^a-z0-9]/g, '')
  for (const path of githubTreeCache) {
    const parts = path.split('/')
    const folderName = parts[2]
    if (!folderName) continue
    const folderSlug = folderName.toLowerCase().replace(/[^a-z0-9]/g, '')
    if (folderSlug === targetSlug && path.toLowerCase().includes(role.toLowerCase())) {
      return `https://raw.githubusercontent.com/dangjimmy33-dotcom/0xoLemon-Launcher/main/${encodeURI(path)}`
    }
  }
  return undefined
}

const resolvedAssetUrls: Record<string, string> = {}

export async function fetchWebAssetUrl(assetId: string): Promise<string | undefined> {
  if (resolvedAssetUrls[assetId]) return resolvedAssetUrls[assetId]
  if (!assetId.startsWith('asset:')) return undefined

  const parts = assetId.slice(6).split('/')
  if (parts.length < 2) return undefined
  const gameId = parts[0]
  const role = parts.slice(1).join('/')

  const url = await resolveGithubAssetUrl(gameId, role)
  if (url) {
    resolvedAssetUrls[assetId] = url
    return url
  }
  return undefined
}

export function assetUrlForId(assetId: string | null | undefined, assets: Record<string, string>) {
  if (!assetId) return undefined
  if (assets[assetId]) return assets[assetId]
  if (assetId.startsWith('http://') || assetId.startsWith('https://')) return assetId
  if (isRemoteAssetId(assetId)) return decodeRemoteAssetId(assetId)

  return undefined
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
  const safeMedia = Array.isArray(detail.media) ? detail.media : []
  const first = safeMedia.find((item) => isCarouselMedia(item) && item.mimeType?.startsWith('image/') && assetUrlForId(item.assetId, assets))
  return first ? assetUrlForId(first.assetId, assets) : undefined
}

export function processDescriptionHtml(html: string, assets: Record<string, string>) {
  if (!html) return '<p>No description available.</p>'
  let processed = html

  // Decode HTML entities that may appear double-escaped in source data
  processed = processed
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&apos;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')

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
