import type { GameSummary } from '../types'
import { globalAssetsOverride } from '../hooks/useRealtimeAssets'

// const STEAM_API_KEY = 'C8389A6AE249466D0A5234DC9D2D23C6'
const STEAMGRIDDB_API_KEY = '6949533daea9444b0e8f2dfe121a0c30'

const CACHE_PREFIX = 'oxo_asset_cache_'

async function getSteamGridDbGameId(title: string): Promise<number | null> {
  const cacheKey = `${CACHE_PREFIX}sgdb_id_${title}`
  const cached = localStorage.getItem(cacheKey)
  if (cached) return parseInt(cached, 10)

  try {
    const res = await fetch(`https://www.steamgriddb.com/api/v2/search/autocomplete/${encodeURIComponent(title)}`, {
      headers: {
        Authorization: `Bearer ${STEAMGRIDDB_API_KEY}`
      }
    })
    const data = await res.json()
    if (data.success && data.data && data.data.length > 0) {
      const id = data.data[0].id
      localStorage.setItem(cacheKey, id.toString())
      return id
    }
  } catch (e) {
    console.error('Failed to get SteamGridDB Game ID', e)
  }
  return null
}

export async function fetchRemoteAssetUrl(assetId: string, game: GameSummary): Promise<string | undefined> {
  let type: 'grid' | 'hero' | 'logo' | 'icon' | null = null
  
  if (assetId === game.gridAssetId) type = 'grid'
  else if (assetId === game.heroAssetId) type = 'hero'
  else if (assetId === game.logoAssetId) type = 'logo'
  else if (assetId === game.iconAssetId) type = 'icon'

  if (!type) {
    return undefined
  }

  // Check Firestore override FIRST
  if (globalAssetsOverride[game.id]) {
    const overrideUrl = globalAssetsOverride[game.id][type]
    if (overrideUrl) return overrideUrl
  }

  const sgdbId = await getSteamGridDbGameId(game.title)
  if (!sgdbId) return undefined

  const cacheKey = `${CACHE_PREFIX}sgdb_url_${sgdbId}_${type}`
  const cachedUrl = localStorage.getItem(cacheKey)
  if (cachedUrl) return cachedUrl

  let endpoint = ''
  if (type === 'grid') endpoint = `https://www.steamgriddb.com/api/v2/grids/game/${sgdbId}?dimensions=600x900,460x215`
  else if (type === 'hero') endpoint = `https://www.steamgriddb.com/api/v2/heroes/game/${sgdbId}?dimensions=1920x620,3840x1240`
  else if (type === 'logo') endpoint = `https://www.steamgriddb.com/api/v2/logos/game/${sgdbId}`
  else if (type === 'icon') endpoint = `https://www.steamgriddb.com/api/v2/icons/game/${sgdbId}`

  if (!endpoint) return undefined

  try {
    const res = await fetch(endpoint, {
      headers: {
        Authorization: `Bearer ${STEAMGRIDDB_API_KEY}`
      }
    })
    const data = await res.json()
    if (data.success && data.data && data.data.length > 0) {
      const url = data.data[0].url
      localStorage.setItem(cacheKey, url)
      return url
    }
  } catch (e) {
    console.error(`Failed to fetch ${type} from SteamGridDB`, e)
  }

  return undefined
}
