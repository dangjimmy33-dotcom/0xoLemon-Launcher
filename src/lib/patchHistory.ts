export interface PatchHistoryRow {
  buildId: string
  title: string
  publishedAt: string
}

export interface BuildHistoryLike {
  build_id: string
  version: string | null
  build_date?: string
  manifests: Array<{ depot_id: number; manifest_gid: string }>
  patch_title?: string
  manifest_available?: boolean
  history_source?: 'custom' | 'steamdb_rss' | 'merged'
}

function decodeXml(value: string): string {
  return value
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, '$1')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .trim()
}

function tagValue(block: string, tag: string): string {
  const match = block.match(new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/${tag}>`, 'i'))
  return match ? decodeXml(match[1]) : ''
}

export function parseSteamDbPatchRss(xml: string): PatchHistoryRow[] {
  if (!xml || typeof xml !== 'string') return []
  const seen = new Set<string>()
  const rows: PatchHistoryRow[] = []
  const itemRe = /<item(?:\s[^>]*)?>([\s\S]*?)<\/item>/gi
  let match: RegExpExecArray | null

  while ((match = itemRe.exec(xml)) !== null) {
    const block = match[1]
    const guid = tagValue(block, 'guid')
    const buildMatch = guid.match(/^build#(\d+)$/i)
    if (!buildMatch) continue
    const buildId = buildMatch[1]
    if (seen.has(buildId)) continue
    seen.add(buildId)
    rows.push({
      buildId,
      title: tagValue(block, 'title'),
      publishedAt: tagValue(block, 'pubDate'),
    })
  }

  return rows
}

function dateScore(value?: string): number {
  if (!value) return 0
  if (/^\d+$/.test(value)) {
    const seconds = Number(value)
    return Number.isFinite(seconds) ? seconds * 1000 : 0
  }
  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : 0
}

export function mergeBuildHistory<T extends BuildHistoryLike>(
  customBuilds: T[],
  rssRows: PatchHistoryRow[],
): Array<T & BuildHistoryLike> {
  const byId = new Map<string, T & BuildHistoryLike>()

  for (const build of customBuilds) {
    byId.set(build.build_id, {
      ...build,
      manifest_available: build.manifests.length > 0,
      history_source: 'custom',
    })
  }

  for (const rss of rssRows) {
    const current = byId.get(rss.buildId)
    if (current) {
      byId.set(rss.buildId, {
        ...current,
        build_date: rss.publishedAt || current.build_date,
        patch_title: rss.title || current.patch_title,
        manifest_available: current.manifests.length > 0,
        history_source: 'merged',
      })
      continue
    }

    byId.set(rss.buildId, {
      build_id: rss.buildId,
      version: null,
      build_date: rss.publishedAt,
      manifests: [],
      patch_title: rss.title,
      manifest_available: false,
      history_source: 'steamdb_rss',
    } as unknown as T & BuildHistoryLike)
  }

  return Array.from(byId.values()).sort((a, b) => {
    const dateDelta = dateScore(b.build_date) - dateScore(a.build_date)
    if (dateDelta !== 0) return dateDelta
    return Number(b.build_id) - Number(a.build_id)
  })
}
