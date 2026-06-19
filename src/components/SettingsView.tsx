import type { ReactNode } from 'react'
import {
  Bell,
  Cloud,
  Download,
  FolderOpen,
  Gamepad2,
  Gauge,
  HardDrive,
  Info,
  MonitorCog,
  RefreshCcw,
  RotateCcw,
  Settings,
  Sparkles,
} from 'lucide-react'
import type { LauncherPreferences } from '../lib/preferences'
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
  onOpenBigPicture,
  onReset,
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
  onOpenBigPicture: () => void
  onReset: () => void
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
                <option value="Store">Store</option>
                <option value="Library">Library</option>
                <option value="Updates">Updates</option>
                <option value="Downloads">Downloads</option>
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
            <SettingRow title="Reduce motion" description="Disable non-essential interface transitions and animated effects.">
              <Toggle
                checked={preferences.reduceMotion}
                onChange={(checked) => onChange('reduceMotion', checked)}
                label="Reduce motion"
              />
            </SettingRow>
            <SettingRow title="Theme" description="The launcher currently uses its native dark theme.">
              <div className="settings-static-value">Dark</div>
            </SettingRow>
            <SettingRow title="Notifications" description="Download and update status remains visible in the sidebar and queue.">
              <div className="settings-static-value">
                <Bell size={14} /> In-app
              </div>
            </SettingRow>
            <SettingRow title="Installation complete sound" description="Play a short notification after a new game installation commits successfully.">
              <Toggle
                checked={preferences.playInstallCompleteSound}
                onChange={(checked) => onChange('playInstallCompleteSound', checked)}
                label="Play installation complete sound"
              />
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
            <span>Version 0.1.1 · Multi-game content, install, update and repair client</span>
          </div>
          <Download size={17} />
        </section>
      </div>
    </section>
  )
}
