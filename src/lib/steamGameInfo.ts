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
const MAX_CACHE_ENTRIES = 250

const cache = new Map<string, SteamGameInfo>()
const pending = new Map<string, Promise<SteamGameInfo | null>>()
const queue: Array<{ appid: string; resolve: (info: SteamGameInfo | null) => void }> = []
let activeRequests = 0

function cacheInfo(appid: string, info: SteamGameInfo) {
  cache.delete(appid)
  cache.set(appid, info)
  while (cache.size > MAX_CACHE_ENTRIES) {
    const oldest = cache.keys().next().value
    if (oldest === undefined) break
    cache.delete(oldest)
  }
}

export function getCachedSteamGameInfo(appid: string) {
  const info = cache.get(appid)
  if (!info) return undefined
  cache.delete(appid)
  cache.set(appid, info)
  return info
}

export function seedSteamGameInfo(appid: string, info: SteamGameInfo) {
  if (!info.name) return
  cacheInfo(appid, info)
}

export function steamHeaderImageUrl(appid: string) {
  return 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/header.jpg'
}

export function steamCapsuleImageUrl(appid: string) {
  return 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/capsule_231x87.jpg'
}

async function loadSteamGameInfo(appid: string): Promise<SteamGameInfo | null> {
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
    if (!response.ok) return null

    const data = await response.json()
    const entry = data?.[appid]
    if (!entry?.success || !entry?.data?.name) return null

    const info: SteamGameInfo = {
      name: entry.data.name,
      header_image:
        entry.data.header_image ||
        steamHeaderImageUrl(appid),
    }
    cacheInfo(appid, info)
    return info
  } catch {
    return null
  }
}

function drainQueue() {
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

export function fetchSteamGameInfo(appid: string): Promise<SteamGameInfo | null> {
  const cached = getCachedSteamGameInfo(appid)
  if (cached) return Promise.resolve(cached)

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
