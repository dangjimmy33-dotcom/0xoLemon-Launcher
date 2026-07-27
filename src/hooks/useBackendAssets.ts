import { useEffect, useState } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
// Primary tenant (0xoLemon) has all game assets.
// Secondary tenant (0xoLemon-1) may have additional/newer assets that override primary.
const TENANT_PRIMARY = import.meta.env.VITE_TENANT_ID || '0xolemon'
const TENANT_SECONDARY = '0xolemon1'

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
        // Fetch primary (0xolemon) — has all games' assets
        const primaryRes = await fetch(`${BACKEND_URL}/api/${TENANT_PRIMARY}/assets`, {
          headers: { 'Accept': 'application/json' }
        })

        let merged: Record<string, string> = {}

        if (primaryRes.ok) {
          const primaryData = await primaryRes.json()
          merged = { ...primaryData }
        }

        // Fetch secondary (0xolemon1) — may override with newer data
        try {
          const secondaryRes = await fetch(`${BACKEND_URL}/api/${TENANT_SECONDARY}/assets`, {
            headers: { 'Accept': 'application/json' }
          })
          if (secondaryRes.ok) {
            const secondaryData = await secondaryRes.json()
            merged = { ...merged, ...secondaryData }  // secondary overrides primary
          }
        } catch {
          // secondary failure is non-fatal
        }

        if (!mounted) return

        if (Object.keys(merged).length > 0) {
          // Store globally for catalog normalization
          ;(window as any).globalAssetsOverride = merged
          setVersion(v => v + 1)
        }
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
