import { useEffect, useMemo } from 'react'
import { CheckCircle2, ChevronLeft, ChevronRight, Library, Settings } from 'lucide-react'
import type { GameInstallState, GameSummary } from '../../types'
import { assetUrlForId } from '../../lib/gameMeta'

type SteamLibraryHomeProps = {
  games: GameSummary[]
  assets: Record<string, string>
  installStates?: Record<string, GameInstallState>
  favoriteGameIds: ReadonlySet<string>
  onSelectGame: (gameId: string) => void
  onCustomizeShelves: () => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}

function SteamShelfHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <header className="steam-library-shelf-header">
      <div>
        <h2>{title}</h2>
        {subtitle ? <span>{subtitle}</span> : null}
      </div>
      <div className="steam-library-shelf-arrows" aria-hidden="true">
        <ChevronLeft size={24} />
        <ChevronRight size={24} />
      </div>
    </header>
  )
}

function SteamArtwork({
  game,
  assets,
  kind,
}: {
  game: GameSummary
  assets: Record<string, string>
  kind: 'hero' | 'portrait'
}) {
  const assetId = kind === 'hero' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId
  const url = assetUrlForId(assetId, assets)
  return url ? (
    <img src={url} alt="" loading="lazy" decoding="async" />
  ) : (
    <span className="steam-library-art-placeholder">{game.title.slice(0, 1)}</span>
  )
}

export function SteamLibraryHome({
  games,
  assets,
  installStates,
  favoriteGameIds,
  onSelectGame,
  onCustomizeShelves,
  onRequestAsset,
}: SteamLibraryHomeProps) {
  const recentGames = useMemo(() => [...games]
    .sort((left, right) => {
      const installDelta = Number(Boolean(installStates?.[right.id]?.installed)) - Number(Boolean(installStates?.[left.id]?.installed))
      if (installDelta !== 0) return installDelta
      return right.latestVersion.localeCompare(left.latestVersion, undefined, { numeric: true })
    })
    .slice(0, 8), [games, installStates])

  const newsGames = useMemo(() => games.slice(0, 5), [games])
  const playNextGames = useMemo(() => {
    const favorites = games.filter((game) => favoriteGameIds.has(game.id))
    const remainder = games.filter((game) => !favoriteGameIds.has(game.id))
    return [...favorites, ...remainder].slice(0, 8)
  }, [favoriteGameIds, games])

  useEffect(() => {
    const visible = new Map<string, GameSummary>()
    for (const game of [...newsGames, ...recentGames, ...playNextGames]) visible.set(game.id, game)
    for (const game of visible.values()) {
      onRequestAsset(game, game.gridAssetId)
      onRequestAsset(game, game.heroAssetId)
    }
  }, [newsGames, onRequestAsset, playNextGames, recentGames])

  if (games.length === 0) {
    return (
      <main className="steam-library-home steam-library-home-empty">
        <strong>Your library is empty</strong>
        <span>Add a game from Store to see it here.</span>
      </main>
    )
  }

  const featured = recentGames[0] ?? games[0]
  const featuredInstalled = Boolean(installStates?.[featured.id]?.installed)

  return (
    <main className="steam-library-home" aria-label="Steam-style Library home">
      <section className="steam-library-shelf steam-library-news-shelf">
        <SteamShelfHeader title="What's New" />
        <div className="steam-library-news-track">
          {newsGames.map((game) => (
            <button key={game.id} type="button" className="steam-library-news-card" onClick={() => onSelectGame(game.id)}>
              <span className="steam-library-news-art"><SteamArtwork game={game} assets={assets} kind="hero" /></span>
              <strong>{game.title}</strong>
              <small>{game.latestVersion}</small>
            </button>
          ))}
        </div>
      </section>

      <section className="steam-library-shelf steam-library-recent-shelf">
        <SteamShelfHeader title="Recent games" />
        <div className="steam-library-recent-track">
          <button type="button" className="steam-library-featured-card" onClick={() => onSelectGame(featured.id)}>
            <span className="steam-library-featured-art"><SteamArtwork game={featured} assets={assets} kind="hero" /></span>
            <span className="steam-library-featured-footer">
              <span className={`steam-library-status-tile${featuredInstalled ? ' is-installed' : ''}`} aria-hidden="true">
                {featuredInstalled ? <CheckCircle2 size={28} /> : <Library size={27} />}
              </span>
              <span>
                <strong>{featured.title}</strong>
                <small>{featuredInstalled ? 'Installed' : 'In your library'}</small>
              </span>
            </span>
          </button>
          {recentGames.slice(1).map((game) => (
            <button key={game.id} type="button" className="steam-library-portrait-card" onClick={() => onSelectGame(game.id)} title={game.title}>
              <SteamArtwork game={game} assets={assets} kind="portrait" />
              <span className="steam-library-card-status">{installStates?.[game.id]?.installed ? 'Installed' : 'Library'}</span>
            </button>
          ))}
        </div>
      </section>

      <section className="steam-library-shelf steam-library-next-shelf">
        <SteamShelfHeader title="Play next" subtitle="Games from your library" />
        <div className="steam-library-next-track">
          {playNextGames.map((game) => (
            <button key={game.id} type="button" className="steam-library-next-card" onClick={() => onSelectGame(game.id)}>
              <SteamArtwork game={game} assets={assets} kind="portrait" />
              <span><strong>{game.title}</strong><small>{game.developer}</small></span>
            </button>
          ))}
        </div>
      </section>

      <button type="button" className="steam-library-add-shelf" aria-label="Customize Library shelves" onClick={onCustomizeShelves}>
        <Settings size={15} /> Customize shelves
      </button>
    </main>
  )
}
