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

export function useRealtimeAssets() {
  const [assetVersion, setAssetVersion] = useState(0)

  useEffect(() => {
    let mounted = true

    // Data lives in Firestore at: config/assets_override (document, not collection)
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'assets_override'),
      (snap) => {
        if (!mounted) return
        if (!snap.exists()) return

        const parsed = parseFlatOverride(snap.data() as Record<string, string>)
        globalAssetsOverride = parsed

        setAssetVersion((v) => v + 1)

        // Clear per-game local cache so new URLs are fetched
        Object.keys(parsed).forEach((gameId) => {
          invoke('clear_game_cache', { gameId }).catch(() => {})
        })
      },
      (error) => {
        console.error('[useRealtimeAssets] Firestore error:', error)
      },
    )

    return () => {
      mounted = false
      unsubscribe()
    }
  }, [])

  return assetVersion
}
