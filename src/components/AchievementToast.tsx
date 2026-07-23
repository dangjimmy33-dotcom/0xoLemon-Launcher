import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Trophy } from 'lucide-react'
import { isTauriRuntime } from '../lib/gameMeta'
import '../assets/achievement-toast.css'

interface AchievementUnlockedEvent {
  game_id: string;
  achievement_id: string;
  unlocked_at: string;
}

interface Toast {
  id: string;
  gameId: string;
  achievementId: string;
  timestamp: number;
}

export function AchievementToastOverlay() {
  const [toasts, setToasts] = useState<Toast[]>([])

  useEffect(() => {
    if (!isTauriRuntime()) return
    const unlisten = listen<AchievementUnlockedEvent>('launcher://achievement-unlocked', (event) => {
      const newToast: Toast = {
        id: Math.random().toString(36).substr(2, 9),
        gameId: event.payload.game_id,
        achievementId: event.payload.achievement_id,
        timestamp: Date.now()
      }
      
      setToasts(prev => [...prev, newToast])
      
      // Auto-remove after 5 seconds
      setTimeout(() => {
        setToasts(prev => prev.filter(t => t.id !== newToast.id))
      }, 5000)
    })

    return () => {
      unlisten.then(f => f())
    }
  }, [])

  if (toasts.length === 0) return null

  return (
    <div className="achievement-toast-container">
      {toasts.map(toast => (
        <div key={toast.id} className="achievement-toast slide-in">
          <div className="achievement-toast-icon">
            <Trophy size={24} />
          </div>
          <div className="achievement-toast-content">
            <div className="achievement-toast-title">Achievement Unlocked!</div>
            <div className="achievement-toast-name">{toast.achievementId}</div>
          </div>
        </div>
      ))}
    </div>
  )
}
