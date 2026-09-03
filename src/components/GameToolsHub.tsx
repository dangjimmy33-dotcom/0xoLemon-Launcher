import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { AlertTriangle, Check, LoaderCircle, RefreshCcw } from 'lucide-react'
import { useLocale } from '../context/locale'
import { isTauriRuntime } from '../lib/tauriRuntime'
import type {
  GameToolsCatalogItem,
  GameToolsCatalogKind,
  GameToolsCatalogResponse,
  GameToolsGameStatus,
  GameToolsImportResult,
  GameToolsLibraryItem,
  GameToolsPackageProgress,
  GameToolsPackageResult,
  GameToolsSourceIdentity,
  GameToolsStatus,
  TabId,
} from '../types'
import BypassCatalogView from './cinematic/BypassCatalogView'
import BypassGameDetailView from './cinematic/BypassGameDetailView'
import BypassProviderView from './cinematic/BypassProviderView'
import GameToolsCatalogView from './cinematic/GameToolsCatalogView'
import ToolsWorkspaceView from './cinematic/ToolsWorkspaceView'
import type { BypassProviderId } from './cinematic/bypassProviders'
import './cinematic/cinematic.css'
import './LightningHub.css'

const SteamLaunchOptionsDialog = lazy(() => import('./SteamLaunchOptionsDialog'))

type GameToolsSection = 'store' | 'tools' | 'bypass' | 'onlineFix'

type GameToolsViewProps = {
  steamInstalledAppIds?: number[]
  libraryItems?: GameToolsLibraryItem[]
  onNavigate: (tab: TabId) => void
  onReloadLibrary?: () => void
}

const GAME_TOOLS_SECTION_KEY = '0xolemon.game-tools.section'
const GAME_TOOLS_SECTION_EVENT = 'launcher://game-tools-open-section'
const COMMUNITY_URL = 'https://discord.gg/7ZXdTUVsJE'
const SECTIONS: readonly GameToolsSection[] = ['store', 'tools', 'bypass', 'onlineFix']

function isGameToolsSection(value: unknown): value is GameToolsSection {
  return typeof value === 'string' && SECTIONS.includes(value as GameToolsSection)
}

function readSection(): GameToolsSection {
  const value = window.localStorage.getItem(GAME_TOOLS_SECTION_KEY)
  return isGameToolsSection(value) ? value : 'tools'
}

function catalogKindForSection(section: GameToolsSection): GameToolsCatalogKind | null {
  if (section === 'store') return 'store'
  if (section === 'bypass') return 'bypass'
  if (section === 'onlineFix') return 'onlineFix'
  return null
}

function createRequestId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  const random = new Uint8Array(16)
  crypto.getRandomValues(random)
  random[6] = (random[6] & 0x0f) | 0x40
  random[8] = (random[8] & 0x3f) | 0x80
  const value = Array.from(random, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${value.slice(0, 8)}-${value.slice(8, 12)}-${value.slice(12, 16)}-${value.slice(16, 20)}-${value.slice(20)}`
}

export function GameToolsView({
  steamInstalledAppIds = [],
  libraryItems = [],
  onNavigate,
  onReloadLibrary,
}: GameToolsViewProps) {
  const { locale } = useLocale()
  const vi = locale === 'vi-VN'
  const desktop = isTauriRuntime()
  const installedApps = useMemo(() => new Set(steamInstalledAppIds), [steamInstalledAppIds])
  const [section, setSectionState] = useState<GameToolsSection>(readSection)
  const [catalogs, setCatalogs] = useState<Partial<Record<GameToolsCatalogKind, GameToolsCatalogResponse>>>({})
  const [loadingCatalog, setLoadingCatalog] = useState<GameToolsCatalogKind | null>(null)
  const [provider, setProvider] = useState<BypassProviderId | null>(null)
  const [selectedItem, setSelectedItem] = useState<GameToolsCatalogItem | null>(null)
  const [gameStatus, setGameStatus] = useState<GameToolsGameStatus | null>(null)
  const [progress, setProgress] = useState<GameToolsPackageProgress | null>(null)
  const [operationAppId, setOperationAppId] = useState<number | null>(null)
  const [importBusy, setImportBusy] = useState(false)
  const [addBusy, setAddBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [steamLaunchOptionsAppId, setSteamLaunchOptionsAppId] = useState<number | null | undefined>(undefined)

  const setSection = useCallback((next: GameToolsSection) => {
    window.localStorage.setItem(GAME_TOOLS_SECTION_KEY, next)
    setSectionState(next)
    setProvider(null)
    setSelectedItem(null)
    setGameStatus(null)
    setProgress(null)
    setError(null)
    setNotice(null)
  }, [])

  useEffect(() => {
    const handleOpenSection = (event: Event) => {
      const detail = (event as CustomEvent<GameToolsSection | { section?: string }>).detail
      const candidate = typeof detail === 'string' ? detail : detail?.section
      if (isGameToolsSection(candidate)) setSection(candidate)
    }
    window.addEventListener(GAME_TOOLS_SECTION_EVENT, handleOpenSection)
    return () => window.removeEventListener(GAME_TOOLS_SECTION_EVENT, handleOpenSection)
  }, [setSection])

  useEffect(() => {
    if (!desktop) return
    void invoke<GameToolsStatus>('get_game_tools_status').catch((cause) => setError(String(cause)))
  }, [desktop])

  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<GameToolsPackageProgress>('launcher://game-tools-package-progress', (event) => {
      if (!disposed) setProgress(event.payload)
    }).then((dispose) => {
      if (disposed) dispose()
      else unlisten = dispose
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [desktop])

  const loadCatalog = useCallback(async (kind: GameToolsCatalogKind, force = false) => {
    if (!desktop || (!force && catalogs[kind])) return
    setLoadingCatalog(kind)
    setError(null)
    try {
      const response = await invoke<GameToolsCatalogResponse>('get_game_tools_catalog', { kind })
      setCatalogs((current) => ({ ...current, [kind]: response }))
    } catch (cause) {
      setError(String(cause))
    } finally {
      setLoadingCatalog(null)
    }
  }, [catalogs, desktop])

  useEffect(() => {
    const kind = catalogKindForSection(section)
    if (!kind) return
    const timer = window.setTimeout(() => void loadCatalog(kind), 0)
    return () => window.clearTimeout(timer)
  }, [loadCatalog, section])

  useEffect(() => {
    if (!desktop || !selectedItem || selectedItem.kind === 'store') {
      setGameStatus(null)
      return
    }
    let disposed = false
    void invoke<GameToolsGameStatus>('get_game_tools_game_status', { appId: selectedItem.appId })
      .then((status) => { if (!disposed) setGameStatus(status) })
      .catch((cause) => { if (!disposed) setError(String(cause)) })
    return () => { disposed = true }
  }, [desktop, selectedItem])

  const importFiles = useCallback(async (paths: string[] | null) => {
    if (!desktop || importBusy) return
    setImportBusy(true)
    setError(null)
    setNotice(null)
    try {
      const result = await invoke<GameToolsImportResult | null>('import_game_tools_files', { paths })
      if (!result) return
      const identities = [
        result.appIds.length > 0 ? `${result.appIds.length} Lua AppID` : null,
        result.depotIds.length > 0 ? `${result.depotIds.length} depot manifest` : null,
      ].filter(Boolean).join(', ')
      setNotice(vi
        ? `Đã import an toàn ${result.installedFiles} file${identities ? ` (${identities})` : ''}.${result.requiresSteamRestart ? ' Hãy khởi động lại Steam.' : ''}`
        : `Safely imported ${result.installedFiles} files${identities ? ` (${identities})` : ''}.${result.requiresSteamRestart ? ' Restart Steam to load them.' : ''}`)
    } catch (cause) {
      setError(String(cause))
    } finally {
      setImportBusy(false)
    }
  }, [desktop, importBusy, vi])

  const addAppId = useCallback(async (appId: number) => {
    if (!desktop || addBusy) return
    setAddBusy(true)
    setError(null)
    setNotice(null)
    try {
      await invoke<void>('add_to_steam', { appid: appId, forceUpdate: false })
      setNotice(vi ? `Đã thêm AppID ${appId} bằng pipeline Steam Lua hiện có.` : `Added AppID ${appId} through the existing Steam Lua pipeline.`)
    } catch (cause) {
      setError(String(cause))
    } finally {
      setAddBusy(false)
    }
  }, [addBusy, desktop, vi])

  const restartSteam = useCallback(async () => {
    if (!desktop) return
    setError(null)
    try {
      await invoke('restart_steam')
      setNotice(vi ? 'Steam đang khởi động lại.' : 'Steam is restarting.')
    } catch (cause) {
      setError(String(cause))
    }
  }, [desktop, vi])

  const openSteamDb = useCallback(async (appId: number | null) => {
    if (!desktop) return
    try {
      await invoke('open_url', { url: appId ? `https://steamdb.info/app/${appId}/` : 'https://steamdb.info/' })
    } catch (cause) {
      setError(String(cause))
    }
  }, [desktop])

  const applyPackage = useCallback(async (item: GameToolsCatalogItem) => {
    if (!desktop || item.kind === 'store' || operationAppId !== null) return
    setError(null)
    setNotice(null)
    setProgress(null)
    setOperationAppId(item.appId)
    try {
      // The native picker is deliberately the first state-changing step. A
      // cancel returns null and never creates a download or transaction job.
      const installDir = await invoke<string | null>('pick_game_tools_install_dir', {
        kind: item.kind,
        appId: item.appId,
      })
      if (!installDir) return
      const identity = await invoke<GameToolsSourceIdentity>('get_game_tools_package_identity', {
        kind: item.kind,
        appId: item.appId,
      })
      const result = await invoke<GameToolsPackageResult>('apply_game_tools_package', {
        request: {
          kind: item.kind,
          appId: item.appId,
          requestId: createRequestId(),
          installDir,
          revision: identity.revision,
          packageSha256: identity.packageSha256,
        },
      })
      setNotice(vi
        ? `Đã áp dụng ${result.appliedFiles} file và giữ ${result.backupFiles} bản sao rollback.`
        : `Applied ${result.appliedFiles} files and retained ${result.backupFiles} rollback backups.`)
      setGameStatus(await invoke<GameToolsGameStatus>('get_game_tools_game_status', { appId: item.appId }))
    } catch (cause) {
      setError(String(cause))
    } finally {
      setOperationAppId(null)
    }
  }, [desktop, operationAppId, vi])

  const restorePackage = useCallback(async (appId: number) => {
    if (!desktop || operationAppId !== null) return
    setOperationAppId(appId)
    setError(null)
    try {
      const result = await invoke<GameToolsPackageResult>('restore_latest_game_tools_package', { appId })
      setNotice(vi ? `Đã khôi phục ${result.appliedFiles} file.` : `Restored ${result.appliedFiles} files.`)
      setGameStatus(await invoke<GameToolsGameStatus>('get_game_tools_game_status', { appId }))
    } catch (cause) {
      setError(String(cause))
    } finally {
      setOperationAppId(null)
    }
  }, [desktop, operationAppId, vi])

  const openComponents = useCallback(() => {
    window.localStorage.setItem('0xolemon.settings.activePane', 'components')
    onNavigate('Settings')
    window.setTimeout(() => {
      window.dispatchEvent(new CustomEvent('0xo-settings-pane', { detail: { pane: 'components' } }))
      document.getElementById('feature-packages')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }, 80)
  }, [onNavigate])

  const openCommunity = useCallback(() => {
    if (!desktop) return
    void invoke('open_url', { url: COMMUNITY_URL }).catch((cause) => setError(String(cause)))
  }, [desktop])

  const catalogKind = catalogKindForSection(section)
  const activeCatalog = catalogKind ? catalogs[catalogKind] : undefined
  const operationBusy = selectedItem ? operationAppId === selectedItem.appId : false

  return (
    <section className="game-tools-view cinematic-game-tools" data-game-tools-section={section}>
      {!desktop ? <div className="lightning-hub-message is-warning"><AlertTriangle /> Game Tools actions require the desktop launcher.</div> : null}
      {error ? <div className="lightning-hub-message is-error"><AlertTriangle /><span>{error}</span><button type="button" onClick={() => setError(null)}>Dismiss</button></div> : null}
      {notice ? <div className="lightning-hub-message is-success"><Check /><span>{notice}</span><button type="button" onClick={() => setNotice(null)}>Dismiss</button></div> : null}

      {section === 'tools' ? (
        <ToolsWorkspaceView
          desktop={desktop}
          locale={locale}
          games={libraryItems}
          importBusy={importBusy}
          addBusy={addBusy}
          onImport={importFiles}
          onAddAppId={addAppId}
          onRestartSteam={restartSteam}
          onOpenSteamDb={openSteamDb}
          onReloadCovers={() => {
            onReloadLibrary?.()
            setNotice(vi ? 'Đang tải lại catalog và cover.' : 'Reloading catalog and covers.')
          }}
          onOpenSteamLaunchOptions={(appId) => setSteamLaunchOptionsAppId(appId)}
        />
      ) : null}

      {section === 'bypass' && !provider && !selectedItem ? <BypassProviderView items={activeCatalog?.items ?? []} loading={loadingCatalog === 'bypass'} onSelect={setProvider} /> : null}
      {section === 'bypass' && provider && !selectedItem ? <BypassCatalogView items={activeCatalog?.items ?? []} providerId={provider} onProviderChange={setProvider} onBack={() => setProvider(null)} onSelect={setSelectedItem} /> : null}

      {(section === 'store' || section === 'onlineFix') && !selectedItem ? (
        <GameToolsCatalogView
          title={section === 'store' ? 'Instant Gaming' : 'OnlineFix'}
          description={section === 'store' ? 'Curated community listings' : 'Multiplayer compatibility packages'}
          items={activeCatalog?.items ?? []}
          categories={activeCatalog?.categories ?? []}
          loading={loadingCatalog === catalogKind}
          onSelect={setSelectedItem}
        />
      ) : null}

      {selectedItem ? (
        <BypassGameDetailView
          item={selectedItem}
          progress={progress}
          status={gameStatus}
          busy={operationBusy}
          onBack={() => setSelectedItem(null)}
          onApply={() => applyPackage(selectedItem)}
          onRestore={() => restorePackage(selectedItem.appId)}
          onOpenComponents={openComponents}
          onOpenCommunity={openCommunity}
        />
      ) : null}

      {catalogKind && activeCatalog ? <button className="cinematic-catalog-refresh" type="button" onClick={() => void loadCatalog(catalogKind, true)} title="Reload verified catalog">{loadingCatalog === catalogKind ? <LoaderCircle className="spin" /> : <RefreshCcw />}</button> : null}
      {steamLaunchOptionsAppId !== undefined ? (
        <Suspense fallback={<div className="lightning-hub-message"><LoaderCircle className="spin" /> Loading Steam Launch Options…</div>}>
          <SteamLaunchOptionsDialog
            locale={locale}
            initialAppId={steamLaunchOptionsAppId}
            onClose={() => setSteamLaunchOptionsAppId(undefined)}
          />
        </Suspense>
      ) : null}
      <span className="sr-only">{installedApps.size} Steam games detected.</span>
    </section>
  )
}

export default GameToolsView
