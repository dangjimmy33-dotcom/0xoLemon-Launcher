import fs from 'node:fs'
import path from 'node:path'
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const mode = fs.readFileSync(path.join(root, 'lib', 'bigPictureMode.ts'), 'utf8')
const view = fs.readFileSync(path.join(root, 'components', 'BigPictureView.tsx'), 'utf8')
const css = fs.readFileSync(path.join(root, 'components', 'BigPictureView.css'), 'utf8')

assert.match(mode, /onFocusChanged/, 'Big Picture must repair Windows shell focus transitions')
assert.match(mode, /setAlwaysOnTop\(false\)[\s\S]*setAlwaysOnTop\(true\)/, 'repair must refresh HWND topmost z-order')
assert.match(mode, /disposeMaintenance/, 'focus maintenance must be disposed on exit')
assert.match(view, /scrollIntoView\([\s\S]*inline:\s*'center'/, 'active grid card must be browser-centered')
assert.match(view, /visibilitychange/, 'carousel must resync after screenshot\/visibility overlays')
assert.match(css, /flex:\s*0 0 clamp\(360px, 38vh, 450px\)/, 'carousel must reserve poster headroom')
assert.match(css, /scroll-snap-type:\s*x mandatory/, 'carousel must have deterministic snap centering')
console.log('Big Picture v4.4 contract passed')
