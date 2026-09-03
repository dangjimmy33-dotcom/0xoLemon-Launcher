import { useEffect, useState, useCallback } from 'react'
import {
  VIETNAMESE_TRANSLATIONS_DATA,
  type VietnameseTranslationItem,
} from '../data/vietnameseTranslations'

const REMOTE_TRANSLATION_URLS = [
  'https://huggingface.co/datasets/JOINCANE/0XoLemon/raw/main/translations.json',
  '/translations.json',
]

const CACHE_KEY = '0xo_cached_translations_v3'

function readCachedTranslations(): VietnameseTranslationItem[] {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed)) {
        return parsed
      }
    }
  } catch {}
  return VIETNAMESE_TRANSLATIONS_DATA
}

export function useRealtimeTranslations() {
  const [translations, setTranslations] = useState<VietnameseTranslationItem[]>(readCachedTranslations)
  const [isSyncing, setIsSyncing] = useState(false)
  const [lastSyncTime, setLastSyncTime] = useState<number | null>(null)

  const syncTranslations = useCallback(async () => {
    setIsSyncing(true)
    for (const url of REMOTE_TRANSLATION_URLS) {
      try {
        const res = await fetch(url, { headers: { 'Cache-Control': 'no-cache' } })
        if (res.ok) {
          const text = await res.text()
          const cleaned = text.replace(/^\uFEFF/, '').trim()
          const data = JSON.parse(cleaned)
          if (Array.isArray(data)) {
            setTranslations(data)
            try {
              localStorage.setItem(CACHE_KEY, JSON.stringify(data))
            } catch {}
            setLastSyncTime(Date.now())
            setIsSyncing(false)
            return
          }
        }
      } catch (err) {
        console.warn(`Could not sync translations from ${url}:`, err)
      }
    }
    setIsSyncing(false)
  }, [])

  useEffect(() => {
    void syncTranslations()
  }, [syncTranslations])

  return {
    translations,
    isSyncing,
    lastSyncTime,
    refresh: syncTranslations,
  }
}
