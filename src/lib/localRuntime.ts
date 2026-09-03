import type { LocalRuntimeIntegration, SaveProvider, SaveProviderKind } from '../types'

export const DEFAULT_LOCAL_RUNTIME: LocalRuntimeIntegration = {
  steamRuntime: 'none',
  achievementsEnabled: false,
  saveProviders: [],
}

const SAVE_PROVIDER_KINDS = new Set<SaveProviderKind>([
  'gse',
  'goldbergSteamEmu',
  'goldbergUplayEmu',
  'legacyVersioned',
])

export function normalizeLocalRuntime(value: unknown): LocalRuntimeIntegration {
  const raw = value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {}
  const steamRuntime = raw.steamRuntime === 'managedGse' ? 'managedGse' : 'none'
  const saveProviders = Array.isArray(raw.saveProviders)
    ? raw.saveProviders.flatMap<SaveProvider>((value) => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) return []
        const provider = value as Record<string, unknown>
        const kind = typeof provider.provider === 'string' ? provider.provider as SaveProviderKind : 'legacyVersioned'
        const saveId = typeof provider.saveId === 'string' ? provider.saveId.trim() : ''
        return SAVE_PROVIDER_KINDS.has(kind) && /^[A-Za-z0-9_-]+$/.test(saveId)
          ? [{ provider: kind, saveId }]
          : []
      })
    : []

  return {
    ...DEFAULT_LOCAL_RUNTIME,
    steamRuntime,
    achievementsEnabled: steamRuntime === 'managedGse' && raw.achievementsEnabled === true,
    saveProviders,
  }
}
