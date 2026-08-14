import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { LuaGameState } from '../types'

export function useLuaUpdateCheck(appid: number | undefined, isAddedToSteam: boolean) {
  const [updateInfo, setUpdateInfo] = useState<LuaGameState | null>(null)
  const [checking, setChecking] = useState(false)
  const requestGeneration = useRef(0)

  const refresh = useCallback(async () => {
    if (!appid || !isAddedToSteam) {
      requestGeneration.current += 1
      setUpdateInfo(null)
      return null
    }
    const generation = ++requestGeneration.current
    setChecking(true)
    try {
      const state = await invoke<LuaGameState | null>('get_lua_game_state', { appid })
      if (requestGeneration.current === generation) setUpdateInfo(state)
      return state
    } finally {
      if (requestGeneration.current === generation) setChecking(false)
    }
  }, [appid, isAddedToSteam])

  useEffect(() => {
    if (!appid || !isAddedToSteam) {
      const timer = window.setTimeout(() => setUpdateInfo(null), 0)
      return () => window.clearTimeout(timer)
    }

    let mounted = true
    let stopListening: (() => void) | undefined
    const timer = window.setTimeout(() => {
      void refresh()
        .catch((err) => {
          console.error('Failed to read Lua game state:', err)
          if (mounted) setChecking(false)
        })
    }, 0)

    void listen<LuaGameState>('launcher://lua-game-state', (event) => {
      if (mounted && event.payload.appid === appid) {
        setUpdateInfo(event.payload)
      }
    }).then((unlisten) => {
      if (mounted) stopListening = unlisten
      else unlisten()
    })

    return () => {
      mounted = false
      requestGeneration.current += 1
      window.clearTimeout(timer)
      stopListening?.()
    }
  }, [appid, isAddedToSteam, refresh])

  return {
    updateInfo: appid && isAddedToSteam ? updateInfo : null,
    checking: appid && isAddedToSteam ? checking : false,
    refresh,
  }
}
