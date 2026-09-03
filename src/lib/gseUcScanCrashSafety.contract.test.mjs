import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const dialog = fs.readFileSync(path.join(root, 'components', 'LuaGameManagerDialog.tsx'), 'utf8')
const runtime = fs.readFileSync(path.resolve(root, '..', 'src-tauri', 'src', 'lua_runtime_profiles.rs'), 'utf8')
const gse = fs.readFileSync(path.resolve(root, '..', 'src-tauri', 'src', 'gse_uc_setup.rs'), 'utf8')

test('Check game only performs the read-only target scan and never starts GSE bundle verification', () => {
  const start = dialog.indexOf('  const scanRuntimeTarget = async () => {')
  const end = dialog.indexOf('\n  const approveRuntimeTarget', start)
  assert.ok(start >= 0 && end > start)
  const body = dialog.slice(start, end)
  assert.match(body, /scan_lua_runtime_target/)
  assert.doesNotMatch(body, /planGseUcSetup\s*\(/)
  assert.doesNotMatch(body, /getGseUcResourceHealth\s*\(/)
  assert.doesNotMatch(body, /verifyGseUcSetup\s*\(/)
})

test('target scan runs blocking filesystem work off the Tauri main thread', () => {
  const start = runtime.indexOf('pub async fn scan_lua_runtime_target')
  assert.ok(start >= 0, 'scan_lua_runtime_target must be async')
  const end = runtime.indexOf('\n}\n\n#[tauri::command]', start)
  const body = runtime.slice(start, end)
  assert.match(body, /spawn_blocking/)
  assert.match(body, /scan_installed_game/)
})

test('GSE plan runs blocking hashing and filesystem work off the Tauri main thread', () => {
  const start = gse.indexOf('pub async fn plan_gse_uc_setup')
  assert.ok(start >= 0, 'plan_gse_uc_setup must be async')
  const end = gse.indexOf('\n}\n\nfn app_local_root', start)
  const body = gse.slice(start, end)
  assert.match(body, /spawn_blocking/)
  assert.match(body, /plan_internal/)
})

test('Rust-internal GSE planning uses the synchronous scan helper instead of awaiting the Tauri command', () => {
  assert.match(runtime, /pub\(crate\) fn scan_lua_runtime_target_sync/)
  assert.doesNotMatch(gse, /lua_runtime_profiles::scan_lua_runtime_target\(app\.clone\(\),/)
  assert.match(gse, /lua_runtime_profiles::scan_lua_runtime_target_sync\(app,/)
})

test('full GSE resource health hashing also runs off the Tauri main thread', () => {
  const start = gse.indexOf('pub async fn get_gse_uc_resource_health')
  assert.ok(start >= 0, 'get_gse_uc_resource_health must be async')
  const end = gse.indexOf('\n}\n\n/// Lightweight component metadata', start)
  const body = gse.slice(start, end)
  assert.match(body, /spawn_blocking/)
  assert.match(body, /compute_health/)
})
