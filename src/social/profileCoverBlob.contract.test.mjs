import fs from 'node:fs'
import path from 'node:path'

const file = path.resolve('src/social/SocialPrototype.tsx')
const source = fs.readFileSync(file, 'utf8')

const snapshotBlobPattern = /const\s+coverBytesForBlob\s*=\s*new\s+Uint8Array\(snapshot\.coverBytes\)[\s\S]{0,240}?new\s+Blob\(\[coverBytesForBlob\]/

if (!snapshotBlobPattern.test(source)) {
  console.error('FAIL: restored/local cover bytes must be cloned into an ArrayBuffer-backed Uint8Array before Blob construction')
  process.exit(1)
}

console.log('PASS: local profile cover Blob uses an ArrayBuffer-backed Uint8Array copy')
