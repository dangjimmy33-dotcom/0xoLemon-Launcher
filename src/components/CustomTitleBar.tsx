import { useEffect, useMemo, useState } from 'react'
import type React from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Bell, Download, LogOut, Wifi, WifiOff, Monitor } from 'lucide-react'
import { isTauriRuntime } from '../lib/gameMeta'
import type { ClockFormat, CloseBehavior } from '../lib/preferences'
import type { DiscordAuthUser, JobJournal, LauncherUpdateProgress, NotificationRecord } from '../types'
import { NotificationPopover } from './NotificationCenter'

type StatusPreferences = {
  showClock: boolean
  showDate: boolean
  showNetworkStatus: boolean
  showDownloadIndicator: boolean
  showNotificationBell: boolean
  clockFormat: ClockFormat
  hoverHints: boolean
  glassEffects: boolean
}

export function CustomTitleBar({
  closeBehavior = 'exit',
  serviceOnline,
  job,
  updateProgress,
  notifications,
  notificationOpen,
  discordUser,
  statusPreferences,
  isBlockedState = false,
  onToggleNotifications,
  onCloseNotifications,
  onOpenNotification,
  onMarkAllNotificationsRead,
  onClearNotifications,
  onOpenNotificationSettings,
  onDiscordLogout,
  onToggleBigPicture,
}: {
  closeBehavior?: CloseBehavior
  serviceOnline: boolean
  job: JobJournal | null
  updateProgress: LauncherUpdateProgress | null
  notifications: NotificationRecord[]
  notificationOpen: boolean
  discordUser: DiscordAuthUser | null
  statusPreferences: StatusPreferences
  isBlockedState?: boolean
  onToggleNotifications: () => void
  onCloseNotifications: () => void
  onOpenNotification: (notification: NotificationRecord) => void
  onMarkAllNotificationsRead: () => void
  onClearNotifications: () => void
  onOpenNotificationSettings: () => void
  onDiscordLogout: () => void
  onToggleBigPicture: () => void
}) {
  const win = isTauriRuntime() ? getCurrentWindow() : null
  const [now, setNow] = useState(() => new Date())
  const unread = notifications.filter((notification) => !notification.read).length
  const activeJob = job && !['committed', 'failed', 'canceled'].includes(job.status) ? job : null
  const updateActive = updateProgress && ['downloading', 'verifying', 'installing', 'restarting'].includes(updateProgress.phase)
  const updatePercent =
    updateProgress?.totalBytes && updateProgress.phase === 'downloading'
      ? Math.min(100, Math.round((updateProgress.downloadedBytes / updateProgress.totalBytes) * 100))
      : null
  const jobPercent = activeJob ? Math.min(100, Math.round(activeJob.overallProgress * 100)) : null
  const taskPercent = updatePercent ?? jobPercent

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  const clock = useMemo(() => {
    const hour12 =
      statusPreferences.clockFormat === '12h'
        ? true
        : statusPreferences.clockFormat === '24h'
          ? false
          : undefined
    return now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12 })
  }, [now, statusPreferences.clockFormat])

  function handleMinimize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.minimize()
  }
  function handleMaximize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.toggleMaximize()
  }
  function handleClose(e: React.MouseEvent) {
    e.stopPropagation()
    if (closeBehavior === 'minimize') {
      void win?.minimize()
      return
    }
    void win?.close()
  }

  return (
    <div
      data-tauri-drag-region
      className={`custom-titlebar premium-titlebar${statusPreferences.glassEffects ? ' use-glass' : ''}`}
    >
      <div className="titlebar-drag-area" data-tauri-drag-region>
        <span className="titlebar-label">0xoLemon Launcher</span>
        {taskPercent !== null ? (
          <div className="titlebar-mini-progress" aria-label={`${taskPercent}% complete`}>
            <i style={{ width: `${Math.max(2, taskPercent)}%` }} />
          </div>
        ) : updateActive ? (
          <div className="titlebar-mini-progress is-indeterminate"><i /></div>
        ) : null}
      </div>

      <div className="titlebar-status-cluster">
        {statusPreferences.showNetworkStatus ? (
          <span
            className={`titlebar-status-icon${serviceOnline ? '' : ' is-offline'}`}
            data-hint={statusPreferences.hoverHints ? (serviceOnline ? 'Content service online' : 'Content service unavailable') : undefined}
          >
            {serviceOnline ? <Wifi size={14} /> : <WifiOff size={14} />}
          </span>
        ) : null}
        {statusPreferences.showDownloadIndicator && (activeJob || updateActive) ? (
          <span
            className="titlebar-status-icon has-task"
            data-hint={statusPreferences.hoverHints ? (updateActive ? `Launcher ${updateProgress?.phase}` : activeJob?.phase) : undefined}
          >
            <Download size={14} />
            {taskPercent !== null ? <small>{taskPercent}%</small> : null}
          </span>
        ) : null}
        {statusPreferences.showClock ? (
          <div className="titlebar-clock">
            <strong>{clock}</strong>
            {statusPreferences.showDate ? (
              <span>{now.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })}</span>
            ) : null}
          </div>
        ) : null}
        {discordUser ? (
          <button
            type="button"
            className="titlebar-discord-user"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={onDiscordLogout}
            title="Sign out of Discord"
            aria-label={`Discord user ${discordUser.displayName}. Sign out`}
          >
            <img src={discordUser.avatarUrl} alt="" />
            <span>{discordUser.displayName}</span>
            <LogOut size={12} />
          </button>
        ) : null}
        {statusPreferences.showNotificationBell && !isBlockedState ? (
          <div className="titlebar-notification-anchor">
            <button
              type="button"
              className={notificationOpen ? 'titlebar-bell is-active' : 'titlebar-bell'}
              onMouseDown={(event) => event.stopPropagation()}
              onClick={onToggleNotifications}
              aria-label={`Notifications${unread > 0 ? `, ${unread} unread` : ''}`}
              aria-expanded={notificationOpen}
              data-hint={statusPreferences.hoverHints ? 'Notifications' : undefined}
            >
              <Bell size={16} />
              {unread > 0 ? <i>{Math.min(unread, 99)}</i> : null}
            </button>
            <NotificationPopover
              open={notificationOpen}
              notifications={notifications}
              onClose={onCloseNotifications}
              onOpenNotification={onOpenNotification}
              onMarkAllRead={onMarkAllNotificationsRead}
              onClear={onClearNotifications}
              onOpenSettings={onOpenNotificationSettings}
            />
          </div>
        ) : null}
        {!isBlockedState && (
          <button
            type="button"
            className="titlebar-bell"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={onToggleBigPicture}
            title="Enter Big Picture Mode"
            data-hint={statusPreferences.hoverHints ? 'Big Picture Mode' : undefined}
          >
            <Monitor size={16} />
          </button>
        )}
      </div>

      <div className="titlebar-actions">
        <button
          className="titlebar-btn minimize-btn"
          title="Minimize"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMinimize}
        >
          <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor" /></svg>
        </button>
        <button
          className="titlebar-btn maximize-btn"
          title="Maximize"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMaximize}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" /></svg>
        </button>
        <button
          className="titlebar-btn close-btn"
          title={closeBehavior === 'minimize' ? 'Minimize to taskbar' : 'Exit launcher'}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" strokeWidth="1.2" /><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" strokeWidth="1.2" /></svg>
        </button>
      </div>
    </div>
  )
}
