import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import type { OSTTrack } from '../hooks/useOSTData'

export interface GlobalAudioState {
  activeGameId: string | null
  gameTitle: string
  bgImage: string | undefined
  tracks: OSTTrack[]
  activeTrackIndex: number
  activeTrack: OSTTrack | null
  isPlaying: boolean
  playMode: 'sequential' | 'repeat' | 'shuffle'
  progress: number
  currentTime: number
  duration: number
  actualDurations: Record<string, string>
  isDetailActive: boolean
}

export interface GlobalAudioContextType extends GlobalAudioState {
  playTrack: (gameId: string, gameTitle: string, bgImage: string | undefined, tracks: OSTTrack[], index: number) => void
  setTracksForGame: (gameId: string, gameTitle: string, bgImage: string | undefined, tracks: OSTTrack[]) => void
  togglePlayPause: () => void
  nextTrack: () => void
  prevTrack: () => void
  togglePlayMode: () => void
  setDetailActive: (gameId: string | null, active: boolean) => void
  seek: (progressPct: number) => void
  navigateToActiveGame: () => void
  registerNavigateCallback: (cb: (gameId: string) => void) => void
}

const GlobalAudioContext = createContext<GlobalAudioContextType | null>(null)

type PlayMode = GlobalAudioState['playMode']

function clampTrackIndex(index: number, length: number) {
  if (length <= 0) return 0
  return Math.min(length - 1, Math.max(0, Math.trunc(index)))
}

export function GlobalAudioProvider({ children }: { children: React.ReactNode }) {
  const [activeGameId, setActiveGameId] = useState<string | null>(null)
  const [gameTitle, setGameTitle] = useState('')
  const [bgImage, setBgImage] = useState<string | undefined>(undefined)
  const [tracks, setTracks] = useState<OSTTrack[]>([])
  const [activeTrackIndex, setActiveTrackIndex] = useState(0)
  const [isPlaying, setIsPlaying] = useState(false)
  const [playMode, setPlayMode] = useState<PlayMode>('sequential')
  const [progress, setProgress] = useState(0)
  const [currentTime, setCurrentTime] = useState(0)
  const [duration, setDuration] = useState(0)
  const [actualDurations, setActualDurations] = useState<Record<string, string>>({})
  const [currentVisibleDetailGameId, setCurrentVisibleDetailGameId] = useState<string | null>(null)

  // The media element lives here, above every launcher tab. No page/detail component owns it,
  // so navigation cannot unmount the real player or reset playback.
  const audioRef = useRef<HTMLAudioElement | null>(null)
  const navigateCallbackRef = useRef<((gameId: string) => void) | null>(null)
  const playRequestIdRef = useRef(0)

  // Event/callback handlers need the latest playlist state without being recreated on every
  // timeupdate tick. Keep render state and imperative media state deliberately separated.
  const activeGameIdRef = useRef<string | null>(null)
  const tracksRef = useRef<OSTTrack[]>([])
  const activeTrackIndexRef = useRef(0)
  const isPlayingRef = useRef(false)
  const playModeRef = useRef<PlayMode>('sequential')

  useEffect(() => { activeGameIdRef.current = activeGameId }, [activeGameId])
  useEffect(() => { tracksRef.current = tracks }, [tracks])
  useEffect(() => { activeTrackIndexRef.current = activeTrackIndex }, [activeTrackIndex])
  useEffect(() => { isPlayingRef.current = isPlaying }, [isPlaying])
  useEffect(() => { playModeRef.current = playMode }, [playMode])

  const activeTrack = tracks.length > 0 && activeTrackIndex >= 0 && activeTrackIndex < tracks.length
    ? tracks[activeTrackIndex]
    : null

  const isDetailActive = Boolean(activeGameId && currentVisibleDetailGameId === activeGameId)

  const resetProgressState = useCallback(() => {
    setProgress(0)
    setCurrentTime(0)
    setDuration(0)
  }, [])

  const pauseElement = useCallback(() => {
    playRequestIdRef.current += 1
    const audio = audioRef.current
    if (audio && !audio.paused) audio.pause()
    isPlayingRef.current = false
    setIsPlaying(false)
  }, [])

  const requestPlay = useCallback((audio: HTMLAudioElement) => {
    const requestId = ++playRequestIdRef.current
    void audio.play().then(() => {
      if (playRequestIdRef.current !== requestId) return
      isPlayingRef.current = !audio.paused
      setIsPlaying(!audio.paused)
    }).catch((error: unknown) => {
      if (playRequestIdRef.current !== requestId) return
      isPlayingRef.current = false
      setIsPlaying(false)
      console.warn('[OST] Playback could not start', error)
    })
  }, [])

  const loadTrackIntoElement = useCallback((track: OSTTrack, shouldPlay: boolean, resetPosition = true) => {
    const audio = audioRef.current
    if (!audio) return

    const sourceChanged = audio.dataset.oxoTrackId !== track.id || audio.getAttribute('src') !== track.url
    if (sourceChanged) {
      // Invalidate a play() Promise belonging to the previous source before replacing it.
      playRequestIdRef.current += 1
      audio.pause()
      audio.dataset.oxoTrackId = track.id
      audio.src = track.url
      audio.load()
    }

    if (resetPosition && !sourceChanged) {
      try {
        audio.currentTime = 0
      } catch {
        // Metadata may not be available yet. The freshly selected source already starts at 0.
      }
    }

    resetProgressState()
    isPlayingRef.current = false
    setIsPlaying(false)
    if (shouldPlay) requestPlay(audio)
  }, [requestPlay, resetProgressState])

  const changeTrackIndex = useCallback((requestedIndex: number, shouldPlay = isPlayingRef.current) => {
    const currentTracks = tracksRef.current
    if (currentTracks.length === 0) return

    const newIndex = clampTrackIndex(requestedIndex, currentTracks.length)
    const track = currentTracks[newIndex]
    if (!track) return

    activeTrackIndexRef.current = newIndex
    setActiveTrackIndex(newIndex)
    loadTrackIntoElement(track, shouldPlay, true)
  }, [loadTrackIntoElement])

  const playTrack = useCallback((
    newGameId: string,
    newGameTitle: string,
    newBgImage: string | undefined,
    newTracks: OSTTrack[],
    index: number,
  ) => {
    if (newTracks.length === 0) return
    const newIndex = clampTrackIndex(index, newTracks.length)
    const track = newTracks[newIndex]
    if (!track) return

    activeGameIdRef.current = newGameId
    tracksRef.current = newTracks
    activeTrackIndexRef.current = newIndex

    setActiveGameId(newGameId)
    setGameTitle(newGameTitle)
    setBgImage(newBgImage)
    setTracks(newTracks)
    setActiveTrackIndex(newIndex)
    loadTrackIntoElement(track, true, true)
  }, [loadTrackIntoElement])

  const setTracksForGame = useCallback((
    newGameId: string,
    newGameTitle: string,
    newBgImage: string | undefined,
    newTracks: OSTTrack[],
  ) => {
    if (activeGameIdRef.current !== newGameId || newTracks.length === 0) return

    const oldTracks = tracksRef.current
    const oldIndex = clampTrackIndex(activeTrackIndexRef.current, oldTracks.length)
    const oldTrack = oldTracks[oldIndex] ?? null
    const preservedIndex = oldTrack
      ? newTracks.findIndex((candidate) => candidate.id === oldTrack.id)
      : -1
    const nextIndex = preservedIndex >= 0
      ? preservedIndex
      : clampTrackIndex(activeTrackIndexRef.current, newTracks.length)
    const nextTrack = newTracks[nextIndex]

    tracksRef.current = newTracks
    activeTrackIndexRef.current = nextIndex
    setGameTitle(newGameTitle)
    setBgImage(newBgImage)
    setTracks(newTracks)
    setActiveTrackIndex(nextIndex)

    // Metadata refreshes often rebuild the array. Do not restart a song merely because the
    // array identity changed; only replace the media source when the active track disappeared.
    if (oldTrack && nextTrack && oldTrack.id !== nextTrack.id) {
      loadTrackIntoElement(nextTrack, isPlayingRef.current, true)
    }
  }, [loadTrackIntoElement])

  const togglePlayPause = useCallback(() => {
    const audio = audioRef.current
    const currentTracks = tracksRef.current
    const currentTrack = currentTracks[activeTrackIndexRef.current]
    if (!audio || !currentTrack) return

    if (isPlayingRef.current && !audio.paused) {
      pauseElement()
      return
    }

    // A stopped-at-end track should restart naturally when Play is pressed again.
    if (audio.ended) {
      try { audio.currentTime = 0 } catch { /* metadata not ready */ }
    }
    requestPlay(audio)
  }, [pauseElement, requestPlay])

  const nextTrack = useCallback(() => {
    const currentTracks = tracksRef.current
    if (currentTracks.length === 0) return

    let nextIndex: number
    if (playModeRef.current === 'shuffle') {
      nextIndex = Math.floor(Math.random() * currentTracks.length)
      if (nextIndex === activeTrackIndexRef.current && currentTracks.length > 1) {
        nextIndex = (nextIndex + 1) % currentTracks.length
      }
    } else {
      nextIndex = (activeTrackIndexRef.current + 1) % currentTracks.length
    }
    changeTrackIndex(nextIndex, isPlayingRef.current)
  }, [changeTrackIndex])

  const prevTrack = useCallback(() => {
    const currentTracks = tracksRef.current
    if (currentTracks.length === 0) return
    const previousIndex = activeTrackIndexRef.current === 0
      ? currentTracks.length - 1
      : activeTrackIndexRef.current - 1
    changeTrackIndex(previousIndex, isPlayingRef.current)
  }, [changeTrackIndex])

  const togglePlayMode = useCallback(() => {
    setPlayMode((previous) => {
      const next: PlayMode = previous === 'sequential'
        ? 'repeat'
        : previous === 'repeat'
          ? 'shuffle'
          : 'sequential'
      playModeRef.current = next
      return next
    })
  }, [])

  const setDetailActive = useCallback((gameId: string | null, active: boolean) => {
    if (active) {
      setCurrentVisibleDetailGameId(gameId)
    } else {
      setCurrentVisibleDetailGameId((previous) => (previous === gameId ? null : previous))
    }
  }, [])

  const seek = useCallback((progressPct: number) => {
    const audio = audioRef.current
    if (!audio || !Number.isFinite(audio.duration) || audio.duration <= 0) return
    const safePercent = Math.max(0, Math.min(100, progressPct))
    const targetTime = (safePercent / 100) * audio.duration
    audio.currentTime = targetTime
    setCurrentTime(targetTime)
    setProgress(safePercent)
  }, [])

  const registerNavigateCallback = useCallback((cb: (gameId: string) => void) => {
    navigateCallbackRef.current = cb
  }, [])

  const navigateToActiveGame = useCallback(() => {
    const gameId = activeGameIdRef.current
    if (gameId && navigateCallbackRef.current) navigateCallbackRef.current(gameId)
  }, [])

  const handleTimeUpdate = useCallback(() => {
    const audio = audioRef.current
    if (!audio) return
    const nextCurrentTime = Number.isFinite(audio.currentTime) ? audio.currentTime : 0
    const nextDuration = Number.isFinite(audio.duration) && audio.duration > 0 ? audio.duration : 0
    setCurrentTime(nextCurrentTime)
    if (nextDuration > 0) {
      setDuration(nextDuration)
      setProgress(Math.max(0, Math.min(100, (nextCurrentTime / nextDuration) * 100)))
    }
  }, [])

  const handleLoadedMetadata = useCallback(() => {
    const audio = audioRef.current
    const currentTrack = tracksRef.current[activeTrackIndexRef.current]
    if (!audio || !currentTrack || !Number.isFinite(audio.duration) || audio.duration <= 0) return

    const mediaDuration = audio.duration
    setDuration(mediaDuration)
    if (!currentTrack.durationStr || currentTrack.durationStr === '0:00') {
      const mins = Math.floor(mediaDuration / 60)
      const secs = Math.floor(mediaDuration % 60).toString().padStart(2, '0')
      setActualDurations((previous) => (
        previous[currentTrack.id]
          ? previous
          : { ...previous, [currentTrack.id]: `${mins}:${secs}` }
      ))
    }
  }, [])

  const handleEnded = useCallback(() => {
    const currentTracks = tracksRef.current
    if (currentTracks.length === 0) return

    if (playModeRef.current === 'repeat') {
      const audio = audioRef.current
      if (!audio) return
      audio.currentTime = 0
      requestPlay(audio)
      return
    }

    if (playModeRef.current === 'shuffle') {
      let nextIndex = Math.floor(Math.random() * currentTracks.length)
      if (nextIndex === activeTrackIndexRef.current && currentTracks.length > 1) {
        nextIndex = (nextIndex + 1) % currentTracks.length
      }
      changeTrackIndex(nextIndex, true)
      return
    }

    if (activeTrackIndexRef.current < currentTracks.length - 1) {
      changeTrackIndex(activeTrackIndexRef.current + 1, true)
      return
    }

    // Sequential playback stops on the real final track. Keep the UI and media element on the
    // same track so pressing Play reliably restarts it instead of pointing at a different item.
    isPlayingRef.current = false
    setIsPlaying(false)
    setProgress(100)
  }, [changeTrackIndex, requestPlay])

  const handlePlay = useCallback(() => {
    isPlayingRef.current = true
    setIsPlaying(true)
  }, [])

  const handlePause = useCallback(() => {
    isPlayingRef.current = false
    setIsPlaying(false)
  }, [])

  const handleError = useCallback(() => {
    isPlayingRef.current = false
    setIsPlaying(false)
  }, [])

  const value = useMemo<GlobalAudioContextType>(() => ({
    activeGameId,
    gameTitle,
    bgImage,
    tracks,
    activeTrackIndex,
    activeTrack,
    isPlaying,
    playMode,
    progress,
    currentTime,
    duration,
    actualDurations,
    isDetailActive,
    playTrack,
    setTracksForGame,
    togglePlayPause,
    nextTrack,
    prevTrack,
    togglePlayMode,
    setDetailActive,
    seek,
    navigateToActiveGame,
    registerNavigateCallback,
  }), [
    activeGameId,
    gameTitle,
    bgImage,
    tracks,
    activeTrackIndex,
    activeTrack,
    isPlaying,
    playMode,
    progress,
    currentTime,
    duration,
    actualDurations,
    isDetailActive,
    playTrack,
    setTracksForGame,
    togglePlayPause,
    nextTrack,
    prevTrack,
    togglePlayMode,
    setDetailActive,
    seek,
    navigateToActiveGame,
    registerNavigateCallback,
  ])

  return (
    <GlobalAudioContext.Provider value={value}>
      {children}
      <audio
        ref={audioRef}
        preload="metadata"
        onTimeUpdate={handleTimeUpdate}
        onLoadedMetadata={handleLoadedMetadata}
        onEnded={handleEnded}
        onPlay={handlePlay}
        onPause={handlePause}
        onError={handleError}
      />
    </GlobalAudioContext.Provider>
  )
}

export function useGlobalAudio(): GlobalAudioContextType {
  const context = useContext(GlobalAudioContext)
  if (!context) throw new Error('useGlobalAudio must be used within a GlobalAudioProvider')
  return context
}
