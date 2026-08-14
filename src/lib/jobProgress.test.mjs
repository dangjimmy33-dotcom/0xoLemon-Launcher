import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { deriveLogicalTransferProgress } from './transferProgress.ts'

const resumed = deriveLogicalTransferProgress({
  bytesDone: 0,
  bytesTotal: 5 * 1024 ** 3,
  logicalBytesDone: 3 * 1024 ** 3,
  logicalBytesTotal: 8 * 1024 ** 3,
  sessionBaseBytes: 3 * 1024 ** 3,
})

assert.equal(resumed.logicalBytesDone, 3 * 1024 ** 3)
assert.equal(resumed.logicalBytesTotal, 8 * 1024 ** 3)
assert.equal(resumed.sessionBytesDone, 0)
assert.equal(resumed.sessionBytesTotal, 5 * 1024 ** 3)
assert.equal(resumed.remainingBytes, 5 * 1024 ** 3)
assert.equal(resumed.logicalPercent, 37.5)

const legacy = deriveLogicalTransferProgress({
  bytesDone: 3 * 1024 ** 3,
  bytesTotal: 8 * 1024 ** 3,
})
assert.equal(legacy.logicalBytesDone, 3 * 1024 ** 3)
assert.equal(legacy.logicalBytesTotal, 8 * 1024 ** 3)
assert.equal(legacy.logicalPercent, 37.5)

const complete = deriveLogicalTransferProgress({
  bytesDone: 5 * 1024 ** 3,
  bytesTotal: 5 * 1024 ** 3,
  logicalBytesDone: 8 * 1024 ** 3,
  logicalBytesTotal: 8 * 1024 ** 3,
  sessionBaseBytes: 3 * 1024 ** 3,
})
assert.equal(complete.logicalPercent, 100)
assert.equal(complete.remainingBytes, 0)

const resumedLargeInstall = deriveLogicalTransferProgress({
  bytesDone: 0,
  bytesTotal: 33 * 1024 ** 3,
  logicalBytesDone: 34 * 1024 ** 3,
  logicalBytesTotal: 67 * 1024 ** 3,
  sessionBaseBytes: 34 * 1024 ** 3,
})
assert.equal(resumedLargeInstall.logicalBytesDone, 34 * 1024 ** 3)
assert.equal(resumedLargeInstall.logicalBytesTotal, 67 * 1024 ** 3)
assert.equal(resumedLargeInstall.sessionBytesDone, 0)
assert.equal(resumedLargeInstall.sessionBytesTotal, 33 * 1024 ** 3)
assert.equal(resumedLargeInstall.remainingBytes, 33 * 1024 ** 3)

console.log('jobProgress logical transfer tests passed')

const jobProgressSource = await readFile(new URL('./jobProgress.ts', import.meta.url), 'utf8')
assert.ok(jobProgressSource.includes("job.kind === 'patch'"), 'patch jobs must participate in download progress')
assert.ok(jobProgressSource.includes("runningStep?.name.toLowerCase().includes('download')"), 'patch download detection must be phase-aware')
assert.ok(jobProgressSource.includes('const networkPercent = transfer.logicalPercent'), 'network UI must preserve logical progress across resume')
console.log('patch download progress contract tests passed')
