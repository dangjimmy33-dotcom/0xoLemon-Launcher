import { Archive, Download, HardDrive, RotateCcw, ShieldCheck, Square } from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
import type { ChangedFile, GameDetail, Snapshot } from '../types'
import { formatBytes, formatDelta } from '../lib/format'

export function CachePanel({ snapshot }: { snapshot: Snapshot }) {
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
      <button type="button" disabled>
        {snapshot.cache.cacheSize === 0 ? 'NO STORED CHUNKS' : 'REUSED AUTOMATICALLY'}
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
  return (
    <section className="panel game-info-panel">
      <header className="side-header">
        <ShieldCheck size={17} />
        <strong>{t.library.details}</strong>
      </header>
      <dl className="game-info-list">
        <div>
          <dt>Developer</dt>
          <dd>{detail.developers.join(', ')}</dd>
        </div>
        <div>
          <dt>Publisher</dt>
          <dd>{detail.publishers.join(', ')}</dd>
        </div>
        <div>
          <dt>Release date</dt>
          <dd>{detail.releaseDate}</dd>
        </div>
        <div>
          <dt>Genres</dt>
          <dd>
            {detail.genres.map((genre) => (
              <span key={genre}>{genre}</span>
            ))}
          </dd>
        </div>
      </dl>
      {detail.ratings.map((rating) => (
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
