import { useCallback, useEffect, useMemo, useState, type ChangeEvent, type ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  Activity,
  Archive,
  ArrowRightLeft,
  BarChart3,
  CheckCircle2,
  Cloud,
  Database,
  FileSearch,
  FolderOpen,
  Gamepad2,
  HardDrive,
  KeyRound,
  Loader2,
  Pin,
  RefreshCw,
  RotateCcw,
  Server,
  Settings2,
  ShieldCheck,
  Terminal,
  Trash2,
  TriangleAlert,
  Wrench,
  XCircle,
} from 'lucide-react'
import { useLocale } from '../context/locale'
import { formatBytes } from '../lib/format'
import './CloudRedirectSettings.css'

type ProviderId = 'gdrive' | 'onedrive' | 'r2' | 's3' | 'folder' | 'local'
type ViewId = 'overview' | 'provider' | 'games' | 'backups' | 'migration' | 'maintenance' | 'diagnostics'

type EngineStatus = {
  version: string
  sourceCommit: string
  engineReady: boolean
  engineDir?: string | null
  steamPath?: string | null
  steamRunning: boolean
  steamProcessIds?: number[]
  steamVersion?: number | null
  steamVersionSupported?: boolean
  supportedSteamVersions?: number[]
  dllInstalled: boolean
  mode: string
  provider?: string | null
  providerDisplayName?: string | null
  authenticated: boolean
  tokenPath?: string | null
  syncPath?: string | null
  accountIds: string[]
  lastError?: string | null
  supportedProviders: string[]
}

type SteamRuntimeState = {
  running: boolean
  processIds: number[]
  version?: number | null
  versionSupported: boolean
  supportedVersions: number[]
}

type ProviderConfig = {
  provider: ProviderId
  tokenPath?: string | null
  syncPath?: string | null
  authenticated: boolean
  uploadInflightMb: number
  r2?: {
    accountId: string
    accessKeyId: string
    hasSecret: boolean
    bucket: string
    keyPrefix?: string | null
    endpoint?: string | null
  } | null
  s3?: {
    accessKeyId: string
    hasSecret: boolean
    bucket: string
    endpoint: string
    region: string
    keyPrefix?: string | null
    signPayload: boolean
    allowInsecureHttp: boolean
    allowInsecureTls: boolean
    caCertPath?: string | null
  } | null
}

type RemoteApp = {
  accountId: string
  appId: string
  fileCount: number
  totalSize: number
}

type RemoteFile = { path: string; size: number; modifiedTime: number }
type OperationResult = { success: boolean; message: string; deleted?: number; failed?: number; raw?: unknown }
type MigrationEvent = {
  eventType: string
  phase?: string | null
  message?: string | null
  file?: string | null
  done?: number | null
  total?: number | null
  bytes?: number | null
  migrated?: number | null
  skipped?: number | null
  failed?: number | null
  totalBytes?: number | null
}
type DiagnosticItem = {
  id: string
  severity: 'ok' | 'warning' | 'error' | string
  title: string
  detail: string
  fixAction?: string | null
}
type DiagnosticsReport = { generatedAt: string; items: DiagnosticItem[]; logTail: string[] }
type StatsEntry = { accountId: string; appId: string; content: string }
type ManifestPinConfig = {
  enabled: boolean
  autoComment: boolean
  pinnedApps: string[]
  path: string
  restartRequired: boolean
}
type LocalBackup = {
  id: string
  accountId: string
  appId: string
  size: number
  createdAt: string
  reason: string
  path: string
}
type Cloud760File = { name: string; size: number; persisted: boolean }
type Cloud760Result = {
  appId: string
  cloudEnabledForAccount?: boolean | null
  cloudEnabledForApp?: boolean | null
  quotaTotal?: number | null
  quotaUsed?: number | null
  files: Cloud760File[]
}

type R2Form = {
  accountId: string
  accessKeyId: string
  secretAccessKey: string
  bucket: string
  keyPrefix: string
  endpoint: string
}
type S3Form = {
  accessKeyId: string
  secretAccessKey: string
  bucket: string
  endpoint: string
  region: string
  keyPrefix: string
  signPayload: boolean
  allowInsecureHttp: boolean
  allowInsecureTls: boolean
  caCertPath: string
}

const PROVIDERS: ProviderId[] = ['gdrive', 'onedrive', 'r2', 's3', 'folder', 'local']
const EMPTY_R2: R2Form = { accountId: '', accessKeyId: '', secretAccessKey: '', bucket: '', keyPrefix: '', endpoint: '' }
const EMPTY_S3: S3Form = {
  accessKeyId: '', secretAccessKey: '', bucket: '', endpoint: '', region: '', keyPrefix: '',
  signPayload: false, allowInsecureHttp: false, allowInsecureTls: false, caCertPath: '',
}

export function CloudRedirectSettings() {
  const { t } = useLocale()
  const c = t.cloudRedirectV2
  const [view, setView] = useState<ViewId>('overview')
  const [status, setStatus] = useState<EngineStatus | null>(null)
  const [steamState, setSteamState] = useState<SteamRuntimeState | null>(null)
  const [providerConfig, setProviderConfig] = useState<ProviderConfig | null>(null)
  const [busy, setBusy] = useState('')
  const [message, setMessage] = useState<{ tone: 'success' | 'error' | 'info'; text: string } | null>(null)
  const [mode, setMode] = useState('cloudredirect')
  const [installCore, setInstallCore] = useState(false)

  const [provider, setProvider] = useState<ProviderId>('gdrive')
  const [folderPath, setFolderPath] = useState('')
  const [uploadInflightMb, setUploadInflightMb] = useState(24)
  const [r2, setR2] = useState<R2Form>(EMPTY_R2)
  const [s3, setS3] = useState<S3Form>(EMPTY_S3)
  const [authBusy, setAuthBusy] = useState(false)

  const [apps, setApps] = useState<RemoteApp[]>([])
  const [appsLoading, setAppsLoading] = useState(false)
  const [accountId, setAccountId] = useState('')
  const [selectedApp, setSelectedApp] = useState<RemoteApp | null>(null)
  const [files, setFiles] = useState<RemoteFile[]>([])
  const [deleteConfirm, setDeleteConfirm] = useState('')

  const [migrationSource, setMigrationSource] = useState<ProviderId>('gdrive')
  const [migrationDestination, setMigrationDestination] = useState<ProviderId>('r2')
  const [migration, setMigration] = useState<MigrationEvent | null>(null)
  const [switchAfterMigration, setSwitchAfterMigration] = useState(true)

  const [diagnostics, setDiagnostics] = useState<DiagnosticsReport | null>(null)
  const [maintenanceAccount, setMaintenanceAccount] = useState('')
  const [maintenanceApp, setMaintenanceApp] = useState('')
  const [stats, setStats] = useState<StatsEntry[]>([])
  const [statsLoading, setStatsLoading] = useState(false)
  const [manifestPins, setManifestPins] = useState<ManifestPinConfig | null>(null)
  const [pinnedAppsText, setPinnedAppsText] = useState('')
  const [backups, setBackups] = useState<LocalBackup[]>([])
  const [backupsLoading, setBackupsLoading] = useState(false)
  const [backupAppId, setBackupAppId] = useState('')
  const [backupAccountId, setBackupAccountId] = useState('')
  const [restoreConfirm, setRestoreConfirm] = useState('')
  const [publishAfterRestore, setPublishAfterRestore] = useState(false)
  const [cloud760, setCloud760] = useState<Cloud760Result | null>(null)
  const [cloud760Selection, setCloud760Selection] = useState<string[]>([])
  const [cloud760Confirm, setCloud760Confirm] = useState('')

  const notify = useCallback((tone: 'success' | 'error' | 'info', text: string) => {
    setMessage({ tone, text })
    window.dispatchEvent(new CustomEvent('0xo-toast', {
      detail: {
        category: 'cloudSaves',
        severity: tone === 'error' ? 'error' : tone === 'success' ? 'success' : 'info',
        title: c.title,
        message: text,
        dedupeKey: `cloudredirect-${tone}`,
        action: null,
      },
    }))
  }, [c.title])

  const loadCore = useCallback(async () => {
    const [nextStatus, nextProvider] = await Promise.all([
      invoke<EngineStatus>('cloud_redirect_engine_get_status'),
      invoke<ProviderConfig>('cloud_redirect_engine_get_provider'),
    ])
    setStatus(nextStatus)
    setSteamState({
      running: nextStatus.steamRunning,
      processIds: nextStatus.steamProcessIds || [],
      version: nextStatus.steamVersion ?? null,
      versionSupported: Boolean(nextStatus.steamVersionSupported),
      supportedVersions: nextStatus.supportedSteamVersions || [],
    })
    setMode(nextStatus.mode || 'cloudredirect')
    setProviderConfig(nextProvider)
    setProvider(nextProvider.provider)
    setFolderPath(nextProvider.syncPath || '')
    setUploadInflightMb(nextProvider.uploadInflightMb || 24)
    setR2(nextProvider.r2 ? {
      accountId: nextProvider.r2.accountId,
      accessKeyId: nextProvider.r2.accessKeyId,
      secretAccessKey: '',
      bucket: nextProvider.r2.bucket,
      keyPrefix: nextProvider.r2.keyPrefix || '',
      endpoint: nextProvider.r2.endpoint || '',
    } : EMPTY_R2)
    setS3(nextProvider.s3 ? {
      accessKeyId: nextProvider.s3.accessKeyId,
      secretAccessKey: '',
      bucket: nextProvider.s3.bucket,
      endpoint: nextProvider.s3.endpoint,
      region: nextProvider.s3.region,
      keyPrefix: nextProvider.s3.keyPrefix || '',
      signPayload: nextProvider.s3.signPayload,
      allowInsecureHttp: nextProvider.s3.allowInsecureHttp,
      allowInsecureTls: nextProvider.s3.allowInsecureTls,
      caCertPath: nextProvider.s3.caCertPath || '',
    } : EMPTY_S3)
    if (!accountId && nextStatus.accountIds.length > 0) setAccountId(nextStatus.accountIds[0])
    if (!maintenanceAccount && nextStatus.accountIds.length > 0) setMaintenanceAccount(nextStatus.accountIds[0])
    if (!backupAccountId && nextStatus.accountIds.length > 0) setBackupAccountId(nextStatus.accountIds[0])
  }, [accountId, backupAccountId, maintenanceAccount])

  const refreshSteamState = useCallback(async () => {
    const next = await invoke<SteamRuntimeState>('cloud_redirect_engine_get_steam_state')
    setSteamState(next)
    setStatus((current) => current ? {
      ...current,
      steamRunning: next.running,
      steamProcessIds: next.processIds,
      steamVersion: next.version ?? null,
      steamVersionSupported: next.versionSupported,
      supportedSteamVersions: next.supportedVersions,
    } : current)
    return next
  }, [])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadCore().catch((error) => notify('error', c.loadFailed.replace('{error}', String(error))))
    }, 0)
    return () => window.clearTimeout(timer)
  }, [loadCore, notify, c.loadFailed])

  useEffect(() => {
    let disposed = false
    const poll = async () => {
      try {
        const next = await invoke<SteamRuntimeState>('cloud_redirect_engine_get_steam_state')
        if (!disposed) {
          setSteamState(next)
          setStatus((current) => current ? {
            ...current,
            steamRunning: next.running,
            steamProcessIds: next.processIds,
            steamVersion: next.version ?? null,
            steamVersionSupported: next.versionSupported,
            supportedSteamVersions: next.supportedVersions,
          } : current)
        }
      } catch {
        // The full refresh surface will show actionable backend errors.
      }
    }
    void poll()
    const timer = window.setInterval(poll, 1500)
    return () => { disposed = true; window.clearInterval(timer) }
  }, [])

  useEffect(() => {
    let dispose: (() => void) | undefined
    listen<MigrationEvent>('cloudredirect://migration-progress', (event) => setMigration(event.payload))
      .then((fn) => { dispose = fn })
      .catch(() => undefined)
    return () => dispose?.()
  }, [])

  const run = useCallback(async <T,>(id: string, operation: () => Promise<T>, success?: string): Promise<T | null> => {
    setBusy(id)
    setMessage(null)
    try {
      const result = await operation()
      if (success) notify('success', success)
      await loadCore()
      return result
    } catch (error) {
      notify('error', String(error))
      return null
    } finally {
      setBusy('')
    }
  }, [loadCore, notify])

  const closeSteam = useCallback(async () => {
    setBusy('close-steam')
    setMessage(null)
    try {
      const result = await invoke<OperationResult>('cloud_redirect_engine_close_steam')
      notify('success', result.message)
      for (let attempt = 0; attempt < 20; attempt += 1) {
        const next = await refreshSteamState()
        if (!next.running) break
        await sleep(250)
      }
    } catch (error) {
      notify('error', String(error))
    } finally {
      setBusy('')
    }
  }, [notify, refreshSteamState])

  const providerName = useCallback((id: string) => {
    const names: Record<string, string> = {
      gdrive: c.googleDrive,
      onedrive: c.oneDrive,
      r2: c.cloudflareR2,
      s3: c.s3Compatible,
      folder: c.folderProvider,
      local: c.localOnly,
    }
    return names[id] || id
  }, [c])

  const startOAuth = async () => {
    if (!matchesOAuth(provider)) return
    setAuthBusy(true)
    setMessage({ tone: 'info', text: c.finishInBrowser })
    try {
      await saveProvider(false)
      const authUrl = await invoke<string>('cloud_redirect_start_oauth', { provider })
      await invoke('open_url', { url: authUrl })
      // OAuth polling runs only after a user action, never during render.
      // eslint-disable-next-line react-hooks/purity
      const deadline = Date.now() + 5 * 60_000
      // eslint-disable-next-line react-hooks/purity
      while (Date.now() < deadline) {
        await sleep(800)
        const error = await invoke<string | null>('cloud_redirect_poll_oauth_error')
        if (error) throw new Error(error)
        const code = await invoke<string | null>('cloud_redirect_poll_oauth_code')
        if (!code) continue
        await invoke('cloud_redirect_complete_oauth', { provider, code })
        await loadCore()
        notify('success', c.authSuccess)
        return
      }
      throw new Error(c.authTimeout)
    } catch (error) {
      notify('error', c.authFailed.replace('{error}', String(error)))
    } finally {
      setAuthBusy(false)
    }
  }

  const saveProvider = async (showSuccess = true) => {
    const input = {
      provider,
      tokenPath: null,
      syncPath: provider === 'folder' ? folderPath : null,
      uploadInflightMb,
      r2: provider === 'r2' ? {
        accountId: r2.accountId,
        accessKeyId: r2.accessKeyId,
        secretAccessKey: r2.secretAccessKey || null,
        bucket: r2.bucket,
        keyPrefix: r2.keyPrefix || null,
        endpoint: r2.endpoint || null,
      } : null,
      s3: provider === 's3' ? {
        accessKeyId: s3.accessKeyId,
        secretAccessKey: s3.secretAccessKey || null,
        bucket: s3.bucket,
        endpoint: s3.endpoint,
        region: s3.region,
        keyPrefix: s3.keyPrefix || null,
        signPayload: s3.signPayload,
        allowInsecureHttp: s3.allowInsecureHttp,
        allowInsecureTls: s3.allowInsecureTls,
        caCertPath: s3.caCertPath || null,
      } : null,
    }
    const result = await invoke<ProviderConfig>('cloud_redirect_engine_save_provider', { input })
    setProviderConfig(result)
    if (showSuccess) notify('success', c.providerSaved)
    return result
  }

  const chooseFolder = async (target: 'folder' | 'ca') => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open(target === 'folder'
      ? { directory: true, multiple: false, title: c.chooseFolder }
      : { multiple: false, title: c.chooseCertificate, filters: [{ name: 'Certificates', extensions: ['pem', 'crt', 'cer'] }] })
    if (typeof selected !== 'string') return
    if (target === 'folder') setFolderPath(selected)
    else setS3((value) => ({ ...value, caCertPath: selected }))
  }

  const effectiveProvider = providerConfig?.provider || provider

  const refreshApps = useCallback(async () => {
    setAppsLoading(true)
    try {
      const result = await invoke<RemoteApp[]>('cloud_redirect_engine_list_apps', { provider: effectiveProvider })
      setApps(result)
      if (result[0]) setAccountId((current) => current || result[0].accountId)
    } catch (error) {
      notify('error', String(error))
    } finally {
      setAppsLoading(false)
    }
  }, [effectiveProvider, notify])

  useEffect(() => {
    if (view !== 'games' || !providerConfig?.authenticated || isLocalOnly(providerConfig.provider)) return
    const timer = window.setTimeout(() => void refreshApps(), 0)
    return () => window.clearTimeout(timer)
  }, [providerConfig, refreshApps, view])

  const visibleApps = useMemo(
    () => accountId ? apps.filter((app) => app.accountId === accountId) : apps,
    [accountId, apps],
  )

  const openFiles = async (app: RemoteApp) => {
    setSelectedApp(app)
    setDeleteConfirm('')
    setFiles([])
    await run('files', async () => {
      const result = await invoke<RemoteFile[]>('cloud_redirect_engine_list_files', {
        provider: providerConfig?.provider || provider,
        accountId: app.accountId,
        appId: app.appId,
      })
      setFiles(result)
      return result
    })
  }

  const runDiagnostics = useCallback(async () => {
    setBusy('diagnostics')
    try {
      setDiagnostics(await invoke<DiagnosticsReport>('cloud_redirect_engine_diagnostics'))
    } catch (error) {
      notify('error', String(error))
    } finally {
      setBusy('')
    }
  }, [notify])

  const loadManifestPins = useCallback(async () => {
    try {
      const config = await invoke<ManifestPinConfig>('cloud_redirect_engine_get_manifest_pins')
      setManifestPins(config)
      setPinnedAppsText(config.pinnedApps.join(', '))
    } catch (error) {
      notify('error', String(error))
    }
  }, [notify])

  const saveManifestPins = async () => {
    const pinnedApps = pinnedAppsText
      .split(/[\s,;]+/)
      .map((value) => value.trim())
      .filter(Boolean)
    const config = await invoke<ManifestPinConfig>('cloud_redirect_engine_save_manifest_pins', {
      input: {
        enabled: manifestPins?.enabled ?? false,
        autoComment: manifestPins?.autoComment ?? true,
        pinnedApps,
      },
    })
    setManifestPins(config)
    setPinnedAppsText(config.pinnedApps.join(', '))
    return config
  }

  const refreshBackups = useCallback(async () => {
    setBackupsLoading(true)
    try {
      const result = await invoke<LocalBackup[]>('cloud_redirect_engine_list_backups', {
        appId: backupAppId || null,
      })
      setBackups(result)
    } catch (error) {
      notify('error', String(error))
    } finally {
      setBackupsLoading(false)
    }
  }, [backupAppId, notify])

  const createBackup = async () => {
    const result = await invoke<LocalBackup>('cloud_redirect_engine_create_backup', {
      provider: providerConfig?.provider || provider,
      accountId: backupAccountId,
      appId: backupAppId,
    })
    setBackups((current) => [result, ...current.filter((entry) => entry.id !== result.id)])
    return result
  }

  const restoreBackup = async (backup: LocalBackup) => {
    const result = await invoke<OperationResult>('cloud_redirect_engine_restore_backup', {
      provider: providerConfig?.provider || provider,
      backupId: backup.id,
      confirmation: restoreConfirm,
      publishAfterRestore,
    })
    setRestoreConfirm('')
    return result
  }

  const loadStats = async () => {
    setStatsLoading(true)
    try {
      setStats(await invoke<StatsEntry[]>('cloud_redirect_engine_list_stats', {
        provider: providerConfig?.provider || provider,
      }))
    } catch (error) {
      notify('error', String(error))
    } finally {
      setStatsLoading(false)
    }
  }

  const runCloud760 = async (action: 'list' | 'delete' | 'delete-all') => {
    const result = await invoke<OperationResult>('cloud_redirect_engine_run_cloud760', {
      appId: maintenanceApp || null,
      action,
      files: action === 'delete' ? cloud760Selection : null,
      confirmation: action === 'list' ? null : cloud760Confirm,
    })
    if (result.raw && typeof result.raw === 'object') {
      const raw = result.raw as Cloud760Result
      setCloud760({ ...raw, files: Array.isArray(raw.files) ? raw.files : [] })
    }
    if (action !== 'list') {
      setCloud760Selection([])
      setCloud760Confirm('')
    }
    return result
  }

  useEffect(() => {
    if (view !== 'maintenance') return
    const timer = window.setTimeout(() => void loadManifestPins(), 0)
    return () => window.clearTimeout(timer)
  }, [loadManifestPins, view])

  useEffect(() => {
    if (view !== 'backups') return
    const timer = window.setTimeout(() => void refreshBackups(), 0)
    return () => window.clearTimeout(timer)
  }, [refreshBackups, view])

  useEffect(() => {
    if (view !== 'diagnostics' || diagnostics || busy) return
    const timer = window.setTimeout(() => void runDiagnostics(), 0)
    return () => window.clearTimeout(timer)
  }, [busy, diagnostics, runDiagnostics, view])

  const providerReady = providerConfig?.authenticated ?? false
  const effectiveSteamRunning = steamState?.running ?? status?.steamRunning ?? false
  const steamVersion = steamState?.version ?? status?.steamVersion ?? null
  const steamVersionSupported = steamState?.versionSupported ?? status?.steamVersionSupported ?? false
  const steamPids = steamState?.processIds ?? status?.steamProcessIds ?? []
  const migrationPercent = migration?.total && migration?.done != null
    ? Math.min(100, Math.round((migration.done / migration.total) * 100))
    : null

  return (
    <section className="cr2-shell">
      <header className="cr2-titlebar">
        <div className="cr2-title-copy">
          <div className="cr2-title-row">
            <h2>{c.title}</h2>
            <span className="cr2-version">v{status?.version || '2.6.4'}</span>
          </div>
          <p>{c.subtitle}</p>
          <small className="cr2-attribution">{c.attribution}</small>
        </div>
        <button className="cr2-icon-button" onClick={() => void loadCore()} title={c.refresh}>
          <RefreshCw size={16} />
        </button>
      </header>

      {message ? <div className={`cr2-message is-${message.tone}`} role="status">{message.text}</div> : null}

      <nav className="cr2-tabs" aria-label={c.title}>
        {([
          ['overview', Activity, c.overview],
          ['provider', KeyRound, c.provider],
          ['games', Gamepad2, c.games],
          ['backups', Archive, c.backups],
          ['migration', ArrowRightLeft, c.migration],
          ['maintenance', Settings2, c.maintenance],
          ['diagnostics', Terminal, c.diagnostics],
        ] as const).map(([id, Icon, label]) => (
          <button key={id} className={view === id ? 'is-active' : ''} onClick={() => setView(id)}>
            <Icon size={15} /> {label}
          </button>
        ))}
      </nav>

      {view === 'overview' ? (
        <div className="cr2-stack">
          <div className="cr2-status-strip">
            <div className={status?.engineReady ? 'is-good' : 'is-danger'}><Database size={15} /><span><small>{c.engine}</small><b>{status?.engineReady ? `${c.ready} · ${status.version}` : c.notBuilt}</b></span></div>
            <div className={status?.dllInstalled ? 'is-good' : 'is-warning'}>{status?.dllInstalled ? <ShieldCheck size={15} /> : <TriangleAlert size={15} />}<span><small>{c.steamPatch}</small><b>{status?.dllInstalled ? c.installed : c.notInstalled}</b></span></div>
            <div className={status?.authenticated ? 'is-good' : 'is-warning'}><Cloud size={15} /><span><small>{c.activeProvider}</small><b>{providerName(status?.provider || provider)}</b></span></div>
            <div className={effectiveSteamRunning ? 'is-warning' : 'is-good'}>{effectiveSteamRunning ? <TriangleAlert size={15} /> : <CheckCircle2 size={15} />}<span><small>{c.steam}</small><b>{effectiveSteamRunning ? c.steamRunning : c.steamClosed}</b></span></div>
          </div>

          {steamVersion && !steamVersionSupported ? (
            <div className="cr2-compatibility-notice" role="status">
              <TriangleAlert size={16} />
              <div><b>{c.unsupportedSteamTitle}</b><span>{c.unsupportedSteamBody.replace('{version}', String(steamVersion)).replace('{engine}', status?.version || '2.6.4')}</span></div>
            </div>
          ) : null}

          <section className="cr2-panel cr2-setup-panel">
            <div className="cr2-panel-heading">
              <div><h3>{c.setup}</h3><p>{c.setupDesc}</p></div>
              <span className="cr2-setup-state">{status?.engineReady && status?.dllInstalled ? c.operational : c.actionNeeded}</span>
            </div>
            <div className="cr2-mode-row">
              <label><input type="radio" checked={mode === 'cloudredirect'} onChange={() => setMode('cloudredirect')} /> <span><b>{c.cloudRedirectMode}</b><small>{c.cloudRedirectModeDesc}</small></span></label>
              <label><input type="radio" checked={mode === 'stfixer'} onChange={() => setMode('stfixer')} /> <span><b>{c.stfixerMode}</b><small>{c.stfixerModeDesc}</small></span></label>
              <label><input type="radio" checked={mode === 'thirdparty'} onChange={() => setMode('thirdparty')} /> <span><b>{c.thirdPartyMode}</b><small>{c.thirdPartyModeDesc}</small></span></label>
            </div>
            <label className="cr2-check"><input type="checkbox" checked={installCore} onChange={(event: ChangeEvent<HTMLInputElement>) => setInstallCore(event.target.checked)} /> {c.installCore}</label>
            <div className="cr2-actions">
              {effectiveSteamRunning ? (
                <button className="primary" disabled={Boolean(busy)} onClick={() => void closeSteam()}>
                  {busy === 'close-steam' ? <Loader2 className="spin" size={15} /> : <TriangleAlert size={15} />} {c.closeSteam}
                </button>
              ) : (
                <button className="primary" disabled={Boolean(busy) || Boolean(steamVersion && !steamVersionSupported)} onClick={() => void run('patch', () => invoke<OperationResult>('cloud_redirect_engine_run_required_patches', { mode, installCoreIfMissing: installCore }), c.patchesApplied)}>
                  {busy === 'patch' ? <Loader2 className="spin" size={15} /> : <Wrench size={15} />} {c.runAllPatches}
                </button>
              )}
              <button disabled={Boolean(busy) || effectiveSteamRunning} onClick={() => void run('install', () => invoke<OperationResult>('cloud_redirect_engine_install'), c.engineInstalled)}>{c.installEngine}</button>
              <button className="danger-quiet" disabled={Boolean(busy) || effectiveSteamRunning || !status?.dllInstalled} onClick={() => void run('remove', () => invoke<OperationResult>('cloud_redirect_engine_remove'), c.engineRemoved)}>{c.removeEngine}</button>
            </div>
            {effectiveSteamRunning ? <p className="cr2-inline-warning"><TriangleAlert size={14} /> {c.steamRunningBackground.replace('{pids}', steamPids.length ? steamPids.join(', ') : '—')}</p> : null}
          </section>
        </div>
      ) : null}

      {view === 'provider' ? (
        <div className="cr2-stack">
          <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.provider}</h3><p>{c.providerDesc}</p></div></div>
            <div className="cr2-provider-grid">
              {PROVIDERS.map((id) => (
                <button key={id} className={provider === id ? 'is-selected' : ''} onClick={() => setProvider(id)}>
                  {providerIcon(id)}<span><b>{providerName(id)}</b><small>{providerDescription(id, c)}</small></span>
                  {providerConfig?.provider === id ? <CheckCircle2 className="selected-check" size={16} /> : null}
                </button>
              ))}
            </div>

            <div className="cr2-form">
              <label>{c.parallelUpload}<input type="range" min="24" max="64" step="8" value={uploadInflightMb} onChange={(event: ChangeEvent<HTMLInputElement>) => setUploadInflightMb(Number(event.target.value))} /><output>{uploadInflightMb} MB</output></label>
              {provider === 'folder' ? (
                <Field label={c.syncFolder}><div className="cr2-field-with-button"><input value={folderPath} onChange={(event: ChangeEvent<HTMLInputElement>) => setFolderPath(event.target.value)} /><button onClick={() => void chooseFolder('folder')}><FolderOpen size={14} /> {c.browse}</button></div></Field>
              ) : null}
              {provider === 'r2' ? <R2Fields c={c} value={r2} onChange={setR2} /> : null}
              {provider === 's3' ? <S3Fields c={c} value={s3} onChange={setS3} onBrowseCa={() => void chooseFolder('ca')} /> : null}
            </div>

            <div className="cr2-actions">
              <button className="primary" disabled={Boolean(busy)} onClick={() => void run('save-provider', () => saveProvider(false), c.providerSaved)}>{busy === 'save-provider' ? <Loader2 className="spin" size={15} /> : <Settings2 size={15} />} {c.saveProvider}</button>
              {matchesOAuth(provider) ? <button disabled={authBusy || Boolean(busy)} onClick={() => void startOAuth()}>{authBusy ? <Loader2 className="spin" size={15} /> : <KeyRound size={15} />} {c.signIn}</button> : null}
              <button disabled={Boolean(busy)} onClick={() => void run('test-provider', () => invoke<OperationResult>('cloud_redirect_engine_test_provider', { provider }), c.connectionVerified)}><FileSearch size={15} /> {c.testConnection}</button>
            </div>
            <div className={`cr2-provider-state ${providerReady ? 'is-good' : ''}`}>
              {providerReady ? <CheckCircle2 size={17} /> : <TriangleAlert size={17} />}
              <span><b>{providerReady ? c.configured : c.notConfigured}</b><small>{providerConfig?.tokenPath || providerConfig?.syncPath || c.credentialsProtected}</small></span>
            </div>
          </section>
        </div>
      ) : null}

      {view === 'games' ? (
        <div className="cr2-stack">
          <section className="cr2-panel">
            <div className="cr2-panel-heading">
              <div><h3>{c.remoteGames}</h3><p>{c.remoteGamesDesc}</p></div>
              <button onClick={() => void refreshApps()} disabled={appsLoading || isLocalOnly(providerConfig?.provider || provider)}>{appsLoading ? <Loader2 className="spin" size={15} /> : <RefreshCw size={15} />} {c.scanCloud}</button>
            </div>
            <div className="cr2-filter-row">
              <label>{c.steamAccount}<select value={accountId} onChange={(event: ChangeEvent<HTMLSelectElement>) => setAccountId(event.target.value)}>{status?.accountIds.map((id) => <option key={id} value={id}>{id}</option>)}</select></label>
              <button disabled={!accountId || Boolean(busy)} onClick={() => void run('sync-all', () => invoke<OperationResult>('cloud_redirect_engine_sync_all', { provider: providerConfig?.provider || provider, accountId }), c.syncCompleted)}>{c.syncAll}</button>
            </div>
            {isLocalOnly(providerConfig?.provider || provider) ? <EmptyState icon={HardDrive} title={c.remoteUnavailable} body={c.remoteUnavailableDesc} /> : visibleApps.length === 0 ? <EmptyState icon={Cloud} title={c.noCloudGames} body={c.noCloudGamesDesc} /> : (
              <div className="cr2-app-list">
                {visibleApps.map((app) => <button key={`${app.accountId}:${app.appId}`} onClick={() => void openFiles(app)} className={selectedApp?.appId === app.appId && selectedApp.accountId === app.accountId ? 'is-selected' : ''}>
                  <span className="cr2-app-icon"><Gamepad2 size={17} /></span><span><b>App {app.appId}</b><small>{app.fileCount} {c.files} · {formatBytes(app.totalSize)}</small></span><span className="cr2-account-tag">{app.accountId}</span>
                </button>)}
              </div>
            )}
          </section>

          {selectedApp ? <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.appDetails.replace('{appId}', selectedApp.appId)}</h3><p>{selectedApp.accountId}</p></div></div>
            <div className="cr2-actions">
              <button className="primary" disabled={Boolean(busy)} onClick={() => void run('sync-app', () => invoke<OperationResult>('cloud_redirect_engine_sync_app', { provider: providerConfig?.provider || provider, accountId: selectedApp.accountId, appId: selectedApp.appId }), c.syncCompleted)}>{c.syncNow}</button>
              <button disabled={Boolean(busy)} onClick={() => void openFiles(selectedApp)}>{c.refreshFiles}</button>
              <button disabled={Boolean(busy)} onClick={() => void run('manifest', () => invoke<OperationResult>('cloud_redirect_engine_publish_manifest', { provider: providerConfig?.provider || provider, accountId: selectedApp.accountId, appId: selectedApp.appId }), c.manifestPublished)}>{c.publishManifest}</button>
              <button disabled={Boolean(busy)} onClick={() => void run('gc', () => invoke<OperationResult>('cloud_redirect_engine_gc_blobs', { provider: providerConfig?.provider || provider, accountId: selectedApp.accountId, appId: selectedApp.appId }), c.cleanupCompleted)}>{c.cleanOrphans}</button>
              <button disabled={Boolean(busy)} onClick={() => { setBackupAccountId(selectedApp.accountId); setBackupAppId(selectedApp.appId); setView('backups') }}><Archive size={14} /> {c.createBackup}</button>
            </div>
            <div className="cr2-file-list">
              {files.length === 0 ? <span>{c.noFiles}</span> : files.map((file) => <div key={file.path}><span>{file.path}</span><small>{formatBytes(file.size)}</small></div>)}
            </div>
            <div className="cr2-danger-zone">
              <div><b>{c.deleteCloudData}</b><small>{c.deleteCloudDataDesc}</small></div>
              <input value={deleteConfirm} onChange={(event: ChangeEvent<HTMLInputElement>) => setDeleteConfirm(event.target.value)} placeholder={`DELETE ${selectedApp.appId}`} />
              <button disabled={deleteConfirm !== `DELETE ${selectedApp.appId}` || Boolean(busy)} onClick={() => void run('delete', () => invoke<OperationResult>('cloud_redirect_engine_delete_app', { provider: providerConfig?.provider || provider, accountId: selectedApp.accountId, appId: selectedApp.appId, confirmation: deleteConfirm }), c.deletedCloudData).then(() => { setSelectedApp(null); setFiles([]); void refreshApps() })}><Trash2 size={14} /> {c.delete}</button>
            </div>
          </section> : null}
        </div>
      ) : null}

      {view === 'backups' ? (
        <div className="cr2-stack">
          <section className="cr2-panel">
            <div className="cr2-panel-heading">
              <div><h3>{c.backups}</h3><p>{c.backupsDesc}</p></div>
              <button onClick={() => void refreshBackups()} disabled={backupsLoading}>{backupsLoading ? <Loader2 className="spin" size={15} /> : <RefreshCw size={15} />} {c.refresh}</button>
            </div>
            <div className="cr2-maintenance-fields">
              <label>{c.steamAccount}<input value={backupAccountId} onChange={(event: ChangeEvent<HTMLInputElement>) => setBackupAccountId(event.target.value)} /></label>
              <label>{c.appId}<input value={backupAppId} onChange={(event: ChangeEvent<HTMLInputElement>) => setBackupAppId(event.target.value)} /></label>
            </div>
            <div className="cr2-actions">
              <button className="primary" disabled={!backupAccountId || !backupAppId || Boolean(busy) || effectiveSteamRunning} onClick={() => void run('backup', createBackup, c.backupCreated).then(() => void refreshBackups())}><Archive size={15} /> {c.createBackup}</button>
            </div>
            {status?.steamRunning ? <p className="cr2-inline-warning"><TriangleAlert size={14} /> {c.closeSteamForBackup}</p> : null}
          </section>
          <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.availableBackups}</h3><p>{c.restoreDesc}</p></div></div>
            {backups.length === 0 ? <EmptyState icon={Archive} title={c.noBackups} body={c.noBackupsDesc} /> : <div className="cr2-backup-list">
              {backups.map((backup) => <article key={backup.id}>
                <span><Archive size={17} /></span>
                <div><b>App {backup.appId}</b><small>{new Date(backup.createdAt).toLocaleString()} · {formatBytes(backup.size)} · {backup.reason}</small><code>{backup.id}</code></div>
                <button onClick={() => setRestoreConfirm(`RESTORE ${backup.id}`)}>{c.prepareRestore}</button>
                <button className="danger-quiet" disabled={restoreConfirm !== `RESTORE ${backup.id}` || Boolean(busy) || effectiveSteamRunning} onClick={() => void run('restore', () => restoreBackup(backup), c.backupRestored)}><RotateCcw size={14} /> {c.restore}</button>
              </article>)}
            </div>}
            <label className="cr2-check"><input type="checkbox" checked={publishAfterRestore} onChange={(event: ChangeEvent<HTMLInputElement>) => setPublishAfterRestore(event.target.checked)} /> {c.publishAfterRestore}</label>
            {restoreConfirm ? <div className="cr2-confirm-line"><label>{c.restoreConfirmation}<input value={restoreConfirm} onChange={(event: ChangeEvent<HTMLInputElement>) => setRestoreConfirm(event.target.value)} /></label></div> : null}
          </section>
        </div>
      ) : null}

      {view === 'migration' ? (
        <div className="cr2-stack"><section className="cr2-panel">
          <div className="cr2-panel-heading"><div><h3>{c.migration}</h3><p>{c.migrationDesc}</p></div></div>
          <div className="cr2-migration-picker">
            <label>{c.sourceProvider}<select value={migrationSource} onChange={(event: ChangeEvent<HTMLSelectElement>) => setMigrationSource(event.target.value as ProviderId)}>{PROVIDERS.filter((id) => !isLocalOnly(id)).map((id) => <option value={id} key={id}>{providerName(id)}</option>)}</select></label>
            <ArrowRightLeft size={22} />
            <label>{c.destinationProvider}<select value={migrationDestination} onChange={(event: ChangeEvent<HTMLSelectElement>) => setMigrationDestination(event.target.value as ProviderId)}>{PROVIDERS.filter((id) => !isLocalOnly(id)).map((id) => <option value={id} key={id}>{providerName(id)}</option>)}</select></label>
          </div>
          <label className="cr2-check"><input type="checkbox" checked={switchAfterMigration} onChange={(event: ChangeEvent<HTMLInputElement>) => setSwitchAfterMigration(event.target.checked)} /> {c.switchAfterMigration}</label>
          <button className="primary" disabled={Boolean(busy) || migrationSource === migrationDestination} onClick={() => void run('migration', () => invoke<MigrationEvent>('cloud_redirect_engine_migrate', { request: { sourceProvider: migrationSource, destinationProvider: migrationDestination, switchAfterVerify: switchAfterMigration } }), c.migrationCompleted)}>{busy === 'migration' ? <Loader2 className="spin" size={15} /> : <ArrowRightLeft size={15} />} {c.startMigration}</button>
          {migration ? <div className="cr2-progress-card">
            <div><b>{migration.phase || migration.eventType}</b><span>{migration.message || migration.file || c.migrating}</span><output>{migrationPercent == null ? '—' : `${migrationPercent}%`}</output></div>
            <div className="cr2-progress"><span style={{ width: `${migrationPercent || 0}%` }} /></div>
            <small>{c.migrationCounts.replace('{migrated}', String(migration.migrated || 0)).replace('{skipped}', String(migration.skipped || 0)).replace('{failed}', String(migration.failed || 0))}</small>
          </div> : null}
        </section></div>
      ) : null}

      {view === 'maintenance' ? (
        <div className="cr2-stack">
          <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.maintenance}</h3><p>{c.maintenanceDesc}</p></div></div>
            <div className="cr2-maintenance-fields">
              <label>{c.steamAccount}<input value={maintenanceAccount} onChange={(event: ChangeEvent<HTMLInputElement>) => setMaintenanceAccount(event.target.value)} /></label>
              <label>{c.appId}<input value={maintenanceApp} onChange={(event: ChangeEvent<HTMLInputElement>) => setMaintenanceApp(event.target.value)} /></label>
            </div>
            <div className="cr2-tool-grid">
              <ToolButton icon={Database} title={c.pruneLegacy} body={c.pruneLegacyDesc} disabled={Boolean(busy)} onClick={() => void run('prune', () => invoke<OperationResult>('cloud_redirect_engine_prune_legacy'), c.cleanupCompleted)} />
              <ToolButton icon={HardDrive} title={c.cloud760} body={c.cloud760Desc} disabled={Boolean(busy)} onClick={() => void run('cloud760-list', () => runCloud760('list'), c.cloud760Loaded)} />
              <ToolButton icon={BarChart3} title={c.statsMetadata} body={c.statsMetadataDesc} disabled={Boolean(busy) || statsLoading} onClick={() => void loadStats()} />
              <ToolButton icon={FileSearch} title={c.diagnostics} body={c.diagnosticsDesc} disabled={Boolean(busy)} onClick={() => { setView('diagnostics'); void runDiagnostics() }} />
              <ToolButton icon={RefreshCw} title={c.reloadEngine} body={c.reloadEngineDesc} disabled={Boolean(busy)} onClick={() => void run('reload', () => invoke<EngineStatus>('cloud_redirect_engine_get_status'), c.statusRefreshed)} />
            </div>
          </section>

          <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.manifestPinning}</h3><p>{c.manifestPinningDesc}</p></div><Pin size={18} /></div>
            {manifestPins ? <>
              <div className="cr2-option-grid">
                <label><input type="checkbox" checked={manifestPins.enabled} onChange={(event: ChangeEvent<HTMLInputElement>) => setManifestPins({ ...manifestPins, enabled: event.target.checked })} /> {c.enableManifestPinning}</label>
                <label><input type="checkbox" checked={manifestPins.autoComment} onChange={(event: ChangeEvent<HTMLInputElement>) => setManifestPins({ ...manifestPins, autoComment: event.target.checked })} /> {c.autoCommentPins}</label>
              </div>
              <Field label={c.pinnedAppIds}><input value={pinnedAppsText} onChange={(event: ChangeEvent<HTMLInputElement>) => setPinnedAppsText(event.target.value)} placeholder="2371090, 3768760" /></Field>
              <small className="cr2-path-note">{manifestPins.path}</small>
              <div className="cr2-actions"><button className="primary" disabled={Boolean(busy)} onClick={() => void run('save-pins', saveManifestPins, c.manifestPinningSaved)}><Pin size={14} /> {c.saveManifestPinning}</button></div>
              {manifestPins.restartRequired ? <p className="cr2-inline-warning"><TriangleAlert size={14} /> {c.restartSteamForPins}</p> : null}
            </> : <span>{c.loading}</span>}
          </section>

          {stats.length > 0 ? <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.statsMetadata}</h3><p>{c.statsFound.replace('{count}', String(stats.length))}</p></div></div>
            <div className="cr2-stats-list">{stats.map((entry) => <details key={`${entry.accountId}:${entry.appId}`}><summary><BarChart3 size={14} /> App {entry.appId} · {entry.accountId}</summary><pre>{prettyJson(entry.content)}</pre></details>)}</div>
          </section> : null}

          {cloud760 ? <section className="cr2-panel">
            <div className="cr2-panel-heading"><div><h3>{c.cloud760Inventory.replace('{appId}', cloud760.appId)}</h3><p>{c.cloud760Quota.replace('{used}', formatBytes(cloud760.quotaUsed || 0)).replace('{total}', formatBytes(cloud760.quotaTotal || 0))}</p></div><button onClick={() => void run('cloud760-refresh', () => runCloud760('list'))}><RefreshCw size={14} /> {c.refresh}</button></div>
            <div className="cr2-file-list cr2-selectable-files">{cloud760.files.length === 0 ? <span>{c.noCloud760Files}</span> : cloud760.files.map((file) => <label key={file.name}><input type="checkbox" checked={cloud760Selection.includes(file.name)} onChange={(event: ChangeEvent<HTMLInputElement>) => setCloud760Selection((current) => event.target.checked ? [...current, file.name] : current.filter((value) => value !== file.name))} /><span>{file.name}</span><small>{formatBytes(file.size)}</small></label>)}</div>
            <div className="cr2-danger-zone">
              <div><b>{c.cloud760Delete}</b><small>{c.cloud760DeleteDesc}</small></div>
              <input value={cloud760Confirm} onChange={(event: ChangeEvent<HTMLInputElement>) => setCloud760Confirm(event.target.value)} placeholder={`DELETE ${cloud760.appId}`} />
              <button disabled={!cloud760Selection.length || cloud760Confirm !== `DELETE ${cloud760.appId}` || Boolean(busy)} onClick={() => void run('cloud760-delete', () => runCloud760('delete'), c.cloud760Completed)}><Trash2 size={14} /> {c.deleteSelected}</button>
              <button disabled={cloud760Confirm !== `DELETE ${cloud760.appId}` || Boolean(busy)} onClick={() => void run('cloud760-delete-all', () => runCloud760('delete-all'), c.cloud760Completed)}><Trash2 size={14} /> {c.deleteAll}</button>
            </div>
          </section> : null}

        </div>
      ) : null}

      {view === 'diagnostics' ? (
        <div className="cr2-stack">
          <section className="cr2-panel">
            <div className="cr2-panel-heading">
              <div><h3>{c.diagnostics}</h3><p>{c.diagnosticsDesc}</p></div>
              <button disabled={Boolean(busy)} onClick={() => void runDiagnostics()}>
                {busy === 'diagnostics' ? <Loader2 className="spin" size={14} /> : <RefreshCw size={14} />} {c.refresh}
              </button>
            </div>
            {diagnostics ? <>
              <small className="cr2-path-note">{new Date(diagnostics.generatedAt).toLocaleString()}</small>
              <div className="cr2-diagnostic-list">{diagnostics.items.map((item) => <div key={item.id} className={`is-${item.severity}`}>{item.severity === 'ok' ? <CheckCircle2 size={16} /> : item.severity === 'error' ? <XCircle size={16} /> : <TriangleAlert size={16} />}<span><b>{item.title}</b><small>{item.detail}</small></span></div>)}</div>
              <details className="cr2-log"><summary><Terminal size={14} /> {c.technicalLog}</summary><pre>{diagnostics.logTail.join('\n') || c.noLog}</pre></details>
            </> : <EmptyState icon={FileSearch} title={c.diagnostics} body={c.diagnosticsDesc} />}
          </section>
        </div>
      ) : null}
    </section>
  )
}

function prettyJson(content: string) {
  try { return JSON.stringify(JSON.parse(content), null, 2) } catch { return content }
}
function matchesOAuth(provider: string): provider is 'gdrive' | 'onedrive' {
  return provider === 'gdrive' || provider === 'onedrive'
}
function isLocalOnly(provider: string) { return provider === 'local' }
function sleep(ms: number) { return new Promise((resolve) => setTimeout(resolve, ms)) }
function providerIcon(id: ProviderId) {
  if (id === 'folder') return <FolderOpen size={20} />
  if (id === 'local') return <HardDrive size={20} />
  if (id === 'r2' || id === 's3') return <Server size={20} />
  return <Cloud size={20} />
}
function providerDescription(id: ProviderId, c: Record<string, string>) {
  const map: Record<ProviderId, string> = { gdrive: c.googleDriveDesc, onedrive: c.oneDriveDesc, r2: c.r2Desc, s3: c.s3Desc, folder: c.folderDesc, local: c.localDesc }
  return map[id]
}
function Field({ label, children }: { label: string; children: ReactNode }) { return <label className="cr2-field"><span>{label}</span>{children}</label> }
function R2Fields({ c, value, onChange }: { c: Record<string, string>; value: R2Form; onChange: (value: R2Form) => void }) {
  const set = (key: keyof R2Form) => (event: ChangeEvent<HTMLInputElement>) => onChange({ ...value, [key]: event.target.value })
  return <div className="cr2-form-grid"><Field label={c.accountId}><input value={value.accountId} onChange={set('accountId')} /></Field><Field label={c.accessKey}><input value={value.accessKeyId} onChange={set('accessKeyId')} /></Field><Field label={c.secretKey}><input type="password" value={value.secretAccessKey} onChange={set('secretAccessKey')} placeholder={c.leaveBlankToKeep} /></Field><Field label={c.bucket}><input value={value.bucket} onChange={set('bucket')} /></Field><Field label={c.keyPrefix}><input value={value.keyPrefix} onChange={set('keyPrefix')} /></Field><Field label={c.endpointOptional}><input value={value.endpoint} onChange={set('endpoint')} /></Field></div>
}
function S3Fields({ c, value, onChange, onBrowseCa }: { c: Record<string, string>; value: S3Form; onChange: (value: S3Form) => void; onBrowseCa: () => void }) {
  const set = (key: keyof S3Form) => (event: ChangeEvent<HTMLInputElement>) => onChange({ ...value, [key]: event.target.type === 'checkbox' ? event.target.checked : event.target.value })
  return <><div className="cr2-form-grid"><Field label={c.accessKey}><input value={value.accessKeyId} onChange={set('accessKeyId')} /></Field><Field label={c.secretKey}><input type="password" value={value.secretAccessKey} onChange={set('secretAccessKey')} placeholder={c.leaveBlankToKeep} /></Field><Field label={c.bucket}><input value={value.bucket} onChange={set('bucket')} /></Field><Field label={c.endpoint}><input value={value.endpoint} onChange={set('endpoint')} /></Field><Field label={c.region}><input value={value.region} onChange={set('region')} /></Field><Field label={c.keyPrefix}><input value={value.keyPrefix} onChange={set('keyPrefix')} /></Field><Field label={c.caCertificate}><div className="cr2-field-with-button"><input value={value.caCertPath} onChange={set('caCertPath')} /><button onClick={onBrowseCa}>{c.browse}</button></div></Field></div><div className="cr2-option-grid"><label><input type="checkbox" checked={value.signPayload} onChange={set('signPayload')} /> {c.signPayload}</label><label><input type="checkbox" checked={value.allowInsecureHttp} onChange={set('allowInsecureHttp')} /> {c.allowHttp}</label><label><input type="checkbox" checked={value.allowInsecureTls} onChange={set('allowInsecureTls')} /> {c.allowTls}</label></div></>
}
function EmptyState({ icon: Icon, title, body }: { icon: typeof Cloud; title: string; body: string }) { return <div className="cr2-empty"><Icon size={28} /><b>{title}</b><span>{body}</span></div> }
function ToolButton({ icon: Icon, title, body, disabled, onClick }: { icon: typeof Cloud; title: string; body: string; disabled: boolean; onClick: () => void }) { return <button className="cr2-tool" disabled={disabled} onClick={onClick}><span><Icon size={18} /></span><div><b>{title}</b><small>{body}</small></div></button> }
