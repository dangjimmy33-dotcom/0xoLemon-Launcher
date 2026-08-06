import { useMemo } from 'react'
import {
  Archive,
  CheckCircle2,
  ChevronDown,
  CircleAlert,
  Clock3,
  Download,
  Gauge,
  HardDrive,
  Loader2,
  Pause,
  Play,
  ShieldCheck,
  TerminalSquare,
  Wrench,
  X,
} from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
import type { JobJournal, JobLog, JobStep, PhaseProgress } from '../types'
import { formatBytes, formatDuration } from '../lib/format'
import { useSmoothNumber } from '../hooks/useSmoothNumber'

function lastJobError(job: JobJournal) {
  const last = [...job.logs]
    .reverse()
    .find((log) => log.level.toLowerCase().includes('error') || log.level.toLowerCase().includes('warn'))
  return last?.message || 'Network error. Open job log for exact cause.'
}

function jobLabel(job: JobJournal) {
  if (job.kind === 'install') return `Install ${job.toVersion}`
  if (job.kind === 'patch') return `Patch fix ${job.toVersion}`
  if (job.kind === 'repair') return `Repair ${job.toVersion}`
  return `Update ${job.fromVersion} → ${job.toVersion}`
}

function statusLabel(job: JobJournal, phaseProgress: PhaseProgress) {
  if (job.status === 'failed') return 'Transfer failed'
  if (job.status === 'canceled') return 'Transfer canceled'
  if (job.status === 'paused') return 'Paused — progress is safely saved'
  if (phaseProgress.isDownloading) return 'Downloading missing byte ranges'
  return phaseProgress.detail || phaseProgress.name
}

function Artwork({ src, icon }: { src?: string; icon?: 'download' | 'patch' }) {
  return (
    <div className="transfer-artwork" aria-hidden="true">
      {src ? <img src={src} alt="" loading="eager" decoding="async" /> : icon === 'patch' ? <Wrench size={22} /> : <Download size={22} />}
      <span />
    </div>
  )
}

export function DownloadQueuePanel({
  gameTitle,
  gameArtwork,
  job,
  hasJob,
  phaseProgress,
  selectedVersion,
  downloadSize,
  isInstalled,
  isRunning,
  isPaused,
  onOpenOptions,
  onPause,
  onCancel,
  onResume,
  isResuming = false,
}: {
  gameTitle: string
  gameArtwork?: string
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  selectedVersion: string
  downloadSize: number
  isInstalled: boolean
  isRunning: boolean
  isPaused: boolean
  onOpenOptions: () => void
  onPause: () => void
  onCancel: () => void
  onResume?: () => void
  isResuming?: boolean
}) {
  const logicalPercent = useSmoothNumber(
    phaseProgress.logicalBytesTotal > 0
      ? (phaseProgress.logicalBytesDone / phaseProgress.logicalBytesTotal) * 100
      : phaseProgress.percent,
  )
  const failed = job.status === 'failed'
  const canceled = job.status === 'canceled'

  if (!hasJob) {
    return (
      <section className="panel transfer-overview-panel">
        <header className="transfer-section-heading">
          <div>
            <span>Downloads</span>
            <strong>No active transfers</strong>
          </div>
          <small>Queue is clear</small>
        </header>
        <div className="transfer-empty-state">
          <div className="transfer-empty-icon">{isInstalled ? <CheckCircle2 size={22} /> : <Download size={22} />}</div>
          <div>
            <strong>{isInstalled ? 'Game is installed' : 'Ready when you are'}</strong>
            <span>
              {isInstalled
                ? `${gameTitle} has no pending download or update task.`
                : `${gameTitle} ${selectedVersion} requires approximately ${formatBytes(downloadSize)}.`}
            </span>
          </div>
          {!isInstalled ? <button type="button" onClick={onOpenOptions}>{t.library.chooseInstall}</button> : null}
        </div>
      </section>
    )
  }

  return (
    <section className="panel transfer-overview-panel">
      <header className="transfer-section-heading">
        <div>
          <span>Downloads</span>
          <strong>{failed ? 'Needs attention' : canceled ? 'Canceled' : '1 active transfer'}</strong>
        </div>
        <small>{statusLabel(job, phaseProgress)}</small>
      </header>

      <article className={`transfer-card ${failed ? 'is-failed' : ''} ${isPaused ? 'is-paused' : ''}`}>
        <Artwork src={gameArtwork} icon={job.kind === 'patch' ? 'patch' : 'download'} />
        <div className="transfer-card-main">
          <div className="transfer-card-title-row">
            <div>
              <strong>{gameTitle}</strong>
              <span>{jobLabel(job)}</span>
            </div>
            <div className="transfer-percent-block">
              <strong>{logicalPercent.toFixed(1)}%</strong>
              <span>total data ready</span>
            </div>
          </div>

          <p className="transfer-status-copy">{failed ? lastJobError(job) : statusLabel(job, phaseProgress)}</p>

          <div
            className="transfer-primary-track"
            role="progressbar"
            aria-label={`${gameTitle} total transfer progress`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(logicalPercent)}
          >
            <i style={{ width: `${logicalPercent}%` }} />
          </div>

          <div className="transfer-byte-line">
            <strong>{formatBytes(phaseProgress.logicalBytesDone)}</strong>
            <span>/ {formatBytes(phaseProgress.logicalBytesTotal)}</span>
            <small>{phaseProgress.isDownloading && phaseProgress.rateBytesPerSecond > 0 ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : phaseProgress.name}</small>
          </div>

          <div className="transfer-metric-grid">
            <div>
              <HardDrive size={15} />
              <span>Already available</span>
              <strong>{formatBytes(phaseProgress.sessionBaseBytes)}</strong>
            </div>
            <div>
              <Download size={15} />
              <span>This session</span>
              <strong>{formatBytes(phaseProgress.sessionBytesDone)} / {formatBytes(phaseProgress.sessionBytesTotal)}</strong>
            </div>
            <div>
              <Clock3 size={15} />
              <span>Remaining</span>
              <strong>{formatBytes(phaseProgress.remainingBytes)}</strong>
            </div>
          </div>
        </div>

        <div className="transfer-card-actions">
          {failed || canceled ? (
            <button
              className="transfer-action-primary"
              type="button"
              onClick={!isResuming ? (failed ? (onResume ?? onOpenOptions) : onOpenOptions) : undefined}
              disabled={isResuming}
            >
              {isResuming ? <Loader2 size={16} className="is-spinning" /> : <Play size={16} />}
              {isResuming ? 'Resuming…' : failed ? 'Resume' : 'Start again'}
            </button>
          ) : isRunning ? (
            <>
              <button className="transfer-action-primary" type="button" onClick={onPause}>
                {isPaused ? <Play size={16} fill="currentColor" /> : <Pause size={16} fill="currentColor" />}
                {isPaused ? 'Resume' : 'Pause'}
              </button>
              <button className="transfer-action-danger" type="button" onClick={onCancel} aria-label="Cancel download">
                <X size={17} />
              </button>
            </>
          ) : (
            <span className="transfer-state-pill">{job.status}</span>
          )}
        </div>
      </article>
    </section>
  )
}

export function JobCenter({
  gameTitle,
  gameArtwork,
  job,
  hasJob,
  phaseProgress,
  onPause,
  onCancel,
  isPaused,
  showControls = true,
}: {
  gameTitle?: string
  gameArtwork?: string
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  onPause: () => void
  onCancel: () => void
  isPaused: boolean
  showControls?: boolean
}) {
  const displayOverall = useSmoothNumber(phaseProgress.overallPercent)
  const phasePercent = useSmoothNumber(phaseProgress.percent)
  const canControl = hasJob && ['running', 'downloading', 'assembling', 'paused'].includes(job.status)
  const title = gameTitle || 'Selected game'

  return (
    <section className="panel transfer-job-panel">
      <header className="transfer-job-header">
        <Artwork src={gameArtwork} icon={job.kind === 'patch' ? 'patch' : 'download'} />
        <div className="transfer-job-heading">
          <span>{hasJob ? jobLabel(job) : t.jobs.noActiveJob}</span>
          <strong>{title}</strong>
          <small>{hasJob ? statusLabel(job, phaseProgress) : t.jobs.chooseVersion}</small>
        </div>
        <div className="transfer-overall-value">
          <span>Overall</span>
          <strong>{displayOverall.toFixed(1)}%</strong>
        </div>
      </header>

      <div
        className="transfer-primary-track is-overall"
        role="progressbar"
        aria-label="Overall job progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(displayOverall)}
      >
        <i style={{ width: `${displayOverall}%` }} />
      </div>

      <div className="transfer-job-metrics">
        <div><Gauge size={16} /><span>Current phase</span><strong>{phaseProgress.name}</strong></div>
        <div><Download size={16} /><span>Data ready</span><strong>{formatBytes(phaseProgress.logicalBytesDone)} / {formatBytes(phaseProgress.logicalBytesTotal)}</strong></div>
        <div><Gauge size={16} /><span>Speed</span><strong>{phaseProgress.isDownloading && phaseProgress.rateBytesPerSecond > 0 ? `${formatBytes(phaseProgress.rateBytesPerSecond)}/s` : '—'}</strong></div>
        <div><Clock3 size={16} /><span>ETA</span><strong>{formatDuration(phaseProgress.etaSeconds)}</strong></div>
      </div>

      <div className="transfer-phase-summary">
        <div>
          <span>Phase progress</span>
          <strong>{phasePercent.toFixed(1)}%</strong>
        </div>
        <div className="transfer-secondary-track" role="progressbar" aria-label="Current phase progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(phasePercent)}>
          <i style={{ width: `${phasePercent}%` }} />
        </div>
      </div>

      <div className="transfer-timeline" aria-label="Job phases">
        {job.steps.map((step, index) => <StepRow key={`${step.name}-${index}`} index={index + 1} step={step} />)}
      </div>

      {showControls ? (
        <footer className="transfer-job-actions">
          {canControl ? (
            <>
              <button className="transfer-action-primary" type="button" onClick={onPause}>
                {isPaused ? <Play size={17} /> : <Pause size={17} />}
                {isPaused ? t.jobs.resume : t.jobs.pause}
              </button>
              <button className="transfer-action-secondary" type="button" onClick={onCancel}>
                <X size={17} />
                {t.jobs.cancel}
              </button>
              <span className="transfer-resume-note">Progress is saved and can resume after launcher restart.</span>
            </>
          ) : (
            <span className="transfer-resume-note">No running download, assemble, or repair job.</span>
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
    if (step.name.toLowerCase().includes('download') || step.name.toLowerCase().includes('stream')) return Download
    if (step.name.toLowerCase().includes('verify')) return ShieldCheck
    if (step.name.toLowerCase().includes('assemble') || step.name.toLowerCase().includes('commit')) return Archive
    return TerminalSquare
  }, [step.name, step.status])

  return (
    <article className={`transfer-step ${step.status}`}>
      <div className="transfer-step-rail"><span><Icon size={16} /></span><i /></div>
      <span className="transfer-step-index">{index}</span>
      <div className="transfer-step-copy"><strong>{step.name}</strong><small>{step.detail}</small></div>
      <div className="transfer-step-progress"><i style={{ width: `${displayProgress}%` }} /></div>
      <strong className="transfer-step-percent">{Math.round(displayProgress)}%</strong>
      <span className="transfer-step-retry">{step.retryCount > 0 ? `${step.retryCount} retry` : '—'}</span>
    </article>
  )
}

export function JobLogPanel({ logs }: { logs: JobLog[] }) {
  return (
    <details className="job-log-disclosure">
      <summary>
        <div><TerminalSquare size={16} /><strong>Technical job log</strong><span>{logs.length} entries</span></div>
        <ChevronDown size={17} />
      </summary>
      <div className="log-list">
        {logs.slice(-12).map((log, index) => (
          <div className={`log-row ${log.level}`} key={`${log.at}-${index}`}>
            <span>[{log.at}]</span>
            {log.level.toLowerCase().includes('error') ? <CircleAlert size={15} /> : <CheckCircle2 size={15} />}
            <p>{log.message}</p>
          </div>
        ))}
      </div>
    </details>
  )
}
