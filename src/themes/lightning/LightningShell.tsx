import { useEffect, useState } from 'react'
import { ChevronLeft, ChevronRight, Download, Newspaper } from 'lucide-react'
import type { TabId } from '../../types'
import CinematicNewsView from '../../components/cinematic/CinematicNewsView'
import type { ThemeShellProps } from '../contracts'
import './index.css'

export type GameToolsSection = 'store' | 'tools' | 'bypass' | 'onlineFix'

const GAME_TOOLS_SECTION_KEY = '0xolemon.game-tools.section'
const GAME_TOOLS_SECTION_EVENT = 'launcher://game-tools-open-section'
const NEWS_SESSION_KEY = '0xolemon.cinematic.news.v5.shown'
const ROUTE_DIRECTION_EVENT = 'launcher://cinematic-route-direction'

type Destination = { label: string; tab: TabId; section?: GameToolsSection }

const DESTINATIONS: readonly Destination[] = [
  { label: 'Home', tab: 'Home' },
  { label: 'Nexus', tab: 'Store' },
  { label: 'Library', tab: 'Library' },
  { label: 'Instant Gaming', tab: 'Tools', section: 'store' },
  { label: 'Tools', tab: 'Tools', section: 'tools' },
  { label: 'Bypass', tab: 'Tools', section: 'bypass' },
  { label: 'OnlineFix', tab: 'Tools', section: 'onlineFix' },
  { label: 'Settings', tab: 'Settings' },
]

function readGameToolsSection(): GameToolsSection {
  const value = window.localStorage.getItem(GAME_TOOLS_SECTION_KEY)
  return value === 'store' || value === 'bypass' || value === 'onlineFix' ? value : 'tools'
}

function publishGameToolsSection(section: GameToolsSection) {
  window.localStorage.setItem(GAME_TOOLS_SECTION_KEY, section)
  window.dispatchEvent(new CustomEvent<GameToolsSection>(GAME_TOOLS_SECTION_EVENT, { detail: section }))
}

export default function LightningShell({
  children,
  activeTab,
  onNavigate,
  onBack,
  onForward,
  canGoBack,
  canGoForward,
  downloadCount,
  discordAuthorized,
  instances,
  onSelectGame,
}: ThemeShellProps) {
  const [gameToolsSection, setGameToolsSection] = useState<GameToolsSection>(readGameToolsSection)
  const fixtureParameters = new URLSearchParams(window.location.search)
  const fixtureForcesNews = fixtureParameters.get('fixture') === 'theme-lightning' && fixtureParameters.get('news') !== '0'
  const fixtureSkipsNews = fixtureParameters.get('fixture') === 'theme-lightning' && fixtureParameters.get('news') === '0'
  const [newsOpen, setNewsOpen] = useState(fixtureForcesNews)

  useEffect(() => {
    const handleSection = (event: Event) => {
      const section = (event as CustomEvent<GameToolsSection>).detail
      if (section === 'store' || section === 'tools' || section === 'bypass' || section === 'onlineFix') setGameToolsSection(section)
    }
    window.addEventListener(GAME_TOOLS_SECTION_EVENT, handleSection)
    return () => window.removeEventListener(GAME_TOOLS_SECTION_EVENT, handleSection)
  }, [])

  useEffect(() => {
    if (fixtureSkipsNews || !discordAuthorized || newsOpen || window.sessionStorage.getItem(NEWS_SESSION_KEY) === '1') return
    setNewsOpen(true)
  }, [discordAuthorized, fixtureSkipsNews, newsOpen])

  const closeNews = () => {
    window.sessionStorage.setItem(NEWS_SESSION_KEY, '1')
    setNewsOpen(false)
  }

  const navigate = (destination: Destination) => {
    window.dispatchEvent(new CustomEvent(ROUTE_DIRECTION_EVENT, { detail: { direction: 'forward' } }))
    if (destination.tab === 'Store' || destination.tab === 'Library') onSelectGame(null)
    if (destination.section) {
      setGameToolsSection(destination.section)
      publishGameToolsSection(destination.section)
    }
    onNavigate(destination.tab)
  }

  const navigateBack = () => {
    window.dispatchEvent(new CustomEvent(ROUTE_DIRECTION_EVENT, { detail: { direction: 'backward' } }))
    onBack()
  }

  const navigateForward = () => {
    window.dispatchEvent(new CustomEvent(ROUTE_DIRECTION_EVENT, { detail: { direction: 'forward' } }))
    onForward()
  }

  const activeDestination = DESTINATIONS.find((destination) => (
    destination.tab === activeTab && (!destination.section || destination.section === gameToolsSection)
  ))

  const [dragOverLibrary, setDragOverLibrary] = useState(false)

  return (
    <main className="launcher-shell lightning-shell-layout" data-theme-shell="lightning" data-theme-reference="project-lightning-v5.0.8-installed">
      <header className="lightning-window-brand">0xoLemon Launcher</header>
      <nav className="lightning-primary-nav" aria-label="Launcher areas">
        {DESTINATIONS.map((destination) => {
          const active = destination === activeDestination
          const isLib = destination.tab === 'Library'
          return (
            <button
              key={destination.label}
              type="button"
              className={[
                active ? 'is-active' : '',
                isLib && dragOverLibrary ? 'is-drag-over' : '',
              ].filter(Boolean).join(' ')}
              aria-current={active ? 'page' : undefined}
              data-library-drop-target={isLib ? 'true' : undefined}
              onClick={() => navigate(destination)}
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
              {destination.label}
            </button>
          )
        })}
      </nav>
      <section className="lightning-shell-content" aria-label={activeDestination?.label || activeTab}>{children}</section>
      <footer className="lightning-status-bar">
        <button type="button" disabled={!canGoBack} onClick={navigateBack} aria-label="Back"><ChevronLeft /> Back</button>
        <span>0xoLemon Launcher</span>
        <button type="button" onClick={() => onNavigate('Downloads')}><Download />{downloadCount ? `${downloadCount} active` : 'Downloads'}</button>
        <button type="button" disabled={!canGoForward} onClick={navigateForward} aria-label="Forward">Forward <ChevronRight /></button>
      </footer>
      {activeTab === 'Home' && !newsOpen && instances.some((item) => item.heroUrl || item.gridUrl) ? (
        <button className="cinematic-news-reopen" type="button" onClick={() => setNewsOpen(true)}><Newspaper /> Last News</button>
      ) : null}
      {newsOpen && instances.some((item) => item.heroUrl || item.gridUrl) ? (
        <CinematicNewsView
          items={instances}
          onClose={closeNews}
          onOpen={(gameId) => { closeNews(); onSelectGame(gameId); onNavigate('Library') }}
        />
      ) : null}
    </main>
  )
}
