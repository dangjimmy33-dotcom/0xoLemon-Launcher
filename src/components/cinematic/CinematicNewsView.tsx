import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { ArrowRight, ChevronLeft, ChevronRight, X } from 'lucide-react'
import type { ThemeInstanceViewModel } from '../../themes/contracts'
import './cinematic.css'

type CinematicNewsViewProps = {
  items: readonly ThemeInstanceViewModel[]
  onClose: () => void
  onOpen: (gameId: string) => void
}

const STORY_INTERVAL_MS = 6_000
const CROSSFADE_MS = 420

export default function CinematicNewsView({ items, onClose, onOpen }: CinematicNewsViewProps) {
  const stories = useMemo(() => items.filter((item) => item.heroUrl || item.gridUrl).slice(0, 12), [items])
  const [index, setIndex] = useState(0)
  const [outgoingIndex, setOutgoingIndex] = useState<number | null>(null)
  const [paused, setPaused] = useState(false)
  const filmstripRefs = useRef(new Map<number, HTMLButtonElement>())
  const transitionTimer = useRef<number | null>(null)
  const pointerStart = useRef<number | null>(null)
  const active = stories[index]

  const selectIndex = useCallback((requested: number) => {
    if (stories.length === 0) return
    const next = (requested + stories.length) % stories.length
    setIndex((current) => {
      if (current === next) return current
      setOutgoingIndex(current)
      if (transitionTimer.current !== null) window.clearTimeout(transitionTimer.current)
      transitionTimer.current = window.setTimeout(() => setOutgoingIndex(null), CROSSFADE_MS)
      return next
    })
  }, [stories.length])

  useEffect(() => () => {
    if (transitionTimer.current !== null) window.clearTimeout(transitionTimer.current)
  }, [])

  useEffect(() => {
    if (paused || stories.length < 2) return
    const timer = window.setInterval(() => selectIndex(index + 1), STORY_INTERVAL_MS)
    return () => window.clearInterval(timer)
  }, [index, paused, selectIndex, stories.length])

  useEffect(() => {
    filmstripRefs.current.get(index)?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' })
    const next = stories[(index + 1) % stories.length]
    const url = next?.heroUrl || next?.gridUrl
    if (!url) return
    const image = new Image()
    image.decoding = 'async'
    image.src = url
  }, [index, stories])

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === 'ArrowLeft') { event.preventDefault(); selectIndex(index - 1) }
    if (event.key === 'ArrowRight') { event.preventDefault(); selectIndex(index + 1) }
    if (event.key === 'Home') { event.preventDefault(); selectIndex(0) }
    if (event.key === 'End') { event.preventDefault(); selectIndex(stories.length - 1) }
    if (event.key === 'Escape') { event.preventDefault(); onClose() }
  }

  if (!active) return null
  const outgoing = outgoingIndex === null ? null : stories[outgoingIndex]

  return (
    <section
      className="cinematic-news-view"
      role="dialog"
      aria-modal="true"
      aria-label="Last News"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocusCapture={() => setPaused(true)}
      onBlurCapture={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setPaused(false) }}
      onPointerDown={(event) => { pointerStart.current = event.clientX }}
      onPointerUp={(event) => {
        const start = pointerStart.current
        pointerStart.current = null
        if (start === null || Math.abs(event.clientX - start) < 48) return
        selectIndex(event.clientX < start ? index + 1 : index - 1)
      }}
    >
      <div className="cinematic-news-backgrounds" aria-hidden="true">
        {outgoing ? <img key={`out-${outgoing.gameId}`} className="is-outgoing" src={outgoing.heroUrl || outgoing.gridUrl} alt="" decoding="async" /> : null}
        <img key={`current-${active.gameId}`} className="is-current" src={active.heroUrl || active.gridUrl} alt="" decoding="async" />
        <div className="cinematic-news-mask" />
      </div>
      <div className="cinematic-news-brand">0xoLemon Launcher</div>
      <button className="cinematic-news-close" type="button" onClick={onClose} aria-label="Close Last News"><X /></button>
      <button className="cinematic-news-arrow is-previous" type="button" onClick={() => selectIndex(index - 1)} aria-label="Previous story"><ChevronLeft /></button>
      <button className="cinematic-news-arrow is-next" type="button" onClick={() => selectIndex(index + 1)} aria-label="Next story"><ChevronRight /></button>
      <article className="cinematic-news-headline" aria-live="polite">
        <span>LAST NEWS · {String(index + 1).padStart(2, '0')}</span>
        <h1>{active.title}</h1>
        <p>{active.description || `${active.title} is available now in 0xoLemon Launcher.`}</p>
        <button type="button" onClick={() => onOpen(active.gameId)}>Open game <ArrowRight /></button>
      </article>
      <div className="cinematic-news-filmstrip" aria-label="News stories">
        {stories.map((story, storyIndex) => (
          <button
            ref={(node) => { if (node) filmstripRefs.current.set(storyIndex, node); else filmstripRefs.current.delete(storyIndex) }}
            key={story.gameId}
            type="button"
            className={storyIndex === index ? 'is-active' : ''}
            aria-current={storyIndex === index ? 'true' : undefined}
            onClick={() => selectIndex(storyIndex)}
          >
            <img src={story.gridUrl || story.heroUrl} alt="" loading={Math.abs(storyIndex - index) <= 1 ? 'eager' : 'lazy'} decoding="async" />
            <span><small>{String(storyIndex + 1).padStart(2, '0')}</small><strong>{story.title}</strong></span>
          </button>
        ))}
      </div>
      <div className={`cinematic-news-timer${paused ? ' is-paused' : ''}`} key={`${index}-${paused ? 'paused' : 'running'}`} aria-hidden="true"><span /></div>
    </section>
  )
}
