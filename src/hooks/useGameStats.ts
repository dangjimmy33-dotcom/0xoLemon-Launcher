import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'

export interface GameStats {
  downloads: Record<string, number>
  likes: Record<string, number>
}

export function useGameStats(): GameStats {
  const [stats, setStats] = useState<GameStats>({ downloads: {}, likes: {} })

  useEffect(() => {
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'gameStats'),
      (snap) => {
        if (snap.exists()) {
          const data = snap.data() as Record<string, unknown>
          setStats({
            downloads: (data.downloads as Record<string, number>) || {},
            likes: (data.likes as Record<string, number>) || {},
          })
        }
      },
      (error) => {
        console.warn('[useGameStats] Could not load stats:', error)
      }
    )
    return () => unsubscribe()
  }, [])

  return stats
}
