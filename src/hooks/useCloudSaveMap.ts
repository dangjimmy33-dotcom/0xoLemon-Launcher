/**
 * useCloudSaveMap.ts
 *
 * Lắng nghe Firestore contentDb → config/cloudSaveMap realtime.
 * Khi có data mới → đẩy vào Rust qua Tauri command `push_cloud_save_map`
 * để Rust lưu vào local LKG cache và dùng cho cloud save operations.
 *
 * Không expose bất kỳ dữ liệu map nào ra ngoài — đây là internal sync only.
 */

import { useEffect, useRef } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { invoke } from '@tauri-apps/api/core'
import { contentDb } from '../firebase'
import { isTauriRuntime } from '../lib/tauriRuntime'

export function useCloudSaveMap(): void {
  // Track last pushed mapVersion to avoid redundant Tauri calls
  const lastVersionRef = useRef<string | null>(null)

  useEffect(() => {
    if (!isTauriRuntime()) return

    const unsubscribe = onSnapshot(
      doc(contentDb, 'config', 'cloudSaveMap'),
      async (snap) => {
        if (!snap.exists()) return
        const data = snap.data()
        if (!data) return

        const mapVersion: string = data.mapVersion ?? ''
        if (mapVersion && mapVersion === lastVersionRef.current) return

        try {
          await invoke('push_cloud_save_map', { payload: JSON.stringify(data) })
          lastVersionRef.current = mapVersion
        } catch (err) {
          // Rust logs internally; no user-facing noise needed
          console.error('[CloudSaveMap] push failed:', err)
        }
      },
      (err) => {
        // Offline / network error — Rust falls back to LKG cache automatically
        console.warn('[CloudSaveMap] snapshot error:', err.code)
      }
    )

    return () => unsubscribe()
  }, [])
}
