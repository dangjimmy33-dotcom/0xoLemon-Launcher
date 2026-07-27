import { useState, useEffect } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { contentDb as db } from '../firebase'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

// Đọc mapping gameId -> appId từ Firestore (config/steam_appids)
// Admin tự cập nhật bằng upload_steam_appids.js
let cachedMapping: Record<string, number> | null = null
const listeners: Array<(m: Record<string, number>) => void> = []

export function useSteamAppIds() {
  const [mapping, setMapping] = useState<Record<string, number>>(cachedMapping ?? {})

  useEffect(() => {
    let mounted = true
    if (cachedMapping !== null) {
      setMapping(cachedMapping)
    }

    // Fetch from backend (tenant specific)
    const fetchBackendMapping = async () => {
      try {
        const res = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/steam-appids`)
        if (res.ok && mounted) {
          const data = await res.json()
          cachedMapping = { ...cachedMapping, ...data }
          setMapping(cachedMapping!)
          listeners.forEach(fn => fn(cachedMapping!))
        }
      } catch (e) {
        console.error('Failed to fetch backend steam-appids', e)
      }
    }

    fetchBackendMapping()

    // Listen to default firestore
    const unsub = onSnapshot(doc(db, 'config', 'steam_appids'), (snap) => {
      if (!mounted) return
      const data = snap.exists() ? (snap.data() as Record<string, number>) : {}
      cachedMapping = { ...cachedMapping, ...data }
      setMapping(cachedMapping)
      listeners.forEach(fn => fn(cachedMapping!))
    })

    return () => {
      mounted = false
      unsub()
    }
  }, [])

  return { mapping }
}

// Helper: lấy appId từ gameId (dùng ngoài React component nếu cần)
export function getAppIdForGame(gameId: string): number | undefined {
  return cachedMapping?.[gameId]
}
