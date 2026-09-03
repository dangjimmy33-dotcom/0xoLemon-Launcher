import { useEffect, useState } from 'react'
import {
  ChevronLeft,
  ChevronRight,
  CircleUserRound,
  Download,
  FileCode,
  Home,
  Library,
  Layers3,
  MoreHorizontal,
  RefreshCcw,
  Settings,
  ShoppingBag,
  ShoppingCart,
  Sparkles,
  UsersRound,
  WifiOff,
} from 'lucide-react'
import { useLocale } from '../../context/locale'
import type { TabId } from '../../types'
import type { ThemeShellProps } from '../contracts'
import './index.css'

type RailItem = [TabId, string, typeof Home, number]

export default function XmclShell({
  children,
  activeTab,
  onNavigate,
  onBack,
  onForward,
  canGoBack,
  canGoForward,
  serviceStatus,
  updateCount,
  downloadCount,
  luaModeEnabled,
  selectedGameId,
  instances,
  instanceGroups,
  onSelectGame,
}: ThemeShellProps) {
  const { t } = useLocale()
  const [moreOpen, setMoreOpen] = useState(false)
  const [groupsOpen, setGroupsOpen] = useState(false)
  const [activeGroupId, setActiveGroupId] = useState(instanceGroups[0]?.id ?? 'all-instances')
  const normalizedStatus = serviceStatus.toLowerCase()
  const online = !normalizedStatus.includes('unavailable') && !normalizedStatus.includes('offline')
  const primary: RailItem[] = [
    ['Home', t.nav.home, Home, 0],
    ['Store', t.nav.store, ShoppingBag, 0],
    ['Library', t.nav.library, Library, 0],
    ['Social', t.nav.social, UsersRound, 0],
    ...(luaModeEnabled ? [['Lua Shop', t.nav.luaShop, ShoppingCart, 0] as RailItem] : []),
  ]
  const utilities: RailItem[] = [
    ['Downloads', t.nav.downloads, Download, downloadCount],
    ['Updates', t.nav.updates, RefreshCcw, updateCount],
  ]
  const more: Array<[TabId, string, typeof Home]> = [
    ["What's New!", t.nav.whatsNew, Sparkles],
    ...(luaModeEnabled ? [['Lua Installer', t.nav.luaInstaller, FileCode] as [TabId, string, typeof Home]] : []),
    ['Offline Activation', t.nav.offlineActivation, WifiOff],
  ]

  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setMoreOpen(false)
        setGroupsOpen(false)
      }
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [])

  useEffect(() => {
    if (instanceGroups.some((group) => group.id === activeGroupId)) return
    setActiveGroupId(instanceGroups[0]?.id ?? 'all-instances')
  }, [activeGroupId, instanceGroups])

  const navigate = (tab: TabId) => {
    setMoreOpen(false)
    setGroupsOpen(false)
    onNavigate(tab)
  }

  const activeGroup = instanceGroups.find((group) => group.id === activeGroupId)
  const visibleInstances = activeGroup
    ? instances.filter((instance) => activeGroup.gameIds.includes(instance.gameId))
    : instances

  const [dragOverLibrary, setDragOverLibrary] = useState(false)

  const RailButton = ({ item: [tabId, label, Icon, badge] }: { item: RailItem }) => {
    const isLib = tabId === 'Library'
    return (
      <button
        type="button"
        className={[
          `xmcl-nav-item${activeTab === tabId ? ' is-active' : ''}`,
          isLib && dragOverLibrary ? 'is-drag-over' : '',
        ].filter(Boolean).join(' ')}
        aria-label={label}
        aria-current={activeTab === tabId ? 'page' : undefined}
        title={label}
        data-library-drop-target={isLib ? 'true' : undefined}
        onClick={() => navigate(tabId)}
        onDragOver={isLib ? (e) => {
          e.preventDefault()
          e.dataTransfer.dropEffect = 'copy'
          setDragOverLibrary(true)
        } : undefined}
        onDragLeave={isLib ? () => setDragOverLibrary(false) : undefined}
        onDrop={isLib ? (e) => {
          e.preventDefault()
          setDragOverLibrary(false)
          const gameId = e.dataTransfer.getData('application/0xo-game-id') || e.dataTransfer.getData('text/plain')
          if (gameId) {
            window.dispatchEvent(new CustomEvent('0xo-add-to-library', { detail: { gameId } }))
          }
        } : undefined}
      >
        <Icon size={23} strokeWidth={1.85} />
        {badge > 0 ? <span className="xmcl-nav-badge">{Math.min(badge, 99)}</span> : null}
      </button>
    )
  }

  return (
    <main className="launcher-shell premium-shell xmcl-shell-layout" data-theme-shell="xmcl" data-theme-reference="xmcl-source-2026.08">
      <aside className="xmcl-primary-nav" aria-label="XMCL instance navigation" data-tour="xmcl-primary-nav">
        <div className="xmcl-nav-top">
          <button type="button" className="xmcl-profile-button" onClick={() => navigate('Social')} title={t.nav.social} aria-label={t.nav.social}>
            <CircleUserRound size={30} strokeWidth={1.55} />
            <span className={`xmcl-connection-dot${online ? ' is-online' : ''}`} aria-hidden="true" />
          </button>
          <div className="xmcl-history-controls">
            <button type="button" className="xmcl-back-button" disabled={!canGoBack} onClick={onBack} title="Back" aria-label="Back"><ChevronLeft size={20} /></button>
            <button type="button" className="xmcl-back-button" disabled={!canGoForward} onClick={onForward} title="Forward" aria-label="Forward"><ChevronRight size={20} /></button>
          </div>
        </div>
        <nav className="xmcl-nav-primary" aria-label="Instances and primary pages">
          {primary.map((item) => <RailButton key={item[0]} item={item} />)}
        </nav>
        <div className="xmcl-instance-divider">
          <button type="button" aria-expanded={groupsOpen} onClick={() => setGroupsOpen((open) => !open)} title={activeGroup?.name || 'Instance groups'}>
            <Layers3 size={16} />
          </button>
          {groupsOpen ? (
            <div className="xmcl-instance-groups" role="menu">
              {instanceGroups.map((group) => (
                <button
                  key={group.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={group.id === activeGroupId}
                  className={group.id === activeGroupId ? 'is-active' : ''}
                  onClick={() => { setActiveGroupId(group.id); setGroupsOpen(false) }}
                >
                  <span>{group.name}</span><small>{group.gameIds.length || instances.length}</small>
                </button>
              ))}
            </div>
          ) : null}
        </div>
        <div className="xmcl-instance-stack" role="list" aria-label={activeGroup?.name || 'Game instances'}>
          {visibleInstances.map((instance) => (
            <button
              key={instance.gameId}
              type="button"
              role="listitem"
              className={`xmcl-instance-item${selectedGameId === instance.gameId ? ' is-active' : ''}${instance.playing ? ' is-playing' : ''}`}
              onClick={() => onSelectGame(instance.gameId)}
              title={instance.title}
              aria-label={instance.title}
              aria-current={selectedGameId === instance.gameId ? 'true' : undefined}
            >
              <span className="xmcl-instance-indicator" aria-hidden="true" />
              <span className="xmcl-instance-art">
                {instance.iconUrl ? <img src={instance.iconUrl} alt="" loading="lazy" decoding="async" /> : <b>{instance.title.slice(0, 1).toUpperCase()}</b>}
              </span>
              {instance.favorite ? <span className="xmcl-instance-pin" aria-label="Favorite">•</span> : null}
            </button>
          ))}
          {visibleInstances.length === 0 ? (
            <button type="button" className="xmcl-instance-empty" onClick={() => navigate('Store')} title="Add an instance">+</button>
          ) : null}
        </div>
        <nav className="xmcl-nav-utilities" aria-label="Utilities">
          {utilities.map((item) => <RailButton key={item[0]} item={item} />)}
          <div className="xmcl-more-anchor">
            <button type="button" className={`xmcl-nav-item${moreOpen ? ' is-active' : ''}`} aria-label="More tools" aria-expanded={moreOpen} onClick={() => setMoreOpen((open) => !open)}>
              <MoreHorizontal size={23} strokeWidth={1.85} />
            </button>
            {moreOpen ? (
              <div className="xmcl-more-popover" role="menu">
                {more.map(([tabId, label, Icon]) => (
                  <button key={tabId} type="button" role="menuitem" onClick={() => navigate(tabId)}><Icon size={17} /><span>{label}</span></button>
                ))}
              </div>
            ) : null}
          </div>
          <RailButton item={['Settings', t.nav.settings, Settings, 0]} />
        </nav>
      </aside>
      {children}
    </main>
  )
}
