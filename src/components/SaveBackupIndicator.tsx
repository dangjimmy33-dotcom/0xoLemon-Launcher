import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Check, X, Loader2 } from 'lucide-react'
import './SaveBackupIndicator.css'

export interface SaveBackupProgressEvent {
  gameId: string
  state: 'starting' | 'copying' | 'uploading' | 'done' | 'error' | 'skipped'
  message: string
  filesCopied: number
  bytesCopied: number
  snapshotId: string | null
}

export function SaveBackupIndicator({ gameId }: { gameId: string }) {
  const [event, setEvent] = useState<SaveBackupProgressEvent | null>(null)
  const [visible, setVisible] = useState(false)

  useEffect(() => {
    let timeout: ReturnType<typeof setTimeout>

    const unlisten = listen<SaveBackupProgressEvent>('launcher://save-backup-progress', (e) => {
      if (e.payload.gameId === gameId) {
        setEvent(e.payload)
        
        if (e.payload.state !== 'skipped') {
          setVisible(true)
        }

        if (e.payload.state === 'done' || e.payload.state === 'error' || e.payload.state === 'skipped') {
          clearTimeout(timeout)
          timeout = setTimeout(() => {
            setVisible(false)
            // Wait for fade out animation before clearing data
            setTimeout(() => setEvent(null), 300)
          }, 4000)
        }
      }
    })

    return () => {
      unlisten.then(f => f())
      clearTimeout(timeout)
    }
  }, [gameId])

  if (!event && !visible) return null

  let icon = <Loader2 size={14} className="save-backup-spinner" />
  let colorClass = 'save-backup-running'
  
  if (event?.state === 'done') {
    icon = <Check size={14} />
    colorClass = 'save-backup-success'
  } else if (event?.state === 'error') {
    icon = <X size={14} />
    colorClass = 'save-backup-error'
  }

  return (
    <div className={`save-backup-indicator ${visible ? 'visible' : ''} ${colorClass}`}>
      {icon}
      <span>{event?.state === 'done' ? 'Cloud Sync' : 'Syncing'}</span>
      {event?.state === 'error' && (
        <div className="save-backup-tooltip">{event.message}</div>
      )}
    </div>
  )
}
