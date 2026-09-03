import { useEffect, useMemo, useRef, useState } from 'react'
import { ChevronLeft, ChevronRight, Library, Search, Star } from 'lucide-react'
import type { GameSummary } from '../../types'
import { assetUrlForId } from '../../lib/gameMeta'
import { getGameTags } from '../../lib/gameTags'

type SteamStoreHomeProps = {
  games: GameSummary[]
  assets: Record<string, string>
  ownedGameIds: ReadonlySet<string>
  wishlistGameIds: ReadonlySet<string>
  onSelectGame: (gameId: string) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}

function normalize(value: string) {
  return value
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .trim()
}

function artworkUrl(game: GameSummary, assets: Record<string, string>, kind: 'hero' | 'grid') {
  const assetId = kind === 'hero' ? (game.heroAssetId || game.gridAssetId) : game.gridAssetId
  return assetUrlForId(assetId, assets)
}

function SteamStoreArtwork({
  game,
  assets,
  kind,
}: {
  game: GameSummary
  assets: Record<string, string>
  kind: 'hero' | 'grid'
}) {
  const url = artworkUrl(game, assets, kind)
  return url ? (
    <img src={url} alt="" loading={kind === 'hero' ? 'eager' : 'lazy'} decoding="async" />
  ) : (
    <span className="steam-store-art-placeholder" aria-hidden="true">{game.title.slice(0, 1)}</span>
  )
}

export function SteamStoreHome({
  games,
  assets,
  ownedGameIds,
  wishlistGameIds,
  onSelectGame,
  onRequestAsset,
}: SteamStoreHomeProps) {
  const [query, setQuery] = useState('')
  const [activeCategory, setActiveCategory] = useState<string | null>(null)
  const [featuredIndex, setFeaturedIndex] = useState(0)
  const [wishlistOnly, setWishlistOnly] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)

  const categories = useMemo(() => {
    const counts = new Map<string, { label: string; count: number }>()
    for (const game of games) {
      for (const tag of getGameTags(game)) {
        const current = counts.get(tag.id)
        counts.set(tag.id, { label: tag.label, count: (current?.count ?? 0) + 1 })
      }
    }
    return [...counts.entries()]
      .sort((left, right) => right[1].count - left[1].count)
      .slice(0, 5)
      .map(([id, value]) => ({ id, ...value }))
  }, [games])

  const filteredGames = useMemo(() => {
    const term = normalize(query)
    return games.filter((game) => {
      if (wishlistOnly && !wishlistGameIds.has(game.id)) return false
      if (activeCategory && !getGameTags(game).some((tag) => tag.id === activeCategory)) return false
      if (!term) return true
      return [game.title, game.subtitle, game.developer, game.publisher, String(game.appid ?? '')]
        .some((value) => normalize(value).includes(term))
    })
  }, [activeCategory, games, query, wishlistGameIds, wishlistOnly])

  const featuredGames = filteredGames.slice(0, 6)
  const featured = featuredGames[featuredIndex % Math.max(featuredGames.length, 1)]
  const recommendationGames = filteredGames.slice(1, 9)
  const updatedGames = [...filteredGames]
    .sort((left, right) => right.latestVersion.localeCompare(left.latestVersion, undefined, { numeric: true }))
    .slice(0, 8)

  useEffect(() => {
    setFeaturedIndex(0)
  }, [activeCategory, query, wishlistOnly])

  useEffect(() => {
    const visible = new Map<string, GameSummary>()
    for (const game of [...featuredGames, ...recommendationGames, ...updatedGames]) visible.set(game.id, game)
    for (const game of visible.values()) {
      onRequestAsset(game, game.heroAssetId)
      onRequestAsset(game, game.gridAssetId)
    }
  }, [featuredGames, onRequestAsset, recommendationGames, updatedGames])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        searchRef.current?.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const scrollToSection = (id: string) => {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  const changeFeatured = (delta: number) => {
    if (featuredGames.length === 0) return
    setFeaturedIndex((current) => (current + delta + featuredGames.length) % featuredGames.length)
  }

  return (
    <main className="steam-store-home" aria-label="0xoLemon Store">
      <header className="steam-store-navigation">
        <nav aria-label="Store sections">
          <button type="button" onClick={() => scrollToSection('steam-store-featured')}>Browse</button>
          <button type="button" onClick={() => scrollToSection('steam-store-recommended')}>Recommendations</button>
          <button type="button" onClick={() => scrollToSection('steam-store-categories')}>Categories</button>
          <button type="button" onClick={() => scrollToSection('steam-store-updated')}>New & updated</button>
        </nav>
        <label className="steam-store-search-control">
          <input
            ref={searchRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search the store"
            aria-label="Search the store"
          />
          <Search size={18} />
        </label>
        <button
          type="button"
          className={wishlistOnly ? 'steam-store-wishlist is-active' : 'steam-store-wishlist'}
          onClick={() => setWishlistOnly((current) => !current)}
          aria-pressed={wishlistOnly}
        >
          <Star size={15} fill={wishlistOnly ? 'currentColor' : 'none'} /> Wishlist {wishlistGameIds.size}
        </button>
      </header>

      <div className="steam-store-scroll">
        {featured ? (
          <section id="steam-store-featured" className="steam-store-featured-section">
            <h1>Featured & recommended</h1>
            <div className="steam-store-featured-stage">
              <button type="button" className="steam-store-featured-arrow previous" onClick={() => changeFeatured(-1)} aria-label="Previous featured game">
                <ChevronLeft size={40} />
              </button>
              <button type="button" className="steam-store-featured-main" onClick={() => onSelectGame(featured.id)}>
                <SteamStoreArtwork game={featured} assets={assets} kind="hero" />
              </button>
              <button type="button" className="steam-store-featured-info" onClick={() => onSelectGame(featured.id)}>
                <strong>{featured.title}</strong>
                <span>{featured.subtitle || featured.developer}</span>
                <div className="steam-store-featured-thumbnails" aria-hidden="true">
                  {featuredGames.slice(0, 4).map((game) => (
                    <span key={game.id}><SteamStoreArtwork game={game} assets={assets} kind="hero" /></span>
                  ))}
                </div>
                <small>{ownedGameIds.has(featured.id) ? 'In your Library' : `Latest version ${featured.latestVersion}`}</small>
              </button>
              <button type="button" className="steam-store-featured-arrow next" onClick={() => changeFeatured(1)} aria-label="Next featured game">
                <ChevronRight size={40} />
              </button>
            </div>
            <div className="steam-store-featured-dots" aria-label="Choose featured game">
              {featuredGames.map((game, index) => (
                <button
                  key={game.id}
                  type="button"
                  className={index === featuredIndex ? 'is-active' : ''}
                  onClick={() => setFeaturedIndex(index)}
                  aria-label={`Show ${game.title}`}
                />
              ))}
            </div>
          </section>
        ) : (
          <section className="steam-store-empty">
            <Search size={28} />
            <strong>No games match these filters</strong>
            <button type="button" onClick={() => { setQuery(''); setActiveCategory(null); setWishlistOnly(false) }}>Clear filters</button>
          </section>
        )}

        {filteredGames.length > 0 ? (
          <>
            <section id="steam-store-categories" className="steam-store-section steam-store-category-section">
              <h2>Browse by category</h2>
              <div className="steam-store-category-track">
                <button type="button" className={!activeCategory ? 'is-active' : ''} onClick={() => setActiveCategory(null)}>
                  <Library size={20} /><span>All games</span>
                </button>
                {categories.map((category) => (
                  <button
                    key={category.id}
                    type="button"
                    className={activeCategory === category.id ? 'is-active' : ''}
                    onClick={() => setActiveCategory(category.id)}
                  >
                    <span>{category.label}</span><small>{category.count} games</small>
                  </button>
                ))}
              </div>
            </section>

            <section id="steam-store-recommended" className="steam-store-section">
              <h2>Recommended for you</h2>
              <div className="steam-store-card-track">
                {recommendationGames.map((game) => (
                  <button key={game.id} type="button" className="steam-store-landscape-card" onClick={() => onSelectGame(game.id)}>
                    <span><SteamStoreArtwork game={game} assets={assets} kind="hero" /></span>
                    <strong>{game.title}</strong>
                    <small>{ownedGameIds.has(game.id) ? 'In your Library' : game.developer}</small>
                  </button>
                ))}
              </div>
            </section>

            <section id="steam-store-updated" className="steam-store-section">
              <h2>New & updated</h2>
              <div className="steam-store-update-grid">
                {updatedGames.map((game) => (
                  <button key={game.id} type="button" className="steam-store-update-card" onClick={() => onSelectGame(game.id)}>
                    <span><SteamStoreArtwork game={game} assets={assets} kind="hero" /></span>
                    <div><strong>{game.title}</strong><small>{game.latestVersion}</small></div>
                  </button>
                ))}
              </div>
            </section>
          </>
        ) : null}
      </div>
    </main>
  )
}
