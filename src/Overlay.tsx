import { useEffect, useMemo, useState } from 'react'
import { Activity, CircleDot, Gamepad2, Gauge, Radio, ShieldCheck, Trophy, X } from 'lucide-react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'

import type { AchievementEventV2, AchievementState, GameSessionStateV1 } from './types'
import { subscribeAchievementEvents } from './lib/achievementEventBus'
import { getSessionAchievementState, listGameSessionStates } from './lib/gameSession'
import './Overlay.css'

type OverlayTab = 'session' | 'achievements' | 'performance'

const ACTIVE_LIFECYCLES = new Set<GameSessionStateV1['lifecycle']>([
  'starting',
  'running',
  'exiting',
])

function rendererLabel(renderer: GameSessionStateV1['renderer']): string {
  switch (renderer) {
    case 'gseNative': return 'GSE native'
    case 'reshadeCompatibility': return 'ReShade compatibility'
    case 'desktopFallback': return 'Desktop fallback'
    default: return 'Disabled'
  }
}

function transportLabel(transport: GameSessionStateV1['achievementTransport']): string {
  switch (transport) {
    case 'namedPipe': return 'Named pipe'
    case 'scopedFallback': return 'Scoped fallback'
    case 'connecting': return 'Connecting'
    default: return 'Closed'
  }
}

function formatBytes(value: number | null): string {
  if (value === null) return 'Not sampled'
  const units = ['B', 'KiB', 'MiB', 'GiB']
  let amount = value
  let unit = 0
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024
    unit += 1
  }
  return `${amount.toFixed(unit > 1 ? 1 : 0)} ${units[unit]}`
}

function formatMetric(value: number | null, suffix: string): string {
  return value === null ? 'Not sampled' : `${value.toFixed(2)} ${suffix}`
}

function upsertSession(
  sessions: GameSessionStateV1[],
  next: GameSessionStateV1,
): GameSessionStateV1[] {
  const retained = sessions.filter((item) => item.gameId !== next.gameId)
  const combined = [next, ...retained]
  return combined.sort((left, right) => {
    const leftActive = ACTIVE_LIFECYCLES.has(left.lifecycle) ? 0 : 1
    const rightActive = ACTIVE_LIFECYCLES.has(right.lifecycle) ? 0 : 1
    return leftActive - rightActive || left.gameId.localeCompare(right.gameId)
  })
}

async function closeOverlay(): Promise<void> {
  const window = getCurrentWebviewWindow()
  await window.setIgnoreCursorEvents(true)
  await window.hide()
}

export default function Overlay() {
  const [sessions, setSessions] = useState<GameSessionStateV1[]>([])
  const [achievementState, setAchievementState] = useState<AchievementState | null>(null)
  const [activity, setActivity] = useState<AchievementEventV2[]>([])
  const [tab, setTab] = useState<OverlayTab>('session')
  const [loadError, setLoadError] = useState<string | null>(null)

  const session = useMemo(
    () => sessions.find((item) => ACTIVE_LIFECYCLES.has(item.lifecycle)) ?? sessions[0] ?? null,
    [sessions],
  )

  useEffect(() => {
    let disposed = false
    let unlistenState: UnlistenFn | null = null

    void listGameSessionStates()
      .then((next) => {
        if (!disposed) setSessions(next)
      })
      .catch((error) => {
        if (!disposed) setLoadError(String(error))
      })

    void listen<GameSessionStateV1>('launcher://game-session-state', (event) => {
      if (!disposed) setSessions((current) => upsertSession(current, event.payload))
    }).then((unlisten) => {
      if (disposed) unlisten()
      else unlistenState = unlisten
    }).catch((error) => {
      if (!disposed) setLoadError(String(error))
    })

    const unsubscribeAchievements = subscribeAchievementEvents((event) => {
      if (disposed) return
      setActivity((current) => [event, ...current.filter((item) => item.eventId !== event.eventId)].slice(0, 64))
    })

    return () => {
      disposed = true
      unlistenState?.()
      unsubscribeAchievements()
    }
  }, [])

  useEffect(() => {
    if (!session || !ACTIVE_LIFECYCLES.has(session.lifecycle)) {
      setAchievementState(null)
      return
    }
    let disposed = false
    void getSessionAchievementState(session.gameId)
      .then((next) => {
        if (!disposed) setAchievementState(next)
      })
      .catch(() => {
        if (!disposed) setAchievementState(null)
      })
    return () => { disposed = true }
  }, [session?.gameId, session?.latestAchievementSequence, session?.sessionId, session?.lifecycle])

  const currentActivity = activity.filter((item) => (
    session && item.sessionId === session.sessionId && item.gameId === session.gameId
  ))
  const achievementRows = achievementState?.schema.map((schema) => ({
    schema,
    record: achievementState.achievements[schema.id],
  })) ?? []
  const unlockedCount = achievementRows.filter((item) => item.record?.unlocked).length

  if (!session || !ACTIVE_LIFECYCLES.has(session.lifecycle)) {
    return (
      <main className="diagnostic-overlay diagnostic-overlay--idle" aria-label="0xoLemon diagnostic overlay">
        <section className="overlay-idle-card" role="status">
          <div className="overlay-brand-mark"><Gamepad2 aria-hidden="true" /></div>
          <p className="overlay-eyebrow">Desktop fallback</p>
          <h1>No active game session</h1>
          <p>Shift+F1 stays dormant until a launcher-owned game process is running.</p>
          {loadError && <p className="overlay-error">Session read failed: {loadError}</p>}
          <button type="button" className="overlay-close-action" onClick={() => void closeOverlay()}>
            <X size={16} aria-hidden="true" /> Close
          </button>
        </section>
      </main>
    )
  }

  return (
    <main className="diagnostic-overlay" aria-label="0xoLemon game session dashboard">
      <header className="overlay-command-bar">
        <div className="overlay-brand">
          <span className="overlay-brand-mark"><Gamepad2 aria-hidden="true" /></span>
          <span>
            <strong>0xoLemon</strong>
            <small>Session diagnostics</small>
          </span>
        </div>
        <nav className="overlay-tabs" aria-label="Overlay views">
          <button type="button" className={tab === 'session' ? 'is-active' : ''} onClick={() => setTab('session')}>
            <ShieldCheck size={17} aria-hidden="true" /> Session
          </button>
          <button type="button" className={tab === 'achievements' ? 'is-active' : ''} onClick={() => setTab('achievements')}>
            <Trophy size={17} aria-hidden="true" /> Achievements
          </button>
          <button type="button" className={tab === 'performance' ? 'is-active' : ''} onClick={() => setTab('performance')}>
            <Gauge size={17} aria-hidden="true" /> Performance
          </button>
        </nav>
        <button type="button" className="overlay-icon-button" aria-label="Close diagnostic overlay" onClick={() => void closeOverlay()}>
          <X aria-hidden="true" />
        </button>
      </header>

      <section className="overlay-session-strip" aria-live="polite">
        <div>
          <p className="overlay-eyebrow">Active game</p>
          <h1>{session.gameId}</h1>
        </div>
        <span className={`overlay-status overlay-status--${session.lifecycle}`}>
          <CircleDot size={14} aria-hidden="true" /> {session.lifecycle}
        </span>
        <dl>
          <div><dt>AppID</dt><dd>{session.appId || 'Not reported'}</dd></div>
          <div><dt>Session</dt><dd title={session.sessionId}>{session.sessionId.slice(0, 12)}</dd></div>
          <div><dt>Renderer</dt><dd>{rendererLabel(session.renderer)}</dd></div>
        </dl>
      </section>

      {tab === 'session' && (
        <section className="overlay-grid overlay-grid--session">
          <article className="overlay-panel overlay-panel--primary">
            <div className="overlay-panel-heading">
              <span><Radio size={18} aria-hidden="true" /></span>
              <div><p className="overlay-eyebrow">Runtime link</p><h2>{transportLabel(session.achievementTransport)}</h2></div>
            </div>
            <dl className="overlay-detail-list">
              <div><dt>Launcher PID</dt><dd>{session.rootPid || 'Waiting'}</dd></div>
              <div><dt>Runtime PID</dt><dd>{session.runtimePid ?? 'Not bound'}</dd></div>
              <div><dt>Last sequence</dt><dd>{session.latestAchievementSequence}</dd></div>
              <div><dt>Dropped events</dt><dd>{session.droppedEventCount}</dd></div>
              <div><dt>Network state</dt><dd>Not reported by runtime</dd></div>
            </dl>
          </article>

          <article className="overlay-panel">
            <div className="overlay-panel-heading">
              <span><Trophy size={18} aria-hidden="true" /></span>
              <div><p className="overlay-eyebrow">Achievement state</p><h2>{unlockedCount} / {achievementRows.length}</h2></div>
            </div>
            <div className="overlay-activity-list">
              {currentActivity.slice(0, 5).map((event) => (
                <div className="overlay-activity-item" key={event.eventId}>
                  <span className={`overlay-event-kind overlay-event-kind--${event.kind}`}>{event.kind}</span>
                  <span><strong>{event.name || event.achievementId || 'Runtime event'}</strong><small>Sequence {event.sequence} · {event.source}</small></span>
                </div>
              ))}
              {currentActivity.length === 0 && <p className="overlay-empty">No session activity received yet.</p>}
            </div>
          </article>

          <article className="overlay-panel overlay-panel--notice">
            <Activity size={18} aria-hidden="true" />
            <div>
              <p className="overlay-eyebrow">Fallback boundary</p>
              <h2>Shift+F1 diagnostics</h2>
              <p>Native Shift+Tab remains owned by the approved per-game renderer. This window does not inject into the game process.</p>
            </div>
          </article>
        </section>
      )}

      {tab === 'achievements' && (
        <section className="overlay-panel overlay-achievements">
          <div className="overlay-panel-title-row">
            <div><p className="overlay-eyebrow">Universal event model</p><h2>Achievements</h2></div>
            <span>{unlockedCount} unlocked · {achievementRows.length - unlockedCount} locked</span>
          </div>
          <div className="overlay-achievement-list">
            {achievementRows.map(({ schema, record }) => (
              <article className={record?.unlocked ? 'is-unlocked' : ''} key={schema.id}>
                <span className="overlay-achievement-icon"><Trophy aria-hidden="true" /></span>
                <div><h3>{schema.name || schema.id}</h3><p>{schema.description || 'No description supplied by the runtime.'}</p></div>
                <div className="overlay-achievement-progress">
                  <strong>{record?.unlocked ? 'Unlocked' : 'Locked'}</strong>
                  <small>{record?.progress ?? 0} / {record?.target ?? schema.target}</small>
                </div>
              </article>
            ))}
            {achievementRows.length === 0 && <p className="overlay-empty">The active runtime has not supplied an achievement schema.</p>}
          </div>
        </section>
      )}

      {tab === 'performance' && (
        <section className="overlay-grid overlay-grid--metrics">
          {[
            ['Frame interval p95', formatMetric(session.metrics.frameIntervalP95Ms, 'ms')],
            ['Overlay callback p95', formatMetric(session.metrics.overlayCallbackP95Ms, 'ms')],
            ['Private bytes', formatBytes(session.metrics.privateBytes)],
            ['Commit bytes', formatBytes(session.metrics.commitBytes)],
            ['VRAM', formatBytes(session.metrics.vramBytes)],
            ['Handles', session.metrics.handleCount?.toString() ?? 'Not sampled'],
            ['Queue depth', session.metrics.queueDepth.toString()],
            ['Samples', session.metrics.sampleCount.toString()],
          ].map(([label, value]) => (
            <article className="overlay-metric" key={label}>
              <p>{label}</p><strong>{value}</strong>
            </article>
          ))}
          <p className="overlay-metric-note">Unsampled values stay explicit; the dashboard never substitutes mock FPS, CPU, GPU or memory figures.</p>
        </section>
      )}

      <footer className="overlay-footer">
        <span>Desktop/windowed fallback</span>
        <span>Press <kbd>Shift</kbd> + <kbd>F1</kbd> to close</span>
      </footer>
    </main>
  )
}
