import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const library = await readFile(new URL('../components/library.tsx', import.meta.url), 'utf8')
const layout = await readFile(new URL('../components/layout.tsx', import.meta.url), 'utf8')
const lightning = await readFile(new URL('../themes/lightning/LightningShell.tsx', import.meta.url), 'utf8')
const steam = await readFile(new URL('../themes/steam/SteamShell.tsx', import.meta.url), 'utf8')
const xmcl = await readFile(new URL('../themes/xmcl/XmclShell.tsx', import.meta.url), 'utf8')
const css = await readFile(new URL('../App.css', import.meta.url), 'utf8')

assert.match(library, /beginPointerGameDrag/, 'Store cards must start the custom pointer drag flow')
assert.match(library, /document\.elementFromPoint/, 'pointer drag must resolve the live drop target under the cursor')
assert.match(library, /data-library-drop-target=\"true\"/, 'pointer drag must look for explicit Library targets')
assert.match(library, /0xo-add-to-library/, 'successful pointer drop must dispatch the existing add-to-library event')
assert.match(library, /draggable=\{false\}/, 'Store card drag must not depend on WebView HTML5 drag-and-drop')
for (const [name, source] of [['default', layout], ['lightning', lightning], ['steam', steam], ['xmcl', xmcl]]) {
  assert.match(source, /data-library-drop-target/, `${name} shell must expose a Library pointer-drop target`)
}
assert.match(library, /cloneNode\(true\)/, 'pointer drag must move a visual clone of the real Store card')
assert.match(library, /game-card-pointer-drag-status/, 'dragged card must show whether Library is a valid destination')
assert.match(library, /Library only/, 'invalid drag surfaces must clearly say that Library is the only destination')
assert.match(library, /Drop to Library/, 'Library hover must switch the drag status to a valid drop message')
assert.match(library, /setPointerCapture/, 'custom drag must retain pointer tracking while the lifted card moves')
assert.match(css, /\.store-game-card\.game-card-pointer-drag-ghost/, 'pointer drag must style the complete cloned game card')
assert.match(css, /cursor:\s*no-drop\s*!important/, 'invalid drop surfaces must use a no-drop cursor')
assert.match(css, /game-card-pointer-drag-can-drop/, 'valid Library hover must expose a distinct cursor state')
assert.match(css, /\.nav-item\.is-drag-over/, 'default Library nav must visibly acknowledge a drop target')

console.log('storeDragToLibrary.contract: PASS')
