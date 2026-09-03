import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manager = fs.readFileSync(path.join(root, 'components', 'LuaGameManagerDialog.tsx'), 'utf8')
const runtime = fs.readFileSync(path.resolve(root, '..', 'src-tauri', 'src', 'lua_runtime_profiles.rs'), 'utf8')
const gse = fs.readFileSync(path.resolve(root, '..', 'src-tauri', 'src', 'gse_uc_setup.rs'), 'utf8')

test('opening Lua Game Manager does not hash the whole GSE bundle automatically', () => {
  const mountStart = manager.indexOf('useEffect(() => {\n    if (!appid) return\n    let active = true')
  const mountEnd = manager.indexOf('  }, [appid, gameName, onState])', mountStart)
  assert.ok(mountStart >= 0 && mountEnd > mountStart)
  const mountEffect = manager.slice(mountStart, mountEnd)
  assert.doesNotMatch(mountEffect, /getGseUcResourceHealth\s*\(/)
  assert.doesNotMatch(mountEffect, /verifyGseUcSetup\s*\(/)
})

test('settings use lightweight GSE metadata rather than full payload hashing', () => {
  const start = runtime.indexOf('fn component_health(app: &AppHandle)')
  const end = runtime.indexOf('\n}\n\nfn make_settings_state', start)
  assert.ok(start >= 0 && end > start)
  const body = runtime.slice(start, end)
  assert.match(body, /gse_uc_setup::lua_component_summary\(app\)/)
  assert.doesNotMatch(body, /lua_component_health\(app\)/)
})

test('lightweight GSE summary never calls file hashing or full component verification', () => {
  const start = gse.indexOf('pub fn lua_component_summary')
  const end = gse.indexOf('\npub fn lua_component_health', start)
  assert.ok(start >= 0 && end > start)
  const body = gse.slice(start, end)
  assert.doesNotMatch(body, /sha256_file|verify_component_at|compute_health/)
  assert.match(body, /read_manifest/)
})
