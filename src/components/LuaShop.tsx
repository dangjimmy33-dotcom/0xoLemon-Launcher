import { useState, useEffect, useMemo, useRef, useCallback, memo } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Search, ChevronLeft, ChevronRight, Plus, Trash2, RefreshCw, CheckCircle, Settings } from 'lucide-react'
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
  LuaAddQuotaState,
  LuaCatalogItem,
  LuaCatalogSearchPage,
  LuaGameState,
  LuaSourceAvailability,
  LuaSourceOperation,
  LuaSourceProvider,
  LuaSourceSettingsState,
} from '../types'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

type FilterTab = 'all' | 'installed' | 'notInstalled'

type LuaSourceActionIntent = {
  appid: number
  gameName: string
  operation: LuaSourceOperation
  purpose: 'add' | 'update' | 'sync' | 'switchLive'
  preferredProvider: LuaSourceProvider | null
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
    variant === 'warning' ? '#ffa500' : variant === 'error' ? '#ff4d4d' : '#4ade80'
  const accentBg =
    variant === 'warning' ? 'rgba(255,165,0,.2)' : variant === 'error' ? 'rgba(255,0,0,.2)' : 'rgba(74,222,128,.2)'
  const accentBorder =
    variant === 'warning' ? '1px solid rgba(255,165,0,.4)' : variant === 'error' ? '1px solid rgba(255,0,0,.4)' : '1px solid rgba(74,222,128,.4)'

  return createPortal(
    <div
      className="dialog-backdrop"
      onClick={onCancel}
      style={{
        position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)',
        zIndex: 99999, display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}
    >
      <div
        className="dialog-box"
        onClick={(e) => e.stopPropagation()}
        style={{
          background: 'linear-gradient(135deg, rgba(30,30,40,.98), rgba(20,20,30,.98))',
          border: '1px solid rgba(255,255,255,.1)', borderRadius: '12px',
          padding: '24px', maxWidth: '480px', width: '90%',
          boxShadow: '0 20px 60px rgba(0,0,0,.5)',
        }}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: '18px', fontWeight: 700, color: accentColor }}>
          {title}
        </h3>
        <p style={{ margin: '0 0 20px', color: '#ccc', fontSize: '14px', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>
          {message}
        </p>
        {children}
        <div style={{ display: 'flex', gap: '12px', marginTop: '20px' }}>
          <button
            onClick={onCancel}
            style={{
              flex: 1, padding: '10px 16px', borderRadius: '6px',
              background: 'rgba(255,255,255,.1)', border: '1px solid rgba(255,255,255,.2)',
              color: '#fff', fontWeight: 600, cursor: 'pointer',
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
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [sourceSettings, setSourceSettings] = useState<LuaSourceSettingsState | null>(null)
  const [addQuota, setAddQuota] = useState<LuaAddQuotaState | null>(null)
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

  const refreshSourceOverview = useCallback(async () => {
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
    const [settingsResult, quotaResult] = await Promise.allSettled([
      invoke<LuaSourceSettingsState>('get_lua_source_settings'),
      invoke<LuaAddQuotaState>('get_lua_add_quota', { timezone }),
    ])
    if (settingsResult.status === 'fulfilled') setSourceSettings(settingsResult.value)
    if (quotaResult.status === 'fulfilled') setAddQuota(quotaResult.value)
  }, [])

  // ── Initial local state ─────────────────────────────────────────────────────
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void Promise.all([fetchInstalledLuas(), refreshSourceOverview()])
    }, 0)
    return () => window.clearTimeout(timer)
  }, [fetchInstalledLuas, refreshSourceOverview])

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
      }
    }
  }, [catalogCursor, debouncedSearch])

  useEffect(() => {
    if (filterTab === 'installed') {
      catalogRequestRef.current += 1
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
        refreshSourceOverview(),
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

  const installedTotalPages = Math.max(1, Math.ceil(filteredCatalogItems.length / ITEMS_PER_PAGE))
  const displayedCatalogItems = filterTab === 'installed'
    ? filteredCatalogItems.slice(
        (catalogPageNumber - 1) * ITEMS_PER_PAGE,
        catalogPageNumber * ITEMS_PER_PAGE,
      )
    : filteredCatalogItems

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
    })
  }, [])

  const handleSourceConfirm = useCallback(async (provider: LuaSourceProvider) => {
    if (!sourceAction) return
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
    let state: LuaGameState
    if (sourceAction.purpose === 'add') {
      state = await invoke<LuaGameState>('install_lua_game_from_source', {
        request: {
          appid: sourceAction.appid,
          gameName: sourceAction.gameName,
          channel: 'live',
          buildId: null,
          accessToken: null,
          statSteamId: null,
          conflictResolution: null,
          provider,
          requestId: crypto.randomUUID(),
          timezone,
        },
      })
    } else if (sourceAction.purpose === 'switchLive') {
      state = await invoke<LuaGameState>('set_lua_game_channel', {
        request: {
          appid: sourceAction.appid,
          channel: 'live',
          buildId: null,
          conflictResolution: 'restoreLive',
          provider,
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
    if (state.requiresSteamRestart) {
      if (skipConfirm) void performRestart()
      else setShowRestartConfirm(true)
    }
    void refreshSourceOverview()
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
              {addQuota && (
                <span className={`lua-shop-quota quota-${addQuota.remaining <= 2 ? 'low' : addQuota.remaining <= 6 ? 'medium' : 'high'}`}>
                  {t.luaShop.dailyAdds}: <strong>{addQuota.remaining}/{addQuota.limit}</strong>
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

      {/* ── Controls: search + filter tabs ── */}
      <div className="lua-shop-controls">
        <div className="lua-shop-search">
          <Search size={16} />
          <input
            type="text"
            placeholder={t.luaShop.searchPlaceholder || 'Search by game name or AppID...'}
            value={search}
            onChange={(e) => {
              setSearch(e.target.value)
              setIsSearching(filterTab !== 'installed' && Boolean(e.target.value.trim()))
            }}
          />
          {isSearching && <div className="spinner" style={{ marginLeft: '8px' }} />}
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
                if (tab.id === 'installed') {
                  catalogRequestRef.current += 1
                  setIsLoading(false)
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
        <div className="lua-shop-results" ref={gridRef}>
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
              background: 'rgba(0,0,0,.2)',
              borderRadius: '8px',
              border: '1px solid rgba(255,255,255,.05)',
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
                paddingBottom: '12px', borderBottom: '1px solid rgba(255,255,255,.05)',
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
              <span style={{ flex: 1, fontSize: '14px', color: autoInstall ? '#fff' : 'rgba(255,255,255,.6)' }}>
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
              <span style={{ flex: 1, fontSize: '13px', color: 'rgba(255,255,255,.5)' }}>
                {t.library.rememberThisChoice}
              </span>
            </label>
          </div>
        </ConfirmDialog>
      )}
    </div>
  )
}
