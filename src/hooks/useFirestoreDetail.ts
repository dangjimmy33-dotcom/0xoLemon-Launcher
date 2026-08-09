import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { contentDb as db } from '../firebase'
import type { GameDetail } from '../types'

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
    
    // First, subscribe to Firestore (default tenant / 0xolemon)
    const unsub = onSnapshot(
      doc(db, 'gameDetails', gameId),
      async (snap) => {
        if (!mounted) return
        if (snap.exists()) {
          const data = snap.data() as GameDetail
          setDetail(data)
        } else {
          // If not found in default Firestore, try fetching from backend (0xolemon1)
          try {
            const res = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/game-details/${gameId}`)
            if (res.ok) {
              const data = await res.json()
              if (mounted) setDetail(data)
            } else {
              if (mounted) setDetail(null)
            }
          } catch (e) {
            console.error('[useFirestoreDetail] backend fetch failed:', e)
            if (mounted) setDetail(null)
          }
        }
      },
      (error) => {
        if (!mounted) return
        console.error('[useFirestoreDetail] firestore error:', error)
      },
    )
    return () => {
      mounted = false
      unsub()
    }
  }, [gameId])

  return gameId ? detail : null
}
