import { useEffect, useMemo, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, CloudOff, FolderSync, TriangleAlert, Wrench, CheckCircle2, XCircle, Terminal, ChevronLeft, ShieldCheck, ShieldX, RefreshCw, Save, ChevronDown, FolderOpen } from 'lucide-react'
import type { CloudSaveStatus, GameCatalog, GameInstallState, CloudRedirectStatus, StfixerResult, CloudProviderConfig } from '../types'
import { isTauriRuntime } from '../lib/gameMeta'

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

  // --- Cloud Save status polling ---
  useEffect(() => {
    for (const game of installed) onRequestAsset(game.id, game.gridAssetId)
    if (!isTauriRuntime()) return
    let disposed = false
    Promise.all(
      installed.map(async (game) => {
        const status = await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })
        return [game.id, status] as const
      }),
    )
      .then((entries) => {
        if (!disposed) setStatuses(Object.fromEntries(entries))
      })
      .catch(() => undefined)
    return () => {
      disposed = true
    }
  }, [installed, installedIds, onRequestAsset])

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
              <h3>Cloud Save</h3>
              <p>Sync game saves natively to Google Drive. Manage backup status and conflicts for your installed games.</p>
            </button>
            <button className="cloud-mode-card" onClick={() => setActiveMode('stfixer')}>
              <div className="cloud-mode-icon stfixer-icon"><Wrench size={36} /></div>
              <h3>STFixer</h3>
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
              <p>{activeMode === 'native' ? 'Backup status and conflicts for installed games.' : 'Manage STFixer patches & Cloud Provider.'}</p>
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
            installed.length === 0 ? (
              <div className="cloud-overview-empty">
                <CloudOff size={28} />
                <strong>No installed games</strong>
                <span>Cloud save controls become available from each installed game in Library.</span>
              </div>
            ) : (
              <div className="cloud-overview-list">
                {installed.map((game) => {
                  const status = statuses[game.id]
                  const hasConflict = Boolean(status?.conflicts.length)
                  return (
                    <button type="button" key={game.id} onClick={() => onOpenGame(game.id)}>
                      {assets[game.gridAssetId] ? <img src={assets[game.gridAssetId]} alt="" /> : <div />}
                      <span>
                        <strong>{game.title}</strong>
                        <small>{status?.lastMessage || (status?.enabled ? 'Cloud save ready' : 'Cloud save is disabled')}</small>
                      </span>
                      <em className={hasConflict ? 'is-conflict' : status?.enabled ? 'is-ready' : ''}>
                        {hasConflict ? <TriangleAlert size={15} /> : <FolderSync size={15} />}
                        {hasConflict ? `${status.conflicts.length} conflict${status.conflicts.length === 1 ? '' : 's'}` : status?.enabled ? 'Enabled' : 'Disabled'}
                      </em>
                    </button>
                  )
                })}
              </div>
            )
          )}
        </>
      )}
    </section>
  )
}
