import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import type { ThemeRouteFrameProps } from '../contracts'

export default function LightningRouteFrame({ children, activeTab }: ThemeRouteFrameProps) {
  const previous = useRef<{ route: string; content: ReactNode }>({ route: activeTab, content: children })
  const direction = useRef<'forward' | 'backward'>('forward')
  const cleanupTimer = useRef<number | null>(null)
  const [outgoing, setOutgoing] = useState<{ route: string; content: ReactNode; direction: 'forward' | 'backward' } | null>(null)

  useEffect(() => {
    const handleDirection = (event: Event) => {
      const requested = (event as CustomEvent<{ direction?: string }>).detail?.direction
      direction.current = requested === 'backward' ? 'backward' : 'forward'
    }
    window.addEventListener('launcher://cinematic-route-direction', handleDirection)
    return () => window.removeEventListener('launcher://cinematic-route-direction', handleDirection)
  }, [])

  useLayoutEffect(() => {
    const prior = previous.current
    if (prior.route !== activeTab) {
      setOutgoing({ ...prior, direction: direction.current })
      if (cleanupTimer.current !== null) window.clearTimeout(cleanupTimer.current)
      cleanupTimer.current = window.setTimeout(() => setOutgoing(null), 380)
      direction.current = 'forward'
    }
    previous.current = { route: activeTab, content: children }
  }, [activeTab, children])

  useEffect(() => () => {
    if (cleanupTimer.current !== null) window.clearTimeout(cleanupTimer.current)
  }, [])

  return (
    <div className="lightning-route-frame" data-lightning-route={activeTab}>
      {outgoing ? <div className={`lightning-route-content is-outgoing is-${outgoing.direction}`} data-lightning-route={outgoing.route} aria-hidden="true">{outgoing.content}</div> : null}
      <div className={`lightning-route-content is-incoming is-${outgoing?.direction ?? 'forward'}`}>{children}</div>
    </div>
  )
}
