import { useEffect, useState } from 'react'
import { fetchWithRetry } from '../lib/fetchWithRetry'
import { updateGameTagTable, type GameTagTable } from '../lib/gameTags'

export function useRealtimeGameTags() {
  const [tagVersion, setTagVersion] = useState(0)

  useEffect(() => {
    let mounted = true
    const fetchGameTags = async () => {
      try {
        const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
        const response = await fetchWithRetry(`${BACKEND_URL}/api/0xolemon/game-tags`)
        if (!mounted) return
        if (response.ok) {
          const data = await response.json()
          updateGameTagTable(data as Partial<GameTagTable>)
        }
        setTagVersion(v => v + 1)
      } catch (error) {
        if (!mounted) return
        console.error("Lỗi đồng bộ Game Tags:", error)
        setTagVersion(v => v + 1)
      }
    }

    fetchGameTags()

    return () => {
      mounted = false
    }
  }, [])

  return tagVersion
}
