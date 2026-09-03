import { useEffect, useState } from 'react'
import { Trophy } from 'lucide-react'
import { subscribeAchievementEvents } from '../lib/achievementEventBus'
import '../assets/achievement-toast.css'

interface Toast {
  id: string;
  gameId: string;
  achievementId: string;
  timestamp: number;
}

export function AchievementToastOverlay() {
  const [toasts, setToasts] = useState<Toast[]>([])

  useEffect(() => {
    return subscribeAchievementEvents((event) => {
      if (event.kind !== 'unlock') return
      const newToast: Toast = {
        id: event.eventId,
        gameId: event.gameId,
        achievementId: event.name || event.achievementId,
        timestamp: event.occurredAt,
      }
      
      setToasts(prev => [...prev, newToast])
      
      // Auto-remove after 5 seconds
      setTimeout(() => {
        setToasts(prev => prev.filter(t => t.id !== newToast.id))
      }, 5000)
    })
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
