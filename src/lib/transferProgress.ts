export type LogicalTransferInput = {
  bytesDone: number
  bytesTotal: number
  logicalBytesDone?: number
  logicalBytesTotal?: number
  sessionBaseBytes?: number
}

export type LogicalTransferProgress = {
  logicalBytesDone: number
  logicalBytesTotal: number
  sessionBytesDone: number
  sessionBytesTotal: number
  sessionBaseBytes: number
  remainingBytes: number
  logicalPercent: number
}

function finiteNonNegative(value: number | undefined): number {
  return Number.isFinite(value) && (value ?? 0) > 0 ? Number(value) : 0
}

export function deriveLogicalTransferProgress(input: LogicalTransferInput): LogicalTransferProgress {
  const sessionBytesTotal = finiteNonNegative(input.bytesTotal)
  const sessionBytesDone = Math.min(finiteNonNegative(input.bytesDone), sessionBytesTotal || Number.MAX_SAFE_INTEGER)
  const sessionBaseBytes = finiteNonNegative(input.sessionBaseBytes)
  const explicitLogicalTotal = finiteNonNegative(input.logicalBytesTotal)
  const explicitLogicalDone = finiteNonNegative(input.logicalBytesDone)

  const derivedTotal = sessionBaseBytes + sessionBytesTotal
  const logicalBytesTotal = Math.max(explicitLogicalTotal, derivedTotal, sessionBytesTotal)
  const legacyDone = sessionBaseBytes > 0 || explicitLogicalTotal > 0
    ? sessionBaseBytes + sessionBytesDone
    : sessionBytesDone
  const logicalBytesDone = Math.min(
    Math.max(explicitLogicalDone, legacyDone),
    logicalBytesTotal || Number.MAX_SAFE_INTEGER,
  )
  const remainingBytes = Math.max(logicalBytesTotal - logicalBytesDone, 0)
  const logicalPercent = logicalBytesTotal > 0
    ? Math.min(Math.max((logicalBytesDone / logicalBytesTotal) * 100, 0), 100)
    : 0

  return {
    logicalBytesDone,
    logicalBytesTotal,
    sessionBytesDone,
    sessionBytesTotal,
    sessionBaseBytes,
    remainingBytes,
    logicalPercent,
  }
}
