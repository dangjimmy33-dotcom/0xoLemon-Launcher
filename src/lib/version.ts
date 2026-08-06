/**
 * Returns the canonical version label used for update comparisons.
 * Display-only suffixes from catalog cards must never make an installed build
 * look older than the exact same depot version.
 */
export function cleanVersionLabel(value: string | null | undefined): string {
  if (!value) return ''
  return value
    .trim()
    .replace(/\s*-\s*Uploaded\b.*$/i, '')
    .replace(/\s*\(Build\b[^)]*\)\s*$/i, '')
    .trim()
}

export function versionsEquivalent(
  left: string | null | undefined,
  right: string | null | undefined,
): boolean {
  const normalizedLeft = cleanVersionLabel(left).toLocaleLowerCase('en-US')
  const normalizedRight = cleanVersionLabel(right).toLocaleLowerCase('en-US')
  return normalizedLeft.length > 0 && normalizedLeft === normalizedRight
}
