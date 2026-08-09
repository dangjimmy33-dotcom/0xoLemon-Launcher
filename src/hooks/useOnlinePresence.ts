/**
 * useOnlinePresence — Theo dõi và hiển thị số người dùng đang online.
 *
 * Cách hoạt động:
 *  1. Mỗi lần mở launcher → ghi/update document `presence/{clientId}` trong Firestore socialDb
 *  2. Poll mỗi 2 phút để update lastSeen của mình
 *  3. Query đếm số users có lastSeen trong vòng 5 phút gần nhất
 *  4. Khi unmount → đánh dấu offline
 *
 * Dùng Firestore (không phải RTDB) vì dự án chưa enable RTDB.
 */

import { useEffect, useRef, useState } from 'react'
import {
  collection,
  doc,
  setDoc,
  deleteDoc,
  getDocs,
  query,
  where,
  Timestamp,
} from 'firebase/firestore'
import { socialDb } from '../firebase'

const PRESENCE_COLLECTION = 'launcher_presence'
const HEARTBEAT_INTERVAL_MS = 2 * 60 * 1000   // 2 phút
const ONLINE_WINDOW_MS      = 5 * 60 * 1000   // user được coi là online nếu lastSeen < 5 phút trước

/** Tạo hoặc lấy client ID ẩn danh (persistent qua sessions) */
function getClientId(): string {
  const KEY = '0xolemon_client_id'
  let id = localStorage.getItem(KEY)
  if (!id) {
    id = `anon_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`
    localStorage.setItem(KEY, id)
  }
  return id
}

export interface OnlinePresenceInfo {
  /** Số người dùng online hiện tại (bao gồm cả mình) */
  onlineCount: number
  /** true khi đang tải lần đầu */
  loading: boolean
}

export function useOnlinePresence(
  /** Discord user ID nếu có — dùng thay thế cho anonymous ID */
  discordUserId?: string | null,
): OnlinePresenceInfo {
  const [onlineCount, setOnlineCount] = useState(0)
  const [loading, setLoading] = useState(true)
  const clientId = useRef<string>(discordUserId || getClientId())

  // Cập nhật discordUserId nếu user đăng nhập sau
  useEffect(() => {
    if (discordUserId) {
      clientId.current = discordUserId
    }
  }, [discordUserId])

  useEffect(() => {
    let mounted = true
    const myDocRef = doc(socialDb, PRESENCE_COLLECTION, clientId.current)

    async function heartbeat() {
      try {
        // Cập nhật presence của mình
        await setDoc(myDocRef, {
          lastSeen: Timestamp.now(),
          online: true,
          clientId: clientId.current,
        }, { merge: true })

        // Đếm users online trong window 5 phút
        const cutoff = Timestamp.fromMillis(Date.now() - ONLINE_WINDOW_MS)
        const q = query(
          collection(socialDb, PRESENCE_COLLECTION),
          where('lastSeen', '>=', cutoff),
          where('online', '==', true),
        )
        const snap = await getDocs(q)
        if (mounted) {
          setOnlineCount(Math.max(snap.size, 1)) // ít nhất là 1 (chính mình)
          setLoading(false)
        }
      } catch (err) {
        // Presence failure không nên crash launcher
        console.warn('[useOnlinePresence] heartbeat error:', err)
        if (mounted) setLoading(false)
      }
    }

    // Chạy ngay lập tức
    void heartbeat()

    // Poll định kỳ
    const interval = setInterval(() => void heartbeat(), HEARTBEAT_INTERVAL_MS)

    // Cleanup: đánh dấu offline khi unmount (tab đóng / launcher tắt)
    return () => {
      mounted = false
      clearInterval(interval)
      // Fire-and-forget — không await vì cleanup không async
      void deleteDoc(myDocRef).catch(() => {})
    }

  }, [])

  return { onlineCount, loading }
}
