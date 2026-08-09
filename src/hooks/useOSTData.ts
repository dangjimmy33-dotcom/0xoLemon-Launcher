import { useState, useEffect } from 'react'
import * as mm from 'music-metadata'
import { invoke } from '@tauri-apps/api/core'

export interface OSTTrack {
  id: string
  url: string
  title: string
  artist: string
  durationStr: string
}

type HfTreeFile = {
  type: string
  path: string
  size?: number
  lfs?: { size?: number }
}

function isAudioTreeFile(value: unknown): value is HfTreeFile {
  if (!value || typeof value !== 'object') return false
  const file = value as Record<string, unknown>
  if (file.type !== 'file' || typeof file.path !== 'string') return false
  const path = file.path.toLowerCase()
  return path.endsWith('.mp3') || path.endsWith('.flac')
}

function audioFilesFromResponse(value: unknown): HfTreeFile[] {
  return Array.isArray(value) ? value.filter(isAudioTreeFile) : []
}

// Module-level cache: repo URL per game (avoids multi-repo scan within session)
const ostRepoCache: Record<string, { treeUrl: string, resolveBaseUrl: string, token: string | null }> = {}

const TRACKS_CACHE_PREFIX = 'ost_tracks_v1_'

function loadTracksFromStorage(gameId: string): OSTTrack[] | null {
  try {
    const raw = localStorage.getItem(TRACKS_CACHE_PREFIX + gameId)
    if (raw) return JSON.parse(raw) as OSTTrack[]
  } catch {
    // Corrupt cache entries are ignored and replaced by fresh metadata.
  }
  return null
}

function saveTracksToStorage(gameId: string, tracks: OSTTrack[]) {
  try {
    localStorage.setItem(TRACKS_CACHE_PREFIX + gameId, JSON.stringify(tracks))
  } catch {
    // Storage quotas must not break soundtrack playback.
  }
}

export function useOSTData(gameId: string | null) {
  const [tracks, setTracks] = useState<OSTTrack[]>(() => {
    // Initialize from localStorage immediately — no loading flash on restart
    if (gameId) {
      const cached = loadTracksFromStorage(gameId)
      if (cached && cached.length > 0) return cached
    }
    return []
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let mounted = true
    if (!gameId) {
      return
    }

    const fetchTracks = async () => {
      setLoading(true)
      setError(null)
      const loadedTracks: OSTTrack[] = []

      // Load existing cache to skip re-fetching metadata for known tracks
      const cachedTracks = loadTracksFromStorage(gameId) || []
      const cachedTrackMap = new Map(cachedTracks.map(t => [t.url, t]))

      try {
        const repoInfos: [string, string, string | null][] = await invoke('get_game_ost_repo_info', { gameId })
        if (!repoInfos || repoInfos.length === 0) {
          throw new Error('No repository configured for this game')
        }

        let audioFiles: HfTreeFile[] = []
        let activeResolveBaseUrl = ''
        let activeHeaders: Record<string, string> = {}

        // 1. Try cached repo URL first (skip multi-repo scan)
        if (ostRepoCache[gameId]) {
          const cached = ostRepoCache[gameId]
          const headers: Record<string, string> = {}
          if (cached.token) headers['Authorization'] = `Bearer ${cached.token}`
          try {
            const res = await fetch(`${cached.treeUrl}?t=${Date.now()}`, { headers, cache: 'no-store' })
            if (res.ok) {
              const audios = audioFilesFromResponse(await res.json())
              if (audios.length > 0) {
                audioFiles = audios
                activeResolveBaseUrl = cached.resolveBaseUrl
                activeHeaders = headers
              }
            }
          } catch (e) {
            console.warn('[OST] Failed to fetch from cached repo', e)
          }
        }

        // 2. If no results from cache, scan all configured repos
        if (audioFiles.length === 0) {
          for (const [treeUrl, resolveBaseUrl, token] of repoInfos) {
            const headers: Record<string, string> = {}
            if (token) headers['Authorization'] = `Bearer ${token}`
            try {
              const res = await fetch(`${treeUrl}?t=${Date.now()}`, { headers, cache: 'no-store' })
              if (res.ok) {
                const audios = audioFilesFromResponse(await res.json())
                if (audios.length > 0) {
                  audioFiles = audios
                  activeResolveBaseUrl = resolveBaseUrl
                  activeHeaders = headers
                  console.log(`[OST] Found ${audios.length} tracks at ${treeUrl}`)
                  // Cache which repo had the music
                  ostRepoCache[gameId] = { treeUrl, resolveBaseUrl, token }
                  break
                }
              }
            } catch (e) {
              console.warn(`[OST] Failed to fetch from ${treeUrl}`, e)
            }
          }
        }

        if (audioFiles.length === 0) {
          console.log('[OST] No soundtracks found in any configured repo.')
          if (mounted) setTracks([])
          return
        }

        // Process each audio file and fetch ID3 metadata
        for (const file of audioFiles) {
          if (!mounted) return
          const fileName = file.path.split('/').pop() || ''
          const encodedFileName = encodeURIComponent(fileName)
          const url = `${activeResolveBaseUrl}/${encodedFileName}`

          // If we already have this track in cache, reuse it
          if (cachedTrackMap.has(url)) {
            loadedTracks.push(cachedTrackMap.get(url)!)
            continue
          }

          let title = fileName.replace(/\.(mp3|flac)$/i, '')
          let artist = 'Original Soundtrack'
          let durationStr = '0:00'

          try {
            const metaHeaders = { ...activeHeaders, Range: 'bytes=0-131072' }
            const metaRes = await fetch(url, { headers: metaHeaders })
            if (metaRes.ok || metaRes.status === 206) {
              const buffer = await metaRes.arrayBuffer()
              // For xet/LFS files the real size is in lfs.size; file.size is also real for xet
              const fileSize = file.lfs?.size ?? (file.size && file.size > 10_000 ? file.size : undefined)
              const isFlac = fileName.toLowerCase().endsWith('.flac')
              const metadata = await mm.parseBuffer(new Uint8Array(buffer), isFlac ? 'audio/flac' : 'audio/mpeg', {
                skipCovers: true,
                duration: true
              })
              if (metadata.common.title) title = metadata.common.title
              if (metadata.common.artist) artist = metadata.common.artist

              let durationSecs = metadata.format.duration
              // Estimate if not present in header (CBR without Xing frame)
              if (!durationSecs && metadata.format.bitrate && fileSize) {
                durationSecs = (fileSize * 8) / metadata.format.bitrate
              }
              if (durationSecs) {
                const mins = Math.floor(durationSecs / 60)
                const secs = Math.floor(durationSecs % 60).toString().padStart(2, '0')
                durationStr = `${mins}:${secs}`
              }
            }
          } catch (metaErr) {
            console.warn(`[OST] Failed to parse metadata for ${fileName}`, metaErr)
          }

          loadedTracks.push({ id: url, url, title, artist, durationStr })

          // Show tracks progressively as they load
          if (mounted) setTracks([...loadedTracks])
        }

        // Final update to ensure we show everything (especially if all were cached)
        if (mounted) {
          setTracks([...loadedTracks])
        }
      } catch (error) {
        if (mounted) setError(errorMessage(error))
      } finally {
        // Persist to localStorage — survives launcher restarts
        if (gameId && loadedTracks.length > 0) {
          saveTracksToStorage(gameId, loadedTracks)
        }
        if (mounted) setLoading(false)
      }
    }

    fetchTracks()

    return () => {
      mounted = false
    }
  }, [gameId])

  return { tracks: gameId ? tracks : [], loading: gameId ? loading : false, error: gameId ? error : null }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
