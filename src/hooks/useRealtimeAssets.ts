import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { contentDb as db } from '../firebase'
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

    // Data lives in Firestore at: config/assets_override (document, not collection)
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'assets_override'),
      (snap) => {
        if (!mounted) return
        if (!snap.exists()) return

        const parsed = parseFlatOverride(snap.data() as Record<string, string>)
        
        const changedGames = Object.keys(parsed).filter(gameId => {
           return JSON.stringify(parsed[gameId]) !== JSON.stringify(globalAssetsOverride[gameId])
        })

        globalAssetsOverride = parsed
        setAssetVersion((v) => v + 1)

        // Clear per-game local cache so new URLs are fetched ONLY for changed games
        if (!isInitialLoad) {
          changedGames.forEach((gameId) => {
            invoke('clear_game_cache', { gameId }).catch(() => {})
          })
        }
        isInitialLoad = false
      },
      (error) => {
        console.error('[useRealtimeAssets] Firestore error:', error)
      },
    )

    const unsubscribeTags = onSnapshot(
      doc(db, 'config', 'version_tags'),
      (snap) => {
        if (!mounted) return
        if (snap.exists()) {
          globalVersionTags = parseFlatVersionTags(snap.data() as Record<string, string[]>)
          if (typeof window !== 'undefined') {
            if (!window.globalVersionTags) window.globalVersionTags = {}
            Object.assign(window.globalVersionTags, globalVersionTags)
          }
          setAssetVersion((v) => v + 1)
        }
      },
      (error) => {
        console.error('[useRealtimeAssets] version_tags error:', error)
      }
    )

    return () => {
      mounted = false
      unsubscribe()
      unsubscribeTags()
    }
  }, [])


  return assetVersion
}
