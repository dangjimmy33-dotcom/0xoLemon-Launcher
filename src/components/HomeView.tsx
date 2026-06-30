import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { gameHasTag } from '../lib/gameTags'
import { firstMediaUrl } from '../lib/gameMeta'
import { DiscordWidget } from './DiscordWidget'
import '../home-view.css'
import type {
  GameCatalog,
  GameDetail,
  GameInstallState,
  GameRuntimeState,
  GameSummary,
  JobJournal,
  LauncherUpdateInfo,
  LauncherUpdateProgress,
  TabId,
} from '../types'
import { useRealtimeConfig } from '../hooks/useRealtimeConfig'

type HomePreferences = {
  showContinuePlaying: boolean
  showRecentGames: boolean
  showActiveTasks: boolean
  showDiscordCard: boolean
  showDonateCard: boolean
  carouselAutoplay: boolean
}

export function HomeView({
  catalog,
  installStates,
  runtimeStates,
  assets,
  job,
  launcherUpdate,
  launcherUpdateProgress,
  preferences,
  reducedMotion,
  onRequestAsset,
  onOpenGame,
  onPlayGame,
  onOpenTab,
  onOpenDiscord,
  onOpenDonate,
}: {
  catalog: GameCatalog
  installStates: Record<string, GameInstallState>
  runtimeStates: GameRuntimeState[]
  assets: Record<string, string>
  job: JobJournal | null
  launcherUpdate: LauncherUpdateInfo | null
  launcherUpdateProgress: LauncherUpdateProgress | null
  preferences: HomePreferences
  reducedMotion: boolean
  onRequestAsset: (gameId: string, assetId: string, urgent?: boolean) => void
  onOpenGame: (gameId: string) => void
  onPlayGame: (gameId: string) => void
  onOpenTab: (tab: TabId) => void
  onOpenDiscord: () => void
  onOpenDonate: () => void
}) {
  // ── data ──────────────────────────────────────────────────────
  const installedGames = useMemo(
    () => catalog.games.filter((g) => installStates[g.id]?.installed),
    [catalog.games, installStates],
  )
  const runtimeByGame = useMemo(
    () => new Map(runtimeStates.map((r) => [r.gameId, r])),
    [runtimeStates],
  )
  const recentGames = useMemo(
    () =>
      [...installedGames].sort((a, b) => {
        const aAt = runtimeByGame.get(a.id)?.lastPlayedAt ?? ''
        const bAt = runtimeByGame.get(b.id)?.lastPlayedAt ?? ''
        return bAt.localeCompare(aAt)
      }),
    [installedGames, runtimeByGame],
  )

  const realtimeConfig = useRealtimeConfig()
  const featuredGames = useMemo(() => {
    if (!realtimeConfig.featuredGames?.length) return null
    return realtimeConfig.featuredGames
      .map((id) => catalog.games.find((g) => g.id === id))
      .filter((g): g is GameSummary => g != null)
  }, [realtimeConfig.featuredGames, catalog.games])

  const heroGames = useMemo(() => {
    if (featuredGames && featuredGames.length > 0)
      return featuredGames.filter((g) => !gameHasTag(g, 'coming soon'))

    const seen = new Set<string>()
    const mix: GameSummary[] = []
    for (const g of recentGames) {
      if (gameHasTag(g, 'coming soon')) continue
      if (seen.size >= 3) break
      mix.push(g); seen.add(g.id)
    }
    const random = [...catalog.games].sort(() => 0.5 - Math.random())
    for (const g of random) {
      if (gameHasTag(g, 'coming soon')) continue
      if (mix.length >= 6) break
      if (!seen.has(g.id)) { mix.push(g); seen.add(g.id) }
    }
    return mix
  }, [featuredGames, recentGames, catalog.games])

  // ── hero carousel state ───────────────────────────────────────
  const [heroIndex, setHeroIndex] = useState(0)
  const resolvedIdx = heroGames.length > 0 ? heroIndex % heroGames.length : 0
  const heroGame = heroGames[resolvedIdx] ?? null
  const [heroDetail, setHeroDetail] = useState<GameDetail | null>(null)

  const isComingSoon = heroGame ? gameHasTag(heroGame, 'coming soon') : false
  const fallbackHeroUrl = heroDetail ? firstMediaUrl(heroDetail, assets) : null

  // pause on hover
  const [paused, setPaused] = useState(false)

  // ── asset loading ─────────────────────────────────────────────
  useEffect(() => {
    if (!heroGame) { setHeroDetail(null); return }
    onRequestAsset(heroGame.id, heroGame.heroAssetId, true)
    onRequestAsset(heroGame.id, heroGame.logoAssetId, true)
    onRequestAsset(heroGame.id, heroGame.gridAssetId, true)
    if (!heroGame.heroAssetId || !assets[heroGame.heroAssetId]) {
      invoke<GameDetail>('get_game_detail', { gameId: heroGame.id, locale: 'en-US' })
        .then((d) => { setHeroDetail(d); d.media?.forEach((m) => onRequestAsset(heroGame.id, m.assetId)) })
        .catch(() => setHeroDetail(null))
    } else {
      setHeroDetail(null)
    }
  }, [heroGame?.id]) // eslint-disable-line

  useEffect(() => {
    for (const g of heroGames) {
      onRequestAsset(g.id, g.iconAssetId, true)
      onRequestAsset(g.id, g.gridAssetId, true)
      onRequestAsset(g.id, g.heroAssetId)
      onRequestAsset(g.id, g.logoAssetId)
    }
  }, [heroGames]) // eslint-disable-line

  // ── autoplay ──────────────────────────────────────────────────
  useEffect(() => {
    if (reducedMotion || !preferences.carouselAutoplay || heroGames.length < 2 || paused) return
    const t = window.setInterval(() => {
      if (document.hasFocus()) setHeroIndex((i) => (i + 1) % heroGames.length)
    }, 6000)
    return () => window.clearInterval(t)
  }, [heroGames.length, preferences.carouselAutoplay, reducedMotion, paused])

  // ── scroll reveal for rec/news cards ─────────────────────────
  const mainRef = useRef<HTMLElement>(null)
  useEffect(() => {
    const root = mainRef.current
    if (!root) return
    const cards = root.querySelectorAll<HTMLElement>('.hv-rec-card, .hv-news-card')
    cards.forEach((el, i) => {
      if (!el.style.transitionDelay) el.style.transitionDelay = `${i * 55}ms`
    })
    const io = new IntersectionObserver(
      (entries) => entries.forEach((e) => { if (e.isIntersecting) { (e.target as HTMLElement).classList.add('in-view'); io.unobserve(e.target) } }),
      { threshold: 0.12 },
    )
    cards.forEach((el) => io.observe(el))
    return () => io.disconnect()
  })

  // ── stats ─────────────────────────────────────────────────────
  const totalHours = useMemo(() => {
    const secs = runtimeStates.reduce((sum, r) => sum + (r.totalPlaytimeSeconds ?? 0), 0)
    return Math.floor(secs / 3600)
  }, [runtimeStates])

  const activeTask = job && !['committed', 'canceled', 'failed'].includes(job.status) ? job : null
  const updatePhase = launcherUpdateProgress?.phase

  // ── poster gradient fallback ──────────────────────────────────
  const POSTERS = ['hv-poster-1','hv-poster-2','hv-poster-3','hv-poster-4','hv-poster-5','hv-poster-6','hv-poster-7','hv-poster-8']

  // ── render ────────────────────────────────────────────────────
  return (
    <main className="home-view" ref={mainRef}>

      {/* ═══════════════ HERO CAROUSEL ═══════════════ */}
      <section
        className="hv-hero"
        onMouseEnter={() => setPaused(true)}
        onMouseLeave={() => setPaused(false)}
      >
        {heroGames.length > 0 ? heroGames.map((game, i) => {
          const isActive = i === resolvedIdx
          const bgUrl = assets[game.heroAssetId] || (isActive ? fallbackHeroUrl : null)
          const logoUrl = assets[game.logoAssetId]
          const runtime = runtimeByGame.get(game.id)
          const installed = installStates[game.id]?.installed
          const tag = runtime?.lastPlayedAt
            ? '▶ CONTINUE PLAYING'
            : installed
              ? '✓ INSTALLED'
              : '★ FEATURED'
          
          // Capture game.id to avoid stale closure
          const currentGameId = game.id;

          return (
            <div key={game.id} className={`hv-hero-slide${isActive ? ' active' : ''}`} style={{ pointerEvents: isActive ? undefined : 'none' }}>
              <div
                className="hv-hero-bg"
                style={!bgUrl ? { background: getGradientFromTitle(game.title) } : undefined}
              >
                {bgUrl && <img src={bgUrl} alt="" />}
              </div>
              <div className="hv-hero-shade" />
              <div className="hv-hero-content">
                <span className="hv-hero-tag">{tag}</span>
                {logoUrl ? (
                  <img className="hv-hero-logo" src={logoUrl} alt={game.title} />
                ) : (
                  <h1 className="hv-hero-title">{game.title}</h1>
                )}
                <div className="hv-hero-meta">
                  {game.developer && <span>{game.developer}</span>}
                  {game.developer && runtime?.lastPlayedAt && <span className="dot">•</span>}
                  {runtime?.lastPlayedAt && (
                    <span>{formatPlaytime(runtime.totalPlaytimeSeconds ?? 0)}</span>
                  )}
                  {installed && installStates[game.id]?.currentVersion && (
                    <><span className="dot">•</span><span>v{installStates[game.id].currentVersion}</span></>
                  )}
                </div>
                <div className="hv-hero-actions">
                  {!isComingSoon ? (
                    <>
                      <button className="hv-btn hv-btn-primary" onClick={() => onPlayGame(currentGameId)}>
                        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>
                        {installed ? 'Play Now' : 'View Game'}
                      </button>
                      <button className="hv-btn hv-btn-ghost" onClick={() => onOpenGame(currentGameId)}>
                        Details
                      </button>
                    </>
                  ) : (
                    <button className="hv-btn hv-btn-ghost" style={{ opacity: .5, cursor: 'not-allowed' }} disabled>
                      Coming Soon
                    </button>
                  )}
                </div>
              </div>
            </div>
          )
        }) : (
          /* Empty state */
          <div className="hv-hero-slide active">
            <div className="hv-hero-bg" style={{ background: 'linear-gradient(135deg,#1a1a2e,#0a0a0f)' }} />
            <div className="hv-hero-shade" />
            <div className="hv-hero-content">
              <span className="hv-hero-tag">Welcome</span>
              <h1 className="hv-hero-title">Your launcher is ready</h1>
              <div className="hv-hero-meta"><span>Start by browsing the store</span></div>
              <div className="hv-hero-actions">
                <button className="hv-btn hv-btn-primary" onClick={() => onOpenTab('Store')}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M4 8l1.5-4h13L20 8"/><path d="M4 8h16v11a1 1 0 01-1 1H5a1 1 0 01-1-1V8z"/><path d="M9 12a3 3 0 006 0"/></svg>
                  Browse Store
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Tick indicators */}
        {heroGames.length > 1 && (
          <div className="hv-hero-ticks">
            {heroGames.map((g, i) => (
              <button
                key={g.id}
                className={`hv-hero-tick${i === resolvedIdx ? ' active' : ''}`}
                onClick={() => { setHeroIndex(i); setPaused(true) }}
                aria-label={`Slide ${i + 1}`}
              />
            ))}
          </div>
        )}
      </section>

      {/* ═══════════════ CONTINUE PLAYING ═══════════════ */}
      {recentGames.length > 0 && preferences.showContinuePlaying && (
        <section className="hv-section">
          <div className="hv-section-head">
            <h2>Continue Playing</h2>
            <button className="hv-see-all" onClick={() => onOpenTab('Library')}>See all →</button>
          </div>
          <div className="hv-row-scroll">
            {recentGames.slice(0, 8).map((game, idx) => {
              const art = assets[game.gridAssetId] || assets[game.heroAssetId]
              const runtime = runtimeByGame.get(game.id)
              const state = installStates[game.id]
              // determine chip from active job
              const gameHasActiveJob = activeTask?.gameId === game.id
              const chipClass = gameHasActiveJob
                ? (activeTask?.phase === 'update' ? 'hv-chip-update' : 'hv-chip-installing')
                : 'hv-chip-ready'
              const chipLabel = gameHasActiveJob
                ? (activeTask?.phase === 'update' ? 'UPDATE' : 'INSTALLING')
                : 'READY'

              return (
                <div
                  key={game.id}
                  className="hv-cp-card"
                  onClick={() => onOpenGame(game.id)}
                  onDoubleClick={() => onPlayGame(game.id)}
                >
                  <div className={`hv-cp-art ${art ? '' : POSTERS[idx % POSTERS.length]}`}>
                    {art && <img src={art} alt={game.title} />}
                    <span className={`hv-cp-chip ${chipClass}`}>{chipLabel}</span>
                  </div>
                  <div className="hv-cp-body">
                    <div className="hv-cp-title">{game.title}</div>
                    <div className="hv-cp-sub">
                      {runtime?.lastPlayedAt
                        ? formatPlaytime(runtime.totalPlaytimeSeconds ?? 0)
                        : 'Not played yet'}
                    </div>
                    <div className="hv-progress-track">
                      <div className="hv-progress-fill" style={{ width: '100%' }} />
                    </div>
                    <div className="hv-cp-foot">
                      <span>v{state?.currentVersion ?? '—'}</span>
                      <span>{game.developer ?? ''}</span>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </section>
      )}

      {/* ═══════════════ STATS ═══════════════ */}
      <section className="hv-section">
        <div className="hv-stats">
          <div className="hv-stat-card">
            <div className="hv-stat-icon" style={{ background: 'var(--hv-amber-soft)', color: 'var(--hv-amber)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 3"/>
              </svg>
            </div>
            <div className="hv-stat-value">{totalHours.toLocaleString()}</div>
            <div className="hv-stat-label">Hours Played</div>
          </div>
          <div className="hv-stat-card">
            <div className="hv-stat-icon" style={{ background: 'var(--hv-teal-soft)', color: 'var(--hv-teal)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="4" width="7" height="16" rx="1.5"/><rect x="14" y="4" width="7" height="16" rx="1.5"/>
              </svg>
            </div>
            <div className="hv-stat-value">{installedGames.length}</div>
            <div className="hv-stat-label">Games Installed</div>
          </div>
          <div className="hv-stat-card">
            <div className="hv-stat-icon" style={{ background: 'var(--hv-violet-soft)', color: 'var(--hv-violet)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 2l3 6 6 .9-4.5 4.3 1 6L12 16l-5.5 3.2 1-6L3 9.9 9 9z"/>
              </svg>
            </div>
            <div className="hv-stat-value">{catalog.games.length}</div>
            <div className="hv-stat-label">Games in Catalog</div>
          </div>
          <div className="hv-stat-card" style={{ cursor: 'pointer' }} onClick={() => onOpenTab('Downloads')}>
            <div className="hv-stat-icon" style={{ background: 'rgba(255,255,255,.07)', color: 'var(--hv-text-2)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 3v12m0 0l-4-4m4 4l4-4"/><path d="M4 19h16"/>
              </svg>
            </div>
            <div className="hv-stat-value" style={{ fontSize: activeTask ? '14px' : undefined, paddingTop: activeTask ? '4px' : undefined }}>
              {activeTask ? titleCase(activeTask.phase ?? activeTask.kind) : 'Idle'}
            </div>
            <div className="hv-stat-label">
              {activeTask
                ? `${Math.round((activeTask.overallProgress ?? 0) * 100)}% — ${titleCase(activeTask.kind)}`
                : launcherUpdate
                  ? `Update ${launcherUpdate.version} ready`
                  : 'No active downloads'}
            </div>
          </div>
        </div>
      </section>

      {/* ═══════════════ RECOMMENDED / ALL GAMES ═══════════════ */}
      {heroGames.length > 0 && (
        <section className="hv-section">
          <div className="hv-section-head">
            <h2>{recentGames.length > 0 ? 'Recommended' : 'Discover'}</h2>
            <button className="hv-see-all" onClick={() => onOpenTab('Store')}>Browse store →</button>
          </div>
          <div className="hv-grid-rec">
            {heroGames.slice(0, 8).map((game, idx) => {
              const art = assets[game.heroAssetId] || assets[game.gridAssetId]
              const tags = [game.developer].filter(Boolean).join(' · ')
              const currentGameId = game.id; // Capture game ID to avoid closure stale issue
              return (
                <div key={game.id} className="hv-rec-card" onClick={() => onOpenGame(currentGameId)}>
                  <div className={`hv-rec-art ${art ? '' : POSTERS[idx % POSTERS.length]}`}>
                    {art && <img src={art} alt={game.title} />}
                    <span className="hv-rec-rating">
                      <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l3 6 6 .9-4.5 4.3 1 6L12 16l-5.5 3.2 1-6L3 9.9 9 9z"/></svg>
                      {installStates[game.id]?.installed ? 'Installed' : 'Available'}
                    </span>
                    <div className="hv-rec-overlay">
                      <button
                        className="hv-icon-round"
                        title="Play"
                        onClick={(e) => { e.stopPropagation(); onPlayGame(currentGameId) }}
                      >
                        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>
                      </button>
                      <button
                        className="hv-icon-round ghost"
                        title="Details"
                        onClick={(e) => { e.stopPropagation(); onOpenGame(currentGameId) }}
                      >
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="9"/><path d="M12 8v4m0 4h.01"/></svg>
                      </button>
                    </div>
                  </div>
                  <div className="hv-rec-body">
                    <div className="hv-rec-title">{game.title}</div>
                    <div className="hv-rec-tags">{tags || game.subtitle || 'Game'}</div>
                  </div>
                </div>
              )
            })}
          </div>
        </section>
      )}

      {/* ═══════════════ NEWS / EVENTS ═══════════════ */}
      {(activeTask || launcherUpdate || preferences.showDiscordCard) && (
        <section className="hv-section">
          <div className="hv-section-head"><h2>News & Updates</h2></div>
          <div className="hv-news-grid">
            {activeTask && (
              <div className="hv-news-card" onClick={() => onOpenTab('Downloads')}>
                <span className="hv-news-tag hv-tag-patch">ACTIVE DOWNLOAD</span>
                <div className="hv-news-title">{titleCase(activeTask.kind)}</div>
                <div className="hv-news-excerpt">
                  {titleCase(activeTask.phase ?? 'Preparing')} — {Math.round((activeTask.overallProgress ?? 0) * 100)}% complete
                </div>
                <div className="hv-progress-track" style={{ marginTop: 4 }}>
                  <div className="hv-progress-fill" style={{ width: `${Math.round((activeTask.overallProgress ?? 0) * 100)}%` }} />
                </div>
              </div>
            )}
            {launcherUpdate && (
              <div className="hv-news-card" onClick={() => onOpenTab('Updates')}>
                <span className="hv-news-tag hv-tag-event">LAUNCHER UPDATE</span>
                <div className="hv-news-title">Version {launcherUpdate.version} Available</div>
                <div className="hv-news-excerpt">
                  {updatePhase ? titleCase(updatePhase) : 'A new version of the launcher is ready to install.'}
                </div>
                <div className="hv-news-time">Check updates tab →</div>
              </div>
            )}
            {preferences.showDiscordCard && (
              <div className="hv-news-card" onClick={onOpenDiscord}>
                <span className="hv-news-tag hv-tag-info">COMMUNITY</span>
                <div className="hv-news-title">Join the Discord</div>
                <div className="hv-news-excerpt">
                  Connect with other players, get support, and stay up to date on the latest launcher news.
                </div>
                <div className="hv-news-time">Open Discord →</div>
              </div>
            )}
            {preferences.showDonateCard && (
              <div className="hv-news-card" onClick={onOpenDonate}>
                <span className="hv-news-tag" style={{ background: 'rgba(255,93,108,.15)', color: '#ff5d6c' }}>SUPPORT</span>
                <div className="hv-news-title">Support the project</div>
                <div className="hv-news-excerpt">
                  Help keep the launcher improving. Every contribution makes a difference.
                </div>
                <div className="hv-news-time">Donate →</div>
              </div>
            )}
          </div>
        </section>
      )}

      {/* ═══════════════ DISCORD WIDGET ═══════════════ */}
      {preferences.showDiscordCard && (
        <div className="hv-discord-slot">
          <DiscordWidget
            serverId="1492076309323714570"
            onOpenDiscord={onOpenDiscord}
            reducedMotion={reducedMotion}
          />
        </div>
      )}

    </main>
  )
}

// ── helpers ─────────────────────────────────────────────────────
function getGradientFromTitle(title: string) {
  let hash = 0
  for (let i = 0; i < title.length; i++) hash = title.charCodeAt(i) + ((hash << 5) - hash)
  const hue = Math.abs(hash) % 360
  return `radial-gradient(120% 120% at 70% 20%, hsl(${hue},60%,18%), #0a0d14 65%)`
}

function formatPlaytime(seconds: number) {
  if (seconds <= 0) return 'Not played yet'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m}m played` : `${Math.max(1, m)}m played`
}

function titleCase(v: string) {
  return v.replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}
