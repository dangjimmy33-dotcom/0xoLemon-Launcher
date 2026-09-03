import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const tsx = fs.readFileSync(path.join(here, 'DepotDownloaderView.tsx'), 'utf8')
const css = fs.readFileSync(path.join(here, 'DepotDownloaderView.css'), 'utf8')

test('live depot logs scroll only inside the terminal, never the whole detail pane', () => {
  assert.doesNotMatch(tsx, /scrollIntoView\s*\(/)
  assert.match(tsx, /terminalBodyRef/)
  assert.match(tsx, /terminalBodyRef\.current\.scrollTop\s*=\s*terminalBodyRef\.current\.scrollHeight/)
  assert.match(tsx, /className="depot-terminal-body"\s+ref=\{terminalBodyRef\}/)
})

test('download CTA avoids the old hard-coded orange slab', () => {
  const start = css.indexOf('.depot-btn-primary-start {')
  const end = css.indexOf('}', start)
  const block = css.slice(start, end + 1)
  assert.doesNotMatch(block, /#ff9f43|#f59e0b/)
  assert.match(block, /border:/)
})


test('game selection races cannot mix one game with another build or folder', () => {
  assert.match(tsx, /detailRequestSeqRef/)
  assert.match(tsx, /const requestId = \+\+detailRequestSeqRef\.current/)
  assert.match(tsx, /setSelectedBuildId\(''\)/)
  assert.match(tsx, /detailRequestSeqRef\.current !== requestId/)
  assert.match(tsx, /appid: gameDetail\.appid/)
  assert.match(tsx, /folderName: gameDetail\.folderName/)
})

test('catalog and inspector own their scroll instead of chaining to the whole page', () => {
  assert.match(css, /\.depot-main-layout\s*\{[\s\S]*?min-height:\s*0/)
  assert.match(css, /\.depot-cards-grid\s*\{[\s\S]*?overflow-y:\s*auto[\s\S]*?overscroll-behavior-y:\s*contain/)
  assert.match(css, /\.depot-detail-scroll\s*\{[\s\S]*?overflow-y:\s*auto[\s\S]*?overscroll-behavior-y:\s*contain/)
})

test('download button matches Lua Shop Add to Steam neutral action styling', () => {
  const start = css.indexOf('.depot-btn-primary-start {')
  const end = css.indexOf('}', start)
  const block = css.slice(start, end + 1)
  assert.match(block, /background:\s*rgba\(255, 255, 255, 0\.1\)/)
  assert.match(block, /border[^;]*rgba\(255, 255, 255, 0\.2\)/)
  assert.match(block, /color:\s*#fff/)
})

test('Depot Downloader persistent layer replaces the normal tab body instead of being pushed below it', () => {
  const app = fs.readFileSync(path.join(here, '..', 'App.tsx'), 'utf8')
  const appCss = fs.readFileSync(path.join(here, '..', 'App.css'), 'utf8')
  // Tab content is hidden for both Depot Downloader and GSE / UC Setup (each has its own persistent layer)
  assert.match(app, /activeTab === 'Depot Downloader' \|\| activeTab === 'GSE \/ UC Setup'\) \? \{ display: 'none' \} : undefined/)
  assert.match(app, /className="depot-persistent-layer"/)
  assert.match(appCss, /\.depot-persistent-layer[\s\S]*?\{[\s\S]*?flex:\s*1 1 0[\s\S]*?min-height:\s*0[\s\S]*?width:\s*100%/)
})

test('Depot Downloader exposes Steam-like pause/resume and explicit version switching', () => {
  assert.match(tsx, /depot_downloader_pause_download/)
  assert.match(tsx, /depot_downloader_resume_download/)
  assert.match(tsx, /depot_downloader_get_install_state/)
  assert.match(tsx, /isPaused/)
  assert.match(tsx, /installedBuildId/)
  assert.match(tsx, /Switch to BuildID|Chuyển sang BuildID/)
  assert.match(tsx, /Pause|Tạm dừng/)
  assert.match(tsx, /Resume|Tiếp tục/)
})
