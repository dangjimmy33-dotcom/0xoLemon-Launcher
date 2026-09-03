import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type { AchievementEventV2 } from '../types'
import { isTauriRuntime } from './gameMeta'

type AchievementSubscriber = (event: AchievementEventV2) => void

type LegacyAchievementUnlockedEvent = {
  gameId: string
  id: string
  name: string
  description: string
  unlockedAt: string
}

const subscribers = new Set<AchievementSubscriber>()
const seenEventIds = new Set<string>()
const eventOrder: string[] = []
const recentCanonicalUnlocks = new Map<string, number>()
const recentCanonicalOrder: string[] = []
const MAX_SEEN_EVENTS = 2048
const LEGACY_SUPPRESSION_WINDOW_MS = 10_000

let listenerPromise: Promise<void> | null = null
let unlisteners: UnlistenFn[] = []

function unlockFingerprint(gameId: string, achievementId: string): string {
  return `${gameId}:${achievementId}`
}

function rememberEventId(eventId: string): boolean {
  if (seenEventIds.has(eventId)) return false
  seenEventIds.add(eventId)
  eventOrder.push(eventId)
  while (eventOrder.length > MAX_SEEN_EVENTS) {
    const expired = eventOrder.shift()
    if (expired) seenEventIds.delete(expired)
  }
  return true
}

function dispatch(event: AchievementEventV2): void {
  if (!rememberEventId(event.eventId)) return
  if (event.kind === 'unlock') {
    const fingerprint = unlockFingerprint(event.gameId, event.achievementId)
    if (!recentCanonicalUnlocks.has(fingerprint)) recentCanonicalOrder.push(fingerprint)
    recentCanonicalUnlocks.set(fingerprint, Date.now())
    while (recentCanonicalOrder.length > MAX_SEEN_EVENTS) {
      const expired = recentCanonicalOrder.shift()
      if (expired) recentCanonicalUnlocks.delete(expired)
    }
  }
  for (const subscriber of subscribers) subscriber(event)
}

function normalizeLegacyEvent(event: LegacyAchievementUnlockedEvent): AchievementEventV2 | null {
  const fingerprint = unlockFingerprint(event.gameId, event.id)
  const canonicalAt = recentCanonicalUnlocks.get(fingerprint)
  if (canonicalAt !== undefined && Date.now() - canonicalAt <= LEGACY_SUPPRESSION_WINDOW_MS) {
    return null
  }
  const parsedTimestamp = Date.parse(event.unlockedAt)
  return {
    schemaVersion: 2,
    eventId: `launcher:${fingerprint}:${event.unlockedAt}`,
    sequence: 0,
    sessionId: 'launcher',
    gameId: event.gameId,
    appId: 0,
    achievementId: event.id,
    kind: 'unlock',
    name: event.name,
    description: event.description,
    occurredAt: Number.isFinite(parsedTimestamp) ? parsedTimestamp : Date.now(),
    source: 'scopedFallback',
  }
}

async function ensureListeners(): Promise<void> {
  if (!isTauriRuntime() || unlisteners.length > 0) return
  const [unlistenV2, unlistenLegacy] = await Promise.all([
    listen<AchievementEventV2>('launcher://achievement-event-v2', (event) => {
      dispatch(event.payload)
    }),
    listen<LegacyAchievementUnlockedEvent>('launcher://achievement-unlocked', (event) => {
      const normalized = normalizeLegacyEvent(event.payload)
      if (normalized) dispatch(normalized)
    }),
  ])
  unlisteners = [unlistenV2, unlistenLegacy]
}

export function subscribeAchievementEvents(subscriber: AchievementSubscriber): () => void {
  subscribers.add(subscriber)
  listenerPromise ??= ensureListeners().catch((error) => {
    listenerPromise = null
    console.error('[achievement] Could not attach the shared event bus', error)
  })
  return () => {
    subscribers.delete(subscriber)
  }
}

export function shutdownAchievementEventBus(): void {
  for (const unlisten of unlisteners) unlisten()
  unlisteners = []
  listenerPromise = null
  subscribers.clear()
  seenEventIds.clear()
  eventOrder.length = 0
  recentCanonicalUnlocks.clear()
  recentCanonicalOrder.length = 0
}
