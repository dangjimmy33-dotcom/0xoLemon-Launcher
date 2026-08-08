import { useEffect, useMemo, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Cloud, CloudOff, FolderSync, TriangleAlert, Wrench, CheckCircle2, XCircle, Terminal, ChevronLeft, ShieldCheck, ShieldX, RefreshCw, Save, ChevronDown, FolderOpen, Info, HardDrive, DatabaseBackup, Clock3, RotateCw, Settings2 } from 'lucide-react'
import type { CloudSaveStatus, GameCatalog, GameInstallState, CloudRedirectStatus, StfixerResult, CloudProviderConfig } from '../types'
import { isTauriRuntime } from '../lib/gameMeta'
import { formatBytes } from '../lib/format'
import { cloudSavePresentation, quotaPercent } from '../lib/cloudSaveStatus'
import { useLocale } from '../context/LocaleContext'
import { CloudRedirectSettings } from './CloudRedirectSettings'

const PROVIDER_VALUES = ['gdrive', 'onedrive', 'folder'] as const

export function CloudSavesOverview({
  catalog,
  installStates,
  assets,
  onOpenGame,
  onRequestAsset,
}: {
  catalog: GameCatalog
  installStates: Record<string, GameInstallState>
  assets: Record<string, string>
  onOpenGame: (gameId: string) => void
  onRequestAsset: (gameId: string, assetId: string, urgent?: boolean) => void
}) {
  const { t, locale } = useLocale()
  const c = t.cloudSave
  const installed = useMemo(
    () => catalog.games.filter((game) => installStates[game.id]?.installed),
    [catalog.games, installStates],
  )
  const installedIds = useMemo(() => installed.map((game) => game.id).join('|'), [installed])
  const [statuses, setStatuses] = useState<Record<string, CloudSaveStatus>>({})
  const [activeMode, setActiveMode] = useState<'native' | 'stfixer' | null>(null)

  const [nativeGoogleConnected, setNativeGoogleConnected] = useState(false)
  const [nativeBusyMessage, setNativeBusyMessage] = useState('')

  // --- Cloud Save status polling ---
  useEffect(() => {
    for (const game of installed) onRequestAsset(game.id, game.gridAssetId)
    if (!isTauriRuntime()) return
    let disposed = false
    
    // Poll global auth state alongside game statuses
    const pollNativeData = async () => {
      try {
        const isConnected = await invoke<boolean>('global_is_google_drive_connected')
        if (!disposed) setNativeGoogleConnected(isConnected)

        const entries = await Promise.all(
          installed.map(async (game) => {
            const status = await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })
            return [game.id, status] as const
          }),
        )
        if (!disposed) setStatuses(Object.fromEntries(entries))
      } catch (e) {
        // ignore
      }
    }
    
    pollNativeData()
    const interval = setInterval(pollNativeData, 5000)
    
    return () => {
      disposed = true
      clearInterval(interval)
    }
  }, [installed, installedIds, onRequestAsset])

  const nativeSummary = useMemo(() => {
    const values = Object.values(statuses)
    const pendingCount = values.reduce((sum, status) => sum + (status.pendingOperationCount || 0), 0)
    const pendingBytes = values.reduce((sum, status) => sum + (status.pendingUploadBytes || 0), 0)
    const quota = values.find((status) => status.quota)?.quota ?? null
    const protectedGames = values.filter((status) => status.enabled && status.automaticProtection).length
    const attentionGames = values.filter((status) => cloudSavePresentation(status, c.states).blocking).length
    return { pendingCount, pendingBytes, quota, protectedGames, attentionGames }
  }, [statuses])

  // --- STFixer state ---
  const [crStatus, setCrStatus] = useState<CloudRedirectStatus | null>(null)
  const [stfixerBusy, setStfixerBusy] = useState(false)
  const [installCoreIfMissing, setInstallCoreIfMissing] = useState(false)
  const [stfixerResult, setStfixerResult] = useState<StfixerResult | null>(null)

  // --- Cloud Provider state (reads from real config.json) ---
  const [providerConfig, setProviderConfig] = useState<CloudProviderConfig | null>(null)
  const [editProvider, setEditProvider] = useState('')
  const [editTokenPath, setEditTokenPath] = useState('')
  const [providerSaving, setProviderSaving] = useState(false)
  const [providerSaveMsg, setProviderSaveMsg] = useState('')
  const [providerSaveError, setProviderSaveError] = useState(false)

  // Load STFixer status + provider config when entering stfixer mode
  const loadStfixerData = useCallback(async () => {
    if (!isTauriRuntime()) return
    try {
      const [status, config] = await Promise.all([
        invoke<CloudRedirectStatus>('cloud_redirect_get_status'),
        invoke<CloudProviderConfig>('cloud_redirect_get_provider_config'),
      ])
      setCrStatus(status)
      setProviderConfig(config)
      setEditProvider(config.provider || '')
      setEditTokenPath(config.tokenPath || '')
    } catch (e) {
      console.error('Failed to load STFixer data:', e)
    }
  }, [])

  useEffect(() => {
    if (activeMode === 'stfixer') loadStfixerData()
  }, [activeMode, loadStfixerData])

  async function handleApplyStfixer() {
    if (!isTauriRuntime()) return
    setStfixerBusy(true)
    setStfixerResult(null)
    try {
      const result = await invoke<StfixerResult>('cloud_redirect_run_stfixer', {
        installCoreIfMissing
      })
      setStfixerResult(result)
      const newStatus = await invoke<CloudRedirectStatus>('cloud_redirect_get_status')
      setCrStatus(newStatus)
    } catch (e: any) {
      setStfixerResult({ succeeded: false, log: [String(e)], error: String(e) })
    } finally {
      setStfixerBusy(false)
    }
  }

  async function handleSaveProviderConfig() {
    if (!isTauriRuntime()) return
    setProviderSaving(true)
    setProviderSaveMsg('')
    setProviderSaveError(false)
    try {
      await invoke('cloud_redirect_save_provider_config', {
        provider: editProvider,
        tokenPath: editTokenPath,
      })
      // Re-read to get real auth status
      const config = await invoke<CloudProviderConfig>('cloud_redirect_get_provider_config')
      setProviderConfig(config)
      setEditProvider(config.provider || '')
      setEditTokenPath(config.tokenPath || '')
      setProviderSaveMsg(c.configurationSaved)
    } catch (e: any) {
      setProviderSaveError(true)
      setProviderSaveMsg(c.providerError.replace('{error}', String(e)))
    } finally {
      setProviderSaving(false)
      setTimeout(() => setProviderSaveMsg(''), 4000)
    }
  }

  async function handleBrowseTokenPath() {
    if (!isTauriRuntime()) return
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const isFolder = editProvider === 'folder'
      if (isFolder) {
        const selected = await open({ directory: true, title: c.selectSyncFolder })
        if (selected) setEditTokenPath(selected as string)
      } else {
        const selected = await open({
          filters: [{ name: 'JSON', extensions: ['json'] }],
          title: c.selectTokenFile,
        })
        if (selected) setEditTokenPath(selected as string)
      }
    } catch (e) {
      console.error('Browse failed:', e)
    }
  }

  async function handleConnectGoogle() {
    if (!isTauriRuntime()) return
    setProviderSaving(true)
    setProviderSaveError(false)
    setProviderSaveMsg(c.openingBrowser)
    try {
      await invoke('cloud_redirect_connect_google')
      await loadStfixerData()
      setProviderSaveMsg(c.authVerified)
    } catch (e: any) {
      setProviderSaveError(true)
      setProviderSaveMsg(c.providerError.replace('{error}', String(e)))
    } finally {
      setProviderSaving(false)
      setTimeout(() => setProviderSaveMsg(''), 4000)
    }
  }

  const [nativeAuthBusy, setNativeAuthBusy] = useState(false)

  const refreshNativeStatuses = useCallback(async () => {
    if (!isTauriRuntime()) return
    const [connected, entries] = await Promise.all([
      invoke<boolean>('global_is_google_drive_connected'),
      Promise.all(
        installed.map(async (game) => [game.id, await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })] as const),
      ),
    ])
    setNativeGoogleConnected(connected)
    setStatuses(Object.fromEntries(entries))
  }, [installed])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    listen<{ connected: boolean }>('launcher://cloud-save-auth-changed', (event) => {
      setNativeGoogleConnected(Boolean(event.payload?.connected))
      void refreshNativeStatuses().catch(() => undefined)
    }).then((dispose) => { unlisten = dispose }).catch(() => undefined)
    return () => unlisten?.()
  }, [refreshNativeStatuses])

  async function handleNativeConnectGoogle() {
    if (!isTauriRuntime()) return
    setNativeAuthBusy(true)
    setNativeBusyMessage(c.openingBrowser)
    try {
      await invoke('global_connect_google_drive')
      await refreshNativeStatuses()
      const connected = await invoke<boolean>('global_is_google_drive_connected')
      if (!connected) throw new Error('OAuth token was not persisted')
      setNativeBusyMessage(c.authVerified)
    } catch (e) {
      console.error('Native Google Connect Error:', e)
      setNativeGoogleConnected(false)
      setNativeBusyMessage(c.authFailed.replace('{error}', String(e)))
    } finally {
      setNativeAuthBusy(false)
    }
  }

  async function handleNativeDisconnectGoogle() {
    if (!isTauriRuntime()) return
    setNativeAuthBusy(true)
    try {
      await invoke('global_disconnect_google_drive')
      await refreshNativeStatuses()
    } catch (e) {
      setNativeBusyMessage(c.authFailed.replace('{error}', String(e)))
    } finally {
      setNativeAuthBusy(false)
    }
  }

  async function handleRetryPendingCloudSaves() {
    if (!isTauriRuntime()) return
    setNativeBusyMessage(c.checkingPending)
    try {
      await invoke('retry_pending_cloud_saves', { gameId: null })
      const entries = await Promise.all(
        installed.map(async (game) => [game.id, await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })] as const),
      )
      setStatuses(Object.fromEntries(entries))
      setNativeBusyMessage(c.pendingChecked)
    } catch (error) {
      setNativeBusyMessage(c.pendingFailed.replace('{error}', String(error)))
    }
  }

  async function handleRefreshCloudSaveMap() {
    if (!isTauriRuntime()) return
    setNativeBusyMessage(c.checkingDetection)
    try {
      const report = await invoke<{ message: string }>('refresh_cloud_save_map')
      setNativeBusyMessage(report.message)
    } catch (error) {
      setNativeBusyMessage(c.stableMapFallback.replace('{error}', String(error)))
    }
  }

  // Compute auth display
  const authLabel = providerConfig == null
    ? c.loading
    : providerConfig.authenticated
      ? c.authenticated
      : c.notAuthenticated

  const isAuthed = providerConfig?.authenticated ?? false

  return (
    <section className="cloud-overview">
      {activeMode === null ? (
        <>
          <header>
            <div className="cloud-overview-icon"><Cloud size={22} /></div>
            <div>
              <h1>{c.pageTitle}</h1>
              <p>{c.chooseMode}</p>
            </div>
          </header>
          <div className="cloud-mode-selection">
            <button className="cloud-mode-card" onClick={() => setActiveMode('native')}>
              <div className="cloud-mode-icon native-icon"><FolderSync size={24} /></div>
              <span className="cloud-mode-info">
                <strong>{c.nativeModeTitle}</strong>
                <span>{c.nativeModeDesc}</span>
              </span>
              <Info className="cloud-mode-help" size={15} aria-label={c.nativeModeInfo} />
            </button>
            <button className="cloud-mode-card" onClick={() => setActiveMode('stfixer')}>
              <div className="cloud-mode-icon stfixer-icon"><Wrench size={24} /></div>
              <span className="cloud-mode-info">
                <strong>{c.legacyModeTitle}</strong>
                <span>{c.legacyModeDesc}</span>
              </span>
              <Info className="cloud-mode-help" size={15} aria-label={c.legacyModeInfo} />
            </button>
          </div>
        </>
      ) : (
        <>
          <header className="cloud-overview-header-with-back">
            <button className="cloud-back-btn" onClick={() => setActiveMode(null)}>
              <ChevronLeft size={20} />
              <span>{c.back}</span>
            </button>
            <div>
              <h1>{activeMode === 'native' ? c.nativeTitle : c.legacyTitle}</h1>
              <p>{activeMode === 'native' ? c.nativeSubtitle : c.legacySubtitle}</p>
            </div>
          </header>

          {activeMode === 'stfixer' ? <CloudRedirectSettings /> : null}

          {false && activeMode === 'stfixer' && (
            <div className="cloud-redirect-panel">
              <header className="cr-header">
                <div className="cr-header-title">
                  <Wrench size={20} />
                  <h2>{c.stfixerConfig}</h2>
                </div>
                <div className="cr-status-badges">
                  {crStatus ? (
                    <>
                      <span className={`cr-badge ${crStatus?.steamRunning ? 'warning' : 'ok'}`}>
                        {c.steamLabel}: {crStatus?.steamRunning ? c.steamRunning : c.steamClosed}
                      </span>
                      <span className={`cr-badge ${crStatus?.steamVersionSupported ? 'ok' : 'error'}`}>
                        {c.versionLabel}: {crStatus?.steamVersion || c.versionUnknown} {crStatus?.steamVersionSupported ? '' : `(${c.unsupported})`}
                      </span>
                      <span className={`cr-badge ${crStatus?.stfixerApplied ? 'ok' : 'warning'}`}>
                        {c.stfixerLabel}: {crStatus?.stfixerApplied ? c.applied : c.notApplied}
                      </span>
                    </>
                  ) : (
                    <span className="cr-badge">{c.loading}</span>
                  )}
                </div>
              </header>

              <div className="cr-body">
                <p>
                  {c.stfixerExplain}
                </p>
                <div className="cr-actions">
                  <button 
                    className={`cr-btn primary ${stfixerBusy ? 'busy' : ''}`}
                    onClick={handleApplyStfixer}
                    disabled={stfixerBusy || !crStatus?.steamPath}
                  >
                    {stfixerBusy ? c.applyingPatches : c.applyPatches}
                  </button>
                  <label className="cr-checkbox-label">
                    <input 
                      type="checkbox" 
                      checked={installCoreIfMissing}
                      onChange={(e) => setInstallCoreIfMissing(e.target.checked)}
                      disabled={stfixerBusy}
                    />
                    {c.installCore}
                  </label>
                </div>

                {stfixerResult && (
                  <div className={`cr-result ${stfixerResult?.succeeded ? 'success' : 'error'}`}>
                    <div className="cr-result-header">
                      {stfixerResult?.succeeded ? <CheckCircle2 size={18} /> : <XCircle size={18} />}
                      <strong>{stfixerResult?.succeeded ? c.patchSuccess : c.patchFailed}</strong>
                    </div>
                    <div className="cr-terminal">
                      <Terminal size={14} className="cr-term-icon" />
                      <div className="cr-term-content">
                        {stfixerResult?.log.map((line, i) => (
                          <div key={i} className="cr-term-line">{line}</div>
                        ))}
                      </div>
                    </div>
                  </div>
                )}
              </div>

              {/* Cloud Provider — reads from CloudRedirect's actual config.json */}
              <div className="cr-provider-config">
                <header className="cr-header">
                  <div className="cr-header-title">
                    <Cloud size={20} />
                    <h2>{c.legacyProvider}</h2>
                  </div>
                  <button className="cr-btn-icon" onClick={loadStfixerData} title={c.refresh}>
                    <RefreshCw size={16} />
                  </button>
                </header>
                <div className="cr-body">
                  <p>{c.providerDesc}</p>
                  
                  <div className="cr-form-group">
                    <label>{c.provider}</label>
                    <div className="cr-select-wrapper">
                      <select 
                        value={editProvider} 
                        onChange={(e) => setEditProvider(e.target.value)}
                        className="cr-select"
                      >
                        <option value="">— {c.selectProvider} —</option>
                        {PROVIDER_VALUES.map((value) => (
                          <option key={value} value={value}>
                            {value === 'gdrive' ? c.providerGoogleDrive : value === 'onedrive' ? c.providerOneDrive : c.providerLocalFolder}
                          </option>
                        ))}
                      </select>
                      <ChevronDown size={16} className="cr-select-icon" />
                    </div>
                  </div>

                  {editProvider && (
                    <div className="cr-form-group">
                      <label>{editProvider === 'folder' ? c.syncFolderPath : c.tokenFilePath}</label>
                      <div className="cr-input-group">
                        <input 
                          type="text" 
                          value={editTokenPath}
                          onChange={(e) => setEditTokenPath(e.target.value)}
                          className="cr-input"
                          placeholder={editProvider === 'folder' ? c.folderPlaceholder : c.tokenPlaceholder}
                        />
                        <button className="cr-btn secondary" onClick={handleBrowseTokenPath}>
                          <FolderOpen size={14} />
                          {c.browse}
                        </button>
                      </div>
                    </div>
                  )}

                  <div className="cr-form-group">
                    <label>{c.authentication}</label>
                    <div className="cr-auth-card">
                      <div className="cr-auth-status">
                        <strong>{c.authentication}</strong>
                        <span>{authLabel}</span>
                      </div>
                      {isAuthed
                        ? <ShieldCheck size={24} className="cr-auth-icon cr-auth-ok" />
                        : <ShieldX size={24} className="cr-auth-icon cr-auth-none" />}
                    </div>
                  </div>

                  <div className="cr-actions" style={{ marginTop: '20px' }}>
                    <button
                      className="cr-btn primary cr-btn-with-icon"
                      onClick={handleSaveProviderConfig}
                      disabled={providerSaving || !editProvider}
                    >
                      <Save size={16} />
                      {providerSaving ? c.saving : c.saveConfiguration}
                    </button>
                    {editProvider === 'gdrive' && (
                      <button
                        className="cr-btn secondary cr-btn-with-icon"
                        onClick={handleConnectGoogle}
                        disabled={providerSaving}
                      >
                        <Cloud size={16} />
                        {c.signInGoogle}
                      </button>
                    )}
                  </div>

                  {providerSaveMsg && (
                    <div className={`cr-save-msg ${providerSaveError ? 'error' : 'success'}`}>
                      {providerSaveMsg}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          {activeMode === 'native' && (
            <div className="cloud-native-dashboard">
              <section className="cloud-native-hero">
                <div className="cloud-native-title">
                  <span className="cloud-native-hero-icon"><ShieldCheck size={22} /></span>
                  <div>
                    <h2>{c.automaticProtection}</h2>
                    <p>{c.automaticProtectionDesc}</p>
                  </div>
                </div>
                <div className={`cloud-native-account ${nativeGoogleConnected ? 'is-connected' : ''}`}>
                  <span>{nativeGoogleConnected ? <CheckCircle2 size={17} /> : <CloudOff size={17} />}</span>
                  <div>
                    <strong>{nativeGoogleConnected ? c.connected : c.notConnected}</strong>
                    <small>{nativeGoogleConnected ? c.connectedDesc : c.notConnectedDesc}</small>
                  </div>
                  {nativeGoogleConnected ? (
                    <button type="button" onClick={handleNativeDisconnectGoogle} disabled={nativeAuthBusy}>{c.disconnect}</button>
                  ) : (
                    <button type="button" className="primary" onClick={handleNativeConnectGoogle} disabled={nativeAuthBusy}>
                      <Cloud size={15} /> {c.connect}
                    </button>
                  )}
                </div>
              </section>

              <section className="cloud-native-metrics" aria-label={c.pageTitle}>
                <article>
                  <ShieldCheck size={18} />
                  <span><b>{nativeSummary.protectedGames}</b><small>{c.protectedGames}</small></span>
                </article>
                <article>
                  <Clock3 size={18} />
                  <span><b>{nativeSummary.pendingCount}</b><small>{c.pendingSync}</small></span>
                </article>
                <article className={nativeSummary.attentionGames ? 'needs-attention' : ''}>
                  <TriangleAlert size={18} />
                  <span><b>{nativeSummary.attentionGames}</b><small>{c.needsAttention}</small></span>
                </article>
                <article>
                  <DatabaseBackup size={18} />
                  <span><b>{formatBytes(nativeSummary.pendingBytes)}</b><small>{c.protectedLocally}</small></span>
                </article>
              </section>

              {nativeSummary.quota ? (
                <section className={`cloud-native-quota is-${nativeSummary.quota.state}`}>
                  <div>
                    <HardDrive size={18} />
                    <span>
                      <strong>{c.driveStorage}</strong>
                      <small>
                        {nativeSummary.quota.availableBytes == null
                          ? `${formatBytes(nativeSummary.quota.usageBytes)} ${c.used}`
                          : `${formatBytes(nativeSummary.quota.availableBytes)} ${c.available}`}
                      </small>
                    </span>
                  </div>
                  {quotaPercent(nativeSummary.quota) != null ? (
                    <div className="cloud-native-quota-bar">
                      <span style={{ width: `${quotaPercent(nativeSummary.quota)}%` }} />
                    </div>
                  ) : null}
                </section>
              ) : null}

              <section className="cloud-native-toolbar">
                <div>
                  <button type="button" className="primary" onClick={() => void handleRetryPendingCloudSaves()} disabled={!nativeGoogleConnected || nativeAuthBusy}>
                    <RotateCw size={15} /> {c.retryPending}
                  </button>
                  <button type="button" onClick={() => void handleRefreshCloudSaveMap()} disabled={nativeAuthBusy}>
                    <Settings2 size={15} /> {c.refreshDetection}
                  </button>
                </div>
                {nativeBusyMessage ? <p role="status" aria-live="polite">{nativeBusyMessage}</p> : null}
              </section>

              <section className="cloud-native-games">
                <header>
                  <div>
                    <h3>{c.installedGames}</h3>
                    <p>{c.installedGamesDesc}</p>
                  </div>
                </header>
                {installed.length === 0 ? (
                  <div className="cloud-overview-empty">
                    <CloudOff size={28} />
                    <strong>{c.noLocalGames}</strong>
                    <span>{c.noLocalGamesDesc}</span>
                  </div>
                ) : (
                  <div className="cloud-overview-list cloud-native-game-list">
                    {installed.map((game) => {
                      const status = statuses[game.id]
                      const view = cloudSavePresentation(status ?? null, c.states)
                      return (
                        <button type="button" key={game.id} onClick={() => onOpenGame(game.id)}>
                          {assets[game.gridAssetId] ? <img src={assets[game.gridAssetId]} alt="" /> : <div className="cloud-game-placeholder" />}
                          <span className="cloud-game-copy">
                            <strong>{game.title}</strong>
                            <small>{view.description}</small>
                            <span className="cloud-game-meta">
                              {status?.pendingOperationCount ? `${c.pendingLabel} · ${formatBytes(status.pendingUploadBytes)}` : c.gameProtected}
                              {status?.lastSyncAt ? ` · ${new Date(status.lastSyncAt).toLocaleString(locale)}` : ''}
                            </span>
                          </span>
                          <em className={`cloud-game-state tone-${view.tone}`}>
                            {view.tone === 'danger' ? <TriangleAlert size={15} /> : view.tone === 'success' ? <CheckCircle2 size={15} /> : <FolderSync size={15} />}
                            {view.title}
                          </em>
                        </button>
                      )
                    })}
                  </div>
                )}
              </section>
            </div>
          )}
        </>
      )}
    </section>
  )
}
