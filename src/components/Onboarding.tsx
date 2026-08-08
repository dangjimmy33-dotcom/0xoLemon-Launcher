import { useEffect, useLayoutEffect, useMemo, useState } from 'react'
import { ChevronLeft, ChevronRight, CircleHelp, Compass, X } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'

type TourRect = { top: number; left: number; width: number; height: number }

type StepKey = 'welcome' | 'sidebar' | 'store' | 'luaShop' | 'help' | 'settings'

const TOUR_STEPS: Array<{ key: StepKey; selector?: string }> = [
  { key: 'welcome' },
  { key: 'sidebar', selector: '[data-tour="sidebar"]' },
  { key: 'store', selector: '[data-tour="nav-store"]' },
  { key: 'luaShop', selector: '[data-tour="nav-lua-shop"]' },
  { key: 'help', selector: '[data-tour="page-help"]' },
  { key: 'settings', selector: '[data-tour="nav-settings"]' },
]

function readRect(selector?: string): TourRect | null {
  if (!selector) return null
  const element = document.querySelector<HTMLElement>(selector)
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  const pad = 7
  return {
    top: Math.max(8, rect.top - pad),
    left: Math.max(8, rect.left - pad),
    width: Math.min(window.innerWidth - 16, rect.width + pad * 2),
    height: Math.min(window.innerHeight - 16, rect.height + pad * 2),
  }
}

function cardPosition(rect: TourRect | null) {
  const width = 370
  const gap = 18
  if (!rect) {
    return {
      left: Math.max(18, (window.innerWidth - width) / 2),
      top: Math.max(100, window.innerHeight * 0.5 - 170),
    }
  }
  const rightSpace = window.innerWidth - (rect.left + rect.width)
  const leftSpace = rect.left
  if (rightSpace >= width + gap) {
    return { left: rect.left + rect.width + gap, top: Math.min(Math.max(22, rect.top), window.innerHeight - 320) }
  }
  if (leftSpace >= width + gap) {
    return { left: rect.left - width - gap, top: Math.min(Math.max(22, rect.top), window.innerHeight - 320) }
  }
  return {
    left: Math.max(18, Math.min(window.innerWidth - width - 18, rect.left)),
    top: rect.top + rect.height + 16 < window.innerHeight - 280 ? rect.top + rect.height + 16 : Math.max(22, rect.top - 290),
  }
}

export function Onboarding({
  onComplete,
  onEnableWindowsNotifications: _onEnableWindowsNotifications,
}: {
  onComplete: () => void
  onEnableWindowsNotifications: () => void
}) {
  const { t } = useLocale()
  const [step, setStep] = useState(0)
  const [rect, setRect] = useState<TourRect | null>(null)
  const current = TOUR_STEPS[step]
  const isWelcome = current.key === 'welcome'

  const copy = useMemo(() => {
    if (current.key === 'welcome') {
      return {
        title: t.onboarding.welcomeTitle,
        body: t.onboarding.welcomeBody,
        note: t.onboarding.welcomeBenefit,
      }
    }
    const value = t.onboarding.steps[current.key]
    return { title: value.title, body: value.body, note: null as string | null }
  }, [current.key, t])

  useLayoutEffect(() => {
    const update = () => setRect(readRect(current.selector))
    update()
    const raf = window.requestAnimationFrame(update)
    return () => window.cancelAnimationFrame(raf)
  }, [current.selector, step])

  useEffect(() => {
    const update = () => setRect(readRect(current.selector))
    window.addEventListener('resize', update)
    window.addEventListener('scroll', update, true)
    return () => {
      window.removeEventListener('resize', update)
      window.removeEventListener('scroll', update, true)
    }
  }, [current.selector])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onComplete()
      if (event.key === 'ArrowRight' && step < TOUR_STEPS.length - 1) setStep((value) => value + 1)
      if (event.key === 'ArrowLeft' && step > 0) setStep((value) => value - 1)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onComplete, step])

  const position = typeof window !== 'undefined' ? cardPosition(rect) : { left: 24, top: 120 }
  const progressLabel = t.onboarding.step
    .replace('{current}', String(step + 1))
    .replace('{total}', String(TOUR_STEPS.length))

  const next = () => {
    if (step === TOUR_STEPS.length - 1) {
      onComplete()
      return
    }
    setStep((value) => value + 1)
  }

  return (
    <div className={`guided-tour${isWelcome ? ' is-welcome' : ''}`} role="dialog" aria-modal="true" aria-labelledby="guided-tour-title">
      {!isWelcome && rect ? (
        <div
          className="guided-tour-spotlight"
          aria-hidden="true"
          style={{ top: rect.top, left: rect.left, width: rect.width, height: rect.height }}
        />
      ) : <div className="guided-tour-dim" aria-hidden="true" />}

      <section className="guided-tour-card" style={position}>
        <button type="button" className="guided-tour-close" onClick={onComplete} aria-label={t.onboarding.skip}><X size={17} /></button>
        <div className="guided-tour-icon">{isWelcome ? <Compass size={22} /> : <CircleHelp size={20} />}</div>
        <span className="guided-tour-progress">{progressLabel}</span>
        <h2 id="guided-tour-title">{copy.title}</h2>
        <p>{copy.body}</p>
        {copy.note ? <small>{copy.note}</small> : null}
        <div className="guided-tour-dots" aria-hidden="true">
          {TOUR_STEPS.map((item, index) => <i key={item.key} className={index === step ? 'active' : index < step ? 'done' : ''} />)}
        </div>
        <footer>
          <button type="button" className="guided-tour-skip" onClick={onComplete}>{t.onboarding.skip}</button>
          <div>
            {step > 0 ? <button type="button" className="guided-tour-secondary" onClick={() => setStep((value) => value - 1)}><ChevronLeft size={15} />{t.onboarding.back}</button> : null}
            <button type="button" className="guided-tour-primary" onClick={next}>
              {step === TOUR_STEPS.length - 1 ? t.onboarding.finish : t.onboarding.next}
              {step < TOUR_STEPS.length - 1 ? <ChevronRight size={15} /> : null}
            </button>
          </div>
        </footer>
      </section>
    </div>
  )
}
