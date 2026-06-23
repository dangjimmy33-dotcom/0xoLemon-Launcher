import { useEffect, useMemo, useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import {
  Cloud,
  Download,
  Gamepad2,
  Heart,
  Play,
  RefreshCcw,
  ShieldCheck,
} from 'lucide-react'
import donateImage from '../assets/donate/donate.png'
import { formatBytes } from '../lib/format'
import { MOTION } from '../lib/motion'
import { DiscordWidget } from './DiscordWidget'
import type {
  GameCatalog,
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
  const installedGames = useMemo(
    () => catalog.games.filter((game) => installStates[game.id]?.installed),
    [catalog.games, installStates],
  )
  const runtimeByGame = useMemo(
    () => new Map(runtimeStates.map((runtime) => [runtime.gameId, runtime])),
    [runtimeStates],
  )
  const recentGames = useMemo(
    () =>
      [...installedGames].sort((left, right) => {
        const leftAt = runtimeByGame.get(left.id)?.lastPlayedAt ?? ''
        const rightAt = runtimeByGame.get(right.id)?.lastPlayedAt ?? ''
        return rightAt.localeCompare(leftAt)
      }),
    [installedGames, runtimeByGame],
  )
  const realtimeConfig = useRealtimeConfig()

  const featuredGames = useMemo(() => {
    if (!realtimeConfig.featuredGames || realtimeConfig.featuredGames.length === 0) return null
    return realtimeConfig.featuredGames
      .map((id) => catalog.games.find((g) => g.id === id))
      .filter((g): g is GameSummary => g != null)
  }, [realtimeConfig.featuredGames, catalog.games])

  const heroGames = featuredGames ?? (recentGames.length > 0 ? recentGames : installedGames)
  const [heroIndex, setHeroIndex] = useState(0)
  const [carouselPaused, setCarouselPaused] = useState(false)
  const resolvedHeroIndex = heroGames.length > 0 ? heroIndex % heroGames.length : 0
  const heroGame = heroGames[resolvedHeroIndex] ?? null

  useEffect(() => {
    if (!heroGame) return
    onRequestAsset(heroGame.id, heroGame.heroAssetId, true)
    onRequestAsset(heroGame.id, heroGame.logoAssetId, true)
    onRequestAsset(heroGame.id, heroGame.gridAssetId, true)
  }, [heroGame, onRequestAsset])

  useEffect(() => {
    for (const game of recentGames.slice(0, 8)) {
      onRequestAsset(game.id, game.gridAssetId)
      onRequestAsset(game.id, game.heroAssetId)
    }
  }, [onRequestAsset, recentGames])

  useEffect(() => {
    if (
      reducedMotion ||
      !preferences.carouselAutoplay ||
      carouselPaused ||
      heroGames.length < 2 ||
      document.visibilityState !== 'visible'
    ) {
      return
    }
    const timer = window.setInterval(() => {
      if (document.hasFocus()) {
        setHeroIndex((current) => (current + 1) % heroGames.length)
      }
    }, 8000)
    return () => window.clearInterval(timer)
  }, [carouselPaused, heroGames.length, preferences.carouselAutoplay, reducedMotion])

  const activeTask = job && !['committed', 'canceled', 'failed'].includes(job.status) ? job : null
  const updatePhase = launcherUpdateProgress?.phase

  return (
    <section className="premium-home">
      {preferences.showContinuePlaying ? (
        <motion.section
          className="home-hero"
          layout
          transition={MOTION.hero}
          onMouseEnter={() => setCarouselPaused(true)}
          onMouseLeave={() => setCarouselPaused(false)}
          onFocusCapture={() => setCarouselPaused(true)}
          onBlurCapture={() => setCarouselPaused(false)}
        >
          <AnimatePresence mode="wait">
            {heroGame ? (
              <motion.div
                key={heroGame.id}
                className="home-hero-slide"
                initial={reducedMotion ? false : { opacity: 0, x: 18 }}
                animate={{ opacity: 1, x: 0 }}
                exit={reducedMotion ? undefined : { opacity: 0, x: -18 }}
                transition={MOTION.hero}
              >
                {assets[heroGame.heroAssetId] ? (
                  <img className="home-hero-art" src={assets[heroGame.heroAssetId]} alt="" />
                ) : (
                  <div className="home-hero-art home-hero-art-placeholder" />
                )}
                <div className="home-hero-shade" />
                <div className="home-hero-copy">
                  <span className="home-overline">
                    {runtimeByGame.get(heroGame.id)?.lastPlayedAt ? 'Continue playing' : 'Ready to play'}
                  </span>
                  {assets[heroGame.logoAssetId] ? (
                    <img className="home-hero-logo" src={assets[heroGame.logoAssetId]} alt={heroGame.title} />
                  ) : (
                    <h2>{heroGame.title}</h2>
                  )}
                  <p>{heroGame.subtitle || heroGame.developer}</p>
                  <div className="home-hero-actions">
                    <button type="button" className="home-play-button" onClick={() => onPlayGame(heroGame.id)}>
                      <Play size={17} fill="currentColor" /> Play
                    </button>
                    <button type="button" className="home-secondary-button" onClick={() => onOpenGame(heroGame.id)}>
                      Game details
                    </button>
                  </div>
                  <div className="home-play-meta">
                    <Gamepad2 size={14} />
                    <span>{formatPlaytime(runtimeByGame.get(heroGame.id)?.totalPlaytimeSeconds ?? 0)}</span>
                    <ShieldCheck size={14} />
                    <span>Installed {installStates[heroGame.id]?.currentVersion}</span>
                  </div>
                </div>
                {heroGames.length > 1 ? (
                  <div className="home-carousel-controls" aria-label="Featured games">
                    {heroGames.map((game, index) => (
                      <button
                        key={game.id}
                        type="button"
                        className={index === resolvedHeroIndex ? 'is-active' : ''}
                        aria-label={`Show ${game.title}`}
                        aria-current={index === resolvedHeroIndex}
                        onClick={() => setHeroIndex(index)}
                      />
                    ))}
                  </div>
                ) : null}
              </motion.div>
            ) : (
              <motion.div className="home-hero-empty" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
                <Gamepad2 size={30} />
                <h2>Build your library</h2>
                <p>Installed games will appear here with playtime and recent activity.</p>
                <button type="button" className="home-play-button" onClick={() => onOpenTab('Store')}>
                  Browse Store
                </button>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.section>
      ) : null}

      {preferences.showRecentGames ? (
            <motion.section
              className="home-section reveal"
              initial={reducedMotion ? false : { opacity: 0, y: 12 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true, amount: 0.2 }}
            >
              <div className="home-section-heading reveal-clip">
                <h2>Recent games</h2>
                <span>{installedGames.length} installed</span>
              </div>
              {recentGames.length > 0 ? (
                <div className="home-game-rail stagger-children">
                  {recentGames.slice(0, 8).map((game) => (
                    <motion.button
                      type="button"
                      className="home-game-card reveal-scale"
                      key={game.id}
                      transition={MOTION.micro}
                      onClick={() => onOpenGame(game.id)}
                    >
                      {assets[game.gridAssetId] ? (
                        <img src={assets[game.gridAssetId]} alt="" />
                      ) : (
                        <div className="home-game-placeholder" />
                      )}
                      <span>
                        <strong>{game.title}</strong>
                        <small>{relativeLastPlayed(runtimeByGame.get(game.id)?.lastPlayedAt)}</small>
                      </span>
                    </motion.button>
                  ))}
                </div>
              ) : (
                <div className="home-inline-empty">Install a game from Store to see it here.</div>
              )}
            </motion.section>
          ) : null}
      <div className="home-bottom-cards">
          {preferences.showActiveTasks ? (
            <motion.section
              className="home-side-card active-task-card reveal-left"
              initial={reducedMotion ? false : { opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
            >
              <div className="home-side-card-title">
                <Download size={17} />
                <h2>Active tasks</h2>
              </div>
              {activeTask ? (
                <>
                  <strong>{titleCase(activeTask.kind)} · {activeTask.toVersion}</strong>
                  <span>{activeTask.phase}</span>
                  <div className="home-task-progress">
                    <i style={{ width: `${Math.max(2, Math.min(100, activeTask.overallProgress * 100))}%` }} />
                  </div>
                  <small>
                    {formatBytes(activeTask.bytesDone)} / {formatBytes(activeTask.bytesTotal)}
                  </small>
                  <button type="button" onClick={() => onOpenTab('Downloads')}>View Downloads</button>
                </>
              ) : launcherUpdate ? (
                <>
                  <strong>Launcher {launcherUpdate.version}</strong>
                  <span>{updatePhase ? titleCase(updatePhase) : 'Ready to download'}</span>
                  <button type="button" onClick={() => onOpenTab('Updates')}>View update</button>
                </>
              ) : (
                <div className="home-task-idle">
                  <RefreshCcw size={18} />
                  <span>No active downloads or updates.</span>
                </div>
              )}
            </motion.section>
          ) : null}

          {preferences.showDiscordCard ? (
            <DiscordWidget 
              serverId="1492076309323714570" 
              onOpenDiscord={onOpenDiscord} 
              reducedMotion={reducedMotion} 
            />
          ) : null}

          {preferences.showDonateCard ? (
            <motion.section className="home-side-card donate-card reveal-left shimmer-card" whileHover={reducedMotion ? undefined : { y: -3 }}>
              <img src={donateImage} alt="" />
              <div>
                <h2>Support development</h2>
                <p>Keep the launcher independent and improving.</p>
              </div>
              <button type="button" onClick={onOpenDonate}>
                <Heart size={14} /> Donate
              </button>
            </motion.section>
          ) : null}

          <section className="home-side-card cloud-glance-card reveal-left shimmer-card">
            <Cloud size={18} />
            <div>
              <h2>Cloud Saves</h2>
              <p>Backups and conflicts are managed per installed game.</p>
            </div>
            <button type="button" onClick={() => onOpenTab('Cloud Saves')}>Open</button>
          </section>
      </div>
    </section>
  )
}

function formatPlaytime(seconds: number) {
  if (seconds <= 0) return 'Not played yet'
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return hours > 0 ? `${hours}h ${minutes}m played` : `${Math.max(1, minutes)}m played`
}

function relativeLastPlayed(value: string | null | undefined) {
  if (!value) return 'Ready to play'
  const elapsed = Date.now() - new Date(value).getTime()
  const days = Math.floor(elapsed / 86_400_000)
  if (days <= 0) return 'Played today'
  if (days === 1) return 'Played yesterday'
  return `Played ${days} days ago`
}

function titleCase(value: string) {
  return value
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}
