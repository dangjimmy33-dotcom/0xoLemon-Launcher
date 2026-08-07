import assert from 'node:assert/strict'
import { cloudSavePresentation, quotaPercent } from './cloudSaveStatus.mjs'

const vi = {
  synced: { title: 'Đã đồng bộ', description: 'Đã bảo vệ.' },
  storage_full: { title: 'Google Drive đã đầy', description: 'Save vẫn an toàn.' },
  conflict_check_required: { title: 'Đang kiểm tra bản Save mới', description: 'Đang kiểm tra.' },
}

assert.equal(cloudSavePresentation({ state: 'synced' }).title, 'Synced')
assert.equal(cloudSavePresentation({ state: 'synced' }, vi).title, 'Đã đồng bộ')
assert.equal(cloudSavePresentation({ state: 'rate_limited' }).blocking, false)
assert.equal(cloudSavePresentation({ state: 'storage_full' }, vi).title, 'Google Drive đã đầy')
assert.equal(cloudSavePresentation({ state: 'conflict' }).blocking, true)
assert.equal(cloudSavePresentation({ state: 'waiting_for_save' }).blocking, false)
assert.equal(cloudSavePresentation({ state: 'conflict_check_required' }, vi).title, 'Đang kiểm tra bản Save mới')
assert.equal(quotaPercent({ limitBytes: 100, usageBytes: 25 }), 25)
assert.equal(quotaPercent({ limitBytes: null, usageBytes: 25 }), null)
console.log('cloud save status presentation: PASS')
