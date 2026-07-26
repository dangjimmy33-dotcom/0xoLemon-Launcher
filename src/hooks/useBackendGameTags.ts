import { useEffect } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

/**
 * Fetches game filter tags from backend API.
 * Stores globally for use in catalog/filter logic.
 */
export function useBackendGameTags(): void {
  useEffect(() => {
    let mounted = true

    async function fetchTags() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/game-tags`, {
          headers: { 'Accept': 'application/json' }
        })

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()

        if (!mounted) return

          // Store globally for catalog usage
          ; (window as any).globalVersionTags = data
      } catch (error) {
        console.error('[useBackendGameTags] Failed to fetch:', error)
      }
    }

    fetchTags()

    // Poll every 5 minutes
    const interval = setInterval(fetchTags, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])
}
