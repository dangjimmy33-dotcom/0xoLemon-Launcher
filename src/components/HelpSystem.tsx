import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { BookOpen, CircleHelp, RotateCcw, Search, X } from 'lucide-react'
import { useLocale } from '../context/locale'
import type { TabId } from '../types'
import {
  HELP_CONCEPT_ORDER,
  HELP_TOPIC_BY_TAB,
  HELP_TOPIC_ORDER,
  type HelpConceptId,
  type HelpTopicId,
} from '../lib/helpRegistry'
import './HelpSystem.css'

type RectPoint = { top: number; left: number }

function popoverPoint(button: HTMLButtonElement | null): RectPoint {
  if (!button) return { top: 72, left: Math.max(16, window.innerWidth - 396) }
  const rect = button.getBoundingClientRect()
  const width = 340
  const gap = 10
  const left = Math.min(Math.max(16, rect.right - width), Math.max(16, window.innerWidth - width - 16))
  const preferredTop = rect.bottom + gap
  const top = preferredTop + 280 < window.innerHeight
    ? preferredTop
    : Math.max(16, rect.top - 290)
  return { top, left }
}

export function HelpButton({
  title,
  body,
  bullets,
  ariaLabel,
  actionLabel,
  onAction,
  className = '',
}: {
  title: string
  body: string
  bullets?: readonly string[]
  ariaLabel?: string
  actionLabel?: string
  onAction?: () => void
  className?: string
}) {
  const { t } = useLocale()
  const id = useId()
  const buttonRef = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const [point, setPoint] = useState<RectPoint>({ top: 72, left: 16 })

  useEffect(() => {
    const closeOther = (event: Event) => {
      const custom = event as CustomEvent<string>
      if (custom.detail !== id) setOpen(false)
    }
    window.addEventListener('0xo-help-popover-open', closeOther)
    return () => window.removeEventListener('0xo-help-popover-open', closeOther)
  }, [id])

  useEffect(() => {
    if (!open) return
    const update = () => setPoint(popoverPoint(buttonRef.current))
    update()
    window.addEventListener('resize', update)
    window.addEventListener('scroll', update, true)
    return () => {
      window.removeEventListener('resize', update)
      window.removeEventListener('scroll', update, true)
    }
  }, [open])

  const toggle = () => {
    const next = !open
    if (next) window.dispatchEvent(new CustomEvent('0xo-help-popover-open', { detail: id }))
    setOpen(next)
  }

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className={`help-icon-button ${className}`.trim()}
        aria-label={ariaLabel || `${t.help.buttonLabel}: ${title}`}
        aria-expanded={open}
        onClick={toggle}
      >
        <span aria-hidden="true">?</span>
      </button>
      {open && typeof document !== 'undefined' ? createPortal(
        <div className="help-popover" role="dialog" aria-label={title} style={point}>
          <div className="help-popover-head">
            <span className="help-popover-icon"><CircleHelp size={17} /></span>
            <strong>{title}</strong>
            <button type="button" aria-label={t.help.close} onClick={() => setOpen(false)}><X size={15} /></button>
          </div>
          <p>{body}</p>
          {bullets?.length ? (
            <ul>
              {bullets.map((item) => <li key={item}>{item}</li>)}
            </ul>
          ) : null}
          {actionLabel && onAction ? (
            <button
              type="button"
              className="help-popover-action"
              onClick={() => {
                setOpen(false)
                onAction()
              }}
            >
              <BookOpen size={15} /> {actionLabel}
            </button>
          ) : null}
        </div>,
        document.body,
      ) : null}
    </>
  )
}

export function PageHelpButton({
  tab,
  onOpenCenter,
  placement = 'floating',
}: {
  tab: TabId
  onOpenCenter: () => void
  placement?: 'floating' | 'titlebar'
}) {
  const { t } = useLocale()
  const topicId = HELP_TOPIC_BY_TAB[tab]
  const topic = t.help.topics[topicId]
  const inTitlebar = placement === 'titlebar'
  return (
    <div className={inTitlebar ? 'titlebar-help-anchor' : 'page-help-float'} data-tour="page-help">
      <HelpButton
        title={topic.title}
        body={topic.summary}
        bullets={topic.canDo}
        actionLabel={t.help.openHelpCenter}
        onAction={onOpenCenter}
        ariaLabel={`${t.help.buttonLabel}: ${topic.title}`}
        className={inTitlebar ? 'titlebar-help-button' : 'page-help-button'}
      />
    </div>
  )
}

type HelpSelection =
  | { kind: 'topic'; id: HelpTopicId }
  | { kind: 'concept'; id: HelpConceptId }

export function HelpCenter({
  open,
  activeTab,
  onClose,
  onReplayTour,
}: {
  open: boolean
  activeTab: TabId
  onClose: () => void
  onReplayTour: () => void
}) {
  const { t } = useLocale()
  const [query, setQuery] = useState('')
  const [selection, setSelection] = useState<HelpSelection>({ kind: 'topic', id: HELP_TOPIC_BY_TAB[activeTab] })

  useEffect(() => {
    if (!open) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [open, onClose])

  const normalized = query.trim().toLocaleLowerCase()
  const visibleTopics = useMemo(() => HELP_TOPIC_ORDER.filter((id) => {
    if (!normalized) return true
    const topic = t.help.topics[id]
    return `${topic.title} ${topic.summary} ${topic.canDo.join(' ')}`.toLocaleLowerCase().includes(normalized)
  }), [normalized, t])
  const visibleConcepts = useMemo(() => HELP_CONCEPT_ORDER.filter((id) => {
    if (!normalized) return true
    const concept = t.help.conceptGuides[id]
    return `${concept.title} ${concept.body}`.toLocaleLowerCase().includes(normalized)
  }), [normalized, t])

  if (!open || typeof document === 'undefined') return null

  const selectedTopic = selection.kind === 'topic' ? t.help.topics[selection.id] : null
  const selectedConcept = selection.kind === 'concept' ? t.help.conceptGuides[selection.id] : null

  return createPortal(
    <div className="help-center-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.currentTarget === event.target) onClose()
    }}>
      <section className="help-center" role="dialog" aria-modal="true" aria-labelledby="help-center-title">
        <header className="help-center-header">
          <div>
            <span className="help-center-mark"><CircleHelp size={20} /></span>
            <div>
              <h2 id="help-center-title">{t.help.centerTitle}</h2>
              <p>{t.help.centerSubtitle}</p>
            </div>
          </div>
          <button type="button" className="help-center-close" aria-label={t.help.close} onClick={onClose}><X size={18} /></button>
        </header>

        <div className="help-center-search">
          <Search size={17} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.help.searchPlaceholder} autoFocus />
        </div>

        <div className="help-center-layout">
          <aside className="help-center-nav">
            <strong>{t.help.pageGuides}</strong>
            {visibleTopics.map((id) => (
              <button type="button" key={id} className={selection.kind === 'topic' && selection.id === id ? 'active' : ''} onClick={() => setSelection({ kind: 'topic', id })}>
                {t.help.topics[id].title}
              </button>
            ))}
            <strong>{t.help.concepts}</strong>
            {visibleConcepts.map((id) => (
              <button type="button" key={id} className={selection.kind === 'concept' && selection.id === id ? 'active' : ''} onClick={() => setSelection({ kind: 'concept', id })}>
                {t.help.conceptGuides[id].title}
              </button>
            ))}
            {visibleTopics.length === 0 && visibleConcepts.length === 0 ? <p className="help-center-empty">{t.help.noResults}</p> : null}
          </aside>

          <article className="help-center-content">
            {selectedTopic ? (
              <>
                <span className="help-center-kicker">{t.help.whatIsThis}</span>
                <h3>{selectedTopic.title}</h3>
                <p>{selectedTopic.summary}</p>
                <h4>{t.help.whatCanIDo}</h4>
                <ul>{selectedTopic.canDo.map((item) => <li key={item}>{item}</li>)}</ul>
                <div className="help-center-tip"><strong>{t.help.goodToKnow}</strong><span>{selectedTopic.tip}</span></div>
              </>
            ) : selectedConcept ? (
              <>
                <span className="help-center-kicker">{t.help.concepts}</span>
                <h3>{selectedConcept.title}</h3>
                <p>{selectedConcept.body}</p>
              </>
            ) : null}
          </article>
        </div>

        <footer className="help-center-footer">
          <div><RotateCcw size={16} /><span><strong>{t.help.replayTour}</strong><small>{t.help.replayTourDesc}</small></span></div>
          <button type="button" onClick={() => { onClose(); onReplayTour() }}><RotateCcw size={15} />{t.help.replayTour}</button>
        </footer>
      </section>
    </div>,
    document.body,
  )
}

export function InlineHelpLabel({ children, help }: { children: ReactNode; help: { title: string; body: string; bullets?: readonly string[] } }) {
  return <span className="inline-help-label">{children}<HelpButton title={help.title} body={help.body} bullets={help.bullets} /></span>
}
