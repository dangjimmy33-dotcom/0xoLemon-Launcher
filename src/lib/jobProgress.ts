import type { JobJournal, PhaseProgress, Snapshot } from '../types'
import { DEFAULT_GAME_ID } from './installPaths'
import { deriveLogicalTransferProgress } from './transferProgress'

export function createIdleJob(snapshot: Snapshot): JobJournal {
  return {
    id: 'idle',
    gameId: DEFAULT_GAME_ID,
    kind: 'update',
    status: 'planned',
    installPath: '',
    fromVersion: snapshot.currentVersion,
    toVersion: snapshot.latestVersion,
    phase: 'Ready',
    overallProgress: 0,
    bytesDone: 0,
    bytesTotal: snapshot.updateSize,
    logicalBytesDone: 0,
    logicalBytesTotal: snapshot.updateSize,
    sessionBaseBytes: 0,
    applyBytesDone: 0,
    applyBytesTotal: 0,
    durableBytes: 0,
    currentFile: '',
    pipelineVersion: '',
    commitState: 'idle',
    plannedFiles: [],
    retryCount: 0,
    resumable: true,
    updatedAt: new Date().toISOString(),
    steps: [
      { name: 'Scan', detail: 'Find local files and detect version', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Verify', detail: 'Hash manifest-owned files', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Download packs', detail: 'Resume missing byte ranges from proxy', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Assemble files', detail: 'Rebuild files into verified temp outputs', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Finalize', detail: 'Replace only after full-file hash match', status: 'waiting', progress: 0, retryCount: 0 },
      { name: 'Patch fix', detail: 'Checking for version-specific file patches', status: 'waiting', progress: 0, retryCount: 0 },
    ],
    logs: [
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'No launcher job is running.' },
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'Select a target version or scan an existing install.' },
      { at: new Date().toLocaleTimeString(), level: 'info', message: 'Install uses 0xoLemon store; update uses the selected game folder.' },
    ],
  }
}

export function getPhaseProgress(
  job: JobJournal,
  rateBytesPerSecond: number,
  applyRateBytesPerSecond = 0,
): PhaseProgress {
  const runningStep =
    job.steps.find((step) => step.status === 'running' || step.status === 'paused') ??
    job.steps.find((step) => step.status !== 'completed') ??
    job.steps[job.steps.length - 1]
  const isDownloading =
    job.status === 'downloading' ||
    (job.kind === 'patch' &&
      job.status === 'running' &&
      job.bytesTotal > 0 &&
      Boolean(runningStep?.name.toLowerCase().includes('download')))
  const transfer = deriveLogicalTransferProgress(job)
  const applyBytesDone = Math.min(job.applyBytesDone ?? 0, job.applyBytesTotal ?? 0)
  const applyBytesTotal = job.applyBytesTotal ?? 0
  const networkPercent = transfer.logicalPercent
  const applyPercent = bytePercent(applyBytesDone, applyBytesTotal)
  const streamingTotal = transfer.logicalBytesTotal + applyBytesTotal
  const streamingDone = transfer.logicalBytesDone + applyBytesDone
  const phasePercent = isDownloading && streamingTotal > 0
    ? bytePercent(streamingDone, streamingTotal)
    : clampPercent((runningStep?.progress ?? job.overallProgress) * 100)
  const commitState = job.commitState ?? 'idle'
  const isCommitting = ['preparing', 'committing', 'filesInstalled', 'metadata', 'cleanup'].includes(commitState)
  const committed = job.status === 'committed'
  const capUntilCommitted = (value: number) =>
    committed ? clampPercent(value) : Math.min(clampPercent(value), 99.9)

  return {
    name: runningStep?.name ?? job.phase,
    detail: job.phase,
    percent: committed ? 100 : capUntilCommitted(phasePercent),
    overallPercent: committed ? 100 : capUntilCommitted(job.overallProgress * 100),
    bytesDone: transfer.logicalBytesDone,
    bytesTotal: transfer.logicalBytesTotal,
    logicalBytesDone: transfer.logicalBytesDone,
    logicalBytesTotal: transfer.logicalBytesTotal,
    sessionBytesDone: transfer.sessionBytesDone,
    sessionBytesTotal: transfer.sessionBytesTotal,
    sessionBaseBytes: transfer.sessionBaseBytes,
    remainingBytes: transfer.remainingBytes,
    rateBytesPerSecond,
    applyRateBytesPerSecond,
    etaSeconds: isDownloading && rateBytesPerSecond > 1
      ? Math.max(transfer.sessionBytesTotal - transfer.sessionBytesDone, 0) / rateBytesPerSecond
      : null,
    applyEtaSeconds: applyRateBytesPerSecond > 1
      ? Math.max(applyBytesTotal - applyBytesDone, 0) / applyRateBytesPerSecond
      : null,
    networkPercent,
    applyPercent,
    applyBytesDone,
    applyBytesTotal,
    durableBytes: job.durableBytes ?? applyBytesDone,
    currentFile: job.currentFile ?? '',
    pipelineVersion: job.pipelineVersion ?? '',
    commitState,
    isCommitting,
    isDownloading,
  }
}

export function bytePercent(done: number, total: number) {
  if (total <= 0) return 0
  return clampPercent((done / total) * 100)
}

export function clampPercent(value: number) {
  return Math.min(Math.max(value, 0), 100)
}
