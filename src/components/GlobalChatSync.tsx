import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { collection, onSnapshot, query, limit, orderBy } from 'firebase/firestore'
import { db } from '../firebase'
import type { GameCatalog } from '../types'
import type { ChatMessage } from './GameChat'

export function GlobalChatSync({ catalog }: { catalog: GameCatalog | null }) {
  useEffect(() => {
    if (!catalog || catalog.games.length === 0) return

    // Mở listener ngầm cho tất cả các game để tự động tải tin nhắn/ảnh về máy
    // khi launcher đang mở. (Nếu catalog quá lớn, ta có thể chỉ filter những game user đã cài đặt)
    const unsubscribes = catalog.games.map((game) => {
      const q = query(
        collection(db, 'chats', game.id, 'messages'),
        orderBy('timestamp', 'desc'),
        limit(20) // Chỉ lắng nghe những tin nhắn mới nhất
      )

      return onSnapshot(q, (snapshot) => {
        snapshot.docChanges().forEach((change) => {
          if (change.type === 'added') {
            const data = change.doc.data()
            const ts = data.timestamp?.toMillis ? data.timestamp.toMillis() : Date.now()
            const msg: ChatMessage = {
              id: change.doc.id,
              senderId: data.senderId || 'unknown',
              senderName: data.senderName || 'Unknown',
              text: data.text || '',
              imageBase64: data.imageBase64,
              timestamp: ts
            }

            // Gọi Rust lưu ngầm vào ổ cứng
            invoke('save_chat_message', { gameId: game.id, message: msg }).catch(console.error)
          }
        })
      })
    })

    return () => {
      unsubscribes.forEach(unsub => unsub())
    }
  }, [catalog])

  return null
}
