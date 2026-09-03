import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const reveal = await readFile(new URL('../hooks/useScrollReveal.ts', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')

assert.ok(reveal.includes('documentMutation.observe(document.body'), 'scroll reveal must survive the workspace root being unmounted by Big Picture')
assert.ok(reveal.includes('nextRoot === currentRoot'), 'scroll reveal must detect/rebind a replacement workspace root')
assert.ok(reveal.includes('intersection?.disconnect()'), 'old IntersectionObserver roots must be disconnected before rebinding')
assert.ok(app.includes("window.dispatchEvent(new Event('resize'))"), 'Big Picture exit must notify the remounted normal layout after window restore')

console.log('Big Picture exit/remount regression PASS')
