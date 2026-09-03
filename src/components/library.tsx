import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState, memo } from 'react'
import type { PointerEvent as ReactPointerEvent, ReactElement } from 'react'
import { doc, setDoc, increment } from 'firebase/firestore'
import { contentDb as db } from '../firebase'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { BookOpen, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, PlusCircle, Download, FolderOpen, HardDrive, Image as ImageIcon, Library, Play, RefreshCcw, Search, ShieldCheck, ShoppingBag, SlidersHorizontal, ThumbsUp, Trash2, Trophy, X, MessageSquare, Info, Sparkles, Clock3, TrendingUp } from 'lucide-react'
import { useGameStats } from '../hooks/useGameStats'
import { TutorialModal } from './TutorialModal'
import { useLocale } from '../context/locale'
import { useSteamAppIds } from '../hooks/useSteamAppIds'
import { useLuaUpdateCheck } from '../hooks/useLuaUpdateCheck'
import type { CloudSaveStatus, GameAchievement, GameCatalog, GameDetail, GameMedia, GameSummary, GameInstallState, GameVersionInfo, LuaGameChannel, LuaGameState, LuaSourceOperation, LuaSourceProvider, VerifyUiStatus } from '../types'
import { assetUrlForId, firstMediaUrl, isCarouselMedia, mediaPriority, processDescriptionHtml, thumbnailUrlForMedia, isTauriRuntime } from '../lib/gameMeta'
import { formatBytes } from '../lib/format'
import { getGameTags, gameHasTag } from '../lib/gameTags'
import { GameDetailsPanel, InstallSummaryPanel, OSTPlayer } from './panels'
import { CloudSavePanel } from './CloudSavePanel'
import { GameChat } from './GameChat'
import { ConfirmDialog } from './ConfirmDialog'
import { LuaSourcePickerDialog } from './LuaSourcePickerDialog'
import { useRealtimeConfig } from '../hooks/useRealtimeConfig'
import { useFirestoreDetail } from '../hooks/useFirestoreDetail'
import { SaveBackupIndicator } from './SaveBackupIndicator'
import { normalizeStoreSearchTerm, rankStoreGames, type StoreSearchFilter, type StoreSearchResult } from '../lib/storeSearch'
import type { UiThemeId } from '../lib/uiThemes'
import { getThemeLibraryPresentation } from '../themes/libraryPolicy'
import { useStoreSearchTelemetry, type StoreSearchTermStat } from '../hooks/useStoreSearchTelemetry'
import { useLauncherLibraryLayout } from '../hooks/useLauncherLibraryLayout'
import { SteamLibraryHome } from '../themes/steam/SteamLibraryHome'
import { SteamLibraryActivity } from '../themes/steam/SteamLibraryActivity'
import { SteamLibraryDetail } from '../themes/steam/SteamLibraryDetail'
import { SteamStoreHome } from '../themes/steam/SteamStoreHome'

function LazyGameCardImageBase({
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
  const [loadedUrl, setLoadedUrl] = useState<string | undefined>()
  const imageLoaded = loadedUrl === url

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

  return (
    <div
      ref={ref}
      className={`store-card-image-wrapper ${!imageLoaded ? 'is-loading' : ''} ${url ? 'has-image' : 'is-empty'}`}
    >
      {url ? (
        <img
          src={url}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          className={imageLoaded ? 'loaded' : 'loading'}
          onLoad={() => setLoadedUrl(url)}
        />
      ) : (
        <span className="store-card-image-placeholder" aria-hidden="true">
          <ImageIcon size={variant === 'browse' ? 34 : 26} />
        </span>
      )}
    </div>
  )
}

// Memo-ize để tránh re-render khi parent component update nhưng image props không đổi
const LazyGameCardImage = memo(LazyGameCardImageBase)

const STORE_SEARCH_HISTORY_KEY = '0xo_store_search_history_v1'

const STORE_SEARCH_FILTERS: Array<{ id: StoreSearchFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'installed', label: 'Installed' },
  { id: 'popular', label: 'Popular' },
  { id: 'new', label: 'New releases' },
  { id: 'downloaded', label: 'Most downloaded' },
]

function readStoreSearchHistory(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(STORE_SEARCH_HISTORY_KEY) || '[]')
    return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string').slice(0, 12) : []
  } catch {
    return []
  }
}

function compactMetric(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}m`
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}k`
  return value.toLocaleString()
}

function StoreSearchOverlay({
  open,
  query,
  filter,
  results,
  history,
  trending,
  searchVolume,
  statsLoading,
  assets,
  downloads,
  likes,
  installedGameIds,
  onQueryChange,
  onFilterChange,
  onClose,
  onSubmit,
  onSuggestion,
  onClearHistory,
  onSelectResult,
  onRequestAsset,
}: {
  open: boolean
  query: string
  filter: StoreSearchFilter
  results: StoreSearchResult[]
  history: string[]
  trending: StoreSearchTermStat[]
  searchVolume: number | null
  statsLoading: boolean
  assets: Record<string, string>
  downloads: Record<string, number>
  likes: Record<string, number>
  installedGameIds: Set<string>
  onQueryChange: (value: string) => void
  onFilterChange: (value: StoreSearchFilter) => void
  onClose: () => void
  onSubmit: () => void
  onSuggestion: (term: string) => void
  onClearHistory: () => void
  onSelectResult: (game: GameSummary) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const normalizedQuery = normalizeStoreSearchTerm(query)
  const hasQuery = normalizedQuery.length > 0

  useEffect(() => {
    if (!open) return
    document.body.classList.add('store-search-overlay-open')
    const focusTimer = window.setTimeout(() => inputRef.current?.focus(), 30)
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.clearTimeout(focusTimer)
      window.removeEventListener('keydown', handleKeyDown)
      document.body.classList.remove('store-search-overlay-open')
    }
  }, [onClose, open])

  if (!open || typeof document === 'undefined') return null

  return createPortal(
    <div className="store-search-overlay" role="dialog" aria-modal="true" aria-label="Search the store">
      <div className="store-search-backdrop" onClick={onClose} />
      <section className="store-search-surface">
        <header className="store-search-hero">
          <form
            className="store-search-command"
            onSubmit={(event) => {
              event.preventDefault()
              onSubmit()
            }}
          >
            <Search size={22} />
            <input
              ref={inputRef}
              aria-label="Search games, AppIDs, developers, tags, or versions"
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder="Search games, AppID, developer, tag, version..."
              autoComplete="off"
              spellCheck="false"
            />
            {query ? (
              <button type="button" className="store-search-clear" onClick={() => onQueryChange('')} title="Clear search">
                <X size={18} />
              </button>
            ) : (
              <kbd>Ctrl K</kbd>
            )}
          </form>
          <button type="button" className="store-search-close" onClick={onClose} title="Close search">
            <X size={20} />
          </button>
        </header>

        <div className="store-search-filters" role="tablist" aria-label="Search filters">
          {STORE_SEARCH_FILTERS.map((option) => (
            <button
              key={option.id}
              type="button"
              role="tab"
              aria-selected={filter === option.id}
              className={filter === option.id ? 'active' : ''}
              onClick={() => onFilterChange(option.id)}
            >
              {option.label}
            </button>
          ))}
        </div>

        {!hasQuery ? (
          <div className="store-search-discovery">
            <div className="store-search-discovery-columns">
              <section>
                <div className="store-search-section-title">
                  <span><Clock3 size={15} /> Recent searches</span>
                  {history.length ? <button type="button" onClick={onClearHistory}>Clear</button> : null}
                </div>
                <div className="store-search-chips">
                  {history.length ? history.map((term) => (
                    <button key={term} type="button" onClick={() => onSuggestion(term)}>{term}</button>
                  )) : <p>Your recent searches will appear here.</p>}
                </div>
              </section>
              <section>
                <div className="store-search-section-title">
                  <span><TrendingUp size={15} /> Trending now</span>
                  {statsLoading ? <small>Updating...</small> : null}
                </div>
                <div className="store-search-chips trending">
                  {trending.length ? trending.slice(0, 8).map((entry) => (
                    <button key={entry.term} type="button" onClick={() => onSuggestion(entry.term)}>
                      <span>{entry.term}</span>
                      <small>{compactMetric(entry.searches)}</small>
                    </button>
                  )) : <p>Trending searches will appear as the community uses search.</p>}
                </div>
              </section>
            </div>
            <div className="store-search-results-heading">
              <span><Sparkles size={16} /> Recommended for discovery</span>
              <small>Popularity, freshness, and community interest</small>
            </div>
          </div>
        ) : (
          <div className="store-search-results-heading">
            <span>{results.length} result{results.length === 1 ? '' : 's'} for “{query.trim()}”</span>
            <small>
              {statsLoading
                ? 'Checking community interest...'
                : searchVolume !== null
                  ? `${compactMetric(searchVolume)} community search${searchVolume === 1 ? '' : 'es'}`
                  : 'Smart ranked results'}
            </small>
          </div>
        )}

        <div className="store-search-results" aria-live="polite">
          {results.length ? results.slice(0, hasQuery ? 36 : 18).map(({ game, matchLabel }) => {
            const gameDownloads = downloads[game.id] || 0
            const gameLikes = likes[game.id] || 0
            return (
              <button key={game.id} type="button" className="store-search-result" onClick={() => onSelectResult(game)}>
                <div className="store-search-result-media">
                  <LazyGameCardImage
                    game={game}
                    assetId={game.gridAssetId}
                    url={assetUrlForId(game.gridAssetId, assets)}
                    variant="browse"
                    onRequestAsset={onRequestAsset}
                  />
                </div>
                <div className="store-search-result-copy">
                  <strong>{game.title}</strong>
                  <span>{game.developer || game.publisher || '0xoLemon catalog'}</span>
                  <small>{matchLabel}</small>
                </div>
                <div className="store-search-result-meta">
                  {installedGameIds.has(game.id) ? <span className="installed"><CheckCircle2 size={13} /> Installed</span> : null}
                  {gameDownloads > 0 ? <span><Download size={13} /> {compactMetric(gameDownloads)}</span> : null}
                  {gameLikes > 0 ? <span><ThumbsUp size={13} /> {compactMetric(gameLikes)}</span> : null}
                </div>
              </button>
            )
          }) : (
            <div className="store-search-empty">
              <Search size={28} />
              <strong>No matching games</strong>
              <span>Try a title, AppID, publisher, version, or a broader filter.</span>
              {filter !== 'all' ? <button type="button" onClick={() => onFilterChange('all')}>Search all games</button> : null}
            </div>
          )}
        </div>
      </section>
    </div>,
    document.body,
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

// Global tracker to ensure only one hover popup stays open
let activeHoverSetter: ((val: boolean) => void) | null = null

function GameHoverCard({
  game,
  assets,
  onRequestAsset,
  children,
}: {
  game: GameSummary
  assets: Record<string, string>
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  children: ReactElement
}) {
  const [hovered, setHovered] = useState(false)
  const [show, setShow] = useState(false)
  const [pos, setPos] = useState({ top: 0, left: 0, right: 0, alignRight: false })

  useEffect(() => {
    if (!hovered) return
    const timer = setTimeout(() => {
      if (activeHoverSetter && activeHoverSetter !== setShow) {
        activeHoverSetter(false)
      }
      activeHoverSetter = setShow
      setShow(true)
    }, 600)
    return () => clearTimeout(timer)
  }, [hovered])

  const handleMouseEnter = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    const isListMode = rect.width > 300
    const spaceRight = window.innerWidth - rect.right
    const spaceLeft = rect.left
    setPos({
      top: rect.top + rect.height / 2,
      left: isListMode ? rect.left + 220 : rect.right + 10,
      right: window.innerWidth - rect.left + 10,
      alignRight: !isListMode && spaceRight < 340 && spaceLeft > 340,
    })
    setHovered(true)
  }, [])

  return (
    <>
      <div
        className="library-hover-anchor"
        onMouseEnter={handleMouseEnter}
        onMouseLeave={() => {
          setHovered(false)
          setShow(false)
        }}
      >
        {children}
      </div>
      {show && createPortal(<HoverCardPopup game={game} assets={assets} pos={pos} onRequestAsset={onRequestAsset} />, document.body)}
    </>
  )
}

import type { DiscordAuthUser } from '../types'

// StoreModeSwitch Component
function StoreModeSwitch({ value, onChange }: { value: 'local' | 'steam'; onChange: (mode: 'local' | 'steam') => void }) {
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
  uiTheme,
  onOpenLibrary,
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
  uiTheme: UiThemeId
  onOpenLibrary: (gameId: string) => void
}) {
  const { t } = useLocale()
  const {
    acquisitionOnlyStore,
    ownershipLibrary,
    showLibraryRail,
    showSourceSwitches,
  } = getThemeLibraryPresentation(uiTheme, viewMode)
  const [query, setQuery] = useState('')
  const deferredQuery = useDeferredValue(query)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchFilter, setSearchFilter] = useState<StoreSearchFilter>('all')
  const [searchHistory, setSearchHistory] = useState<string[]>(readStoreSearchHistory)
  const [tutorialVisible, setTutorialVisible] = useState(false)
  const [storeMode, setStoreMode] = useState<'local' | 'steam'>(() => {
    const saved = localStorage.getItem('libraryStoreMode')
    return (saved === 'local' || saved === 'steam') ? saved : 'local'
  })
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
  const {
    layout: launcherLibraryLayout,
    libraryGameIds,
    favoriteGameIds: likedGames,
    addGameIds: addLauncherLibraryGameIds,
    removeGameIds: removeLauncherLibraryGameIds,
    setFavorite: setLauncherFavorite,
    saveCollection,
    updateLayout: updateLauncherLibraryLayout,
    persistError: libraryPersistError,
  } = useLauncherLibraryLayout()
  const [activeSteamCollectionId, setActiveSteamCollectionId] = useState('all')
  const [collectionEditorOpen, setCollectionEditorOpen] = useState(false)
  const [collectionName, setCollectionName] = useState('')
  const [collectionDropTarget, setCollectionDropTarget] = useState<string | null>(null)
  const pointerGameDragRef = useRef<{
    gameId: string
    title: string
    source: HTMLElement
    pointerId: number
    startX: number
    startY: number
    lastX: number
    lastY: number
    grabOffsetX: number
    grabOffsetY: number
    cardWidth: number
    cardHeight: number
    active: boolean
    preview: HTMLDivElement | null
    status: HTMLDivElement | null
    target: HTMLElement | null
  } | null>(null)
  const suppressCardClickRef = useRef<string | null>(null)

  const finishPointerGameDrag = useCallback((commit: boolean) => {
    const drag = pointerGameDragRef.current
    if (!drag) return

    if (drag.active) {
      suppressCardClickRef.current = drag.gameId
      window.setTimeout(() => {
        if (suppressCardClickRef.current === drag.gameId) suppressCardClickRef.current = null
      }, 0)
    }

    drag.source.classList.remove('is-pointer-pressed', 'is-pointer-dragging')
    drag.target?.classList.remove('is-drag-over')
    try {
      if (drag.source.hasPointerCapture(drag.pointerId)) drag.source.releasePointerCapture(drag.pointerId)
    } catch {
      // Pointer capture can already be released by pointerup/pointercancel.
    }

    const root = document.documentElement
    root.classList.remove(
      'game-card-pointer-drag-active',
      'game-card-pointer-drag-can-drop',
      'game-card-pointer-drag-cannot-drop',
    )

    const shouldCommit = commit && drag.active && !!drag.target
    if (drag.preview) {
      if (shouldCommit && drag.target && typeof drag.preview.animate === 'function') {
        const targetRect = drag.target.getBoundingClientRect()
        const fromRect = drag.preview.getBoundingClientRect()
        const targetX = targetRect.left + targetRect.width / 2 - fromRect.width / 2
        const targetY = targetRect.top + targetRect.height / 2 - fromRect.height / 2
        drag.preview.animate([
          { opacity: 1, transform: drag.preview.style.transform },
          { opacity: .2, transform: `translate3d(${targetX}px, ${targetY}px, 0) perspective(900px) rotateX(0deg) rotateY(0deg) rotateZ(0deg) scale(.32)` },
        ], { duration: 150, easing: 'cubic-bezier(.2,.75,.2,1)', fill: 'forwards' })
        window.setTimeout(() => drag.preview?.remove(), 160)
      } else {
        drag.preview.remove()
      }
    }

    if (shouldCommit) {
      window.dispatchEvent(new CustomEvent('0xo-add-to-library', {
        detail: { gameId: drag.gameId, title: drag.title },
      }))
    }

    pointerGameDragRef.current = null
  }, [])

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const drag = pointerGameDragRef.current
      if (!drag || event.pointerId !== drag.pointerId) return

      if (!drag.active) {
        const distance = Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY)
        if (distance < 6) return

        drag.active = true
        drag.source.classList.add('is-pointer-dragging')
        const root = document.documentElement
        root.classList.add('game-card-pointer-drag-active', 'game-card-pointer-drag-cannot-drop')

        // Clone the real Store card so the entire game grid card lifts and follows
        // the pointer instead of replacing it with a generic tooltip preview.
        const preview = drag.source.cloneNode(true) as HTMLDivElement
        preview.classList.remove('active', 'is-pointer-pressed', 'is-pointer-dragging')
        preview.classList.add('game-card-pointer-drag-ghost', 'cannot-drop')
        preview.setAttribute('aria-hidden', 'true')
        preview.setAttribute('role', 'presentation')
        preview.tabIndex = -1
        preview.style.width = `${drag.cardWidth}px`
        preview.style.height = `${drag.cardHeight}px`
        preview.querySelectorAll<HTMLElement>('button, a, input, select, textarea, [tabindex]').forEach((node) => {
          node.tabIndex = -1
        })

        const status = document.createElement('div')
        status.className = 'game-card-pointer-drag-status'
        status.innerHTML = '<span aria-hidden="true">×</span><strong>Library only</strong>'
        preview.appendChild(status)
        document.body.appendChild(preview)
        drag.preview = preview
        drag.status = status
      }

      event.preventDefault()

      const element = document.elementFromPoint(event.clientX, event.clientY)
      const nextTarget = element?.closest<HTMLElement>('[data-library-drop-target="true"]') ?? null
      if (nextTarget !== drag.target) {
        drag.target?.classList.remove('is-drag-over')
        nextTarget?.classList.add('is-drag-over')
        drag.target = nextTarget
      }

      const canDrop = !!nextTarget
      const root = document.documentElement
      root.classList.toggle('game-card-pointer-drag-can-drop', canDrop)
      root.classList.toggle('game-card-pointer-drag-cannot-drop', !canDrop)

      if (drag.preview) {
        drag.preview.classList.toggle('can-drop', canDrop)
        drag.preview.classList.toggle('cannot-drop', !canDrop)
        if (drag.status) {
          drag.status.innerHTML = canDrop
            ? '<span aria-hidden="true">+</span><strong>Drop to Library</strong>'
            : '<span aria-hidden="true">×</span><strong>Library only</strong>'
        }

        const velocityX = event.clientX - drag.lastX
        const velocityY = event.clientY - drag.lastY
        const tiltY = Math.max(-7, Math.min(7, velocityX * .85))
        const tiltX = Math.max(-5, Math.min(5, -velocityY * .6))
        const tiltZ = Math.max(-2.2, Math.min(2.2, velocityX * .2))
        const x = event.clientX - drag.grabOffsetX
        const y = event.clientY - drag.grabOffsetY
        drag.preview.style.transform = `translate3d(${x}px, ${y}px, 0) perspective(900px) rotateX(${tiltX}deg) rotateY(${tiltY}deg) rotateZ(${tiltZ}deg) scale(1.035)`
      }

      drag.lastX = event.clientX
      drag.lastY = event.clientY
    }

    const handlePointerUp = (event: PointerEvent) => {
      const drag = pointerGameDragRef.current
      if (!drag || event.pointerId !== drag.pointerId) return
      finishPointerGameDrag(true)
    }
    const handlePointerCancel = (event: PointerEvent) => {
      const drag = pointerGameDragRef.current
      if (!drag || event.pointerId !== drag.pointerId) return
      finishPointerGameDrag(false)
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') finishPointerGameDrag(false)
    }

    window.addEventListener('pointermove', handlePointerMove, { passive: false })
    window.addEventListener('pointerup', handlePointerUp)
    window.addEventListener('pointercancel', handlePointerCancel)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', handlePointerUp)
      window.removeEventListener('pointercancel', handlePointerCancel)
      window.removeEventListener('keydown', handleKeyDown)
      finishPointerGameDrag(false)
    }
  }, [finishPointerGameDrag])

  const beginPointerGameDrag = useCallback((event: ReactPointerEvent<HTMLElement>, game: GameSummary) => {
    if (viewMode !== 'store' || event.button !== 0 || event.pointerType === 'touch') return
    const target = event.target as Element
    if (target.closest('button, a, input, select, textarea')) return

    const source = event.currentTarget
    const rect = source.getBoundingClientRect()
    source.classList.add('is-pointer-pressed')
    try {
      source.setPointerCapture(event.pointerId)
    } catch {
      // Window-level pointer listeners still keep the drag functional.
    }

    pointerGameDragRef.current = {
      gameId: game.id,
      title: game.title,
      source,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      lastX: event.clientX,
      lastY: event.clientY,
      grabOffsetX: event.clientX - rect.left,
      grabOffsetY: event.clientY - rect.top,
      cardWidth: rect.width,
      cardHeight: rect.height,
      active: false,
      preview: null,
      status: null,
      target: null,
    }
  }, [viewMode])

  const addToLauncherLibrary = useCallback((gameId: string) => {
    addLauncherLibraryGameIds([gameId])
    window.dispatchEvent(new CustomEvent('0xo-add-to-library', { detail: { gameId } }))
  }, [addLauncherLibraryGameIds])

  const removeFromLauncherLibrary = useCallback((gameId: string) => {
    removeLauncherLibraryGameIds([gameId])
    window.dispatchEvent(new CustomEvent('0xo-remove-from-library', { detail: { gameId } }))
  }, [removeLauncherLibraryGameIds])

  const createSteamCollection = useCallback(() => {
    const name = collectionName.trim()
    if (!name) return
    const now = new Date().toISOString()
    const id = `collection-${crypto.randomUUID()}`
    saveCollection({ id, name, gameIds: [], createdAt: now, updatedAt: now })
    setActiveSteamCollectionId(id)
    setCollectionName('')
    setCollectionEditorOpen(false)
  }, [collectionName, saveCollection])

  const removeSteamCollection = useCallback((collectionId: string) => {
    updateLauncherLibraryLayout((current) => ({
      ...current,
      collections: current.collections.filter((collection) => collection.id !== collectionId),
      shelves: current.shelves.filter((shelf) => shelf.collectionId !== collectionId),
    }))
    setActiveSteamCollectionId((current) => current === collectionId ? 'all' : current)
  }, [updateLauncherLibraryLayout])

  const addGameToSteamCollection = useCallback((collectionId: string, gameId: string) => {
    const collection = launcherLibraryLayout.collections.find((entry) => entry.id === collectionId)
    if (!collection || collection.gameIds.includes(gameId)) return
    saveCollection({
      ...collection,
      gameIds: [...collection.gameIds, gameId],
      updatedAt: new Date().toISOString(),
    })
  }, [launcherLibraryLayout.collections, saveCollection])

  useEffect(() => {
    if (!installStates) return
    const installedIds = Object.entries(installStates)
      .filter(([, state]) => state?.installed)
      .map(([gameId]) => gameId)
    if (installedIds.length === 0) return
    addLauncherLibraryGameIds(installedIds)
  }, [addLauncherLibraryGameIds, installStates])
  const [sortOpen, setSortOpen] = useState(false)
  const realtimeConfig = useRealtimeConfig()
  const gameStats = useGameStats()
  const {
    stats: searchStats,
    loading: searchStatsLoading,
    recordSearch,
    recordResultClick,
  } = useStoreSearchTelemetry(searchOpen, deferredQuery)

  const rememberSearch = useCallback((term: string) => {
    const normalized = normalizeStoreSearchTerm(term)
    if (normalized.length < 2) return
    setSearchHistory((current) => {
      const next = [term.trim(), ...current.filter((entry) => normalizeStoreSearchTerm(entry) !== normalized)].slice(0, 12)
      localStorage.setItem(STORE_SEARCH_HISTORY_KEY, JSON.stringify(next))
      return next
    })
  }, [])

  const clearSearchHistory = useCallback(() => {
    localStorage.removeItem(STORE_SEARCH_HISTORY_KEY)
    setSearchHistory([])
  }, [])

  const closeSearch = useCallback(() => setSearchOpen(false), [])

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
    setLauncherFavorite(gameId, !isLiked)
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
      setLauncherFavorite(gameId, isLiked)
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
    const timer = window.setTimeout(() => {
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
    }, 0)
    return () => window.clearTimeout(timer)
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

  const browseGames = useMemo(() => {
    let baseGames = catalog.games

    if (ownershipLibrary) {
      baseGames = baseGames.filter((game) => libraryGameIds.has(game.id) || Boolean(installStates?.[game.id]?.installed))
    } else if (viewMode === 'library') {
      baseGames = baseGames.filter((game) => {
        if (libraryMode === 'local') {
          return libraryGameIds.has(game.id) || Boolean(installStates?.[game.id]?.installed)
        } else if (libraryMode === 'steam') {
          const appid = mapping[game.id]
          return Boolean(appid && steamInstalledAppIds?.includes(appid))
        }
        return false
      })
    }

    return baseGames
  }, [catalog.games, viewMode, ownershipLibrary, libraryGameIds, libraryMode, installStates, steamInstalledAppIds, mapping])

  const rankedSearchResults = useMemo(() => rankStoreGames({
    games: browseGames,
    query: deferredQuery,
    filter: searchFilter,
    installStates,
    steamInstalledAppIds,
    steamMapping: mapping,
    downloads: gameStats.downloads,
    likes: gameStats.likes,
    searchClicks: searchStats.gameClicks,
  }), [
    browseGames,
    deferredQuery,
    searchFilter,
    installStates,
    steamInstalledAppIds,
    mapping,
    gameStats.downloads,
    gameStats.likes,
    searchStats.gameClicks,
  ])

  const visibleGames = useMemo(() => {
    if (normalizeStoreSearchTerm(deferredQuery) || searchFilter !== 'all') {
      return rankedSearchResults.map((result) => result.game)
    }

    // Apply sort
    const sorted = [...browseGames]
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
  }, [browseGames, catalog.games, deferredQuery, searchFilter, rankedSearchResults, sortBy, gameStats.downloads, gameStats.likes])

  const installedGameIds = useMemo(() => new Set(catalog.games.flatMap((game) => {
    const mappedAppId = mapping[game.id]
    const installedLocally = installStates?.[game.id]?.installed
    const installedOnSteam = Boolean(mappedAppId && steamInstalledAppIds?.includes(mappedAppId))
    return installedLocally || installedOnSteam ? [game.id] : []
  })), [catalog.games, installStates, mapping, steamInstalledAppIds])

  useEffect(() => {
    if (!searchOpen || normalizeStoreSearchTerm(deferredQuery).length < 2) return
    const timer = window.setTimeout(() => {
      rememberSearch(deferredQuery)
      recordSearch('stable-query', deferredQuery, rankedSearchResults.length)
    }, 1_100)
    return () => window.clearTimeout(timer)
  }, [deferredQuery, rankedSearchResults.length, recordSearch, rememberSearch, searchOpen])

  useEffect(() => {
    if (selectedGame) return
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setSearchOpen(true)
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [selectedGame])

  useEffect(() => {
    const timer = window.setTimeout(() => setCurrentPage(1), 0)
    return () => window.clearTimeout(timer)
  }, [deferredQuery, searchFilter, sortBy, viewLayout, viewMode, libraryMode, gridCols])

  const itemsPerPage = viewLayout === 'list' ? 70 : 50
  const paginatedGames = visibleGames.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage)
  const totalPages = Math.ceil(visibleGames.length / itemsPerPage)
  const newestGameIdsForSash = catalog.newestGameIds || (catalog.games.length > 0 ? [catalog.games[catalog.games.length - 1].id] : [])

  const actionDockRef = useRef<HTMLDivElement>(null)
  const [stickyVisible, setStickyVisible] = useState(false)
  const [activeDetailTab, setActiveDetailTab] = useState<'overview' | 'chat' | 'lua-game'>('overview')
  const [showLuaGameTab, setShowLuaGameTab] = useState(false)
  const [steamGameRunning, setSteamGameRunning] = useState(false)
  const [luaSyncing, setLuaSyncing] = useState(false)

  // Get current game's Steam App ID
  const currentSteamAppId = selectedGame ? mapping[selectedGame.id] : undefined

  // Check for Lua manifest updates
  const { updateInfo, checking: luaStateChecking, refresh: refreshLuaState } = useLuaUpdateCheck(currentSteamAppId, showLuaGameTab)

  const syncCurrentLuaGame = useCallback(async () => {
    if (!currentSteamAppId) return
    setLuaSyncing(true)
    try {
      await invoke('check_lua_game_update', { appid: currentSteamAppId })
      await refreshLuaState()
    } catch (error) {
      const message = String(error)
      if (message) {
        window.dispatchEvent(new CustomEvent('0xo-toast', {
          detail: {
            category: 'launcher',
            severity: 'error',
            title: t.luaShop.liveChannel,
            message,
            dedupeKey: `lua-sync:${currentSteamAppId}`,
          },
        }))
      }
    } finally {
      setLuaSyncing(false)
    }
  }, [currentSteamAppId, refreshLuaState, t.luaShop])

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

    window.addEventListener('lua-game-mode-changed', handleLuaGameModeChange)

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
      window.removeEventListener('lua-game-mode-changed', handleLuaGameModeChange)
    }
  }, [selectedGame, activeDetailTab, mapping])

  useEffect(() => {
    // Poll to check if the Steam game process is running
    const executable = detail?.install?.launchExecutable
    const appid = mapping[selectedGame?.id || '']
    const isInstalledOnSteam = Boolean(appid && steamInstalledAppIds?.includes(appid))
    const effectiveMode = viewMode === 'library' ? libraryMode : storeMode

    if (effectiveMode !== 'steam' || !isInstalledOnSteam || !executable) {
      const timer = window.setTimeout(() => setSteamGameRunning(false), 0)
      return () => window.clearTimeout(timer)
    }

    const interval = setInterval(async () => {
      try {
        const running = await invoke<boolean>('is_process_running', { executable })
        setSteamGameRunning(running)
      } catch {
        // ignore
      }
    }, 3000)

    return () => clearInterval(interval)
  }, [detail, mapping, selectedGame, steamInstalledAppIds, libraryMode, storeMode, viewMode])

  const [steamlessStatus, setSteamlessStatus] = useState<boolean>(false)
  const [steamlessLoading, setSteamlessLoading] = useState<boolean>(false)
  const [steamlessMessage, setSteamlessMessage] = useState<{ text: string; isError: boolean } | null>(null)

  /** Resolve the exe path: prefer Steam's own install dir over launcher's installPath */
  const resolveSteamlessExePath = useCallback(async (): Promise<string | null> => {
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
  }, [mapping, selectedGame, selectedInstallState])

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
  }, [activeDetailTab, resolveSteamlessExePath, selectedGame, selectedInstallState])

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
        const res = await invoke<{ success: boolean; message: string }>('steamless_apply', { exePath })
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

  const handleSearchSubmit = useCallback(() => {
    rememberSearch(query)
    recordSearch('submit', query, rankedSearchResults.length)
  }, [query, rankedSearchResults.length, recordSearch, rememberSearch])

  const handleSearchSuggestion = useCallback((term: string) => {
    setQuery(term)
    setSearchFilter('all')
    rememberSearch(term)
  }, [rememberSearch])

  const handleSearchResult = useCallback((game: GameSummary) => {
    const telemetryTerm = query.trim() || game.title
    rememberSearch(telemetryTerm)
    recordResultClick(telemetryTerm, rankedSearchResults.length, game.id)
    setSearchOpen(false)
    onSelectGame(game.id)
  }, [onSelectGame, query, rankedSearchResults.length, recordResultClick, rememberSearch])

  const renderGameCard = (game: GameSummary, variant: 'compact' | 'browse') => {
    const tags = getGameTags(game)
    const isComingSoon = gameHasTag(game, 'coming soon')
    const inWishlist = wishlist.has(game.id)
    const downloads = gameStats.downloads[game.id] || 0
    const likes = gameStats.likes[game.id] || 0

    return (
      <div
        className={[
          'store-game-card',
          variant === 'browse' ? 'browse-game-card' : '',
          game.id === selectedGameId ? 'active' : '',
          isComingSoon ? 'coming-soon' : '',
        ]
          .filter(Boolean)
          .join(' ')}
        key={game.id}
        role="button"
        tabIndex={isComingSoon ? -1 : 0}
        draggable={false}
        onPointerDown={(event) => {
          if (!isComingSoon) beginPointerGameDrag(event, game)
        }}
        onClick={() => {
          if (suppressCardClickRef.current === game.id) {
            suppressCardClickRef.current = null
            return
          }
          if (!isComingSoon) onSelectGame(game.id)
        }}
        onKeyDown={(event) => {
          if (!isComingSoon && (event.key === 'Enter' || event.key === ' ')) {
            event.preventDefault()
            onSelectGame(game.id)
          }
        }}
      >
        <div className="store-game-card-media">
          <LazyGameCardImage
            game={game}
            assetId={variant === 'browse' && viewLayout === 'list' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId}
            url={assetUrlForId(variant === 'browse' && viewLayout === 'list' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId, assets)}
            variant={variant}
            onRequestAsset={onRequestAsset}
          />
          {newestGameIdsForSash.includes(game.id) ? (
            <div className="game-card-new-sash">NEW!</div>
          ) : tags.some(t => t.id === 'demo bypass' || t.tone === 'demo') ? (
            <div className="game-card-demo-sash">DEMO</div>
          ) : tags.some(t => t.tone === 'ubisoft' || t.tone === 'ubisoft game' || t.id === 'ubisoft' || t.id === 'ubisoft game') ? (
            <div className="game-card-ubisoft-sash">Ubisoft</div>
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
      </div>

    )
  }

  const renderSteamLibraryRail = (detailMode = false) => {
    if (!showLibraryRail) return null
    const activeCollection = launcherLibraryLayout.collections.find((collection) => collection.id === activeSteamCollectionId)
    const filteredRailGames = activeSteamCollectionId === 'favorites'
      ? visibleGames.filter((game) => likedGames.has(game.id))
      : activeSteamCollectionId === 'installed'
        ? visibleGames.filter((game) => Boolean(installStates?.[game.id]?.installed))
        : activeCollection
          ? visibleGames.filter((game) => activeCollection.gameIds.includes(game.id))
          : visibleGames
    const railGames = filteredRailGames.slice(0, 180)

    return (
      <aside className={detailMode ? 'steam-library-detail-rail' : 'steam-library-rail'} aria-label="Library games">
        <button
          type="button"
          className={`steam-library-home-button${selectedGame ? '' : ' is-active'}`}
          onClick={() => onSelectGame(null)}
        >
          <Library size={15} />
          <span>Home</span>
          <small>{browseGames.length}</small>
        </button>
        <label className="steam-library-search">
          <Search size={14} />
          <input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search library"
            aria-label="Search library"
          />
          <kbd>Ctrl K</kbd>
        </label>
        <div className="steam-library-rail-heading steam-library-collections-heading">
          <span>Collections</span>
          <button
            type="button"
            className="steam-library-collection-add"
            onClick={() => setCollectionEditorOpen((open) => !open)}
            title="Create collection"
            aria-label="Create collection"
            aria-expanded={collectionEditorOpen}
          >
            <PlusCircle size={13} />
          </button>
        </div>
        {collectionEditorOpen ? (
          <form
            className="steam-library-collection-editor"
            onSubmit={(event) => { event.preventDefault(); createSteamCollection() }}
          >
            <input
              autoFocus
              value={collectionName}
              onChange={(event) => setCollectionName(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  setCollectionEditorOpen(false)
                  setCollectionName('')
                }
              }}
              maxLength={48}
              placeholder="Collection name"
              aria-label="Collection name"
            />
            <button type="submit" disabled={!collectionName.trim()} aria-label="Save collection">+</button>
          </form>
        ) : null}
        <div className="steam-library-collection-list" aria-label="Library collections">
          {[
            { id: 'all', name: 'All games', count: visibleGames.length },
            { id: 'favorites', name: 'Favorites', count: visibleGames.filter((game) => likedGames.has(game.id)).length },
            { id: 'installed', name: 'Installed', count: visibleGames.filter((game) => Boolean(installStates?.[game.id]?.installed)).length },
          ].map((collection) => (
            <button
              key={collection.id}
              type="button"
              className={`steam-library-collection-row${activeSteamCollectionId === collection.id ? ' is-active' : ''}`}
              onClick={() => setActiveSteamCollectionId(collection.id)}
            >
              <span>{collection.name}</span><small>{collection.count}</small>
            </button>
          ))}
          {launcherLibraryLayout.collections.map((collection) => (
            <div
              key={collection.id}
              className={`steam-library-custom-collection${collectionDropTarget === collection.id ? ' is-drop-target' : ''}`}
              onDragOver={(event) => { event.preventDefault(); setCollectionDropTarget(collection.id) }}
              onDragLeave={() => setCollectionDropTarget((current) => current === collection.id ? null : current)}
              onDrop={(event) => {
                event.preventDefault()
                setCollectionDropTarget(null)
                const gameId = event.dataTransfer.getData('application/x-0xolemon-game-id')
                if (gameId) addGameToSteamCollection(collection.id, gameId)
              }}
            >
              <button
                type="button"
                className={`steam-library-collection-row${activeSteamCollectionId === collection.id ? ' is-active' : ''}`}
                onClick={() => setActiveSteamCollectionId(collection.id)}
                title="Drop a game here to add it"
              >
                <span>{collection.name}</span><small>{collection.gameIds.length}</small>
              </button>
              <button
                type="button"
                className="steam-library-collection-remove"
                onClick={() => removeSteamCollection(collection.id)}
                title="Delete collection"
                aria-label={`Delete ${collection.name}`}
              >
                <X size={11} />
              </button>
            </div>
          ))}
        </div>
        <div className="steam-library-rail-heading">
          <span>Games</span>
          <small>{railGames.length}</small>
        </div>
        <div className="steam-library-game-list">
          {railGames.map((game) => {
            const railIcon = assetUrlForId(game.iconAssetId, assets) || assetUrlForId(game.gridAssetId, assets)
            const isActive = selectedGame?.id === game.id
            return (
              <button
                type="button"
                key={game.id}
                draggable
                className={`steam-library-game-row${isActive ? ' is-active' : ''}`}
                onClick={() => onSelectGame(game.id)}
                onMouseEnter={() => onRequestAsset(game, game.iconAssetId || game.gridAssetId)}
                onDragStart={(event) => {
                  event.dataTransfer.effectAllowed = 'copy'
                  event.dataTransfer.setData('application/x-0xolemon-game-id', game.id)
                }}
                title={game.title}
              >
                <span className="steam-library-game-icon">
                  {railIcon ? <img src={railIcon} alt="" loading="lazy" decoding="async" /> : <Library size={13} />}
                </span>
                <span className="steam-library-game-name">{game.title}</span>
                {installStates?.[game.id]?.installed ? <span className="steam-library-installed-dot" aria-label="Installed" /> : null}
              </button>
            )
          })}
        </div>
      </aside>
    )
  }

  if (!selectedGame) {
    if (catalogLoadState === 'loading' && catalog.games.length === 0) {
      return <CatalogLoadingView viewMode={viewMode} />
    }

    if (catalogLoadState === 'error' && catalog.games.length === 0) {
      return <CatalogUnavailableView viewMode={viewMode} onRetry={onRetryCatalog} />
    }

    if (showLibraryRail) {
      return (
        <section className="library-browse-view has-steam-library-rail steam-library-home-view">
          {renderSteamLibraryRail(false)}
          <SteamLibraryHome
            games={visibleGames}
            assets={assets}
            installStates={installStates}
            favoriteGameIds={likedGames}
            onSelectGame={(gameId) => onSelectGame(gameId)}
            onCustomizeShelves={() => setCollectionEditorOpen(true)}
            onRequestAsset={onRequestAsset}
          />
        </section>
      )
    }

    if (uiTheme === 'steam' && viewMode === 'store') {
      return (
        <SteamStoreHome
          games={browseGames}
          assets={assets}
          ownedGameIds={libraryGameIds}
          wishlistGameIds={wishlist}
          onSelectGame={(gameId) => onSelectGame(gameId)}
          onRequestAsset={onRequestAsset}
        />
      )
    }

    return (
      <section className={`library-browse-view${showLibraryRail ? ' has-steam-library-rail' : ''}`}>
        {renderSteamLibraryRail(false)}
        <header className="library-browse-toolbar">
          <div className="library-browse-heading">
            <strong>{viewMode === 'store' ? t.nav.store : t.nav.library}</strong>
            <span>
              {visibleGames.length} game{visibleGames.length === 1 ? '' : 's'}
            </span>
          </div>
          {viewMode === 'store' && showSourceSwitches && <StoreModeSwitch value={storeMode} onChange={(v) => { setStoreMode(v); localStorage.setItem('libraryStoreMode', v) }} />}
          {viewMode === 'library' && showSourceSwitches && <LibraryModeSwitch value={libraryMode} onChange={(v) => { setLibraryMode(v); localStorage.setItem('libraryMode', v) }} />}

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

            <label className="store-search" data-open={searchOpen ? 'true' : 'false'}>
              <Search size={16} />
              <input
                aria-label="Search games"
                aria-expanded={searchOpen}
                value={query}
                onFocus={() => setSearchOpen(true)}
                onClick={() => setSearchOpen(true)}
                onChange={(event) => {
                  setQuery(event.target.value)
                  setSearchOpen(true)
                }}
                placeholder="Search games..."
              />
              <kbd>Ctrl K</kbd>
            </label>
          </div>
        </header>
        <StoreSearchOverlay
          open={searchOpen}
          query={query}
          filter={searchFilter}
          results={rankedSearchResults}
          history={searchHistory}
          trending={searchStats.trending}
          searchVolume={searchStats.query?.searches ?? null}
          statsLoading={searchStatsLoading}
          assets={assets}
          downloads={gameStats.downloads}
          likes={gameStats.likes}
          installedGameIds={installedGameIds}
          onQueryChange={setQuery}
          onFilterChange={setSearchFilter}
          onClose={closeSearch}
          onSubmit={handleSearchSubmit}
          onSuggestion={handleSearchSuggestion}
          onClearHistory={clearSearchHistory}
          onSelectResult={handleSearchResult}
          onRequestAsset={onRequestAsset}
        />
        {visibleGames.length > 0 ? (
          <>
            <div className={`library-browse-grid layout-${viewLayout} ${viewLayout === 'grid' ? `grid-cols-${gridCols}` : ''}`}>
              {paginatedGames.map((game) => (
                <GameHoverCard key={game.id} game={game} assets={assets} onRequestAsset={onRequestAsset}>
                  {renderGameCard(game, 'browse') as ReactElement}
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
                <strong>{t.library.noGamesInLibrary || 'No games in Library'}</strong>
                <span>{t.library.noGamesInLibraryDesc || 'Add games from Store to see them here.'}</span>
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
  const inLauncherLibrary = libraryGameIds.has(selectedGame.id) || installed
  const installBlocked = ['recovering', 'conflict', 'unavailable'].includes(selectedInstallState?.discoveryStatus ?? '')
  const isVerifying = verifyStatus?.state === 'running'

  // Determine effective mode to control button visibility
  const effectiveMode = viewMode === 'library' ? libraryMode : storeMode

  const steamAppId = mapping[selectedGame.id]
  const isInstalledOnSteam = Boolean(steamAppId && steamInstalledAppIds?.includes(steamAppId))
  const steamBuildId = steamAppId ? steamBuildIds?.[steamAppId] : undefined

  const isDownloading = isJobRunning
  const isPlaying = isGameRunning || steamGameRunning
  const discoveryStatus = selectedInstallState?.discoveryStatus ?? ''

  let actionLabel: string = installed ? t.library.play : (!isTauriRuntime() ? 'Remote Install' : t.library.chooseInstall)
  let actionClass = 'primary-control'
  let primaryDisabled = false
  const stateLabel = acquisitionOnlyStore
    ? (inLauncherLibrary ? 'In Library' : 'Available to add')
    : discoveryStatus === 'recovering'
      ? t.installRecovery.checking
      : installBlocked
        ? selectedInstallState?.unavailableReason || t.installRecovery.libraryUnavailable
        : !installed
          ? t.library.readyToInstall
          : updateReady
            ? t.library.readyToUpdate
            : t.library.readyToPlay

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
  if (installBlocked && effectiveMode !== 'steam') {
    actionLabel = discoveryStatus === 'recovering'
      ? t.installRecovery.checking
      : discoveryStatus === 'conflict'
        ? t.installRecovery.resolveLocation
        : t.installRecovery.libraryUnavailable
    primaryDisabled = true
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
  const updateDisabled = installBlocked || !canUpdate || isDownloading || isPlaying
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
  // local: show play/install buttons
  // steam: hide install/play buttons completely, show only "Add to Steam"
  const showInstallButton = effectiveMode === 'local' && !acquisitionOnlyStore
  const showSteamButton = effectiveMode === 'steam' && !acquisitionOnlyStore

  if (showLibraryRail) {
    const libraryPrimaryAction = isPlaying
      ? onStop
      : updateReady
        ? onPrimaryAction
        : installed
          ? onPlay
          : onOpenInstallOptions

    return (
      <section className="game-detail-view has-steam-library-rail steam-library-game-detail steam-library-dedicated-detail">
        {renderSteamLibraryRail(true)}
        <SteamLibraryDetail
          game={selectedGame}
          detail={detail}
          assets={assets}
          installState={selectedInstallState}
          displayedVersion={displayedVersion}
          heroUrl={hero}
          logoUrl={logo}
          coverUrl={gridAsset}
          downloadSize={downloadSize}
          installed={installed}
          updateReady={updateReady}
          installing={isDownloading}
          playing={isPlaying}
          verifying={isVerifying}
          installBlocked={installBlocked}
          favorite={likedGames.has(selectedGame.id)}
          showVersionAction={showVersionAction}
          verifyStatus={verifyStatus}
          onInstall={onOpenInstallOptions}
          onPlay={onPlay}
          onStop={onStop}
          onUpdate={libraryPrimaryAction}
          onVersions={onPrimaryAction}
          onVerify={onVerify}
          onBrowse={() => selectedInstallState?.installPath && void invoke('open_folder', { path: selectedInstallState.installPath })}
          onUninstall={onUninstall}
          onToggleFavorite={() => setLauncherFavorite(selectedGame.id, !likedGames.has(selectedGame.id))}
          onOpenStore={onOpenStore}
        />
      </section>
    )
  }

  return (
    <section className={`game-detail-view${showLibraryRail ? ' has-steam-library-rail steam-library-game-detail' : ''}`}>
      {renderSteamLibraryRail(true)}
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
          {(!acquisitionOnlyStore && installed && effectiveMode !== 'steam' && selectedGame?.id.includes('among')) && (
            <button type="button" onClick={() => setTutorialVisible(true)}>
              <BookOpen size={15} />
              Tutorial
            </button>
          )}
          {(!acquisitionOnlyStore && installed && effectiveMode !== 'steam') && (
            <button type="button" disabled={installBlocked} onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
              <FolderOpen size={15} />
              Browse
            </button>
          )}
          {!acquisitionOnlyStore && effectiveMode !== 'steam' && (
            <button type="button" onClick={onVerify} disabled={!installed || isVerifying || installBlocked}>
              <VerifyIcon size={15} />
              {verifyLabel}
            </button>
          )}
          {acquisitionOnlyStore ? (
            <button
              className="primary-control steam-add-library-control"
              data-library-state={inLauncherLibrary ? 'owned' : 'available'}
              type="button"
              onClick={() => inLauncherLibrary ? onOpenLibrary(selectedGame.id) : addToLauncherLibrary(selectedGame.id)}
            >
              <Library size={15} />
              <span>{inLauncherLibrary ? 'View in Library' : 'Add to Library'}</span>
            </button>
          ) : null}
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
          {(effectiveMode === 'steam') && isInstalledOnSteam && showLuaGameTab && (
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
          {(!acquisitionOnlyStore && installed && effectiveMode !== 'steam') ? (
            <button className="danger-control" type="button" onClick={onUninstall} disabled={installBlocked}>
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
            {showLibraryRail && gridAsset ? (
              <img className="steam-detail-cover" src={gridAsset} alt="" loading="eager" decoding="async" />
            ) : null}
            <div className="detail-copy-main">
            <span className="storage-pill">
              <HardDrive size={14} />
              {detail.install?.storageLabel || 'HDD'}
            </span>
            {logo ? <img className="detail-logo" src={logo} alt={detail.title} /> : <h1>{detail.title}</h1>}
            <p>{detail.shortDescription}</p>
            <div className="library-meta-row">
              <span>{effectiveMode === 'steam' ? (isInstalledOnSteam ? `Steam Build ${steamBuildId || 'Unknown'}` : 'Not Installed on Steam') : `Version ${displayedVersion}`}</span>
              {!acquisitionOnlyStore && effectiveMode !== 'steam' && <span>{formatBytes(downloadSize)}</span>}
              {!acquisitionOnlyStore && effectiveMode !== 'steam' && detail.install?.supportsResume ? <span>{t.library.resumeSupported}</span> : null}
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
              {viewMode === 'store' && (
                !inLauncherLibrary ? (
                  <button
                    type="button"
                    className="detail-add-library-btn"
                    onClick={() => addToLauncherLibrary(selectedGame.id)}
                    title={t.library.addToLibrary}
                  >
                    <PlusCircle size={15} />
                    <span>{t.library.addToLibrary}</span>
                  </button>
                ) : (
                  <div className="detail-library-group" style={{ display: 'inline-flex', alignItems: 'center', gap: '6px' }}>
                    <button
                      type="button"
                      className="detail-add-library-btn is-in-library"
                      onClick={() => onOpenLibrary(selectedGame.id)}
                      title={t.library.inLibrary}
                    >
                      <CheckCircle2 size={15} />
                      <span>{t.library.inLibrary}</span>
                    </button>
                    <button
                      type="button"
                      className="detail-action-icon-btn remove-library-btn"
                      onClick={() => removeFromLauncherLibrary(selectedGame.id)}
                      title={t.library.removeFromLibrary}
                      aria-label={t.library.removeFromLibrary}
                    >
                      <Trash2 size={14} />
                      <span>{t.library.removeFromLibrary}</span>
                    </button>
                  </div>
                )
              )}
              {viewMode === 'library' && inLauncherLibrary && (
                <button
                  type="button"
                  className="detail-action-icon-btn remove-library-btn"
                  onClick={() => {
                    removeFromLauncherLibrary(selectedGame.id)
                    onSelectGame(null)
                  }}
                  title={t.library.removeFromLibrary}
                  aria-label={t.library.removeFromLibrary}
                >
                  <Trash2 size={14} />
                  <span>{t.library.removeFromLibrary}</span>
                </button>
              )}
            </div>
            </div>
            {showLibraryRail ? (
              <dl className="steam-detail-facts">
                <div><dt>Developer</dt><dd>{detail.developers.join(', ') || selectedGame.developer}</dd></div>
                <div><dt>Publisher</dt><dd>{detail.publishers.join(', ') || selectedGame.publisher}</dd></div>
                <div><dt>Release date</dt><dd>{detail.releaseDate || 'Available now'}</dd></div>
                <div><dt>Features</dt><dd>{detail.categories.slice(0, 3).join(' · ') || 'Single-player'}</dd></div>
              </dl>
            ) : null}
          </div>
          {viewMode !== 'store' ? (
          <div className="store-action-dock" ref={actionDockRef}>
            {(installed && effectiveMode !== 'steam' && (selectedGame?.id.includes('among') || selectedGame?.id === 'persona-3-reload')) && (
              <button type="button" onClick={() => setTutorialVisible(true)}>
                <BookOpen size={15} />
                Tutorial
              </button>
            )}
            {(!acquisitionOnlyStore && installed && effectiveMode !== 'steam') && (
              <button type="button" disabled={installBlocked} onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
                <FolderOpen size={15} />
                Browse
              </button>
            )}
            {!acquisitionOnlyStore && effectiveMode !== 'steam' && (
              <button type="button" onClick={onVerify} disabled={!installed || isVerifying || installBlocked}>
                <VerifyIcon size={17} />
                {verifyLabel}
              </button>
            )}
            {viewMode === 'library' && inLauncherLibrary && !installed && (
              <button
                type="button"
                className="library-action-remove-btn"
                onClick={() => {
                  removeFromLauncherLibrary(selectedGame.id)
                  onSelectGame(null)
                }}
                title={t.library.removeFromLibrary}
              >
                <Trash2 size={16} />
                <span>{t.library.removeFromLibrary}</span>
              </button>
            )}
            {acquisitionOnlyStore ? (
              <button
                className="primary-control steam-add-library-control"
                data-library-state={inLauncherLibrary ? 'owned' : 'available'}
                type="button"
                onClick={() => inLauncherLibrary ? onOpenLibrary(selectedGame.id) : addToLauncherLibrary(selectedGame.id)}
              >
                <Library size={17} />
                <span>{inLauncherLibrary ? 'View in Library' : 'Add to Library'}</span>
              </button>
            ) : null}
            {acquisitionOnlyStore && libraryPersistError ? (
              <span className="library-persist-error" role="alert">{t.library.libraryPersistFailed}</span>
            ) : null}
            {showInstallButton && (
              <button
                className={actionClass}
                type="button"
                onClick={primaryActionBtn}
                disabled={primaryDisabled}
                data-steam-action={isPlaying ? 'stop' : installed ? 'play' : 'install'}
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
                data-steam-action={isPlaying ? 'stop' : 'play'}
                data-stop-label={isPlaying ? 'STOP' : undefined}
              >
                {isPlaying ? <Play size={17} /> : <Play size={17} />}
                <span>{isPlaying ? 'Running' : t.library.play}</span>
              </button>
            )}

            {selectedGameId && (
              <SaveBackupIndicator gameId={selectedGameId} />
            )}

            {(!acquisitionOnlyStore && showSteamButton && !isJobRunning && !isPlaying && selectedGameId) && (
              <SteamIntegrationButton gameId={selectedGameId} gameTitle={detail.title} storeMode={effectiveMode} />
            )}

            {/* Steam Versions button — shown in steam OR hybrid mode */}
            {(effectiveMode === 'steam') && isInstalledOnSteam && showLuaGameTab && (
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
            
            {(!acquisitionOnlyStore && installed && effectiveMode !== 'steam') ? (
              <button className="danger-control" type="button" onClick={onUninstall} disabled={installBlocked}>
                <X size={17} />
                {t.library.uninstall}
              </button>
            ) : null}
          </div>
          ) : null}
        </div>

        <nav className={`detail-tabs${showLibraryRail ? ' steam-detail-subnav' : ''}`}>
          <button
            className={activeDetailTab === 'overview' ? 'active' : ''}
            onClick={() => setActiveDetailTab('overview')}
            type="button"
          >
            <Info size={16} /> {showLibraryRail ? 'Activity' : 'Overview'}
          </button>
          <button
            className={activeDetailTab === 'chat' ? 'active' : ''}
            onClick={() => setActiveDetailTab('chat')}
            type="button"
          >
            <MessageSquare size={16} /> Live Chat
          </button>
        </nav>

        {activeDetailTab === 'overview' ? (
          showLibraryRail ? (
            <SteamLibraryActivity
              game={selectedGame}
              detail={detail}
              assets={assets}
              installState={selectedInstallState}
              displayedVersion={displayedVersion}
            />
          ) : (
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
          )
        ) : activeDetailTab === 'lua-game' ? (
          <section className="detail-body lua-game-tab-container">
            {updateInfo && (
              <div className={`lua-live-status-panel status-${updateInfo.syncStatus}`}>
                <div className="lua-live-status-copy">
                  <span className={`lua-channel-badge channel-${updateInfo.channel}`}>
                    {updateInfo.channel === 'live'
                      ? 'Live'
                      : `${t.luaShop.lockedChannel}${updateInfo.pinnedBuildId ? ` · ${updateInfo.pinnedBuildId}` : ''}`}
                  </span>
                  <strong>
                    {updateInfo.lastError
                      || (updateInfo.lastSyncAt
                        ? t.luaShop.syncedAt.replace('{time}', new Date(updateInfo.lastSyncAt).toLocaleString())
                        : t.luaShop.syncNever)}
                  </strong>
                  {updateInfo.sharedDepotConflicts.length > 0 && (
                    <small>
                      {t.luaShop.sharedDepotLocked.replace(
                        '{depots}',
                        updateInfo.sharedDepotConflicts.join(', '),
                      )}
                    </small>
                  )}
                </div>
                {updateInfo.channel === 'live' && (
                  <button
                    type="button"
                    className="lua-live-sync-button"
                    onClick={() => void syncCurrentLuaGame()}
                    disabled={luaSyncing || luaStateChecking || updateInfo.syncStatus === 'checking'}
                  >
                    <RefreshCcw size={15} className={luaSyncing || updateInfo.syncStatus === 'checking' ? 'spin' : ''} />
                    <span>{t.luaShop.sync}</span>
                  </button>
                )}
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

              {/* ── Steamless / Error 54 Fix ── */}
              <div style={{
                marginTop: '16px',
                padding: '20px',
                background: 'var(--theme-control-bg)',
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

      {activeDetailTab === 'overview' && !showLibraryRail && (
        <aside className="store-info-column">
          <section className="panel status-card">
            <header className="side-header">
              <CheckCircle2 size={17} />
              <strong>{stateLabel}</strong>
            </header>
            <dl className="metric-list">
              {acquisitionOnlyStore ? (
                <>
                  <div>
                    <dt>Library</dt>
                    <dd>{inLauncherLibrary ? 'In Library' : 'Not in Library'}</dd>
                  </div>
                  <div>
                    <dt>{t.library.latestVersion}</dt>
                    <dd>{selectedGame.latestVersion}</dd>
                  </div>
                </>
              ) : effectiveMode === 'steam' ? (
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
          {!acquisitionOnlyStore && effectiveMode !== 'steam' && (
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
          <OSTPlayer bgImage={hero || logo || undefined} gameId={selectedGame.id} gameTitle={selectedGame.title} />
          <GameTagsPreview game={selectedGame} />
          <AchievementPreview gameId={selectedGame.id} achievements={detail?.achievements || []} assets={assets} />
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
function SteamIntegrationButton({ gameId, gameTitle, storeMode }: { gameId: string, gameTitle: string, storeMode: 'local' | 'steam' }) {
  const [status, setStatus] = useState<boolean>(false)
  const [luaState, setLuaState] = useState<LuaGameState | null>(null)
  const [loading, setLoading] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const [showRestartConfirm, setShowRestartConfirm] = useState(false)
  const [showEnableModePrompt, setShowEnableModePrompt] = useState(false)
  const [sourceOperation, setSourceOperation] = useState<LuaSourceOperation | null>(null)
  const [autoInstall, setAutoInstall] = useState(() => localStorage.getItem('steamAutoInstall') !== 'false')
  const [skipConfirm, setSkipConfirm] = useState(() => localStorage.getItem('steamSkipRestartConfirm') === 'true')
  const { mapping } = useSteamAppIds()
  const { t } = useLocale()

  const appid = mapping[gameId]

  const checkStatus = useCallback(async () => {
    try {
      const [isAdded, state] = await Promise.all([
        invoke<boolean>('check_steam_status', { appid }),
        invoke<LuaGameState | null>('get_lua_game_state', { appid }),
      ])
      setStatus(isAdded)
      setLuaState(state)
    } catch (e) {
      console.error('Failed to check steam status', e)
    }
  }, [appid])

  // MUST be before any conditional return (React hooks rule)
  useEffect(() => {
    if (!appid) return
    const timer = window.setTimeout(() => void checkStatus(), 0)
    return () => window.clearTimeout(timer)
  }, [appid, checkStatus])

  useEffect(() => {
    if (!appid) return
    let active = true
    let unlisten: (() => void) | undefined
    void listen<LuaGameState>('launcher://lua-game-state', (event) => {
      if (active && event.payload.appid === appid) setLuaState(event.payload)
    }).then((stop) => {
      if (active) unlisten = stop
      else stop()
    })
    return () => {
      active = false
      unlisten?.()
    }
  }, [appid])

  // Hide in "local" mode
  if (storeMode === 'local') {
    return null
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

    setSourceOperation('add')
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
      const requiresRestart = luaState?.requiresSteamRestart === true
      await invoke('remove_from_steam', { appid })
      setStatus(false)
      setLuaState(null)
      showToast(t.library.removeFromSteam, t.library.removeFromSteamSuccess, 'success')

      // Dispatch event to hide Lua-Game Mode tab
      window.dispatchEvent(new CustomEvent('lua-game-mode-changed', {
        detail: { gameId, added: false }
      }))

      if (requiresRestart) {
        if (localStorage.getItem('steamSkipRestartConfirm') === 'true') {
          performRestart();
        } else {
          setShowRestartConfirm(true);
        }
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

  const handleSync = async () => {
    if (!appid || luaState?.channel !== 'live') return
    setSourceOperation(luaState.updateAvailable ? 'update' : 'sync')
  }

  const handleSourceConfirm = async (provider: LuaSourceProvider, statSteamId: string | null, channel: LuaGameChannel) => {
    if (!appid || !sourceOperation) return
    setLoading(true)
    try {
      const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
      const state = sourceOperation === 'add' || channel === 'locked'
        ? await invoke<LuaGameState>('install_lua_game_from_source', {
            request: {
              appid,
              gameName: gameTitle,
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
        : await invoke<LuaGameState>(sourceOperation === 'update'
          ? 'apply_lua_game_update'
          : 'sync_lua_game_from_source', {
            request: {
              appid,
              provider,
              requestId: crypto.randomUUID(),
              timezone,
              statSteamId,
              conflictResolution: null,
            },
          })
      setLuaState(state)
      setStatus(true)
      setSourceOperation(null)
      showToast(
        sourceOperation === 'add' ? t.library.addToSteam : t.luaShop.liveChannel,
        sourceOperation === 'add'
          ? t.library.addToSteamSuccess
          : state.syncStatus === 'updated' ? t.luaShop.syncUpdated : t.luaShop.syncCurrent,
        'success',
      )
      if (sourceOperation === 'add') {
        window.dispatchEvent(new CustomEvent('lua-game-mode-changed', {
          detail: { gameId, added: true }
        }))
      }
      if (state.requiresSteamRestart || sourceOperation === 'add') {
        if (skipConfirm) void performRestart()
        else setShowRestartConfirm(true)
      }
    } finally {
      setLoading(false)
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
              <span>
                {luaState?.channel === 'live'
                  ? 'Live'
                  : luaState?.channel === 'locked'
                    ? `${t.luaShop.lockedChannel}${luaState.pinnedBuildId ? ` · ${luaState.pinnedBuildId}` : ''}`
                    : t.library.addedToSteam}
              </span>
            </button>
            {luaState?.channel === 'live' && (
              <button
                type="button"
                className="lua-integration-sync-button"
                onClick={() => void handleSync()}
                disabled={loading || luaState.syncStatus === 'checking'}
                title={luaState.lastError || (luaState.lastSyncAt
                  ? t.luaShop.syncedAt.replace('{time}', new Date(luaState.lastSyncAt).toLocaleString())
                  : t.luaShop.syncNever)}
              >
                <RefreshCcw size={15} className={loading || luaState.syncStatus === 'checking' ? 'spin' : ''} />
                <span>{t.luaShop.sync}</span>
              </button>
            )}
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

      {sourceOperation && (
        <LuaSourcePickerDialog
          appid={appid}
          gameName={gameTitle}
          operation={sourceOperation}
          preferredProvider={sourceOperation === 'add' ? null : luaState?.selectedSource ?? null}
          preferredChannel={sourceOperation === 'add' ? null : luaState?.channel ?? null}
          onClose={() => setSourceOperation(null)}
          onConfirm={handleSourceConfirm}
        />
      )}

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
            background: 'var(--theme-control-bg)',
            borderRadius: '8px',
            border: '1px solid var(--line)',
            display: 'flex',
            flexDirection: 'column',
            gap: '12px'
          }}>
            {/* Auto Install toggle — uses the same settings-toggle CSS class */}
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer', margin: 0, paddingBottom: '12px', borderBottom: '1px solid var(--line)' }}>
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
              <span style={{ flex: 1, fontSize: '14px', color: autoInstall ? 'var(--text-strong)' : 'var(--muted)' }}>
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
              <span style={{ flex: 1, fontSize: '13px', color: 'var(--muted)' }}>
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

function MediaThumbnail({
  item,
  url,
  eager,
}: {
  item: GameMedia
  url: string
  eager: boolean
}) {
  const ref = useRef<HTMLSpanElement>(null)
  const [shouldLoad, setShouldLoad] = useState(
    () => typeof IntersectionObserver === 'undefined',
  )

  useEffect(() => {
    if (eager || shouldLoad) return

    const element = ref.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      const timer = window.setTimeout(() => setShouldLoad(true), 0)
      return () => window.clearTimeout(timer)
    }

    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        setShouldLoad(true)
        observer.disconnect()
      }
    }, { rootMargin: '96px 0px', threshold: 0.01 })
    observer.observe(element)
    return () => observer.disconnect()
  }, [eager, shouldLoad])

  return (
    <span ref={ref} className="media-thumb-preview">
      {eager || shouldLoad ? <img src={url} alt={item.title} loading="lazy" decoding="async" /> : null}
    </span>
  )
}

export function MediaRail({ detail, assets }: { detail: GameDetail; assets: Record<string, string> }) {
  const { t } = useLocale()
  const safeMedia = useMemo(
    () => Array.isArray(detail.media) ? detail.media : [],
    [detail.media],
  )

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


  const media = useMemo(() => safeMedia
    .filter((item) => isCarouselMedia(item) && assetUrlForId(item.assetId, assets))
    .sort((left, right) => mediaPriority(left) - mediaPriority(right))
    .map((item) => ({ ...item, url: assetUrlForId(item.assetId, assets)! })), [assets, safeMedia])
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
              <img src={active.url} alt="" decoding="async" />
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
            const thumbUrl = isVideo
              ? assetUrlForId(item.thumbnailAssetId, assets) ?? videoThumbMap[item.id] ?? null
              : thumbnailUrlForMedia(item, assets) ?? null

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
                      <MediaThumbnail item={item} url={thumbUrl} eager={index === safeActiveIndex} />
                    ) : (
                      <span className="video-thumb-placeholder"><Play size={24} /></span>
                    )}
                    <Play size={16} className="video-thumb-overlay" />
                  </span>
                ) : (
                  thumbUrl ? <MediaThumbnail item={item} url={thumbUrl} eager={index === safeActiveIndex} /> : null
                )}
              </button>
            )
          })}
        </div>
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
  gameId,
  achievements,
  assets,
}: {
  gameId: string
  achievements: GameAchievement[]
  assets: Record<string, string>
}) {
  const { t } = useLocale()
  const [showAll, setShowAll] = useState(false)
  const [unlockedIds, setUnlockedIds] = useState<Set<string>>(new Set())
  const safeAchievements = Array.isArray(achievements) ? achievements : []
  const available = safeAchievements.filter((achievement) => assetUrlForId(achievement.iconAssetId, assets))
  const preview = available.slice(0, 10)

  useEffect(() => {
    if (!isTauriRuntime()) return
    // Fetch initial unlocked achievements from backend
    invoke<{ achievements?: { id: string, unlocked: boolean }[] }>('get_game_platform_state', { gameId })
      .then((state) => {
        if (state && state.achievements) {
          const unlocked = new Set(state.achievements.filter((a) => a.unlocked).map((a) => a.id))
          setUnlockedIds(unlocked)
        }
      })
      .catch((err) => console.error('Failed to get platform state for achievements:', err))
  }, [gameId])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlistenFn: (() => void) | undefined
    listen<{ gameId: string, id: string }>('launcher://achievement-unlocked', (event) => {
      if (event.payload.gameId === gameId) {
        setUnlockedIds((prev) => {
          const next = new Set(prev)
          next.add(event.payload.id)
          return next
        })
      }
    })
      .then((fn) => {
        unlistenFn = fn
      })
      .catch((err) => console.error('Failed to listen for achievement events:', err))

    return () => {
      if (unlistenFn) unlistenFn()
    }
  }, [gameId])

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
        {preview.map((achievement) => {
          const isUnlocked = unlockedIds.has(achievement.id)
          return (
            <article key={achievement.id} style={{ opacity: isUnlocked ? 1 : 0.6 }}>
              <img
                src={assetUrlForId(achievement.iconAssetId, assets)}
                alt=""
                loading="lazy"
                decoding="async"
                style={{
                  filter: isUnlocked ? 'none' : 'grayscale(100%)',
                  boxShadow: isUnlocked ? '0 0 10px rgba(255, 215, 0, 0.5)' : 'none',
                  transition: 'all 0.3s ease'
                }}
              />
              <div>
                <strong style={{ color: isUnlocked ? 'inherit' : 'rgba(255, 255, 255, 0.5)' }}>{achievement.name}</strong>
                <small>{achievement.hidden && !isUnlocked ? 'Hidden' : achievement.description}</small>
              </div>
            </article>
          )
        })}
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
              {available.map((achievement) => {
                const isUnlocked = unlockedIds.has(achievement.id)
                return (
                  <article key={achievement.id} style={{ opacity: isUnlocked ? 1 : 0.6 }}>
                    <img
                      src={assetUrlForId(achievement.iconAssetId, assets)}
                      alt=""
                      loading="lazy"
                      decoding="async"
                      style={{
                        filter: isUnlocked ? 'none' : 'grayscale(100%)',
                        boxShadow: isUnlocked ? '0 0 10px rgba(255, 215, 0, 0.5)' : 'none',
                        transition: 'all 0.3s ease'
                      }}
                    />
                    <div>
                      <strong style={{ color: isUnlocked ? 'inherit' : 'rgba(255, 255, 255, 0.5)' }}>{achievement.name}</strong>
                      <small>{achievement.hidden && !isUnlocked ? 'Hidden' : achievement.description}</small>
                    </div>
                  </article>
                )
              })}
            </div>
          </section>
        </div>,
        document.body!
      ) : null}
    </section>
  )
}
