import { useEffect, useState } from 'react'
import { doc, onSnapshot } from 'firebase/firestore'
import { db } from '../firebase'

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
    const unsubscribe = onSnapshot(
      doc(db, 'config', 'appSettings'),
      (docSnap) => {
        if (docSnap.exists()) {
          setConfig(docSnap.data() as AppConfig)
        }
      },
      (error) => {
        console.error("Lỗi đồng bộ appSettings:", error)
      }
    )
    return () => unsubscribe()
  }, [])

  return config
}
