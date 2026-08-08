import assert from 'node:assert/strict'
import { parseSteamDbPatchRss, mergeBuildHistory } from './patchHistory.ts'

const rss = `<?xml version="1.0"?><rss><channel>
<item><guid>build#23634047</guid><title>Resident Evil Requiem update for 25 June 2026</title><pubDate>Thu, 25 Jun 2026 02:00:04 +0000</pubDate></item>
<item><guid isPermaLink="false">build#22472737</guid><title>Notice &amp; Update</title><pubDate>Fri, 27 Mar 2026 01:00:31 +0000</pubDate></item>
</channel></rss>`

const rows = parseSteamDbPatchRss(rss)
assert.equal(rows.length, 2)
assert.deepEqual(rows[0], {
  buildId: '23634047',
  title: 'Resident Evil Requiem update for 25 June 2026',
  publishedAt: 'Thu, 25 Jun 2026 02:00:04 +0000',
})
assert.equal(rows[1].title, 'Notice & Update')

const merged = mergeBuildHistory([
  { build_id: '23634047', version: '1.2.0', build_date: undefined, manifests: [{ depot_id: 1, manifest_gid: '9' }] },
  { build_id: '99999999', version: null, build_date: '1780000000', manifests: [{ depot_id: 2, manifest_gid: '10' }] },
], rows)

assert.equal(merged[0].build_id, '23634047')
assert.equal(merged[0].build_date, 'Thu, 25 Jun 2026 02:00:04 +0000')
assert.equal(merged[0].manifest_available, true)
assert.equal(merged.find((row) => row.build_id === '22472737')?.manifest_available, false)
assert.equal(merged.find((row) => row.build_id === '22472737')?.patch_title, 'Notice & Update')
console.log('patchHistory tests passed')
