import { useEffect, useMemo, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, CloudOff, FolderSync, TriangleAlert, Wrench, CheckCircle2, XCircle, Terminal, ChevronLeft, ShieldCheck, ShieldX, RefreshCw, Save, ChevronDown, FolderOpen, Info, HardDrive, DatabaseBackup, Clock3, RotateCw, Settings2 } from 'lucide-react'
import type { CloudSaveStatus, GameCatalog, GameInstallState, CloudRedirectStatus, StfixerResult, CloudProviderConfig } from '../types'
import { isTauriRuntime } from '../lib/gameMeta'
import { formatBytes } from '../lib/format'
import { cloudSavePresentation, quotaPercent } from '../lib/cloudSaveStatus'

const PROVIDER_OPTIONS = [
  { value: 'gdrive', label: 'Google Drive' },
  { value: 'onedrive', label: 'OneDrive' },
  { value: 'folder', label: 'Local Folder' },
] as const

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
    const attentionGames = values.filter((status) => cloudSavePresentation(status).blocking).length
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
      setProviderSaveMsg('Configuration saved.')
    } catch (e: any) {
      setProviderSaveMsg(`Error: ${e}`)
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
        const selected = await open({ directory: true, title: 'Select sync folder' })
        if (selected) setEditTokenPath(selected as string)
      } else {
        const selected = await open({
          filters: [{ name: 'JSON', extensions: ['json'] }],
          title: 'Select token file',
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
    setProviderSaveMsg('Opening browser for Google Drive authentication...')
    try {
      await invoke('cloud_redirect_connect_google')
      await loadStfixerData()
      setProviderSaveMsg('Successfully authenticated with Google Drive.')
    } catch (e: any) {
      setProviderSaveMsg(`Error: ${e}`)
    } finally {
      setProviderSaving(false)
      setTimeout(() => setProviderSaveMsg(''), 4000)
    }
  }

  const [nativeAuthBusy, setNativeAuthBusy] = useState(false)
  async function handleNativeConnectGoogle() {
    if (!isTauriRuntime()) return
    setNativeAuthBusy(true)
    try {
      await invoke('global_connect_google_drive')
      setNativeGoogleConnected(true)
    } catch (e) {
      console.error('Native Google Connect Error:', e)
    } finally {
      setNativeAuthBusy(false)
    }
  }

  async function handleNativeDisconnectGoogle() {
    if (!isTauriRuntime()) return
    setNativeAuthBusy(true)
    try {
      await invoke('global_disconnect_google_drive')
      setNativeGoogleConnected(false)
    } catch (e) {
      console.error('Native Google Disconnect Error:', e)
    } finally {
      setNativeAuthBusy(false)
    }
  }

  async function handleRetryPendingCloudSaves() {
    if (!isTauriRuntime()) return
    setNativeBusyMessage('Đang kiểm tra các tác vụ chờ…')
    try {
      await invoke('retry_pending_cloud_saves', { gameId: null })
      const entries = await Promise.all(
        installed.map(async (game) => [game.id, await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })] as const),
      )
      setStatuses(Object.fromEntries(entries))
      setNativeBusyMessage('Đã kiểm tra xong. Tác vụ an toàn sẽ tiếp tục tự động.')
    } catch (error) {
      setNativeBusyMessage(`Chưa thể thử lại: ${String(error)}`)
    }
  }

  async function handleRefreshCloudSaveMap() {
    if (!isTauriRuntime()) return
    setNativeBusyMessage('Đang xác minh cấu hình nhận diện Save…')
    try {
      const report = await invoke<{ message: string }>('refresh_cloud_save_map')
      setNativeBusyMessage(report.message)
    } catch (error) {
      setNativeBusyMessage(`Vẫn đang dùng cấu hình ổn định trước đó: ${String(error)}`)
    }
  }

  // Compute auth display
  const authLabel = providerConfig == null
    ? 'Loading...'
    : !providerConfig.configFound
      ? 'Not configured — select a provider and save.'
      : !providerConfig.provider
        ? 'No provider selected.'
        : providerConfig.authenticated
          ? 'Authenticated.'
          : 'Not authenticated — sign in via CloudRedirect.'

  const isAuthed = providerConfig?.authenticated ?? false

  return (
    <section className="cloud-overview">
      {activeMode === null ? (
        <>
          <header>
            <div className="cloud-overview-icon"><Cloud size={22} /></div>
            <div>
              <h1>Cloud Saves</h1>
              <p>Select your preferred cloud save management mode.</p>
            </div>
          </header>
          <div className="cloud-mode-selection">
            <button className="cloud-mode-card" onClick={() => setActiveMode('native')}>
              <div className="cloud-mode-icon native-icon"><FolderSync size={36} /></div>
              <h3>
                Cloud Save
                <span className="cloud-mode-info" title="Chỉ áp dụng cho game cài local qua 0xoLemon; không dùng SteamTools/Lua." onClick={(e) => e.stopPropagation()}>
                  <Info size={16} />
                </span>
              </h3>
              <p>Sync game saves natively to Google Drive. Manage backup status and conflicts for your installed games.</p>
            </button>
            <button className="cloud-mode-card" onClick={() => setActiveMode('stfixer')}>
              <div className="cloud-mode-icon stfixer-icon"><Wrench size={36} /></div>
              <h3>
                STFixer
                <span className="cloud-mode-info" title="Chế độ riêng cho SteamTools/Lua; không dùng chung với Cloud Save local." onClick={(e) => e.stopPropagation()}>
                  <Info size={16} />
                </span>
              </h3>
              <p>Patch SteamTools to bypass the AppID 760 sync bug and configure CloudRedirect provider.</p>
            </button>
          </div>
        </>
      ) : (
        <>
          <header className="cloud-overview-header-with-back">
            <button className="cloud-back-btn" onClick={() => setActiveMode(null)}>
              <ChevronLeft size={20} />
              <span>Back</span>
            </button>
            <div>
              <h1>{activeMode === 'native' ? 'Cloud Save' : 'STFixer'}</h1>
              <p>{activeMode === 'native' ? 'Tự động bảo vệ save của game cài local bằng Google Drive.' : 'Quản lý STFixer cho game SteamTools/Lua; tách biệt với Cloud Save local.'}</p>
            </div>
          </header>

          {activeMode === 'stfixer' && (
            <div className="cloud-redirect-panel">
              <header className="cr-header">
                <div className="cr-header-title">
                  <Wrench size={20} />
                  <h2>STFixer Configuration</h2>
                </div>
                <div className="cr-status-badges">
                  {crStatus ? (
                    <>
                      <span className={`cr-badge ${crStatus.steamRunning ? 'warning' : 'ok'}`}>
                        Steam: {crStatus.steamRunning ? 'Running' : 'Closed'}
                      </span>
                      <span className={`cr-badge ${crStatus.steamVersionSupported ? 'ok' : 'error'}`}>
                        Version: {crStatus.steamVersion || 'Unknown'} {crStatus.steamVersionSupported ? '' : '(Unsupported)'}
                      </span>
                      <span className={`cr-badge ${crStatus.stfixerApplied ? 'ok' : 'warning'}`}>
                        STFixer: {crStatus.stfixerApplied ? 'Applied' : 'Not Applied'}
                      </span>
                    </>
                  ) : (
                    <span className="cr-badge">Loading status...</span>
                  )}
                </div>
              </header>

              <div className="cr-body">
                <p>
                  CloudRedirect patches SteamTools to allow proper cloud saves for non-owned (lua) games, bypassing the AppID 760 (Screenshots) limitation.
                </p>
                <div className="cr-actions">
                  <button 
                    className={`cr-btn primary ${stfixerBusy ? 'busy' : ''}`}
                    onClick={handleApplyStfixer}
                    disabled={stfixerBusy || !crStatus?.steamPath}
                  >
                    {stfixerBusy ? 'Applying Patch...' : 'Apply STFixer Patches'}
                  </button>
                  <label className="cr-checkbox-label">
                    <input 
                      type="checkbox" 
                      checked={installCoreIfMissing}
                      onChange={(e) => setInstallCoreIfMissing(e.target.checked)}
                      disabled={stfixerBusy}
                    />
                    Tự động tải &amp; cài đặt SteamTools Core (nếu chưa có)
                  </label>
                </div>

                {stfixerResult && (
                  <div className={`cr-result ${stfixerResult.succeeded ? 'success' : 'error'}`}>
                    <div className="cr-result-header">
                      {stfixerResult.succeeded ? <CheckCircle2 size={18} /> : <XCircle size={18} />}
                      <strong>{stfixerResult.succeeded ? 'Patch Applied Successfully' : 'Patch Failed'}</strong>
                    </div>
                    <div className="cr-terminal">
                      <Terminal size={14} className="cr-term-icon" />
                      <div className="cr-term-content">
                        {stfixerResult.log.map((line, i) => (
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
                    <h2>Cloud Provider</h2>
                  </div>
                  <button className="cr-btn-icon" onClick={loadStfixerData} title="Refresh">
                    <RefreshCw size={16} />
                  </button>
                </header>
                <div className="cr-body">
                  <p>Configure your Steam Cloud provider. Google Drive/OneDrive/Local Folder only for now.</p>
                  
                  <div className="cr-form-group">
                    <label>Provider</label>
                    <div className="cr-select-wrapper">
                      <select 
                        value={editProvider} 
                        onChange={(e) => setEditProvider(e.target.value)}
                        className="cr-select"
                      >
                        <option value="">— Select Provider —</option>
                        {PROVIDER_OPTIONS.map((opt) => (
                          <option key={opt.value} value={opt.value}>{opt.label}</option>
                        ))}
                      </select>
                      <ChevronDown size={16} className="cr-select-icon" />
                    </div>
                  </div>

                  {editProvider && (
                    <div className="cr-form-group">
                      <label>{editProvider === 'folder' ? 'Sync Folder Path' : 'Token File Path'}</label>
                      <div className="cr-input-group">
                        <input 
                          type="text" 
                          value={editTokenPath}
                          onChange={(e) => setEditTokenPath(e.target.value)}
                          className="cr-input"
                          placeholder={editProvider === 'folder' ? 'Select a local folder...' : 'Path to token file...'}
                        />
                        <button className="cr-btn secondary" onClick={handleBrowseTokenPath}>
                          <FolderOpen size={14} />
                          Browse
                        </button>
                      </div>
                    </div>
                  )}

                  <div className="cr-form-group">
                    <label>Authentication</label>
                    <div className="cr-auth-card">
                      <div className="cr-auth-status">
                        <strong>Authentication Status</strong>
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
                      {providerSaving ? 'Saving...' : 'Save Configuration'}
                    </button>
                    {editProvider === 'gdrive' && (
                      <button
                        className="cr-btn secondary cr-btn-with-icon"
                        onClick={handleConnectGoogle}
                        disabled={providerSaving}
                      >
                        <Cloud size={16} />
                        Sign in with Google Drive
                      </button>
                    )}
                  </div>

                  {providerSaveMsg && (
                    <div className={`cr-save-msg ${providerSaveMsg.startsWith('Error') ? 'error' : 'success'}`}>
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
                <div>
                  <span className="cloud-native-hero-icon"><ShieldCheck size={22} /></span>
                  <div>
                    <h2>Bảo vệ Save tự động</h2>
                    <p>Launcher tự nhận diện, sao lưu và đồng bộ trước/sau khi chơi. Người dùng không cần chọn file thủ công.</p>
                  </div>
                </div>
                <div className={`cloud-native-account ${nativeGoogleConnected ? 'is-connected' : ''}`}>
                  <span>{nativeGoogleConnected ? <CheckCircle2 size={17} /> : <CloudOff size={17} />}</span>
                  <div>
                    <strong>{nativeGoogleConnected ? 'Google Drive đã kết nối' : 'Chưa kết nối Google Drive'}</strong>
                    <small>{nativeGoogleConnected ? 'Dữ liệu nằm trong vùng riêng của launcher.' : 'Save vẫn được bảo vệ cục bộ trên máy này.'}</small>
                  </div>
                  {nativeGoogleConnected ? (
                    <button type="button" onClick={handleNativeDisconnectGoogle} disabled={nativeAuthBusy}>Ngắt kết nối</button>
                  ) : (
                    <button type="button" className="primary" onClick={handleNativeConnectGoogle} disabled={nativeAuthBusy}>
                      <Cloud size={15} /> Kết nối
                    </button>
                  )}
                </div>
              </section>

              <section className="cloud-native-metrics" aria-label="Tổng quan Cloud Save">
                <article>
                  <ShieldCheck size={18} />
                  <span><b>{nativeSummary.protectedGames}</b><small>game được bảo vệ</small></span>
                </article>
                <article>
                  <Clock3 size={18} />
                  <span><b>{nativeSummary.pendingCount}</b><small>Đang chờ đồng bộ</small></span>
                </article>
                <article className={nativeSummary.attentionGames ? 'needs-attention' : ''}>
                  <TriangleAlert size={18} />
                  <span><b>{nativeSummary.attentionGames}</b><small>cần bạn kiểm tra</small></span>
                </article>
                <article>
                  <DatabaseBackup size={18} />
                  <span><b>{formatBytes(nativeSummary.pendingBytes)}</b><small>đang bảo vệ trên máy</small></span>
                </article>
              </section>

              {nativeSummary.quota ? (
                <section className={`cloud-native-quota is-${nativeSummary.quota.state}`}>
                  <div>
                    <HardDrive size={18} />
                    <span>
                      <strong>Dung lượng Google Drive</strong>
                      <small>
                        {nativeSummary.quota.availableBytes == null
                          ? `${formatBytes(nativeSummary.quota.usageBytes)} đã dùng`
                          : `${formatBytes(nativeSummary.quota.availableBytes)} còn trống`}
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
                    <RotateCw size={15} /> Thử lại tác vụ chờ
                  </button>
                  <button type="button" onClick={() => void handleRefreshCloudSaveMap()} disabled={nativeAuthBusy}>
                    <Settings2 size={15} /> Cập nhật nhận diện Save
                  </button>
                </div>
                {nativeBusyMessage ? <p role="status" aria-live="polite">{nativeBusyMessage}</p> : null}
              </section>

              <section className="cloud-native-games">
                <header>
                  <div>
                    <h3>Game đã cài</h3>
                    <p>Mỗi game dùng đường dẫn Save đã được xác minh; chỉ hiện chi tiết kỹ thuật khi cần.</p>
                  </div>
                </header>
                {installed.length === 0 ? (
                  <div className="cloud-overview-empty">
                    <CloudOff size={28} />
                    <strong>Chưa có game local</strong>
                    <span>Cloud Save sẽ tự xuất hiện sau khi bạn cài game bằng launcher.</span>
                  </div>
                ) : (
                  <div className="cloud-overview-list cloud-native-game-list">
                    {installed.map((game) => {
                      const status = statuses[game.id]
                      const view = cloudSavePresentation(status ?? null)
                      return (
                        <button type="button" key={game.id} onClick={() => onOpenGame(game.id)}>
                          {assets[game.gridAssetId] ? <img src={assets[game.gridAssetId]} alt="" /> : <div className="cloud-game-placeholder" />}
                          <span className="cloud-game-copy">
                            <strong>{game.title}</strong>
                            <small>{status?.lastMessage || view.description}</small>
                            <span className="cloud-game-meta">
                              {status?.pendingOperationCount ? `Đang chờ đồng bộ · ${formatBytes(status.pendingUploadBytes)}` : 'Được bảo vệ trên máy này'}
                              {status?.lastSyncAt ? ` · ${new Date(status.lastSyncAt).toLocaleString('vi-VN')}` : ''}
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
