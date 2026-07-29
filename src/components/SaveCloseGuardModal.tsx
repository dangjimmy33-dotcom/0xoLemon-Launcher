import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { Loader2, AlertTriangle, X } from 'lucide-react'
import './SaveCloseGuardModal.css'

export function SaveCloseGuardModal() {
  const [visible, setVisible] = useState(false)
  const [forceCloseWarning, setForceCloseWarning] = useState(false)

  useEffect(() => {
    // Listen for the request to close while backup is running
    const unlistenRequest = listen('launcher://close-requested-while-backup', () => {
      setVisible(true)
    })

    // Listen for the backup finishing, which releases the guard
    const unlistenRelease = listen('launcher://save-backup-guard-released', () => {
      // If we are showing the modal waiting to close, and it finishes, just exit the app
      setVisible(v => {
        if (v) {
          invoke('exit_app')
        }
        return false
      })
    })

    return () => {
      unlistenRequest.then(f => f())
      unlistenRelease.then(f => f())
    }
  }, [])

  if (!visible) return null

  const handleForceClose = () => {
    if (!forceCloseWarning) {
      setForceCloseWarning(true)
    } else {
      invoke('exit_app')
    }
  }

  return (
    <div className="save-close-guard-overlay">
      <div className="save-close-guard-modal">
        <button className="close-btn" onClick={() => setVisible(false)}>
          <X size={18} />
        </button>
        
        {!forceCloseWarning ? (
          <>
            <div className="save-close-icon">
              <Loader2 size={32} className="spin" />
            </div>
            <h3>Syncing Cloud Saves</h3>
            <p>Please wait while we back up your save files to Google Drive. The launcher will close automatically when finished.</p>
            <div className="save-close-actions">
              <button className="primary-btn" onClick={() => setVisible(false)}>
                Return to Launcher
              </button>
              <button className="text-btn danger" onClick={handleForceClose}>
                Close Anyway
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="save-close-icon warning">
              <AlertTriangle size={32} />
            </div>
            <h3>Are you sure?</h3>
            <p>If you force close now, your save files might not be backed up to the cloud. Local saves on this computer will remain intact.</p>
            <div className="save-close-actions">
              <button className="primary-btn" onClick={() => setForceCloseWarning(false)}>
                Wait for Sync
              </button>
              <button className="danger-btn" onClick={handleForceClose}>
                Force Close
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
