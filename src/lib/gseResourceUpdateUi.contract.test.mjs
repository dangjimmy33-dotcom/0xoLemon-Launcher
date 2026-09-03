import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const view = fs.readFileSync(path.join(here, '..', 'components', 'GseUcStandaloneView.tsx'), 'utf8')

assert.match(view, /const \[resourceUpdateStatus, setResourceUpdateStatus\]/, 'resource update status must be stored in UI state')
assert.match(view, /const checkResourceUpdates = async \(\) =>/, 'Check updates must have a real handler')
assert.doesNotMatch(view, /invoke\('gse_auto_setup_check_updates'\)\.catch\(\(\) => \{\}\)/, 'Check updates must not silently discard result/errors')
assert.match(view, /resourceStatusSuffix\(resourceUpdateStatus\?\.gse\)/, 'GSE latest version must be shown')
assert.match(view, /Component release check complete/, 'completion must be visible/logged')
console.log('GSE resource update UI contract: PASS')
