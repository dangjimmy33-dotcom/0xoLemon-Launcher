import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'

const view = fs.readFileSync(new URL('./GseUcStandaloneView.tsx', import.meta.url), 'utf8')
const layout = fs.readFileSync(new URL('./layout.tsx', import.meta.url), 'utf8')
const app = fs.readFileSync(new URL('../App.tsx', import.meta.url), 'utf8')
const manager = fs.readFileSync(new URL('./LuaGameManagerDialog.tsx', import.meta.url), 'utf8')

test('GSE / UC Setup is a first-class sidebar tab', () => {
  assert.match(layout, /\['GSE \/ UC Setup'/)
  assert.match(app, /GseUcStandaloneView/)
})

test('standalone UI mirrors original workspaces and hides API key', () => {
  for (const text of ['Setup &amp; Emulator','Savegame Manager','Real AppID','Game folder','GSE deployment','SteamStub handling','Identity & saves','Overlay & compatibility','Resources & updates','Activity']) {
    assert.ok(view.includes(text), `missing ${text}`)
  }
  assert.doesNotMatch(view, /Steam Web API key/i)
  assert.match(view, /gse_auto_setup_run/)
})

test('Lua Game Manager no longer embeds GSE auto setup controls', () => {
  assert.doesNotMatch(manager, /GSE \/ UC Setup/)
  assert.doesNotMatch(manager, /planGseUcSetup|applyGseUcSetup|verifyGseUcSetup|repairGseUcSetup|restoreGseUcSetup/)
})
