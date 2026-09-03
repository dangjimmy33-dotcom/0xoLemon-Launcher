import { useEffect, useState } from 'react'
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Download,
  FileCode,
  Gauge,
  HelpCircle,
  MessageSquare,
  Plus,
  RefreshCcw,
  Settings,
  ShoppingCart,
  Sparkles,
  Wrench,
} from 'lucide-react'
import { useLocale } from '../../context/locale'
import type { TabId } from '../../types'
import type { ThemeShellProps } from '../contracts'
import '../steam.css'

type ToolItem = [TabId, string, typeof Settings, number]

export default function SteamShell({
  children,
  activeTab,
  onNavigate,
  onBack,
  onForward,
  canGoBack,
  canGoForward,
  updateCount,
  downloadCount,
  luaModeEnabled,
  displayName,
  onSelectGame,
  onOpenSelfProfile,
}: ThemeShellProps) {
  const { t } = useLocale()
  const [toolsOpen, setToolsOpen] = useState(false)
  const [dragOverLibrary, setDragOverLibrary] = useState(false)
  const primary: Array<[TabId, string]> = [
    ['Store', t.nav.store],
    ['Library', t.nav.library],
    ['Social', t.nav.social],
  ]
  const tools: ToolItem[] = [
    ["What's New!", t.nav.whatsNew, Sparkles, 0],
    ...(luaModeEnabled ? [['Lua Shop', t.nav.luaShop, ShoppingCart, 0] as ToolItem] : []),
    ...(luaModeEnabled ? [['Lua Installer', t.nav.luaInstaller, FileCode, 0] as ToolItem] : []),
    ['Updates', t.nav.updates, RefreshCcw, updateCount],
    ['Downloads', t.nav.downloads, Download, downloadCount],
    ['Settings', t.nav.settings, Settings, 0],
  ]

  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setToolsOpen(false)
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [])

  const navigate = (tab: TabId) => {
    setToolsOpen(false)
    onNavigate(tab)
  }

  const navigatePrimary = (tab: TabId) => {
    if (tab === 'Store' || tab === 'Library') onSelectGame(null)
    navigate(tab)
  }

  return (
    <main
      className={`launcher-shell premium-shell steam-shell-layout${activeTab === 'Library' ? ' steam-shell-has-bottom-bar' : ''}`}
      data-theme-shell="steam"
      data-theme-reference="steam-desktop-2026.08"
    >
      <header className="steam-primary-nav" data-tour="steam-primary-nav">
        <button className="steam-nav-home-mark" type="button" onClick={() => navigate('Home')} aria-label={t.nav.home}>
          <span className="steam-mark-disc" aria-hidden="true" />
          <strong>0xoLemon</strong>
        </button>
        <div className="steam-history-controls" aria-label="Navigation history">
          <button type="button" disabled={!canGoBack} onClick={onBack} aria-label="Back"><ChevronLeft size={18} /></button>
          <button type="button" disabled={!canGoForward} onClick={onForward} aria-label="Forward"><ChevronRight size={18} /></button>
        </div>
        <nav className="steam-primary-tabs" aria-label="Primary">
          {primary.map(([tabId, label]) => {
            const isLib = tabId === 'Library'
            return (
              <button
                key={tabId}
                type="button"
                className={[
                  activeTab === tabId ? 'is-active' : '',
                  isLib && dragOverLibrary ? 'is-drag-over' : '',
                ].filter(Boolean).join(' ')}
                aria-current={activeTab === tabId ? 'page' : undefined}
                data-library-drop-target={isLib ? 'true' : undefined}
                onClick={() => navigatePrimary(tabId)}
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
                {label}
              </button>
            )
          })}
          <button
            type="button"
            className={`steam-account-tab${activeTab === 'Social' ? ' is-active' : ''}`}
            aria-label={t.social.yourProfile}
            onClick={() => {
              setToolsOpen(false)
              onOpenSelfProfile()
            }}
          >
            {(displayName || t.social.yourProfile).toUpperCase()}
          </button>
        </nav>
        <div className="steam-nav-tools">
          <button
            type="button"
            className={toolsOpen ? 'steam-tools-trigger is-open' : 'steam-tools-trigger'}
            aria-expanded={toolsOpen}
            aria-haspopup="menu"
            onClick={() => setToolsOpen((open) => !open)}
          >
            <Wrench size={14} />
            <span>TOOLS</span>
            <ChevronDown size={13} />
          </button>
          {toolsOpen ? (
            <div className="steam-tools-popover" role="menu">
              {tools.map(([tabId, label, Icon, badge]) => (
                <button key={tabId} type="button" role="menuitem" onClick={() => navigate(tabId)}>
                  <Icon size={15} />
                  <span>{label}</span>
                  {badge > 0 ? <b>{badge}</b> : null}
                </button>
              ))}
              <button type="button" role="menuitem" onClick={() => navigate("What's New!")}>
                <HelpCircle size={15} /><span>Help & release notes</span>
              </button>
            </div>
          ) : null}
        </div>
      </header>
      {children}
      {activeTab === 'Library' ? (
        <footer className="steam-bottom-bar">
          <button type="button" onClick={() => navigate('Store')}><Plus size={14} /> Add a game</button>
          <button type="button" onClick={() => navigate('Downloads')}><Download size={14} /> Manage downloads</button>
          <button type="button" onClick={() => navigate('Social')}><MessageSquare size={14} /> Friends & Chat</button>
          <span className="steam-bottom-status"><Gauge size={13} /> 0xoLemon services</span>
        </footer>
      ) : null}
    </main>
  )
}
