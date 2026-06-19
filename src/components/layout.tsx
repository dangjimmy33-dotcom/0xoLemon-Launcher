import type { ReactNode } from 'react'
import { Database, Download, Image as ImageIcon, Library, RefreshCcw, Settings, ShoppingBag, Wifi } from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
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
  const normalizedStatus = serviceStatus.toLowerCase()
  const connectionLabel = normalizedStatus.includes('unavailable')
    ? 'Offline'
    : normalizedStatus.includes('checking')
      ? 'Connecting'
      : 'Online'
  const items = [
    [t.nav.store, ShoppingBag],
    [t.nav.library, Library],
    [t.nav.updates, RefreshCcw],
    [t.nav.downloads, Download],
    [t.nav.cache, Database],
    [t.nav.settings, Settings],
  ] as const

  return (
    <aside className="sidebar">
      <nav>
        {items.map(([label, Icon]) => (
          <button
            className={activeTab === label ? 'nav-item active' : 'nav-item'}
            key={label}
            type="button"
            aria-label={label}
            title={label}
            onClick={() => onSelect(label)}
          >
            <Icon size={20} />
            <span>{label}</span>
            {label === 'Updates' && updateCount > 0 ? <span className="nav-badge">{updateCount}</span> : null}
            {label === 'Downloads' && downloadCount > 0 ? <span className="nav-badge">{downloadCount}</span> : null}
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

export function TabEmptyState({
  activeTab,
  catalog,
  onSelectGame,
  assets,
}: {
  activeTab: TabId
  catalog: GameCatalog
  onSelectGame: (gameId: string | null) => void
  assets: Record<string, string>
}) {
  return (
    <section className="tab-empty-view">
      <header className="tab-empty-header">
        <div>
          <strong>{activeTab}</strong>
          <span>Choose a game to continue.</span>
        </div>
      </header>
      <div className="tab-game-list">
        {catalog.games.length === 0 ? (
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
          catalog.games.map((game) => (
            <button className="tab-game-row" key={game.id} type="button" onClick={() => onSelectGame(game.id)}>
              {assets[game.gridAssetId] ? (
                <img src={assets[game.gridAssetId]} alt="" />
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
