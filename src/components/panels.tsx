import { Archive, Download, HardDrive, RotateCcw, ShieldCheck, Square, Play, SkipBack, SkipForward, Music, ArrowRight, Repeat, Shuffle } from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
import type { ChangedFile, GameDetail, Snapshot } from '../types'
import { formatBytes, formatDelta } from '../lib/format'

export function CachePanel({
  snapshot,
  busy,
  onClear,
}: {
  snapshot: Snapshot
  busy: boolean
  onClear: () => void
}) {
  const radius = 38
  const circumference = 2 * Math.PI * radius
  const offset = circumference - (circumference * snapshot.cache.healthPercent) / 100

  return (
    <section className="panel metric-panel">
      <header className="side-header">
        <HardDrive size={17} />
        <strong>DOWNLOADED CHUNKS</strong>
      </header>
      <div className="cache-meter">
        <svg viewBox="0 0 96 96" aria-hidden="true">
          <circle cx="48" cy="48" r={radius} />
          <circle cx="48" cy="48" r={radius} style={{ strokeDasharray: circumference, strokeDashoffset: offset }} />
        </svg>
        <strong>{snapshot.cache.healthPercent}%</strong>
      </div>
      <dl className="metric-list">
        <div>
          <dt>Stored chunks</dt>
          <dd>{formatBytes(snapshot.cache.cacheSize)}</dd>
        </div>
        <div>
          <dt>Free space</dt>
          <dd>{formatBytes(snapshot.cache.freeSpace)}</dd>
        </div>
      </dl>
      <p className="cache-location" title={snapshot.cache.cachePath}>
        Stored beside the library in <code>{'downloading\\<game>\\chunks'}</code>, never copied to AppData.
      </p>
      <button type="button" disabled={busy || snapshot.cache.cacheSize === 0} onClick={onClear}>
        {busy ? 'CLEARING CACHE...' : snapshot.cache.cacheSize === 0 ? 'NO STORED CHUNKS' : 'CLEAR CHUNK CACHE'}
      </button>
    </section>
  )
}

export function RollbackPanel({ snapshot, rollbackVersion }: { snapshot: Snapshot; rollbackVersion: string }) {
  const rollbackKnown = snapshot.cache.rollbackReady || snapshot.cache.rollbackMissingBytes > 0

  return (
    <section className="panel rollback-panel">
      <header className="side-header">
        <RotateCcw size={17} />
        <strong>ROLLBACK READINESS</strong>
      </header>
      <div className="rollback-state">
        <span className={snapshot.cache.rollbackReady ? 'ready-pill' : 'warn-pill'}>
          {snapshot.cache.rollbackReady ? 'READY' : rollbackKnown ? 'NEEDS DOWNLOAD' : 'NOT PREPARED'}
        </span>
        <div>
          <strong>{snapshot.cache.rollbackReady ? `${rollbackVersion} rollback ready` : 'Rollback not staged'}</strong>
          <small>
            {snapshot.cache.rollbackReady
              ? 'All required chunks are cached.'
              : rollbackKnown
                ? `${formatBytes(snapshot.cache.rollbackMissingBytes)} required from proxy.`
                : 'Run verify/cache analysis before rollback.'}
          </small>
        </div>
      </div>
      <button type="button" disabled={!snapshot.cache.rollbackReady}>
        ROLLBACK TO {rollbackVersion}
      </button>
    </section>
  )
}

export function GameDetailsPanel({ detail }: { detail: GameDetail }) {
  const devData = detail.developers as unknown
  const devList: string[] = Array.isArray(devData) ? devData : (typeof devData === 'string' ? [devData] : [])
  const pubData = detail.publishers as unknown
  const pubList: string[] = Array.isArray(pubData) ? pubData : (typeof pubData === 'string' ? [pubData] : [])
  const genreData = detail.genres as unknown
  const genreList: string[] = Array.isArray(genreData) ? genreData : (typeof genreData === 'string' ? genreData.split(',').map((s: string)=>s.trim()) : [])
  const ratingsData = detail.ratings as unknown
  const ratingsList: any[] = Array.isArray(ratingsData) ? ratingsData : []

  return (
    <section className="panel game-info-panel">
      <header className="side-header">
        <ShieldCheck size={17} />
        <strong>{t.library.details}</strong>
      </header>
      <dl className="game-info-list">
        <div>
          <dt>Developer</dt>
          <dd>{devList.join(', ')}</dd>
        </div>
        <div>
          <dt>Publisher</dt>
          <dd>{pubList.join(', ')}</dd>
        </div>
        <div className="meta-pair">
          <dt>Release Date</dt>
          <dd>
            {typeof detail.releaseDate === 'object' && detail.releaseDate !== null 
              ? (detail.releaseDate as any).date 
              : detail.releaseDate || 'Unknown'}
          </dd>
        </div>
        <div>
          <dt>Genres</dt>
          <dd>
            {genreList.map((genre) => (
              <span key={genre}>{genre}</span>
            ))}
          </dd>
        </div>
      </dl>
      {ratingsList.map((rating) => (
        <div className="rating-strip" key={rating.source}>
          <strong>{rating.score}</strong>
          <span>{rating.source}</span>
        </div>
      ))}
    </section>
  )
}

export function InstallSummaryPanel({
  selectedVersion,
  downloadSize,
  installSize,
  temporarySpace,
}: {
  selectedVersion: string
  downloadSize: number
  installSize: number
  temporarySpace: number
}) {
  return (
    <section className="panel install-summary-panel">
      <header className="side-header">
        <Download size={17} />
        <strong>{t.library.install}</strong>
      </header>
      <dl className="metric-list">
        <div>
          <dt>Version</dt>
          <dd>{selectedVersion}</dd>
        </div>
        <div>
          <dt>Network download</dt>
          <dd>{formatBytes(downloadSize)}</dd>
        </div>
        <div>
          <dt>Installed size</dt>
          <dd>{formatBytes(installSize)}</dd>
        </div>
        <div>
          <dt>Temporary space</dt>
          <dd>{formatBytes(temporarySpace)}</dd>
        </div>
      </dl>
    </section>
  )
}

export function ChangedFiles({ files }: { files: ChangedFile[] }) {
  return (
    <section className="panel changed-panel">
      <header className="side-header">
        <Square size={17} />
        <strong>CHANGED FILES ({files.length})</strong>
      </header>
      <div className="changed-list">
        {files.map((file) => (
          <article key={file.path}>
            <Archive size={18} />
            <div>
              <strong>{file.path}</strong>
              <small>
                {formatBytes(file.oldSize)} {'->'} {formatBytes(file.newSize)}
              </small>
            </div>
            <span>{formatDelta(file.newSize - file.oldSize)}</span>
          </article>
        ))}
      </div>
      <button type="button" disabled={files.length === 0}>
        {files.length === 0 ? 'NO CHANGED FILES' : 'VIEW ALL FILES'}
      </button>
    </section>
  )
}
import { useState, useRef, useEffect } from 'react'
import { useOSTData } from '../hooks/useOSTData'

export function OSTPlayer({ bgImage, gameId }: { bgImage?: string, gameId: string }) {
  const { tracks, loading } = useOSTData(gameId)
  
  const [playMode, setPlayMode] = useState<'sequential' | 'repeat' | 'shuffle'>('sequential')
  const [activeTrackIndex, setActiveTrackIndex] = useState(0)
  const [isPlaying, setIsPlaying] = useState(false)
  const [transitioning, setTransitioning] = useState(false)
  const [progress, setProgress] = useState(0)
  const [actualDurations, setActualDurations] = useState<Record<string, string>>({})

  const audioRef = useRef<HTMLAudioElement | null>(null)

  const activeTrack = tracks.length > 0 ? tracks[activeTrackIndex] : null

  // Auto-play when active track changes
  useEffect(() => {
    if (audioRef.current && activeTrack) {
      if (isPlaying) {
        audioRef.current.play().catch(e => console.warn('Autoplay prevented', e))
      }
    }
  }, [activeTrackIndex])

  const togglePlayMode = () => {
    if (playMode === 'sequential') setPlayMode('repeat')
    else if (playMode === 'repeat') setPlayMode('shuffle')
    else setPlayMode('sequential')
  }

  const handleTrackSelect = (index: number) => {
    if (index === activeTrackIndex) {
      togglePlayPause()
      return
    }
    changeTrack(index)
  }

  const changeTrack = (newIndex: number) => {
    setTransitioning(true)
    setTimeout(() => {
      setActiveTrackIndex(newIndex)
      setTransitioning(false)
      // If we weren't playing, start playing when selecting a new track
      if (!isPlaying) {
        setIsPlaying(true)
      }
    }, 300)
  }

  const togglePlayPause = () => {
    if (audioRef.current) {
      if (isPlaying) {
        audioRef.current.pause()
      } else {
        audioRef.current.play().catch(e => console.warn('Play prevented', e))
      }
      setIsPlaying(!isPlaying)
    }
  }

  const handleNext = () => {
    if (tracks.length === 0) return
    if (playMode === 'shuffle') {
      let nextIndex = Math.floor(Math.random() * tracks.length)
      if (nextIndex === activeTrackIndex && tracks.length > 1) {
         nextIndex = (nextIndex + 1) % tracks.length
      }
      changeTrack(nextIndex)
    } else {
      const nextIndex = (activeTrackIndex + 1) % tracks.length
      // If sequential and we reached the end, stop playing
      if (playMode === 'sequential' && nextIndex === 0) {
        setIsPlaying(false)
        setActiveTrackIndex(0)
      } else {
        changeTrack(nextIndex)
      }
    }
  }

  const handlePrev = () => {
    if (tracks.length === 0) return
    const prevIndex = activeTrackIndex === 0 ? tracks.length - 1 : activeTrackIndex - 1
    changeTrack(prevIndex)
  }

  const handleTimeUpdate = () => {
    if (audioRef.current) {
      const current = audioRef.current.currentTime
      const duration = audioRef.current.duration
      if (duration > 0) {
        setProgress((current / duration) * 100)
      }
    }
  }

  const handleLoadedMetadata = () => {
    if (audioRef.current && activeTrack) {
      const duration = audioRef.current.duration
      if (duration > 0 && (!activeTrack.durationStr || activeTrack.durationStr === '0:00')) {
        const mins = Math.floor(duration / 60)
        const secs = Math.floor(duration % 60).toString().padStart(2, '0')
        setActualDurations(prev => ({ ...prev, [activeTrack.id]: `${mins}:${secs}` }))
      }
    }
  }

  const handleEnded = () => {
    if (playMode === 'repeat' && audioRef.current) {
      // Actually 'repeat' usually means repeat list. 'repeat-1' is repeat one.
      // If we treat repeat as repeat-list:
      handleNext()
    } else {
      handleNext()
    }
  }

  if (loading && tracks.length === 0) {
    return (
      <section className="ost-player-panel ost-player-expanded" style={{ marginBottom: '16px', marginTop: '16px', justifyContent: 'center' }}>
        <div style={{ color: 'rgba(255,255,255,0.5)', padding: '20px' }}>Loading Soundtrack...</div>
      </section>
    )
  }

  if (tracks.length === 0) {
    return null
  }

  return (
    <section className="ost-player-panel ost-player-expanded" style={{ marginBottom: '16px', marginTop: '16px' }}>
      <div className="ost-player-bg" style={{ backgroundImage: bgImage ? `url(${bgImage})` : undefined }} />
      
      <div className="ost-player-content">
        <div className={`ost-vinyl-container ${isPlaying && !transitioning ? 'spinning' : ''} ${transitioning ? 'fade-out' : 'fade-in'}`}>
          <div className="ost-vinyl">
            {bgImage ? <img src={bgImage} alt="" /> : <Music size={20} color="rgba(255,255,255,0.2)" />}
            <div className="ost-hole" />
          </div>
        </div>
        
        <div className={`ost-info ${transitioning ? 'fade-out' : 'fade-in'}`}>
          <strong>{activeTrack?.title || 'Unknown Track'}</strong>
          <span>{activeTrack?.artist || 'Unknown Artist'}</span>
        </div>
        
        <div className="ost-controls">
          <button type="button" onClick={togglePlayMode} title={`Play Mode: ${playMode}`}>
            {playMode === 'sequential' && <ArrowRight size={14} />}
            {playMode === 'repeat' && <Repeat size={14} />}
            {playMode === 'shuffle' && <Shuffle size={14} />}
          </button>
          <button type="button" onClick={handlePrev}><SkipBack size={16} fill="currentColor" /></button>
          <button type="button" className="play-btn" onClick={togglePlayPause}>
            {isPlaying ? (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
            ) : (
              <Play size={18} fill="currentColor" />
            )}
          </button>
          <button type="button" onClick={handleNext}><SkipForward size={16} fill="currentColor" /></button>
        </div>
      </div>
      
      <div className="ost-tracklist">
        {tracks.map((track, index) => {
          const isActive = index === activeTrackIndex
          return (
            <div 
              key={track.id} 
              className={`ost-track ${isActive ? 'active' : ''}`}
              onClick={() => handleTrackSelect(index)}
            >
              <div className="ost-track-num">
                {isActive && isPlaying ? (
                  <div className="ost-visualizer">
                    <span/><span/><span/><span/>
                  </div>
                ) : (
                  <Music size={12} className="ost-track-icon" />
                )}
              </div>
              <div className="ost-track-title">{track.title}</div>
              <div className="ost-track-duration">{actualDurations[track.id] || track.durationStr}</div>
            </div>
          )
        })}
      </div>
      
      <div className="ost-progress-bar">
        <div className="ost-progress-fill" style={{ width: `${progress}%`, animation: 'none' }} />
      </div>

      {activeTrack && (
        <audio 
          ref={audioRef}
          src={activeTrack.url}
          onTimeUpdate={handleTimeUpdate}
          onLoadedMetadata={handleLoadedMetadata}
          onEnded={handleEnded}
        />
      )}
    </section>
  )
}
