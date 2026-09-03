import { useEffect, useMemo, useState } from 'react'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import {
  Activity,
  BellRing,
  ChevronRight,
  Cloud,
  Download,
  Gamepad2,
  HeartHandshake,
  ImagePlus,
  Library,
  MessageCircle,
  Newspaper,
  Pin,
  Play,
  RotateCcw,
  SlidersHorizontal,
  Sparkles,
  UserRound,
  Wifi,
  X,
} from 'lucide-react'
import { assetUrlForId } from '../../lib/gameMeta'
import { isTauriRuntime } from '../../lib/tauriRuntime'
import type { HomeWallpaperPreference } from '../../types'
import type { HomeViewProps } from '../../components/HomeView'
import './DefaultHomeView.css'

const WALLPAPER_STORAGE_KEY = '0xolemon.default.home.wallpaper.v1'

type HomeWallpaperAsset = {
  assetId: string
  filePath: string
  width: number
  height: number
  sizeBytes: number
}

type SheetKind = 'news' | 'discover' | 'stats'

type WallpaperLayers = {
  visible: string | null
  previous: string | null
}

function loadWallpaperPreference(): HomeWallpaperPreference {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(WALLPAPER_STORAGE_KEY) ?? 'null') as Partial<HomeWallpaperPreference> | null
    if (parsed?.kind === 'featured') return { kind: 'featured', assetId: typeof parsed.assetId === 'string' ? parsed.assetId : undefined }
    if ((parsed?.kind === 'pinned' || parsed?.kind === 'custom') && typeof parsed.assetId === 'string' && parsed.assetId.length > 0) {
      return { kind: parsed.kind, assetId: parsed.assetId }
    }
  } catch {
    // Invalid preferences safely fall back to the current featured game.
  }
  return { kind: 'featured' }
}

function saveWallpaperPreference(preference: HomeWallpaperPreference) {
  window.localStorage.setItem(WALLPAPER_STORAGE_KEY, JSON.stringify(preference))
}

export default function DefaultHomeView({
  catalog,
  installStates,
  runtimeStates,
  assets,
  job,
  launcherUpdate,
  onRequestAsset,
  onOpenGame,
  onPlayGame,
  onOpenTab,
  onOpenDiscord,
  onOpenDonate,
  displayName,
  online = true,
  preferences,
  reducedMotion,
}: HomeViewProps) {
  const [now, setNow] = useState(() => new Date())
  const [preference, setPreference] = useState<HomeWallpaperPreference>(loadWallpaperPreference)
  const [featuredIndex, setFeaturedIndex] = useState(0)
  const [customAsset, setCustomAsset] = useState<HomeWallpaperAsset | null>(null)
  const [sheet, setSheet] = useState<SheetKind | null>(null)
  const [wallpaperError, setWallpaperError] = useState<string | null>(null)
  const [customizeOpen, setCustomizeOpen] = useState(false)
  const [failedWallpaperUrls, setFailedWallpaperUrls] = useState<string[]>([])
  const [wallpaperLayers, setWallpaperLayers] = useState<WallpaperLayers>({ visible: null, previous: null })

  const runtimeByGame = useMemo(() => new Map(runtimeStates.map((state) => [state.gameId, state])), [runtimeStates])
  const installedGames = useMemo(() => catalog.games.filter((game) => installStates[game.id]?.installed), [catalog.games, installStates])
  const recentGames = useMemo(() => [...installedGames].sort((left, right) => {
    const leftAt = runtimeByGame.get(left.id)?.lastPlayedAt ?? ''
    const rightAt = runtimeByGame.get(right.id)?.lastPlayedAt ?? ''
    return rightAt.localeCompare(leftAt)
  }), [installedGames, runtimeByGame])
  const featuredGames = useMemo(() => {
    const seen = new Set<string>()
    return [...recentGames, ...catalog.games].filter((game) => {
      if (seen.has(game.id)) return false
      seen.add(game.id)
      return true
    }).slice(0, 8)
  }, [catalog.games, recentGames])
  const pinnedGame = preference.kind === 'pinned' ? catalog.games.find((game) => game.id === preference.assetId) ?? null : null
  const featuredGame = featuredGames[featuredIndex % Math.max(1, featuredGames.length)] ?? null
  const wallpaperGame = pinnedGame ?? featuredGame
  const preferredWallpaperUrl = preference.kind === 'custom'
    ? customAsset ? convertFileSrc(customAsset.filePath) : null
    : wallpaperGame ? assetUrlForId(wallpaperGame.heroAssetId, assets) : null
  const fallbackWallpaperUrl = preference.kind === 'custom' && wallpaperGame
    ? assetUrlForId(wallpaperGame.heroAssetId, assets)
    : null
  const wallpaperCandidate = [preferredWallpaperUrl, fallbackWallpaperUrl]
    .find((url): url is string => Boolean(url && !failedWallpaperUrls.includes(url))) ?? null
  const totalHours = Math.floor(runtimeStates.reduce((total, state) => total + (state.totalPlaytimeSeconds ?? 0), 0) / 3600)
  const activeJob = job && !['committed', 'canceled', 'failed'].includes(job.status) ? job : null
  const desktopWallpaperAvailable = isTauriRuntime()
  const wallpaperModeLabel = preference.kind === 'custom'
    ? 'Custom photo'
    : preference.kind === 'pinned'
      ? `Pinned · ${wallpaperGame?.title ?? 'Game artwork'}`
      : 'Featured rotation'

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1_000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    if (preference.kind !== 'custom' || !isTauriRuntime()) return
    let disposed = false
    void invoke<HomeWallpaperAsset>('get_home_wallpaper_asset', { assetId: preference.assetId })
      .then((asset) => { if (!disposed) setCustomAsset(asset) })
      .catch(() => {
        if (disposed) return
        setWallpaperError('The saved custom wallpaper is unavailable. Restored the default background.')
        setPreference({ kind: 'featured' })
        window.localStorage.removeItem(WALLPAPER_STORAGE_KEY)
      })
    return () => { disposed = true }
  }, [preference])

  useEffect(() => {
    if (!wallpaperGame || preference.kind === 'custom') return
    onRequestAsset(wallpaperGame.id, wallpaperGame.heroAssetId, true)
  }, [onRequestAsset, preference.kind, wallpaperGame])

  useEffect(() => {
    if (preference.kind !== 'featured' || reducedMotion || featuredGames.length < 2) return
    const timer = window.setInterval(() => setFeaturedIndex((value) => (value + 1) % featuredGames.length), 12_000)
    return () => window.clearInterval(timer)
  }, [featuredGames.length, preference.kind, reducedMotion])

  useEffect(() => {
    if (preference.kind !== 'featured' || featuredGames.length < 2) return
    const next = featuredGames[(featuredIndex + 1) % featuredGames.length]
    const url = assetUrlForId(next.heroAssetId, assets)
    if (!url) onRequestAsset(next.id, next.heroAssetId)
    else {
      const preload = new Image()
      preload.decoding = 'async'
      preload.src = url
    }
  }, [assets, featuredGames, featuredIndex, onRequestAsset, preference.kind])

  useEffect(() => {
    if (!wallpaperCandidate || wallpaperCandidate === wallpaperLayers.visible) return

    let disposed = false
    const preload = new Image()
    preload.decoding = 'async'
    preload.onload = () => {
      if (disposed) return
      setWallpaperLayers((previous) => previous.visible === wallpaperCandidate
        ? previous
        : { visible: wallpaperCandidate, previous: previous.visible })
    }
    preload.onerror = () => {
      if (disposed) return
      setFailedWallpaperUrls((failed) => failed.includes(wallpaperCandidate) ? failed : [...failed, wallpaperCandidate])
      if (preference.kind === 'custom' && wallpaperCandidate === preferredWallpaperUrl) {
        setWallpaperError('The custom wallpaper could not be displayed. Showing the featured background instead.')
      }
    }
    preload.src = wallpaperCandidate

    return () => {
      disposed = true
      preload.onload = null
      preload.onerror = null
    }
  }, [preference.kind, preferredWallpaperUrl, wallpaperCandidate, wallpaperLayers.visible])

  useEffect(() => {
    if (!wallpaperLayers.previous) return
    const timer = window.setTimeout(() => {
      setWallpaperLayers((layers) => ({ ...layers, previous: null }))
    }, reducedMotion ? 0 : 700)
    return () => window.clearTimeout(timer)
  }, [reducedMotion, wallpaperLayers.previous])

  const updatePreference = (next: HomeWallpaperPreference) => {
    setPreference(next)
    saveWallpaperPreference(next)
  }

  const chooseCustomWallpaper = async () => {
    if (!desktopWallpaperAvailable) return
    setWallpaperError(null)
    try {
      const asset = await invoke<HomeWallpaperAsset | null>('pick_home_wallpaper')
      if (!asset) return
      setCustomAsset(asset)
      updatePreference({ kind: 'custom', assetId: asset.assetId })
    } catch (cause) {
      setWallpaperError(String(cause))
    }
  }

  const resetWallpaper = () => {
    setPreference({ kind: 'featured' })
    setCustomAsset(null)
    setWallpaperError(null)
    setCustomizeOpen(false)
    window.localStorage.removeItem(WALLPAPER_STORAGE_KEY)
  }

  const closeCustomizerAfter = (action: () => void) => {
    action()
    setCustomizeOpen(false)
  }

  return (
    <main className="default-ipados-home">
      <div className="default-home-wallpaper" aria-hidden="true">
        {wallpaperLayers.previous ? <img className="is-outgoing" src={wallpaperLayers.previous} alt="" decoding="async" /> : null}
        {wallpaperLayers.visible ? <img className="is-current" src={wallpaperLayers.visible} alt="" decoding="async" fetchPriority="high" /> : null}
        <div />
      </div>

      <header className="default-lock-header">
        <div className="default-lock-clock">
          <span>{now.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })}</span>
          <strong>{now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false })}</strong>
        </div>
        <div
          className="default-wallpaper-customizer"
          onMouseEnter={() => setCustomizeOpen(true)}
          onMouseLeave={() => setCustomizeOpen(false)}
          onBlurCapture={(event) => {
            if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) {
              setCustomizeOpen(false)
            }
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              setCustomizeOpen(false)
            }
          }}
        >
          <button
            type="button"
            className="default-wallpaper-trigger"
            aria-haspopup="menu"
            aria-expanded={customizeOpen}
            onClick={() => setCustomizeOpen(true)}
          >
            <SlidersHorizontal />
            <span>Customize</span>
          </button>
          {customizeOpen ? (
            <div className="default-wallpaper-menu" role="menu" aria-label="Customize Home background">
              <div className="default-wallpaper-menu-status">
                <Sparkles />
                <span><small>BACKGROUND</small><strong>{wallpaperModeLabel}</strong></span>
              </div>
              <button
                type="button"
                role="menuitem"
                disabled={!desktopWallpaperAvailable}
                title={!desktopWallpaperAvailable ? 'Custom wallpapers require the desktop launcher.' : undefined}
                onClick={() => {
                  setCustomizeOpen(false)
                  void chooseCustomWallpaper()
                }}
              >
                <ImagePlus />
                <span><strong>Choose photo</strong><small>{desktopWallpaperAvailable ? 'PNG, JPEG or WebP' : 'Desktop launcher only'}</small></span>
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!wallpaperGame || preference.kind === 'pinned'}
                onClick={() => {
                  if (!wallpaperGame) return
                  closeCustomizerAfter(() => updatePreference({ kind: 'pinned', assetId: wallpaperGame.id }))
                }}
              >
                <Pin />
                <span><strong>{preference.kind === 'pinned' ? 'Game artwork pinned' : 'Pin game artwork'}</strong><small>{wallpaperGame?.title ?? 'No featured game available'}</small></span>
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={preference.kind === 'featured'}
                onClick={resetWallpaper}
              >
                <RotateCcw />
                <span><strong>Reset to default</strong><small>Return to featured rotation</small></span>
              </button>
            </div>
          ) : null}
        </div>
      </header>

      {wallpaperError ? <div className="default-wallpaper-error"><span>{wallpaperError}</span><button type="button" onClick={() => setWallpaperError(null)}><X /></button></div> : null}

      <section className="default-system-widgets" aria-label="Launcher status widgets">
        <button type="button" onClick={() => onOpenTab('Social')}><UserRound /><span>User</span><strong>{displayName || 'Local Player'}</strong><small>{online ? 'Connected' : 'Offline mode'}</small></button>
        <button type="button" onClick={() => onOpenTab('Downloads')}><Download /><span>Download</span><strong>{activeJob ? `${Math.round((activeJob.overallProgress ?? 0) * 100)}%` : 'Idle'}</strong><small>{activeJob ? activeJob.gameId : 'No active transfer'}</small></button>
        <button type="button" onClick={() => onOpenTab('Social')}><Wifi /><span>Online</span><strong>{online ? 'Available' : 'Offline'}</strong><small>{online ? 'Services connected' : 'Local features only'}</small></button>
        <button
          type="button"
          onClick={() => onOpenTab('Library')}
          onDragOver={(e) => {
            e.preventDefault()
            e.dataTransfer.dropEffect = 'copy'
          }}
          onDrop={(e) => {
            e.preventDefault()
            const gameId = e.dataTransfer.getData('application/0xo-game-id') || e.dataTransfer.getData('text/plain')
            if (gameId) {
              window.dispatchEvent(new CustomEvent('0xo-add-to-library', { detail: { gameId } }))
            }
          }}
        >
          <Library /><span>Library</span><strong>{installedGames.length}</strong><small>{catalog.games.length} games available</small>
        </button>
        <button type="button" onClick={() => onOpenTab('Updates')}><BellRing /><span>Update</span><strong>{launcherUpdate ? 'Ready' : 'Current'}</strong><small>{launcherUpdate ? `Version ${launcherUpdate.version}` : 'Launcher is up to date'}</small></button>
      </section>

      <section className="default-featured-action" aria-live="polite">
        {wallpaperGame ? (
          <>
            <span>{installStates[wallpaperGame.id]?.installed ? 'CONTINUE PLAYING' : 'FEATURED GAME'}</span>
            <h1>{wallpaperGame.title}</h1>
            <p>{wallpaperGame.subtitle || wallpaperGame.developer}</p>
            <div>
              <button type="button" className="is-primary" onClick={() => onPlayGame(wallpaperGame.id)}><Play />{installStates[wallpaperGame.id]?.installed ? 'Play' : 'View & Install'}</button>
              <button type="button" onClick={() => onOpenGame(wallpaperGame.id)}>Details <ChevronRight /></button>
            </div>
          </>
        ) : <><span>WELCOME</span><h1>Your launcher is ready</h1><button type="button" onClick={() => onOpenTab('Store')}>Browse Store</button></>}
      </section>

      <aside className="default-context-widgets" aria-label="Home content">
        <button type="button" onClick={() => setSheet('news')}><Newspaper /><span>News</span><small>Updates and active tasks</small></button>
        <button type="button" onClick={() => setSheet('discover')}><Sparkles /><span>Discover</span><small>Recommendations</small></button>
        <button type="button" onClick={() => setSheet('stats')}><Activity /><span>Stats</span><small>{totalHours} hours played</small></button>
        <button type="button" onClick={() => onOpenTab('CloudRedirect')}><Cloud /><span>Cloud</span><small>Save protection</small></button>
        {preferences.showDiscordCard ? <button type="button" onClick={onOpenDiscord}><MessageCircle /><span>Discord</span><small>Join the server</small></button> : null}
        {preferences.showDonateCard ? <button type="button" onClick={onOpenDonate}><HeartHandshake /><span>Support</span><small>Donate to 0xoLemon</small></button> : null}
      </aside>

      <section className="default-home-dock" aria-label="Continue Playing">
        <span>Continue Playing</span>
        <div>
          {(recentGames.length > 0 ? recentGames : catalog.games).slice(0, 8).map((game) => {
            const icon = assetUrlForId(game.iconAssetId, assets) || assetUrlForId(game.gridAssetId, assets)
            return <button key={game.id} type="button" aria-label={game.title} onClick={() => onOpenGame(game.id)} onDoubleClick={() => onPlayGame(game.id)}>{icon ? <img src={icon} alt="" loading="lazy" decoding="async" /> : <Gamepad2 />}<small>{game.title}</small></button>
          })}
          <button type="button" aria-label="Open Library" onClick={() => onOpenTab('Library')}><Library /><small>Library</small></button>
        </div>
      </section>

      {sheet ? (
        <section className="default-home-sheet" aria-modal="true" role="dialog" aria-label={`${sheet} sheet`}>
          <button className="default-sheet-close" type="button" onClick={() => setSheet(null)} aria-label="Close"><X /></button>
          {sheet === 'news' ? <><span>NEWS & UPDATES</span><h2>{activeJob ? 'A transfer is in progress' : launcherUpdate ? `Version ${launcherUpdate.version} is ready` : 'Everything is current'}</h2><p>{activeJob ? `${activeJob.gameId} · ${Math.round((activeJob.overallProgress ?? 0) * 100)}% complete` : launcherUpdate ? 'Open Updates to review and install the launcher update.' : 'There are no active downloads or launcher updates.'}</p><button type="button" onClick={() => onOpenTab(activeJob ? 'Downloads' : 'Updates')}>Open status <ChevronRight /></button></> : null}
          {sheet === 'discover' ? <><span>RECOMMENDED</span><h2>Discover something next</h2><div className="default-sheet-games">{featuredGames.slice(0, 5).map((game) => <button type="button" key={game.id} onClick={() => onOpenGame(game.id)}>{assetUrlForId(game.gridAssetId, assets) ? <img src={assetUrlForId(game.gridAssetId, assets)} alt="" loading="lazy" /> : <Gamepad2 />}<strong>{game.title}</strong></button>)}</div></> : null}
          {sheet === 'stats' ? <><span>YOUR LIBRARY</span><h2>{totalHours.toLocaleString()} hours played</h2><div className="default-sheet-stats"><div><strong>{installedGames.length}</strong><small>Installed</small></div><div><strong>{catalog.games.length}</strong><small>Available</small></div><div><strong>{recentGames.length}</strong><small>Played</small></div></div></> : null}
        </section>
      ) : null}
    </main>
  )
}
