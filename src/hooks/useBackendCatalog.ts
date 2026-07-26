import { useEffect, useState } from 'react'
import type { GameCatalog } from '../types'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

/**
 * Fetches game catalog from backend API instead of Firestore directly.
 * Backend caches for 1 hour, reducing Firestore reads from 49k/day to ~100/day.
 */
export function useBackendCatalog(assetOverrideVersion?: number): GameCatalog | null {
  const [catalog, setCatalog] = useState<GameCatalog | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let mounted = true

    async function fetchCatalog() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/catalog`, {
          headers: { 'Accept': 'application/json' }
        })
        
        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()
        
        if (!mounted) return
        
        setCatalog(data)
        setLoading(false)
      } catch (error) {
        console.error('[useBackendCatalog] Failed to fetch:', error)
        if (!mounted) return
        setLoading(false)
      }
    }

    fetchCatalog()

    // Poll every 5 minutes to get updates (much less than Firestore listeners)
    const interval = setInterval(fetchCatalog, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [assetOverrideVersion])

  return loading ? null : catalog
}
