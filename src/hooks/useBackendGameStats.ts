import { useEffect } from 'react'
import { fetchWithRetry } from '../lib/fetchWithRetry'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

/**
 * Fetches game stats from backend API.
 * Stores globally for use in UI.
 */
export function useBackendGameStats(): void {
  useEffect(() => {
    let mounted = true

    async function fetchStats() {
      try {
        const response = await fetchWithRetry(
          `${BACKEND_URL}/api/${TENANT_ID}/game-stats`,
          { headers: { 'Accept': 'application/json' } },
          { maxRetries: 3, baseDelay: 1500 },
        )
        
        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json() as Record<string, unknown>
        
        if (!mounted) return
        
        // Store globally
        window.globalGameStats = { ...(window.globalGameStats || {}), ...data }
      } catch (error) {
        console.error('[useBackendGameStats] Failed to fetch:', error)
      }
    }

    fetchStats()

    // Poll every 1 hour
    const interval = setInterval(fetchStats, 60 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])
}
