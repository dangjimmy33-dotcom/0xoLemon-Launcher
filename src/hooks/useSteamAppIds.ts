import { useState, useEffect } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'

// Đọc mapping gameId -> appId từ Firestore (config/steam_appids)
// Admin tự cập nhật bằng upload_steam_appids.js
let cachedMapping: Record<string, number> | null = null
const listeners: Array<(m: Record<string, number>) => void> = []

export function useSteamAppIds() {
  const [mapping, setMapping] = useState<Record<string, number>>(cachedMapping ?? {})

  useEffect(() => {
    if (cachedMapping !== null) {
      setMapping(cachedMapping)
    }

    const unsub = onSnapshot(doc(db, 'config', 'steam_appids'), (snap) => {
      const data = snap.exists() ? (snap.data() as Record<string, number>) : {}
      cachedMapping = data
      setMapping(data)
      listeners.forEach(fn => fn(data))
    })

    return unsub
  }, [])

  return { mapping }
}

// Helper: lấy appId từ gameId (dùng ngoài React component nếu cần)
export function getAppIdForGame(gameId: string): number | undefined {
  return cachedMapping?.[gameId]
}
