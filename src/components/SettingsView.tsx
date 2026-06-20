import type { ReactNode } from 'react'
import {
  Bell,
  Clock3,
  Cloud,
  Download,
  FolderOpen,
  Gamepad2,
  Gauge,
  HardDrive,
  Info,
  MonitorCog,
  PanelTop,
  RefreshCcw,
  RotateCcw,
  Settings,
  Sparkles,
} from 'lucide-react'
import type { LauncherPreferences, NotificationCategory } from '../lib/preferences'
import type { LauncherSettings, SteamEnvironmentInfo } from '../types'

function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
}) {
  return (
    <button
      type="button"
      className={checked ? 'settings-toggle is-on' : 'settings-toggle'}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  )
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-copy">
        <strong>{title}</strong>
        <span>{description}</span>
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  )
}

export function SettingsView({
  preferences,
  launcherSettings,
  onChange,
  onLauncherSettingChange,
  onChooseLibrary,
  onOpenLibrary,
  onOpenCache,
  onChooseCloudRoot,
  onOpenCloudRoot,
  onCheckForUpdates,
  steamEnvironment,
  steamStatus,
  onRefreshSteam,
  onOpenSteam,
  onRestartSteam,
  onOpenBigPicture,
  onReset,
  onResetOnboarding,
  onManageNotifications,
  appVersion,
  updateStatus,
}: {
  preferences: LauncherPreferences
  launcherSettings: LauncherSettings
  onChange: <K extends keyof LauncherPreferences>(key: K, value: LauncherPreferences[K]) => void
  onLauncherSettingChange: <K extends keyof LauncherSettings>(key: K, value: LauncherSettings[K]) => void
  onChooseLibrary: () => void
  onOpenLibrary: () => void
  onOpenCache: () => void
  onChooseCloudRoot: () => void
  onOpenCloudRoot: () => void
  onCheckForUpdates: () => void
  steamEnvironment: SteamEnvironmentInfo | null
  steamStatus: string | null
  onRefreshSteam: () => void
  onOpenSteam: () => void
  onRestartSteam: () => void
  onOpenBigPicture: () => void
  onReset: () => void
  onResetOnboarding: () => void
  onManageNotifications: () => void
  appVersion: string
  updateStatus: string | null
}) {
  return (
    <section className="settings-view settings-view-global">
      <header className="settings-page-header">
        <div>
          <span className="settings-page-icon">
            <Settings size={21} />
          </span>
          <div>
            <h1>Settings</h1>
            <p>Launcher-wide preferences. Game-specific actions stay in Library.</p>
          </div>
        </div>
        <button type="button" className="settings-reset" onClick={onReset}>
          <RotateCcw size={15} />
          Restore defaults
        </button>
      </header>

      <div className="settings-sections">
        <section className="settings-group">
          <header>
            <MonitorCog size={18} />
            <div>
              <strong>General</strong>
              <span>Startup and window behavior</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title="Open launcher on" description="Choose the page shown when the launcher starts.">
              <select
                className="settings-select"
                value={preferences.startupPage}
                onChange={(event) => onChange('startupPage', event.target.value as LauncherPreferences['startupPage'])}
              >
                <option value="Home">Home</option>
                <option value="Store">Store</option>
                <option value="Library">Library</option>
                <option value="Updates">Updates</option>
                <option value="Downloads">Downloads</option>
                <option value="Cloud Saves">Cloud Saves</option>
              </select>
            </SettingRow>
            <SettingRow title="Close button" description="Choose what the title-bar close button does.">
              <select
                className="settings-select"
                value={preferences.closeBehavior}
                onChange={(event) => onChange('closeBehavior', event.target.value as LauncherPreferences['closeBehavior'])}
              >
                <option value="exit">Exit launcher</option>
                <option value="minimize">Minimize to taskbar</option>
              </select>
            </SettingRow>
            <SettingRow title="Confirm before uninstall" description="Show a confirmation dialog before deleting a game.">
              <Toggle
                checked={preferences.confirmBeforeUninstall}
                onChange={(checked) => onChange('confirmBeforeUninstall', checked)}
                label="Confirm before uninstall"
              />
            </SettingRow>
            <SettingRow title="Confirm before cancel cleanup" description="Ask before canceling a job and deleting its temporary downloaded data.">
              <Toggle
                checked={preferences.confirmBeforeCancelCleanup}
                onChange={(checked) => onChange('confirmBeforeCancelCleanup', checked)}
                label="Confirm before cancel cleanup"
              />
            </SettingRow>
            <SettingRow title="Confirm before clearing cache" description="Ask before removing reusable downloaded chunks.">
              <Toggle
                checked={preferences.confirmBeforeClearCache}
                onChange={(checked) => onChange('confirmBeforeClearCache', checked)}
                label="Confirm before clearing cache"
              />
            </SettingRow>
            <SettingRow title="Confirm cloud restore" description="Ask before restoring a snapshot over local save files.">
              <Toggle
                checked={preferences.confirmBeforeCloudRestore}
                onChange={(checked) => onChange('confirmBeforeCloudRestore', checked)}
                label="Confirm before cloud restore"
              />
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <PanelTop size={18} />
            <div>
              <strong>Home & layout</strong>
              <span>Choose which dashboard surfaces are visible</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showContinuePlaying', 'Continue Playing', 'Show the large recent-game hero and quick Play action.'],
              ['showRecentGames', 'Recent games', 'Show the installed-game carousel on Home.'],
              ['showActiveTasks', 'Active tasks', 'Show download, update and launcher update progress on Home.'],
              ['showDiscordCard', 'Discord community', 'Show the official community invitation card.'],
              ['showDonateCard', 'Support development', 'Show the compact donate card and QR modal.'],
              ['carouselAutoplay', 'Carousel autoplay', 'Rotate featured installed games every eight seconds.'],
            ] as const).map(([key, title, description]) => (
              <SettingRow key={key} title={title} description={description}>
                <Toggle checked={preferences[key]} onChange={(checked) => onChange(key, checked)} label={title} />
              </SettingRow>
            ))}
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Cloud size={18} />
            <div>
              <strong>Cloud saves</strong>
              <span>Folder-based sync for OneDrive, Google Drive Desktop, NAS or mapped drives</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title="Cloud Save root"
              description={
                launcherSettings.cloudSaveRoot
                  ? 'Each game is isolated under 0xoLemon Cloud Saves inside this folder.'
                  : 'Choose a locally synchronized folder before enabling cloud saves for a game.'
              }
            >
              <div className="settings-path-control">
                <span title={launcherSettings.cloudSaveRoot}>
                  {launcherSettings.cloudSaveRoot || 'Not configured'}
                </span>
                <button
                  type="button"
                  onClick={onOpenCloudRoot}
                  title="Open cloud save folder"
                  disabled={!launcherSettings.cloudSaveRoot}
                >
                  <FolderOpen size={15} />
                </button>
                <button type="button" onClick={onChooseCloudRoot}>Change</button>
              </div>
            </SettingRow>
            <SettingRow
              title="Provider"
              description="Folder sync remains available, while Google Drive backup uses the private app-data area of the signed-in account."
            >
              <div className="settings-static-value">
                <Cloud size={14} />
                Folder + Google Drive (ready)
              </div>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Gamepad2 size={18} />
            <div>
              <strong>Steam integration</strong>
              <span>Client discovery, libraries, shortcuts and controller-friendly launch</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title="Steam client"
              description={steamStatus ?? steamEnvironment?.rootPath ?? 'Detecting the local Steam installation.'}
            >
              <div className={`settings-status-pill ${steamEnvironment?.running ? 'is-online' : ''}`}>
                {steamEnvironment
                  ? steamEnvironment.installed
                    ? steamEnvironment.running
                      ? 'Installed · Running'
                      : 'Installed · Stopped'
                    : 'Not detected'
                  : 'Checking...'}
              </div>
            </SettingRow>
            <SettingRow
              title="Steam libraries"
              description={
                steamEnvironment?.libraryPaths.length
                  ? steamEnvironment.libraryPaths.join(' · ')
                  : 'Launcher reads Steam libraryfolders.vdf without moving or changing game data.'
              }
            >
              <div className="settings-static-value">
                <HardDrive size={14} />
                {steamEnvironment?.libraryPaths.length ?? 0} detected
              </div>
            </SettingRow>
            <SettingRow
              title="Active Steam profile"
              description={
                steamEnvironment?.activeAccountId
                  ? `Account ${steamEnvironment.activeAccountId} · UI language ${steamEnvironment.uiLanguage ?? 'unknown'}`
                  : 'Sign in to Steam once to enable automatic non-Steam shortcut management.'
              }
            >
              <div className="settings-static-value">
                {steamEnvironment?.pendingShortcutActions ?? 0} shortcut actions queued
              </div>
            </SettingRow>
            <SettingRow
              title="Steam interface"
              description="Open the desktop client or Big Picture for controller-first navigation."
            >
              <div className="settings-action-row">
                <button type="button" className="settings-secondary-button" onClick={onRefreshSteam}>
                  <RefreshCcw size={15} />
                  Refresh
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenSteam}>
                  <MonitorCog size={15} />
                  Open Steam
                </button>
                <button type="button" className="settings-secondary-button" onClick={onRestartSteam}>
                  <RotateCcw size={15} />
                  Restart Steam
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenBigPicture}>
                  <Gamepad2 size={15} />
                  Big Picture
                </button>
              </div>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <HardDrive size={18} />
            <div>
              <strong>Downloads & storage</strong>
              <span>Default library and job behavior</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title="Default game library" description="New games are installed in the common folder under this location.">
              <div className="settings-path-control">
                <span title={preferences.defaultLibraryRoot}>{preferences.defaultLibraryRoot}</span>
                <button type="button" onClick={onOpenLibrary} title="Open folder">
                  <FolderOpen size={15} />
                </button>
                <button type="button" onClick={onChooseLibrary}>Change</button>
              </div>
            </SettingRow>
            <SettingRow title="Open Downloads when a job starts" description="Switch to the queue after starting an install or update.">
              <Toggle
                checked={preferences.openDownloadsOnJobStart}
                onChange={(checked) => onChange('openDownloadsOnJobStart', checked)}
                label="Open Downloads when a job starts"
              />
            </SettingRow>
            <SettingRow
              title="Game update mode"
              description="Automatic updates run when the launcher is idle; manual mode only updates when you request it."
            >
              <select
                className="settings-select"
                value={launcherSettings.gameUpdateMode}
                onChange={(event) =>
                  onLauncherSettingChange(
                    'gameUpdateMode',
                    event.target.value as LauncherSettings['gameUpdateMode'],
                  )
                }
              >
                <option value="automatic">Automatic</option>
                <option value="scheduled">Scheduled window</option>
                <option value="manual">Manual only</option>
              </select>
            </SettingRow>
            {launcherSettings.gameUpdateMode === 'scheduled' ? (
              <SettingRow
                title="Update window"
                description="Updates may start daily inside this local-time window. Overnight windows are supported."
              >
                <div className="settings-time-range">
                  <input
                    type="time"
                    value={launcherSettings.gameUpdateScheduleStart}
                    onChange={(event) =>
                      onLauncherSettingChange('gameUpdateScheduleStart', event.target.value)
                    }
                  />
                  <span>to</span>
                  <input
                    type="time"
                    value={launcherSettings.gameUpdateScheduleEnd}
                    onChange={(event) =>
                      onLauncherSettingChange('gameUpdateScheduleEnd', event.target.value)
                    }
                  />
                </div>
              </SettingRow>
            ) : null}
            <SettingRow
              title="Download profile"
              description={`${launcherSettings.downloadWorkers} workers · ${launcherSettings.downloadQueueMb} MiB memory budget`}
            >
              <select
                className="settings-select"
                value={launcherSettings.downloadProfile}
                onChange={(event) =>
                  onLauncherSettingChange(
                    'downloadProfile',
                    event.target.value as LauncherSettings['downloadProfile'],
                  )
                }
              >
                <option value="eco">Eco</option>
                <option value="balanced">Balanced</option>
                <option value="turbo">Turbo</option>
              </select>
            </SettingRow>
            <SettingRow
              title="Downloader V2 preview"
              description="Write verified chunks directly into resumable staging files. Turn this off to use the V1 chunk-cache fallback."
            >
              <Toggle
                checked={launcherSettings.directToStaging}
                onChange={(checked) => onLauncherSettingChange('directToStaging', checked)}
                label="Downloader V2 preview"
              />
            </SettingRow>
            <SettingRow title="Pause downloads before launching a game" description="Pause the active job before starting a game to reduce disk and network contention.">
              <Toggle
                checked={preferences.pauseDownloadsBeforeLaunch}
                onChange={(checked) => onChange('pauseDownloadsBeforeLaunch', checked)}
                label="Pause downloads before launching a game"
              />
            </SettingRow>
            <SettingRow title="Chunk cache" description="Inspect cached chunks, health, free space and rollback readiness.">
              <button type="button" className="settings-secondary-button" onClick={onOpenCache}>
                <Gauge size={15} />
                Manage cache
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Sparkles size={18} />
            <div>
              <strong>Appearance</strong>
              <span>Interface comfort and motion</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title="Motion" description="Use full motion, follow Windows, or disable non-essential movement.">
              <select
                className="settings-select"
                value={preferences.motionMode}
                onChange={(event) => onChange('motionMode', event.target.value as LauncherPreferences['motionMode'])}
              >
                <option value="full">Full</option>
                <option value="system">Follow Windows</option>
                <option value="reduced">Reduced</option>
              </select>
            </SettingRow>
            <SettingRow title="Glass effects" description="Use acrylic blur for temporary panels and popovers.">
              <Toggle checked={preferences.glassEffects} onChange={(value) => onChange('glassEffects', value)} label="Glass effects" />
            </SettingRow>
            <SettingRow title="Scroll effects" description="Reveal dashboard sections with a restrained depth transition.">
              <Toggle checked={preferences.scrollEffects} onChange={(value) => onChange('scrollEffects', value)} label="Scroll effects" />
            </SettingRow>
            <SettingRow title="Hover hints" description="Show delayed explanations for compact titlebar and toolbar controls.">
              <Toggle checked={preferences.hoverHints} onChange={(value) => onChange('hoverHints', value)} label="Hover hints" />
            </SettingRow>
            <SettingRow title="Installation complete sound" description="Play a short notification after a new game installation commits successfully.">
              <Toggle
                checked={preferences.playInstallCompleteSound}
                onChange={(checked) => onChange('playInstallCompleteSound', checked)}
                label="Play installation complete sound"
              />
            </SettingRow>
            <SettingRow title="Onboarding" description="Replay the launcher introduction.">
              <button type="button" className="settings-secondary-button" onClick={onResetOnboarding}>
                <RefreshCcw size={15} /> Replay introduction
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Clock3 size={18} />
            <div>
              <strong>Status bar</strong>
              <span>Clock and live launcher status in the titlebar</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showClock', 'Clock', 'Show local time in the titlebar.'],
              ['showDate', 'Date', 'Show the date beneath the clock.'],
              ['showNetworkStatus', 'Network status', 'Show the real content-service connection state.'],
              ['showDownloadIndicator', 'Download indicator', 'Show active job or launcher update progress.'],
              ['showNotificationBell', 'Notification bell', 'Show unread status and notification history.'],
            ] as const).map(([key, title, description]) => (
              <SettingRow key={key} title={title} description={description}>
                <Toggle checked={preferences[key]} onChange={(value) => onChange(key, value)} label={title} />
              </SettingRow>
            ))}
            <SettingRow title="Clock format" description="Use Windows preference or force a 12/24-hour clock.">
              <select
                className="settings-select"
                value={preferences.clockFormat}
                onChange={(event) => onChange('clockFormat', event.target.value as LauncherPreferences['clockFormat'])}
              >
                <option value="system">System</option>
                <option value="12h">12-hour</option>
                <option value="24h">24-hour</option>
              </select>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group" id="notification-settings">
          <header>
            <Bell size={18} />
            <div>
              <strong>Notifications</strong>
              <span>Real event history, in-app toasts and Windows notifications</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title="In-app notifications" description="Show a toast while the launcher is visible and keep history under the bell.">
              <Toggle checked={preferences.inAppNotifications} onChange={(value) => onChange('inAppNotifications', value)} label="In-app notifications" />
            </SettingRow>
            <SettingRow title="Windows notifications" description="Use Windows Notification Center when the launcher is minimized or unfocused.">
              <Toggle checked={preferences.windowsNotifications} onChange={(value) => onChange('windowsNotifications', value)} label="Windows notifications" />
            </SettingRow>
            <SettingRow title="Notification sound" description="Allow native and in-app notification sounds where supported.">
              <Toggle checked={preferences.notificationSound} onChange={(value) => onChange('notificationSound', value)} label="Notification sound" />
            </SettingRow>
            <SettingRow title="Do not disturb while playing" description="Keep history but suppress popups while a game is running.">
              <Toggle checked={preferences.doNotDisturbWhilePlaying} onChange={(value) => onChange('doNotDisturbWhilePlaying', value)} label="Do not disturb while playing" />
            </SettingRow>
            {([
              ['launcher', 'Launcher updates'],
              ['installs', 'Install, update and repair'],
              ['downloads', 'Downloads and cleanup'],
              ['cloudSaves', 'Cloud saves'],
              ['storage', 'Storage and cache'],
              ['achievements', 'Achievements'],
              ['errors', 'Important errors'],
            ] as Array<[NotificationCategory, string]>).map(([category, label]) => (
              <SettingRow key={category} title={label} description={`Allow ${label.toLowerCase()} events in notification history.`}>
                <Toggle
                  checked={preferences.notificationCategories[category]}
                  onChange={(value) =>
                    onChange('notificationCategories', {
                      ...preferences.notificationCategories,
                      [category]: value,
                    })
                  }
                  label={label}
                />
              </SettingRow>
            ))}
            <SettingRow title="Notification history" description="Open the titlebar notification center and review recorded events.">
              <button type="button" className="settings-secondary-button" onClick={onManageNotifications}>
                <Bell size={15} /> Manage history
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <RefreshCcw size={18} />
            <div>
              <strong>Launcher updates</strong>
              <span>Keep the launcher client current</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title="Automatically check for launcher updates" description="Check shortly after startup and show an update banner when available.">
              <Toggle
                checked={preferences.autoCheckLauncherUpdates}
                onChange={(checked) => onChange('autoCheckLauncherUpdates', checked)}
                label="Automatically check for launcher updates"
              />
            </SettingRow>
            <SettingRow title="Update channel" description="Receive stable launcher releases.">
              <div className="settings-static-value">Stable</div>
            </SettingRow>
            <SettingRow title="Check now" description={updateStatus ?? 'Manually check the configured launcher update source.'}>
              <button type="button" className="settings-secondary-button" onClick={onCheckForUpdates}>
                <RefreshCcw size={15} />
                Check for updates
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-about-card">
          <Info size={18} />
          <div>
            <strong>0xoLemon Launcher</strong>
            <span>Version {appVersion} · Multi-game content, install, update and repair client</span>
          </div>
          <Download size={17} />
        </section>
      </div>
    </section>
  )
}
