import { useState, useEffect } from 'react'
import type { ReactNode } from 'react'
import { Cloud, Download, Home, Image as ImageIcon, Library, RefreshCcw, Settings, ShoppingBag, ShoppingCart, Wifi, WifiOff, Languages, Sparkles, FileCode, Search, UsersRound, FolderDown, Boxes } from 'lucide-react'
import { useLocale } from '../context/locale'
import type { GameCatalog, TabId } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { UnifiedSearchOverlay, UnifiedSearchResult } from './UnifiedSearchOverlay'

export function Sidebar({
  serviceStatus,
  activeTab,
  onSelect,
  updateCount,
  downloadCount,
  luaModeEnabled,
  isSidebarCollapsed,
}: {
  serviceStatus: string
  activeTab: TabId
  onSelect: (tab: TabId) => void
  updateCount: number
  downloadCount: number
  luaModeEnabled: boolean
  isSidebarCollapsed?: boolean
  onToggleSidebar?: () => void
}) {
  const { t } = useLocale()

  // Use controlled props if provided, otherwise default to expanded
  const collapsed = isSidebarCollapsed ?? false

  const normalizedStatus = serviceStatus.toLowerCase()
  const connectionLabel = normalizedStatus.includes('unavailable')
    ? 'Offline'
    : normalizedStatus.includes('checking')
      ? 'Connecting'
      : 'Online'
  const items: [TabId, string, typeof Home][] = [
    ['What\'s New!', t.nav.whatsNew, Sparkles],
    ['Home', t.nav.home, Home],
    ['Social', t.nav.social, UsersRound],
    ['Store', t.nav.store, ShoppingBag],
    ...(luaModeEnabled ? [['Lua Shop', t.nav.luaShop, ShoppingCart] as [TabId, string, typeof Home]] : []),
    ...(luaModeEnabled ? [['Lua Installer', t.nav.luaInstaller, FileCode] as [TabId, string, typeof Home]] : []),
    ['Depot Downloader', 'Depot Downloader', FolderDown],
    ['GSE / UC Setup', 'GSE / UC Setup', Boxes],
    ['Library', t.nav.library, Library],
    ['Offline Activation', t.nav.offlineActivation, WifiOff],
    ['Updates', t.nav.updates, RefreshCcw],
    ['Downloads', t.nav.downloads, Download],
    ['CloudRedirect', t.nav.cloudRedirect, Cloud],
    ['Translations', t.nav.translations, Languages],
    ['Settings', t.nav.settings, Settings],
  ]

  const [dragOverLibrary, setDragOverLibrary] = useState(false)

  return (
    <aside className={`sidebar ${collapsed ? 'sidebar-collapsed' : ''}`} data-tour="sidebar">
      <div className="sidebar-header">
      </div>
      <nav>
        {items.map(([tabId, label, Icon]) => {
          const isLib = tabId === 'Library'
          const isComingSoon = tabId === 'GSE / UC Setup' || tabId === 'Depot Downloader'
          return (
            <button
              className={[
                activeTab === tabId ? 'nav-item active' : 'nav-item',
                isLib && dragOverLibrary ? 'is-drag-over' : '',
                isComingSoon ? 'nav-item-disabled' : '',
              ].filter(Boolean).join(' ')}
              key={tabId}
              type="button"
              disabled={isComingSoon}
              aria-disabled={isComingSoon}
              aria-label={label}
              title={isComingSoon ? `${label} (Coming Soon)` : label}
              data-tour={`nav-${tabId.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '')}`}
              data-library-drop-target={isLib ? 'true' : undefined}
              onClick={() => {
                if (!isComingSoon) {
                  onSelect(tabId)
                }
              }}
              onDragEnter={isLib ? (e) => {
                e.preventDefault()
                e.stopPropagation()
                setDragOverLibrary(true)
              } : undefined}
              onDragOver={isLib ? (e) => {
                e.preventDefault()
                e.stopPropagation()
                e.dataTransfer.dropEffect = 'copy'
                if (!dragOverLibrary) setDragOverLibrary(true)
              } : undefined}
              onDragLeave={isLib ? (e) => {
                e.preventDefault()
                e.stopPropagation()
                if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                  setDragOverLibrary(false)
                }
              } : undefined}
              onDrop={isLib ? (e) => {
                e.preventDefault()
                e.stopPropagation()
                setDragOverLibrary(false)
                const gameId = e.dataTransfer.getData('application/0xo-game-id') || e.dataTransfer.getData('text/plain')
                if (gameId) {
                  window.dispatchEvent(new CustomEvent('0xo-add-to-library', { detail: { gameId } }))
                }
              } : undefined}
            >
              <Icon size={20} style={{ pointerEvents: 'none' }} />
              <span style={{ pointerEvents: 'none' }}>{label}</span>
              {isComingSoon ? <span className="nav-badge soon-badge" style={{ pointerEvents: 'none', background: 'rgba(255, 180, 0, 0.2)', color: '#ffb400', border: '1px solid rgba(255, 180, 0, 0.4)', fontSize: '10px', padding: '1px 5px', borderRadius: '4px', textTransform: 'uppercase', letterSpacing: '0.5px' }}>Soon</span> : null}
              {tabId === 'Updates' && updateCount > 0 ? <span className="nav-badge" style={{ pointerEvents: 'none' }}>{updateCount}</span> : null}
              {tabId === 'Downloads' && downloadCount > 0 ? <span className="nav-badge" style={{ pointerEvents: 'none' }}>{downloadCount}</span> : null}
            </button>
          )
        })}
      </nav>
      <div className="sidebar-status">
        <div className={`status-line${connectionLabel === 'Offline' ? ' offline' : ''}`}>
          <Wifi size={16} />
          <span>{connectionLabel}</span>
        </div>
      </div>
    </aside>
  )
}
export function TabEmptyState({
  activeTab,
  catalog,
  onSelectGame,
  assets,
  onRequestAsset,
}: {
  activeTab: TabId
  catalog: GameCatalog
  onSelectGame: (gameId: string | null) => void
  assets: Record<string, string>
  onRequestAsset?: (game: import('../types').GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const [searchQuery, setSearchQuery] = useState('')
  const [catalogSearchOverlayOpen, setCatalogSearchOverlayOpen] = useState(false)

  useEffect(() => {
    if (!onRequestAsset) return
    for (const game of catalog.games) {
      if (game.gridAssetId && !assets[game.gridAssetId]) {
        onRequestAsset(game, game.gridAssetId)
      }
    }
  }, [catalog.games, onRequestAsset, assets])

  useEffect(() => {
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setCatalogSearchOverlayOpen(true)
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [])

  const visibleGames = catalog.games.filter(game => {
    const q = searchQuery.toLowerCase().trim()
    if (!q) return true
    return game.title.toLowerCase().includes(q) || (game.developer && game.developer.toLowerCase().includes(q))
  })

  return (
    <section className="tab-empty-view">
      <header className="tab-empty-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <strong>{activeTab}</strong>
          <span>Choose a game to continue.</span>
        </div>
        <div className="search-bar" style={{ display: 'flex', alignItems: 'center', background: 'rgba(255,255,255,0.05)', padding: '6px 12px', borderRadius: '6px', width: '250px' }}>
          <Search size={16} style={{ opacity: 0.5, marginRight: '8px' }} />
          <input
            type="text"
            placeholder="Search games..."
            value={searchQuery}
            onFocus={() => setCatalogSearchOverlayOpen(true)}
            onClick={() => setCatalogSearchOverlayOpen(true)}
            onChange={(e) => setSearchQuery(e.target.value)}
            style={{ background: 'transparent', border: 'none', color: 'white', width: '100%', outline: 'none' }}
          />
        </div>
      </header>
      <UnifiedSearchOverlay
        open={catalogSearchOverlayOpen}
        query={searchQuery}
        onQueryChange={setSearchQuery}
        onClose={() => setCatalogSearchOverlayOpen(false)}
        onSubmit={() => setCatalogSearchOverlayOpen(false)}
        placeholder="Search games, AppID, developer..."
        ariaLabel={`Search ${activeTab} games`}
        resultCount={visibleGames.length}
        resultsHint={`${activeTab} catalog`}
        discoveryTitle="Recommended for discovery"
        discoveryHint="Search by game title or developer. Results stay synchronized with this tab."
        historyKey={`0xo.${activeTab}.searchHistory`}
      >
        {visibleGames.length ? visibleGames.slice(0, 36).map((game) => (
          <UnifiedSearchResult
            key={`tab-search-${game.id}`}
            title={game.title}
            subtitle={game.developer || '0xoLemon catalog'}
            matchLabel={game.latestVersion || game.subtitle || 'Available in catalog'}
            imageUrl={assetUrlForId(game.gridAssetId, assets) || null}
            onClick={() => {
              onSelectGame(game.id)
              setCatalogSearchOverlayOpen(false)
            }}
          />
        )) : (
          <div className="store-search-empty">
            <Search size={28} />
            <strong>No matching games</strong>
            <span>Try another title or developer.</span>
          </div>
        )}
      </UnifiedSearchOverlay>

      <div className="tab-game-list stagger-children">
        {visibleGames.length === 0 ? (
          <div className="downloads-empty">
            <div className="queue-art">
              <RefreshCcw size={19} />
            </div>
            <div>
              <strong>{activeTab === 'Updates' ? 'No updates available' : 'No games available'}</strong>
              <span>
                {activeTab === 'Updates'
                  ? 'Only installed games with a newer published version appear here.'
                  : 'The catalog does not currently contain any games.'}
              </span>
            </div>
          </div>
        ) : (
          visibleGames.map((game) => (
            <button className="tab-game-row reveal" key={game.id} type="button" onClick={() => onSelectGame(game.id)}>
              {assetUrlForId(game.gridAssetId, assets) ? (
                <img src={assetUrlForId(game.gridAssetId, assets)} alt="" loading="lazy" decoding="async" />
              ) : (
                <div className="tab-game-art">
                  <ImageIcon size={22} />
                </div>
              )}
              <span>
                <strong>{game.title}</strong>
                <small>{game.developer}</small>
              </span>
              <Download size={16} />
            </button>
          ))
        )}
      </div>
    </section>
  )
}

export function ScopedTabEmptyState({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return (
    <section className="panel scoped-empty-state">
      <div>{icon}</div>
      <strong>{title}</strong>
      <span>{body}</span>
    </section>
  )
}

export function StatusTile({ label, value }: { label: string; value: string }) {
  return (
    <article className="settings-tile">
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  )
}
