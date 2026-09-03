import { useEffect, useState } from 'react'
import { subscribeSocialOnlineCount } from '../social/socialApi'

export type OnlinePresenceInfo = {
  onlineCount: number
  loading: boolean
}

/**
 * Presence is owned by the authenticated Render social service. This hook is a
 * small projection for the title bar; it never reads or writes Firestore.
 */
export function useOnlinePresence(_discordUserId?: string | null): OnlinePresenceInfo {
  const [onlineCount, setOnlineCount] = useState(0)

  useEffect(() => subscribeSocialOnlineCount(setOnlineCount), [])

  return { onlineCount, loading: false }
}
