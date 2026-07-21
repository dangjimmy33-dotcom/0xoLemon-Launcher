import { useEffect, useMemo, useState, useRef } from 'react'
import type React from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Bell, Download, LogOut, Monitor } from 'lucide-react'
import { isTauriRuntime } from '../lib/gameMeta'
import type { ClockFormat, CloseBehavior } from '../lib/preferences'
import type { DiscordAuthUser, JobJournal, LauncherUpdateProgress, NotificationRecord } from '../types'
import { NotificationPopover } from './NotificationCenter'

type NetworkQuality = 'good' | 'weak' | 'offline'
type BatteryState = { level: number; charging: boolean } | null

function useNetworkQuality(): NetworkQuality {
  const getQuality = (): NetworkQuality => {
    if (!navigator.onLine) return 'offline'
    const conn = (navigator as any).connection ||
                 (navigator as any).mozConnection ||
                 (navigator as any).webkitConnection
    if (!conn) return 'good'
    const type: string = conn.effectiveType || ''
    if (type === 'slow-2g' || type === '2g') return 'weak'
    if (type === '3g') return 'weak'
    return 'good'
  }
  const [quality, setQuality] = useState<NetworkQuality>(getQuality)
  useEffect(() => {
    const update = () => setQuality(getQuality())
    window.addEventListener('online', update)
    window.addEventListener('offline', update)
    const conn = (navigator as any).connection ||
                 (navigator as any).mozConnection ||
                 (navigator as any).webkitConnection
    conn?.addEventListener('change', update)
    return () => {
      window.removeEventListener('online', update)
      window.removeEventListener('offline', update)
      conn?.removeEventListener('change', update)
    }
  }, [])
  return quality
}

function useBattery(): BatteryState {
  const [battery, setBattery] = useState<BatteryState>(null)
  const batteryRef = useRef<any>(null)
  useEffect(() => {
    if (!('getBattery' in navigator)) return
    ;(navigator as any).getBattery().then((bat: any) => {
      batteryRef.current = bat
      const update = () => setBattery({ level: bat.level, charging: bat.charging })
      update()
      bat.addEventListener('levelchange', update)
      bat.addEventListener('chargingchange', update)
      return () => {
        bat.removeEventListener('levelchange', update)
        bat.removeEventListener('chargingchange', update)
      }
    })
  }, [])
  return battery
}

function WifiIcon({ quality }: { quality: NetworkQuality }) {
  const color = quality === 'good' ? '#4ade80' : quality === 'weak' ? '#facc15' : '#f87171'
  if (quality === 'offline') {
    return (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <line x1="1" y1="1" x2="23" y2="23"/>
        <path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55"/>
        <path d="M5 12.55a10.94 10.94 0 0 1 5.17-2.39"/>
        <path d="M10.71 5.05A16 16 0 0 1 22.56 9"/>
        <path d="M1.42 9a15.91 15.91 0 0 1 4.7-2.88"/>
        <path d="M8.53 16.11a6 6 0 0 1 6.95 0"/>
        <circle cx="12" cy="20" r="1" fill={color}/>
      </svg>
    )
  }
  if (quality === 'weak') {
    return (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path d="M5 12.55a10.94 10.94 0 0 1 14 0" stroke={color} strokeWidth="2" opacity="0.3"/>
        <path d="M1.42 9a15.91 15.91 0 0 1 21.16 0" stroke={color} strokeWidth="2" opacity="0.2"/>
        <path d="M8.53 16.11a6 6 0 0 1 6.95 0" stroke={color} strokeWidth="2"/>
        <circle cx="12" cy="20" r="1" fill={color}/>
      </svg>
    )
  }
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 12.55a10.94 10.94 0 0 1 14 0"/>
      <path d="M1.42 9a15.91 15.91 0 0 1 21.16 0"/>
      <path d="M8.53 16.11a6 6 0 0 1 6.95 0"/>
      <circle cx="12" cy="20" r="1" fill={color}/>
    </svg>
  )
}

function BatteryIcon({ battery }: { battery: BatteryState }) {
  if (!battery) return null
  const pct = Math.round(battery.level * 100)
  const color = pct > 50 ? '#4ade80' : pct > 20 ? '#facc15' : '#f87171'
  const fillWidth = Math.max(0, Math.min(13, Math.round((pct / 100) * 13)))
  return (
    <span style={{ display: 'flex', alignItems: 'center', gap: 2 }} title={`Battery: ${pct}%${battery.charging ? ' (charging)' : ''}`}>
      <svg width="18" height="10" viewBox="0 0 18 10" fill="none">
        {/* Battery body */}
        <rect x="0.5" y="0.5" width="15" height="9" rx="2" ry="2" stroke={color} strokeWidth="1"/>
        {/* Battery tip */}
        <path d="M16 3.5 L16 6.5 Q18 6.5 18 5 Q18 3.5 16 3.5" fill={color}/>
        {/* Fill */}
        <rect x="2" y="2" width={fillWidth} height="6" rx="1" fill={color}/>
        {/* Charging bolt */}
        {battery.charging && (
          <path d="M8 1.5 L5.5 5.5 H8 L6 8.5 L10.5 4 H8 Z" fill="#fff" opacity="0.9" stroke="none"/>
        )}
      </svg>
    </span>
  )
}

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
  onToggleSidebar,
  isSidebarCollapsed,
}: {
  closeBehavior?: CloseBehavior
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
  onToggleSidebar?: () => void
  isSidebarCollapsed?: boolean
}) {
  const win = isTauriRuntime() ? getCurrentWindow() : null
  const [now, setNow] = useState(() => new Date())
  const networkQuality = useNetworkQuality()
  const battery = useBattery()
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
      {/* Toggle outside drag area for independent sizing */}
      {onToggleSidebar && (
        <button
          className={`titlebar-sidebar-toggle${isSidebarCollapsed ? ' is-collapsed' : ''}`}
          onClick={onToggleSidebar}
          aria-label={isSidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          title={isSidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          {isSidebarCollapsed ? (
            /* Sidebar collapsed → show open arrow */
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
              <line x1="9" y1="3" x2="9" y2="21" />
              <path d="M14 9l3 3-3 3" />
            </svg>
          ) : (
            /* Sidebar open → show collapse arrow */
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
              <line x1="9" y1="3" x2="9" y2="21" />
              <path d="M17 9l-3 3 3 3" />
            </svg>
          )}
        </button>
      )}
      <div className="titlebar-drag-area" data-tauri-drag-region>
        <span className="titlebar-label">
          <span className="titlebar-label-primary">0xoLemon</span>
          <span className="titlebar-label-secondary">Launcher</span>
        </span>
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
            className="titlebar-status-icon"
            data-hint={statusPreferences.hoverHints ? (
              networkQuality === 'good' ? 'Network: Good' :
              networkQuality === 'weak' ? 'Network: Weak signal' :
              'Network: Offline'
            ) : undefined}
          >
            <WifiIcon quality={networkQuality} />
          </span>
        ) : null}
        <BatteryIcon battery={battery} />
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
