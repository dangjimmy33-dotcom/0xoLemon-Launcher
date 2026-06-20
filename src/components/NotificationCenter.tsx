import { AnimatePresence, motion } from 'motion/react'
import {
  Bell,
  Check,
  CheckCircle2,
  Cloud,
  Download,
  HardDrive,
  RefreshCcw,
  ShieldAlert,
  Sparkles,
  Trash2,
  TriangleAlert,
  X,
} from 'lucide-react'
import { MOTION } from '../lib/motion'
import type { NotificationRecord } from '../types'

export function NotificationPopover({
  open,
  notifications,
  onClose,
  onOpenNotification,
  onMarkAllRead,
  onClear,
  onOpenSettings,
}: {
  open: boolean
  notifications: NotificationRecord[]
  onClose: () => void
  onOpenNotification: (notification: NotificationRecord) => void
  onMarkAllRead: () => void
  onClear: () => void
  onOpenSettings: () => void
}) {
  const unread = notifications.filter((item) => !item.read).length
  if (!open) return null
  return (
    <motion.section
      className="notification-popover"
      role="dialog"
      aria-label="Notifications"
      initial={{ opacity: 0, y: -8, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={MOTION.panel}
    >
          <header>
            <div>
              <Bell size={18} />
              <h2>Notifications</h2>
              {unread > 0 ? <span>{unread} unread</span> : null}
            </div>
            <button type="button" onClick={onClose} aria-label="Close notifications"><X size={17} /></button>
          </header>
          <div className="notification-toolbar">
            <button type="button" onClick={onMarkAllRead} disabled={unread === 0}>
              <Check size={14} /> Mark all read
            </button>
            <button type="button" onClick={onClear} disabled={notifications.length === 0}>
              <Trash2 size={14} /> Clear
            </button>
          </div>
          <div className="notification-list">
            {notifications.length === 0 ? (
              <div className="notification-empty">
                <CheckCircle2 size={24} />
                <strong>You’re all caught up</strong>
                <span>Real launcher events will appear here.</span>
              </div>
            ) : (
              notifications.slice(0, 30).map((notification) => (
                <button
                  type="button"
                  key={notification.id}
                  className={notification.read ? 'notification-item' : 'notification-item is-unread'}
                  onClick={() => onOpenNotification(notification)}
                >
                  <NotificationIcon notification={notification} />
                  <span className="notification-item-copy">
                    <strong>{notification.title}</strong>
                    <span>{notification.message}</span>
                    <small>{formatNotificationTime(notification.timestamp)}</small>
                  </span>
                  {!notification.read ? <i /> : null}
                </button>
              ))
            )}
          </div>
          <footer>
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation()
                onClose()
                onOpenSettings()
              }}
            >
              Notification settings
            </button>
          </footer>
    </motion.section>
  )
}

export function NotificationToasts({
  notifications,
  onOpen,
  onDismiss,
}: {
  notifications: NotificationRecord[]
  onOpen: (notification: NotificationRecord) => void
  onDismiss: (notificationId: string) => void
}) {
  return (
    <div className="notification-toast-stack" aria-live="polite">
      <AnimatePresence>
        {notifications.map((notification) => (
          <motion.article
            className={`notification-toast severity-${notification.severity}`}
            key={notification.id}
            initial={{ opacity: 0, x: 36, scale: 0.97 }}
            animate={{ opacity: 1, x: 0, scale: 1 }}
            exit={{ opacity: 0, x: 24, scale: 0.98 }}
            transition={MOTION.panel}
          >
            <button type="button" className="notification-toast-main" onClick={() => onOpen(notification)}>
              <NotificationIcon notification={notification} />
              <span>
                <strong>{notification.title}</strong>
                <small>{notification.message}</small>
              </span>
            </button>
            <button type="button" className="notification-toast-close" onClick={() => onDismiss(notification.id)} aria-label="Dismiss">
              <X size={15} />
            </button>
          </motion.article>
        ))}
      </AnimatePresence>
    </div>
  )
}

function NotificationIcon({ notification }: { notification: NotificationRecord }) {
  const props = { size: 17, strokeWidth: 1.9 }
  switch (notification.category) {
    case 'launcher': return <span className="notification-icon"><RefreshCcw {...props} /></span>
    case 'downloads':
    case 'installs': return <span className="notification-icon"><Download {...props} /></span>
    case 'cloudSaves': return <span className="notification-icon"><Cloud {...props} /></span>
    case 'storage': return <span className="notification-icon"><HardDrive {...props} /></span>
    case 'achievements': return <span className="notification-icon"><Sparkles {...props} /></span>
    case 'errors': return <span className="notification-icon"><ShieldAlert {...props} /></span>
    default: return <span className="notification-icon"><TriangleAlert {...props} /></span>
  }
}

function formatNotificationTime(value: string) {
  const date = new Date(value)
  const elapsed = Date.now() - date.getTime()
  if (elapsed < 60_000) return 'Just now'
  if (elapsed < 3_600_000) return `${Math.floor(elapsed / 60_000)} min ago`
  if (elapsed < 86_400_000) return `${Math.floor(elapsed / 3_600_000)} hr ago`
  return date.toLocaleDateString()
}
