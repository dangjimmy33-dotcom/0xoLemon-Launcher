import type { GameVersionInfo } from '../types'

type VersionTagTable = Record<string, string[]>

function stringValue(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

function numberValue(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function buildIdFromVersion(version: string): string {
  return version.match(/\(Build ([^)]+)\)/i)?.[1]?.trim() ?? ''
}

function cleanVersionLabel(value: string): string {
  return value
    .replace(/\s*-\s*Uploaded\s+\d{4}-\d{2}-\d{2}.*$/i, '')
    .replace(/\s*\(Build [^)]+\)\s*/i, '')
    .trim()
}

function semverPrefix(value: string): string {
  return value ? value.split(/[ (]/)[0].trim() : value
}

function tagsForVersion(version: GameVersionInfo, versionTags: VersionTagTable): string[] | undefined {
  const keys = [
    version.version,
    version.label,
    version.buildId,
    semverPrefix(version.version),
    semverPrefix(version.label),
    semverPrefix(version.buildId),
  ]
  return keys.map((key) => versionTags[key]).find(Boolean) ?? version.tags
}

export function normalizeGameVersions(value: unknown, versionTags: VersionTagTable = {}): GameVersionInfo[] {
  if (!Array.isArray(value)) return []

  return value.flatMap((entry): GameVersionInfo[] => {
    let normalized: GameVersionInfo

    if (typeof entry === 'string') {
      normalized = {
        version: entry,
        label: cleanVersionLabel(entry),
        buildId: buildIdFromVersion(entry),
        sizeBytes: 0,
        latest: false,
      }
    } else if (entry && typeof entry === 'object') {
      const record = entry as Record<string, unknown>
      const version = stringValue(record.version)
      const rawLabel = stringValue(record.label) || version
      normalized = {
        version,
        label: cleanVersionLabel(rawLabel),
        buildId: stringValue(record.buildId) || buildIdFromVersion(version),
        sizeBytes: numberValue(record.sizeBytes),
        latest: record.latest === true,
        tags: Array.isArray(record.tags)
          ? record.tags.filter((tag): tag is string => typeof tag === 'string')
          : undefined,
      }
    } else {
      return []
    }

    return [{ ...normalized, tags: tagsForVersion(normalized, versionTags) }]
  })
}
