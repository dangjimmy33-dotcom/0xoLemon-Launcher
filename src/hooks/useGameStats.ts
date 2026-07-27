import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { contentDb as db } from '../firebase'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon'

export interface GameStats {
  downloads: Record<string, number>
  likes: Record<string, number>
}

export function useGameStats(): GameStats {
  const [stats, setStats] = useState<GameStats>({ downloads: {}, likes: {} })

  useEffect(() => {
    let mounted = true
    let currentStats: GameStats = { downloads: {}, likes: {} }

    const updateStats = (newStats: Partial<GameStats>) => {
      currentStats = {
        downloads: { ...currentStats.downloads, ...(newStats.downloads || {}) },
        likes: { ...currentStats.likes, ...(newStats.likes || {}) }
      }
      setStats(currentStats)
    }

    const fetchBackendStats = async () => {
      try {
        const res = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/game-stats`)
        if (res.ok && mounted) {
          const data = await res.json()
          updateStats(data)
        }
      } catch (e) {
        console.warn('[useGameStats] Could not load backend stats:', e)
      }
    }

    fetchBackendStats()

    const unsubscribe = onSnapshot(
      doc(db, 'config', 'gameStats'),
      (snap) => {
        if (!mounted) return
        if (snap.exists()) {
          const data = snap.data() as Record<string, unknown>
          updateStats({
            downloads: (data.downloads as Record<string, number>) || {},
            likes: (data.likes as Record<string, number>) || {},
          })
        }
      },
      (error) => {
        if (!mounted) return
        console.warn('[useGameStats] Could not load firestore stats:', error)
      }
    )
    return () => {
      mounted = false
      unsubscribe()
    }
  }, [])

  return stats
}
