import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const activeView = readFileSync(new URL('../components/ActiveView.tsx', import.meta.url), 'utf8')
const library = readFileSync(new URL('../components/library.tsx', import.meta.url), 'utf8')

assert.ok(
  activeView.includes("import type { UiThemeId } from '../lib/uiThemes'"),
  'ActiveView must consume the shared UiThemeId instead of duplicating a narrower union',
)
assert.match(activeView, /uiTheme:\s*UiThemeId/, 'ActiveView uiTheme prop must accept every registered theme')

assert.ok(
  library.includes("import type { UiThemeId } from '../lib/uiThemes'"),
  'StoreLibraryView must consume the shared UiThemeId instead of duplicating a narrower union',
)
assert.match(library, /uiTheme:\s*UiThemeId/, 'StoreLibraryView uiTheme prop must accept every registered theme')

console.log('xmclThemeTypePropagation.contract: PASS')
