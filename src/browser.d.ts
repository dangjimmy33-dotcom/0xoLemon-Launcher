interface NetworkInformation extends EventTarget {
  readonly effectiveType?: string
}

interface BatteryManager extends EventTarget {
  readonly level: number
  readonly charging: boolean
}

interface Navigator {
  readonly connection?: NetworkInformation
  readonly mozConnection?: NetworkInformation
  readonly webkitConnection?: NetworkInformation
  getBattery?: () => Promise<BatteryManager>
}

type DefenderExclusionResult = { success: boolean; skipped?: boolean }

interface Window {
  globalAssetsOverride?: Record<string, string>
  globalVersionTags?: Record<string, Record<string, string[]>>
  globalGameStats?: Record<string, unknown>
  __defenderExclusionResolve?: (result: DefenderExclusionResult) => void
}

interface WindowEventMap {
  'lua-game-mode-changed': CustomEvent<{ gameId: string; added: boolean }>
}
