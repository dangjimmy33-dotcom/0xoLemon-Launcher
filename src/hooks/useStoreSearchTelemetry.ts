import { useCallback, useEffect, useRef, useState } from 'react'
import { normalizeStoreSearchTerm } from '../lib/storeSearch'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com'
const TENANT_ID = import.meta.env.VITE_TENANT_ID || '0xolemon1'

export type StoreSearchTermStat = {
  term: string
  searches: number
  lastSearchedAt?: string
}

export type StoreSearchStats = {
  trending: StoreSearchTermStat[]
  query: StoreSearchTermStat | null
  gameClicks: Record<string, number>
  generatedAt: string
}

type SearchEventSource = 'stable-query' | 'submit' | 'suggestion-click' | 'result-click'

const EMPTY_STATS: StoreSearchStats = {
  trending: [],
  query: null,
  gameClicks: {},
  generatedAt: '',
}

export function useStoreSearchTelemetry(open: boolean, query: string) {
  const [stats, setStats] = useState<StoreSearchStats>(EMPTY_STATS)
  const [loading, setLoading] = useState(false)
  const recordedTerms = useRef(new Set<string>())
  const wasOpen = useRef(false)

  useEffect(() => {
    if (open && !wasOpen.current) recordedTerms.current.clear()
    wasOpen.current = open
  }, [open])

  useEffect(() => {
    if (!open) return
    const controller = new AbortController()
    const timer = window.setTimeout(async () => {
      setLoading(true)
      try {
        const normalized = normalizeStoreSearchTerm(query)
        const params = normalized ? `?query=${encodeURIComponent(normalized)}` : ''
        const response = await fetch(`${BACKEND_URL}/api/${TENANT_ID}/search-stats${params}`, {
          signal: controller.signal,
        })
        if (!response.ok) throw new Error(`Search stats returned ${response.status}`)
        const payload = await response.json() as StoreSearchStats
        setStats({
          trending: Array.isArray(payload.trending) ? payload.trending : [],
          query: payload.query || null,
          gameClicks: payload.gameClicks || {},
          generatedAt: payload.generatedAt || '',
        })
      } catch (error) {
        if (!controller.signal.aborted) console.debug('[store-search] Search stats are unavailable:', error)
      } finally {
        if (!controller.signal.aborted) setLoading(false)
      }
    }, query.trim() ? 420 : 0)

    return () => {
      window.clearTimeout(timer)
      controller.abort()
    }
  }, [open, query])

  const postEvent = useCallback(async (
    source: SearchEventSource,
    term: string,
    resultCount: number,
    selectedGameId?: string,
  ) => {
    const normalized = normalizeStoreSearchTerm(term)
    if (normalized.length < 2) return

    try {
      await fetch(`${BACKEND_URL}/api/${TENANT_ID}/search-events`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: normalized, resultCount, selectedGameId, source }),
        keepalive: true,
      })
    } catch (error) {
      console.warn('[store-search] Could not record search event:', error)
    }
  }, [])

  const recordSearch = useCallback((source: Exclude<SearchEventSource, 'result-click'>, term: string, resultCount: number) => {
    const normalized = normalizeStoreSearchTerm(term)
    if (normalized.length < 2 || recordedTerms.current.has(normalized)) return
    recordedTerms.current.add(normalized)
    void postEvent(source, normalized, resultCount)
  }, [postEvent])

  const recordResultClick = useCallback((term: string, resultCount: number, gameId: string) => {
    void postEvent('result-click', term, resultCount, gameId)
    setStats((current) => ({
      ...current,
      gameClicks: {
        ...current.gameClicks,
        [gameId]: (current.gameClicks[gameId] || 0) + 1,
      },
    }))
  }, [postEvent])

  return { stats, loading, recordSearch, recordResultClick }
}
