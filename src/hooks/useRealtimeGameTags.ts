import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'
import { updateGameTagTable, type GameTagTable } from '../lib/gameTags'

export function useRealtimeGameTags() {
  const [tagVersion, setTagVersion] = useState(0)

  useEffect(() => {
    // Lắng nghe realtime document "gameTags" trong collection "config"
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'gameTags'),
      (docSnap) => {
        if (docSnap.exists()) {
          const data = docSnap.data() as Partial<GameTagTable>
          updateGameTagTable(data)
        }
        setTagVersion(v => v + 1)
      },
      (error) => {
        console.error("Lỗi đồng bộ Game Tags:", error)
        // Nếu lỗi mạng, Firestore persistentLocalCache vẫn sẽ trả về data offline nếu có.
        setTagVersion(v => v + 1)
      }
    )

    return () => unsubscribe()
  }, [])

  return tagVersion
}
