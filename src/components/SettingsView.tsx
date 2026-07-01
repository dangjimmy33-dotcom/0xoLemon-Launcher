import type { ReactNode } from 'react'
import { useState, useEffect, useRef } from 'react'
import { useLocale, type Locale } from '../context/LocaleContext'
import { ChevronDown, Bell,
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
  CircleAlert,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type { LauncherPreferences, NotificationCategory } from '../lib/preferences'
import type { LauncherSettings, SteamEnvironmentInfo } from '../types'

function CustomSelect<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (value: T) => void
  options: { value: T; label: string }[]
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const selectedLabel = options.find((o) => o.value === value)?.label ?? value

  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  return (
    <div className={`cs-wrap${open ? ' is-open' : ''}`} ref={ref}>
      <button type="button" className="cs-trigger" onClick={() => setOpen((v) => !v)}>
        <span>{selectedLabel}</span>
        <ChevronDown size={14} className="cs-chevron" />
      </button>
      <div className="cs-dropdown">
        <div className="cs-list">
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              className={`cs-option${opt.value === value ? ' is-selected' : ''}`}
              onClick={() => { onChange(opt.value); setOpen(false) }}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

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
  const { locale, setLocale, t } = useLocale()
  return (
    <section className="settings-view settings-view-global">
      <header className="settings-page-header">
        <div>
          <span className="settings-page-icon">
            <Settings size={21} />
          </span>
          <div>
            <h1>{t.settings.title}</h1>
            <p>{t.settings.subtitle}</p>
          </div>
        </div>
        <button type="button" className="settings-reset" onClick={onReset}>
          <RotateCcw size={15} />
          {t.settings.restoreDefaults}
        </button>
      </header>

      <div className="settings-sections">
        <section className="settings-group">
          <header>
            <MonitorCog size={18} />
            <div>
              <strong>{t.settings.general}</strong>
              <span>{t.settings.generalDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.openLauncherOn} description={t.settings.openLauncherOnDesc}>
              <CustomSelect
                value={preferences.startupPage}
                onChange={(v) => onChange('startupPage', v)}
                options={[
                  { value: 'Home', label: t.nav.home },
                  { value: 'Store', label: t.nav.store },
                  { value: 'Library', label: t.nav.library },
                  { value: 'Updates', label: t.nav.updates },
                  { value: 'Downloads', label: t.nav.downloads },
                  { value: 'Cloud Saves', label: t.nav.cloudSaves },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.closeButton} description={t.settings.closeButtonDesc}>
              <CustomSelect
                value={preferences.closeBehavior}
                onChange={(v) => onChange('closeBehavior', v)}
                options={[
                  { value: 'exit', label: t.settings.closeExit },
                  { value: 'minimize', label: t.settings.closeMinimize },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmUninstall} description={t.settings.confirmUninstallDesc}>
              <Toggle
                checked={preferences.confirmBeforeUninstall}
                onChange={(checked) => onChange('confirmBeforeUninstall', checked)}
                label={t.settings.confirmUninstall}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmCancelCleanup} description={t.settings.confirmCancelCleanupDesc}>
              <Toggle
                checked={preferences.confirmBeforeCancelCleanup}
                onChange={(checked) => onChange('confirmBeforeCancelCleanup', checked)}
                label={t.settings.confirmCancelCleanup}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmClearCache} description={t.settings.confirmClearCacheDesc}>
              <Toggle
                checked={preferences.confirmBeforeClearCache}
                onChange={(checked) => onChange('confirmBeforeClearCache', checked)}
                label={t.settings.confirmClearCache}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmCloudRestore} description={t.settings.confirmCloudRestoreDesc}>
              <Toggle
                checked={preferences.confirmBeforeCloudRestore}
                onChange={(checked) => onChange('confirmBeforeCloudRestore', checked)}
                label={t.settings.confirmCloudRestore}
              />
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <PanelTop size={18} />
            <div>
              <strong>{t.settings.homeLayout}</strong>
              <span>{t.settings.homeLayoutDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showContinuePlaying', t.settings.showContinuePlaying, t.settings.showContinuePlayingDesc],
              ['showRecentGames', t.settings.showRecentGames, t.settings.showRecentGamesDesc],
              ['showActiveTasks', t.settings.showActiveTasks, t.settings.showActiveTasksDesc],
              ['showDiscordCard', t.settings.showDiscordCard, t.settings.showDiscordCardDesc],
              ['showDonateCard', t.settings.showDonateCard, t.settings.showDonateCardDesc],
              ['carouselAutoplay', t.settings.carouselAutoplay, t.settings.carouselAutoplayDesc],
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
              <strong>{t.settings.cloudSaves}</strong>
              <span>{t.settings.cloudSavesDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title={t.settings.cloudSaveRoot}
              description={
                launcherSettings.cloudSaveRoot
                  ? t.settings.cloudSaveRootConfigured
                  : t.settings.cloudSaveRootEmpty
              }
            >
              <div className="settings-path-control">
                <span title={launcherSettings.cloudSaveRoot}>
                  {launcherSettings.cloudSaveRoot || t.settings.cloudSaveNotConfigured}
                </span>
                <button
                  type="button"
                  onClick={onOpenCloudRoot}
                  title="Open cloud save folder"
                  disabled={!launcherSettings.cloudSaveRoot}
                >
                  <FolderOpen size={15} />
                </button>
                <button type="button" onClick={onChooseCloudRoot}>{t.settings.change}</button>
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.cloudSaveProvider}
              description={t.settings.cloudSaveProviderDesc}
            >
              <div className="settings-static-value">
                <Cloud size={14} />
                {t.settings.cloudSaveProviderValue}
              </div>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Gamepad2 size={18} />
            <div>
              <strong>{t.settings.steamIntegration}</strong>
              <span>{t.settings.steamIntegrationDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title={t.settings.steamClient}
              description={steamStatus ?? steamEnvironment?.rootPath ?? t.settings.steamClientDefault}
            >
              <div className={`settings-status-pill ${steamEnvironment?.running ? 'is-online' : ''}`}>
                {steamEnvironment
                  ? steamEnvironment.installed
                    ? steamEnvironment.running
                      ? `${t.settings.steamInstalled} · ${t.settings.steamRunning}`
                      : `${t.settings.steamInstalled} · ${t.settings.steamStopped}`
                    : t.settings.steamNotDetected
                  : t.settings.steamChecking}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamLibraries}
              description={
                steamEnvironment?.libraryPaths.length
                  ? steamEnvironment.libraryPaths.join(' · ')
                  : t.settings.steamLibrariesDesc
              }
            >
              <div className="settings-static-value">
                <HardDrive size={14} />
                {steamEnvironment?.libraryPaths.length ?? 0} {t.settings.steamLibrariesDetected}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamProfile}
              description={
                steamEnvironment?.activeAccountId
                  ? `Account ${steamEnvironment.activeAccountId} · UI language ${steamEnvironment.uiLanguage ?? 'unknown'}`
                  : t.settings.steamProfileDesc
              }
            >
              <div className="settings-static-value">
                {steamEnvironment?.pendingShortcutActions ?? 0} {t.settings.steamShortcutsQueued}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamInterface}
              description={t.settings.steamInterfaceDesc}
            >
              <div className="settings-action-row">
                <button type="button" className="settings-secondary-button" onClick={onRefreshSteam}>
                  <RefreshCcw size={15} />
                  {t.settings.refresh}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenSteam}>
                  <MonitorCog size={15} />
                  {t.settings.openSteam}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onRestartSteam}>
                  <RotateCcw size={15} />
                  {t.settings.restartSteam}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenBigPicture}>
                  <Gamepad2 size={15} />
                  {t.settings.bigPicture}
                </button>
              </div>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <HardDrive size={18} />
            <div>
              <strong>{t.settings.downloadsStorage}</strong>
              <span>{t.settings.downloadsStorageDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.defaultLibrary} description={t.settings.defaultLibraryDesc}>
              <div className="settings-path-control">
                <span title={preferences.defaultLibraryRoot}>{preferences.defaultLibraryRoot}</span>
                <button type="button" onClick={onOpenLibrary} title="Open folder">
                  <FolderOpen size={15} />
                </button>
                <button type="button" onClick={onChooseLibrary}>{t.settings.change}</button>
              </div>
            </SettingRow>
            <SettingRow title={t.settings.openDownloadsOnStart} description={t.settings.openDownloadsOnStartDesc}>
              <Toggle
                checked={preferences.openDownloadsOnJobStart}
                onChange={(checked) => onChange('openDownloadsOnJobStart', checked)}
                label={t.settings.openDownloadsOnStart}
              />
            </SettingRow>
            <SettingRow title={t.settings.gameUpdateMode} description={t.settings.gameUpdateModeDesc}>
              <CustomSelect
                value={launcherSettings.gameUpdateMode}
                onChange={(v) => onLauncherSettingChange('gameUpdateMode', v)}
                options={[
                  { value: 'automatic', label: t.settings.updateAutomatic },
                  { value: 'scheduled', label: t.settings.updateScheduled },
                  { value: 'manual', label: t.settings.updateManual },
                ]}
              />
            </SettingRow>
            {launcherSettings.gameUpdateMode === 'scheduled' ? (
              <SettingRow title={t.settings.updateWindow} description={t.settings.updateWindowDesc}>
                <div className="settings-time-range">
                  <input
                    type="time"
                    value={launcherSettings.gameUpdateScheduleStart}
                    onChange={(event) =>
                      onLauncherSettingChange('gameUpdateScheduleStart', event.target.value)
                    }
                  />
                  <span>{t.settings.updateWindowTo}</span>
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
              title={t.settings.downloadProfile}
              description={`${launcherSettings.downloadWorkers} workers · ${launcherSettings.downloadQueueMb} MiB memory budget`}
            >
              <CustomSelect
                value={launcherSettings.downloadProfile}
                onChange={(v) => onLauncherSettingChange('downloadProfile', v)}
                options={[
                  { value: 'eco', label: 'Eco' },
                  { value: 'balanced', label: 'Balanced' },
                  { value: 'turbo', label: 'Turbo' },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.downloaderV2} description={t.settings.downloaderV2Desc}>
              <Toggle
                checked={launcherSettings.directToStaging}
                onChange={(checked) => onLauncherSettingChange('directToStaging', checked)}
                label={t.settings.downloaderV2}
              />
            </SettingRow>
            <SettingRow title={t.settings.pauseBeforeLaunch} description={t.settings.pauseBeforeLaunchDesc}>
              <Toggle
                checked={preferences.pauseDownloadsBeforeLaunch}
                onChange={(checked) => onChange('pauseDownloadsBeforeLaunch', checked)}
                label={t.settings.pauseBeforeLaunch}
              />
            </SettingRow>
            <SettingRow title={t.settings.chunkCache} description={t.settings.chunkCacheDesc}>
              <button type="button" className="settings-secondary-button" onClick={onOpenCache}>
                <Gauge size={15} />
                {t.settings.manageCache}
              </button>
            </SettingRow>
            <SettingRow title={t.settings.resetAppData} description={t.settings.resetAppDataDesc}>
              <button
                type="button"
                style={{ background: '#e02424', color: 'white', border: 'none', padding: '8px 16px', borderRadius: '6px', fontWeight: 'bold', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: '8px' }}
                onClick={() => {
                  if (confirm('Are you sure you want to clear all app data and restart?')) {
                    localStorage.clear();
                    sessionStorage.clear();
                    window.location.reload();
                  }
                }}
              >
                <CircleAlert size={16} />
                {t.settings.clearCacheRestart}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Sparkles size={18} />
            <div>
              <strong>{t.settings.language}</strong>
              <span>{t.settings.languageDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.displayLanguage} description={t.settings.displayLanguageDesc}>
              <select
                className="settings-select"
                value={locale}
                onChange={(e) => setLocale(e.target.value as Locale)}
              >
                <option value="en-US">English</option>
                <option value="vi-VN">Tiếng Việt</option>
              </select>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Sparkles size={18} />
            <div>
              <strong>{t.settings.appearance}</strong>
              <span>{t.settings.appearanceDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.languageLabel} description={t.settings.languageLabelDesc}>
              <CustomSelect
                value={locale}
                onChange={(v) => setLocale(v as Locale)}
                options={[
                  { value: 'en-US', label: 'English' },
                  { value: 'vi-VN', label: 'Tiếng Việt' },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.motion} description={t.settings.motionDesc}>
              <CustomSelect
                value={preferences.motionMode}
                onChange={(v) => onChange('motionMode', v)}
                options={[
                  { value: 'full', label: t.settings.motionFull },
                  { value: 'system', label: t.settings.motionSystem },
                  { value: 'reduced', label: t.settings.motionReduced },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.glassEffects} description={t.settings.glassEffectsDesc}>
              <Toggle checked={preferences.glassEffects} onChange={(value) => onChange('glassEffects', value)} label={t.settings.glassEffects} />
            </SettingRow>
            <SettingRow title={t.settings.scrollEffects} description={t.settings.scrollEffectsDesc}>
              <Toggle checked={preferences.scrollEffects} onChange={(value) => onChange('scrollEffects', value)} label={t.settings.scrollEffects} />
            </SettingRow>
            <SettingRow title={t.settings.hoverHints} description={t.settings.hoverHintsDesc}>
              <Toggle checked={preferences.hoverHints} onChange={(value) => onChange('hoverHints', value)} label={t.settings.hoverHints} />
            </SettingRow>
            <SettingRow title={t.settings.installSound} description={t.settings.installSoundDesc}>
              <Toggle
                checked={preferences.playInstallCompleteSound}
                onChange={(checked) => onChange('playInstallCompleteSound', checked)}
                label={t.settings.installSound}
              />
            </SettingRow>
            <SettingRow title={t.settings.onboarding} description={t.settings.onboardingDesc}>
              <button type="button" className="settings-secondary-button" onClick={onResetOnboarding}>
                <RefreshCcw size={15} /> {t.settings.replayIntro}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Clock3 size={18} />
            <div>
              <strong>{t.settings.statusBar}</strong>
              <span>{t.settings.statusBarDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showClock', t.settings.clock, t.settings.clockDesc],
              ['showDate', t.settings.date, t.settings.dateDesc],
              ['showNetworkStatus', t.settings.networkStatus, t.settings.networkStatusDesc],
              ['showDownloadIndicator', t.settings.downloadIndicator, t.settings.downloadIndicatorDesc],
              ['showNotificationBell', t.settings.notificationBell, t.settings.notificationBellDesc],
            ] as const).map(([key, title, description]) => (
              <SettingRow key={key} title={title} description={description}>
                <Toggle checked={preferences[key]} onChange={(value) => onChange(key, value)} label={title} />
              </SettingRow>
            ))}
            <SettingRow title={t.settings.clockFormat} description={t.settings.clockFormatDesc}>
              <CustomSelect
                value={preferences.clockFormat}
                onChange={(v) => onChange('clockFormat', v)}
                options={[
                  { value: 'system', label: t.settings.clockSystem },
                  { value: '12h', label: t.settings.clock12h },
                  { value: '24h', label: t.settings.clock24h },
                ]}
              />
            </SettingRow>
          </div>
        </section>

        <section className="settings-group" id="notification-settings">
          <header>
            <Bell size={18} />
            <div>
              <strong>{t.settings.notifications}</strong>
              <span>{t.settings.notificationsDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.inAppNotifications} description={t.settings.inAppNotificationsDesc}>
              <Toggle checked={preferences.inAppNotifications} onChange={(value) => onChange('inAppNotifications', value)} label={t.settings.inAppNotifications} />
            </SettingRow>
            <SettingRow title={t.settings.windowsNotifications} description={t.settings.windowsNotificationsDesc}>
              <Toggle checked={preferences.windowsNotifications} onChange={(value) => onChange('windowsNotifications', value)} label={t.settings.windowsNotifications} />
            </SettingRow>
            <SettingRow title={t.settings.notificationSound} description={t.settings.notificationSoundDesc}>
              <Toggle checked={preferences.notificationSound} onChange={(value) => onChange('notificationSound', value)} label={t.settings.notificationSound} />
            </SettingRow>
            <SettingRow title={t.settings.doNotDisturb} description={t.settings.doNotDisturbDesc}>
              <Toggle checked={preferences.doNotDisturbWhilePlaying} onChange={(value) => onChange('doNotDisturbWhilePlaying', value)} label={t.settings.doNotDisturb} />
            </SettingRow>
            {([
              ['launcher', t.settings.notifCatLauncher],
              ['installs', t.settings.notifCatInstalls],
              ['downloads', t.settings.notifCatDownloads],
              ['cloudSaves', t.settings.notifCatCloudSaves],
              ['storage', t.settings.notifCatStorage],
              ['achievements', t.settings.notifCatAchievements],
              ['errors', t.settings.notifCatErrors],
            ] as Array<[NotificationCategory, string]>).map(([category, label]) => (
              <SettingRow key={category} title={label} description={t.settings.notifCatAllow.replace('{label}', label.toLowerCase())}>
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
            <SettingRow title={t.settings.manageHistory} description={t.settings.manageHistoryDesc}>
              <button type="button" className="settings-secondary-button" onClick={onManageNotifications}>
                <Bell size={15} /> {t.settings.manageHistoryBtn}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <RefreshCcw size={18} />
            <div>
              <strong>{t.settings.launcherUpdates}</strong>
              <span>{t.settings.launcherUpdatesDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.autoCheckUpdates} description={t.settings.autoCheckUpdatesDesc}>
              <Toggle
                checked={preferences.autoCheckLauncherUpdates}
                onChange={(checked) => onChange('autoCheckLauncherUpdates', checked)}
                label={t.settings.autoCheckUpdates}
              />
            </SettingRow>
            <SettingRow title={t.settings.updateChannel} description={t.settings.updateChannelDesc}>
              <div className="settings-static-value">{t.settings.updateChannelValue}</div>
            </SettingRow>
            <SettingRow title={t.settings.checkNow} description={updateStatus ?? t.settings.checkNowDefault}>
              <button type="button" className="settings-secondary-button" onClick={onCheckForUpdates}>
                <RefreshCcw size={15} />
                {t.settings.checkForUpdates}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <CircleAlert size={18} style={{ color: '#ef4444' }} />
            <div>
              <strong style={{ color: '#ef4444' }}>{t.settings.dangerZone}</strong>
              <span>{t.settings.dangerZoneDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.resetLauncherData} description={t.settings.resetLauncherDataDesc}>
              <button 
                type="button" 
                className="settings-secondary-button" 
                style={{ borderColor: '#ef4444', color: '#ef4444' }}
                onClick={async () => {
                  if (confirm("Bạn có chắc chắn muốn xóa toàn bộ dữ liệu cấu hình và đăng nhập (Google Drive, Discord...) của Launcher không? Việc này không thể hoàn tác.")) {
                    try {
                      await invoke('clear_launcher_config')
                      localStorage.clear()
                      alert("Đã xóa toàn bộ cấu hình! Launcher sẽ tắt bây giờ, vui lòng mở lại.")
                      await invoke('exit_app')
                    } catch (e) {
                      console.error(e)
                      alert("Lỗi khi xóa cấu hình: " + e)
                    }
                  }
                }}
              >
                {t.settings.resetData}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-about-card">
          <Info size={18} />
          <div>
            <strong>0xoLemon Launcher</strong>
            <span>{t.settings.aboutVersion.replace('{version}', appVersion)}</span>
          </div>
          <Download size={17} />
        </section>
      </div>
    </section>
  )
}
