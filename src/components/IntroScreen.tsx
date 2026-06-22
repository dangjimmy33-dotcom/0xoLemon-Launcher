import { useEffect, useRef, useState } from 'react'
import appIcon from '../assets/app-icon.png'

interface IntroScreenProps {
  onExiting?: () => void  // fired when exit animation STARTS (2400ms)
  onDone?: () => void     // fired when fully gone (2900ms)
}

/**
 * Cinematic intro screen — Xbox-style.
 * Timeline:
 *   0ms    → enter: logo + ring animate in
 *   900ms  → hold: glow pulse, text appears
 *   2400ms → exit: blur + scale fade begins  →  onExiting() called
 *   2900ms → fully transparent               →  onDone() called
 *
 * IMPORTANT: useEffect has empty deps [] to run timers exactly once.
 * Callbacks are stored in refs so they always call the latest version
 * without causing the effect to re-run.
 */
export function IntroScreen({ onExiting, onDone }: IntroScreenProps) {
  const [phase, setPhase] = useState<'enter' | 'hold' | 'exit'>('enter')

  // Stable refs so timers always call the latest callback without re-running
  const onExitingRef = useRef(onExiting)
  const onDoneRef = useRef(onDone)
  onExitingRef.current = onExiting
  onDoneRef.current = onDone

  useEffect(() => {
    const t1 = window.setTimeout(() => setPhase('hold'), 900)
    const t2 = window.setTimeout(() => {
      setPhase('exit')
      onExitingRef.current?.()
    }, 2400)
    const t3 = window.setTimeout(() => onDoneRef.current?.(), 2900)
    return () => {
      window.clearTimeout(t1)
      window.clearTimeout(t2)
      window.clearTimeout(t3)
    }
  }, []) // empty — timers fire exactly once per mount

  return (
    <div className={`intro-screen phase-${phase}`} aria-hidden="true">
      <div className="intro-bg" />
      <div className="intro-glow" />

      <div className="intro-logo-wrap">
        <svg className="intro-ring" viewBox="0 0 120 120" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle className="intro-ring-track" cx="60" cy="60" r="50" strokeWidth="1.5" />
          <circle className="intro-ring-arc"   cx="60" cy="60" r="50" strokeWidth="1.5" />
        </svg>
        <div className="intro-logo-icon">
          <img src={appIcon} alt="0xo Lemon" draggable={false} />
        </div>
      </div>

      <div className="intro-text-block">
        <p className="intro-app-name">0xo Lemon</p>
        <p className="intro-tagline">Launching…</p>
      </div>
    </div>
  )
}
