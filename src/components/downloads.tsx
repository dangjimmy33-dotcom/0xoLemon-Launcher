import { useMemo } from 'react'
import { Archive, CheckCircle2, CircleAlert, Download, Pause, Play, ShieldCheck, TerminalSquare, X } from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
import type { JobJournal, JobLog, JobStep, PhaseProgress } from '../types'
import { formatBytes, formatDuration } from '../lib/format'
import { useSmoothNumber } from '../hooks/useSmoothNumber'

function lastJobError(job: JobJournal) {
  const last = [...job.logs].reverse().find((log) => log.level.toLowerCase().includes('error') || log.level.toLowerCase().includes('warn'))
  return last?.message || 'Network error. Open job log for exact cause.'
}

export function DownloadQueuePanel({
  gameTitle,
  job,
  hasJob,
  progress,
  phaseProgress,
  selectedVersion,
  downloadSize,
  isRunning,
  isPaused,
  onOpenOptions,
  onPause,
  onCancel,
  onResume,
}: {
  gameTitle: string
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  selectedVersion: string
  downloadSize: number
  isRunning: boolean
  isPaused: boolean
  onOpenOptions: () => void
  onPause: () => void
  onCancel: () => void
  onResume?: () => void
}) {
  const displayProgress = useSmoothNumber(progress)

  if (!hasJob) {
    return (
      <section className="panel download-queue-panel">
        <header className="panel-header compact">
          <strong>DOWNLOADS</strong>
          <span>No queued downloads</span>
        </header>
        <div className="downloads-empty">
          <div className="queue-art">
            <Download size={19} />
          </div>
          <div>
            <strong>No active download</strong>
            <span>
              {gameTitle} {selectedVersion} is available, {formatBytes(downloadSize)} required.
            </span>
          </div>
          <button type="button" onClick={onOpenOptions}>
            {t.library.chooseInstall}
          </button>
        </div>
      </section>
    )
  }

  const queuedLabel = `${job.kind === 'install' ? 'Install' : 'Update'} ${job.toVersion}`
  const failed = job.status === 'failed'
  const canceled = job.status === 'canceled'

  return (
    <section className="panel download-queue-panel">
      <header className="panel-header compact">
        <strong>DOWNLOADS</strong>
        <span>{failed ? 'Download failed' : canceled ? 'Download canceled' : 'Active queue'}</span>
      </header>
      <article className={failed ? 'queue-row failed' : 'queue-row active'}>
        <div className="queue-art">
          {failed ? <CircleAlert size={19} /> : <Download size={19} />}
        </div>
        <div className="queue-copy">
          <strong>{gameTitle}</strong>
          <span>{queuedLabel}</span>
          <small>
            {failed ? lastJobError(job) : `${phaseProgress.name} - ${phaseProgress.detail}`}
          </small>
        </div>
        <div className="queue-progress">
          <div className="mini-track">
            <span style={{ width: `${displayProgress}%` }} />
          </div>
          <div className="queue-transfer">
            <span>{displayProgress.toFixed(1)}%</span>
            <span>
              {formatBytes(phaseProgress.bytesDone)} / {formatBytes(phaseProgress.bytesTotal)}
            </span>
            <span>{phaseProgress.isDownloading ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : 'Phase progress'}</span>
          </div>
        </div>
        {failed || canceled ? (
          <button type="button" onClick={failed ? (onResume ?? onOpenOptions) : onOpenOptions}>
            {failed ? 'Resume' : t.library.chooseInstall}
          </button>
        ) : isRunning ? (
          <div className="queue-actions">
            <button
              className="queue-pause-control"
              type="button"
              onClick={onPause}
              aria-label={isPaused ? 'Resume download' : 'Pause download'}
            >
              {isPaused ? <Play size={16} fill="currentColor" /> : <Pause size={16} fill="currentColor" />}
              <span>{isPaused ? 'RESUME' : 'PAUSE'}</span>
            </button>
            <button
              className="queue-cancel-control"
              type="button"
              onClick={onCancel}
              aria-label="Cancel download"
              title="Cancel download"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <circle cx="12" cy="12" r="8.25" />
                <rect x="9" y="9" width="6" height="6" rx="1.25" />
              </svg>
            </button>
          </div>
        ) : (
          <span className="queue-pill">{job.status}</span>
        )}
      </article>
    </section>
  )
}

export function JobCenter({
  job,
  hasJob,
  progress,
  phaseProgress,
  onPause,
  onCancel,
  isPaused,
  showControls = true,
}: {
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  onPause: () => void
  onCancel: () => void
  isPaused: boolean
  showControls?: boolean
}) {
  const displayProgress = useSmoothNumber(progress)
  const displayOverall = useSmoothNumber(phaseProgress.overallPercent)
  const canControl = hasJob && ['running', 'downloading', 'assembling', 'paused'].includes(job.status)
  const jobTitle = job.kind === 'install' ? `INSTALL JOB: ${job.toVersion}` : `UPDATE JOB: ${job.fromVersion} -> ${job.toVersion}`

  return (
    <section className="panel job-panel">
      <header className="panel-header">
        <div>
          <strong>{hasJob ? jobTitle : t.jobs.noActiveJob}</strong>
          <span>{hasJob ? `${phaseProgress.name} - ${phaseProgress.detail}` : t.jobs.chooseVersion}</span>
        </div>
        <div className="progress-summary">
          <span>Current phase</span>
          <strong>{displayProgress.toFixed(1)}%</strong>
          <span>Overall {displayOverall.toFixed(1)}%</span>
        </div>
      </header>
      <div className="track">
        <span style={{ width: `${displayProgress}%` }} />
      </div>
      <div className="phase-transfer-row">
        <span>
          Downloaded <strong>{formatBytes(phaseProgress.bytesDone)}</strong> / {formatBytes(phaseProgress.bytesTotal)}
        </span>
        <span>
          Speed <strong>{phaseProgress.isDownloading ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : '--'}</strong>
        </span>
        <span>
          ETA <strong>{formatDuration(phaseProgress.etaSeconds)}</strong>
        </span>
      </div>
      <div className="steps">
        {job.steps.map((step, index) => (
          <StepRow key={step.name} index={index + 1} step={step} />
        ))}
      </div>
      {showControls ? (
        <footer className="job-actions">
          {canControl ? (
            <>
              <button className="primary-control" type="button" onClick={onPause}>
                {isPaused ? <Play size={17} /> : <Pause size={17} />}
                {isPaused ? t.jobs.resume : t.jobs.pause}
              </button>
              <button type="button" onClick={onCancel}>
                <X size={17} />
                {t.jobs.cancel}
              </button>
              <span className="resume-state">{t.jobs.resumable}</span>
            </>
          ) : (
            <span className="resume-state idle">No running download, assemble, or repair job.</span>
          )}
        </footer>
      ) : null}
    </section>
  )
}

export function StepRow({ index, step }: { index: number; step: JobStep }) {
  const displayProgress = useSmoothNumber(step.progress * 100)
  const Icon = useMemo(() => {
    if (step.status === 'completed') return CheckCircle2
    if (step.status === 'failed') return CircleAlert
    if (step.name.includes('Download')) return Download
    if (step.name.includes('Verify')) return ShieldCheck
    if (step.name.includes('Assemble')) return Archive
    return TerminalSquare
  }, [step.name, step.status])

  return (
    <article className={`step-row ${step.status}`}>
      <div className="step-icon">
        <Icon size={21} />
      </div>
      <span className="step-index">{index}</span>
      <div className="step-copy">
        <strong>{step.name}</strong>
        <small>{step.detail}</small>
      </div>
      <div className="mini-track">
        <span style={{ width: `${displayProgress}%` }} />
      </div>
      <strong className="step-percent">{Math.round(displayProgress)}%</strong>
      <span className="retry-count">{step.retryCount} retry</span>
    </article>
  )
}

export function JobLogPanel({ logs }: { logs: JobLog[] }) {
  return (
    <section className="panel log-panel">
      <header className="panel-header compact">
        <strong>JOB LOG</strong>
        <button type="button">CLEAR</button>
      </header>
      <div className="log-list">
        {logs.slice(-7).map((log, index) => (
          <div className={`log-row ${log.level}`} key={`${log.at}-${index}`}>
            <span>[{log.at}]</span>
            <CheckCircle2 size={15} />
            <p>{log.message}</p>
          </div>
        ))}
      </div>
    </section>
  )
}
