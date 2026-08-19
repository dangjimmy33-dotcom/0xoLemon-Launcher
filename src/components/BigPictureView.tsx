import { useCallback, useEffect, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import { Bell, ChevronLeft, ChevronRight, Gamepad2, Monitor, Play, X } from 'lucide-react'
import type { GameSummary, NotificationRecord } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { NotificationPopover } from './NotificationCenter'
import './BigPictureView.css'

interface BigPictureViewProps {
  games: GameSummary[]
  assetUrls: Record<string, string>
  phase: 'entering' | 'active' | 'exiting'
  reducedMotion: boolean
  onExit: () => void
  onPlayGame: (gameId: string) => void
  notifications: NotificationRecord[]
  notificationOpen: boolean
  onToggleNotifications: () => void
  onCloseNotifications: () => void
  onOpenNotification: (n: NotificationRecord) => void
  onMarkAllNotificationsRead: () => void
  onClearNotifications: () => void
  onOpenNotificationSettings: () => void
}

function resolveHeroUrl(game: GameSummary, assets: Record<string, string>) {
  return (
    assetUrlForId(game.heroAssetId, assets) ||
    assetUrlForId(game.gridAssetId, assets) ||
    assetUrlForId(game.iconAssetId, assets)
  )
}

function resolveGridUrl(game: GameSummary, assets: Record<string, string>) {
  return assetUrlForId(game.gridAssetId, assets) || assetUrlForId(game.iconAssetId, assets)
}

function resolveLogoUrl(game: GameSummary, assets: Record<string, string>) {
  return assetUrlForId(game.logoAssetId, assets)
}

const AUTO_ADVANCE_DELAY = 6000
const GAMEPAD_DEAD_ZONE = 0.55

export function BigPictureView({
  games,
  assetUrls,
  phase,
  reducedMotion,
  onExit,
  onPlayGame,
  notifications,
  notificationOpen,
  onToggleNotifications,
  onCloseNotifications,
  onOpenNotification,
  onMarkAllNotificationsRead,
  onClearNotifications,
  onOpenNotificationSettings,
}: BigPictureViewProps) {
  const [now, setNow] = useState(new Date())
  const [activeIndex, setActiveIndex] = useState(0)
  const [showHelp, setShowHelp] = useState(false)
  const [gamepadConnected, setGamepadConnected] = useState(() => typeof navigator !== 'undefined' && Boolean(navigator.getGamepads?.().some(Boolean)))
  const trackRef = useRef<HTMLDivElement>(null)
  const autoTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastInteractionRef = useRef(0)

  const activeGame = games[activeIndex] || null
  const unread = notifications.filter((notification) => !notification.read).length
  const isInteractive = phase === 'active'

  useEffect(() => {
    lastInteractionRef.current = Date.now()
  }, [])

  useEffect(() => {
    if (activeIndex < games.length) return
    setActiveIndex(Math.max(0, games.length - 1))
  }, [activeIndex, games.length])

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    const centerActiveCard = () => {
      if (!trackRef.current) return
      const activeElement = trackRef.current.children[activeIndex] as HTMLElement | undefined
      if (!activeElement) return
      const containerWidth = trackRef.current.parentElement?.clientWidth ?? window.innerWidth
      const scrollLeft = activeElement.offsetLeft - containerWidth / 2 + activeElement.offsetWidth / 2
      trackRef.current.style.transform = `translate3d(${-scrollLeft}px, 0, 0)`
    }

    centerActiveCard()
    const animationFrame = window.requestAnimationFrame(centerActiveCard)
    window.addEventListener('resize', centerActiveCard)
    return () => {
      window.cancelAnimationFrame(animationFrame)
      window.removeEventListener('resize', centerActiveCard)
    }
  }, [activeIndex, phase])

  useEffect(() => {
    const updateGamepadState = () => setGamepadConnected(Boolean(navigator.getGamepads?.().some(Boolean)))
    window.addEventListener('gamepadconnected', updateGamepadState)
    window.addEventListener('gamepaddisconnected', updateGamepadState)
    updateGamepadState()
    return () => {
      window.removeEventListener('gamepadconnected', updateGamepadState)
      window.removeEventListener('gamepaddisconnected', updateGamepadState)
    }
  }, [])

  const scheduleAutoAdvance = useCallback(() => {
    if (autoTimerRef.current) window.clearTimeout(autoTimerRef.current)
    if (!isInteractive || notificationOpen || showHelp || games.length <= 1) return
    autoTimerRef.current = window.setTimeout(() => {
      if (Date.now() - lastInteractionRef.current >= AUTO_ADVANCE_DELAY - 50) {
        setActiveIndex((previous) => (previous + 1) % games.length)
      }
    }, AUTO_ADVANCE_DELAY)
  }, [games.length, isInteractive, notificationOpen, showHelp])

  useEffect(() => {
    scheduleAutoAdvance()
    return () => {
      if (autoTimerRef.current) window.clearTimeout(autoTimerRef.current)
    }
  }, [activeIndex, scheduleAutoAdvance])

  const markInteraction = useCallback(() => {
    lastInteractionRef.current = Date.now()
    scheduleAutoAdvance()
  }, [scheduleAutoAdvance])

  const goTo = useCallback((index: number) => {
    if (!games.length || !isInteractive) return
    markInteraction()
    setActiveIndex(Math.min(games.length - 1, Math.max(0, index)))
  }, [games.length, isInteractive, markInteraction])

  const goNext = useCallback(() => {
    if (!games.length || !isInteractive) return
    markInteraction()
    setActiveIndex((previous) => (previous + 1) % games.length)
  }, [games.length, isInteractive, markInteraction])

  const goPrev = useCallback(() => {
    if (!games.length || !isInteractive) return
    markInteraction()
    setActiveIndex((previous) => (previous - 1 + games.length) % games.length)
  }, [games.length, isInteractive, markInteraction])

  const launchActive = useCallback(() => {
    if (!activeGame || !isInteractive) return
    markInteraction()
    onPlayGame(activeGame.id)
  }, [activeGame, isInteractive, markInteraction, onPlayGame])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (notificationOpen) {
        if (event.key === 'Escape') onCloseNotifications()
        return
      }
      if (showHelp) {
        if (event.key === 'Escape' || event.key === '?') {
          event.preventDefault()
          setShowHelp(false)
        }
        return
      }
      if (!isInteractive && event.key !== 'Escape') return

      switch (event.key) {
        case 'ArrowRight':
        case 'Tab':
          event.preventDefault()
          goNext()
          break
        case 'ArrowLeft':
          event.preventDefault()
          goPrev()
          break
        case 'Enter':
        case ' ':
          event.preventDefault()
          launchActive()
          break
        case 'Escape':
          event.preventDefault()
          onExit()
          break
        case '?':
          event.preventDefault()
          setShowHelp(true)
          break
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [goNext, goPrev, isInteractive, launchActive, notificationOpen, onCloseNotifications, onExit, showHelp])

  useEffect(() => {
    const handlePointerMove = () => markInteraction()
    window.addEventListener('pointermove', handlePointerMove, { passive: true })
    return () => window.removeEventListener('pointermove', handlePointerMove)
  }, [markInteraction])

  // Controller-first input. Buttons are edge-triggered so holding A/B/LB/RB cannot repeat actions.
  useEffect(() => {
    let animationFrame = 0
    const previousAxes = new Map<string, number>()
    const previousButtons = new Map<string, boolean[]>()

    const poll = () => {
      const pads = navigator.getGamepads?.()
      if (pads) {
        for (const pad of pads) {
          if (!pad) continue
          const key = `${pad.index}:${pad.id}`
          const axisX = pad.axes[0] ?? 0
          const previousAxisX = previousAxes.get(key) ?? 0
          const currentButtons = pad.buttons.map((button) => button.pressed)
          const priorButtons = previousButtons.get(key) ?? []
          const pressedEdge = (buttonIndex: number) => Boolean(currentButtons[buttonIndex] && !priorButtons[buttonIndex])

          const overlayOpen = notificationOpen || showHelp
          if (isInteractive && !overlayOpen) {
            if ((axisX > GAMEPAD_DEAD_ZONE && previousAxisX <= GAMEPAD_DEAD_ZONE) || pressedEdge(15)) goNext()
            if ((axisX < -GAMEPAD_DEAD_ZONE && previousAxisX >= -GAMEPAD_DEAD_ZONE) || pressedEdge(14)) goPrev()
            if (pressedEdge(0)) launchActive() // A / Cross
            if (pressedEdge(4)) goPrev() // LB / L1
            if (pressedEdge(5)) goNext() // RB / R1
          }
          if (pressedEdge(1)) { // B / Circle
            if (notificationOpen) onCloseNotifications()
            else if (showHelp) setShowHelp(false)
            else onExit()
          }

          previousButtons.set(key, currentButtons)
          previousAxes.set(key, axisX)
        }
      }
      animationFrame = window.requestAnimationFrame(poll)
    }

    animationFrame = window.requestAnimationFrame(poll)
    return () => window.cancelAnimationFrame(animationFrame)
  }, [goNext, goPrev, isInteractive, launchActive, notificationOpen, onCloseNotifications, onExit, showHelp])

  const heroUrl = activeGame ? resolveHeroUrl(activeGame, assetUrls) : undefined
  const logoUrl = activeGame ? resolveLogoUrl(activeGame, assetUrls) : undefined
  const phaseClass = `is-${phase}`

  return (
    <motion.div
      className={`big-picture-container ${phaseClass}${reducedMotion ? ' is-reduced-motion' : ''}`}
      initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 1.035, filter: 'blur(8px)' }}
      animate={phase === 'exiting'
        ? (reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 1.018, filter: 'blur(7px)' })
        : { opacity: 1, scale: 1, filter: 'blur(0px)' }}
      transition={{ duration: reducedMotion ? 0.08 : phase === 'exiting' ? 0.36 : 0.52, ease: [0.2, 0.78, 0.2, 1] }}
      aria-label="Big Picture mode"
    >
      <motion.div
        className="bp-cinematic-curtain"
        aria-hidden="true"
        initial={{ opacity: 1 }}
        animate={{ opacity: phase === 'entering' ? 0.34 : phase === 'exiting' ? 0.72 : 0 }}
        transition={{ duration: reducedMotion ? 0.05 : 0.46, ease: 'easeOut' }}
      />

      <div className="bp-background-layer">
        <AnimatePresence mode="sync">
          {heroUrl ? (
            <motion.img
              key={heroUrl}
              src={heroUrl}
              className="bp-background-image"
              initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 1.06 }}
              animate={{ opacity: 1, scale: phase === 'exiting' ? 1.025 : 1 }}
              exit={{ opacity: 0, scale: 1.025 }}
              transition={{ duration: reducedMotion ? 0.1 : 0.72, ease: [0.2, 0.78, 0.2, 1] }}
              alt=""
              draggable={false}
              fetchPriority="high"
              decoding="async"
            />
          ) : <div className="bp-background-fallback" />}
        </AnimatePresence>
        <div className="bp-background-overlay" />
        <div className="bp-background-vignette" />
        <div className="bp-accent-bloom" aria-hidden="true" />
      </div>

      <motion.div
        className="bp-ui-layer"
        initial={reducedMotion ? { opacity: 0 } : { opacity: 0, y: 24 }}
        animate={phase === 'exiting'
          ? (reducedMotion ? { opacity: 0 } : { opacity: 0, y: -14 })
          : { opacity: 1, y: 0 }}
        transition={{ duration: reducedMotion ? 0.08 : phase === 'exiting' ? 0.28 : 0.48, delay: reducedMotion ? 0 : 0.06, ease: [0.2, 0.78, 0.2, 1] }}
      >
        <header className="bp-header">
          <div className="bp-header-left">
            <span className="bp-brand-mark"><Monitor size={21} /></span>
            <div className="bp-brand-copy">
              <span className="bp-brand-label">0XOLEMON</span>
              <strong>Big Picture</strong>
            </div>
          </div>

          <div className="bp-header-clock">
            <span className="bp-time">{now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
            <span className="bp-date">{now.toLocaleDateString([], { weekday: 'long', month: 'short', day: 'numeric' })}</span>
          </div>

          <div className="bp-header-actions">
            <div className="bp-controller-hint" aria-hidden="true">
              <Gamepad2 size={18} />
              <span>{gamepadConnected ? 'Controller connected' : 'Keyboard / controller'}</span>
            </div>
            <button className="bp-icon-btn" onClick={() => setShowHelp(true)} title="Controls (?)" aria-label="Open Big Picture controls">
              <span className="bp-question-mark">?</span>
            </button>
            <div className="bp-notification-anchor">
              <button className="bp-icon-btn" onClick={onToggleNotifications} title="Notifications" aria-label="Notifications">
                <Bell size={21} />
                {unread > 0 ? <span className="bp-notification-badge">{unread > 99 ? '99+' : unread}</span> : null}
              </button>
              <NotificationPopover
                open={notificationOpen}
                notifications={notifications}
                onClose={onCloseNotifications}
                onOpenNotification={onOpenNotification}
                onMarkAllRead={onMarkAllNotificationsRead}
                onClear={onClearNotifications}
                onOpenSettings={onOpenNotificationSettings}
              />
            </div>
            <button className="bp-exit-btn" onClick={onExit} title="Exit Big Picture (Esc)">
              <X size={18} />
              <span>Exit</span>
            </button>
          </div>
        </header>

        <AnimatePresence>
          {showHelp ? (
            <motion.div
              className="bp-help-overlay"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: reducedMotion ? 0.05 : 0.18 }}
              onClick={() => setShowHelp(false)}
            >
              <motion.div
                className="bp-help-panel"
                initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 18 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.97, y: 12 }}
                transition={{ duration: reducedMotion ? 0.05 : 0.24, ease: [0.2, 0.78, 0.2, 1] }}
                onClick={(event) => event.stopPropagation()}
              >
                <div className="bp-help-header">
                  <div>
                    <span>BIG PICTURE</span>
                    <h2>Controls</h2>
                  </div>
                  <button className="bp-help-close" onClick={() => setShowHelp(false)} title="Close (Esc)"><X size={19} /></button>
                </div>
                <div className="bp-help-content">
                  <div className="bp-help-section">
                    <h3>Navigate</h3>
                    <div className="bp-help-item"><kbd>←</kbd><kbd>→</kbd><span>Previous / next game</span></div>
                    <div className="bp-help-item"><kbd>LB</kbd><kbd>RB</kbd><span>Previous / next game</span></div>
                    <div className="bp-help-item"><kbd>Stick</kbd><span>Browse carousel</span></div>
                  </div>
                  <div className="bp-help-section">
                    <h3>Actions</h3>
                    <div className="bp-help-item"><kbd>A</kbd><kbd>Enter</kbd><span>Play selected game</span></div>
                    <div className="bp-help-item"><kbd>B</kbd><kbd>Esc</kbd><span>Exit Big Picture</span></div>
                    <div className="bp-help-item"><kbd>?</kbd><span>Toggle controls</span></div>
                  </div>
                </div>
              </motion.div>
            </motion.div>
          ) : null}
        </AnimatePresence>

        <section className="bp-hero">
          <AnimatePresence mode="wait">
            {activeGame ? (
              <motion.div
                key={activeGame.id}
                className="bp-hero-content"
                initial={reducedMotion ? { opacity: 0 } : { opacity: 0, y: 22 }}
                animate={{ opacity: 1, y: 0 }}
                exit={reducedMotion ? { opacity: 0 } : { opacity: 0, y: -14 }}
                transition={{ duration: reducedMotion ? 0.08 : 0.32, ease: [0.2, 0.78, 0.2, 1] }}
              >
                <span className="bp-eyebrow">READY TO PLAY</span>
                {logoUrl ? <img src={logoUrl} alt={activeGame.title} className="bp-hero-logo" draggable={false} /> : <h1 className="bp-hero-title">{activeGame.title}</h1>}
                {activeGame.subtitle ? <p className="bp-hero-subtitle">{activeGame.subtitle}</p> : null}
                <div className="bp-hero-actions">
                  <motion.button
                    className="bp-play-btn"
                    onClick={launchActive}
                    disabled={!isInteractive}
                    whileHover={reducedMotion ? undefined : { scale: 1.035 }}
                    whileTap={reducedMotion ? undefined : { scale: 0.98 }}
                  >
                    <Play size={20} fill="currentColor" />
                    Play
                  </motion.button>
                  <span className="bp-play-hint"><kbd>A</kbd> or <kbd>Enter</kbd></span>
                </div>
              </motion.div>
            ) : (
              <div className="bp-empty-state">
                <Gamepad2 size={42} />
                <h1>No games available</h1>
                <p>Add games to the launcher, then return to Big Picture.</p>
              </div>
            )}
          </AnimatePresence>
        </section>

        <section className="bp-carousel-container" onPointerMove={markInteraction} aria-label="Game carousel">
          {games.length > 1 && isInteractive ? (
            <motion.div
              key={`${activeGame?.id ?? 'none'}-progress`}
              className="bp-auto-progress"
              initial={{ scaleX: 0 }}
              animate={{ scaleX: 1 }}
              transition={{ duration: AUTO_ADVANCE_DELAY / 1000, ease: 'linear' }}
            />
          ) : null}

          <button className="bp-carousel-arrow is-left" onClick={goPrev} disabled={!isInteractive || games.length < 2} aria-label="Previous game"><ChevronLeft size={26} /></button>
          <div className="bp-carousel-viewport">
            <div className="bp-carousel-track" ref={trackRef}>
              {games.map((game, index) => {
                const isActive = index === activeIndex
                const gridUrl = resolveGridUrl(game, assetUrls)
                return (
                  <button
                    key={game.id}
                    className={`bp-game-card${isActive ? ' is-active' : ''}`}
                    onClick={() => goTo(index)}
                    onDoubleClick={() => {
                      goTo(index)
                      if (index === activeIndex) launchActive()
                    }}
                    type="button"
                    tabIndex={isActive ? 0 : -1}
                    aria-current={isActive ? 'true' : undefined}
                    aria-label={game.title}
                  >
                    <div className="bp-card-inner">
                      {gridUrl ? <img src={gridUrl} alt="" className="bp-card-img" draggable={false} /> : <div className="bp-card-placeholder"><span>{game.title}</span></div>}
                      {isActive ? <motion.div className="bp-card-focus-ring" layoutId="big-picture-focus-ring" transition={{ type: 'spring', stiffness: 420, damping: 34 }} /> : null}
                    </div>
                    <div className={`bp-card-label${isActive ? ' is-active' : ''}`}>{game.title}</div>
                  </button>
                )
              })}
            </div>
          </div>
          <button className="bp-carousel-arrow is-right" onClick={goNext} disabled={!isInteractive || games.length < 2} aria-label="Next game"><ChevronRight size={26} /></button>
        </section>
      </motion.div>
    </motion.div>
  )
}
