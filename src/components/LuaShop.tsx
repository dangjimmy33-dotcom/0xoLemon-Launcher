import { useState, useEffect, useMemo, useRef, useCallback, memo } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { Search, ChevronLeft, ChevronRight, Plus, Trash2, RefreshCw, CheckCircle } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'
import {
  fetchSteamGameInfo,
  getCachedSteamGameInfo,
  seedSteamGameInfo,
  steamCapsuleImageUrl,
  steamHeaderImageUrl,
  type SteamGameInfo,
  type SteamStoreSearchItem,
} from '../lib/steamGameInfo'
import './LuaShop.css'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

type FilterTab = 'all' | 'installed' | 'notInstalled'

function emitLuaShopToast(
  title: string,
  message: string,
  severity: 'success' | 'error' | 'info' = 'info',
) {
  window.dispatchEvent(new CustomEvent('0xo-toast', {
    detail: { category: 'launcher', severity, title, message, dedupeKey: 'lua-shop:' + title },
  }))
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

// ─────────────────────────────────────────────────────────────────────────────
// Module-level manifest cache
// list_available_manifests is fetched ONCE per session, not on every remount
// ─────────────────────────────────────────────────────────────────────────────

let _manifestCache: string[] | null = null
let _manifestPromise: Promise<string[]> | null = null
let _manifestGeneration = 0

async function getManifests(bust = false): Promise<string[]> {
  if (bust) {
    _manifestGeneration += 1
    _manifestCache = null
    _manifestPromise = null
  }
  if (_manifestCache !== null) return _manifestCache
  if (_manifestPromise) return _manifestPromise
  const generation = _manifestGeneration
  const request = invoke<string[]>('list_available_manifests', { force: bust })
    .then((ids) => {
      const sorted = ids.sort((a, b) => parseInt(b) - parseInt(a))
      if (generation === _manifestGeneration) {
        _manifestCache = sorted
        _manifestPromise = null
      }
      return sorted
    })
    .catch((error) => {
      if (generation === _manifestGeneration) _manifestPromise = null
      throw error
    })
  _manifestPromise = request
  return request
}

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

function localLuaSearchMatches(appids: string[], normalizedQuery: string) {
  return appids.filter((appid) => {
    if (appid.includes(normalizedQuery)) return true
    const cached = getCachedSteamGameInfo(appid)
    return cached ? titleMatchesQuery(cached.name, normalizedQuery) : false
  })
}

function mergeSearchResults(
  normalizedQuery: string,
  localMatches: string[],
  remoteMatches: string[],
) {
  const remoteRank = new Map(remoteMatches.map((appid, index) => [appid, index]))
  const merged = Array.from(new Set([...remoteMatches, ...localMatches]))
  return merged.sort((a, b) => {
    const aInfo = getCachedSteamGameInfo(a)
    const bInfo = getCachedSteamGameInfo(b)
    const aName = aInfo ? normalizeSearchText(aInfo.name) : ''
    const bName = bInfo ? normalizeSearchText(bInfo.name) : ''
    const rank = (appid: string, title: string) => {
      if (appid === normalizedQuery) return 0
      if (appid.startsWith(normalizedQuery)) return 1
      if (title === normalizedQuery) return 2
      if (title.startsWith(normalizedQuery)) return 3
      if (title.includes(normalizedQuery)) return 4
      return 5
    }
    const rankDelta = rank(a, aName) - rank(b, bName)
    if (rankDelta !== 0) return rankDelta
    return (remoteRank.get(a) ?? Number.MAX_SAFE_INTEGER) - (remoteRank.get(b) ?? Number.MAX_SAFE_INTEGER)
  })
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
  onAdd,
  onRemove,
}: {
  appid: string
  index: number
  isInstalled: boolean
  onAdd: (appid: string) => void
  onRemove: (appid: string) => void
}) {
  // Initialise from module-level cache so cached cards render instantly
  const [info, setInfo] = useState<SteamGameInfo | null>(getCachedSteamGameInfo(appid) ?? null)
  const [isProcessing, setIsProcessing] = useState(false)
  const [imageLoaded, setImageLoaded] = useState(false)
  const [imageIndex, setImageIndex] = useState(0)
  const [imageFailed, setImageFailed] = useState(false)
  const shouldLoad = true
  const appliedHeaderRef = useRef<string | null>(null)
  const { t } = useLocale()

  // ── IntersectionObserver: trigger fetch only when card enters viewport ──────
  useEffect(() => {
    appliedHeaderRef.current = null
    setImageLoaded(false)
    setImageIndex(0)
    setImageFailed(false)
  }, [appid])

  // ── Fetch game info once the card is visible ────────────────────────────────
  useEffect(() => {
    if (!shouldLoad) {
      return
    }
    const cached = getCachedSteamGameInfo(appid)
    if (cached) {
      setInfo((current) => current ?? cached)
      return
    }
    let mounted = true
    fetchSteamGameInfo(appid).then((result) => {
      if (mounted && result) setInfo(result)
    })
    return () => { mounted = false }
  }, [shouldLoad, appid])

  const handleAction = async () => {
    setIsProcessing(true)
    try {
      if (isInstalled) await onRemove(appid)
      else await onAdd(appid)
    } finally {
      setIsProcessing(false)
    }
  }

  const imageCandidates = useMemo(() => {
    const candidates = [
      info?.header_image,
      steamHeaderImageUrl(appid),
      `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/header.jpg`,
      steamCapsuleImageUrl(appid),
      `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/capsule_231x87.jpg`,
    ].filter((url): url is string => Boolean(url))
    return Array.from(new Set(candidates))
  }, [appid, info?.header_image])

  const imageUrl = shouldLoad && !imageFailed
    ? imageCandidates[Math.min(imageIndex, imageCandidates.length - 1)] ?? ''
    : ''

  useEffect(() => {
    const header = info?.header_image
    if (!header || appliedHeaderRef.current === header) return
    appliedHeaderRef.current = header
    setImageIndex(0)
    setImageFailed(false)
    setImageLoaded((loaded) => loaded && header === imageUrl)
  }, [imageUrl, info?.header_image])

  return (
    <div className="lua-shop-card" style={{ animationDelay: `${Math.min(index, 23) * 30}ms` }}>
      {/* ── Thumbnail ── */}
      <div className={`lua-shop-card-image ${!imageLoaded && !imageFailed ? 'is-loading' : ''} ${imageFailed ? 'is-error' : ''}`}>
        {isInstalled && (
          <div className="lua-shop-installed-badge">
            <CheckCircle size={12} />
            Installed
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
      </div>
    </div>
  )
})

// ─────────────────────────────────────────────────────────────────────────────
// LuaShop main component
// ─────────────────────────────────────────────────────────────────────────────

export function LuaShop() {
  const [allAppIds, setAllAppIds] = useState<string[]>(_manifestCache ?? [])
  const [isLoading, setIsLoading] = useState(_manifestCache === null)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [search, setSearch] = useState('')
  const [isSearching, setIsSearching] = useState(false)
  const [manifestError, setManifestError] = useState<string | null>(null)
  const [searchResults, setSearchResults] = useState<string[]>([])
  const [filterTab, setFilterTab] = useState<FilterTab>('all')
  const [installedLuas, setInstalledLuas] = useState<Set<string>>(new Set())
  const [showRestartConfirm, setShowRestartConfirm] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const [removingAppId, setRemovingAppId] = useState<string | null>(null)
  const [autoInstall, setAutoInstall] = useState(
    localStorage.getItem('steamAutoInstall') === 'true'
  )
  const [skipConfirm, setSkipConfirm] = useState(
    localStorage.getItem('steamSkipRestartConfirm') === 'true'
  )
  const [pageState, setPageState] = useState({ scope: '', value: 1 })
  const ITEMS_PER_PAGE = 24

  const { t } = useLocale()
  const gridRef = useRef<HTMLDivElement>(null)
  const allAppIdsSetRef = useRef<Set<string>>(new Set(_manifestCache ?? []))
  const manifestRequestRef = useRef(0)
  const pageScope = search + '\u0001' + filterTab
  const currentPage = pageState.scope === pageScope ? pageState.value : 1
  const changePage = useCallback((update: (page: number) => number) => {
    setPageState((current) => {
      const page = current.scope === pageScope ? current.value : 1
      return { scope: pageScope, value: update(page) }
    })
  }, [pageScope])

  const fetchInstalledLuas = useCallback(async () => {
    try {
      const luas = await invoke<string[]>('list_installed_luas')
      setInstalledLuas(new Set(luas))
    } catch {
      // A manifest refresh can still succeed when Steam is not available.
    }
  }, [])

  // ── Initial load ────────────────────────────────────────────────────────────
  useEffect(() => {
    const installedTimer = window.setTimeout(() => {
      void fetchInstalledLuas()
    }, 0)
    if (_manifestCache !== null) {
      return () => window.clearTimeout(installedTimer)
    }
    const requestId = ++manifestRequestRef.current
    getManifests()
      .then((ids) => {
        if (requestId !== manifestRequestRef.current) return
        setAllAppIds(ids)
        allAppIdsSetRef.current = new Set(ids)
        setManifestError(null)
      })
      .catch((error) => {
        if (requestId === manifestRequestRef.current) setManifestError(String(error))
      })
      .finally(() => {
        if (requestId === manifestRequestRef.current) setIsLoading(false)
      })
    return () => {
      window.clearTimeout(installedTimer)
      manifestRequestRef.current += 1
    }
  }, [fetchInstalledLuas])

  // ── Manual refresh (busts manifest cache) ───────────────────────────────────
  const handleRefresh = useCallback(async () => {
    const requestId = ++manifestRequestRef.current
    setIsRefreshing(true)
    try {
      const ids = await getManifests(true)
      if (requestId !== manifestRequestRef.current) return
      const previousCount = allAppIds.length
      setAllAppIds(ids)
      allAppIdsSetRef.current = new Set(ids)
      setManifestError(null)
      await fetchInstalledLuas()
      const message = ids.length === previousCount
        ? 'Manifest list is already current (' + ids.length + ')'
        : 'Updated manifest list: ' + ids.length + ' entries (was ' + previousCount + ')'
      emitLuaShopToast('Lua Shop refreshed', message, 'success')
    } catch (error) {
      if (requestId !== manifestRequestRef.current) return
      const message = String(error)
      setManifestError(message)
      emitLuaShopToast('Lua Shop refresh failed', message, 'error')
    } finally {
      if (requestId === manifestRequestRef.current) setIsRefreshing(false)
    }
  }, [allAppIds.length, fetchInstalledLuas])

  // ── Search: immediate local + debounced Steam Store API ─────────────────────
  // Results are ALWAYS filtered to only show appids present in the manifest list.
  useEffect(() => {
    let cancelled = false
    const rawQuery = search.trim()
    const q = normalizeSearchText(rawQuery)
    if (!q) {
      const resetTimer = window.setTimeout(() => {
        if (cancelled) return
        setSearchResults([])
        setIsSearching(false)
      }, 0)
      return () => {
        cancelled = true
        window.clearTimeout(resetTimer)
      }
    }

    const manifestSet = allAppIdsSetRef.current
    const localMatches = localLuaSearchMatches(allAppIds, q)
    setSearchResults(localMatches)
    setIsSearching(true)

    const timer = window.setTimeout(async () => {
      try {
        const items = await invoke<SteamStoreSearchItem[]>('search_steam_store', { term: rawQuery })
        if (cancelled) return

        items.forEach((item) => {
          const appid = String(item.id)
          if (manifestSet.has(appid) && item.name && !getCachedSteamGameInfo(appid)) {
            seedSteamGameInfo(appid, {
              name: item.name,
              header_image: item.header_image || steamHeaderImageUrl(appid),
            })
          }
        })

        const apiMatches = items
          .map((item) => String(item.id))
          .filter((appid) => manifestSet.has(appid))
        const refreshedLocal = localLuaSearchMatches(allAppIds, q)

        if (!cancelled) {
          setSearchResults(mergeSearchResults(q, refreshedLocal, apiMatches))
        }
      } catch (error) {
        console.warn('Lua Shop search failed', error)
      } finally {
        if (!cancelled) setIsSearching(false)
      }
    }, /^\d+$/.test(q) ? 120 : 220)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [search, allAppIds])

  // ── Combined filter: search results + installed/not-installed tab ───────────
  const filteredAppIds = useMemo(() => {
    const base = search.trim() ? searchResults : allAppIds

    if (filterTab === 'installed') {
      return base.filter((id) => installedLuas.has(id))
    }
    if (filterTab === 'notInstalled') {
      return base.filter((id) => !installedLuas.has(id))
    }
    return base
  }, [allAppIds, search, searchResults, filterTab, installedLuas])

  const totalPages = Math.ceil(filteredAppIds.length / ITEMS_PER_PAGE)

  const paginatedAppIds = useMemo(() => {
    const start = (currentPage - 1) * ITEMS_PER_PAGE
    return filteredAppIds.slice(start, start + ITEMS_PER_PAGE)
  }, [filteredAppIds, currentPage])

  // Reset to page 1 when filters change
  // Scroll grid to top on page change (instant — no jank from smooth scroll)
  useEffect(() => {
    const el = gridRef.current
    if (el) el.scrollTo({ top: 0, behavior: 'instant' as ScrollBehavior })
  }, [currentPage])

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

  // ── Add ─────────────────────────────────────────────────────────────────────
  const handleAddToSteam = useCallback(async (appid: string) => {
    const numAppid = parseInt(appid)
    const gameName = getCachedSteamGameInfo(appid)?.name || 'AppID ' + appid

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

    try {
      const checkResult = await invoke('check_steam_update', { appid: numAppid }) as {
        needs_update: boolean; reason: string; is_missing: boolean
      }

      let forceUpdate = false
      if (checkResult.needs_update) {
        if (checkResult.is_missing) {
          showToast(t.library.addToSteam, `Creating config for ${gameName} (30-60s)...`, 'info')
          forceUpdate = true
        } else {
          const { ask } = await import('@tauri-apps/plugin-dialog')
          const shouldUpdate = await ask(
            `Update available.\nReason: ${checkResult.reason}\n\nFetch latest version?`,
            { title: 'Data Update', kind: 'info' }
          )
          if (shouldUpdate) {
            showToast(t.library.addToSteam, `Downloading update for ${gameName} (30-60s)...`, 'info')
            forceUpdate = true
          }
        }
      }

      await invoke('add_to_steam', { appid: numAppid, forceUpdate })
      setInstalledLuas((prev) => new Set([...prev, appid]))
      showToast(t.library.addToSteam, t.library.addToSteamSuccess, 'success')

      if (skipConfirm) performRestart()
      else setShowRestartConfirm(true)
    } catch (err) {
      showToast(t.library.addToSteam, t.library.addToSteamError + ': ' + String(err), 'error')
    }
  }, [performRestart, showToast, skipConfirm, t])

  // ── Remove ──────────────────────────────────────────────────────────────────
  const handleRemove = useCallback((appid: string) => {
    setRemovingAppId(appid)
    setShowRemoveConfirm(true)
  }, [])

  const confirmRemove = async () => {
    setShowRemoveConfirm(false)
    if (!removingAppId) return

    try {
      await invoke('remove_from_steam', { appid: parseInt(removingAppId) })
      setInstalledLuas((prev) => {
        const next = new Set(prev)
        next.delete(removingAppId!)
        return next
      })
      showToast(t.library.removeFromSteam, t.library.removeFromSteamSuccess, 'success')

      if (skipConfirm) performRestart()
      else setShowRestartConfirm(true)
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
                {t.luaShop.available}: <strong>{allAppIds.length}</strong>
              </span>
              <span>
                {t.luaShop.installed}: <strong>{installedLuas.size}</strong>
              </span>
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

      {/* ── Controls: search + filter tabs ── */}
      <div className="lua-shop-controls">
        <div className="lua-shop-search">
          <Search size={16} />
          <input
            type="text"
            placeholder={t.luaShop.searchPlaceholder || 'Search by game name or AppID...'}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {isSearching && <div className="spinner" style={{ marginLeft: '8px' }} />}
        </div>

        <div className="lua-shop-filter-tabs">
          {filterTabs.map((tab) => (
            <button
              key={tab.id}
              className={`lua-shop-filter-tab ${filterTab === tab.id ? 'active' : ''}`}
              onClick={() => setFilterTab(tab.id)}
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
      {isLoading ? (
        <div className="lua-shop-loading">
          <div className="spinner-large" />
          <p>{t.luaShop.loading || 'Đang tải dữ liệu...'}</p>
        </div>
      ) : manifestError && allAppIds.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>{manifestError}</p>
        </div>
      ) : search.trim() && filteredAppIds.length === 0 && isSearching ? (
        <div className="lua-shop-loading">
          <div className="spinner" />
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            Searching Steam catalog...
          </p>
        </div>
      ) : search.trim() && filteredAppIds.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            {t.luaShop.noResults || 'Không tìm thấy kết quả nào'}
          </p>
        </div>
      ) : filteredAppIds.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            {filterTab === 'installed'
              ? 'Chưa cài đặt Lua nào'
              : t.luaShop.noResults || 'Không tìm thấy kết quả nào'}
          </p>
        </div>
      ) : (
        <>
          <div className="lua-shop-grid" ref={gridRef}>
            {paginatedAppIds.map((appid, index) => (
              <LuaShopGameCard
                key={`${appid}-${currentPage}-${filterTab}`}
                appid={appid}
                index={index}
                isInstalled={installedLuas.has(appid)}
                onAdd={handleAddToSteam}
                onRemove={handleRemove}
              />
            ))}
          </div>

          {/* ── Pagination ── */}
          {totalPages > 1 && (
            <div className="lua-shop-pagination">
              <button
                disabled={currentPage === 1}
                onClick={() => changePage((page) => Math.max(1, page - 1))}
                className="pagination-btn"
              >
                <ChevronLeft size={18} />
                {t.luaShop.previous || 'Previous'}
              </button>
              <span className="pagination-info">
                {t.luaShop.page || 'Page'} {currentPage} / {totalPages}
              </span>
              <button
                disabled={currentPage === totalPages}
                onClick={() => changePage((page) => Math.min(totalPages, page + 1))}
                className="pagination-btn"
              >
                {t.luaShop.next || 'Next'}
                <ChevronRight size={18} />
              </button>
            </div>
          )}
        </>
      )}

      {/* ── Remove confirm dialog ── */}
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
