import { useState, useEffect, useMemo, useRef } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { Search, ChevronLeft, ChevronRight, Plus, Trash2 } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'
import './LuaShop.css'

interface SteamGameInfo {
  name: string
  header_image: string
}

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
  useEffect(() => setMounted(true), [])
  if (!mounted) return null

  return createPortal(
    <div
      className="dialog-backdrop"
      onClick={onCancel}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.7)',
        zIndex: 99999,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center'
      }}
    >
      <div
        className="dialog-box"
        onClick={(e) => e.stopPropagation()}
        style={{
          background: 'linear-gradient(135deg, rgba(30,30,40,0.98), rgba(20,20,30,0.98))',
          border: '1px solid rgba(255,255,255,0.1)',
          borderRadius: '12px',
          padding: '24px',
          maxWidth: '480px',
          width: '90%',
          boxShadow: '0 20px 60px rgba(0,0,0,0.5)',
        }}
      >
        <h3
          style={{
            margin: '0 0 12px',
            fontSize: '18px',
            fontWeight: 700,
            color: variant === 'warning' ? '#ffa500' : variant === 'error' ? '#ff4d4d' : '#4ade80',
          }}
        >
          {title}
        </h3>
        <p style={{ margin: '0 0 20px', color: '#ccc', fontSize: '14px', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>{message}</p>
        {children}
        <div style={{ display: 'flex', gap: '12px', marginTop: '20px' }}>
          <button
            onClick={onCancel}
            style={{
              flex: 1,
              padding: '10px 16px',
              borderRadius: '6px',
              background: 'rgba(255,255,255,0.1)',
              border: '1px solid rgba(255,255,255,0.2)',
              color: '#fff',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            {cancelText}
          </button>
          <button
            onClick={onConfirm}
            style={{
              flex: 1,
              padding: '10px 16px',
              borderRadius: '6px',
              background:
                variant === 'warning' ? 'rgba(255,165,0,0.2)' : variant === 'error' ? 'rgba(255,0,0,0.2)' : 'rgba(74,222,128,0.2)',
              border:
                variant === 'warning' ? '1px solid rgba(255,165,0,0.4)' : variant === 'error' ? '1px solid rgba(255,0,0,0.4)' : '1px solid rgba(74,222,128,0.4)',
              color: variant === 'warning' ? '#ffa500' : variant === 'error' ? '#ff4d4d' : '#4ade80',
              fontWeight: 600,
              cursor: 'pointer',
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

const gameInfoCache = new Map<string, SteamGameInfo>()
const pendingRequests = new Map<string, Promise<SteamGameInfo | null>>()

let activeRequests = 0
const MAX_CONCURRENT_REQUESTS = 5
const requestQueue: Array<() => void> = []

function processQueue() {
  while (activeRequests < MAX_CONCURRENT_REQUESTS && requestQueue.length > 0) {
    const next = requestQueue.shift()
    if (next) next()
  }
}

async function fetchGameInfo(appid: string): Promise<SteamGameInfo | null> {
  if (gameInfoCache.has(appid)) return gameInfoCache.get(appid)!
  if (pendingRequests.has(appid)) return pendingRequests.get(appid)!

  if (activeRequests >= MAX_CONCURRENT_REQUESTS) {
    return new Promise((resolve) => {
      requestQueue.push(() => {
        fetchGameInfoInternal(appid).then(resolve)
      })
    })
  }

  return fetchGameInfoInternal(appid)
}

async function fetchGameInfoInternal(appid: string): Promise<SteamGameInfo | null> {
  activeRequests++

  const promise = (async () => {
    try {
      const result = await invoke<SteamGameInfo>('fetch_steam_game_name', { appid: parseInt(appid) })
      if (result?.name) {
        gameInfoCache.set(appid, result)
        return result
      }
    } catch (_) { }

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
            header_image: entry.data.header_image || `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`
          }
          gameInfoCache.set(appid, info)
          return info
        }
      }
    } catch (_) { }

    return null
  })()

  pendingRequests.set(appid, promise)

  const result = await promise

  pendingRequests.delete(appid)
  activeRequests--
  processQueue()

  return result
}

function LuaShopGameCard({ appid, index, isInstalled, onAdd, onRemove }: {
  appid: string
  index: number
  isInstalled: boolean
  onAdd: (appid: string) => void
  onRemove: (appid: string) => void
}) {
  const [info, setInfo] = useState<SteamGameInfo | null>(gameInfoCache.get(appid) ?? null)
  const [isProcessing, setIsProcessing] = useState(false)
  const [imageLoaded, setImageLoaded] = useState(false)
  const { t } = useLocale()

  useEffect(() => {
    if (gameInfoCache.has(appid)) return

    let mounted = true
    fetchGameInfo(appid).then(result => {
      if (mounted && result) {
        setInfo(result)
      }
    })
    return () => { mounted = false }
  }, [appid])

  const handleAction = async () => {
    setIsProcessing(true)
    try {
      if (isInstalled) {
        await onRemove(appid)
      } else {
        await onAdd(appid)
      }
    } finally {
      setIsProcessing(false)
    }
  }

  const imageUrl = info?.header_image || `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/header.jpg`

  return (
    <div className="lua-shop-card" style={{ animationDelay: `${index * 30}ms` }}>
      <div className={`lua-shop-card-image ${!imageLoaded ? 'is-loading' : ''}`}>
        <img
          src={imageUrl}
          alt={info?.name || `AppID ${appid}`}
          loading="lazy"
          className={imageLoaded ? 'loaded' : 'loading'}
          onLoad={() => setImageLoaded(true)}
          onError={(e) => {
            const target = e.target as HTMLImageElement
            if (target.src.includes('cloudflare')) {
              target.src = `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`
            } else if (!target.src.includes('capsule_231x87')) {
              target.src = `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/capsule_231x87.jpg`
            } else {
              setImageLoaded(true)
            }
          }}
        />
      </div>
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
              {isProcessing ? t.luaShop.removing || 'Đang xóa...' : t.luaShop.removeFromSteam || 'Gỡ khỏi Steam'}
            </>
          ) : (
            <>
              <Plus size={16} />
              {isProcessing ? t.luaShop.adding || 'Đang thêm...' : t.luaShop.addToSteam || 'Thêm vào Steam'}
            </>
          )}
        </button>
      </div>
    </div>
  )
}

export function LuaShop() {
  const [allAppIds, setAllAppIds] = useState<string[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [isSearching, setIsSearching] = useState(false)
  const [installedLuas, setInstalledLuas] = useState<Set<string>>(new Set())
  const [showRestartConfirm, setShowRestartConfirm] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const [removingAppId, setRemovingAppId] = useState<string | null>(null)
  const [autoInstall, setAutoInstall] = useState(localStorage.getItem('steamAutoInstall') === 'true')
  const [skipConfirm, setSkipConfirm] = useState(localStorage.getItem('steamSkipRestartConfirm') === 'true')
  const [currentPage, setCurrentPage] = useState(1)
  const ITEMS_PER_PAGE = 24

  const { t } = useLocale()

  useEffect(() => {
    fetchAvailableManifests()
    fetchInstalledLuas()
  }, [])

  const fetchInstalledLuas = async () => {
    try {
      const luas = await invoke<string[]>('list_installed_luas')
      setInstalledLuas(new Set(luas))
    } catch (_) { }
  }

  const fetchAvailableManifests = async () => {
    setIsLoading(true)
    try {
      const appids = await invoke<string[]>('list_available_manifests')
      setAllAppIds(appids.sort((a, b) => parseInt(b) - parseInt(a)))
    } catch (err) {
      setAllAppIds([])
    } finally {
      setIsLoading(false)
    }
  }

  const [searchResults, setSearchResults] = useState<string[]>([])

  useEffect(() => {
    if (!search.trim()) {
      setSearchResults([])
      setIsSearching(false)
      return
    }

    const q = search.toLowerCase().trim()

    // 1. Tìm theo AppID exact match (local manifests)
    const appidMatches = allAppIds.filter(appid => appid.includes(q))

    // 2. Tìm theo tên đã cache (local manifests)
    const cachedNameMatches = allAppIds.filter(appid => {
      const info = gameInfoCache.get(appid)
      return info && info.name.toLowerCase().includes(q)
    })

    // Gộp kết quả local trước
    const localMatches = Array.from(new Set([...appidMatches, ...cachedNameMatches]))
    setSearchResults(localMatches)

    // 3. Gọi Steam Store Search API
    setIsSearching(true)
    const searchSteamAPI = async () => {
      try {
        const response = await fetch(
          `https://store.steampowered.com/api/storesearch/?term=${encodeURIComponent(search)}&cc=us&l=english`,
          { signal: AbortSignal.timeout(8000) }
        )

        if (response.ok) {
          const data = await response.json()

          // Lấy TẤT CẢ kết quả từ Steam API (không filter theo allAppIds)
          const apiMatches = (data.items || [])
            .map((item: any) => String(item.id))
            .slice(0, 25) // Giới hạn 25 kết quả

            // Cache tên game từ API
            (data.items || []).forEach((item: any) => {
              const appid = String(item.id)
              if (item.name && !gameInfoCache.has(appid)) {
                gameInfoCache.set(appid, {
                  name: item.name,
                  header_image: item.tiny_image?.replace('capsule_sm_120', 'header') ||
                    `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`
                })
              }
            })

          // Gộp kết quả: local manifests trước, Steam API sau
          const finalResults = Array.from(new Set([...localMatches, ...apiMatches]))
          setSearchResults(finalResults)
        }
      } catch (err) {
        console.error('Steam API search failed:', err)
      } finally {
        setIsSearching(false)
      }
    }

    // Debounce 300ms trước khi gọi API
    const timer = setTimeout(searchSteamAPI, 300)
    return () => clearTimeout(timer)
  }, [search, allAppIds])

  const filteredAppIds = useMemo(() => {
    if (!search.trim()) return allAppIds
    return searchResults
  }, [allAppIds, search, searchResults])

  const totalPages = Math.ceil(filteredAppIds.length / ITEMS_PER_PAGE)
  const paginatedAppIds = useMemo(() => {
    const start = (currentPage - 1) * ITEMS_PER_PAGE
    return filteredAppIds.slice(start, start + ITEMS_PER_PAGE)
  }, [filteredAppIds, currentPage])

  const gridRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setCurrentPage(1) // Reset về trang 1 khi search thay đổi
  }, [search])

  useEffect(() => {
    // Scroll instant lên top để animation bay lên không bị conflict
    // Try both container and grid
    if (gridRef.current) {
      gridRef.current.scrollTop = 0
    }
    if (containerRef.current) {
      containerRef.current.scrollTop = 0
    }

    // Backup with scrollTo in case scrollTop doesn't work
    requestAnimationFrame(() => {
      if (gridRef.current) {
        gridRef.current.scrollTo({ top: 0, behavior: 'instant' as ScrollBehavior })
      }
      if (containerRef.current) {
        containerRef.current.scrollTo({ top: 0, behavior: 'instant' as ScrollBehavior })
      }
    })
  }, [currentPage])

  const showToast = (title: string, msg: string, severity: 'success' | 'error' | 'info' = 'info') => {
    window.dispatchEvent(new CustomEvent('0xo-toast', {
      detail: {
        category: 'launcher',
        severity,
        title,
        message: msg,
        dedupeKey: `lua-shop:${title}`,
      }
    }))
  }

  const performRestart = async () => {
    try {
      const args = autoInstall ? { postRestartAction: 'steam://open/games' } : {}
      await invoke('force_restart_steam', args)
      showToast(t.library.restartSteamPrompt, t.settings.restartSteam + '...', 'info')
    } catch (e) {
      console.error('Restart Steam error:', e)
      showToast('Error', String(e), 'error')
    }
  }

  const handleAddToSteam = async (appid: string) => {
    const numAppid = parseInt(appid)
    const gameName = gameInfoCache.get(appid)?.name || `AppID ${appid}`

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
      const checkResult = await invoke('check_steam_update', { appid: numAppid }) as { needs_update: boolean, reason: string, is_missing: boolean }

      let forceUpdate = false
      if (checkResult.needs_update) {
        if (checkResult.is_missing) {
          showToast(t.library.addToSteam, `Creating config for ${gameName} (30-60s)...`, 'info')
          forceUpdate = true
        } else {
          const { ask } = await import('@tauri-apps/plugin-dialog')
          const shouldUpdate = await ask(`Update available.\nReason: ${checkResult.reason}\n\nFetch latest version?`, {
            title: 'Data Update',
            kind: 'info',
          })

          if (shouldUpdate) {
            showToast(t.library.addToSteam, `Downloading update for ${gameName} (30-60s)...`, 'info')
            forceUpdate = true
          }
        }
      }

      await invoke('add_to_steam', { appid: numAppid, forceUpdate })
      setInstalledLuas(prev => new Set([...prev, appid]))
      showToast(t.library.addToSteam, t.library.addToSteamSuccess, 'success')

      if (skipConfirm) {
        performRestart()
      } else {
        setShowRestartConfirm(true)
      }
    } catch (err) {
      showToast(t.library.addToSteam, t.library.addToSteamError + ': ' + String(err), 'error')
    }
  }

  const handleRemove = async (appid: string) => {
    setRemovingAppId(appid)
    setShowRemoveConfirm(true)
  }

  const confirmRemove = async () => {
    setShowRemoveConfirm(false)
    if (!removingAppId) return

    try {
      await invoke('remove_from_steam', { appid: parseInt(removingAppId) })
      setInstalledLuas(prev => {
        const next = new Set(prev)
        next.delete(removingAppId)
        return next
      })
      showToast(t.library.removeFromSteam, t.library.removeFromSteamSuccess, 'success')

      if (skipConfirm) {
        performRestart()
      } else {
        setShowRestartConfirm(true)
      }
    } catch (err) {
      showToast(t.library.removeFromSteam, t.library.removeFromSteamError + ': ' + String(err), 'error')
    } finally {
      setRemovingAppId(null)
    }
  }

  return (
    <div className="lua-shop-container" ref={containerRef}>
      <header className="lua-shop-header">
        <div>
          <h1>{t.luaShop.title}</h1>
          <p>{t.luaShop.description}</p>
        </div>
        <div className="lua-shop-stats">
          {!isLoading && (
            <>
              <span>{t.luaShop.available}: <strong>{allAppIds.length}</strong></span>
              <span>{t.luaShop.installed}: <strong>{installedLuas.size}</strong></span>
            </>
          )}
        </div>
      </header>

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
      </div>

      {isLoading ? (
        <div className="lua-shop-loading">
          <div className="spinner-large" />
          <p>{t.luaShop.loading || 'Đang tải dữ liệu...'}</p>
        </div>
      ) : filteredAppIds.length === 0 ? (
        <div className="lua-shop-loading">
          <p style={{ fontSize: '16px', color: 'rgba(255,255,255,0.6)' }}>
            {t.luaShop.noResults || 'Không tìm thấy kết quả nào'}
          </p>
        </div>
      ) : (
        <>
          <div className="lua-shop-grid" ref={gridRef}>
            {paginatedAppIds.map((appid, index) => (
              <LuaShopGameCard
                key={`${appid}-${currentPage}`}
                appid={appid}
                index={index}
                isInstalled={installedLuas.has(appid)}
                onAdd={handleAddToSteam}
                onRemove={handleRemove}
              />
            ))}
          </div>

          {totalPages > 1 && (
            <div className="lua-shop-pagination">
              <button
                disabled={currentPage === 1}
                onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
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
                onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                className="pagination-btn"
              >
                {t.luaShop.next || 'Next'}
                <ChevronRight size={18} />
              </button>
            </div>
          )}
        </>
      )}

      {showRemoveConfirm && removingAppId && (
        <ConfirmDialog
          title={t.library.confirmRemoveTitle}
          message={`${t.library.confirmRemoveMessage}\n\n${gameInfoCache.get(removingAppId)?.name || `AppID ${removingAppId}`}`}
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
          <div style={{
            marginTop: '20px',
            padding: '12px 16px',
            background: 'rgba(0,0,0,0.2)',
            borderRadius: '8px',
            border: '1px solid rgba(255,255,255,0.05)',
            display: 'flex',
            flexDirection: 'column',
            gap: '12px'
          }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer', margin: 0, paddingBottom: '12px', borderBottom: '1px solid rgba(255,255,255,0.05)' }}>
              <button
                type="button"
                className={autoInstall ? 'settings-toggle is-on' : 'settings-toggle'}
                role="switch"
                aria-checked={autoInstall}
                onClick={(e) => {
                  e.preventDefault();
                  const next = !autoInstall;
                  setAutoInstall(next);
                  localStorage.setItem('steamAutoInstall', String(next));
                }}
              >
                <span />
              </button>
              <span style={{ flex: 1, fontSize: '14px', color: autoInstall ? '#fff' : 'rgba(255,255,255,0.6)' }}>
                {t.library.autoInstallAfterRestart}
              </span>
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer', margin: 0 }}>
              <button
                type="button"
                className={skipConfirm ? 'settings-toggle is-on' : 'settings-toggle'}
                role="switch"
                aria-checked={skipConfirm}
                onClick={(e) => {
                  e.preventDefault();
                  const next = !skipConfirm;
                  setSkipConfirm(next);
                  localStorage.setItem('steamSkipRestartConfirm', String(next));
                }}
              >
                <span />
              </button>
              <span style={{ flex: 1, fontSize: '13px', color: 'rgba(255,255,255,0.5)' }}>
                {t.library.rememberThisChoice}
              </span>
            </label>
          </div>
        </ConfirmDialog>
      )}
    </div>
  )
}