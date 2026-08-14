import { invoke } from '@tauri-apps/api/core'

export type SteamGameInfo = {
  name: string
  header_image: string
}

export type SteamStoreSearchItem = {
  id: number
  name: string
  header_image?: string
}

const MAX_CONCURRENT_REQUESTS = 4
const MAX_CACHE_ENTRIES = 500
const SUCCESS_TTL_MS = 10 * 60_000
const FAILURE_TTL_MS = 2 * 60_000

type CacheEntry = { info: SteamGameInfo; expiresAt: number; verified: boolean }
const cache = new Map<string, CacheEntry>()
const failedUntil = new Map<string, number>()
const pending = new Map<string, Promise<SteamGameInfo | null>>()
const queue: Array<{ appid: string; resolve: (info: SteamGameInfo | null) => void }> = []
let activeRequests = 0

function cacheInfo(appid: string, info: SteamGameInfo, verified = true) {
  cache.delete(appid)
  cache.set(appid, { info, expiresAt: Date.now() + SUCCESS_TTL_MS, verified })
  failedUntil.delete(appid)
  while (cache.size > MAX_CACHE_ENTRIES) {
    const oldest = cache.keys().next().value
    if (oldest === undefined) break
    cache.delete(oldest)
  }
}

export function getCachedSteamGameInfo(appid: string) {
  const entry = cache.get(appid)
  if (!entry) return undefined
  if (entry.expiresAt <= Date.now()) {
    cache.delete(appid)
    return undefined
  }
  cache.delete(appid)
  cache.set(appid, entry)
  return entry.info
}

export function seedSteamGameInfo(appid: string, info: SteamGameInfo) {
  if (!info.name) return
  const current = cache.get(appid)
  if (current?.verified && current.expiresAt > Date.now()) return
  const guessedHeader = steamHeaderImageUrl(appid)
  cacheInfo(appid, {
    ...info,
    header_image: info.header_image === guessedHeader ? '' : info.header_image,
  }, false)
}

export function steamHeaderImageUrl(appid: string) {
  return 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/header.jpg'
}

export function steamCapsuleImageUrl(appid: string) {
  return 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/capsule_231x87.jpg'
}

async function loadSteamGameInfo(appid: string): Promise<SteamGameInfo | null> {
  if ((failedUntil.get(appid) ?? 0) > Date.now()) return null
  try {
    const result = await invoke<SteamGameInfo>('fetch_steam_game_name', { appid: Number(appid) })
    if (result?.name) {
      cacheInfo(appid, result)
      return result
    }
  } catch {
    // The public Steam endpoint is the fallback for preview and network failures.
  }

  try {
    const response = await fetch(
      'https://store.steampowered.com/api/appdetails?appids=' + encodeURIComponent(appid) + '&filters=basic',
      { signal: AbortSignal.timeout(6000) },
    )
    if (!response.ok) {
      failedUntil.set(appid, Date.now() + FAILURE_TTL_MS)
      return null
    }

    const data = await response.json()
    const entry = data?.[appid]
    if (!entry?.success || !entry?.data?.name) {
      failedUntil.set(appid, Date.now() + FAILURE_TTL_MS)
      return null
    }

    const info: SteamGameInfo = {
      name: entry.data.name,
      header_image:
        entry.data.header_image ||
        steamHeaderImageUrl(appid),
    }
    cacheInfo(appid, info)
    return info
  } catch {
    failedUntil.set(appid, Date.now() + FAILURE_TTL_MS)
    return null
  }
}

function drainQueue() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return
  while (activeRequests < MAX_CONCURRENT_REQUESTS && queue.length > 0) {
    const next = queue.shift()
    if (!next) return
    activeRequests += 1

    void loadSteamGameInfo(next.appid)
      .then(next.resolve)
      .finally(() => {
        pending.delete(next.appid)
        activeRequests -= 1
        drainQueue()
      })
  }
}

if (typeof document !== 'undefined') {
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') drainQueue()
  })
}

export function fetchSteamGameInfo(appid: string): Promise<SteamGameInfo | null> {
  const cachedEntry = cache.get(appid)
  const cached = getCachedSteamGameInfo(appid)
  if (cached && cachedEntry?.verified) return Promise.resolve(cached)

  const inFlight = pending.get(appid)
  if (inFlight) return inFlight

  let resolveRequest: (info: SteamGameInfo | null) => void = () => undefined
  const request = new Promise<SteamGameInfo | null>((resolve) => {
    resolveRequest = resolve
  })
  pending.set(appid, request)
  queue.push({ appid, resolve: resolveRequest })
  drainQueue()
  return request
}
