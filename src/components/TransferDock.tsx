import { useState } from 'react'
import { motion, AnimatePresence } from 'motion/react'
import { ChevronDown, ChevronUp, Download, Pause, Play } from 'lucide-react'
import type { JobJournal, PhaseProgress } from '../types'
import { formatBytes, formatDuration } from '../lib/format'
import { useSmoothNumber } from '../hooks/useSmoothNumber'

export function TransferDock({
  visible,
  gameTitle,
  gameArtwork,
  job,
  progress,
  isPaused,
  onPause,
  onOpen,
}: {
  visible: boolean
  gameTitle: string
  gameArtwork?: string
  job: JobJournal
  progress: PhaseProgress
  isPaused: boolean
  onPause: () => void
  onOpen: () => void
}) {
  const [collapsed, setCollapsed] = useState(false)
  const overallPercent = useSmoothNumber(progress.overallPercent)
  const etaCandidates = [progress.etaSeconds, progress.applyEtaSeconds].filter(
    (value): value is number => value != null,
  )
  const completionEta = etaCandidates.length > 0 ? Math.max(...etaCandidates) : null

  return (
    <AnimatePresence>
      {visible ? (
        <motion.aside
          className={`transfer-dock ${collapsed ? 'is-collapsed' : ''}`}
          initial={{ opacity: 0, y: 18, scale: 0.98 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 12, scale: 0.98 }}
          transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          aria-label="Active transfer"
        >
          {collapsed ? (
            <>
              <button type="button" className="transfer-dock-collapsed-main" onClick={onOpen} aria-label="Open transfer details">
                <span className="transfer-dock-mini-art">
                  {gameArtwork ? <img src={gameArtwork} alt="" /> : <Download size={17} />}
                </span>
                <span className="transfer-dock-mini-copy">
                  <strong>{Math.round(overallPercent)}%</strong>
                  <i style={{ width: `${overallPercent}%` }} />
                </span>
              </button>
              <button type="button" className="transfer-dock-expand" onClick={() => setCollapsed(false)} aria-label="Expand transfer dock">
                <ChevronUp size={16} />
              </button>
            </>
          ) : (
            <>
              <button type="button" className="transfer-dock-main" onClick={onOpen} aria-label="Open transfer details">
                <span className="transfer-dock-art">
                  {gameArtwork ? <img src={gameArtwork} alt="" /> : <Download size={18} />}
                </span>
                <span className="transfer-dock-copy">
                  <span className="transfer-dock-title"><strong>{gameTitle}</strong><small>{overallPercent.toFixed(1)}%</small></span>
                  <span className="transfer-dock-state">{progress.isCommitting ? 'Completing safe transaction' : isPaused ? 'Paused' : progress.name} · {formatBytes(progress.logicalBytesDone)} / {formatBytes(progress.logicalBytesTotal)}</span>
                  <span className="transfer-dock-track" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(overallPercent)}>
                    <i style={{ width: `${overallPercent}%` }} />
                  </span>
                  <span className="transfer-dock-meta">
                    <span>{progress.rateBytesPerSecond > 0 ? `${formatBytes(progress.rateBytesPerSecond)}/s network` : progress.applyRateBytesPerSecond > 0 ? `${formatBytes(progress.applyRateBytesPerSecond)}/s disk` : progress.detail}</span>
                    <span>{completionEta != null ? formatDuration(completionEta) : job.kind}</span>
                  </span>
                </span>
              </button>
              <button type="button" className="transfer-dock-control" onClick={onPause} aria-label={isPaused ? 'Resume transfer' : 'Pause transfer'} disabled={progress.isCommitting}>
                {isPaused ? <Play size={16} fill="currentColor" /> : <Pause size={16} fill="currentColor" />}
              </button>
              <button type="button" className="transfer-dock-collapse" onClick={() => setCollapsed(true)} aria-label="Collapse transfer dock">
                <ChevronDown size={16} />
              </button>
            </>
          )}
        </motion.aside>
      ) : null}
    </AnimatePresence>
  )
}
