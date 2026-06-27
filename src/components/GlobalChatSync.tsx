import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { collection, onSnapshot, query, limit, orderBy, doc, runTransaction } from 'firebase/firestore'
import { db } from '../firebase'
import type { GameCatalog } from '../types'
import type { ChatMessage } from './GameChat'

export function GlobalChatSync({ catalog }: { catalog: GameCatalog | null }) {
  useEffect(() => {
    if (!catalog || catalog.games.length === 0) return

    const unsubscribes = catalog.games.map((game) => {
      const q = query(
        collection(db, 'chats', game.id, 'messages'),
        orderBy('timestamp', 'desc'),
        limit(20)
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

            invoke('save_chat_message', { gameId: game.id, message: msg }).catch(console.error)
          }
        })
      })
    })

    // Hugging Face Sync Leader Election
    // Kiểm tra và đẩy file lên Hugging Face 30 phút một lần
    const hfSyncInterval = window.setInterval(() => {
      catalog.games.forEach((game, index) => {
        window.setTimeout(async () => {
          try {
            const metaRef = doc(db, 'chat_meta', game.id)
            await runTransaction(db, async (transaction) => {
              const sfDoc = await transaction.get(metaRef)
              const now = Date.now()
              if (!sfDoc.exists()) {
                transaction.set(metaRef, { lastHfSyncTime: now })
              } else {
                const last = sfDoc.data().lastHfSyncTime || 0
                // 30 minutes = 1_800_000 ms
                if (now - last < 1800000) {
                  return Promise.reject('Not enough time passed')
                }
                transaction.update(metaRef, { lastHfSyncTime: now })
              }
            })
            // Nếu transaction thành công (không bị reject / tranh chấp rớt),
            // máy này chính thức trở thành Leader cho đợt đẩy này!
            console.log(`[HF Sync] Won leader election for ${game.id}. Syncing to Hugging Face...`)
            await invoke('sync_to_huggingface', { gameId: game.id })
            console.log(`[HF Sync] Success for ${game.id}!`)
          } catch (e) {
            // Ignored, someone else is leader or it's not time yet
          }
        }, index * 2000) // Stagger mỗi game cách nhau 2s để không lag
      })
    }, 5 * 60_000) // Kiểm tra mỗi 5 phút

    return () => {
      unsubscribes.forEach(unsub => unsub())
      window.clearInterval(hfSyncInterval)
    }
  }, [catalog])

  return null
}
