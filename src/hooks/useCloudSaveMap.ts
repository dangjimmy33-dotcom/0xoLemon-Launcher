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
import { invoke } from '@tauri-apps/api/core'
import { fetchWithRetry } from '../lib/fetchWithRetry'
import { isTauriRuntime } from '../lib/tauriRuntime'

export function useCloudSaveMap(): void {
  // Track last pushed mapVersion to avoid redundant Tauri calls
  const lastVersionRef = useRef<string | null>(null)

  useEffect(() => {
    if (!isTauriRuntime()) return

    const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
    
    let mounted = true
    const fetchMap = async () => {
      try {
        const res = await fetchWithRetry(`${BACKEND_URL}/api/0xolemon/cloud-save-map`)
        if (!mounted || !res.ok) return
        
        const data = await res.json()
        const mapVersion: string = data.mapVersion ?? ''
        if (mapVersion && mapVersion === lastVersionRef.current) return

        try {
          await invoke('push_cloud_save_map', { payload: JSON.stringify(data) })
          lastVersionRef.current = mapVersion
        } catch (err) {
          console.error('[CloudSaveMap] push failed:', err)
        }
      } catch (err) {
        console.warn('[CloudSaveMap] fetch error:', err)
      }
    }

    fetchMap()

    return () => {
      mounted = false
    }
  }, [])
}
