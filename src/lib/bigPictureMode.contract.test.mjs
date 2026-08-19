import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const view = await readFile(new URL('../components/BigPictureView.tsx', import.meta.url), 'utf8')
const css = await readFile(new URL('../components/BigPictureView.css', import.meta.url), 'utf8')
const capabilities = await readFile(new URL('../../src-tauri/capabilities/default.json', import.meta.url), 'utf8')
const lifecycle = await readFile(new URL('./bigPictureMode.ts', import.meta.url), 'utf8').catch(() => '')

assert.ok(capabilities.includes('core:window:allow-set-fullscreen'), 'Big Picture must be allowed to enter native fullscreen')
assert.ok(capabilities.includes('core:window:allow-is-fullscreen'), 'Big Picture must be allowed to snapshot native fullscreen state')
assert.ok(lifecycle.includes('setFullscreen(true)'), 'native Big Picture helper must request fullscreen on enter')
assert.ok(lifecycle.includes('setFullscreen(previousFullscreen)'), 'native Big Picture helper must restore the previous fullscreen state')
assert.ok(app.includes("'entering' | 'active' | 'exiting'"), 'App must expose an explicit Big Picture lifecycle state')
assert.ok(!app.includes("phase={bigPicturePhase === 'closed' ? 'active' : bigPicturePhase}"), 'render branch must not re-check closed after isBigPictureMode narrows the phase')
assert.ok(app.includes('phase={bigPicturePhase}'), 'Big Picture must receive the already-narrowed lifecycle phase directly')
assert.ok(app.includes('onPlayGame='), 'App must wire Big Picture Play to launcher game launch flow')
assert.ok(view.includes('onPlayGame: (gameId: string) => void'), 'Big Picture must expose a real launch callback')
assert.ok(view.includes('pressedEdge(0)') && view.includes('launchActive() // A / Cross'), 'gamepad A/Cross must launch the active game')
assert.ok(view.includes('pressedEdge(1)') && view.includes('// B / Circle') && view.includes('else onExit()'), 'gamepad B/Circle must close overlays or exit Big Picture')
assert.ok(view.includes('pressedEdge(4)') && view.includes('pressedEdge(5)'), 'gamepad shoulder buttons must navigate the carousel')
assert.ok(css.includes('--theme-accent'), 'Big Picture accents must inherit Color Studio')
assert.ok(css.includes('--theme-modal-bg'), 'Big Picture overlays must inherit themed modal surfaces')
assert.ok(css.includes('.bp-cinematic-curtain'), 'Big Picture must have a dedicated cinematic transition layer')
assert.ok(css.includes('@media (prefers-reduced-motion: reduce)'), 'Big Picture CSS must respect reduced motion')

console.log('native Big Picture contract PASS')
