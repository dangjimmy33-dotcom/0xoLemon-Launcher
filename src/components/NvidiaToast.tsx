import { useEffect, useState } from 'react'
import { Gamepad2, X } from 'lucide-react'
import './NvidiaToast.css'

export function NvidiaToast({ onDismiss }: { onDismiss: () => void }) {
  const [visible, setVisible] = useState(false)

  useEffect(() => {
    // Trigger slide-in animation after mount
    const t = window.setTimeout(() => setVisible(true), 16)
    return () => clearTimeout(t)
  }, [])

  function handleDismiss() {
    setVisible(false)
    window.setTimeout(onDismiss, 350)
  }

  return (
    <div className={`nvidia-toast ${visible ? 'nvidia-toast--visible' : ''}`} role="status" aria-live="polite">
      <div className="nvidia-toast-accent" />
      <div className="nvidia-toast-icon">
        <Gamepad2 size={22} />
      </div>
      <div className="nvidia-toast-body">
        <span className="nvidia-toast-title">Game Started</span>
        <span className="nvidia-toast-sub">
          Press <kbd>Shift</kbd>+<kbd>F1</kbd> to open overlay
        </span>
      </div>
      <button className="nvidia-toast-close" onClick={handleDismiss} aria-label="Dismiss">
        <X size={14} />
      </button>
    </div>
  )
}
