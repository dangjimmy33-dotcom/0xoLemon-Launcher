import { useEffect, useMemo, useRef, useState, cloneElement } from 'react'
import { doc, setDoc, increment } from 'firebase/firestore'
import { db } from '../firebase'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { BookOpen, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, PlusCircle, Download, FolderOpen, HardDrive, Image as ImageIcon, Library, Play, RefreshCcw, Search, ShieldCheck, ShoppingBag, SlidersHorizontal, ThumbsUp, Trophy, X, MessageSquare, Info, Sparkles } from 'lucide-react'
import { useGameStats } from '../hooks/useGameStats'
import { TutorialModal } from './TutorialModal'
import { useLocale } from '../context/LocaleContext'
import { useSteamAppIds } from '../hooks/useSteamAppIds'
import { useLuaUpdateCheck } from '../hooks/useLuaUpdateCheck'
import type { CloudSaveStatus, GameAchievement, GameCatalog, GameDetail, GameSummary, GameInstallState, GameVersionInfo, VerifyUiStatus } from '../types'
import { assetUrlForId, firstMediaUrl, isCarouselMedia, mediaPriority, processDescriptionHtml, isTauriRuntime } from '../lib/gameMeta'
import { formatBytes } from '../lib/format'
import { getGameTags, gameHasTag } from '../lib/gameTags'
import { GameDetailsPanel, InstallSummaryPanel, OSTPlayer } from './panels'
import { CloudSavePanel } from './CloudSavePanel'
import { GameChat } from './GameChat'
import { ConfirmDialog } from './ConfirmDialog'
import { useRealtimeConfig } from '../hooks/useRealtimeConfig'
import { useFirestoreDetail } from '../hooks/useFirestoreDetail'

function LazyGameCardImage({
  game,
  assetId,
  url,
  variant,
  onRequestAsset,
}: {
  game: GameSummary
  assetId: string | undefined
  url: string | undefined
  variant: 'compact' | 'browse'
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (url || !assetId) return
    const el = ref.current
    if (!el || typeof IntersectionObserver === 'undefined') {
      onRequestAsset(game, assetId)
      return
    }
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        onRequestAsset(game, assetId)
        observer.disconnect()
      }
    }, { rootMargin: '260px 0px 260px 0px', threshold: 0.01 })
    observer.observe(el)
    return () => observer.disconnect()
  }, [assetId, game, onRequestAsset, url])

  if (url) {
    return <img src={url} alt="" loading="lazy" decoding="async" />
  }

  return (
    <div className="asset-placeholder" ref={ref}>
      <ImageIcon size={variant === 'browse' ? 34 : 26} />
    </div>
  )
}

function CatalogLoadingView({ viewMode }: { viewMode: 'store' | 'library' }) {
  const { t } = useLocale()
  return (
    <section className="library-browse-view library-loading-view" aria-busy="true" aria-label={`Loading ${viewMode}`}>
      <header className="library-browse-toolbar">
        <div className="library-browse-heading">
          <strong>{viewMode === 'store' ? t.nav.store : t.nav.library}</strong>
        </div>
        <div className="library-search-skeleton" aria-hidden="true" />
      </header>
      <div className="library-browse-grid" aria-hidden="true">
        {Array.from({ length: 6 }, (_, index) => (
          <div className="library-card-skeleton" key={index}>
            <div />
            <span />
            <small />
          </div>
        ))}
      </div>
    </section>
  )
}

function CatalogUnavailableView({ viewMode, onRetry }: { viewMode: 'store' | 'library'; onRetry: () => void }) {
  const { t } = useLocale()
  return (
    <section className="library-browse-view">
      <header className="library-browse-toolbar">
        <div className="library-browse-heading">
          <strong>{viewMode === 'store' ? t.nav.store : t.nav.library}</strong>
        </div>
      </header>
      <div className="library-unavailable">
        <CircleAlert size={28} />
        <strong>{viewMode === 'store' ? 'Store unavailable' : 'Library unavailable'}</strong>
        <span>Please try again.</span>
        <button type="button" onClick={onRetry}>
          <RefreshCcw size={16} />
          Try again
        </button>
      </div>
    </section>
  )
}

function GameDetailLoadingView({
  game,
  assets,
  onBack,
}: {
  game: GameSummary
  assets: Record<string, string>
  onBack: () => void
}) {
  const hero = assetUrlForId(game.heroAssetId, assets)
  const icon = assetUrlForId(game.iconAssetId, assets) || assetUrlForId(game.gridAssetId, assets)

  return (
    <section className="game-detail-loading-view" aria-busy="true" aria-label={`Opening ${game.title}`}>
      <button className="back-to-library" type="button" onClick={onBack}>
        <ChevronLeft size={16} />
        Back
      </button>
      <div className="detail-loading-layout" aria-hidden="true">
        <div className="detail-loading-main">
          <div className={`detail-loading-hero${hero ? ' has-image' : ''}`}>
            {hero ? <img src={hero} alt="" /> : null}
            <div className="detail-loading-shade" />
            <div className="detail-loading-title">
              {icon ? <img src={icon} alt="" /> : <div className="detail-loading-icon" />}
              <div>
                <strong>{game.title}</strong>
                <span />
              </div>
            </div>
          </div>
          <div className="detail-loading-row">
            <span />
            <span />
          </div>
        </div>
        <aside className="detail-loading-side">
          <div />
          <div />
          <div />
        </aside>
      </div>
    </section>
  )
}

function HoverCardPopup({
  game,
  assets,
  pos,
  onRequestAsset,
}: {
  game: GameSummary
  assets: Record<string, string>
  pos: { top: number; left: number; right: number; alignRight: boolean }
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const detail = useFirestoreDetail(game.id)

  const videoMedia = detail?.media?.find(
    (m) => m.mimeType?.startsWith('video/') || m.role?.startsWith('video'),
  )
  const videoAssetId = videoMedia?.assetId

  const thumbMedia = detail?.media?.find(
    (m) => m.role === 'video-thumb' || m.role === 'video-thumbnail' || m.role === 'video-poster',
  )
  const thumbAssetId = thumbMedia?.assetId

  useEffect(() => {
    if (videoAssetId) {
      onRequestAsset(game, videoAssetId, true)
    }
    if (thumbAssetId) {
      onRequestAsset(game, thumbAssetId, true)
    }
    onRequestAsset(game, game.heroAssetId, true)
  }, [game, videoAssetId, thumbAssetId, onRequestAsset])

  const videoUrl = videoAssetId ? assetUrlForId(videoAssetId, assets) : null
  const thumbUrl = thumbAssetId ? assetUrlForId(thumbAssetId, assets) : null
  const hero = assetUrlForId(game.heroAssetId, assets)
  const posterUrl = thumbUrl || hero || undefined
  const tags = getGameTags(game)

  const style: React.CSSProperties = {
    position: 'fixed',
    top: pos.top,
    zIndex: 9999,
    transform: 'translateY(-50%)',
  }
  if (pos.alignRight) {
    style.right = pos.right
  } else {
    style.left = pos.left
  }

  const description = detail?.shortDescription || game.subtitle || ''

  return (
    <div className="hover-card-portal" style={style}>
      <div className="hover-card-media">
        {videoUrl ? (
          <video src={videoUrl} autoPlay loop muted playsInline poster={posterUrl} />
        ) : hero ? (
          <img src={hero} alt="" />
        ) : (
          <div className="hover-card-placeholder" />
        )}
      </div>
      <div className="hover-card-info">
        <div className="hover-card-header">
          <strong>{game.title}</strong>
        </div>
        <div className="hover-card-dev">{game.developer}</div>
        <div className="hover-card-tags">
          {tags.map((t) => (
            <i key={t.id} className={`tone-${t.tone}`}>
              {t.label}
            </i>
          ))}
        </div>
        {description ? <p className="hover-card-desc">{description}</p> : null}
      </div>
    </div>
  )
}

function GameHoverCard({
  game,
  assets,
  onRequestAsset,
  children,
}: {
  game: GameSummary
  assets: Record<string, string>
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  children: React.ReactElement
}) {
  const [hovered, setHovered] = useState(false)
  const [show, setShow] = useState(false)
  const [pos, setPos] = useState({ top: 0, left: 0, right: 0, alignRight: false })
  const anchorRef = useRef<HTMLElement>(null)

  useEffect(() => {
    if (!hovered) {
      setShow(false)
      return
    }
    const timer = setTimeout(() => {
      if (anchorRef.current) {
        const rect = anchorRef.current.getBoundingClientRect()
        const isListMode = rect.width > 300
        const spaceRight = window.innerWidth - rect.right
        const spaceLeft = rect.left
        const alignRight = !isListMode && spaceRight < 340 && spaceLeft > 340
        setPos({
          top: rect.top + rect.height / 2,
          left: isListMode ? rect.left + 220 : rect.right + 10,
          right: window.innerWidth - rect.left + 10,
          alignRight,
        })
      }
      setShow(true)
    }, 600)
    return () => clearTimeout(timer)
  }, [hovered])

  const clonedChild = cloneElement(children, {
    ref: anchorRef,
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
  } as any)

  return (
    <>
      {clonedChild}
      {show && createPortal(<HoverCardPopup game={game} assets={assets} pos={pos} onRequestAsset={onRequestAsset} />, document.body)}
    </>
  )
}

import type { DiscordAuthUser } from '../types'

// StoreModeSwitch Component
function StoreModeSwitch({ value, onChange }: { value: 'local' | 'hybrid' | 'steam'; onChange: (mode: 'local' | 'hybrid' | 'steam') => void }) {
  const { t } = useLocale()

  return (
    <div className="store-mode-switch">
      <button
        type="button"
        className={`mode-option ${value === 'local' ? 'active' : ''}`}
        onClick={() => onChange('local')}
      >
        <HardDrive size={14} />
        {t.library?.storeModeLocal || 'Local'}
      </button>
      <button
        type="button"
        className={`mode-option ${value === 'hybrid' ? 'active' : ''}`}
        onClick={() => onChange('hybrid')}
      >
        <Sparkles size={14} />
        {t.library?.storeModeHybrid || 'Hybrid'}
      </button>
      <button
        type="button"
        className={`mode-option ${value === 'steam' ? 'active' : ''}`}
        onClick={() => onChange('steam')}
      >
        <Play size={14} />
        {t.library?.storeModeSteam || 'Steam'}
      </button>
    </div>
  )
}

// LibraryModeSwitch Component
function LibraryModeSwitch({ value, onChange }: { value: 'local' | 'steam'; onChange: (mode: 'local' | 'steam') => void }) {
  const { t } = useLocale()

  return (
    <div className="store-mode-switch two-options">
      <button
        type="button"
        className={`mode-option ${value === 'local' ? 'active' : ''}`}
        onClick={() => onChange('local')}
      >
        <HardDrive size={14} />
        {t.library?.storeModeLocal || 'Local'}
      </button>
      <button
        type="button"
        className={`mode-option ${value === 'steam' ? 'active' : ''}`}
        onClick={() => onChange('steam')}
      >
        <Play size={14} />
        {t.library?.storeModeSteam || 'Steam'}
      </button>
    </div>
  )
}

export function StoreLibraryView({
  viewMode,
  catalog,
  catalogLoadState,
  onRetryCatalog,
  selectedGame,
  selectedGameId,
  onSelectGame,
  onRequestAsset,
  detail,
  assets,
  selectedVersion,
  selectedCurrentVersion,
  selectedVersionInfo,
  selectedInstallState,
  verifyStatus,
  updateReady,
  showVersionAction,
  canUpdate,
  updateSize,
  installSize,
  temporarySpace,
  isJobRunning,
  isGameRunning,
  onPrimaryAction,
  onPlay,
  onStop,
  onVerify,
  onUninstall,
  onOpenInstallOptions,
  onOpenStore,
  cloudSaveStatus,
  cloudSaveBusy,
  cloudLaunchBlocked,
  onToggleCloudSave,
  onAddCloudSaveFolder,
  onSyncCloudSave,
  onResolveCloudConflict,
  onRestoreCloudSnapshot,
  onLaunchWithoutCloudSync,
  onConnectGoogleDrive,
  onDisconnectGoogleDrive,
  onBackupGoogleDrive,
  onRestoreMissingSaveFiles,
  discordUser,
  installStates,
  steamInstalledAppIds,
  steamBuildIds,
}: {
  viewMode: 'store' | 'library'
  catalog: GameCatalog
  catalogLoadState: 'loading' | 'ready' | 'error'
  onRetryCatalog: () => void
  selectedGame: GameSummary | null
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  detail: GameDetail | null
  assets: Record<string, string>
  selectedVersion: string
  selectedCurrentVersion: string
  selectedVersionInfo?: GameVersionInfo
  selectedInstallState?: GameInstallState
  verifyStatus: VerifyUiStatus | null
  updateReady: boolean
  showVersionAction: boolean
  canUpdate: boolean
  updateSize: number
  installSize: number
  temporarySpace: number
  isJobRunning: boolean
  isGameRunning: boolean
  onPrimaryAction: () => void
  onPlay: () => void
  onStop: () => void
  onVerify: () => void
  onUninstall: () => void
  onOpenInstallOptions: () => void
  onOpenStore: () => void
  cloudSaveStatus: CloudSaveStatus | null
  cloudSaveBusy: boolean
  cloudLaunchBlocked: boolean
  onToggleCloudSave: (enabled: boolean) => void
  onAddCloudSaveFolder: () => void
  onSyncCloudSave: () => void
  onResolveCloudConflict: (conflictId: string, resolution: 'local' | 'cloud') => void
  onRestoreCloudSnapshot: (snapshotId: string) => void
  onLaunchWithoutCloudSync: () => void
  onConnectGoogleDrive: () => void
  onDisconnectGoogleDrive: () => void
  onBackupGoogleDrive: () => void
  onRestoreMissingSaveFiles: () => void
  discordUser?: DiscordAuthUser | null
  installStates?: Record<string, GameInstallState>
  steamInstalledAppIds?: number[]
  steamBuildIds?: Record<number, string>
}) {
  const { t } = useLocale()
  const [query, setQuery] = useState('')
  const [tutorialVisible, setTutorialVisible] = useState(false)
  const [storeMode, setStoreMode] = useState<'local' | 'hybrid' | 'steam'>(() =>
    (localStorage.getItem('libraryStoreMode') as 'local' | 'hybrid' | 'steam') || 'hybrid'
  )
  const [libraryMode, setLibraryMode] = useState<'local' | 'steam'>(() =>
    (localStorage.getItem('libraryMode') as 'local' | 'steam') || 'local'
  )
  const [sortBy, setSortBy] = useState<'az' | 'za' | 'liked' | 'downloaded'>('az')
  const [viewLayout, setViewLayout] = useState<'grid' | 'list'>(() =>
    (localStorage.getItem('libraryViewLayout') as 'grid' | 'list') || 'grid'
  )
  const [currentPage, setCurrentPage] = useState(1)
  const [gridCols, setGridCols] = useState<number>(() => {
    const saved = localStorage.getItem('libraryGridCols')
    return saved ? parseInt(saved, 10) : 8
  })
  const [wishlist, setWishlist] = useState<Set<string>>(() => {
    try { return new Set(JSON.parse(localStorage.getItem('libraryWishlist') || '[]')) }
    catch { return new Set() }
  })
  const [likedGames, setLikedGames] = useState<Set<string>>(() => {
    try { return new Set(JSON.parse(localStorage.getItem('libraryLikedGames') || '[]')) }
    catch { return new Set() }
  })
  const [sortOpen, setSortOpen] = useState(false)
  const realtimeConfig = useRealtimeConfig()
  const gameStats = useGameStats()

  const toggleWishlist = (gameId: string) => {
    setWishlist(prev => {
      const next = new Set(prev)
      if (next.has(gameId)) next.delete(gameId); else next.add(gameId)
      localStorage.setItem('libraryWishlist', JSON.stringify([...next]))
      return next
    })
  }

  const [optimisticLikes, setOptimisticLikes] = useState<Record<string, number>>({})

  const toggleLike = async (gameId: string) => {
    const isLiked = likedGames.has(gameId)
    const delta = isLiked ? -1 : 1
    setLikedGames(prev => {
      const next = new Set(prev)
      if (isLiked) next.delete(gameId); else next.add(gameId)
      localStorage.setItem('libraryLikedGames', JSON.stringify([...next]))
      return next
    })
    // Optimistic update so counter changes immediately
    setOptimisticLikes(prev => ({
      ...prev,
      [gameId]: (prev[gameId] ?? gameStats.likes[gameId] ?? 0) + delta,
    }))
    try {
      await setDoc(doc(db, 'config', 'gameStats'), {
        likes: { [gameId]: increment(delta) }
      }, { merge: true })
    } catch (e) {
      // Rollback optimistic update
      setOptimisticLikes(prev => ({
        ...prev,
        [gameId]: (prev[gameId] ?? gameStats.likes[gameId] ?? 0) - delta,
      }))
      console.warn('Failed to update likes', e)
    }
  }

  const toggleViewLayout = (layout: 'grid' | 'list') => {
    setViewLayout(layout)
    localStorage.setItem('libraryViewLayout', layout)
  }

  const handleSetGridCols = (cols: number) => {
    setGridCols(cols)
    localStorage.setItem('libraryGridCols', cols.toString())
  }

  // When Firestore confirms the like count, clear the optimistic override
  useEffect(() => {
    setOptimisticLikes(prev => {
      const next = { ...prev }
      let changed = false
      for (const gameId of Object.keys(next)) {
        if (gameStats.likes[gameId] !== undefined) {
          delete next[gameId]
          changed = true
        }
      }
      return changed ? next : prev
    })
  }, [gameStats.likes])

  useEffect(() => {
    if (selectedGame && selectedInstallState?.installed && selectedGame.id.includes('among')) {
      const shownKey = `tutorial_shown_${selectedGame.id}`
      if (localStorage.getItem(shownKey) !== 'true') {
        localStorage.setItem(shownKey, 'true')
        const timer = window.setTimeout(() => setTutorialVisible(true), 0)
        return () => window.clearTimeout(timer)
      }
    }
    return undefined
  }, [selectedGame, selectedInstallState?.installed])

  const { mapping } = useSteamAppIds()

  const visibleGames = useMemo(() => {
    let baseGames = catalog.games

    // In library view, filter based on libraryMode
    if (viewMode === 'library' && installStates && steamInstalledAppIds) {
      baseGames = baseGames.filter((game) => {
        if (libraryMode === 'local') {
          return installStates[game.id]?.installed
        } else if (libraryMode === 'steam') {
          const appid = mapping[game.id]
          return appid && steamInstalledAppIds.includes(appid)
        }
        return false
      })
    }

    const needle = query.trim().toLowerCase()
    if (needle) {
      baseGames = baseGames.filter((game) =>
        [game.title, game.subtitle, game.developer, game.publisher].some((value) => value.toLowerCase().includes(needle)),
      )
    }

    // Apply sort
    const sorted = [...baseGames]
    if (sortBy === 'az') sorted.sort((a, b) => a.title.localeCompare(b.title))
    else if (sortBy === 'za') sorted.sort((a, b) => b.title.localeCompare(a.title))
    else if (sortBy === 'liked') sorted.sort((a, b) => (gameStats.likes[b.id] || 0) - (gameStats.likes[a.id] || 0))
    else if (sortBy === 'downloaded') sorted.sort((a, b) => (gameStats.downloads[b.id] || 0) - (gameStats.downloads[a.id] || 0))

    // Always push the newest game to the front
    const newestGameId = catalog.games.length > 0 ? catalog.games[catalog.games.length - 1].id : null
    if (newestGameId) {
      const newestIndex = sorted.findIndex(g => g.id === newestGameId)
      if (newestIndex > 0) {
        const [newest] = sorted.splice(newestIndex, 1)
        sorted.unshift(newest)
      }
    }

    return sorted
  }, [catalog.games, query, viewMode, libraryMode, installStates, steamInstalledAppIds, mapping, sortBy, gameStats])

  useEffect(() => {
    setCurrentPage(1)
  }, [query, sortBy, viewLayout, viewMode, libraryMode, gridCols])

  const itemsPerPage = viewLayout === 'list' ? 70 : 50
  const paginatedGames = visibleGames.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage)
  const totalPages = Math.ceil(visibleGames.length / itemsPerPage)
  const newestGameIdForSash = catalog.games.length > 0 ? catalog.games[catalog.games.length - 1].id : null

  const actionDockRef = useRef<HTMLDivElement>(null)
  const [stickyVisible, setStickyVisible] = useState(false)
  const [activeDetailTab, setActiveDetailTab] = useState<'overview' | 'chat' | 'lua-game'>('overview')
  const [showLuaGameTab, setShowLuaGameTab] = useState(false)
  const [steamGameRunning, setSteamGameRunning] = useState(false)

  // Get current game's Steam App ID
  const currentSteamAppId = selectedGame ? mapping[selectedGame.id] : undefined

  // Check for Lua manifest updates
  const { updateInfo } = useLuaUpdateCheck(currentSteamAppId, showLuaGameTab)

  // Listen for lua-game-mode changes
  useEffect(() => {
    if (!selectedGame) return

    const handleLuaGameModeChange = (e: CustomEvent) => {
      const { gameId: eventGameId, added } = e.detail
      if (eventGameId === selectedGame.id) {
        setShowLuaGameTab(added)
        if (added) {
          // Auto-navigate to lua-game tab when game is added
          setActiveDetailTab('lua-game')
        } else if (activeDetailTab === 'lua-game') {
          // Navigate back to overview when tab is removed
          setActiveDetailTab('overview')
        }
      }
    }

    window.addEventListener('lua-game-mode-changed' as any, handleLuaGameModeChange)

    // Check initial status
    const checkLuaGameStatus = async () => {
      try {
        const appid = mapping[selectedGame.id]
        if (appid) {
          const isAdded = await invoke<boolean>('check_steam_status', { appid })
          setShowLuaGameTab(isAdded)
        }
      } catch (e) {
        console.error('Failed to check lua-game status', e)
      }
    }
    checkLuaGameStatus()

    return () => {
      window.removeEventListener('lua-game-mode-changed' as any, handleLuaGameModeChange)
    }
  }, [selectedGame, activeDetailTab, mapping])

  useEffect(() => {
    // Poll to check if the Steam game process is running
    const executable = detail?.install?.launchExecutable
    const appid = mapping[selectedGame?.id || '']
    const isInstalledOnSteam = Boolean(appid && steamInstalledAppIds?.includes(appid))
    const effectiveMode = viewMode === 'library' ? libraryMode : storeMode

    if (effectiveMode !== 'steam' || !isInstalledOnSteam || !executable) {
      if (steamGameRunning) setSteamGameRunning(false)
      return
    }

    const interval = setInterval(async () => {
      try {
        const running = await invoke<boolean>('is_process_running', { executable })
        setSteamGameRunning(running)
      } catch (e) {
        // ignore
      }
    }, 3000)

    return () => clearInterval(interval)
  }, [detail, mapping, selectedGame, steamInstalledAppIds, libraryMode, storeMode, viewMode, steamGameRunning])

  const [steamlessStatus, setSteamlessStatus] = useState<boolean>(false)
  const [steamlessLoading, setSteamlessLoading] = useState<boolean>(false)
  const [steamlessMessage, setSteamlessMessage] = useState<{ text: string; isError: boolean } | null>(null)

  // ── Depot Version Switcher ──
  interface DepotManifestEntry { depotId: string; manifestId: string; manifestFile: string }
  interface DepotVersionEntry { buildId: string; depots: DepotManifestEntry[] }
  interface DepotPatchEvent {
    eventType: string; buildId: string; depotId?: string
    message?: string; index?: number; total?: number; success?: boolean
  }

  const [depotVersions, setDepotVersions] = useState<DepotVersionEntry[]>([])
  const [depotHfRepoId, setDepotHfRepoId] = useState<string>('')
  const [depotSelectedBuildId, setDepotSelectedBuildId] = useState<string>('')
  const [depotPatching, setDepotPatching] = useState<boolean>(false)
  const [depotVersionsError, setDepotVersionsError] = useState<string | null>(null)
  const [depotPatchBuildId, setDepotPatchBuildId] = useState<string | null>(null)
  const [depotPatchLog, setDepotPatchLog] = useState<string[]>([])
  const [depotPatchResult, setDepotPatchResult] = useState<{ ok: boolean; msg: string } | null>(null)
  const [depotVersionsLoading, setDepotVersionsLoading] = useState<boolean>(false)
  const [depotCurrentBuildId, setDepotCurrentBuildId] = useState<string | null>(null)
  const [depotDropdownOpen, setDepotDropdownOpen] = useState<boolean>(false)

  // Load depot HF repo ID from launcher settings (fallback to default if not set)
  useEffect(() => {
    invoke<any>('get_launcher_settings').then((s: any) => {
      // Use configured value, or fall back to default repo
      setDepotHfRepoId(s?.depotHfRepoId || 'Immaking/Luas')
    }).catch(() => setDepotHfRepoId('Immaking/Luas'))
  }, [])

  // When Lua Mode tab is active + game has Steam appid, fetch depot versions from HF
  useEffect(() => {
    if (activeDetailTab !== 'lua-game' || !currentSteamAppId || !depotHfRepoId) {
      setDepotVersions([])
      setDepotSelectedBuildId('')
      setDepotVersionsLoading(false)
      setDepotCurrentBuildId(null)
      return
    }

    // Fetch current buildID from Steam appmanifest
    if (currentSteamAppId) {
      const appidNum = Number(currentSteamAppId)
      console.log('[Depot] Fetching buildID for appid:', appidNum)
      invoke<string | null>('get_steam_game_buildid', { appid: appidNum })
        .then(buildId => {
          console.log('[Depot] Current buildID:', buildId)
          setDepotCurrentBuildId(buildId)
        })
        .catch(err => {
          console.error('[Depot] Failed to get buildID:', err)
          setDepotCurrentBuildId(null)
        })
    }

    setDepotVersionsLoading(true)
    setDepotVersionsError(null)
    invoke<DepotVersionEntry[]>('list_depot_versions', {
      appid: Number(currentSteamAppId),
      hfRepoId: depotHfRepoId,
    }).then(versions => {
      setDepotVersions(versions)
      // Auto-select newest build
      if (versions.length > 0 && !depotSelectedBuildId) {
        setDepotSelectedBuildId(versions[0].buildId)
      }
      setDepotVersionsError(null)
      setDepotVersionsLoading(false)
    }).catch(e => {
      setDepotVersions([])
      setDepotVersionsError(String(e))
      setDepotVersionsLoading(false)
    })
  }, [activeDetailTab, currentSteamAppId, depotHfRepoId])

  // Listen to real-time depot patch events from backend
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<DepotPatchEvent>('depot-patch-progress', (event) => {
      const ev = event.payload
      if (ev.message) setDepotPatchLog(prev => [...prev.slice(-199), ev.message!])
      if (ev.eventType === 'complete' || ev.eventType === 'error') {
        setDepotPatching(false)
        setDepotPatchResult({ ok: ev.success === true, msg: ev.message ?? '' })
      }
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [])

  const handleDepotPatch = async (buildId: string) => {
    if (!currentSteamAppId || depotPatching) return
    const installDir = await invoke<string | null>('get_steam_game_install_dir', { appid: currentSteamAppId }).catch(() => null)
    if (!installDir) {
      setDepotPatchResult({ ok: false, msg: 'Steam install directory not found. Please install the game via Steam first.' })
      return
    }
    setDepotPatching(true)
    setDepotPatchBuildId(buildId)
    setDepotPatchLog([])
    setDepotPatchResult(null)
    invoke<string>('run_depot_patch', {
      appid: currentSteamAppId,
      buildId,
      hfRepoId: depotHfRepoId,
      installDir,
    }).catch((e: any) => {
      setDepotPatching(false)
      setDepotPatchResult({ ok: false, msg: String(e) })
    })
  }

  /** Resolve the exe path: prefer Steam's own install dir over launcher's installPath */
  const resolveSteamlessExePath = async (): Promise<string | null> => {
    const launchExe = selectedInstallState?.launchExecutable
    if (!launchExe) return null
    // Only the filename part — strip any subdirectory that might be in launchExecutable
    const exeFilename = launchExe.split('\\').pop() ?? launchExe
    const appid = selectedGame ? mapping[selectedGame.id] : undefined
    if (appid) {
      try {
        const steamDir = await invoke<string | null>('get_steam_game_install_dir', { appid })
        if (steamDir) {
          return `${steamDir}\\${exeFilename}`
        }
      } catch {
        // fallthrough to installPath
      }
    }
    // Fallback: use launcher's tracked installPath
    const installPath = selectedInstallState?.installPath
    if (!installPath) return null
    return `${installPath}\\${launchExe}`
  }

  useEffect(() => {
    if (!selectedGame || !selectedInstallState?.launchExecutable || activeDetailTab !== 'lua-game') {
      return
    }
    let cancelled = false
    resolveSteamlessExePath().then(exePath => {
      if (!cancelled && exePath) {
        invoke<boolean>('steamless_status', { exePath })
          .then(setSteamlessStatus)
          .catch(console.error)
      }
    })
    return () => { cancelled = true }
  }, [selectedGame, selectedInstallState, activeDetailTab])

  const handleToggleSteamless = async () => {
    const exePath = await resolveSteamlessExePath()
    if (!exePath) return
    setSteamlessLoading(true)
    setSteamlessMessage(null)

    try {
      if (steamlessStatus) {
        const msg = await invoke<string>('steamless_restore', { exePath })
        setSteamlessStatus(false)
        setSteamlessMessage({ text: msg, isError: false })
      } else {
        const res = await invoke<any>('steamless_apply', { exePath })
        if (res.success) {
          setSteamlessStatus(true)
          setSteamlessMessage({ text: res.message, isError: false })
        } else {
          setSteamlessMessage({ text: res.message, isError: true })
        }
      }
    } catch (e) {
      setSteamlessMessage({ text: String(e), isError: true })
    } finally {
      setSteamlessLoading(false)
    }
  }

  useEffect(() => {
    if (!selectedGameId || !detail?.gameId || typeof IntersectionObserver === 'undefined') {
      return
    }

    const el = actionDockRef.current
    if (!el) {
      return
    }

    const observer = new IntersectionObserver(([entry]) => setStickyVisible(!entry.isIntersecting), {
      threshold: 0,
      rootMargin: '-64px 0px 0px 0px',
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [selectedGameId, detail?.gameId])

  const renderGameCard = (game: GameSummary, variant: 'compact' | 'browse') => {
    const tags = getGameTags(game)
    const isComingSoon = gameHasTag(game, 'coming soon')
    const inWishlist = wishlist.has(game.id)
    const downloads = gameStats.downloads[game.id] || 0
    const likes = gameStats.likes[game.id] || 0

    return (
      <button
        className={[
          'store-game-card',
          variant === 'browse' ? 'browse-game-card' : '',
          game.id === selectedGameId ? 'active' : '',
          isComingSoon ? 'coming-soon' : '',
        ]
          .filter(Boolean)
          .join(' ')}
        key={game.id}
        type="button"
        disabled={isComingSoon}
        onClick={() => !isComingSoon && onSelectGame(game.id)}
      >
        <div className="store-game-card-media">
          <LazyGameCardImage
            game={game}
            assetId={variant === 'browse' && viewLayout === 'list' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId}
            url={assetUrlForId(variant === 'browse' && viewLayout === 'list' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId, assets)}
            variant={variant}
            onRequestAsset={onRequestAsset}
          />
          {game.id === newestGameIdForSash ? (
            <div className="game-card-new-sash">NEW!</div>
          ) : tags.some(t => t.id === 'demo bypass' || t.tone === 'demo') ? (
            <div className="game-card-demo-sash">DEMO</div>
          ) : null}
          {/* Wishlist btn inside media (grid mode) */}
          {viewLayout !== 'list' && (
            <button
              type="button"
              className={`game-card-wishlist-btn ${inWishlist ? 'active' : ''}`}
              onClick={(e) => {
                e.stopPropagation()
                toggleWishlist(game.id)
              }}
              title={inWishlist ? t.library.removeFromWishlist : t.library.addToWishlist}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill={inWishlist ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
              </svg>
            </button>
          )}
        </div>
        <div className="store-game-card-info">
          <div className="store-game-card-title">
            <strong>{game.title}</strong>
            <small>{game.developer}</small>
          </div>
          <div className="game-card-stats">
            {downloads > 0 && (
              <span className="stat-pill" title={`${downloads.toLocaleString()} ${t.library.totalDownloads}`}>
                <Download size={12} /> {downloads > 1000 ? `${(downloads / 1000).toFixed(1)}k` : downloads}
              </span>
            )}
            {likes > 0 && (
              <span className="stat-pill" title={`${likes.toLocaleString()} ${t.library.totalLikes}`}>
                <ThumbsUp size={12} /> {likes > 1000 ? `${(likes / 1000).toFixed(1)}k` : likes}
              </span>
            )}
          </div>
          {/* Wishlist btn at end of info row (list mode) */}
          {viewLayout === 'list' && (
            <button
              type="button"
              className={`game-card-wishlist-btn ${inWishlist ? 'active' : ''}`}
              onClick={(e) => {
                e.stopPropagation()
                toggleWishlist(game.id)
              }}
              title={inWishlist ? t.library.removeFromWishlist : t.library.addToWishlist}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill={inWishlist ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
              </svg>
            </button>
          )}
        </div>
      </button>

    )
  }

  if (!selectedGame) {
    if (catalogLoadState === 'loading' && catalog.games.length === 0) {
      return <CatalogLoadingView viewMode={viewMode} />
    }

    if (catalogLoadState === 'error' && catalog.games.length === 0) {
      return <CatalogUnavailableView viewMode={viewMode} onRetry={onRetryCatalog} />
    }

    return (
      <section className="library-browse-view">
        <header className="library-browse-toolbar">
          <div className="library-browse-heading">
            <strong>{viewMode === 'store' ? 'Store' : 'Installed games'}</strong>
            <span>
              {visibleGames.length} game{visibleGames.length === 1 ? '' : 's'}
            </span>
          </div>
          {viewMode === 'store' && <StoreModeSwitch value={storeMode} onChange={(v) => { setStoreMode(v); localStorage.setItem('libraryStoreMode', v) }} />}
          {viewMode === 'library' && <LibraryModeSwitch value={libraryMode} onChange={(v) => { setLibraryMode(v); localStorage.setItem('libraryMode', v) }} />}

          <div className="library-toolbar-actions">
            <div className="store-sort-dropdown">
              <button
                type="button"
                className="sort-toggle-btn"
                onClick={() => setSortOpen(!sortOpen)}
                onBlur={() => setTimeout(() => setSortOpen(false), 200)}
              >
                <SlidersHorizontal size={14} />
                <span>{
                  sortBy === 'az' ? t.library.sortAZ :
                    sortBy === 'za' ? t.library.sortZA :
                      sortBy === 'liked' ? t.library.sortMostLiked :
                        t.library.sortMostDownloaded
                }</span>
              </button>
              {sortOpen && (
                <div className="sort-dropdown-menu">
                  <button type="button" onClick={() => setSortBy('az')} className={sortBy === 'az' ? 'active' : ''}>{t.library.sortAZ}</button>
                  <button type="button" onClick={() => setSortBy('za')} className={sortBy === 'za' ? 'active' : ''}>{t.library.sortZA}</button>
                  <button type="button" onClick={() => setSortBy('liked')} className={sortBy === 'liked' ? 'active' : ''}>{t.library.sortMostLiked}</button>
                  <button type="button" onClick={() => setSortBy('downloaded')} className={sortBy === 'downloaded' ? 'active' : ''}>{t.library.sortMostDownloaded}</button>
                </div>
              )}
            </div>

            <div className="view-layout-toggle" style={{ gap: '4px' }}>
              {viewLayout === 'grid' && (
                <div style={{ display: 'flex', gap: '2px', marginRight: '8px', opacity: 0.8 }}>
                  <button type="button" className={gridCols === 4 ? 'active' : ''} onClick={() => handleSetGridCols(4)} title="4 Columns" style={{ fontSize: '11px', padding: '0 4px' }}>4x</button>
                  <button type="button" className={gridCols === 6 ? 'active' : ''} onClick={() => handleSetGridCols(6)} title="6 Columns" style={{ fontSize: '11px', padding: '0 4px' }}>6x</button>
                  <button type="button" className={gridCols === 8 ? 'active' : ''} onClick={() => handleSetGridCols(8)} title="8 Columns" style={{ fontSize: '11px', padding: '0 4px' }}>8x</button>
                </div>
              )}
              <button type="button" className={viewLayout === 'grid' ? 'active' : ''} onClick={() => toggleViewLayout('grid')} title={t.library.viewGrid}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="7" height="7" x="3" y="3" rx="1" /><rect width="7" height="7" x="14" y="3" rx="1" /><rect width="7" height="7" x="14" y="14" rx="1" /><rect width="7" height="7" x="3" y="14" rx="1" /></svg>
              </button>
              <button type="button" className={viewLayout === 'list' ? 'active' : ''} onClick={() => toggleViewLayout('list')} title={t.library.viewList}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="7" height="7" x="3" y="3" rx="1" /><rect width="7" height="7" x="3" y="14" rx="1" /><path d="M14 6h7" /><path d="M14 10h7" /><path d="M14 14h7" /><path d="M14 18h7" /></svg>
              </button>
            </div>

            <label className="store-search">
              <Search size={16} />
              <input aria-label="Search games" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search..." />
            </label>
          </div>
        </header>
        {visibleGames.length > 0 ? (
          <>
            <div className={`library-browse-grid layout-${viewLayout} ${viewLayout === 'grid' ? `grid-cols-${gridCols}` : ''}`}>
              {paginatedGames.map((game) => (
                <GameHoverCard key={game.id} game={game} assets={assets} onRequestAsset={onRequestAsset}>
                  {renderGameCard(game, 'browse') as React.ReactElement}
                </GameHoverCard>
              ))}
            </div>
            
            {totalPages > 1 && (
              <div className="library-pagination">
                <button type="button" disabled={currentPage <= 1} onClick={() => {
                  setCurrentPage(p => p - 1)
                  window.scrollTo({ top: 0, behavior: 'smooth' })
                }}>Prev</button>
                <span>Page {currentPage} of {totalPages}</span>
                <button type="button" disabled={currentPage >= totalPages} onClick={() => {
                  setCurrentPage(p => p + 1)
                  window.scrollTo({ top: 0, behavior: 'smooth' })
                }}>Next</button>
              </div>
            )}
          </>
        ) : null}
          {visibleGames.length > 0 && viewMode === 'store' ? (
            <div className="store-more-coming-banner">
              <span className="store-more-coming-title">{t.library.storeMoreComingTitle}</span>
              <span className="store-more-coming-body">{t.library.storeMoreComingBody}</span>
            </div>
          ) : null}
          {visibleGames.length === 0 && viewMode === 'library' ? (
            <div className="library-empty-inline library-empty-installed">
              {libraryMode === 'steam' ? (
                <>
                  <Library size={28} />
                  <strong>{t.library.noSteamGames}</strong>
                  <span>{t.library.noSteamGamesDesc}</span>
                  <button type="button" onClick={onOpenStore}>
                    <ShoppingBag size={15} />
                    Open Store
                  </button>
                </>
              ) : (
                <>
                  <Library size={28} />
                  <strong>No installed games</strong>
                  <span>Games installed from Store will appear here.</span>
                  <button type="button" onClick={onOpenStore}>
                    <ShoppingBag size={15} />
                    Open Store
                  </button>
                </>
              )}
            </div>
          ) : visibleGames.length === 0 ? (
            <div className="library-empty-inline">
              <Search size={24} />
              <strong>No matching games</strong>
            </div>
          ) : null}
      </section>
    )
  }

  if (!detail) {
    return <GameDetailLoadingView game={selectedGame} assets={assets} onBack={() => onSelectGame(null)} />
  }

  const hero = assetUrlForId(selectedGame.heroAssetId, assets) || firstMediaUrl(detail, assets)
  const logo = assetUrlForId(selectedGame.logoAssetId, assets)
  const installed = Boolean(selectedInstallState?.installed)
  const isVerifying = verifyStatus?.state === 'running'

  // Determine effective mode to control button visibility
  const effectiveMode = viewMode === 'library' ? libraryMode : storeMode

  const steamAppId = mapping[selectedGame.id]
  const isInstalledOnSteam = Boolean(steamAppId && steamInstalledAppIds?.includes(steamAppId))
  const steamBuildId = steamAppId ? steamBuildIds?.[steamAppId] : undefined

  const isDownloading = isJobRunning
  const isPlaying = isGameRunning || steamGameRunning

  let actionLabel: string = installed ? t.library.play : (!isTauriRuntime() ? 'Remote Install' : t.library.chooseInstall)
  let actionClass = 'primary-control'
  let primaryDisabled = false
  const stateLabel = !installed ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay

  if (isPlaying) {
    actionLabel = 'Running'
    actionClass = 'primary-control running-btn can-stop'
    primaryDisabled = false
  } else if (isDownloading) {
    actionLabel = 'Downloading'
    actionClass = 'primary-control downloading-btn'
    primaryDisabled = true
  } else if (!installed && effectiveMode !== 'steam') {
    actionLabel = !isTauriRuntime() ? 'Remote Install' : t.library.chooseInstall
    primaryDisabled = !canUpdate
  }

  const handleSteamStop = async () => {
    if (!detail?.install.launchExecutable) return
    try {
      await invoke('kill_process_by_name', { executable: detail.install.launchExecutable })
      setSteamGameRunning(false)
    } catch (e) {
      console.error('Failed to kill steam process', e)
    }
  }

  const handleSteamPlay = async () => {
    if (!steamAppId) return
    await invoke('open_url', { url: `steam://run/${steamAppId}` })
    setSteamGameRunning(true) // Optimistically set to true, will be verified by polling
  }

  const primaryActionBtn = isPlaying
    ? (effectiveMode === 'steam' ? handleSteamStop : onStop)
    : (effectiveMode === 'steam' ? handleSteamPlay : (!installed ? onOpenInstallOptions : onPlay))

  const primaryIcon = isPlaying
    ? <Play size={17} />
    : isDownloading
      ? <Download size={17} />
      : installed
        ? <Play size={17} />
        : <Download size={17} />
  const updateDisabled = !canUpdate || isDownloading || isPlaying
  const displayedVersion = installed ? selectedCurrentVersion : selectedVersion
  const downloadSize = updateSize || selectedVersionInfo?.sizeBytes || 0
  const verifyLabel = isVerifying ? 'Verifying...' : t.library.verifyIntegrity
  const VerifyIcon = verifyStatus?.state === 'failed' ? CircleAlert : ShieldCheck
  const missingCount = verifyStatus?.missingFiles?.length ?? 0
  const changedCount = verifyStatus?.mismatchedFiles?.length ?? 0

  const gridAsset = assetUrlForId(selectedGame.gridAssetId, assets)
  const iconAsset = assetUrlForId(selectedGame.iconAssetId, assets)

  const livePlayers = realtimeConfig.livePlayerCount?.[selectedGame.id]

  // local: show all buttons (play/install), hide steam
  // hybrid (store only): show all buttons (play/install) + add to steam
  // steam: hide install/play buttons completely, show only "Add to Steam"
  const showInstallButton = effectiveMode === 'local' || effectiveMode === 'hybrid'
  const showSteamButton = effectiveMode === 'steam' || effectiveMode === 'hybrid'

  return (
    <section className="game-detail-view">
      {/* ── Sticky Floating Bar ── */}
      <div className={`sticky-action-bar${stickyVisible ? ' visible' : ''}`}>
        {(iconAsset || gridAsset) && (
          <img
            className="sticky-bar-icon"
            src={iconAsset || gridAsset}
            alt=""
          />
        )}
        <div className="sticky-bar-info">
          <strong>{detail.title}</strong>
          <span>{effectiveMode === 'steam' ? (isInstalledOnSteam ? `Steam Build ${steamBuildId || 'Unknown'}` : 'Not Installed on Steam') : `${displayedVersion}`} {livePlayers !== undefined ? `• ${livePlayers.toLocaleString()} Playing` : ''}</span>
        </div>
        <div className="sticky-bar-actions">
          {(installed && effectiveMode !== 'steam' && selectedGame?.id.includes('among')) && (
            <button type="button" onClick={() => setTutorialVisible(true)}>
              <BookOpen size={15} />
              Tutorial
            </button>
          )}
          {(installed && effectiveMode !== 'steam') && (
            <button type="button" onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
              <FolderOpen size={15} />
              Browse
            </button>
          )}
          {effectiveMode !== 'steam' && (
            <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
              <VerifyIcon size={15} />
              {verifyLabel}
            </button>
          )}
          {showInstallButton && (
            <button
              className={actionClass}
              type="button"
              onClick={primaryActionBtn}
              disabled={primaryDisabled}
              data-stop-label={isPlaying ? 'STOP' : undefined}
            >
              {primaryIcon}
              <span>{actionLabel}</span>
            </button>
          )}
          {effectiveMode === 'steam' && isInstalledOnSteam && (
            <button
              className="primary-control"
              type="button"
              onClick={() => steamAppId && invoke('open_url', { url: `steam://run/${steamAppId}` })}
            >
              <Play size={15} />
              <span>{t.library.play}</span>
            </button>
          )}
          {/* Steam Versions button — shown in steam OR hybrid mode */}
          {(effectiveMode === 'steam' || effectiveMode === 'hybrid') && isInstalledOnSteam && showLuaGameTab && (
            <button className="update-control" type="button" onClick={() => setActiveDetailTab('lua-game')}>
              <Download size={15} />
              Steam Versions
            </button>
          )}
          {/* Local Versions button — shown in local OR hybrid mode */}
          {(installed && showVersionAction && effectiveMode !== 'steam') ? (
            <button className="update-control" type="button" onClick={onPrimaryAction} disabled={updateDisabled}>
              <Download size={15} />
              {updateReady ? t.library.update : 'Versions'}
            </button>
          ) : null}
          {(installed && effectiveMode !== 'steam') ? (
            <button className="danger-control" type="button" onClick={onUninstall}>
              <X size={15} />
              {t.library.uninstall}
            </button>
          ) : null}
        </div>
      </div>

      <section className="game-detail-main">
        <button className="back-to-library" type="button" onClick={() => onSelectGame(null)}>
          <ChevronLeft size={16} />
          Back
        </button>
        <div className="detail-hero">
          {hero ? <img src={hero} alt="" loading="eager" /> : <div className="detail-placeholder"><ImageIcon size={40} /></div>}
          <div className="detail-hero-shade" />
          <div className="detail-copy">
            <span className="storage-pill">
              <HardDrive size={14} />
              {detail.install?.storageLabel || 'HDD'}
            </span>
            {logo ? <img className="detail-logo" src={logo} alt={detail.title} /> : <h1>{detail.title}</h1>}
            <p>{detail.shortDescription}</p>
            <div className="library-meta-row">
              <span>{effectiveMode === 'steam' ? (isInstalledOnSteam ? `Steam Build ${steamBuildId || 'Unknown'}` : 'Not Installed on Steam') : `Version ${displayedVersion}`}</span>
              {effectiveMode !== 'steam' && <span>{formatBytes(downloadSize)}</span>}
              {effectiveMode !== 'steam' && detail.install?.supportsResume ? <span>{t.library.resumeSupported}</span> : null}
              {livePlayers !== undefined ? <span className="live-players-badge"><span className="pulse-dot"></span>{livePlayers.toLocaleString()} Online</span> : null}
              <button
                type="button"
                className={`detail-action-icon-btn${wishlist.has(selectedGame.id) ? ' active-wishlist' : ''}`}
                onClick={() => toggleWishlist(selectedGame.id)}
                title={wishlist.has(selectedGame.id) ? t.library.removeFromWishlist : t.library.addToWishlist}
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill={wishlist.has(selectedGame.id) ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
                </svg>
                <span>{wishlist.has(selectedGame.id) ? t.library.removeFromWishlist : t.library.addToWishlist}</span>
              </button>
              
              <div className="detail-action-icon-btn" style={{ cursor: 'default' }} title={`${(gameStats.downloads[selectedGame.id] || 0).toLocaleString()} ${t.library.totalDownloads}`}>
                <Download size={15} />
                {(() => {
                  const dls = gameStats.downloads[selectedGame.id] || 0
                  return dls > 1000 ? `${(dls / 1000).toFixed(1)}k` : dls
                })()}
              </div>
              
              <button
                type="button"
                className={`detail-action-icon-btn${likedGames.has(selectedGame.id) ? ' active-like' : ''}`}
                title={likedGames.has(selectedGame.id) ? t.library.unlike : t.library.like}
                onClick={() => toggleLike(selectedGame.id)}
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill={likedGames.has(selectedGame.id) ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3" />
                </svg>
                {(() => {
                  const count = optimisticLikes[selectedGame.id] ?? gameStats.likes[selectedGame.id] ?? 0
                  return count > 1000 ? `${(count / 1000).toFixed(1)}k` : count
                })()}
              </button>
            </div>
          </div>
          <div className="store-action-dock" ref={actionDockRef}>
            {(installed && effectiveMode !== 'steam' && (selectedGame?.id.includes('among') || selectedGame?.id === 'persona-3-reload')) && (
              <button type="button" onClick={() => setTutorialVisible(true)}>
                <BookOpen size={15} />
                Tutorial
              </button>
            )}
            {(installed && effectiveMode !== 'steam') && (
              <button type="button" onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
                <FolderOpen size={15} />
                Browse
              </button>
            )}
            {effectiveMode !== 'steam' && (
              <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
                <VerifyIcon size={17} />
                {verifyLabel}
              </button>
            )}
            {showInstallButton && (
              <button
                className={actionClass}
                type="button"
                onClick={primaryActionBtn}
                disabled={primaryDisabled}
                data-stop-label={isPlaying ? 'STOP' : undefined}
              >
                {primaryIcon}
                <span>{actionLabel}</span>
              </button>
            )}

            {/* Steam Play button — shown ONLY in steam mode. Now reusing the main button logic to support STOP state */}
            {effectiveMode === 'steam' && isInstalledOnSteam && (
              <button
                className={isPlaying ? 'primary-control running-btn can-stop' : 'primary-control'}
                type="button"
                onClick={primaryActionBtn}
                data-stop-label={isPlaying ? 'STOP' : undefined}
              >
                {isPlaying ? <Play size={17} /> : <Play size={17} />}
                <span>{isPlaying ? 'Running' : t.library.play}</span>
              </button>
            )}

            {effectiveMode === 'hybrid' && (!isJobRunning && !isPlaying && selectedGameId) && (
              <div style={{ display: 'flex', alignItems: 'center', margin: '0 4px', color: '#888', fontWeight: 600, fontSize: '13px' }}>or</div>
            )}

            {(showSteamButton && !isJobRunning && !isPlaying && selectedGameId) && (
              <SteamIntegrationButton gameId={selectedGameId} gameTitle={detail.title} storeMode={effectiveMode} />
            )}

            {/* Steam Versions button — shown in steam OR hybrid mode */}
            {(effectiveMode === 'steam' || effectiveMode === 'hybrid') && isInstalledOnSteam && showLuaGameTab && (
              <button className="update-control" type="button" onClick={() => setActiveDetailTab('lua-game')}>
                <Download size={17} />
                Steam Versions
              </button>
            )}

            {/* Local Versions button — shown in local OR hybrid mode */}
            {(installed && showVersionAction && effectiveMode !== 'steam') ? (
              <button className="update-control" type="button" onClick={onPrimaryAction} disabled={updateDisabled}>
                <Download size={17} />
                {updateReady ? t.library.update : 'Versions'}
              </button>
            ) : null}
            {(installed && effectiveMode !== 'steam') ? (
              <button className="danger-control" type="button" onClick={onUninstall}>
                <X size={17} />
                {t.library.uninstall}
              </button>
            ) : null}
          </div>
        </div>

        <nav className="detail-tabs">
          <button
            className={activeDetailTab === 'overview' ? 'active' : ''}
            onClick={() => setActiveDetailTab('overview')}
            type="button"
          >
            <Info size={16} /> Overview
          </button>
          <button
            className={activeDetailTab === 'chat' ? 'active' : ''}
            onClick={() => setActiveDetailTab('chat')}
            type="button"
          >
            <MessageSquare size={16} /> Live Chat
          </button>
          {showLuaGameTab && (
            <button
              className={activeDetailTab === 'lua-game' ? 'active' : ''}
              onClick={() => setActiveDetailTab('lua-game')}
              type="button"
              style={{
                background: 'linear-gradient(135deg, rgba(255,215,0,0.1), rgba(255,165,0,0.1))',
                border: '1px solid rgba(255,215,0,0.3)',
                position: 'relative'
              }}
            >
              <Sparkles size={16} /> {t.library.luaGameMode}
              {updateInfo?.needs_update && !updateInfo.is_missing && (
                <span style={{
                  position: 'absolute',
                  top: '6px',
                  right: '6px',
                  width: '8px',
                  height: '8px',
                  background: '#ff4444',
                  borderRadius: '50%',
                  boxShadow: '0 0 8px rgba(255,68,68,0.8)'
                }} title={updateInfo.reason} />
              )}
            </button>
          )}
        </nav>

        {activeDetailTab === 'overview' ? (
          <>
            <MediaRail detail={detail} assets={assets} />

            <section className="detail-body">
              <div className="detail-description">
                <h2>{detail.title}</h2>
                <div
                  className="description-html"
                  dangerouslySetInnerHTML={{ __html: processDescriptionHtml(detail.detailedDescription, assets) }}
                />
              </div>
            </section>
          </>
        ) : activeDetailTab === 'lua-game' ? (
          <section className="detail-body lua-game-tab-container">
            {/* Update Banner */}
            {updateInfo?.needs_update && !updateInfo.is_missing && (
              <div style={{
                marginBottom: '20px',
                padding: '16px 20px',
                background: 'linear-gradient(135deg, rgba(255,165,0,0.15), rgba(255,69,0,0.15))',
                borderRadius: '12px',
                border: '1px solid rgba(255,165,0,0.4)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                boxShadow: '0 4px 12px rgba(255,165,0,0.2)'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                  <Download size={20} style={{ color: '#ffa500' }} />
                  <div>
                    <div style={{ color: '#ffa500', fontWeight: '600', marginBottom: '4px' }}>
                      {t.settings.luaManifestUpdateAvailable}
                    </div>
                    <div style={{ color: '#bbb', fontSize: '13px' }}>
                      {updateInfo.reason}
                    </div>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={async () => {
                    if (!currentSteamAppId) return
                    try {
                      await invoke('add_to_steam', { appid: currentSteamAppId, forceUpdate: true })
                      // Trigger re-check
                      window.location.reload()
                    } catch (e: any) {
                      alert(`Failed to update: ${e}`)
                    }
                  }}
                  style={{
                    padding: '8px 16px',
                    background: 'linear-gradient(135deg, #ffa500, #ff8c00)',
                    border: 'none',
                    borderRadius: '6px',
                    color: '#fff',
                    fontWeight: '600',
                    cursor: 'pointer',
                    fontSize: '14px',
                    whiteSpace: 'nowrap',
                    transition: 'transform 0.2s',
                  }}
                  onMouseEnter={(e) => e.currentTarget.style.transform = 'scale(1.05)'}
                  onMouseLeave={(e) => e.currentTarget.style.transform = 'scale(1)'}
                >
                  🔄 Cập Nhật Ngay
                </button>
              </div>
            )}

            <div style={{
              padding: '40px',
              textAlign: 'center',
              background: 'rgba(255,215,0,0.05)',
              borderRadius: '12px',
              border: '1px solid rgba(255,215,0,0.2)'
            }}>
              <Sparkles size={48} style={{ color: '#ffd700', marginBottom: '20px' }} />
              <h2 style={{ color: '#ffd700', marginBottom: '12px' }}>{t.library.luaGameMode}</h2>
              <p style={{ color: '#aaa', maxWidth: '600px', margin: '0 auto', lineHeight: '1.6' }}>
                {t.library.luaGameModeDesc}
              </p>
              {/* ── Depot Version Switcher ── */}
              <div style={{
                marginTop: '24px',
                padding: '20px',
                background: 'rgba(0,0,0,0.35)',
                backdropFilter: 'blur(12px)',
                WebkitBackdropFilter: 'blur(12px)',
                borderRadius: '12px',
                border: '1px solid rgba(100,180,255,0.25)',
                textAlign: 'left',
                transition: 'all 0.3s cubic-bezier(0.4, 0, 0.2, 1)',
              }}>
                {/* Header */}
                <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '16px', flexWrap: 'wrap' }}>
                  <span style={{ fontSize: '18px' }}>🔄</span>
                  <h3 style={{ margin: 0, fontSize: '15px', color: '#79c0ff' }}>Depot Version Switcher</h3>
                  {depotVersionsLoading ? (
                    <span style={{
                      fontSize: '11px',
                      color: '#79c0ff',
                      background: 'rgba(121,192,255,0.1)',
                      padding: '2px 8px',
                      borderRadius: '20px',
                      animation: 'pulse 1.5s ease-in-out infinite',
                    }}>
                      Loading...
                    </span>
                  ) : depotVersions.length > 0 && (
                    <span style={{ fontSize: '11px', color: '#888', background: 'rgba(255,255,255,0.06)', padding: '2px 8px', borderRadius: '20px' }}>
                      {depotVersions.length} build{depotVersions.length !== 1 ? 's' : ''} available
                    </span>
                  )}
                  {depotCurrentBuildId && (
                    <span style={{
                      fontSize: '11px',
                      color: '#4cd137',
                      background: 'rgba(76,209,55,0.1)',
                      padding: '3px 10px',
                      borderRadius: '20px',
                      border: '1px solid rgba(76,209,55,0.3)',
                      fontFamily: 'monospace',
                      fontWeight: 600,
                      whiteSpace: 'nowrap',
                    }}>
                      ✓ Current: Build {depotCurrentBuildId}
                    </span>
                  )}
                </div>

                {!currentSteamAppId ? (
                  <p style={{ margin: 0, color: '#555', fontSize: '13px' }}>
                    No Steam AppID detected for this game.
                  </p>
                ) : depotVersionsLoading ? (
                  <div style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '12px',
                    padding: '16px',
                    background: 'rgba(121,192,255,0.05)',
                    backdropFilter: 'blur(8px)',
                    WebkitBackdropFilter: 'blur(8px)',
                    borderRadius: '8px',
                    border: '1px solid rgba(121,192,255,0.15)',
                  }}>
                    <div style={{
                      width: '20px',
                      height: '20px',
                      border: '2px solid rgba(121,192,255,0.3)',
                      borderTop: '2px solid #79c0ff',
                      borderRadius: '50%',
                      animation: 'spin 0.8s linear infinite',
                    }} />
                    <span style={{ color: '#79c0ff', fontSize: '13px' }}>
                      Fetching build versions from server...
                    </span>
                  </div>
                ) : depotVersionsError ? (
                  <p style={{ margin: 0, color: '#f55', fontSize: '13px' }}>
                    Error fetching versions: <strong style={{ color: '#faa' }}>{depotVersionsError}</strong>
                  </p>
                ) : depotVersions.length === 0 ? (
                  <p style={{ margin: 0, color: '#555', fontSize: '13px' }}>
                    No build versions found for App ID <strong style={{ color: '#aaa' }}>{currentSteamAppId}</strong>.
                    <br /><span style={{ color: '#444', fontSize: '12px', marginTop: '4px', display: 'block' }}>
                      Depot files not yet uploaded to server for this game.
                    </span>
                  </p>
                ) : (
                  <>
                    {/* Build ID Selection with current build indicator */}
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginBottom: '12px' }}>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: '8px' }}>
                        <label style={{ color: '#888', fontSize: '13px' }}>
                          Select Build Version:
                        </label>
                        {depotCurrentBuildId ? (
                          <span style={{
                            fontSize: '11px',
                            color: '#4cd137',
                            background: 'rgba(76,209,55,0.1)',
                            padding: '2px 8px',
                            borderRadius: '12px',
                            border: '1px solid rgba(76,209,55,0.25)',
                            fontFamily: 'monospace',
                          }}>
                            ✓ Installed: Build {depotCurrentBuildId}
                          </span>
                        ) : (
                          <span style={{
                            fontSize: '11px',
                            color: '#888',
                            background: 'rgba(255,255,255,0.05)',
                            padding: '2px 8px',
                            borderRadius: '12px',
                            border: '1px solid rgba(255,255,255,0.1)',
                            fontStyle: 'italic',
                          }}>
                            Not installed via Steam
                          </span>
                        )}
                      </div>

                      <div className={`version-dropdown${depotDropdownOpen ? ' open' : ''}`}>
                        <button
                          type="button"
                          className="version-dropdown-trigger primary-control"
                          onClick={() => setDepotDropdownOpen(!depotDropdownOpen)}
                          disabled={depotPatching}
                        >
                          <div style={{ display: 'contents' }}>
                            <span>
                              <strong>Build {depotSelectedBuildId}</strong>
                              <small>
                                {depotVersions.find(v => v.buildId === depotSelectedBuildId)?.depots.length || 0} depot(s)
                              </small>
                            </span>
                            {depotCurrentBuildId === depotSelectedBuildId && (
                              <em style={{ background: '#4cd137', color: '#11100d' }}>Current</em>
                            )}
                          </div>
                          <svg width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="6 9 12 15 18 9" /></svg>
                        </button>

                        {depotDropdownOpen && (
                          <div className="version-dropdown-menu">
                            {depotVersions.map(v => (
                              <button
                                key={v.buildId}
                                type="button"
                                className={`version-dropdown-option${v.buildId === depotSelectedBuildId ? ' active' : ''}`}
                                onClick={() => {
                                  setDepotSelectedBuildId(v.buildId)
                                  setDepotPatchResult(null)
                                  setDepotPatchLog([])
                                  setDepotDropdownOpen(false)
                                }}
                              >
                                <svg width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                  <path d="M13.485 1.431a1.473 1.473 0 0 1 2.104 2.062l-7.84 9.801a1.473 1.473 0 0 1-2.12.04L.431 8.138a1.473 1.473 0 0 1 2.084-2.083l4.111 4.112 6.82-8.69a.486.486 0 0 1 .04-.045z" />
                                </svg>
                                <span>
                                  <strong>Build {v.buildId}</strong>
                                  <small>{v.depots.map(d => d.depotId).join(', ')}</small>
                                </span>
                                {depotCurrentBuildId === v.buildId && (
                                  <em style={{ background: '#4cd137' }}>Installed</em>
                                )}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>

                      <button
                        type="button"
                        disabled={depotPatching || !depotSelectedBuildId}
                        onClick={() => depotSelectedBuildId && handleDepotPatch(depotSelectedBuildId)}
                        className="primary-control"
                        style={{
                          padding: '12px 24px',
                          background: depotPatching
                            ? 'rgba(120,120,120,0.2)'
                            : 'linear-gradient(135deg, rgba(30,111,196,0.9), rgba(26,85,160,1))',
                          border: '1px solid rgba(100,180,255,0.4)',
                          borderRadius: '8px',
                          color: depotPatching ? '#555' : '#79c0ff',
                          fontSize: '14px',
                          fontWeight: 600,
                          cursor: depotPatching || !depotSelectedBuildId ? 'not-allowed' : 'pointer',
                          transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          gap: '8px',
                        }}
                      >
                        {depotPatching && depotPatchBuildId === depotSelectedBuildId ? (
                          <>
                            <div style={{
                              width: '16px',
                              height: '16px',
                              border: '2px solid rgba(255,255,255,0.3)',
                              borderTop: '2px solid #fff',
                              borderRadius: '50%',
                              animation: 'spin 0.8s linear infinite',
                            }} />
                            Patching...
                          </>
                        ) : '⚡ Patch to Selected Build'}
                      </button>
                    </div>
                  </>
                )}

                {/* Real-time log console */}
                {(depotPatching || depotPatchLog.length > 0) && (
                  <div style={{
                    marginTop: '12px',
                    padding: '12px',
                    background: 'rgba(0,0,0,0.6)',
                    backdropFilter: 'blur(8px)',
                    WebkitBackdropFilter: 'blur(8px)',
                    borderRadius: '8px',
                    border: '1px solid rgba(100,180,255,0.2)',
                    fontFamily: 'monospace',
                    fontSize: '12px',
                    maxHeight: '180px',
                    overflowY: 'auto',
                    color: '#aaa',
                    lineHeight: '1.6',
                  }}>
                    {depotPatching && <div style={{ color: '#79c0ff', marginBottom: '4px' }}>● Running…</div>}
                    {depotPatchLog.map((line, i) => (
                      <div key={i} style={{
                        color: line.startsWith('✓') ? '#4cd137' : line.startsWith('✗') ? '#ff6b6b' : '#aaa'
                      }}>{line}</div>
                    ))}
                  </div>
                )}

                {/* Result banner */}
                {depotPatchResult && !depotPatching && (
                  <div style={{
                    marginTop: '10px',
                    padding: '12px 16px',
                    borderRadius: '8px',
                    background: depotPatchResult.ok ? 'rgba(50,200,100,0.12)' : 'rgba(255,50,50,0.12)',
                    backdropFilter: 'blur(8px)',
                    WebkitBackdropFilter: 'blur(8px)',
                    border: `1px solid ${depotPatchResult.ok ? 'rgba(50,200,100,0.35)' : 'rgba(255,50,50,0.35)'}`,
                    color: depotPatchResult.ok ? '#4cd137' : '#ff6b6b',
                    fontSize: '13px',
                    fontWeight: 500,
                    animation: 'fadeIn 0.3s ease-out',
                  }}>
                    {depotPatchResult.ok ? '✅' : '❌'} {depotPatchResult.msg}
                  </div>
                )}
              </div>


              {/* ── Steamless / Error 54 Fix ── */}
              <div style={{
                marginTop: '16px',
                padding: '20px',
                background: 'rgba(0,0,0,0.2)',
                borderRadius: '8px',
                border: '1px solid rgba(255,255,255,0.1)',
                textAlign: 'left'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '10px' }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <h3 style={{ margin: 0, fontSize: '16px', color: '#fff' }}>{t.library.luaGameModeError54Fix}</h3>
                    <span style={{ fontSize: '12px', color: '#00fa9a', background: 'rgba(0,250,154,0.1)', padding: '2px 6px', borderRadius: '4px' }}>
                      {t.library.luaGameModeError54Recommended}
                    </span>
                  </div>
                  <button
                    type="button"
                    className={steamlessStatus ? 'settings-toggle is-on' : 'settings-toggle'}
                    role="switch"
                    aria-checked={steamlessStatus}
                    disabled={steamlessLoading || !selectedInstallState?.installPath}
                    style={{
                      opacity: steamlessLoading ? 0.5 : 1,
                      cursor: steamlessLoading || !selectedInstallState?.installPath ? 'not-allowed' : 'pointer'
                    }}
                    onClick={() => !steamlessLoading && handleToggleSteamless()}
                  >
                    <span />
                  </button>
                </div>
                <p style={{ margin: 0, color: '#888', fontSize: '14px', lineHeight: '1.5' }}>
                  {t.library.luaGameModeError54Desc}
                </p>
                {steamlessMessage && (
                  <div style={{
                    marginTop: '15px',
                    padding: '10px',
                    borderRadius: '6px',
                    background: steamlessMessage.isError ? 'rgba(255,50,50,0.1)' : 'rgba(50,255,50,0.1)',
                    color: steamlessMessage.isError ? '#ff6b6b' : '#4cd137',
                    fontSize: '13px',
                    border: `1px solid ${steamlessMessage.isError ? 'rgba(255,50,50,0.2)' : 'rgba(50,255,50,0.2)'}`
                  }}>
                    {steamlessMessage.text}
                  </div>
                )}
              </div>
            </div>
          </section>
        ) : (
          <section className="detail-body chat-tab-container">
            <GameChat gameId={detail.gameId} discordUser={discordUser} />
          </section>
        )}
      </section>

      {activeDetailTab === 'overview' && (
        <aside className="store-info-column">
          <section className="panel status-card">
            <header className="side-header">
              <CheckCircle2 size={17} />
              <strong>{stateLabel}</strong>
            </header>
            <dl className="metric-list">
              {effectiveMode === 'steam' ? (
                <div>
                  <dt>Steam Status</dt>
                  <dd>{isInstalledOnSteam ? `Installed (Build ${steamBuildId || 'Unknown'})` : 'Not Installed on Steam'}</dd>
                </div>
              ) : (
                <>
                  <div>
                    <dt>{t.library.currentVersion}</dt>
                    <dd>{installed ? selectedCurrentVersion : t.library.notInstalled}</dd>
                  </div>
                  <div>
                    <dt>{t.library.latestVersion}</dt>
                    <dd>{selectedGame.latestVersion}</dd>
                  </div>
                  <div>
                    <dt>{t.library.targetVersion}</dt>
                    <dd>{selectedVersion}</dd>
                  </div>
                  <div>
                    <dt>Install size</dt>
                    <dd>{formatBytes(downloadSize)}</dd>
                  </div>
                </>
              )}
            </dl>
          </section>
          {effectiveMode !== 'steam' && (
            <InstallSummaryPanel
              selectedVersion={selectedVersion}
              downloadSize={downloadSize}
              installSize={installSize || selectedVersionInfo?.sizeBytes || downloadSize}
              temporarySpace={temporarySpace || selectedVersionInfo?.sizeBytes || downloadSize}
            />
          )}
          {verifyStatus ? (
            <section className={`panel verify-feedback ${verifyStatus.state}`}>
              <header className="side-header">
                <VerifyIcon size={17} />
                <strong>{isVerifying ? 'Verifying install' : 'Verify result'}</strong>
              </header>
              <p>{verifyStatus.message}</p>
              {verifyStatus.state === 'failed' ? (
                <div className="verify-count-summary">
                  <span>
                    <strong>{missingCount}</strong>
                    missing
                  </span>
                  <span>
                    <strong>{changedCount}</strong>
                    changed
                  </span>
                </div>
              ) : null}
              <div className="verify-progress">
                <div className="mini-track">
                  <span style={{ width: `${Math.round((verifyStatus.percent ?? 0) * 100)}%` }} />
                </div>
                <small>
                  {Math.round((verifyStatus.percent ?? 0) * 100)}%
                  {verifyStatus.totalBytes ? ` - ${formatBytes(verifyStatus.checkedBytes ?? 0)} / ${formatBytes(verifyStatus.totalBytes)}` : ''}
                </small>
              </div>
              {verifyStatus.currentFile ? <small className="verify-current-file">{verifyStatus.currentFile}</small> : null}
            </section>
          ) : null}
          <GameDetailsPanel detail={detail} />
          {installed ? (
            <CloudSavePanel
              status={cloudSaveStatus}
              busy={cloudSaveBusy}
              launchBlocked={cloudLaunchBlocked}
              onToggle={onToggleCloudSave}
              onAddFolder={onAddCloudSaveFolder}
              onSync={onSyncCloudSave}
              onResolve={onResolveCloudConflict}
              onRestore={onRestoreCloudSnapshot}
              onLaunchWithoutSync={onLaunchWithoutCloudSync}
              onConnectGoogleDrive={onConnectGoogleDrive}
              onDisconnectGoogleDrive={onDisconnectGoogleDrive}
              onBackupGoogleDrive={onBackupGoogleDrive}
              onRestoreMissingFiles={onRestoreMissingSaveFiles}
            />
          ) : null}
          <OSTPlayer bgImage={hero || logo || undefined} gameId={selectedGame.id} />
          <GameTagsPreview game={selectedGame} />
          <AchievementPreview achievements={detail.achievements} assets={assets} />
        </aside>
      )}
      {tutorialVisible && selectedGame ? (
        <TutorialModal
          gameId={selectedGame.id}
          onClose={() => setTutorialVisible(false)}
        />
      ) : null}
    </section>
  )
}
export function OperationHero({
  game,
  detail,
  assets,
  currentVersion,
  latestVersion,
  updateReady,
  showVersionAction,
  updateSize,
  onUpdate,
  onPlay,
  onStop,
  isJobRunning,
  isGameRunning,
  canUpdate,
  installMode,
  selectedVersion,
  storeMode = 'hybrid',
}: {
  game: GameSummary
  detail: GameDetail
  assets: Record<string, string>
  currentVersion: string
  latestVersion: string
  updateReady: boolean
  showVersionAction: boolean
  updateSize: number
  onUpdate: () => void
  onPlay: () => void
  onStop: () => void
  isJobRunning: boolean
  isGameRunning: boolean
  canUpdate: boolean
  installMode: boolean
  selectedVersion: string
  storeMode?: 'local' | 'hybrid' | 'steam'
}) {
  const { t } = useLocale()
  const hero = assetUrlForId(game.heroAssetId, assets) || firstMediaUrl(detail, assets)
  const stateLabel = installMode ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay

  let playLabel = t.library.play.toUpperCase()
  let playClass = 'update-button hero-play-button'
  if (isGameRunning) {
    playLabel = 'RUNNING'
    playClass = 'update-button running-btn can-stop'
  } else if (isJobRunning) {
    playLabel = 'DOWNLOADING'
    playClass = 'update-button downloading-btn'
  }

  const playDisabled = isJobRunning
  const updateDisabled = isGameRunning || isJobRunning || !canUpdate

  return (
    <section className="hero-panel">
      {hero ? <img src={hero} alt="" loading="eager" fetchPriority="high" decoding="async" /> : null}
      <div className="game-strip">
        <div className="game-emblem">
          {assetUrlForId(game.iconAssetId, assets) ? <img src={assetUrlForId(game.iconAssetId, assets)} alt="" decoding="async" loading="lazy" /> : <ImageIcon size={28} />}
        </div>
        <div>
          <h1>{game.title}</h1>
          <div className="version-row">
            <VersionStat label={t.library.currentVersion} value={currentVersion} />
            <VersionStat label={t.library.latestVersion} value={latestVersion} highlight />
            <VersionStat label={t.library.targetVersion} value={selectedVersion} />
            <div className="ready-state">
              <CheckCircle2 size={20} />
              <span>{stateLabel}</span>
              <small>{formatBytes(updateSize)}</small>
            </div>
          </div>
        </div>
        <div className="hero-action-group">
          {installMode ? (
            <button
              className={`update-button${isJobRunning ? ' downloading-btn' : ''}`}
              type="button"
              onClick={onUpdate}
              disabled={isJobRunning || !canUpdate}
            >
              <span>{isJobRunning ? 'DOWNLOADING' : (!isTauriRuntime() ? 'REMOTE INSTALL' : t.library.chooseInstall.toUpperCase())}</span>
              <Download size={18} />
            </button>
          ) : (
            <>
              <button className={playClass} type="button" onClick={isGameRunning ? onStop : onPlay} disabled={playDisabled}
                data-stop-label={isGameRunning ? 'STOP' : undefined}
              >
                <span>{playLabel}</span>
                {isJobRunning ? <Download size={18} /> : <Play size={18} />}
              </button>
              {showVersionAction ? (
                <button className="update-button" type="button" onClick={onUpdate} disabled={updateDisabled}>
                  <span>{updateReady ? t.library.update.toUpperCase() : 'VERSIONS'}</span>
                  <Download size={18} />
                </button>
              ) : null}
            </>
          )}
          {(!isJobRunning && !isGameRunning) && <SteamIntegrationButton gameId={game.id} gameTitle={game.title} storeMode={storeMode} />}
        </div>
      </div>
    </section>
  )
}

function SteamIntegrationButton({ gameId, gameTitle, storeMode }: { gameId: string, gameTitle: string, storeMode: 'local' | 'hybrid' | 'steam' }) {
  const [status, setStatus] = useState<boolean>(false)
  const [loading, setLoading] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const [showRestartConfirm, setShowRestartConfirm] = useState(false)
  const [showEnableModePrompt, setShowEnableModePrompt] = useState(false)
  const [autoInstall, setAutoInstall] = useState(() => localStorage.getItem('steamAutoInstall') !== 'false')
  const [skipConfirm, setSkipConfirm] = useState(() => localStorage.getItem('steamSkipRestartConfirm') === 'true')
  const { mapping } = useSteamAppIds()
  const { t } = useLocale()

  const appid = mapping[gameId]

  // Hide in "local" mode
  if (storeMode === 'local') {
    return null
  }

  useEffect(() => {
    if (!appid) return
    checkStatus()
  }, [appid])

  const checkStatus = async () => {
    try {
      const isAdded = await invoke<boolean>('check_steam_status', { appid })
      setStatus(isAdded)
    } catch (e) {
      console.error('Failed to check steam status', e)
    }
  }

  const showToast = (title: string, msg: string, severity: 'success' | 'error' | 'info' = 'info') => {
    window.dispatchEvent(new CustomEvent('0xo-toast', {
      detail: {
        category: 'launcher',
        severity,
        title,
        message: msg,
        dedupeKey: `steam:${appid}`,
      }
    }))
  }

  const performRestart = async () => {
    try {
      const args = autoInstall ? { postRestartAction: `steam://install/${appid}` } : {}
      await invoke('force_restart_steam', args)
      showToast(t.library.restartSteamPrompt, t.settings.restartSteam + '...', 'info')
    } catch (e) {
      console.error(e)
      showToast('Error', String(e), 'error')
    }
  }

  const handleAdd = async () => {
    if (!appid) return

    // Check if Lua-Game Mode is enabled first
    try {
      const isEnabled = await invoke<boolean>('is_lua_game_mode_enabled')
      if (!isEnabled) {
        setShowEnableModePrompt(true)
        return
      }
    } catch (e) {
      console.error('Failed to check lua-game mode status', e)
      showToast('Error', 'Failed to check Lua-Game Mode status', 'error')
      return
    }

    setLoading(true)
    try {
      const checkResult = await invoke('check_steam_update', { appid }) as { needs_update: boolean, reason: string, is_missing: boolean }

      let forceUpdate = false
      if (checkResult.needs_update) {
        if (checkResult.is_missing) {
          showToast(t.library.addToSteam, `Creating config for ${gameTitle} (30-60s)...`, 'info')
          forceUpdate = true
        } else {
          const { ask } = await import('@tauri-apps/plugin-dialog')
          const shouldUpdate = await ask(`Update available.\nReason: ${checkResult.reason}\n\nFetch latest version?`, {
            title: 'Data Update',
            kind: 'info',
          })

          if (shouldUpdate) {
            showToast(t.library.addToSteam, `Downloading update for ${gameTitle} (30-60s)...`, 'info')
            forceUpdate = true
          }
        }
      }

      await invoke('add_to_steam', { appid, forceUpdate })
      setStatus(true)
      showToast(t.library.addToSteam, t.library.addToSteamSuccess, 'success')

      // Dispatch event to show Lua-Game Mode tab
      window.dispatchEvent(new CustomEvent('lua-game-mode-changed', {
        detail: { gameId, added: true }
      }))

      // Show restart prompt
      if (localStorage.getItem('steamSkipRestartConfirm') === 'true') {
        performRestart();
      } else {
        setShowRestartConfirm(true);
      }
    } catch (e) {
      console.error(e)
      showToast(t.library.addToSteam, t.library.addToSteamError + ': ' + String(e), 'error')
    }
    setLoading(false)
  }

  const handleRemove = async () => {
    if (!appid) return
    setShowRemoveConfirm(true)
  }

  const confirmRemove = async () => {
    setShowRemoveConfirm(false)
    if (!appid) return

    setLoading(true)
    try {
      await invoke('remove_from_steam', { appid })
      setStatus(false)
      showToast(t.library.removeFromSteam, t.library.removeFromSteamSuccess, 'success')

      // Dispatch event to hide Lua-Game Mode tab
      window.dispatchEvent(new CustomEvent('lua-game-mode-changed', {
        detail: { gameId, added: false }
      }))

      // Show restart prompt
      if (localStorage.getItem('steamSkipRestartConfirm') === 'true') {
        performRestart();
      } else {
        setShowRestartConfirm(true);
      }
    } catch (e) {
      console.error(e)
      showToast(t.library.removeFromSteam, t.library.removeFromSteamError + ': ' + String(e), 'error')
    }
    setLoading(false)
  }

  const handleRestart = async () => {
    if (!appid) return
    if (localStorage.getItem('steamSkipRestartConfirm') === 'true') {
      performRestart();
    } else {
      setShowRestartConfirm(true);
    }
  }

  const handleNavigateToSettings = () => {
    setShowEnableModePrompt(false)
    // Dispatch event to navigate to Settings
    window.dispatchEvent(new CustomEvent('navigate-to-settings', {
      detail: { section: 'steam-integration' }
    }))
  }

  if (!appid) return null

  return (
    <>
      <div className="steam-integration-wrapper" style={{ position: 'relative', display: 'flex', alignItems: 'center', gap: '8px', marginLeft: '10px' }}>
        {storeMode === 'hybrid' && <span style={{ fontSize: '14px', color: '#888', fontWeight: 500, marginRight: '4px' }}>{(t as any).common?.or || 'or'}</span>}
        {status ? (
          <>
            <button
              style={{
                display: 'flex', alignItems: 'center', gap: '6px', padding: '0 16px', height: '46px',
                borderRadius: '5px', background: 'transparent', border: '1px solid #4ade80',
                color: '#4ade80', fontWeight: 600, cursor: 'default', whiteSpace: 'nowrap', flexShrink: 0
              }}
              disabled
            >
              <CheckCircle2 size={16} />
              <span>{t.library.addedToSteam}</span>
            </button>
            <button
              style={{
                display: 'flex', alignItems: 'center', gap: '6px', padding: '0 16px', height: '46px',
                borderRadius: '5px', background: 'rgba(255,255,255,0.1)', backdropFilter: 'blur(5px)',
                border: '1px solid rgba(255,255,255,0.2)', color: '#fff', fontWeight: 600, cursor: 'pointer', whiteSpace: 'nowrap', flexShrink: 0
              }}
              onClick={handleRestart}
              disabled={loading}
              onMouseEnter={e => e.currentTarget.style.background = 'rgba(255,255,255,0.2)'}
              onMouseLeave={e => e.currentTarget.style.background = 'rgba(255,255,255,0.1)'}
            >
              <span>{t.settings.restartSteam}</span>
            </button>
            <button
              style={{
                display: 'flex', alignItems: 'center', justifyContent: 'center', width: '46px', height: '46px',
                borderRadius: '5px', background: 'rgba(255,0,0,0.1)', backdropFilter: 'blur(5px)',
                border: '1px solid rgba(255,0,0,0.3)', color: '#ff4d4d', cursor: 'pointer', flexShrink: 0
              }}
              onClick={handleRemove}
              disabled={loading}
              onMouseEnter={e => e.currentTarget.style.background = 'rgba(255,0,0,0.2)'}
              onMouseLeave={e => e.currentTarget.style.background = 'rgba(255,0,0,0.1)'}
            >
              <X size={16} />
            </button>
          </>
        ) : (
          <button
            style={{
              display: 'flex', alignItems: 'center', gap: '6px', padding: '0 16px', height: '46px',
              borderRadius: '5px', background: 'rgba(255,255,255,0.1)', backdropFilter: 'blur(5px)',
              border: '1px solid rgba(255,255,255,0.2)', color: '#fff', fontWeight: 600, cursor: 'pointer', whiteSpace: 'nowrap', flexShrink: 0
            }}
            onClick={handleAdd}
            disabled={loading}
            onMouseEnter={e => e.currentTarget.style.background = 'rgba(255,255,255,0.2)'}
            onMouseLeave={e => e.currentTarget.style.background = 'rgba(255,255,255,0.1)'}
          >
            <PlusCircle size={18} />
            <span>{t.library.addToSteam}</span>
          </button>
        )}
      </div>

      {/* Remove Confirmation Dialog */}
      {showRemoveConfirm && (
        <ConfirmDialog
          title={t.library.confirmRemoveTitle}
          message={t.library.confirmRemoveMessage}
          confirmText={t.library.confirmRemoveYes}
          cancelText={t.library.confirmRemoveNo}
          variant="warning"
          onConfirm={confirmRemove}
          onCancel={() => setShowRemoveConfirm(false)}
        />
      )}

      {/* Restart Steam Confirmation Dialog */}
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
            {/* Auto Install toggle — uses the same settings-toggle CSS class */}
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
            {/* Remember my choice — also a settings-toggle */}
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

      {/* Enable Lua-Game Mode Prompt */}
      {showEnableModePrompt && (
        <ConfirmDialog
          title={t.settings.luaGameMode}
          message={t.settings.luaGameModeRequired}
          confirmText={t.settings.enableLuaGameMode}
          cancelText="Cancel"
          variant="warning"
          onConfirm={handleNavigateToSettings}
          onCancel={() => setShowEnableModePrompt(false)}
        />
      )}
    </>
  )
}

export function VersionStat({ label, value, highlight = false }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="version-stat">
      <small>{label}</small>
      <strong className={highlight ? 'gold-text' : ''}>{value}</strong>
    </div>
  )
}

export function MediaRail({ detail, assets }: { detail: GameDetail; assets: Record<string, string> }) {
  const { t } = useLocale()
  const safeMedia = Array.isArray(detail.media) ? detail.media : []

  // Build a thumb map: video item id -> thumbnail URL
  // e.g. "movie-00" -> URL from item with id "movie-thumb-00"
  const videoThumbMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (const item of safeMedia) {
      // Handle all possible thumbnail role names (Firestore may use any of these)
      const isThumbRole =
        item.role === 'video-thumb' ||
        item.role === 'video-thumbnail' ||
        item.role === 'video-poster'
      const url = isThumbRole ? assetUrlForId(item.assetId, assets) : undefined
      if (!url) continue
      // Derive the video id from the thumb id:
      //   "movie-thumb-0"     → "movie-0"
      //   "movie-thumbnail-0" → "movie-0"
      //   "movie-poster-0"    → "movie-0"
      const videoId = item.id
        .replace(/^movie-thumb-/, 'movie-')
        .replace(/^movie-thumbnail-/, 'movie-')
        .replace(/^movie-poster-/, 'movie-')
      if (!map[videoId]) map[videoId] = url  // first wins
    }
    return map
  }, [safeMedia, assets])


  const media = safeMedia
    .filter((item) => isCarouselMedia(item) && assetUrlForId(item.assetId, assets))
    .sort((left, right) => mediaPriority(left) - mediaPriority(right))
    .map((item) => ({ ...item, url: assetUrlForId(item.assetId, assets)! }))
  const [activeIndex, setActiveIndex] = useState(0)

  if (media.length === 0) {
    return null
  }
  const safeActiveIndex = Math.min(activeIndex, media.length - 1)
  const active = media[safeActiveIndex]
  const activeIsVideo = active.mimeType.startsWith('video/') || active.role === 'video' || active.role === 'video-preview'
  const go = (direction: -1 | 1) => {
    setActiveIndex((current) => (current + direction + media.length) % media.length)
  }

  return (
    <section className="media-section media-carousel-section">
      <header>
        <strong>{t.library.media}</strong>
        <small>
          {media.length} items - {detail.metadataSource}
        </small>
      </header>
      <div className="media-carousel">
        <div className="media-stage">
          {activeIsVideo ? (
            <video src={active.url} controls muted preload="metadata" poster={videoThumbMap[active.id]} />
          ) : (
            <>
              <img src={active.url} alt="" loading="lazy" />
              {active.role === 'video-preview' ? (
                <span className="media-play-badge" aria-hidden="true">
                  <Play size={22} />
                </span>
              ) : null}
            </>
          )}
          <button className="media-nav prev" type="button" onClick={() => go(-1)} aria-label="Previous media">
            <ChevronLeft size={22} />
          </button>
          <button className="media-nav next" type="button" onClick={() => go(1)} aria-label="Next media">
            <ChevronRight size={22} />
          </button>
          <div className="media-stage-caption">
            <strong>{active.title}</strong>
            <span>{active.role}</span>
          </div>
        </div>
        <div className="media-thumb-rail">
          {media.map((item, index) => {
            const isVideo = item.mimeType.startsWith('video/') || item.role === 'video' || item.role === 'video-preview'
            const thumbUrl = isVideo ? (videoThumbMap[item.id] ?? null) : null

            return (
              <button
                className={index === safeActiveIndex ? 'media-thumb active' : 'media-thumb'}
                key={item.id}
                type="button"
                onClick={() => setActiveIndex(index)}
              >
                {isVideo ? (
                  <span className="image-video-thumb">
                    {thumbUrl ? (
                      <img src={thumbUrl} alt="" loading="lazy" />
                    ) : (
                      <span className="video-thumb-placeholder"><Play size={24} /></span>
                    )}
                    <Play size={16} className="video-thumb-overlay" />
                  </span>
                ) : (
                  <img src={item.url} alt="" loading="lazy" />
                )}
              </button>
            )
          })}
        </div>
      </div>
      <div className="media-rail legacy-hidden">
        {media.map((item) => (
          <article key={item.id}>
            {item.mimeType.startsWith('video/') ? (
              <video src={item.url} muted controls />
            ) : (
              <img src={item.url} alt="" loading="lazy" />
            )}
            <span>{item.title}</span>
          </article>
        ))}
      </div>
    </section>
  )
}

function GameTagsPreview({ game }: { game: GameSummary }) {
  const tags = getGameTags(game)
  if (!tags || tags.length === 0) return null
  return (
    <div className="game-detail-tags">
      {tags.map((tag) => (
        <span key={tag.id} className={`game-detail-tag tone-${tag.tone}`}>
          {tag.label}
        </span>
      ))}
    </div>
  )
}

export function AchievementPreview({
  achievements,
  assets,
}: {
  achievements: GameAchievement[]
  assets: Record<string, string>
}) {
  const { t } = useLocale()
  const [showAll, setShowAll] = useState(false)
  const safeAchievements = Array.isArray(achievements) ? achievements : []
  const available = safeAchievements.filter((achievement) => assetUrlForId(achievement.iconAssetId, assets))
  const preview = available.slice(0, 10)

  // KHẮC PHỤC CẢNH BÁO LỖI 'any': Định nghĩa kiểu dữ liệu chuẩn cho Lenis
  useEffect(() => {
    interface LenisWindow {
      __lenis?: {
        stop: () => void
        start: () => void
      }
    }
    const lenis = (window as unknown as LenisWindow).__lenis

    if (showAll) {
      lenis?.stop()
    } else {
      lenis?.start()
    }
    return () => {
      lenis?.start()
    }
  }, [showAll])

  if (available.length === 0) {
    return null
  }

  return (
    <section className="achievement-section">
      <header>
        <strong>{t.library.achievements}</strong>
        <div className="achievement-header-actions">
          <small>{safeAchievements.length} total</small>
          {/* Thêm aria-label để sửa cảnh báo vàng của ESLint */}
          <button type="button" aria-label="See all achievements" onClick={() => setShowAll(true)}>
            <Trophy size={15} />
            See all
          </button>
        </div>
      </header>

      {/* SỬA LỖI ĐÈ/CHỒNG CHÉO: Thêm gridAutoRows: 'max-content' */}
      <div className="achievement-grid" style={{ gridAutoRows: 'max-content' }}>
        {preview.map((achievement) => (
          <article key={achievement.id}>
            <img src={assetUrlForId(achievement.iconAssetId, assets)} alt="" loading="lazy" />
            <div>
              <strong>{achievement.name}</strong>
              <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
            </div>
          </article>
        ))}
      </div>

      {/* Dùng createPortal đẩy popup ra ngoài cùng <body> */}
      {/* Thêm dấu chấm than (!) vào document.body! để báo cho TS biết nó chắc chắn tồn tại */}
      {showAll && typeof document !== 'undefined' ? createPortal(
        <div className="dialog-backdrop" style={{ zIndex: 99999 }} role="presentation" onClick={() => setShowAll(false)}>
          <section className="achievement-modal achievement-modal--enter" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <header>
              <div>
                <strong>{t.library.achievements}</strong>
                <span>{available.length} achievement entries</span>
              </div>
              <button type="button" aria-label="Close" onClick={() => setShowAll(false)}>
                <X size={17} />
              </button>
            </header>

            {/* SỬA LỖI ĐÈ/CHỒNG CHÉO: Thêm style={{ gridAutoRows: 'max-content' }} */}
            <div className="achievement-all-grid" data-lenis-prevent="true" style={{ gridAutoRows: 'max-content' }}>
              {available.map((achievement) => (
                <article key={achievement.id}>
                  <img src={assetUrlForId(achievement.iconAssetId, assets)} alt="" loading="lazy" />
                  <div>
                    <strong>{achievement.name}</strong>
                    <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
                  </div>
                </article>
              ))}
            </div>
          </section>
        </div>,
        document.body!
      ) : null}
    </section>
  )
}
