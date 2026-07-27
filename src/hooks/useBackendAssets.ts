import { useEffect, useState } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

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
        const response = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/assets`, {
          headers: { 'Accept': 'application/json' }
        })

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()

        if (!mounted) return

          // Store globally for catalog normalization
          // Backend returns: { "gameId-grid": "url", "gameId-hero": "url", ... }
          ; (window as any).globalAssetsOverride = data

        // Bump version to trigger catalog re-render
        setVersion(v => v + 1)
      } catch (error) {
        console.error('[useBackendAssets] Failed to fetch:', error)
      }
    }

    fetchAssets()

    // Poll every 1 hour to get updates, avoiding Render free tier rate limits
    const interval = setInterval(fetchAssets, 60 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])

  return version
}
