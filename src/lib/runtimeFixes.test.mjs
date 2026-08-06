import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { cleanVersionLabel, versionsEquivalent } from './version.ts'

assert.equal(cleanVersionLabel('1.01.1.0.3 (Build 22800422) - Uploaded 2026-07-15'), '1.01.1.0.3')
assert.equal(cleanVersionLabel('v2.16.2 (Build 23704290)'), 'v2.16.2')
assert.equal(versionsEquivalent('1.01.1.0.3 (Build 22800422) - Uploaded 2026-07-15', '1.01.1.0.3'), true)
assert.equal(versionsEquivalent('v2.16.1', 'v2.16.2'), false)

const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
assert.ok(app.includes("gameUpdateMode: 'manual'"), 'silent game updates must be opt-in by default')
assert.ok(app.includes('offlineInterruptedJobIdRef'), 'online recovery must only resume a job interrupted in this session')
assert.ok(app.includes('pendingHomeLaunchRef.current = payload.gameId'), 'multi-executable shortcuts must reopen the normal launch picker')
assert.ok(app.includes("invoke('clear_job_journal')"), 'committed transfers must be dismissed from the queue')
assert.ok(app.includes("next.lastJob?.status === 'committed'"), 'stale committed journals must be cleared on launcher restart')
assert.ok(!/setPlayingGames\(\(prev\)[\s\S]{0,120}\[selectedGame\.id\]: true/.test(app), 'launch UI must not mark Running before backend confirms process start')
assert.ok(app.includes('versionsEquivalent(state.currentVersion, latest)'), 'update catalog must use canonical version comparison')

const downloads = await readFile(new URL('../components/downloads.tsx', import.meta.url), 'utf8')
assert.ok(downloads.includes('isInstalled'), 'Downloads empty state must know whether the selected game is already installed')
assert.ok(downloads.includes('Game is installed'), 'installed games must not be presented as pending downloads')

const turboCss = await readFile(new URL('../components/GameTurboModal.css', import.meta.url), 'utf8')
assert.ok(turboCss.includes('background: #0b1014'), 'Turbo dialog must use an opaque launcher surface')
assert.ok(!turboCss.includes('linear-gradient'), 'Turbo dialog must avoid flashy AI-style gradients')

console.log('runtime and queue regression tests passed')
