import { useEffect, useMemo, useRef } from 'react'
import { createPortal } from 'react-dom'
import { CircleAlert, FolderSearch, X } from 'lucide-react'
import { useLocale } from '../context/locale'
import type { InstallDiscoveryConflict } from '../types'

type InstallRecoveryDialogProps = {
  open: boolean
  conflicts: InstallDiscoveryConflict[]
  gameTitles: Record<string, string>
  busy: boolean
  onResolve: (gameId: string, installPath: string) => void
  onLocate: () => void
  onClose: () => void
}

export function InstallRecoveryDialog({
  open,
  conflicts,
  gameTitles,
  busy,
  onResolve,
  onLocate,
  onClose,
}: InstallRecoveryDialogProps) {
  const { t } = useLocale()
  const dialogRef = useRef<HTMLElement>(null)
  const busyRef = useRef(busy)
  const onCloseRef = useRef(onClose)
  const titleId = 'install-recovery-title'
  const hasConflicts = conflicts.length > 0
  const description = useMemo(() => (
    hasConflicts
      ? t.installRecovery.conflictDescription
      : t.installRecovery.locateDescription
  ), [hasConflicts, t.installRecovery.conflictDescription, t.installRecovery.locateDescription])

  useEffect(() => {
    busyRef.current = busy
  }, [busy])

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    if (!open) return
    const dialog = dialogRef.current
    const previousFocus = document.activeElement as HTMLElement | null
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ) ?? [])
    focusable()[0]?.focus()

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busyRef.current) {
        event.preventDefault()
        onCloseRef.current()
        return
      }
      if (event.key !== 'Tab') return
      const items = focusable()
      if (items.length === 0) return
      const first = items[0]
      const last = items[items.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      previousFocus?.focus()
    }
  }, [open])

  if (!open) return null

  return createPortal(
    <div className="install-recovery-backdrop" role="presentation" onMouseDown={() => !busy && onClose()}>
      <section
        ref={dialogRef}
        className="install-recovery-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="install-recovery-header">
          <div className="install-recovery-icon" aria-hidden="true">
            {hasConflicts ? <CircleAlert size={20} /> : <FolderSearch size={20} />}
          </div>
          <div>
            <h2 id={titleId}>{hasConflicts ? t.installRecovery.conflictTitle : t.installRecovery.locateTitle}</h2>
            <p>{description}</p>
          </div>
          <button type="button" className="install-recovery-close" onClick={onClose} disabled={busy} aria-label={t.installRecovery.close}>
            <X size={18} />
          </button>
        </header>

        {hasConflicts && (
          <div className="install-recovery-conflicts">
            {conflicts.map((conflict) => (
              <section className="install-recovery-conflict" key={conflict.gameId}>
                <h3>{gameTitles[conflict.gameId] || conflict.gameId}</h3>
                <div className="install-recovery-paths">
                  {conflict.candidatePaths.map((path) => (
                    <button
                      type="button"
                      key={path}
                      disabled={busy}
                      onClick={() => onResolve(conflict.gameId, path)}
                      title={path}
                    >
                      <FolderSearch size={16} />
                      <span>{path}</span>
                    </button>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}

        <footer className="install-recovery-footer">
          <button type="button" className="secondary" onClick={onClose} disabled={busy}>{t.installRecovery.later}</button>
          <button type="button" className="primary-control" onClick={onLocate} disabled={busy}>
            <FolderSearch size={16} />
            {busy ? t.installRecovery.checking : t.installRecovery.chooseLibrary}
          </button>
        </footer>
      </section>
    </div>,
    document.body,
  )
}
