import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'
import type { GameDetail } from '../types'

/**
 * Subscribes to Firestore `gameDetails/{gameId}` and returns the live GameDetail.
 * Returns null while loading or if no document exists.
 */
export function useFirestoreDetail(gameId: string | null): GameDetail | null {
  const [detail, setDetail] = useState<GameDetail | null>(null)

  useEffect(() => {
    if (!gameId) {
      setDetail(null)
      return
    }
    console.log('[useFirestoreDetail] subscribing to gameDetails/', gameId)
    let mounted = true
    const unsub = onSnapshot(
      doc(db, 'gameDetails', gameId),
      (snap) => {
        if (!mounted) return
        if (snap.exists()) {
          const data = snap.data() as GameDetail
          console.log('[useFirestoreDetail] found doc for', gameId, {
            achievements: data.achievements?.length ?? 0,
            media: data.media?.length ?? 0,
            metadataSource: data.metadataSource,
          })
          setDetail(data)
        } else {
          console.warn('[useFirestoreDetail] NO document for gameId:', gameId)
          setDetail(null)
        }
      },
      (error) => {
        if (!mounted) return
        console.error('[useFirestoreDetail]', error)
      },
    )
    return () => {
      mounted = false
      unsub()
    }
  }, [gameId])

  return detail
}
