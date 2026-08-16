import { useEffect, useState } from 'react'
import type { GameDetail } from '../types'
import { fetchWithRetry } from '../lib/fetchWithRetry'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

/**
 * Subscribes to Firestore `gameDetails/{gameId}` and returns the live GameDetail.
 * Also fetches from backend API for multi-tenant support.
 * Returns null while loading or if no document exists.
 */
export function useFirestoreDetail(gameId: string | null): GameDetail | null {
  const [detail, setDetail] = useState<GameDetail | null>(null)

  useEffect(() => {
    if (!gameId) return
    console.log('[useFirestoreDetail] fetching details for', gameId)
    let mounted = true
    
    const fetchDetails = async () => {
      try {
        // Try default tenant (0xolemon) first
        const res = await fetchWithRetry(`${BACKEND_URL}/api/0xolemon/game-details/${gameId}`)
        if (res.ok) {
          const data = await res.json()
          if (mounted) setDetail(data)
          return
        }
        
        // If not found, try fallback tenant (0xolemon1)
        const fallbackRes = await fetchWithRetry(`${BACKEND_URL}/api/${TENANT_ID}/game-details/${gameId}`)
        if (fallbackRes.ok) {
          const data = await fallbackRes.json()
          if (mounted) setDetail(data)
          return
        }
        
        if (mounted) setDetail(null)
      } catch (e) {
        console.error('[useFirestoreDetail] fetch failed:', e)
        if (mounted) setDetail(null)
      }
    }
    
    fetchDetails()

    return () => {
      mounted = false
    }
  }, [gameId])

  return gameId ? detail : null
}
