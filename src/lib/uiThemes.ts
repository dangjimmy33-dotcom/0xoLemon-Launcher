export type UiThemeId = 'default' | 'lightning' | 'steam' | 'xmcl'

export type ThemeAccentMode = 'native' | 'custom'

export type UiThemeNativePalette = {
  hue: number
  chroma: number
  accent: string
  accentStrong: string
  accentDeep: string
}

export type UiThemeProfile = {
  id: UiThemeId
  label: string
  description: string
  previewLabel: string
  referenceVersion: string
  tier: 'standard' | 'additional'
  recommended?: boolean
  nativePalette: UiThemeNativePalette
}

/**
 * A theme owns its native palette and structural reference. Color Studio only
 * overrides that palette after the user explicitly selects Custom Accent.
 */
export const UI_THEME_PROFILES: readonly UiThemeProfile[] = [
  {
    id: 'default',
    label: '0xoLemon Default',
    description: 'The original adaptive 0xoLemon interface using the current Color Wheel palette.',
    previewLabel: 'Adaptive layout',
    referenceVersion: '0xolemon-v2',
    tier: 'standard',
    nativePalette: {
      hue: 82,
      chroma: 56,
      accent: '#E6B84A',
      accentStrong: '#FFD36B',
      accentDeep: '#A87619',
    },
  },
  {
    id: 'lightning',
    label: '0xoLemon Cinematic',
    description: 'The recommended cinematic 0xoLemon workspace with integrated launcher tools and services.',
    previewLabel: 'Recommended cinematic workspace',
    referenceVersion: 'project-lightning-v5.0.8-snapshot',
    tier: 'standard',
    recommended: true,
    nativePalette: {
      hue: 24,
      chroma: 76,
      accent: '#FF7448',
      accentStrong: '#FFAD70',
      accentDeep: '#9C2C50',
    },
  },
  {
    id: 'steam',
    label: 'Steam Library',
    description: 'A versioned Steam desktop workbench rebuilt around 0xoLemon services and game data.',
    previewLabel: 'Steam desktop snapshot',
    referenceVersion: 'steam-desktop-2026.08',
    tier: 'additional',
    nativePalette: {
      hue: 203,
      chroma: 48,
      accent: '#1A9FFF',
      accentStrong: '#66C0F4',
      accentDeep: '#0E5F91',
    },
  },
  {
    id: 'xmcl',
    label: 'X Minecraft Launcher',
    description: 'A versioned XMCL interaction model that maps 0xoLemon games to compact launcher instances.',
    previewLabel: 'XMCL instance launcher',
    referenceVersion: 'xmcl-source-2026.08',
    tier: 'additional',
    nativePalette: {
      hue: 190,
      chroma: 44,
      accent: '#39B9C7',
      accentStrong: '#7ADBE3',
      accentDeep: '#176D78',
    },
  },
] as const

export function isUiThemeId(value: unknown): value is UiThemeId {
  return value === 'default' || value === 'lightning' || value === 'steam' || value === 'xmcl'
}

export function isThemeAccentMode(value: unknown): value is ThemeAccentMode {
  return value === 'native' || value === 'custom'
}

export function getUiThemeProfile(id: UiThemeId): UiThemeProfile {
  return UI_THEME_PROFILES.find((theme) => theme.id === id) ?? UI_THEME_PROFILES[0]
}
