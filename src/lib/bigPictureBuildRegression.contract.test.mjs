import fs from 'node:fs'
import path from 'node:path'
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const mode = fs.readFileSync(path.join(root, 'lib', 'bigPictureMode.ts'), 'utf8')
const view = fs.readFileSync(path.join(root, 'components', 'BigPictureView.tsx'), 'utf8')

assert.doesNotMatch(view, /const\s+sync\s*=\s*\(\)\s*=>/, 'BigPictureView must not keep an unused sync callback under noUnusedLocals')
assert.doesNotMatch(mode, /appWindow\.currentMonitor\s*\(/, 'currentMonitor is a module-level Tauri API, not a Window instance method')
assert.match(mode, /currentMonitor\s*:\s*typeof import\('@tauri-apps\/api\/window'\)\['currentMonitor'\]/, 'maintenance repair should receive the module-level currentMonitor helper')

console.log('Big Picture build regression contract PASS')
