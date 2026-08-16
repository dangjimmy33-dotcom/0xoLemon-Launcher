import { useEffect, useState } from 'react'
import { fetchWithRetry } from '../lib/fetchWithRetry'
import { invoke } from '@tauri-apps/api/core'

/**
 * globalAssetsOverride: { [gameId]: { grid: url, hero: url, logo: url, icon: url } }
 * Populated from Firestore doc `config/assets_override` which uses flat keys:
 *   "007-first-light-grid": "https://...",
 *   "007-first-light-hero": "https://...",  etc.
 */
export let globalAssetsOverride: Record<string, Record<string, string>> = {}

/**
 * globalVersionTags: { [gameId]: { [versionId]: string[] } }
 * Populated from Firestore doc `config/version_tags` which uses flat keys:
 *   "007-first-light-v1.0.0": ["clean file game", "việt hóa"]
 */
export let globalVersionTags: Record<string, Record<string, string[]>> = {}

const ROLES = ['grid', 'hero', 'logo', 'icon'] as const

function parseFlatOverride(data: Record<string, string>): Record<string, Record<string, string>> {
  const parsed: Record<string, Record<string, string>> = {}
  for (const [key, url] of Object.entries(data)) {
    if (!url || typeof url !== 'string') continue
    // Key format: "{gameId}-{role}"  e.g. "007-first-light-grid"
    for (const role of ROLES) {
      const suffix = `-${role}`
      if (key.endsWith(suffix)) {
        const gameId = key.slice(0, -suffix.length)
        if (!parsed[gameId]) parsed[gameId] = {}
        parsed[gameId][role] = url
        break
      }
    }
  }
  return parsed
}

function parseFlatVersionTags(data: Record<string, string[]>): Record<string, Record<string, string[]>> {
  const parsed: Record<string, Record<string, string[]>> = {}
  if (!data) return parsed
  for (const [key, tags] of Object.entries(data)) {
    if (!Array.isArray(tags)) continue

    if (key.includes('::')) {
      // Format mới: "007-first-light::1.1.0 (Build 24298527) - Uploaded 2026-07-29"
      const sep = key.indexOf('::')
      const gameId = key.substring(0, sep)
      const versionId = key.substring(sep + 2)
      if (gameId && versionId) {
        if (!parsed[gameId]) parsed[gameId] = {}
        parsed[gameId][versionId] = tags
      }
    } else {
      // Format cũ: "007-first-light-1.1.0"
      // gameId có thể chứa dấu '-' nên thử tất cả vị trí split
      // Lưu tags cho MỌI cách split để lookup không bị miss
      let found = false
      // Ưu tiên: versionId bắt đầu bằng số (semver), hoặc 'v', hoặc 'V'
      // Tìm từ trái sang phải, lấy split đầu tiên mà versionId hợp lệ
      for (let i = 0; i < key.length; i++) {
        if (key[i] === '-' && i > 0 && i < key.length - 1) {
          const possibleGameId = key.substring(0, i)
          const possibleVersionId = key.substring(i + 1)
          // versionId hợp lệ: bắt đầu bằng số hoặc 'v'/'V'
          const firstChar = possibleVersionId[0]
          if (firstChar >= '0' && firstChar <= '9' || firstChar === 'v' || firstChar === 'V') {
            if (!parsed[possibleGameId]) parsed[possibleGameId] = {}
            parsed[possibleGameId][possibleVersionId] = tags
            found = true
          }
        }
      }
      // Fallback: nếu không tìm được split hợp lệ, dùng lastIndexOf('-')
      if (!found) {
        const lastDash = key.lastIndexOf('-')
        if (lastDash > 0) {
          const gameId = key.substring(0, lastDash)
          const versionId = key.substring(lastDash + 1)
          if (gameId && versionId) {
            if (!parsed[gameId]) parsed[gameId] = {}
            parsed[gameId][versionId] = tags
          }
        }
      }
    }
  }
  return parsed
}

export function useRealtimeAssets() {
  const [assetVersion, setAssetVersion] = useState(0)

  useEffect(() => {
    let mounted = true

    let isInitialLoad = true

    const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

    const fetchAssetsAndTags = async () => {
      try {
        const [assetsRes, tagsRes] = await Promise.all([
          fetchWithRetry(`${BACKEND_URL}/api/0xolemon/assets`),
          fetchWithRetry(`${BACKEND_URL}/api/0xolemon/tags`)
        ])
        
        if (!mounted) return

        if (assetsRes.ok) {
          const data = await assetsRes.json()
          const parsed = parseFlatOverride(data)
          const changedGames = Object.keys(parsed).filter(gameId => {
             return JSON.stringify(parsed[gameId]) !== JSON.stringify(globalAssetsOverride[gameId])
          })
          globalAssetsOverride = parsed
          
          if (!isInitialLoad && changedGames.length > 0) {
            changedGames.forEach((gameId) => {
              invoke('clear_game_cache', { gameId }).catch(() => {})
            })
          }
        }

        if (tagsRes.ok) {
          const data = await tagsRes.json()
          globalVersionTags = parseFlatVersionTags(data)
          if (typeof window !== 'undefined') {
            if (!window.globalVersionTags) window.globalVersionTags = {}
            Object.assign(window.globalVersionTags, globalVersionTags)
          }
        }

        if (assetsRes.ok || tagsRes.ok) {
          setAssetVersion((v) => v + 1)
          isInitialLoad = false
        }
      } catch (error) {
        if (!mounted) return
        console.error('[useRealtimeAssets] Fetch error:', error)
      }
    }

    fetchAssetsAndTags()

    return () => {
      mounted = false
    }
  }, [])


  return assetVersion
}
