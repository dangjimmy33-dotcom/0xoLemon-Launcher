import { useState, useEffect, useMemo, useRef, useCallback, memo, type CSSProperties } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Search, ChevronLeft, ChevronRight, Plus, Trash2, RefreshCw, CheckCircle, Settings, SlidersHorizontal, X } from 'lucide-react'
import { useLocale } from '../context/locale'
import {
  fetchSteamGameInfo,
  getCachedSteamGameInfo,
  seedSteamGameInfo,
  type SteamGameInfo,
} from '../lib/steamGameInfo'
import './LuaShop.css'
import { LuaGameManagerDialog } from './LuaGameManagerDialog'
import { LuaSourcePickerDialog } from './LuaSourcePickerDialog'
import type {
  LuaCatalogItem,
  LuaCatalogSearchPage,
  LuaGameChannel,
  LuaGameState,
  LuaSourceAvailability,
  LuaSourceOperation,
  LuaSourceProvider,
  LuaSourceSettingsState,
  HubcapKeyState,
} from '../types'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

type FilterTab = 'all' | 'installed' | 'notInstalled'
type LuaShopSort = 'az' | 'za' | 'appidAsc' | 'appidDesc'
type LuaShopLayout = 'grid' | 'list'

type LuaSourceActionIntent = {
  appid: number
  gameName: string
  operation: LuaSourceOperation
  purpose: 'add' | 'update' | 'sync' | 'switchLive'
  preferredProvider: LuaSourceProvider | null
  preferredChannel: LuaGameChannel | null
}

function luaChannelLabel(
  state: LuaGameState | undefined,
  lockedLabel: string,
  installedLabel: string,
) {
  if (!state) return installedLabel
  return state.channel === 'live'
    ? 'Live'
    : `${lockedLabel}${state.pinnedBuildId ? ` · ${state.pinnedBuildId}` : ''}`
}

function formatLuaSyncTime(value: string | null | undefined) {
  if (!value) return ''
  const timestamp = Date.parse(value)
  return Number.isFinite(timestamp) ? new Date(timestamp).toLocaleString() : value
}

function emitLuaShopToast(
  title: string,
  message: string,
  severity: 'success' | 'error' | 'info' = 'info',
) {
  window.dispatchEvent(new CustomEvent('0xo-toast', {
    detail: { category: 'launcher', severity, title, message, dedupeKey: 'lua-shop:' + title },
  }))
}

type ViewportListener = (visible: boolean) => void
const viewportListeners = new Map<Element, ViewportListener>()
let sharedCardObserver: IntersectionObserver | null = null
let viewportFrame = 0
let queuedViewportEntries: IntersectionObserverEntry[] = []

function cardObserver() {
  if (sharedCardObserver || typeof IntersectionObserver === 'undefined') return sharedCardObserver
  sharedCardObserver = new IntersectionObserver((entries) => {
    queuedViewportEntries.push(...entries)
    if (viewportFrame) return
    viewportFrame = window.requestAnimationFrame(() => {
      const latest = new Map<Element, IntersectionObserverEntry>()
      for (const entry of queuedViewportEntries) latest.set(entry.target, entry)
      queuedViewportEntries = []
      viewportFrame = 0
      latest.forEach((entry, target) => {
        viewportListeners.get(target)?.(entry.isIntersecting)
      })
    })
  }, { root: null, rootMargin: '300px 0px', threshold: 0.01 })
  return sharedCardObserver
}

function useCardViewport() {
  const elementRef = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const element = elementRef.current
    const observer = cardObserver()
    if (!element || !observer) {
      setVisible(true)
      return
    }
    viewportListeners.set(element, setVisible)
    observer.observe(element)
    return () => {
      observer.unobserve(element)
      viewportListeners.delete(element)
    }
  }, [])
  return { elementRef, visible }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module-level game-info cache & throttled fetch queue
// Persists across tab switches / remounts — no re-fetch on navigation
// ─────────────────────────────────────────────────────────────────────────────

/*
const gameInfoCache = new Map<string, SteamGameInfo>()
const pendingRequests = new Map<string, Promise<SteamGameInfo | null>>()

let activeRequests = 0
const MAX_CONCURRENT = 6
const requestQueue: Array<() => void> = []

function processQueue() {
  while (activeRequests < MAX_CONCURRENT && requestQueue.length > 0) {
    const next = requestQueue.shift()
    if (next) next()
  }
}

async function fetchGameInfoInternal(appid: string): Promise<SteamGameInfo | null> {
  activeRequests++

  const promise = (async () => {
    // 1. Try Tauri backend (fastest — local cache)
    try {
      const result = await invoke<SteamGameInfo>('fetch_steam_game_name', { appid: parseInt(appid) })
      if (result?.name) {
        gameInfoCache.set(appid, result)
        return result
      }
    } catch (_) {}

    // 2. Fallback — Steam Store API
    try {
      const res = await fetch(
        `https://store.steampowered.com/api/appdetails?appids=${appid}&filters=basic`,
        { signal: AbortSignal.timeout(6000) }
      )
      if (res.ok) {
        const data = await res.json()
        const entry = data?.[appid]
        if (entry?.success && entry?.data?.name) {
          const info: SteamGameInfo = {
            name: entry.data.name,
            header_image:
              entry.data.header_image ||
              `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`,
          }
          gameInfoCache.set(appid, info)
          return info
        }
      }
    } catch (_) {}

    return null
  })()

  pendingRequests.set(appid, promise)
  const result = await promise
  pendingRequests.delete(appid)
  activeRequests--
  processQueue()
  return result
}

async function fetchGameInfo(appid: string): Promise<SteamGameInfo | null> {
  if (gameInfoCache.has(appid)) return gameInfoCache.get(appid)!
  if (pendingRequests.has(appid)) return pendingRequests.get(appid)!
  if (activeRequests >= MAX_CONCURRENT) {
    return new Promise((resolve) => {
      requestQueue.push(() => fetchGameInfoInternal(appid).then(resolve))
    })
  }
  return fetchGameInfoInternal(appid)
}
*/


function normalizeSearchText(value: string) {
  return value
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
}

function titleMatchesQuery(title: string, normalizedQuery: string) {
  const normalizedTitle = normalizeSearchText(title)
  if (normalizedTitle.includes(normalizedQuery)) return true
  const tokens = normalizedQuery.split(' ').filter(Boolean)
  return tokens.length > 0 && tokens.every((token) => normalizedTitle.includes(token))
}

// ─────────────────────────────────────────────────────────────────────────────
// ConfirmDialog
// ─────────────────────────────────────────────────────────────────────────────

function ConfirmDialog({
  title,
  message,
  confirmText,
  cancelText,
  variant = 'info',
  onConfirm,
  onCancel,
  children,
}: {
  title: string
  message: string
  confirmText: string
  cancelText: string
  variant?: 'info' | 'warning' | 'error'
  onConfirm: () => void
  onCancel: () => void
  children?: React.ReactNode
}) {
  const [mounted, setMounted] = useState(false)
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => setMounted(true))
    return () => window.cancelAnimationFrame(frame)
  }, [])
  if (!mounted) return null

  const accentColor =
    variant === 'warning' ? '#fbbf24' : variant === 'error' ? '#f87171' : 'var(--theme-accent-strong)'
  const accentBg =
    variant === 'warning'
      ? 'color-mix(in oklab, transparent 82%, #f59e0b 18%)'
      : variant === 'error'
        ? 'color-mix(in oklab, transparent 82%, #ef4444 18%)'
        : 'var(--theme-accent-surface)'
  const accentBorder =
    variant === 'warning'
      ? '1px solid color-mix(in oklab, transparent 58%, #f59e0b 42%)'
      : variant === 'error'
        ? '1px solid color-mix(in oklab, transparent 58%, #ef4444 42%)'
        : '1px solid color-mix(in oklab, transparent 52%, var(--theme-accent-strong) 48%)'

  return createPortal(
    <div
      className="dialog-backdrop"
      onClick={onCancel}
      style={{
        position: 'fixed', inset: 0, background: 'var(--theme-overlay-bg)',
        zIndex: 99999, display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}
    >
      <div
        className="dialog-box"
        onClick={(e) => e.stopPropagation()}
        style={{
          background: 'var(--theme-modal-bg)',
          border: '1px solid color-mix(in oklab, transparent 68%, var(--theme-accent) 32%)', borderRadius: '12px',
          padding: '24px', maxWidth: '480px', width: '90%',
          boxShadow: '0 20px 60px rgba(0,0,0,.5), 0 18px 50px color-mix(in oklab, transparent 86%, var(--theme-accent-deep) 14%)',
        }}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: '18px', fontWeight: 700, color: accentColor }}>
          {title}
        </h3>
        <p style={{ margin: '0 0 20px', color: 'var(--text)', fontSize: '14px', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>
          {message}
        </p>
        {children}
        <div style={{ display: 'flex', gap: '12px', marginTop: '20px' }}>
          <button
            onClick={onCancel}
            style={{
              flex: 1, padding: '10px 16px', borderRadius: '6px',
              background: 'var(--theme-control-bg)', border: '1px solid var(--line)',
              color: 'var(--text-strong)', fontWeight: 600, cursor: 'pointer',
            }}
          >
            {cancelText}
          </button>
          <button
            onClick={onConfirm}
            style={{
              flex: 1, padding: '10px 16px', borderRadius: '6px',
              background: accentBg, border: accentBorder,
              color: accentColor, fontWeight: 600, cursor: 'pointer',
            }}
          >
            {confirmText}
          </button>
        </div>
      </div>
    </div>,
    document.body
  )
}

// ─────────────────────────────────────────────────────────────────────────────
// LuaShopGameCard
// Only fetches game info when the card enters the viewport (IntersectionObserver).
// Removes the browser loading="lazy" which fires too aggressively on minor scroll.
// ─────────────────────────────────────────────────────────────────────────────

const LuaShopGameCard = memo(function LuaShopGameCard({
  appid,
  index,
  isInstalled,
  gameState,
  availability,
  onAdd,
  onRemove,
  onSync,
  onManage,
}: {
  appid: string
  index: number
  isInstalled: boolean
  gameState?: LuaGameState
  availability?: LuaSourceAvailability
  onAdd: (appid: string) => void
  onRemove: (appid: string) => void
  onSync: (appid: string) => Promise<void>
  onManage: (appid: string) => void
}) {
  // Initialise from module-level cache so cached cards render instantly
  const [info, setInfo] = useState<SteamGameInfo | null>(getCachedSteamGameInfo(appid) ?? null)
  const [isProcessing, setIsProcessing] = useState(false)
  const [imageLoaded, setImageLoaded] = useState(false)
  const [imageIndex, setImageIndex] = useState(0)
  const [imageFailed, setImageFailed] = useState(false)
  const { elementRef, visible: shouldLoad } = useCardViewport()
  const appliedHeaderRef = useRef<string | null>(null)
  const { t } = useLocale()

  // ── Fetch game info once the card is visible ────────────────────────────────
  useEffect(() => {
    if (!shouldLoad) {
      return
    }
    let mounted = true
    fetchSteamGameInfo(appid).then((result) => {
      if (mounted && result && (
        result.name !== info?.name || result.header_image !== info?.header_image
      )) {
        appliedHeaderRef.current = null
        setImageLoaded(false)
        setImageIndex(0)
        setImageFailed(false)
        setInfo(result)
      }
    })
    return () => { mounted = false }
  }, [shouldLoad, appid, info?.name, info?.header_image])

  const handleAction = async () => {
    setIsProcessing(true)
    try {
      if (isInstalled) await onRemove(appid)
      else await onAdd(appid)
    } finally {
      setIsProcessing(false)
    }
  }

  const handleSync = async () => {
    setIsProcessing(true)
    try {
      await onSync(appid)
    } finally {
      setIsProcessing(false)
    }
  }

  const imageCandidates = useMemo(() => {
    const candidates = [
      info?.header_image,
    ].filter((url): url is string => Boolean(url))
    return Array.from(new Set(candidates))
  }, [info?.header_image])

  const imageUrl = shouldLoad && !imageFailed
    ? imageCandidates[Math.min(imageIndex, imageCandidates.length - 1)] ?? ''
    : ''

  const sourceBadgeProvider = isInstalled && gameState
    ? gameState.selectedSource === 'huggingFace'
      ? gameState.selectedVariant ?? gameState.sourceProvider ?? 'none'
      : gameState.selectedSource ?? gameState.sourceProvider ?? 'none'
    : availability?.preferredProvider ?? 'none'

  useEffect(() => {
    const header = info?.header_image
    if (!header || appliedHeaderRef.current === header) return
    appliedHeaderRef.current = header
    setImageIndex(0)
    setImageFailed(false)
    setImageLoaded((loaded) => loaded && header === imageUrl)
  }, [imageUrl, info?.header_image])

  return (
    <div
      ref={elementRef}
      className={`lua-shop-card${isInstalled ? ' is-installed' : ''}`}
      style={{ animationDelay: `${Math.min(index, 23) * 30}ms` }}
    >
      {/* ── Thumbnail ── */}
      <div className={`lua-shop-card-image ${!imageLoaded && !imageFailed ? 'is-loading' : ''} ${imageFailed ? 'is-error' : ''}`}>
        {isInstalled && (
          <div className={`lua-shop-installed-badge channel-${gameState?.channel ?? 'legacy'}`}>
            <CheckCircle size={12} />
            {luaChannelLabel(gameState, t.luaShop.lockedChannel, t.luaShop.installed)}
          </div>
        )}
        {shouldLoad && imageUrl && (
          <img
            src={imageUrl}
            alt={info?.name || `AppID ${appid}`}
            className={imageLoaded ? 'loaded' : 'loading'}
            loading="lazy"
            decoding="async"
            onLoad={() => setImageLoaded(true)}
            onError={(e) => {
              e.currentTarget.removeAttribute('srcset')
              if (imageIndex < imageCandidates.length - 1) {
                setImageLoaded(false)
                setImageFailed(false)
                setImageIndex((current) => Math.min(current + 1, imageCandidates.length - 1))
              } else {
                setImageLoaded(true)
                setImageFailed(true)
              }
            }}
          />
        )}
        {imageFailed && <span className="lua-shop-card-image-fallback">AppID {appid}</span>}
      </div>

      {/* ── Card content ── */}
      <div className="lua-shop-card-content">
        <div className="lua-shop-card-title">
          {info?.name || `AppID ${appid}`}
        </div>
        <div className="lua-shop-card-appid">AppID: {appid}</div>
        {(availability || gameState) && (
          <div className={`lua-shop-source-badge source-${sourceBadgeProvider}`}>
            {sourceBadgeProvider === 'none'
              ? t.luaShop.sourceOnDemand
              : sourceBadgeProvider === 'community'
                ? t.luaShop.sourceCommunity
                : sourceBadgeProvider === 'curated'
                  ? t.luaShop.sourceCurated
                  : sourceBadgeProvider === 'hubcap'
                    ? 'Hubcap'
                    : sourceBadgeProvider === 'ryuu'
                      ? 'Ryuu'
                      : 'Sushi'}
          </div>
        )}
        {gameState && (
          <div className={`lua-shop-sync-summary status-${gameState.syncStatus}`}>
            <span>
              {gameState.runtimeState === 'active' && gameState.sourceState === 'unavailable'
                ? `Live · ${t.luaShop.manager.sourceUnavailableActive}`
                : gameState.lastError || (gameState.lastSyncAt
                ? t.luaShop.syncedAt.replace('{time}', formatLuaSyncTime(gameState.lastSyncAt))
                : t.luaShop.syncNever)}
            </span>
            {gameState.channel === 'live' && (
              <button
                type="button"
                className="lua-shop-card-sync"
                onClick={handleSync}
                disabled={isProcessing || gameState.syncStatus === 'checking'}
                title={t.luaShop.syncLive}
              >
                <RefreshCw size={13} className={gameState.syncStatus === 'checking' ? 'spin' : ''} />
                <span>{t.luaShop.sync}</span>
              </button>
            )}
          </div>
        )}
        <div className="lua-shop-card-actions">
          {isInstalled && gameState?.updateAvailable ? (
            <button
              className="lua-shop-card-btn update"
              onClick={handleSync}
              disabled={isProcessing}
            >
              <RefreshCw size={16} className={isProcessing ? 'spin' : ''} />
              {isProcessing ? t.luaShop.updating : t.luaShop.update}
            </button>
          ) : (
            <button
              className={`lua-shop-card-btn ${isInstalled ? 'remove' : 'add'}`}
              onClick={handleAction}
              disabled={isProcessing}
            >
              {isInstalled ? (
                <>
                  <Trash2 size={16} />
                  {isProcessing
                    ? t.luaShop.removing || 'Đang xóa...'
                    : t.luaShop.removeFromSteam || 'Gỡ khỏi Steam'}
                </>
              ) : (
                <>
                  <Plus size={16} />
                  {isProcessing
                    ? t.luaShop.adding || 'Đang thêm...'
                    : t.luaShop.addToSteam || 'Thêm vào Steam'}
                </>
              )}
            </button>
          )}
          {isInstalled && gameState?.updateAvailable && (
            <button
              type="button"
              className="lua-shop-card-remove-compact"
              onClick={handleAction}
              disabled={isProcessing}
              title={t.luaShop.removeFromSteam}
              aria-label={t.luaShop.removeFromSteam}
            >
              <Trash2 size={16} />
            </button>
          )}
          {isInstalled && (
            <button
              type="button"
              className="lua-shop-card-manage"
              onClick={() => onManage(appid)}
              disabled={isProcessing}
              title={t.luaShop.manager.manage}
              aria-label={t.luaShop.manager.manage}
            >
              <Settings size={16} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
})

// ─────────────────────────────────────────────────────────────────────────────
// LuaShop main component
// ─────────────────────────────────────────────────────────────────────────────

export function LuaShop() {
  const [catalogItems, setCatalogItems] = useState<LuaCatalogItem[]>([])
  const [catalogTotal, setCatalogTotal] = useState<number | null>(null)
  const [nextCatalogCursor, setNextCatalogCursor] = useState<string | null>(null)
  const [catalogCursor, setCatalogCursor] = useState<string | null>(null)
  const [catalogCursorHistory, setCatalogCursorHistory] = useState<Array<string | null>>([])
  const [catalogPageNumber, setCatalogPageNumber] = useState(1)
  const [isLoading, setIsLoading] = useState(true)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [search, setSearch] = useState('')
  const [debouncedSearch, setDebouncedSearch] = useState('')
  const [isSearching, setIsSearching] = useState(false)
  const [isCatalogTransitioning, setIsCatalogTransitioning] = useState(false)
  const [luaShopSort, setLuaShopSort] = useState<LuaShopSort>(() => {
    const saved = localStorage.getItem('luaShopSort')
    return saved === 'za' || saved === 'appidAsc' || saved === 'appidDesc' ? saved : 'az'
  })
  const [sortOpen, setSortOpen] = useState(false)
  const [viewLayout, setViewLayout] = useState<LuaShopLayout>(() => localStorage.getItem('luaShopLayout') === 'list' ? 'list' : 'grid')
  const [gridCols, setGridCols] = useState<4 | 6 | 8>(() => {
    const saved = Number(localStorage.getItem('luaShopGridCols'))
    return saved === 6 || saved === 8 ? saved : 4
  })
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [sourceSettings, setSourceSettings] = useState<LuaSourceSettingsState | null>(null)
  const [filterTab, setFilterTab] = useState<FilterTab>('all')
  const [installedLuas, setInstalledLuas] = useState<Set<string>>(new Set())
  const [installedCatalogItems, setInstalledCatalogItems] = useState<LuaCatalogItem[]>([])
  const [isInstalledCatalogLoading, setIsInstalledCatalogLoading] = useState(false)
  const [luaGameStates, setLuaGameStates] = useState<Record<string, LuaGameState>>({})
  const luaGameStatesRef = useRef<Record<string, LuaGameState>>({})
  const [showRestartConfirm, setShowRestartConfirm] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const [removingAppId, setRemovingAppId] = useState<string | null>(null)
  const [managedGame, setManagedGame] = useState<{ appid: number; name: string } | null>(null)
  const [sourceAction, setSourceAction] = useState<LuaSourceActionIntent | null>(null)
  const [autoInstall, setAutoInstall] = useState(
    localStorage.getItem('steamAutoInstall') === 'true'
  )
  const [skipConfirm, setSkipConfirm] = useState(
    localStorage.getItem('steamSkipRestartConfirm') === 'true'
  )
  const ITEMS_PER_PAGE = 24

  const upsertLuaGameState = useCallback((state: LuaGameState) => {
    setLuaGameStates((current) => {
      const next = { ...current, [String(state.appid)]: state }
      luaGameStatesRef.current = next
      return next
    })
  }, [])

  const { t } = useLocale()

  const gridRef = useRef<HTMLDivElement>(null)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const catalogRequestRef = useRef(0)
  const catalogLoadedRef = useRef(false)

  const fetchInstalledLuas = useCallback(async () => {
    const [luasResult, statesResult] = await Promise.allSettled([
      invoke<string[]>('list_installed_luas'),
      invoke<LuaGameState[]>('get_lua_game_states'),
    ])
    const luas = luasResult.status === 'fulfilled' ? luasResult.value : []
    const states = statesResult.status === 'fulfilled' ? statesResult.value : []
    if (luasResult.status === 'fulfilled' || statesResult.status === 'fulfilled') {
      setInstalledLuas(new Set([
        ...luas,
        ...states.map((state) => String(state.appid)),
      ]))
    }
    if (statesResult.status === 'fulfilled') {
      const next = Object.fromEntries(states.map((state) => [String(state.appid), state]))
      luaGameStatesRef.current = next
      setLuaGameStates(next)
    }
  }, [])

  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined
    void listen<LuaGameState>('launcher://lua-game-state', (event) => {
      if (!active) return
      upsertLuaGameState(event.payload)
      setInstalledLuas((current) => new Set(current).add(String(event.payload.appid)))
    }).then((stop) => {
      if (active) unlisten = stop
      else stop()
    })
    return () => {
      active = false
      unlisten?.()
    }
  }, [upsertLuaGameState])

  const refreshSourceOverview = useCallback(async (refreshHubcapUsage = false) => {
    try {
      let settings = await invoke<LuaSourceSettingsState>('get_lua_source_settings')
      if (refreshHubcapUsage && settings.hubcap.configured) {
        try {
          const hubcap = await invoke<HubcapKeyState>('refresh_hubcap_key_state')
          settings = { ...settings, hubcap }
        } catch (error) {
          console.warn('Hubcap quota refresh failed:', error)
        }
      }
      setSourceSettings(settings)
    } catch (error) {
      console.warn('Lua source overview refresh failed:', error)
    }
  }, [])

  // ── Initial local state ─────────────────────────────────────────────────────
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void Promise.all([fetchInstalledLuas(), refreshSourceOverview(true)])
    }, 0)
    return () => window.clearTimeout(timer)
  }, [fetchInstalledLuas, refreshSourceOverview])

  useEffect(() => {
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        searchInputRef.current?.focus()
        searchInputRef.current?.select()
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [])

  useEffect(() => {
    if (filterTab !== 'installed') return
    const timer = window.setTimeout(() => {
      const ids = [...installedLuas]
        .filter((appid) => /^\d+$/.test(appid))
        .map(Number)
        .filter((appid) => Number.isSafeInteger(appid) && appid > 0)
        .slice(0, 500)
      const items = ids.map((appid) => {
        const info = getCachedSteamGameInfo(String(appid))
        const state = luaGameStates[String(appid)]
        return {
          appid,
          name: info?.name || state?.gameName || `AppID ${appid}`,
          headerImage: info?.header_image || '',
          installed: true,
          availability: {
            appid,
            curatedAvailable: state?.sourceProvider === 'curated',
            communityAvailable: state?.sourceProvider === 'community',
            hubcapAvailable: state?.sourceProvider === 'hubcap',
            sushiAvailable: state?.sourceProvider === 'sushi',
            ryuuAvailable: state?.sourceProvider === 'ryuu',
            preferredProvider: (state?.sourceProvider || 'none') as LuaSourceAvailability['preferredProvider'],
            revision: state?.availableRevision || state?.sourceRevision || null,
            sourceModifiedAt: null,
            errorCode: state?.sourceErrorCode || null,
          },
        }
      })
      setInstalledCatalogItems(items.sort((left, right) => left.name.localeCompare(right.name)))
      setIsInstalledCatalogLoading(false)
    }, 0)
    return () => {
      window.clearTimeout(timer)
    }
  }, [filterTab, installedLuas, luaGameStates])

  // ── Search is server-paged; only the visible page is resolved and probed. ────
  useEffect(() => {
    if (filterTab !== 'installed' && search.trim() !== debouncedSearch) setIsCatalogTransitioning(true)
    const timer = window.setTimeout(() => {
      setDebouncedSearch(search.trim())
      setCatalogCursor(null)
      setCatalogCursorHistory([])
      setCatalogPageNumber(1)
    }, 300)
    return () => window.clearTimeout(timer)
  }, [filterTab, search])

  const loadCatalog = useCallback(async (): Promise<boolean> => {
    const requestId = ++catalogRequestRef.current
    if (!catalogLoadedRef.current) setIsLoading(true)
    setIsCatalogTransitioning(true)
    setIsSearching(Boolean(debouncedSearch))
    try {
      const page = await invoke<LuaCatalogSearchPage>('search_lua_games', {
        request: {
          query: debouncedSearch,
          cursor: catalogCursor,
          limit: ITEMS_PER_PAGE,
        },
      })
      if (requestId !== catalogRequestRef.current) return false
      page.items.forEach((item) => {
        seedSteamGameInfo(String(item.appid), {
          name: item.name,
          header_image: item.headerImage,
        })
      })
      setCatalogItems(page.items)
      setCatalogTotal(page.totalEstimate)
      setNextCatalogCursor(page.nextCursor)
      setCatalogError(null)
      catalogLoadedRef.current = true
      return true
    } catch (error) {
      if (requestId === catalogRequestRef.current) setCatalogError(String(error))
      return false
    } finally {
      if (requestId === catalogRequestRef.current) {
        setIsLoading(false)
        setIsSearching(false)
        setIsCatalogTransitioning(false)
      }
    }
  }, [catalogCursor, debouncedSearch])

  useEffect(() => {
    if (filterTab === 'installed') {
      catalogRequestRef.current += 1
      setIsCatalogTransitioning(false)
      return
    }
    const timer = window.setTimeout(() => {
      void loadCatalog()
    }, 0)
    return () => window.clearTimeout(timer)
  }, [filterTab, loadCatalog])

  const handleRefresh = useCallback(async () => {
    setIsRefreshing(true)
    try {
      const [catalogRefreshed] = await Promise.all([
        loadCatalog(),
        fetchInstalledLuas(),
        refreshSourceOverview(true),
      ])
      emitLuaShopToast(
        t.luaShop.title,
        catalogRefreshed ? t.luaShop.refreshCurrent : t.luaShop.refreshFailed,
        catalogRefreshed ? 'success' : 'error',
      )
    } catch (error) {
      emitLuaShopToast(t.luaShop.title, String(error), 'error')
    } finally {
      setIsRefreshing(false)
    }
  }, [fetchInstalledLuas, loadCatalog, refreshSourceOverview, t.luaShop.refreshCurrent, t.luaShop.refreshFailed, t.luaShop.title])

  const filteredCatalogItems = useMemo(() => {
    if (filterTab === 'installed') {
      const query = normalizeSearchText(debouncedSearch)
      return installedCatalogItems.filter((item) => (
        !query
        || String(item.appid).includes(query)
        || titleMatchesQuery(item.name, query)
      ))
    }
    if (filterTab === 'notInstalled') {
      return catalogItems.filter((item) => !installedLuas.has(String(item.appid)) && !item.installed)
    }
    return catalogItems
  }, [catalogItems, debouncedSearch, filterTab, installedCatalogItems, installedLuas])

  const sortedCatalogItems = useMemo(() => {
    const items = [...filteredCatalogItems]
    items.sort((left, right) => {
      if (luaShopSort === 'za') return right.name.localeCompare(left.name)
      if (luaShopSort === 'appidAsc') return left.appid - right.appid
      if (luaShopSort === 'appidDesc') return right.appid - left.appid
      return left.name.localeCompare(right.name)
    })
    return items
  }, [filteredCatalogItems, luaShopSort])

  const installedTotalPages = Math.max(1, Math.ceil(sortedCatalogItems.length / ITEMS_PER_PAGE))
  const displayedCatalogItems = filterTab === 'installed'
    ? sortedCatalogItems.slice(
        (catalogPageNumber - 1) * ITEMS_PER_PAGE,
        catalogPageNumber * ITEMS_PER_PAGE,
      )
    : sortedCatalogItems

  const setShopSort = useCallback((value: LuaShopSort) => {
    setLuaShopSort(value)
    localStorage.setItem('luaShopSort', value)
    setSortOpen(false)
  }, [])

  const setShopLayout = useCallback((value: LuaShopLayout) => {
    setViewLayout(value)
    localStorage.setItem('luaShopLayout', value)
  }, [])

  const setShopGridCols = useCallback((value: 4 | 6 | 8) => {
    setGridCols(value)
    localStorage.setItem('luaShopGridCols', String(value))
  }, [])

  // Scroll grid to top on page change (instant — no jank from smooth scroll)
  useEffect(() => {
    const el = gridRef.current
    if (el) el.scrollTo({ top: 0, behavior: 'instant' as ScrollBehavior })
  }, [catalogPageNumber])

  // ── Toast helper ────────────────────────────────────────────────────────────
  const showToast = useCallback((
    title: string,
    msg: string,
    severity: 'success' | 'error' | 'info' = 'info'
  ) => {
    emitLuaShopToast(title, msg, severity)
  }, [])

  const performRestart = useCallback(async () => {
    try {
      const args = autoInstall ? { postRestartAction: 'steam://open/games' } : {}
      await invoke('force_restart_steam', args)
      showToast(t.library.restartSteamPrompt, t.settings.restartSteam + '...', 'info')
    } catch (e) {
      console.error('Restart Steam error:', e)
      showToast('Error', String(e), 'error')
    }
  }, [autoInstall, showToast, t.library.restartSteamPrompt, t.settings.restartSteam])

  const handleAddToSteam = useCallback(async (appid: string) => {
    const normalizedAppid = appid.trim()
    if (!/^\d+$/.test(normalizedAppid)) {
      showToast('Error', `Invalid AppID: ${appid}`, 'error')
      return
    }
    const numAppid = Number.parseInt(normalizedAppid, 10)
    const gameName = getCachedSteamGameInfo(normalizedAppid)?.name || 'AppID ' + normalizedAppid

    try {
      const isEnabled = await invoke<boolean>('is_lua_game_mode_enabled')
      if (!isEnabled) {
        showToast(t.luaShop.luaModeRequired || 'Lỗi', 'Please enable Lua-Game Mode in Settings first', 'info')
        return
      }
    } catch (e) {
      console.error('Failed to check lua-game mode status', e)
      showToast('Error', 'Failed to check Lua-Game Mode status: ' + String(e), 'error')
      return
    }

    setSourceAction({
      appid: numAppid,
      gameName,
      operation: 'add',
      purpose: 'add',
      preferredProvider: null,
      preferredChannel: null,
    })
  }, [
    showToast,
    t.luaShop.luaModeRequired,
  ])

  // ── Remove ──────────────────────────────────────────────────────────────────
  const handleSyncLuaGame = useCallback(async (appid: string) => {
    const current = luaGameStatesRef.current[appid]
    const operation: LuaSourceOperation = current?.updateAvailable ? 'update' : 'sync'
    setSourceAction({
      appid: Number.parseInt(appid, 10),
      gameName: current?.gameName || getCachedSteamGameInfo(appid)?.name || `AppID ${appid}`,
      operation,
      purpose: operation,
      preferredProvider: current?.selectedSource ?? null,
      preferredChannel: current?.channel ?? null,
    })
  }, [])

  const handleSourceConfirm = useCallback(async (provider: LuaSourceProvider, statSteamId: string | null, channel: LuaGameChannel) => {
    if (!sourceAction) return
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
    let state: LuaGameState
    if (sourceAction.purpose === 'switchLive') {
      state = await invoke<LuaGameState>('set_lua_game_channel', {
        request: {
          appid: sourceAction.appid,
          channel: 'live',
          buildId: null,
          conflictResolution: 'restoreLive',
          provider,
        },
      })
    } else if (sourceAction.purpose === 'add' || channel === 'locked') {
      state = await invoke<LuaGameState>('install_lua_game_from_source', {
        request: {
          appid: sourceAction.appid,
          gameName: sourceAction.gameName,
          channel,
          buildId: null,
          accessToken: null,
          statSteamId,
          conflictResolution: null,
          provider,
          requestId: crypto.randomUUID(),
          timezone,
        },
      })
    } else {
      const command = sourceAction.purpose === 'update'
        ? 'apply_lua_game_update'
        : 'sync_lua_game_from_source'
      state = await invoke<LuaGameState>(command, {
        request: {
          appid: sourceAction.appid,
          provider,
          requestId: crypto.randomUUID(),
          timezone,
          statSteamId,
          conflictResolution: null,
        },
      })
    }
    upsertLuaGameState(state)
    setInstalledLuas((current) => new Set(current).add(String(sourceAction.appid)))
    setSourceAction(null)
    showToast(
      sourceAction.purpose === 'add' ? t.library.addToSteam : t.luaShop.liveChannel,
      sourceAction.purpose === 'add'
        ? t.library.addToSteamSuccess
        : state.syncStatus === 'updated' ? t.luaShop.syncUpdated : t.luaShop.syncCurrent,
      'success',
    )
    if (state.requiresSteamRestart || sourceAction.purpose === 'add') {
      if (skipConfirm) void performRestart()
      else setShowRestartConfirm(true)
    }
    void refreshSourceOverview(provider === 'hubcap')
  }, [
    performRestart,
    refreshSourceOverview,
    showToast,
    skipConfirm,
    sourceAction,
    t.library.addToSteam,
    t.library.addToSteamSuccess,
    t.luaShop.liveChannel,
    t.luaShop.syncCurrent,
    t.luaShop.syncUpdated,
    upsertLuaGameState,
  ])

  const handleRemove = useCallback((appid: string) => {
    setRemovingAppId(appid)
    setShowRemoveConfirm(true)
  }, [])

  const handleManage = useCallback((appid: string) => {
    setManagedGame({
      appid: Number(appid),
      name: getCachedSteamGameInfo(appid)?.name
        || luaGameStatesRef.current[appid]?.gameName
        || `AppID ${appid}`,
    })
  }, [])
  const closeManagedGame = useCallback(() => setManagedGame(null), [])
  const handleManagerSync = useCallback(async (appid: string) => {
    setManagedGame(null)
    await handleSyncLuaGame(appid)
  }, [handleSyncLuaGame])
  const handleManagerSwitchLive = useCallback(async (appid: string) => {
    const current = luaGameStatesRef.current[appid]
    setManagedGame(null)
    setSourceAction({
      appid: Number.parseInt(appid, 10),
      gameName: current?.gameName || getCachedSteamGameInfo(appid)?.name || `AppID ${appid}`,
      operation: 'sync',
      purpose: 'switchLive',
      preferredProvider: current?.selectedSource ?? null,
      preferredChannel: 'live',
    })
  }, [])

  const confirmRemove = async () => {
    setShowRemoveConfirm(false)
    if (!removingAppId) return
    const removedState = luaGameStates[removingAppId]

    try {
      await invoke('remove_from_steam', { appid: parseInt(removingAppId) })
      setInstalledLuas((prev) => {
        const next = new Set(prev)
        next.delete(removingAppId!)
        return next
      })
      setLuaGameStates((prev) => {
        const next = { ...prev }
        delete next[removingAppId!]
        luaGameStatesRef.current = next
        return next
      })
      showToast(t.library.removeFromSteam, t.library.removeFromSteamSuccess, 'success')

      if (removedState?.requiresSteamRestart) {
        if (skipConfirm) performRestart()
        else setShowRestartConfirm(true)
      }
    } catch (err) {
      showToast(
        t.library.removeFromSteam,
        t.library.removeFromSteamError + ': ' + String(err),
        'error'
      )
    } finally {
      setRemovingAppId(null)
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Render
  // ─────────────────────────────────────────────────────────────────────────────

  const hubcapQuotaLimit = sourceSettings?.hubcap.daily.limit ?? 25
  const hubcapQuotaRemaining = sourceSettings?.hubcap.daily.remaining ?? hubcapQuotaLimit
  const hubcapQuotaRatio = hubcapQuotaLimit > 0 ? hubcapQuotaRemaining / hubcapQuotaLimit : 0
  const hubcapQuotaTone = hubcapQuotaRatio <= 0.2 ? 'low' : hubcapQuotaRatio <= 0.5 ? 'medium' : 'high'

  const filterTabs: { id: FilterTab; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'installed', label: t.luaShop.installed || 'Installed' },
    { id: 'notInstalled', label: 'Not Installed' },
  ]

  return (
    <div className="lua-shop-container">
      {/* ── Header ── */}
      <header className="lua-shop-header">
        <div>
          <h1>{t.luaShop.title}</h1>
          <p>{t.luaShop.description}</p>
        </div>
        <div className="lua-shop-stats">
          {!isLoading && (
            <>
              <span>
                {t.luaShop.available}: <strong>{catalogTotal ?? catalogItems.length}</strong>
              </span>
              <span>
                {t.luaShop.installed}: <strong>{installedLuas.size}</strong>
              </span>
              {sourceSettings?.hubcap.configured && (
                <span className={`lua-shop-quota quota-${hubcapQuotaTone}`}>
                  {t.luaShop.dailyAdds}: <strong>{hubcapQuotaRemaining}/{hubcapQuotaLimit}</strong>
                </span>
              )}
              <button
                className="lua-shop-refresh-btn"
                onClick={handleRefresh}
                disabled={isRefreshing}
                title="Refresh manifest list"
              >
                <RefreshCw size={14} color="currentColor" className={isRefreshing ? 'spin' : ''} />
              </button>
            </>
          )}
        </div>
      </header>

      {sourceSettings && (!sourceSettings.hubcap.configured || sourceSettings.hubcap.expired || sourceSettings.hubcap.expiringSoon) && (
        <div className={`lua-source-notice ${sourceSettings.hubcap.expired ? 'is-error' : 'is-warning'}`}>
          <div>
            <strong>{t.luaShop.hubcapRequiredTitle}</strong>
            <span>
              {!sourceSettings.hubcap.configured
                ? t.luaShop.hubcapRequiredMessage
                : sourceSettings.hubcap.expired
                  ? t.luaShop.hubcapExpired
                  : t.luaShop.hubcapExpiringSoon}
            </span>
          </div>
          <button
            type="button"
            onClick={() => window.dispatchEvent(new CustomEvent('navigate-to-settings', {
              detail: { section: 'lua-sources' },
            }))}
          >
            {t.luaShop.configureSources}
          </button>
        </div>
      )}

      {/* ── Controls: Store-style search + sort + layout + filter tabs ── */}
      <div className="lua-shop-controls">
        <div className="lua-shop-primary-toolbar">
          <div className="store-sort-dropdown">
            <button
              type="button"
              className="sort-toggle-btn"
              onClick={() => setSortOpen((value) => !value)}
              onBlur={() => window.setTimeout(() => setSortOpen(false), 180)}
            >
              <SlidersHorizontal size={14} />
              <span>{luaShopSort === 'az' ? 'A → Z' : luaShopSort === 'za' ? 'Z → A' : luaShopSort === 'appidAsc' ? 'AppID ↑' : 'AppID ↓'}</span>
            </button>
            {sortOpen && (
              <div className="sort-dropdown-menu">
                <button type="button" className={luaShopSort === 'az' ? 'active' : ''} onClick={() => setShopSort('az')}>A → Z</button>
                <button type="button" className={luaShopSort === 'za' ? 'active' : ''} onClick={() => setShopSort('za')}>Z → A</button>
                <button type="button" className={luaShopSort === 'appidAsc' ? 'active' : ''} onClick={() => setShopSort('appidAsc')}>AppID ↑</button>
                <button type="button" className={luaShopSort === 'appidDesc' ? 'active' : ''} onClick={() => setShopSort('appidDesc')}>AppID ↓</button>
              </div>
            )}
          </div>

          <div className="view-layout-toggle lua-shop-layout-toggle">
            {viewLayout === 'grid' && (
              <div className="lua-shop-grid-density">
                <button type="button" className={gridCols === 4 ? 'active' : ''} onClick={() => setShopGridCols(4)} title="4 Columns">4x</button>
                <button type="button" className={gridCols === 6 ? 'active' : ''} onClick={() => setShopGridCols(6)} title="6 Columns">6x</button>
                <button type="button" className={gridCols === 8 ? 'active' : ''} onClick={() => setShopGridCols(8)} title="8 Columns">8x</button>
              </div>
            )}
            <button type="button" className={viewLayout === 'grid' ? 'active' : ''} onClick={() => setShopLayout('grid')} title="Grid view">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="7" height="7" x="3" y="3" rx="1" /><rect width="7" height="7" x="14" y="3" rx="1" /><rect width="7" height="7" x="14" y="14" rx="1" /><rect width="7" height="7" x="3" y="14" rx="1" /></svg>
            </button>
            <button type="button" className={viewLayout === 'list' ? 'active' : ''} onClick={() => setShopLayout('list')} title="List view">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="7" height="7" x="3" y="3" rx="1" /><rect width="7" height="7" x="3" y="14" rx="1" /><path d="M14 6h7" /><path d="M14 10h7" /><path d="M14 14h7" /><path d="M14 18h7" /></svg>
            </button>
          </div>

          <label className="store-search lua-shop-command-search">
            <Search size={16} />
            <input
              ref={searchInputRef}
              type="text"
              placeholder={t.luaShop.searchPlaceholder || 'Search by game name or AppID...'}
              value={search}
              onChange={(event) => {
                const value = event.target.value
                setSearch(value)
                if (filterTab !== 'installed') setIsCatalogTransitioning(value.trim() !== debouncedSearch)
                setIsSearching(filterTab !== 'installed' && Boolean(value.trim()))
              }}
            />
            {search ? (
              <button
                type="button"
                className="lua-shop-search-clear"
                title="Clear search"
                onClick={(event) => {
                  event.preventDefault()
                  setSearch('')
                  if (filterTab !== 'installed') setIsCatalogTransitioning(Boolean(debouncedSearch))
                  searchInputRef.current?.focus()
                }}
              >
                <X size={14} />
              </button>
            ) : <kbd>Ctrl K</kbd>}
          </label>
        </div>

        <div className="lua-shop-filter-tabs">
          {filterTabs.map((tab) => (
            <button
              key={tab.id}
              className={`lua-shop-filter-tab ${filterTab === tab.id ? 'active' : ''}`}
              onClick={() => {
                setFilterTab(tab.id)
                setCatalogCursor(null)
                setCatalogCursorHistory([])
                setCatalogPageNumber(1)
                setIsSearching(tab.id !== 'installed' && Boolean(search.trim()))
                setIsCatalogTransitioning(tab.id !== 'installed')
                if (tab.id === 'installed') {
                  catalogRequestRef.current += 1
                  setIsLoading(false)
                  setIsCatalogTransitioning(false)
                }
              }}
            >
              {tab.label}
              {tab.id === 'installed' && installedLuas.size > 0 && (
                <span className="lua-shop-filter-count">{installedLuas.size}</span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* ── Body ── */}
      {isLoading || (filterTab === 'installed' && isInstalledCatalogLoading) ? (
        <div className="lua-shop-loading">
          <div className="spinner-large" />
          <p>{t.luaShop.loading || 'Đang tải dữ liệu...'}</p>
        </div>
      ) : filterTab !== 'installed' && catalogError && catalogItems.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>{catalogError}</p>
        </div>
      ) : debouncedSearch && filteredCatalogItems.length === 0 && isSearching ? (
        <div className="lua-shop-loading">
          <div className="spinner" />
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            Searching Steam catalog...
          </p>
        </div>
      ) : debouncedSearch && filteredCatalogItems.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            {t.luaShop.noResults || 'Không tìm thấy kết quả nào'}
          </p>
        </div>
      ) : displayedCatalogItems.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            {filterTab === 'installed'
              ? 'Chưa cài đặt Lua nào'
              : t.luaShop.noResults || 'Không tìm thấy kết quả nào'}
          </p>
        </div>
      ) : (
        <div className="lua-shop-results" ref={gridRef}
          data-layout={viewLayout}
          data-transitioning={isCatalogTransitioning ? 'true' : 'false'}
          aria-busy={isCatalogTransitioning}
          style={{ '--lua-shop-grid-cols': gridCols } as CSSProperties}
        >
          {isCatalogTransitioning && (
            <div className="lua-shop-transition-overlay" role="status" aria-live="polite">
              <div className="lua-shop-transition-card">
                <div className="spinner" />
                <span>{search.trim() ? 'Updating search results…' : 'Loading Lua catalog…'}</span>
              </div>
            </div>
          )}
          <div className="lua-shop-grid">
            {displayedCatalogItems.map((item, index) => {
              const appid = String(item.appid)
              return (
              <LuaShopGameCard
                key={`${appid}-${catalogPageNumber}-${filterTab}`}
                appid={appid}
                index={index}
                isInstalled={installedLuas.has(appid)}
                gameState={luaGameStates[appid]}
                availability={item.availability}
                onAdd={handleAddToSteam}
                onRemove={handleRemove}
                onSync={handleSyncLuaGame}
                onManage={handleManage}
              />
              )
            })}
          </div>

          {/* ── Pagination ── */}
          {(filterTab === 'installed'
            ? installedTotalPages > 1
            : catalogCursorHistory.length > 0 || Boolean(nextCatalogCursor)) && (
            <div className="lua-shop-pagination">
              <button
                disabled={filterTab === 'installed'
                  ? catalogPageNumber === 1
                  : catalogCursorHistory.length === 0}
                onClick={() => {
                  if (filterTab === 'installed') {
                    setCatalogPageNumber((page) => Math.max(1, page - 1))
                    return
                  }
                  setCatalogCursorHistory((history) => {
                    const next = [...history]
                    setCatalogCursor(next.pop() ?? null)
                    return next
                  })
                  setCatalogPageNumber((page) => Math.max(1, page - 1))
                }}
                className="pagination-btn"
              >
                <ChevronLeft size={18} />
                {t.luaShop.previous || 'Previous'}
              </button>
              <span className="pagination-info">
                {t.luaShop.page || 'Page'} {catalogPageNumber}
                {filterTab === 'installed' ? ` / ${installedTotalPages}` : ''}
              </span>
              <button
                disabled={filterTab === 'installed'
                  ? catalogPageNumber >= installedTotalPages
                  : !nextCatalogCursor}
                onClick={() => {
                  if (filterTab === 'installed') {
                    setCatalogPageNumber((page) => Math.min(installedTotalPages, page + 1))
                    return
                  }
                  if (!nextCatalogCursor) return
                  setCatalogCursorHistory((history) => [...history, catalogCursor])
                  setCatalogCursor(nextCatalogCursor)
                  setCatalogPageNumber((page) => page + 1)
                }}
                className="pagination-btn"
              >
                {t.luaShop.next || 'Next'}
                <ChevronRight size={18} />
              </button>
            </div>
          )}
        </div>
      )}

      {/* ── Remove confirm dialog ── */}
      {managedGame && (
        <LuaGameManagerDialog
          key={managedGame.appid}
          appid={managedGame.appid}
          gameName={managedGame.name}
          onClose={closeManagedGame}
          onState={upsertLuaGameState}
          onSync={handleManagerSync}
          onSwitchLive={handleManagerSwitchLive}
          onRemove={handleRemove}
          onRestartSteam={performRestart}
        />
      )}

      {sourceAction && (
        <LuaSourcePickerDialog
          key={`${sourceAction.purpose}-${sourceAction.appid}`}
          appid={sourceAction.appid}
          gameName={sourceAction.gameName}
          operation={sourceAction.operation}
          preferredProvider={sourceAction.preferredProvider}
          preferredChannel={sourceAction.preferredChannel}
          forcedChannel={sourceAction.purpose === 'switchLive' ? 'live' : null}
          onClose={() => setSourceAction(null)}
          onConfirm={handleSourceConfirm}
        />
      )}

      {showRemoveConfirm && removingAppId && (
        <ConfirmDialog
          title={t.library.confirmRemoveTitle}
          message={t.library.confirmRemoveMessage + '\n\n' + (getCachedSteamGameInfo(removingAppId)?.name || 'AppID ' + removingAppId)}
          confirmText={t.library.confirmRemoveYes}
          cancelText={t.library.confirmRemoveNo}
          variant="warning"
          onConfirm={confirmRemove}
          onCancel={() => {
            setShowRemoveConfirm(false)
            setRemovingAppId(null)
          }}
        />
      )}

      {/* ── Restart Steam confirm dialog ── */}
      {showRestartConfirm && (
        <ConfirmDialog
          title={t.library.restartSteamPrompt}
          message={t.library.restartSteamMessage}
          confirmText={t.library.restartSteamYes}
          cancelText={t.library.restartSteamNo}
          variant="info"
          onConfirm={() => {
            setShowRestartConfirm(false)
            performRestart()
          }}
          onCancel={() => setShowRestartConfirm(false)}
        >
          <div
            style={{
              marginTop: '20px',
              padding: '12px 16px',
              background: 'var(--theme-control-bg)',
              borderRadius: '8px',
              border: '1px solid var(--line)',
              display: 'flex',
              flexDirection: 'column',
              gap: '12px',
            }}
          >
            {/* Auto-install toggle */}
            <label
              style={{
                display: 'flex', alignItems: 'center', gap: '10px',
                cursor: 'pointer', margin: 0,
                paddingBottom: '12px', borderBottom: '1px solid var(--line)',
              }}
            >
              <button
                type="button"
                className={autoInstall ? 'settings-toggle is-on' : 'settings-toggle'}
                role="switch"
                aria-checked={autoInstall}
                onClick={(e) => {
                  e.preventDefault()
                  const next = !autoInstall
                  setAutoInstall(next)
                  localStorage.setItem('steamAutoInstall', String(next))
                }}
              >
                <span />
              </button>
              <span style={{ flex: 1, fontSize: '14px', color: autoInstall ? 'var(--text-strong)' : 'var(--muted)' }}>
                {t.library.autoInstallAfterRestart}
              </span>
            </label>

            {/* Skip-confirm toggle */}
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer', margin: 0 }}>
              <button
                type="button"
                className={skipConfirm ? 'settings-toggle is-on' : 'settings-toggle'}
                role="switch"
                aria-checked={skipConfirm}
                onClick={(e) => {
                  e.preventDefault()
                  const next = !skipConfirm
                  setSkipConfirm(next)
                  localStorage.setItem('steamSkipRestartConfirm', String(next))
                }}
              >
                <span />
              </button>
              <span style={{ flex: 1, fontSize: '13px', color: 'var(--muted)' }}>
                {t.library.rememberThisChoice}
              </span>
            </label>
          </div>
        </ConfirmDialog>
      )}
    </div>
  )
}
