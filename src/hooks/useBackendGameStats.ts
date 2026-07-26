import { useEffect } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

/**
 * Fetches game stats from backend API.
 * Stores globally for use in UI.
 */
export function useBackendGameStats(): void {
  useEffect(() => {
    let mounted = true

    async function fetchStats() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/game-stats`, {
          headers: { 'Accept': 'application/json' }
        })
        
        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()
        
        if (!mounted) return
        
        // Store globally
        ;(window as any).globalGameStats = data
      } catch (error) {
        console.error('[useBackendGameStats] Failed to fetch:', error)
      }
    }

    fetchStats()

    // Poll every 5 minutes
    const interval = setInterval(fetchStats, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])
}
