import { useEffect } from 'react'
import { collection, doc, onSnapshot, deleteDoc, setDoc, serverTimestamp } from 'firebase/firestore'
import { db } from '../firebase'
import { isTauriRuntime } from '../lib/gameMeta'
import { invoke } from '@tauri-apps/api/core'
import type { DiscordAuthUser } from '../types'

export function FirebaseRemoteControl({
  user,
}: {
  user: DiscordAuthUser
}) {
  useEffect(() => {
    // ONLY RUN THIS ON PC LAUNCHER
    if (!isTauriRuntime()) return

    const userId = user.id
    if (!userId) return

    // 1. Publish PC Online State
    const statusRef = doc(db, 'users', userId, 'pc_status', 'current')
    setDoc(statusRef, {
      online: true,
      lastSeen: serverTimestamp(),
      platform: navigator.platform
    }).catch(console.error)

    // Keep updating status every 5 minutes
    const interval = setInterval(() => {
      setDoc(statusRef, {
        online: true,
        lastSeen: serverTimestamp(),
      }, { merge: true }).catch(console.error)
    }, 5 * 60 * 1000)

    // 2. Listen for incoming commands
    const commandsRef = collection(db, 'users', userId, 'commands')
    const unsubscribe = onSnapshot(commandsRef, (snapshot) => {
      snapshot.docChanges().forEach(async (change) => {
        if (change.type === 'added') {
          const commandData = change.doc.data()
          const commandId = change.doc.id
          
          try {
            if (commandData.action === 'install' && commandData.game_id) {
              // Trigger the Tauri download
              await invoke('request_download_game', { gameId: commandData.game_id })
              console.log(`Remote install requested for ${commandData.game_id}`)
            } else if (commandData.action === 'launch' && commandData.game_id) {
              await invoke('launch_game', { gameId: commandData.game_id })
            }
          } catch (err) {
            console.error('Remote command failed:', err)
          } finally {
            // Delete the command so it doesn't run again
            await deleteDoc(doc(db, 'users', userId, 'commands', commandId))
          }
        }
      })
    })

    return () => {
      clearInterval(interval)
      unsubscribe()
      // Mark offline on unmount
      setDoc(statusRef, { online: false, lastSeen: serverTimestamp() }, { merge: true }).catch(console.error)
    }
  }, [user.id])

  return null // This is a logic-only component
}
