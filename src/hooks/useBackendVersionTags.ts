import { useEffect, useState } from 'react'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'

/**
 * Parses flat version tags from backend into nested structure.
 * Backend returns: { "gameId-versionId": ["tag1", "tag2"], ... }
 * Parses to: { gameId: { versionId: ["tag1", "tag2"] } }
 */
function parseFlatVersionTags(data: Record<string, string[]>): Record<string, Record<string, string[]>> {
  const parsed: Record<string, Record<string, string[]>> = {}
  if (!data) return parsed

  for (const [key, tags] of Object.entries(data)) {
    if (!Array.isArray(tags)) continue

    // Key format: "{gameId}-{versionId}"
    // gameId can contain dashes (e.g. 007-first-light, grand-theft-auto-v-legacy)
    // versionId usually starts with 'v' (e.g. v1.0.0) or digit (e.g. 1.0.3788.0)

    let gameId = ''
    let versionId = ''

    // Strategy: Find the dash before a version-like pattern
    // Version patterns: v + digit, or digit + period
    const versionPattern = /-((v\d+|\d+\.\d+)[\w\.\-]*?)$/
    const match = key.match(versionPattern)

    if (match) {
      // Found version pattern
      gameId = key.substring(0, match.index)
      versionId = match[1]
    } else {
      // Fallback: split by last dash
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

declare global {
  interface Window {
    globalVersionTags?: Record<string, Record<string, string[]>>
  }
}

/**
 * Fetches version tags from backend API and stores in window.globalVersionTags.
 * Used by useBackendCatalog to add tags to game versions.
 * Returns version number to trigger catalog re-render.
 */
export function useBackendVersionTags(): number {
  const [version, setVersion] = useState(0)

  useEffect(() => {
    let mounted = true

    async function fetchTags() {
      try {
        const response = await fetch(`${BACKEND_URL}/api/tags`, {
          headers: { 'Accept': 'application/json' }
        })

        if (!response.ok) {
          throw new Error(`Backend error: ${response.status}`)
        }

        const data = await response.json()

        if (!mounted) return

        // Parse flat tags to nested structure
        const parsed = parseFlatVersionTags(data as Record<string, string[]>)

        // Store in global variable (same as useRealtimeAssets)
        if (typeof window !== 'undefined') {
          window.globalVersionTags = parsed
        }

        // Trigger re-render (same as useRealtimeAssets)
        setVersion((v) => v + 1)

        console.log('[useBackendVersionTags] Version tags loaded from backend:', Object.keys(parsed).length, 'games')
      } catch (error) {
        console.error('[useBackendVersionTags] Failed to fetch:', error)
      }
    }

    fetchTags()

    // Poll every 5 minutes (version tags don't change often)
    const interval = setInterval(fetchTags, 5 * 60 * 1000)

    return () => {
      mounted = false
      clearInterval(interval)
    }
  }, [])

  return version
}
