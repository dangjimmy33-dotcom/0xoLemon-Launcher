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

// Module-level cache: repo URL per game (avoids multi-repo scan within session)
const ostRepoCache: Record<string, { treeUrl: string, resolveBaseUrl: string, token: string | null }> = {}

const TRACKS_CACHE_PREFIX = 'ost_tracks_v1_'

function loadTracksFromStorage(gameId: string): OSTTrack[] | null {
  try {
    const raw = localStorage.getItem(TRACKS_CACHE_PREFIX + gameId)
    if (raw) return JSON.parse(raw) as OSTTrack[]
  } catch {}
  return null
}

function saveTracksToStorage(gameId: string, tracks: OSTTrack[]) {
  try {
    localStorage.setItem(TRACKS_CACHE_PREFIX + gameId, JSON.stringify(tracks))
  } catch {}
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
      setTracks([])
      return
    }

    // Already have tracks from localStorage — skip fetch entirely
    const cached = loadTracksFromStorage(gameId)
    if (cached && cached.length > 0) {
      setTracks(cached)
      return
    }

    const fetchTracks = async () => {
      setLoading(true)
      setError(null)
      const loadedTracks: OSTTrack[] = []

      try {
        const repoInfos: [string, string, string | null][] = await invoke('get_game_ost_repo_info', { gameId })
        if (!repoInfos || repoInfos.length === 0) {
          throw new Error('No repository configured for this game')
        }

        let mp3Files: { type: string, path: string, size?: number, lfs?: { size: number } }[] = []
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
              const files = await res.json()
              const mp3s = files.filter((f: any) => f.type === 'file' && (f.path.toLowerCase().endsWith('.mp3') || f.path.toLowerCase().endsWith('.flac')))
              if (mp3s.length > 0) {
                mp3Files = mp3s
                activeResolveBaseUrl = cached.resolveBaseUrl
                activeHeaders = headers
              }
            }
          } catch (e) {
            console.warn('[OST] Failed to fetch from cached repo', e)
          }
        }

        // 2. If no results from cache, scan all configured repos
        if (mp3Files.length === 0) {
          for (const [treeUrl, resolveBaseUrl, token] of repoInfos) {
            const headers: Record<string, string> = {}
            if (token) headers['Authorization'] = `Bearer ${token}`
            try {
              const res = await fetch(`${treeUrl}?t=${Date.now()}`, { headers, cache: 'no-store' })
              if (res.ok) {
                const files = await res.json()
                const mp3s = files.filter((f: any) => f.type === 'file' && (f.path.toLowerCase().endsWith('.mp3') || f.path.toLowerCase().endsWith('.flac')))
                if (mp3s.length > 0) {
                  mp3Files = mp3s
                  activeResolveBaseUrl = resolveBaseUrl
                  activeHeaders = headers
                  console.log(`[OST] Found ${mp3s.length} tracks at ${treeUrl}`)
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

        if (mp3Files.length === 0) {
          console.log('[OST] No soundtracks found in any configured repo.')
          if (mounted) setTracks([])
          return
        }

        // Process each mp3 file and fetch ID3 metadata
        for (const file of mp3Files) {
          if (!mounted) return
          const fileName = (file.path as string).split('/').pop() || ''
          const encodedFileName = encodeURIComponent(fileName)
          const url = `${activeResolveBaseUrl}/${encodedFileName}`

          let title = fileName.replace(/\.(mp3|flac)$/i, '')
          let artist = 'Original Soundtrack'
          let durationStr = '0:00'

          try {
            const metaHeaders = { ...activeHeaders, Range: 'bytes=0-131072' }
            const metaRes = await fetch(url, { headers: metaHeaders })
            if (metaRes.ok || metaRes.status === 206) {
              const buffer = await metaRes.arrayBuffer()
              // For xet/LFS files the real size is in lfs.size; file.size is also real for xet
              const fileSize = (file as any).lfs?.size ?? ((file as any).size > 10000 ? (file as any).size : undefined)
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
      } catch (err: any) {
        if (mounted) setError(err.message)
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

  return { tracks, loading, error }
}
