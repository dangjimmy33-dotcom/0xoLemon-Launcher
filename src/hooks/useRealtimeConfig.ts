import { useEffect, useState } from 'react'
import { fetchWithRetry } from '../lib/fetchWithRetry'

export interface AppConfig {
  launcherVersion?: {
    version: string;
    forceUpdate: boolean;
  };
  globalAlert?: {
    active: boolean;
    message: string;
    type: 'info' | 'warning' | 'error' | 'success';
  };
  featuredGames?: string[];
  livePlayerCount?: Record<string, number>;
}

export function useRealtimeConfig() {
  const [config, setConfig] = useState<AppConfig>({})

  useEffect(() => {
    let mounted = true
    const fetchConfig = async () => {
      try {
        const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
        const response = await fetchWithRetry(`${BACKEND_URL}/api/0xolemon/app-settings`)
        if (!mounted) return
        if (response.ok) {
          const data = await response.json()
          setConfig(data as AppConfig)
        }
      } catch (error) {
        if (!mounted) return
        console.error("Lỗi đồng bộ appSettings:", error)
      }
    }

    fetchConfig()
    return () => {
      mounted = false
    }
  }, [])

  return config
}
