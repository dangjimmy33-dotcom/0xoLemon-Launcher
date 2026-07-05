import type { ReactNode } from 'react'
import { Cloud, Database, Download, Home, Image as ImageIcon, Library, RefreshCcw, Settings, ShoppingBag, Wifi, WifiOff, Languages, Sparkles } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'
import type { GameCatalog, TabId } from '../types'

export function Sidebar({
  serviceStatus,
  activeTab,
  onSelect,
  updateCount,
  downloadCount,
}: {
  serviceStatus: string
  activeTab: TabId
  onSelect: (tab: TabId) => void
  updateCount: number
  downloadCount: number
}) {
  const { t } = useLocale()
  const normalizedStatus = serviceStatus.toLowerCase()
  const connectionLabel = normalizedStatus.includes('unavailable')
    ? 'Offline'
    : normalizedStatus.includes('checking')
      ? 'Connecting'
      : 'Online'
  const items: [TabId, string, typeof Home][] = [
    ['What\'s New!', t.nav.whatsNew, Sparkles],
    ['Home', t.nav.home, Home],
    ['Store', t.nav.store, ShoppingBag],
    ['Library', t.nav.library, Library],
    ['Offline Activation', t.nav.offlineActivation, WifiOff],
    ['Updates', t.nav.updates, RefreshCcw],
    ['Downloads', t.nav.downloads, Download],
    ['CloudRedirect', t.nav.cloudRedirect, Cloud],
    ['Translations', t.nav.translations, Languages],
    ['Cache', t.nav.cache, Database],
    ['Settings', t.nav.settings, Settings],
  ]

  return (
    <aside className="sidebar">
      <nav>
        {items.map(([tabId, label, Icon]) => (
          <button
            className={activeTab === tabId ? 'nav-item active' : 'nav-item'}
            key={tabId}
            type="button"
            aria-label={label}
            title={label}
            onClick={() => onSelect(tabId)}
          >
            <Icon size={20} />
            <span>{label}</span>
            {tabId === 'Updates' && updateCount > 0 ? <span className="nav-badge">{updateCount}</span> : null}
            {tabId === 'Downloads' && downloadCount > 0 ? <span className="nav-badge">{downloadCount}</span> : null}
          </button>
        ))}
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

import { useEffect, useState } from 'react'
import { Search } from 'lucide-react'
import { assetUrlForId } from '../lib/gameMeta'

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

  useEffect(() => {
    if (!onRequestAsset) return
    for (const game of catalog.games) {
      if (game.gridAssetId && !assets[game.gridAssetId]) {
        onRequestAsset(game, game.gridAssetId)
      }
    }
  }, [catalog.games, onRequestAsset, assets])

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
            onChange={(e) => setSearchQuery(e.target.value)}
            style={{ background: 'transparent', border: 'none', color: 'white', width: '100%', outline: 'none' }}
          />
        </div>
      </header>
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
                <img src={assetUrlForId(game.gridAssetId, assets)} alt="" />
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
