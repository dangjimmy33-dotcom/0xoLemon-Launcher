import { useEffect, useState } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

/**
 * Fetches asset URLs (SteamGridDB fixed links) from backend API.
 * Returns a version number that increments when assets change,
 * triggering catalog re-normalization.
 */
export function useBackendAssets(): number {
  const [version, setVersion] = useState(0)

  useEffect(() => {
    let mounted = true

    async function fetchAssets() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/assets`, {
          headers: { 'Accept': 'application/json' }
        })
        
        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()
        
        if (!mounted) return
        
        // Store globally for catalog normalization (same pattern as useRealtimeAssets)
        ;(window as any).globalAssetsOverride = data
        
        // Bump version to trigger catalog re-render
        setVersion(v => v + 1)
      } catch (error) {
        console.error('[useBackendAssets] Failed to fetch:', error)
      }
    }

    fetchAssets()

    // Poll every 5 minutes
    const interval = setInterval(fetchAssets, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])

  return version
}
