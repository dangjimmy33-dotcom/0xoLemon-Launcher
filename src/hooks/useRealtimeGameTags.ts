import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'
import { updateGameTagTable, type GameTagTable } from '../lib/gameTags'

export function useRealtimeGameTags() {
  const [isReady, setIsReady] = useState(false)

  useEffect(() => {
    // Lắng nghe realtime document "gameTags" trong collection "config"
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'gameTags'),
      (docSnap) => {
        if (docSnap.exists()) {
          const data = docSnap.data() as GameTagTable
          updateGameTagTable(data)
        }
        setIsReady(true)
      },
      (error) => {
        console.error("Lỗi đồng bộ Game Tags:", error)
        // Nếu lỗi mạng, Firestore persistentLocalCache vẫn sẽ trả về data offline nếu có.
        setIsReady(true)
      }
    )

    return () => unsubscribe()
  }, [])

  return isReady
}
