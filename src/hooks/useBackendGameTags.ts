import { useEffect } from 'react'
import { updateGameTagTable, type GameTagTable } from '../lib/gameTags'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon'

/**
 * Fetches game filter tags from backend API.
 * Updates game tag table for use in catalog/filter logic.
 */
export function useBackendGameTags(): void {
  useEffect(() => {
    let mounted = true

    async function fetchTags() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/game-tags`, {
          headers: { 'Accept': 'application/json' }
        })

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()

        if (!mounted) return

        // Update game tag table (same as useRealtimeGameTags)
        updateGameTagTable(data as Partial<GameTagTable>)
      } catch (error) {
        console.error('[useBackendGameTags] Failed to fetch:', error)
      }
    }

    fetchTags()

    // Poll every 1 hour to get updates, avoiding Render free tier rate limits
    const interval = setInterval(fetchTags, 60 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])
}
