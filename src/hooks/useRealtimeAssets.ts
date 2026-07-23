import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'
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
    // Key format: "{gameId}-{versionId}"
    // Assuming versionId doesn't contain '-' or we split by the last dash?
    // Actually, gameId can contain dashes (e.g. 007-first-light).
    // Let's find the known gameIds? Or we can just require the key to be exactly gameId-versionId.
    // To safely parse, if we assume version starts with 'v' or build id:
    // A better approach is to store it as a nested map in Firestore if possible, but Firestore UI doesn't support nested maps easily.
    let gameId = ''
    let versionId = ''
    if (key.includes('::')) {
      const parts = key.split('::')
      gameId = parts[0]
      versionId = parts.slice(1).join('::')
    } else {
      const lastDash = key.lastIndexOf('-')
      if (lastDash > 0) {
        gameId = key.substring(0, lastDash)
        versionId = key.substring(lastDash + 1)
      }
    }
    
    if (gameId && versionId) {
      if (!parsed[gameId]) parsed[gameId] = {}
      parsed[gameId][versionId] = tags
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
          setAssetVersion((v) => v + 1) // Trigger re-render of catalog
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
