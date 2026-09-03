import { useEffect, useMemo, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  Check,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Gamepad2,
  LoaderCircle,
  Plus,
  RefreshCcw,
  Search,
  SlidersHorizontal,
  Upload,
} from 'lucide-react'
import type { GameToolsLibraryItem } from '../../types'
import './cinematic.css'

type ToolsWorkspaceViewProps = {
  desktop: boolean
  locale: string
  games: readonly GameToolsLibraryItem[]
  importBusy: boolean
  addBusy: boolean
  onImport: (paths: string[] | null) => Promise<void>
  onAddAppId: (appId: number) => Promise<void>
  onRestartSteam: () => Promise<void>
  onOpenSteamDb: (appId: number | null) => Promise<void>
  onReloadCovers: () => void
  onOpenSteamLaunchOptions: (appId: number | null) => void
}

const PAGE_SIZE = 12
const ALLOWED_SUFFIXES = ['.lua', '.zip', '.manifest'] as const

function isAllowedImportPath(path: string): boolean {
  const lower = path.toLocaleLowerCase()
  return ALLOWED_SUFFIXES.some((suffix) => lower.endsWith(suffix))
}

export default function ToolsWorkspaceView({
  desktop,
  locale,
  games,
  importBusy,
  addBusy,
  onImport,
  onAddAppId,
  onRestartSteam,
  onOpenSteamDb,
  onReloadCovers,
  onOpenSteamLaunchOptions,
}: ToolsWorkspaceViewProps) {
  const [now, setNow] = useState(() => new Date())
  const [appIdInput, setAppIdInput] = useState('')
  const [query, setQuery] = useState('')
  const [sort, setSort] = useState<'recent' | 'az'>('recent')
  const [page, setPage] = useState(0)
  const [dragOver, setDragOver] = useState(false)

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void getCurrentWindow().onDragDropEvent((event) => {
      if (disposed) return
      if (event.payload.type === 'over') {
        setDragOver(true)
        return
      }
      if (event.payload.type === 'leave') {
        setDragOver(false)
        return
      }
      if (event.payload.type === 'drop') {
        setDragOver(false)
        const paths = event.payload.paths.filter(isAllowedImportPath)
        if (paths.length > 0) void onImport(paths)
      }
    }).then((dispose) => {
      if (disposed) dispose()
      else unlisten = dispose
    }).catch(() => undefined)
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [desktop, onImport])

  const numericAppId = Number(appIdInput)
  const validAppId = Number.isSafeInteger(numericAppId) && numericAppId > 0
  const filteredGames = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    const filtered = normalized
      ? games.filter((game) => game.title.toLocaleLowerCase().includes(normalized)
        || game.subtitle.toLocaleLowerCase().includes(normalized)
        || (game.appId !== null && String(game.appId).includes(normalized)))
      : [...games]
    if (sort === 'az') filtered.sort((left, right) => left.title.localeCompare(right.title, locale))
    return filtered
  }, [games, locale, query, sort])
  const pageCount = Math.max(1, Math.ceil(filteredGames.length / PAGE_SIZE))
  const visiblePage = Math.min(page, pageCount - 1)
  const visibleGames = filteredGames.slice(visiblePage * PAGE_SIZE, (visiblePage + 1) * PAGE_SIZE)

  return (
    <section className="cinematic-tools-workspace" aria-labelledby="cinematic-tools-title">
      <header className="cinematic-view-heading">
        <div>
          <span>0XOLEMON GAME TOOLS</span>
          <h1 id="cinematic-tools-title">Tools</h1>
          <p>Import Steam metadata, add an AppID and browse your launcher library.</p>
        </div>
      </header>

      <div className="cinematic-tools-stage">
        <article className={`cinematic-import-zone${dragOver ? ' is-drag-over' : ''}`}>
          <span>IMPORT</span>
          <button
            type="button"
            disabled={!desktop || importBusy}
            title={!desktop ? 'Import is available in the desktop launcher.' : undefined}
            onClick={() => void onImport(null)}
          >
            {importBusy ? <LoaderCircle className="spin" /> : <Upload />}
            <strong>{dragOver ? 'Release to import' : 'Drag files here'}</strong>
            <small>.lua · .zip · .manifest</small>
            <b>{importBusy ? 'Validating…' : 'Browse'}</b>
          </button>
        </article>

        <article className="cinematic-clock" aria-live="off">
          <strong>{now.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hour12: false })}</strong>
          <b>{now.toLocaleTimeString(locale, { second: '2-digit' })}</b>
          <span>{now.toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long' })}</span>
        </article>

        <article className="cinematic-appid-zone">
          <label>
            STEAM APP ID
            <input
              inputMode="numeric"
              value={appIdInput}
              onChange={(event) => setAppIdInput(event.target.value.replace(/\D/g, ''))}
              placeholder="e.g. 1245620"
            />
          </label>
          <button
            type="button"
            className="is-primary"
            disabled={!desktop || !validAppId || addBusy}
            title={!desktop ? 'Add is available in the desktop launcher.' : !validAppId ? 'Enter a positive numeric Steam AppID.' : undefined}
            onClick={() => void onAddAppId(numericAppId)}
          >
            {addBusy ? <LoaderCircle className="spin" /> : <Plus />}
            Add
          </button>
          <p>Add uses the launcher's existing Steam Lua pipeline. It does not enable the managed GSE runtime.</p>
          <div>
            <button type="button" disabled={!desktop} title={!desktop ? 'Steam controls require the desktop launcher.' : undefined} onClick={() => void onRestartSteam()}>
              <RefreshCcw /> Restart Steam
            </button>
            <button type="button" disabled={!desktop} title={!desktop ? 'SteamDB opens through the desktop launcher.' : undefined} onClick={() => void onOpenSteamDb(validAppId ? numericAppId : null)}>
              <ExternalLink /> SteamDB
            </button>
            <button type="button" disabled={!desktop} title={!desktop ? 'Steam Launch Options requires the desktop launcher.' : 'Stage and safely apply launch entries from Steam appinfo.vdf.'} onClick={() => onOpenSteamLaunchOptions(validAppId ? numericAppId : null)}>
              <SlidersHorizontal /> Launch options
            </button>
          </div>
        </article>
      </div>

      <section className="cinematic-library-panel" aria-labelledby="cinematic-library-title">
        <header>
          <div>
            <span>{games.length} games</span>
            <h2 id="cinematic-library-title">Library</h2>
          </div>
          <div className="cinematic-library-actions">
            <div className="cinematic-segmented" aria-label="Library sort">
              <button type="button" className={sort === 'recent' ? 'is-active' : ''} onClick={() => { setSort('recent'); setPage(0) }}>Recent</button>
              <button type="button" className={sort === 'az' ? 'is-active' : ''} onClick={() => { setSort('az'); setPage(0) }}>A–Z</button>
            </div>
            <button type="button" onClick={onReloadCovers}><RefreshCcw /> Reload covers</button>
            <label><Search /><input value={query} onChange={(event) => { setQuery(event.target.value); setPage(0) }} placeholder="Search library" /></label>
          </div>
        </header>

        <div className="cinematic-library-grid">
          {visibleGames.map((game) => (
            <article key={game.gameId}>
              <div className="cinematic-library-cover">
                {game.imageUrl ? <img src={game.imageUrl} alt="" loading="lazy" decoding="async" /> : <Gamepad2 />}
                {game.installed ? <span><Check /> Installed</span> : null}
              </div>
              <h3>{game.title}</h3>
              <p>{game.appId ? `AppID ${game.appId}` : game.subtitle}</p>
            </article>
          ))}
        </div>
        {visibleGames.length === 0 ? <div className="cinematic-empty"><Search /><strong>No matching games</strong><span>Try another title or AppID.</span></div> : null}
        <footer className="cinematic-pagination">
          <button type="button" disabled={visiblePage === 0} onClick={() => setPage((value) => Math.max(0, value - 1))}><ChevronLeft /> Previous</button>
          <span>Page {visiblePage + 1} / {pageCount}</span>
          <button type="button" disabled={visiblePage + 1 >= pageCount} onClick={() => setPage((value) => Math.min(pageCount - 1, value + 1))}>Next <ChevronRight /></button>
        </footer>
      </section>
    </section>
  )
}
