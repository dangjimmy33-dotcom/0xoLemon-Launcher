import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { Archive, Check, FolderOpen, Loader2, RefreshCw, RotateCcw, Search, Wrench } from 'lucide-react'
import './GseUcStandaloneView.css'
import { UnifiedSearchOverlay, UnifiedSearchResult } from './UnifiedSearchOverlay'

type Engine = 'gse' | 'uc' | 'rune'
type Workspace = 'setup' | 'saves'

type Config = {
  appId: number
  gameFolder: string
  engine: Engine
  gseVariant: 'regular' | 'experimental' | 'coldclient' | 'coldclient_simple'
  networkMode: 'singleplayer' | 'strict_offline' | 'lan'
  steamstubMode: 'auto' | 'steamless' | 'rune' | 'uc_runtime' | 'disabled'
  accountName: string
  saveMode: 'gse' | 'portable' | 'custom'
  customSavePath: string
  ucSpoofAppid: number
  ucPlugins: string[]
  coldclientRenderer: boolean
  coldclientExtra: boolean
  overlay: boolean
  overlayAchievementNotifications: boolean
  overlayAchievementProgress: boolean
  overlayFriendNotifications: boolean
  overlayIcons: boolean
  overlayUserInfo: boolean
  overlayWarnings: boolean
  overlayFps: boolean
  overlayFrametime: boolean
  overlayShowPlaytime: boolean
  overlayPlaytime: boolean
  overlayPosition: string
  overlayHotkey: string
  overlayFontSize: number
  overlayIconSize: number
  overlayRounding: number
  overlayAnimation: number
  overlayAchievementDuration: number
  overlayHookDelay: number
  overlayRendererTimeout: number
  overlayDinputBridge: boolean
  officialGenerator: boolean
  reducedMotion: boolean
  runeProfile: 'regular' | 'steakclient' | 'steamclient'
  runeUsername: string
  runeLanguage: string
  runeUnlockAllDlcs: boolean
  runeLobby: boolean
  runeOverlays: boolean
  runeOffline: boolean
}

type SetupResult = {
  success: boolean
  gameName: string
  installedTargets: string[]
  backupManifest: string
  message: string
  logs: string[]
}

type SaveEntry = {
  appId: string
  gameName?: string
  headerImageUrl?: string
  path: string
  sizeBytes: number
  modifiedUnix: number
  localBackupCount?: number
}

type ResourceUpdateStatus = {
  gse?: string
  uc?: string
  rune?: string
  steamless?: string
  migrate?: string
}

function resourceStatusSuffix(value?: string) {
  if (!value) return ''
  return value.toLowerCase().startsWith('check failed:') ? ` · ${value}` : ` · latest ${value}`
}

const DEFAULTS: Config = {
  appId: 0,
  gameFolder: '',
  engine: 'gse',
  gseVariant: 'regular',
  networkMode: 'singleplayer',
  steamstubMode: 'auto',
  accountName: '0xoLemon',
  saveMode: 'gse',
  customSavePath: '',
  ucSpoofAppid: 480,
  ucPlugins: ['auto'],
  coldclientRenderer: true,
  coldclientExtra: false,
  overlay: false,
  overlayAchievementNotifications: true,
  overlayAchievementProgress: false,
  overlayFriendNotifications: true,
  overlayIcons: true,
  overlayUserInfo: false,
  overlayWarnings: true,
  overlayFps: false,
  overlayFrametime: false,
  overlayShowPlaytime: false,
  overlayPlaytime: false,
  overlayPosition: 'bot_right',
  overlayHotkey: 'shift + tab',
  overlayFontSize: 20,
  overlayIconSize: 64,
  overlayRounding: 10,
  overlayAnimation: 0.35,
  overlayAchievementDuration: 7,
  overlayHookDelay: 0,
  overlayRendererTimeout: 15,
  overlayDinputBridge: false,
  officialGenerator: true,
  reducedMotion: false,
  runeProfile: 'regular',
  runeUsername: 'RUNE',
  runeLanguage: 'english',
  runeUnlockAllDlcs: false,
  runeLobby: true,
  runeOverlays: true,
  runeOffline: false,
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button type="button" className={`gse-toggle${checked ? ' checked' : ''}`} aria-pressed={checked} aria-label={label} onClick={() => onChange(!checked)}>
      <span />
    </button>
  )
}

function SettingRow({ title, subtitle, checked, onChange }: { title: string; subtitle: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="gse-setting-row">
      <div><strong>{title}</strong><small>{subtitle}</small></div>
      <Toggle checked={checked} onChange={onChange} label={title} />
    </div>
  )
}

function Segmented<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void }) {
  return <div className="gse-segmented">{options.map((item) => <button type="button" key={item.value} className={value === item.value ? 'active' : ''} onClick={() => onChange(item.value)}>{item.label}</button>)}</div>
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="gse-field"><span>{label}</span>{children}{hint ? <small>{hint}</small> : null}</label>
}

function bytesLabel(value: number) {
  if (value < 1024) return `${value} B`
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MB`
  return `${(value / 1024 ** 3).toFixed(2)} GB`
}

export function GseUcStandaloneView() {
  const [workspace, setWorkspace] = useState<Workspace>('setup')
  const [config, setConfig] = useState<Config>(DEFAULTS)
  const [busy, setBusy] = useState<'setup' | 'restore' | 'migrate' | null>(null)
  const [progress, setProgress] = useState(0)
  const [progressText, setProgressText] = useState('Ready')
  const [logs, setLogs] = useState<string[]>([])
  const [activityOpen, setActivityOpen] = useState(true)
  const [message, setMessage] = useState<string | null>(null)
  const [showCompletedModal, setShowCompletedModal] = useState(false)
  const [saveRoot, setSaveRoot] = useState('')
  const [saveSearch, setSaveSearch] = useState('')
  const [gseSearchOverlayOpen, setGseSearchOverlayOpen] = useState(false)
  const [saves, setSaves] = useState<SaveEntry[]>([])
  const [savesBusy, setSavesBusy] = useState(false)
  const [resourceUpdateStatus, setResourceUpdateStatus] = useState<ResourceUpdateStatus | null>(null)
  const [resourceCheckBusy, setResourceCheckBusy] = useState(false)
  const logRef = useRef<HTMLPreElement>(null)

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight
    }
  }, [logs])

  useEffect(() => {
    void invoke<Config>('gse_auto_setup_load_config').then((saved) => setConfig({ ...DEFAULTS, ...saved })).catch(() => {})
    let stop: (() => void) | undefined
    void listen<{ percent: number; message: string }>('gse-auto-setup://progress', (event) => {
      setProgress(event.payload.percent)
      setProgressText(event.payload.message)
      setActivityOpen(true)
      setLogs((current) => [...current.slice(-249), event.payload.message])
    }).then((unlisten) => { stop = unlisten })
    return () => stop?.()
  }, [])

  const change = <K extends keyof Config>(key: K, value: Config[K]) => {
    setConfig((current) => ({ ...current, [key]: value }))
    setMessage(null)
  }

  const chooseGameFolder = async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: 'Select game folder' })
    if (typeof selected === 'string') change('gameFolder', selected)
  }

  const chooseCustomSave = async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: 'Select save folder' })
    if (typeof selected === 'string') change('customSavePath', selected)
  }

  const runSetup = async () => {
    if (busy) return
    if (!config.appId || !config.gameFolder.trim()) {
      setMessage('Enter the real Steam AppID and select the game folder first.')
      return
    }
    setBusy('setup')
    setActivityOpen(true)
    setProgress(0)
    setProgressText('Starting setup…')
    setLogs([])
    setMessage(null)
    try {
      await invoke('gse_auto_setup_save_config', { config })
      const result = await invoke<SetupResult>('gse_auto_setup_run', { config })
      setLogs((current) => [...current, ...result.logs])
      setMessage(`${result.gameName}: ${result.message}`)
      setProgress(100)
      setProgressText('Ready')
      setShowCompletedModal(true)
    } catch (error) {
      setMessage(String(error))
      setLogs((current) => [...current, `ERROR: ${String(error)}`])
      setProgressText('Setup failed')
    } finally {
      setBusy(null)
    }
  }

  const restore = async () => {
    if (!config.gameFolder.trim() || busy) return
    setBusy('restore')
    setActivityOpen(true)
    setMessage(null)
    try {
      const text = await invoke<string>('gse_auto_setup_restore', { gameFolder: config.gameFolder })
      setMessage(text)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const migrate = async () => {
    if (busy) return
    setBusy('migrate')
    try { await invoke('gse_auto_setup_launch_migrate') }
    catch (error) { setMessage(String(error)) }
    finally { setBusy(null) }
  }

  const checkResourceUpdates = async () => {
    if (resourceCheckBusy) return
    setResourceCheckBusy(true)
    setActivityOpen(true)
    setMessage(null)
    setLogs((current) => [...current.slice(-249), 'Checking component releases…'])
    try {
      const status = await invoke<ResourceUpdateStatus>('gse_auto_setup_check_updates')
      setResourceUpdateStatus(status)
      const summary = [
        `GSE: ${status.gse || '?'}`,
        `UC Online: ${status.uc || '?'}`,
        `RUNE SteamStub: ${status.rune || '?'}`,
        `Steamless: ${status.steamless || '?'}`,
        `migrate_gse: ${status.migrate || '?'}`,
      ]
      setLogs((current) => [...current.slice(-244), ...summary, 'Component release check complete. Updates are installed on demand during Setup.'])
      setMessage('Component release check complete.')
    } catch (error) {
      const text = `Resource check failed: ${String(error)}`
      setMessage(text)
      setLogs((current) => [...current.slice(-249), text])
    } finally {
      setResourceCheckBusy(false)
    }
  }

  const refreshSaves = useCallback(async () => {
    setSavesBusy(true)
    try { setSaves(await invoke<SaveEntry[]>('gse_auto_setup_list_saves', { root: saveRoot.trim() || null })) }
    catch (error) { setMessage(String(error)) }
    finally { setSavesBusy(false) }
  }, [saveRoot])

  useEffect(() => { if (workspace === 'saves') void refreshSaves() }, [workspace, refreshSaves])

  const [saveSort, setSaveSort] = useState<'modified' | 'name' | 'appid' | 'size'>('modified')

  const filteredSaves = useMemo(() => {
    const q = saveSearch.trim().toLowerCase()
    let list = q ? saves.filter((row) => row.appId.includes(q) || (row.gameName || '').toLowerCase().includes(q) || row.path.toLowerCase().includes(q)) : [...saves]
    if (saveSort === 'name') list.sort((a, b) => (a.gameName || a.appId).localeCompare(b.gameName || b.appId))
    else if (saveSort === 'appid') list.sort((a, b) => Number(a.appId) - Number(b.appId))
    else if (saveSort === 'size') list.sort((a, b) => b.sizeBytes - a.sizeBytes)
    else list.sort((a, b) => b.modifiedUnix - a.modifiedUnix)
    return list
  }, [saveSearch, saves, saveSort])

  useEffect(() => {
    if (workspace !== 'saves') return
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setGseSearchOverlayOpen(true)
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [workspace])

  const openSaveFolder = async (folderPath: string) => {
    try { await invoke('gse_auto_setup_open_folder', { path: folderPath }) }
    catch (error) { setMessage(String(error)) }
  }

  const captureSnapshot = async (row: SaveEntry) => {
    try {
      const path = await invoke<string>('gse_auto_setup_create_snapshot', { savePath: row.path, appId: row.appId })
      setMessage(`Snapshot created: ${path}`)
    } catch (error) { setMessage(String(error)) }
  }

  const engineIsGse = config.engine === 'gse'
  const engineIsUc = config.engine === 'uc'
  const engineIsRune = config.engine === 'rune'

  return (
    <section className="gse-page">
      <header className="gse-page-header">
        <div className="gse-page-header-inner">
          <div><h1>GSE / UC Setup</h1><p>Portable Steam integration workspace · GSE single-player/offline/LAN · UC Online2 Spacewar</p></div>
          <div className="gse-head-actions"><button type="button" className="active">GSE</button><button type="button">Hybrid resources</button></div>
        </div>
      </header>

      <div className="gse-coming-soon-banner">
        <div className="gse-coming-soon-badge-pill">COMING SOON</div>
        <div className="gse-coming-soon-text">
          <strong>Tính năng GSE / UC Setup đang được tạm khóa để tiếp tục hoàn thiện</strong>
          <span>Toàn bộ tab GSE / UC Setup tạm thời hiển thị ở trạng thái xem trước (preview) và sẽ được kích hoạt chính thức trong bản cập nhật tới.</span>
        </div>
      </div>

      <div className="gse-workspace-tabs">
        <button type="button" className={workspace === 'setup' ? 'active' : ''} onClick={() => setWorkspace('setup')}>Setup &amp; Emulator</button>
        <button type="button" className={workspace === 'saves' ? 'active' : ''} onClick={() => setWorkspace('saves')}>Savegame Manager</button>
      </div>

      {workspace === 'saves' ? (
        <div className="gse-content gse-save-manager">
          <article className="gse-card strong">
            <div className="gse-card-title"><div><h2>Savegame Manager</h2><p>Manage complete GSE AppID save folders, local snapshots and Google Drive backups.</p></div><button type="button" onClick={() => void refreshSaves()} disabled={savesBusy}>{savesBusy ? <Loader2 className="spin" /> : <RefreshCw />} Refresh saves</button></div>
            <div className="gse-row"><input value={saveRoot} onChange={(e) => setSaveRoot(e.currentTarget.value)} placeholder="%APPDATA%\\GSE Saves" /><button type="button" onClick={() => setSaveRoot('')}>Use GSE default</button><button type="button" onClick={async () => { const p = await openDialog({ directory: true, multiple: false }); if (typeof p === 'string') setSaveRoot(p) }}><FolderOpen /> Browse source</button></div>
            <div className="gse-row">
              <input value={saveSearch} onFocus={() => setGseSearchOverlayOpen(true)} onClick={() => setGseSearchOverlayOpen(true)} onChange={(e) => setSaveSearch(e.currentTarget.value)} placeholder="Search game name or AppID…" />
              <select value={saveSort} onChange={(e) => setSaveSort(e.currentTarget.value as any)}>
                <option value="modified">Recently modified</option>
                <option value="name">Game name</option>
                <option value="appid">AppID</option>
                <option value="size">Save size</option>
              </select>
            </div>
          </article>
          <UnifiedSearchOverlay
            open={gseSearchOverlayOpen}
            query={saveSearch}
            onQueryChange={setSaveSearch}
            onClose={() => setGseSearchOverlayOpen(false)}
            onSubmit={() => setGseSearchOverlayOpen(false)}
            placeholder="Search game name or AppID…"
            ariaLabel="Search GSE savegames"
            resultCount={filteredSaves.length}
            resultsHint="GSE save folders found on this device"
            discoveryTitle="Recent GSE save folders"
            discoveryHint="Search by Steam AppID, detected game name, or save folder path."
            historyKey="0xo.gseSaveSearchHistory"
          >
            {filteredSaves.length ? filteredSaves.slice(0, 36).map((row) => (
              <UnifiedSearchResult
                key={`gse-save-search-${row.path}`}
                title={row.gameName || `AppID ${row.appId}`}
                subtitle={`AppID ${row.appId}`}
                matchLabel={`${bytesLabel(row.sizeBytes)} · ${row.modifiedUnix ? new Date(row.modifiedUnix * 1000).toLocaleString() : 'N/A'}`}
                imageUrl={row.headerImageUrl || `https://cdn.akamai.steamstatic.com/steam/apps/${row.appId}/header.jpg`}
                onClick={() => {
                  setSaveSearch(row.gameName || row.appId)
                  setGseSearchOverlayOpen(false)
                }}
              />
            )) : (
              <div className="store-search-empty">
                <Search size={28} />
                <strong>No matching save folders</strong>
                <span>Try another game name or exact Steam AppID.</span>
              </div>
            )}
          </UnifiedSearchOverlay>

          <article className="gse-card"><h3>Google Drive</h3><p className="gse-muted">Cloud backup remains handled by the launcher's Google Drive integration. Local GSE snapshots below work without signing in.</p></article>
          <div className="gse-save-list">
            {savesBusy ? <div className="gse-empty"><Loader2 className="spin" /> Scanning GSE Saves…</div> : filteredSaves.length ? filteredSaves.map((row) => (
              <article key={row.path} className="gse-save-row">
                <div className="gse-save-thumb-wrap">
                  <img src={row.headerImageUrl || `https://cdn.akamai.steamstatic.com/steam/apps/${row.appId}/header.jpg`} alt="" onError={(e) => { e.currentTarget.style.display = 'none' }} />
                </div>
                <div className="gse-save-info">
                  <strong>AppID {row.appId}</strong>
                  <span className="gse-save-path">{row.path}</span>
                  <small>{bytesLabel(row.sizeBytes)} · {row.modifiedUnix ? new Date(row.modifiedUnix * 1000).toLocaleString() : 'N/A'}</small>
                </div>
                <div className="gse-save-meta">
                  <button type="button" onClick={() => void openSaveFolder(row.path)}><FolderOpen /> Open folder</button>
                  <button type="button" onClick={() => void captureSnapshot(row)}><Archive /> Snapshot</button>
                </div>
              </article>
            )) : <div className="gse-empty">No GSE save folders found.</div>}
          </div>
        </div>
      ) : (
        <div className="gse-content">
          <article className="gse-card strong">
            <h3>Game</h3><p className="gse-muted">The API key is required for GSE metadata enrichment; UC Online can run without it.</p>
            <div className="gse-grid game-grid">
              <Field label="Real AppID"><input inputMode="numeric" value={config.appId || ''} placeholder="Steam AppID" onChange={(e) => change('appId', Number(e.currentTarget.value.replace(/\D/g, '')) || 0)} /></Field>
              <Field label="Game folder"><div className="gse-input-action"><input value={config.gameFolder} onChange={(e) => change('gameFolder', e.currentTarget.value)} placeholder="D:\\Games\\Game" /><button type="button" onClick={() => void chooseGameFolder()}>Browse</button></div></Field>
            </div>
          </article>

          <article className="gse-card strong">
            <h3>Engine</h3><p className="gse-muted">GSE, UC Online2, and RUNE AutoCracker are independent deployment engines; settings are context-sensitive.</p>
            <div className="gse-engine-grid">
              {([['gse','GSE','Offline / LAN emulator'],['uc','UC Online','Steam client + Spacewar'],['rune','RUNE AutoCracker','Regular / Steak / Steamclient']] as const).map(([value,title,sub]) => <button type="button" key={value} className={config.engine === value ? 'active' : ''} onClick={() => change('engine', value)}><strong>{title}</strong><span>{sub}</span></button>)}
            </div>
            {engineIsGse ? <div className="gse-inset"><h3>GSE deployment</h3><p className="gse-muted">Regular replaces Steam API; Experimental adds native overlay; ColdClient uses the steamclient loader; ColdClient v1 drops steamclient DLLs directly.</p><Segmented value={config.gseVariant} onChange={(value) => change('gseVariant', value)} options={[{value:'regular',label:'Regular'},{value:'experimental',label:'Experimental'},{value:'coldclient',label:'ColdClient'},{value:'coldclient_simple',label:'ColdClient v1'}]} /><Field label="Connectivity" hint="Single-player is recommended. Strict offline reports Steam offline. LAN enables GSE networking."><Segmented value={config.networkMode} onChange={(value) => change('networkMode', value)} options={[{value:'singleplayer',label:'Single-player'},{value:'strict_offline',label:'Strict offline'},{value:'lan',label:'LAN'}]} /></Field></div> : null}
            {engineIsUc ? <div className="gse-inset"><h3>UC Online2</h3><p className="gse-muted">Runs with the real Steam client while spoofing the multiplayer AppID.</p><Field label="Spoof AppID" hint="Spacewar 480 is the default."><input inputMode="numeric" value={config.ucSpoofAppid} onChange={(e) => change('ucSpoofAppid', Number(e.currentTarget.value) || 480)} /></Field>{['auto','eos','photon','playfab','coherence'].map((plugin) => <SettingRow key={plugin} title={plugin === 'auto' ? 'Auto detect' : plugin === 'eos' ? 'EOS' : plugin[0].toUpperCase()+plugin.slice(1)} subtitle={plugin === 'auto' ? 'Scan common backend DLLs automatically.' : `Deploy ${plugin} networking plugin.`} checked={config.ucPlugins.includes(plugin)} onChange={(checked) => change('ucPlugins', checked ? [...config.ucPlugins, plugin] : config.ucPlugins.filter((x) => x !== plugin))} />)}</div> : null}
            {engineIsRune ? <div className="gse-inset"><h3>RUNE deployment profile</h3><p className="gse-muted">Regular replaces Steam API; Steakclient uses winmm proxy loader; Steamclient hooks binary with overlay renderer.</p><Segmented value={config.runeProfile} onChange={(value) => change('runeProfile', value)} options={[{value:'regular',label:'Regular Emu'},{value:'steakclient',label:'Steakclient'},{value:'steamclient',label:'Steamclient'}]} /><div className="gse-grid two"><Field label="Username"><input value={config.runeUsername} onChange={(e) => change('runeUsername', e.currentTarget.value)} /></Field><Field label="Language"><input value={config.runeLanguage} onChange={(e) => change('runeLanguage', e.currentTarget.value)} /></Field></div><SettingRow title="Unlock All DLCs" subtitle="Set DLCUnlockall=1 in config." checked={config.runeUnlockAllDlcs} onChange={(v) => change('runeUnlockAllDlcs', v)} /><SettingRow title="Lobby Enabled" subtitle="Enable Steam lobby support." checked={config.runeLobby} onChange={(v) => change('runeLobby', v)} /><SettingRow title="Overlays" subtitle="Enable Steam overlay rendering." checked={config.runeOverlays} onChange={(v) => change('runeOverlays', v)} /><SettingRow title="Offline Mode" subtitle="Force emulator offline flag." checked={config.runeOffline} onChange={(v) => change('runeOffline', v)} /></div> : null}
          </article>

          <div className="gse-grid two">
            <article className="gse-card"><h3>SteamStub handling</h3><p className="gse-muted">Steamless edits the EXE only after a successful unpack. Proxy/runtime methods remain explicit choices.</p><Field label="Method"><select value={config.steamstubMode} onChange={(e) => change('steamstubMode', e.currentTarget.value as Config['steamstubMode'])}><option value="auto">Auto — Steamless first, never silent proxy fallback</option><option value="steamless">Steamless — unpack/patch executable</option><option value="rune">RUNE SteamStub Patcher — explicit winmm.dll proxy</option><option value="uc_runtime">UC Runtime SteamStub — GetStubbedLol</option><option value="disabled">Disabled</option></select></Field><p className="gse-muted">Auto: Steamless first. If it cannot unpack, no DLL is dropped automatically.</p></article>
            <article className="gse-card"><h3>{'Identity & saves'}</h3><Field label="Account name"><input value={config.accountName} onChange={(e) => change('accountName', e.currentTarget.value)} /></Field><Field label="Save location"><select value={config.saveMode} onChange={(e) => change('saveMode', e.currentTarget.value as Config['saveMode'])}><option value="gse">GSE global — %APPDATA%\GSE Saves</option><option value="portable">Portable — game folder</option><option value="custom">Custom folder</option></select></Field>{config.saveMode === 'custom' ? <Field label="Custom save path"><div className="gse-input-action"><input value={config.customSavePath} onChange={(e) => change('customSavePath', e.currentTarget.value)} /><button type="button" onClick={() => void chooseCustomSave()}>Browse</button></div></Field> : null}</article>
          </div>

          <article className="gse-card">
            <h3>{'Overlay & compatibility'}</h3><p className="gse-muted">GSE Experimental exposes the complete achievement overlay controls. ColdClient compatibility stays separate.</p>
            {engineIsGse ? <><div className="gse-inset"><SettingRow title="Enable GSE overlay" subtitle="Master switch for enable_experimental_overlay. Turning it on automatically selects Experimental GSE." checked={config.overlay} onChange={(v) => { change('overlay', v); if (v) change('gseVariant','experimental') }} /></div><div className="gse-inset">{([
              ['overlayAchievementNotifications','Achievement popup','Show achievement unlock notifications.'],['overlayAchievementProgress','Achievement progress','Show progress notifications for stat-linked achievements.'],['overlayFriendNotifications','Friend notifications','Show invitations and friend/chat notifications.'],['overlayIcons','Achievement icons','Upload achievement icons to the GPU for notifications and overlay lists.'],['overlayUserInfo','Show user info','Always show user identity in the overlay.'],['overlayWarnings','Overlay warnings','Keep GSE overlay warnings visible.'],['overlayFps','FPS counter','Always show FPS counter on screen.'],['overlayFrametime','Frametime','Always show frametime metrics.'],['overlayShowPlaytime','Show playtime','Show current recorded playtime inside the overlay.'],['overlayPlaytime','Record playtime','Record GSE playtime to the save folder every minute.']
            ] as const).map(([key,title,subtitle]) => <SettingRow key={key} title={title} subtitle={subtitle} checked={config[key]} onChange={(v) => change(key, v)} />)}</div><div className="gse-inset gse-overlay-tuning"><div className="gse-grid four"><Field label="Achievement position"><select value={config.overlayPosition} onChange={(e) => change('overlayPosition',e.currentTarget.value)}><option value="bot_right">Bottom right (Steam-like)</option><option value="top_right">Top right</option><option value="bot_left">Bottom left</option><option value="top_left">Top left</option><option value="bot_center">Bottom center</option><option value="top_center">Top center</option></select></Field><Field label="Overlay hotkey"><input value={config.overlayHotkey} onChange={(e) => change('overlayHotkey',e.currentTarget.value)} /></Field><Field label="Font size"><input type="number" min="10" max="64" value={config.overlayFontSize} onChange={(e) => change('overlayFontSize',Number(e.currentTarget.value))} /></Field><Field label="Icon size"><input type="number" min="16" max="256" value={config.overlayIconSize} onChange={(e) => change('overlayIconSize',Number(e.currentTarget.value))} /></Field></div><div className="gse-grid five"><Field label="Popup rounding"><input type="number" value={config.overlayRounding} onChange={(e) => change('overlayRounding',Number(e.currentTarget.value))} /></Field><Field label="Popup animation" hint="seconds"><input type="number" step="0.05" value={config.overlayAnimation} onChange={(e) => change('overlayAnimation',Number(e.currentTarget.value))} /></Field><Field label="Achievement duration" hint="seconds"><input type="number" step="0.5" value={config.overlayAchievementDuration} onChange={(e) => change('overlayAchievementDuration',Number(e.currentTarget.value))} /></Field><Field label="Hook delay" hint="seconds"><input type="number" value={config.overlayHookDelay} onChange={(e) => change('overlayHookDelay',Number(e.currentTarget.value))} /></Field><Field label="Renderer timeout" hint="seconds"><input type="number" value={config.overlayRendererTimeout} onChange={(e) => change('overlayRendererTimeout',Number(e.currentTarget.value))} /></Field></div></div><SettingRow title="Official GSE generator" subtitle="Generate full branches, depots, controllers, app/overlay INIs when the generator resource is available." checked={config.officialGenerator} onChange={(v) => change('officialGenerator', v)} /><div className="gse-inset"><SettingRow title="DInput8 Overlay Bridge" subtitle="Copy dinput8.dll + dinput8.ini into the game folder for DX12 overlay-hotkey compatibility." checked={config.overlayDinputBridge} onChange={(v) => change('overlayDinputBridge',v)} /></div></> : <p className="gse-muted">{engineIsUc ? "UC Online uses the real Steam overlay path. Game-specific online behavior is provided by UC plugins." : "RUNE AutoCracker uses its own overlay/config path."}</p>}
          </article>

          <div className="gse-grid two">
            <article className="gse-card"><h3>Tools</h3><p className="gse-muted">Migration is never run automatically.</p><button type="button" onClick={() => void migrate()} disabled={busy === 'migrate'}><Wrench /> Migrate old Goldberg settings</button><SettingRow title="Reduced motion" subtitle="Disable toggle/drawer/progress interpolation animations." checked={config.reducedMotion} onChange={(v) => change('reducedMotion',v)} /><button type="button" onClick={() => void invoke('gse_auto_setup_open_config').catch(() => {})} style={{ marginTop: 8 }}>Open config.ini</button></article>
            <article className="gse-card"><h3>{'Resources & updates'}</h3><p className="gse-muted">Portable update overrides live beside the EXE. Large package caches are not kept in LocalAppData.</p><div className="gse-resource-list"><span>GSE · embedded baseline{resourceStatusSuffix(resourceUpdateStatus?.gse)}</span><span>UC Online · embedded baseline{resourceStatusSuffix(resourceUpdateStatus?.uc)}</span><span>Steamless · embedded baseline{resourceStatusSuffix(resourceUpdateStatus?.steamless)}</span><span>RUNE SteamStub · embedded baseline{resourceStatusSuffix(resourceUpdateStatus?.rune)}</span><span>migrate_gse · embedded baseline{resourceStatusSuffix(resourceUpdateStatus?.migrate)}</span></div><div className="gse-row" style={{ marginTop: 12 }}><button type="button" onClick={() => void checkResourceUpdates()} disabled={resourceCheckBusy}>{resourceCheckBusy ? <Loader2 className="spin" /> : <RefreshCw />} {resourceCheckBusy ? 'Checking…' : 'Check updates'}</button><button type="button" onClick={() => void invoke('gse_auto_setup_open_updates_folder').catch((error) => setMessage(String(error)))}>Open updates folder</button><button type="button" onClick={() => void invoke<string>('gse_auto_setup_clean_update_temp').then((text) => setMessage(text)).catch((error) => setMessage(String(error)))}>Clean update temp</button></div></article>
          </div>

          <article className="gse-card activity-card"><button type="button" className="gse-activity-head" onClick={() => setActivityOpen((v) => !v)}>Activity {activityOpen ? '▴' : '▾'}</button>{activityOpen ? <div className="gse-activity-body"><div className="gse-progress"><span style={{ width: `${progress}%` }} /></div><small>{progressText}</small><pre ref={logRef}>{logs.join('\n') || 'Ready'}</pre></div> : null}</article>

          {message ? <div className={`gse-message${message.toLowerCase().includes('error') || message.toLowerCase().includes('failed') ? ' error' : ''}`}>{message}</div> : null}
          <div className="gse-footer-actions"><button type="button" onClick={() => void restore()} disabled={Boolean(busy) || !config.gameFolder}><RotateCcw /> Restore original</button><button type="button" className="primary" onClick={() => void runSetup()} disabled={Boolean(busy)}>{busy === 'setup' ? <Loader2 className="spin" /> : <Check />} Setup</button></div>
        </div>
      )}

      {showCompletedModal && (
        <div className="gse-modal-backdrop" onClick={() => setShowCompletedModal(false)}>
          <div className="gse-modal-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="gse-modal-header">
              <div className="gse-modal-title">
                <span className="gse-modal-badge">G</span>
                <span>Setup</span>
              </div>
              <button type="button" className="gse-modal-close" onClick={() => setShowCompletedModal(false)}>×</button>
            </div>
            <div className="gse-modal-body">
              <div className="gse-modal-info-circle">
                <span>i</span>
              </div>
              <span className="gse-modal-text">Setup completed.</span>
            </div>
            <div className="gse-modal-footer">
              <button type="button" className="gse-modal-btn-ok" onClick={() => setShowCompletedModal(false)}>OK</button>
            </div>
          </div>
        </div>
      )}
    </section>
  )
}

export default GseUcStandaloneView
