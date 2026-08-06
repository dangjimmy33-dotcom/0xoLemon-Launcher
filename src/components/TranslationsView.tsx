import { useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  Download,
  Gamepad2,
  HardDrive,
  Languages,
  Loader2,
  Search,
  ShieldCheck,
  Trash2,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type { GameCatalog, GameInstallState, GameSummary } from '../types'
import { assetUrlForId } from '../lib/gameMeta'

interface TranslationInfo {
  file_name: string
  path: string
  size: number
}

type TranslationState = {
  status: 'loading' | 'ready' | 'error'
  items: TranslationInfo[]
  installed: boolean
  error?: string
}

type TranslationFilter = 'all' | 'available' | 'installed'

interface TranslationsViewProps {
  catalog: GameCatalog
  selectedGameId?: string | null
  assets: Record<string, string>
  installStates?: Record<string, GameInstallState>
  onSelectGame: (gameId: string | null) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  onVerify?: () => void
}

function formatTranslationName(fileName: string, gameTitle?: string): string {
  let name = fileName.replace(/\.7z$/i, '')
  if (gameTitle) {
    const titleRegex = new RegExp(gameTitle.replace(/[^a-zA-Z0-9]/g, '.*'), 'i')
    name = name.replace(titleRegex, '')
  }
  name = name.replace(/viethoa/i, '').replace(/[.\-_]+/g, ' ').trim()
  if (!name) return 'Default Vietnamese translation'
  if (name.toLowerCase() === 'full') return 'Full Vietnamese translation'
  return name.charAt(0).toUpperCase() + name.slice(1)
}

function formatSize(size: number) {
  if (size <= 0) return 'Size unavailable'
  const mb = size / 1024 / 1024
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(2)} MB`
}

function TranslationCard({
  game,
  state,
  image,
  gameInstalled,
  onOpen,
}: {
  game: GameSummary
  state?: TranslationState
  image?: string
  gameInstalled: boolean
  onOpen: () => void
}) {
  const available = Boolean(state?.status === 'ready' && state.items.length > 0)
  const statusText = state?.status === 'loading'
    ? 'Checking translation'
    : state?.status === 'error'
      ? 'Could not check'
      : state?.installed
        ? 'Translation installed'
        : available
          ? `${state?.items.length ?? 0} archive${state?.items.length === 1 ? '' : 's'} available`
          : 'No translation available'

  return (
    <button
      type="button"
      className={`translation-catalog-card ${available ? 'is-available' : ''} ${state?.installed ? 'is-installed' : ''}`}
      onClick={onOpen}
    >
      <span className="translation-card-art">
        {image ? <img src={image} alt="" loading="lazy" decoding="async" /> : <Languages size={28} />}
        <i />
        <span className="translation-card-status">
          {state?.status === 'loading' ? <Loader2 size={13} className="is-spinning" /> : state?.installed ? <CheckCircle2 size={13} /> : available ? <Download size={13} /> : <AlertCircle size={13} />}
          {statusText}
        </span>
      </span>
      <span className="translation-card-copy">
        <strong>{game.title}</strong>
        <small>{game.developer || game.publisher || game.subtitle || 'Community translation'}</small>
        <span>
          <em>{gameInstalled ? 'Game installed' : 'Game not installed'}</em>
          {available ? <b>View details</b> : null}
        </span>
      </span>
    </button>
  )
}

export function TranslationsView({
  catalog,
  selectedGameId,
  assets,
  installStates = {},
  onSelectGame,
  onRequestAsset,
  onVerify,
}: TranslationsViewProps) {
  const [translationStates, setTranslationStates] = useState<Record<string, TranslationState>>({})
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<TranslationFilter>('all')
  const [installing, setInstalling] = useState<string | null>(null)
  const [uninstalling, setUninstalling] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const selectedGame = useMemo(
    () => catalog.games.find((game) => game.id === selectedGameId) ?? null,
    [catalog.games, selectedGameId],
  )

  useEffect(() => {
    if (catalog.games.length === 0) return
    let canceled = false
    const games = [...catalog.games].sort((a, b) => a.title.localeCompare(b.title))
    let cursor = 0

    const worker = async () => {
      while (!canceled) {
        const index = cursor++
        if (index >= games.length) return
        const game = games[index]
        setTranslationStates((current) => current[game.id]
          ? current
          : { ...current, [game.id]: { status: 'loading', items: [], installed: false } })
        try {
          const [items, installed] = await Promise.all([
            invoke<TranslationInfo[]>('get_available_translations', { gameId: game.id }),
            invoke<boolean>('get_translation_status', { gameId: game.id }).catch(() => false),
          ])
          if (!canceled) {
            setTranslationStates((current) => ({
              ...current,
              [game.id]: { status: 'ready', items, installed },
            }))
          }
        } catch (error) {
          if (!canceled) {
            setTranslationStates((current) => ({
              ...current,
              [game.id]: { status: 'error', items: [], installed: false, error: String(error) },
            }))
          }
        }
      }
    }

    const workerCount = Math.min(4, games.length)
    void Promise.all(Array.from({ length: workerCount }, () => worker()))
    return () => { canceled = true }
  }, [catalog.games])

  const filteredGames = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return catalog.games
      .filter((game) => {
        const state = translationStates[game.id]
        if (filter === 'available' && !(state?.status === 'ready' && state.items.length > 0)) return false
        if (filter === 'installed' && !state?.installed) return false
        if (!needle) return true
        return [game.title, game.subtitle, game.developer, game.publisher]
          .filter(Boolean)
          .some((value) => String(value).toLowerCase().includes(needle))
      })
      .sort((a, b) => {
        const aState = translationStates[a.id]
        const bState = translationStates[b.id]
        const aRank = aState?.installed ? 0 : aState?.items.length ? 1 : aState?.status === 'loading' ? 2 : 3
        const bRank = bState?.installed ? 0 : bState?.items.length ? 1 : bState?.status === 'loading' ? 2 : 3
        return aRank - bRank || a.title.localeCompare(b.title)
      })
  }, [catalog.games, filter, query, translationStates])

  useEffect(() => {
    filteredGames.slice(0, 28).forEach((game) => {
      if (game.gridAssetId && !assets[game.gridAssetId]) onRequestAsset(game, game.gridAssetId)
    })
  }, [assets, filteredGames, onRequestAsset])

  useEffect(() => {
    if (!selectedGame) return
    if (selectedGame.heroAssetId && !assets[selectedGame.heroAssetId]) onRequestAsset(selectedGame, selectedGame.heroAssetId, true)
    if (selectedGame.gridAssetId && !assets[selectedGame.gridAssetId]) onRequestAsset(selectedGame, selectedGame.gridAssetId, true)
  }, [assets, onRequestAsset, selectedGame])

  const availableCount = useMemo(
    () => Object.values(translationStates).filter((state) => state.status === 'ready' && state.items.length > 0).length,
    [translationStates],
  )
  const installedCount = useMemo(
    () => Object.values(translationStates).filter((state) => state.installed).length,
    [translationStates],
  )

  const handleInstall = async (gameId: string, path: string) => {
    try {
      setInstalling(path)
      setActionError(null)
      await invoke('install_translation', { gameId, translationPath: path })
      setTranslationStates((current) => ({
        ...current,
        [gameId]: { ...(current[gameId] ?? { status: 'ready', items: [] }), installed: true },
      }))
    } catch (error) {
      setActionError(`Failed to install translation: ${error}`)
    } finally {
      setInstalling(null)
    }
  }

  const handleUninstall = async (gameId: string) => {
    try {
      setUninstalling(true)
      setActionError(null)
      await invoke('uninstall_translation', { gameId })
      setTranslationStates((current) => ({
        ...current,
        [gameId]: { ...(current[gameId] ?? { status: 'ready', items: [] }), installed: false },
      }))
      onVerify?.()
    } catch (error) {
      setActionError(`Failed to uninstall translation: ${error}`)
    } finally {
      setUninstalling(false)
    }
  }

  if (selectedGame) {
    const state = translationStates[selectedGame.id]
    const hero = assetUrlForId(selectedGame.heroAssetId, assets) || assetUrlForId(selectedGame.gridAssetId, assets)
    const gameInstalled = Boolean(installStates[selectedGame.id]?.installed)

    return (
      <section className="translations-shell translation-detail-view">
        <header className="translation-detail-hero">
          {hero ? <img src={hero} alt="" /> : null}
          <i />
          <button type="button" className="translation-back" onClick={() => onSelectGame(null)}>
            <ChevronLeft size={16} /> Back to catalog
          </button>
          <div className="translation-detail-copy">
            <span><Languages size={15} /> Vietnamese translation</span>
            <h1>{selectedGame.title}</h1>
            <p>{selectedGame.subtitle || 'Install and manage community Vietnamese translation archives for this game.'}</p>
          </div>
        </header>

        <div className="translation-detail-layout">
          <aside className="translation-game-summary panel">
            <div className="translation-summary-art">
              {assetUrlForId(selectedGame.gridAssetId, assets) ? <img src={assetUrlForId(selectedGame.gridAssetId, assets)} alt="" /> : <Gamepad2 size={30} />}
            </div>
            <strong>{selectedGame.title}</strong>
            <span>{selectedGame.developer || selectedGame.publisher || 'Community release'}</span>
            <dl>
              <div><dt>Game status</dt><dd>{gameInstalled ? 'Installed' : 'Not installed'}</dd></div>
              <div><dt>Translation</dt><dd>{state?.installed ? 'Installed' : state?.items.length ? 'Available' : 'Unavailable'}</dd></div>
              <div><dt>Archives</dt><dd>{state?.items.length ?? 0}</dd></div>
            </dl>
            {state?.installed ? (
              <button type="button" className="translation-danger-action" onClick={() => void handleUninstall(selectedGame.id)} disabled={uninstalling || installing !== null}>
                {uninstalling ? <Loader2 size={16} className="is-spinning" /> : <Trash2 size={16} />}
                {uninstalling ? 'Removing…' : 'Remove translation'}
              </button>
            ) : null}
          </aside>

          <main className="translation-archive-panel panel">
            <header>
              <div><span>Available archives</span><strong>Choose a Vietnamese package</strong></div>
              <small>{state?.status === 'loading' ? 'Checking repository…' : `${state?.items.length ?? 0} found`}</small>
            </header>

            {actionError ? <div className="translation-error"><AlertCircle size={16} />{actionError}</div> : null}

            {state?.status === 'loading' || !state ? (
              <div className="translation-loading"><Loader2 size={21} className="is-spinning" /><span>Checking available translations…</span></div>
            ) : state.status === 'error' ? (
              <div className="translation-empty"><AlertCircle size={25} /><strong>Could not load translations</strong><span>{state.error}</span></div>
            ) : state.items.length === 0 ? (
              <div className="translation-empty"><Languages size={25} /><strong>No translation found</strong><span>This game does not currently have a Vietnamese archive in the repository.</span></div>
            ) : (
              <div className="translation-archive-list">
                {state.items.map((item, index) => (
                  <article className="translation-archive-row" key={item.path}>
                    <span className="translation-archive-index">{String(index + 1).padStart(2, '0')}</span>
                    <div>
                      <strong>{formatTranslationName(item.file_name, selectedGame.title)}</strong>
                      <span><HardDrive size={13} /> {formatSize(item.size)} · Protected archive</span>
                    </div>
                    <span className="translation-archive-safety"><ShieldCheck size={14} /> Verified source</span>
                    <button
                      type="button"
                      onClick={() => void handleInstall(selectedGame.id, item.path)}
                      disabled={!gameInstalled || installing !== null || uninstalling}
                      title={!gameInstalled ? 'Install the game before applying its translation' : undefined}
                    >
                      {installing === item.path ? <Loader2 size={16} className="is-spinning" /> : <Download size={16} />}
                      {installing === item.path ? 'Installing…' : state.installed ? 'Reinstall' : 'Install'}
                    </button>
                  </article>
                ))}
              </div>
            )}

            {!gameInstalled ? <p className="translation-install-note">Install the game first. The translation manager applies files directly to the registered game folder.</p> : null}
          </main>
        </div>
      </section>
    )
  }

  return (
    <section className="translations-shell translation-catalog-view">
      <header className="translation-catalog-header">
        <div>
          <span>Translation catalog</span>
          <h1>Vietnamese translations</h1>
          <p>Browse supported games, open a package, then install or remove it from one place.</p>
        </div>
        <div className="translation-catalog-stats">
          <div><strong>{availableCount}</strong><span>Available</span></div>
          <div><strong>{installedCount}</strong><span>Installed</span></div>
          <div><strong>{catalog.games.length}</strong><span>Games checked</span></div>
        </div>
      </header>

      <div className="translation-catalog-toolbar">
        <label className="translation-search">
          <Search size={16} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search games or translations" />
        </label>
        <div className="translation-filter-tabs" role="tablist" aria-label="Translation filters">
          {(['all', 'available', 'installed'] as TranslationFilter[]).map((item) => (
            <button key={item} type="button" className={filter === item ? 'is-active' : ''} onClick={() => setFilter(item)}>
              {item === 'all' ? 'All games' : item === 'available' ? 'Available' : 'Installed'}
            </button>
          ))}
        </div>
      </div>

      {filteredGames.length === 0 ? (
        <div className="translation-empty catalog-empty"><Search size={26} /><strong>No matching games</strong><span>Try another title or clear the current filter.</span></div>
      ) : (
        <div className="translation-catalog-grid">
          {filteredGames.map((game) => (
            <TranslationCard
              key={game.id}
              game={game}
              state={translationStates[game.id]}
              image={assetUrlForId(game.gridAssetId, assets)}
              gameInstalled={Boolean(installStates[game.id]?.installed)}
              onOpen={() => onSelectGame(game.id)}
            />
          ))}
        </div>
      )}
    </section>
  )
}
