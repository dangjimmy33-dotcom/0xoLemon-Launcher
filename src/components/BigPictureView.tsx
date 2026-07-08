import { useEffect, useState, useRef, useCallback } from 'react'
import { motion, AnimatePresence } from 'motion/react'
import { Bell, Play, X, Monitor } from 'lucide-react'
import type { GameSummary, NotificationRecord } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { NotificationPopover } from './NotificationCenter'
import './BigPictureView.css'

interface BigPictureViewProps {
  games: GameSummary[]
  assetUrls: Record<string, string>
  onExit: () => void
  notifications: NotificationRecord[]
  notificationOpen: boolean
  onToggleNotifications: () => void
  onCloseNotifications: () => void
  onOpenNotification: (n: NotificationRecord) => void
  onMarkAllNotificationsRead: () => void
  onClearNotifications: () => void
  onOpenNotificationSettings: () => void
}

/** Resolve asset in priority order: hero > grid > icon */
function resolveHeroUrl(game: GameSummary, assets: Record<string, string>) {
  return (
    assetUrlForId(game.heroAssetId, assets) ||
    assetUrlForId(game.gridAssetId, assets) ||
    assetUrlForId(game.iconAssetId, assets)
  )
}

/** Resolve grid thumbnail: grid > icon */
function resolveGridUrl(game: GameSummary, assets: Record<string, string>) {
  return (
    assetUrlForId(game.gridAssetId, assets) ||
    assetUrlForId(game.iconAssetId, assets)
  )
}

/** Resolve logo overlay: logo only */
function resolveLogoUrl(game: GameSummary, assets: Record<string, string>) {
  return assetUrlForId(game.logoAssetId, assets)
}

const AUTO_ADVANCE_DELAY = 4000 // 4 seconds

export function BigPictureView({
  games,
  assetUrls,
  onExit,
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
  const trackRef = useRef<HTMLDivElement>(null)
  const autoTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastInteractionRef = useRef(Date.now())

  const activeGame = games[activeIndex] || null
  const unread = notifications.filter((n) => !n.read).length

  // Clock tick
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 1000)
    return () => clearInterval(timer)
  }, [])

  // Center the active card in the carousel track
  useEffect(() => {
    if (!trackRef.current) return
    const activeElement = trackRef.current.children[activeIndex] as HTMLElement
    if (!activeElement) return
    const containerWidth = trackRef.current.parentElement?.clientWidth ?? window.innerWidth
    const scrollLeft = activeElement.offsetLeft - containerWidth / 2 + activeElement.offsetWidth / 2
    trackRef.current.style.transform = `translateX(${-scrollLeft}px)`
    trackRef.current.style.transition = 'transform 0.5s cubic-bezier(0.25, 0.8, 0.25, 1)'
  }, [activeIndex])

  // --- Auto-advance timer ---
  const scheduleAutoAdvance = useCallback(() => {
    if (autoTimerRef.current) clearTimeout(autoTimerRef.current)
    if (games.length <= 1) return
    autoTimerRef.current = setTimeout(() => {
      // Only auto advance if no recent interaction
      if (Date.now() - lastInteractionRef.current >= AUTO_ADVANCE_DELAY - 50) {
        setActiveIndex((prev) => (prev + 1) % games.length)
      }
    }, AUTO_ADVANCE_DELAY)
  }, [games.length])

  // Restart timer whenever active index changes
  useEffect(() => {
    scheduleAutoAdvance()
    return () => {
      if (autoTimerRef.current) clearTimeout(autoTimerRef.current)
    }
  }, [activeIndex, scheduleAutoAdvance])

  /** Mark user interaction and reset auto-advance */
  const markInteraction = useCallback(() => {
    lastInteractionRef.current = Date.now()
    scheduleAutoAdvance()
  }, [scheduleAutoAdvance])

  const goTo = useCallback((idx: number) => {
    markInteraction()
    setActiveIndex(idx)
  }, [markInteraction])

  const goNext = useCallback(() => {
    markInteraction()
    setActiveIndex((prev) => (prev + 1) % games.length)
  }, [markInteraction, games.length])

  const goPrev = useCallback(() => {
    markInteraction()
    setActiveIndex((prev) => (prev - 1 + games.length) % games.length)
  }, [markInteraction, games.length])

  // Keyboard + gamepad (via keyboard events) navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (notificationOpen) {
        if (e.key === 'Escape') onCloseNotifications()
        return
      }
      if (showHelp) {
        if (e.key === 'Escape' || e.key === '?') {
          e.preventDefault()
          setShowHelp(false)
        }
        return
      }
      switch (e.key) {
        case 'ArrowRight':
        case 'Tab':
          e.preventDefault()
          goNext()
          break
        case 'ArrowLeft':
          e.preventDefault()
          goPrev()
          break
        case 'Enter':
        case ' ':
          e.preventDefault()
          // Future: trigger launch
          break
        case 'Escape':
          onExit()
          break
        case '?':
          e.preventDefault()
          setShowHelp(true)
          break
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [notificationOpen, showHelp, onCloseNotifications, onExit, goNext, goPrev])

  // Mouse move resets auto-advance
  useEffect(() => {
    const handleMouseMove = () => markInteraction()
    window.addEventListener('mousemove', handleMouseMove, { passive: true })
    return () => window.removeEventListener('mousemove', handleMouseMove)
  }, [markInteraction])

  // Gamepad polling (for Left/Right axes)
  useEffect(() => {
    let animFrame: number
    let lastAxis = 0
    const DEAD_ZONE = 0.5
    const poll = () => {
      const pads = navigator.getGamepads?.()
      if (pads) {
        for (const pad of pads) {
          if (!pad) continue
          const axis = pad.axes[0] ?? 0
          if (axis > DEAD_ZONE && lastAxis <= DEAD_ZONE) goNext()
          if (axis < -DEAD_ZONE && lastAxis >= -DEAD_ZONE) goPrev()
          lastAxis = axis
        }
      }
      animFrame = requestAnimationFrame(poll)
    }
    animFrame = requestAnimationFrame(poll)
    return () => cancelAnimationFrame(animFrame)
  }, [goNext, goPrev])

  const heroUrl = activeGame ? resolveHeroUrl(activeGame, assetUrls) : undefined
  const logoUrl = activeGame ? resolveLogoUrl(activeGame, assetUrls) : undefined

  return (
    <motion.div
      className="big-picture-container"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.35, ease: 'easeOut' }}
    >
      {/* ── Background: hero art with crossfade ── */}
      <div className="bp-background-layer">
        <AnimatePresence mode="sync">
          {heroUrl && (
            <motion.img
              key={heroUrl}
              src={heroUrl}
              className="bp-background-image"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.7 }}
              alt=""
              draggable={false}
            />
          )}
        </AnimatePresence>
        <div className="bp-background-overlay" />
        {/* Vignette bottom-to-top for hero text legibility */}
        <div className="bp-background-vignette" />
      </div>

      {/* ── UI Layer ── */}
      <div className="bp-ui-layer">
        {/* Header */}
        <header className="bp-header">
          <div className="bp-header-left">
            <Monitor size={22} className="bp-brand-icon" />
            <span className="bp-brand-label">BIG PICTURE</span>
          </div>

          <div className="bp-header-clock">
            <span className="bp-time">
              {now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            </span>
            <span className="bp-date">
              {now.toLocaleDateString([], { weekday: 'long', month: 'short', day: 'numeric' })}
            </span>
          </div>

          <div className="bp-header-actions">
            <button
              className="bp-icon-btn"
              onClick={() => setShowHelp(true)}
              title="Keyboard Shortcuts (?)"
            >
              <span style={{ fontSize: '1.3rem', fontWeight: 700 }}>?</span>
            </button>

            <div style={{ position: 'relative' }}>
              <button
                className="bp-icon-btn"
                onClick={onToggleNotifications}
                title="Notifications"
              >
                <Bell size={22} />
                {unread > 0 && (
                  <span className="bp-notification-badge">
                    {unread > 99 ? '99+' : unread}
                  </span>
                )}
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
              <X size={20} />
              <span>Exit</span>
            </button>
          </div>
        </header>

        {/* Keyboard Shortcuts Help Panel */}
        <AnimatePresence>
          {showHelp && (
            <motion.div
              className="bp-help-overlay"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2 }}
              onClick={() => setShowHelp(false)}
            >
              <motion.div
                className="bp-help-panel"
                initial={{ scale: 0.9, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                exit={{ scale: 0.9, opacity: 0 }}
                transition={{ duration: 0.25, ease: 'easeOut' }}
                onClick={(e) => e.stopPropagation()}
              >
                <div className="bp-help-header">
                  <h2>Keyboard Shortcuts</h2>
                  <button
                    className="bp-help-close"
                    onClick={() => setShowHelp(false)}
                    title="Close (Esc)"
                  >
                    <X size={20} />
                  </button>
                </div>

                <div className="bp-help-content">
                  <div className="bp-help-section">
                    <h3>Navigation</h3>
                    <div className="bp-help-item">
                      <kbd>←</kbd>
                      <span>Previous game</span>
                    </div>
                    <div className="bp-help-item">
                      <kbd>→</kbd>
                      <kbd>Tab</kbd>
                      <span>Next game</span>
                    </div>
                    <div className="bp-help-item">
                      <kbd>Enter</kbd>
                      <kbd>Space</kbd>
                      <span>Launch game</span>
                    </div>
                  </div>

                  <div className="bp-help-section">
                    <h3>Controls</h3>
                    <div className="bp-help-item">
                      <kbd>Esc</kbd>
                      <span>Exit Big Picture</span>
                    </div>
                    <div className="bp-help-item">
                      <kbd>?</kbd>
                      <span>Toggle this help</span>
                    </div>
                  </div>

                  <div className="bp-help-section">
                    <h3>Gamepad</h3>
                    <div className="bp-help-item">
                      <kbd>Left Stick</kbd>
                      <span>Navigate games</span>
                    </div>
                    <div className="bp-help-item">
                      <kbd>A Button</kbd>
                      <span>Launch game</span>
                    </div>
                  </div>
                </div>
              </motion.div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Hero Section — active game info */}
        <section className="bp-hero">
          <AnimatePresence mode="wait">
            {activeGame && (
              <motion.div
                key={activeGame.id}
                className="bp-hero-content"
                initial={{ opacity: 0, y: 24 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -16 }}
                transition={{ duration: 0.32, ease: 'easeOut' }}
              >
                {logoUrl ? (
                  <img
                    src={logoUrl}
                    alt={activeGame.title}
                    className="bp-hero-logo"
                    draggable={false}
                  />
                ) : (
                  <h1 className="bp-hero-title">{activeGame.title}</h1>
                )}

                {activeGame.subtitle && (
                  <p className="bp-hero-subtitle">{activeGame.subtitle}</p>
                )}

                <div className="bp-hero-actions">
                  <button className="bp-play-btn">
                    <Play size={20} fill="currentColor" />
                    Play
                  </button>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </section>

        {/* Carousel */}
        <section className="bp-carousel-container" onMouseMove={markInteraction}>
          <div className="bp-carousel-track" ref={trackRef}>
            {games.map((game, idx) => {
              const isActive = idx === activeIndex
              const gridUrl = resolveGridUrl(game, assetUrls)

              return (
                <div
                  key={game.id}
                  className={`bp-game-card${isActive ? ' is-active' : ''}`}
                  onClick={() => goTo(idx)}
                  role="button"
                  tabIndex={-1}
                  aria-label={game.title}
                >
                  <div className="bp-card-inner">
                    {gridUrl ? (
                      <img
                        src={gridUrl}
                        alt={game.title}
                        className="bp-card-img"
                        draggable={false}
                      />
                    ) : (
                      <div className="bp-card-placeholder">
                        <span>{game.title}</span>
                      </div>
                    )}

                    {/* Active-focus glow ring */}
                    {isActive && (
                      <motion.div
                        className="bp-card-focus-ring"
                        layoutId="focus-ring"
                        transition={{ type: 'spring', stiffness: 400, damping: 30 }}
                      />
                    )}
                  </div>

                  {/* Game title label under card */}
                  <div className={`bp-card-label${isActive ? ' is-active' : ''}`}>
                    {game.title}
                  </div>
                </div>
              )
            })}
          </div>
        </section>
      </div>
    </motion.div>
  )
}
