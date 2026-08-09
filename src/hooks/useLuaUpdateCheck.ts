import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

export interface LuaUpdateInfo {
  needs_update: boolean
  reason: string
  is_missing: boolean
}

export function useLuaUpdateCheck(appid: number | undefined, isAddedToSteam: boolean) {
  const [updateInfo, setUpdateInfo] = useState<LuaUpdateInfo | null>(null)
  const [checking, setChecking] = useState(false)

  useEffect(() => {
    if (!appid || !isAddedToSteam) return

    let mounted = true
    const timer = window.setTimeout(() => {
      setChecking(true)
      invoke<LuaUpdateInfo>('check_steam_update', { appid })
        .then((info) => {
          if (mounted) {
            setUpdateInfo(info)
            setChecking(false)
          }
        })
        .catch((err) => {
          console.error('Failed to check Lua update:', err)
          if (mounted) setChecking(false)
        })
    }, 0)

    return () => {
      mounted = false
      window.clearTimeout(timer)
    }
  }, [appid, isAddedToSteam])

  return {
    updateInfo: appid && isAddedToSteam ? updateInfo : null,
    checking: appid && isAddedToSteam ? checking : false,
  }
}
