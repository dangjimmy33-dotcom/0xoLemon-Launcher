import { createPortal } from 'react-dom'
import { AlertTriangle, CheckCircle2, X } from 'lucide-react'

interface ConfirmDialogProps {
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  variant?: 'warning' | 'info' | 'danger'
  onConfirm: () => void
  onCancel: () => void
  children?: React.ReactNode
}

export function ConfirmDialog({
  title,
  message,
  confirmText = 'Confirm',
  cancelText = 'Cancel',
  variant = 'info',
  onConfirm,
  onCancel,
  children,
}: ConfirmDialogProps) {
  const icon = variant === 'warning' || variant === 'danger' ? (
    <AlertTriangle size={20} />
  ) : (
    <CheckCircle2 size={20} />
  )

  return createPortal(
    <div
      className="dialog-backdrop confirm-dialog-backdrop"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onCancel()
      }}
    >
      <section
        className="confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
      >
        <div className="modal-handle" />

        <button
          type="button"
          className="confirm-dialog-close"
          onClick={onCancel}
          aria-label="Close"
        >
          <X size={18} />
        </button>

        <div className={`confirm-dialog-icon confirm-dialog-icon--${variant}`}>
          {icon}
        </div>

        <header className="confirm-dialog-header">
          <h2 id="confirm-dialog-title">{title}</h2>
          <p>{message}</p>
        </header>

        {children && (
          <div className="confirm-dialog-body" style={{ marginTop: '16px' }}>
            {children}
          </div>
        )}

        <footer className="confirm-dialog-footer">
          <button
            type="button"
            className="secondary"
            onClick={onCancel}
          >
            {cancelText}
          </button>
          <button
            type="button"
            className={`primary-control confirm-dialog-${variant}`}
            onClick={onConfirm}
          >
            {confirmText}
          </button>
        </footer>
      </section>
    </div>,
    document.body
  )
}
