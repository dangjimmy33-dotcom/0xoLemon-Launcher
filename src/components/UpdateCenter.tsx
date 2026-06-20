import { AnimatePresence, motion } from 'motion/react'
import { Check, ChevronRight, Download, RefreshCcw, ShieldCheck, X } from 'lucide-react'
import { formatBytes, formatDuration } from '../lib/format'
import { MOTION } from '../lib/motion'
import type { LauncherUpdateInfo, LauncherUpdateProgress } from '../types'

const phases = ['downloading', 'verifying', 'installing', 'restarting'] as const

export function UpdateBanner({
  update,
  progress,
  onOpen,
  onStart,
}: {
  update: LauncherUpdateInfo
  progress: LauncherUpdateProgress | null
  onOpen: () => void
  onStart: () => void
}) {
  const active = progress && progress.phase !== 'checking' && progress.phase !== 'failed'
  const percent = downloadPercent(progress)
  return (
    <div className="premium-update-banner">
      <div className="premium-update-icon"><RefreshCcw size={16} /></div>
      <div className="premium-update-summary">
        <strong>Launcher {update.version} is available</strong>
        <span>{active ? phaseLabel(progress.phase) : 'A signed update is ready to download.'}</span>
      </div>
      {active && progress?.phase === 'downloading' ? (
        <div className="premium-update-inline-progress" aria-label={`${percent}% downloaded`}>
          <i style={{ width: `${percent}%` }} />
        </div>
      ) : null}
      <button type="button" className="premium-update-details" onClick={onOpen}>
        Details <ChevronRight size={15} />
      </button>
      {!active ? <button type="button" className="premium-update-now" onClick={onStart}>Update now</button> : null}
    </div>
  )
}

export function UpdateCenter({
  open,
  update,
  progress,
  speed,
  eta,
  onClose,
  onStart,
  onRetry,
}: {
  open: boolean
  update: LauncherUpdateInfo | null
  progress: LauncherUpdateProgress | null
  speed: number
  eta: number | null
  onClose: () => void
  onStart: () => void
  onRetry: () => void
}) {
  const phase = progress?.phase ?? 'available'
  const active = ['downloading', 'verifying', 'installing', 'restarting'].includes(phase)
  const percent = downloadPercent(progress)

  return (
    <AnimatePresence>
      {open && update ? (
        <>
          <motion.button
            type="button"
            className="update-center-scrim"
            aria-label="Close Update Center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
          />
          <motion.aside
            className="update-center-drawer"
            role="dialog"
            aria-modal="true"
            aria-labelledby="update-center-title"
            initial={{ opacity: 0, x: 52 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: 52 }}
            transition={MOTION.panel}
          >
            <header>
              <div className="update-center-mark"><RefreshCcw size={21} /></div>
              <div>
                <span>0xoLemon Launcher</span>
                <h2 id="update-center-title">Update {update.version}</h2>
              </div>
              <button type="button" className="update-center-close" onClick={onClose} aria-label="Close">
                <X size={18} />
              </button>
            </header>

            <section className="update-center-progress-card">
              <div className="update-center-phase">
                <span>{phaseLabel(phase)}</span>
                {phase === 'downloading' ? <strong>{percent}%</strong> : null}
              </div>
              <div
                className={`update-center-progress ${phase === 'downloading' ? '' : active ? 'is-indeterminate' : ''}`}
                role="progressbar"
                aria-valuenow={phase === 'downloading' ? percent : undefined}
              >
                <i style={phase === 'downloading' ? { width: `${percent}%` } : undefined} />
              </div>
              <div className="update-center-metrics">
                <div>
                  <span>Downloaded</span>
                  <strong>
                    {formatBytes(progress?.downloadedBytes ?? 0)}
                    {progress?.totalBytes ? ` / ${formatBytes(progress.totalBytes)}` : ''}
                  </strong>
                </div>
                <div>
                  <span>Speed</span>
                  <strong>{phase === 'downloading' && speed > 0 ? `${formatBytes(speed)}/s` : '--'}</strong>
                </div>
                <div>
                  <span>Time left</span>
                  <strong>{phase === 'downloading' ? formatDuration(eta) : '--'}</strong>
                </div>
              </div>
              {progress?.error ? <p className="update-center-error">{progress.error}</p> : null}
            </section>

            <ol className="update-center-steps">
              {phases.map((item) => {
                const currentIndex = phases.indexOf(phase as (typeof phases)[number])
                const itemIndex = phases.indexOf(item)
                const complete = currentIndex > itemIndex || phase === 'restarting'
                const current = phase === item
                return (
                  <li key={item} className={current ? 'is-current' : complete ? 'is-complete' : ''}>
                    <span>{complete ? <Check size={14} /> : item === 'verifying' ? <ShieldCheck size={14} /> : <Download size={14} />}</span>
                    <div>
                      <strong>{phaseLabel(item)}</strong>
                      <small>{phaseDescription(item)}</small>
                    </div>
                  </li>
                )
              })}
            </ol>

            <section className="update-center-notes">
              <h3>What’s new</h3>
              <div>{update.notes?.trim() || 'Maintenance, stability and launcher experience improvements.'}</div>
              {update.publishedAt ? <small>Published {new Date(update.publishedAt).toLocaleString()}</small> : null}
            </section>

            <footer>
              {phase === 'failed' ? (
                <button type="button" className="update-center-primary" onClick={onRetry}>Retry update</button>
              ) : active ? (
                <button type="button" className="update-center-primary" onClick={onClose}>Hide</button>
              ) : (
                <>
                  <button type="button" className="update-center-secondary" onClick={onClose}>Update later</button>
                  <button type="button" className="update-center-primary" onClick={onStart}>Download and install</button>
                </>
              )}
            </footer>
          </motion.aside>
        </>
      ) : null}
    </AnimatePresence>
  )
}

function downloadPercent(progress: LauncherUpdateProgress | null) {
  if (!progress?.totalBytes || progress.totalBytes <= 0) return 0
  return Math.max(0, Math.min(100, Math.round((progress.downloadedBytes / progress.totalBytes) * 100)))
}

function phaseLabel(phase: string) {
  switch (phase) {
    case 'checking': return 'Checking for updates'
    case 'downloading': return 'Downloading'
    case 'verifying': return 'Verifying signature'
    case 'installing': return 'Installing'
    case 'restarting': return 'Restarting launcher'
    case 'failed': return 'Update failed'
    default: return 'Ready to update'
  }
}

function phaseDescription(phase: (typeof phases)[number]) {
  switch (phase) {
    case 'downloading': return 'Downloading the signed installer'
    case 'verifying': return 'Validating the updater signature'
    case 'installing': return 'Applying the verified package'
    case 'restarting': return 'Starting the new launcher version'
  }
}

