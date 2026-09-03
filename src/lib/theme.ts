import type { LauncherPreferences } from './preferences'
import { getUiThemeProfile } from './uiThemes'

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))

export const THEME_LIGHTNESS = 0.73
export const THEME_MIN_CHROMA = 0.025
export const THEME_MAX_CHROMA = 0.13

function srgbEncode(value: number) {
  const v = clamp(value, 0, 1)
  return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055
}

export function oklchToHex(lightness: number, chroma: number, hue: number) {
  const h = (hue * Math.PI) / 180
  const a = chroma * Math.cos(h)
  const b = chroma * Math.sin(h)

  const lPrime = lightness + 0.3963377774 * a + 0.2158037573 * b
  const mPrime = lightness - 0.1055613458 * a - 0.0638541728 * b
  const sPrime = lightness - 0.0894841775 * a - 1.291485548 * b
  const l = lPrime ** 3
  const m = mPrime ** 3
  const s = sPrime ** 3

  const r = srgbEncode(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s)
  const g = srgbEncode(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s)
  const blue = srgbEncode(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s)
  const channel = (value: number) => Math.round(clamp(value, 0, 1) * 255).toString(16).padStart(2, '0')
  return `#${channel(r)}${channel(g)}${channel(blue)}`.toUpperCase()
}

export function launcherAccent(preferences: Pick<LauncherPreferences, 'accentHue' | 'accentChroma'>) {
  const hue = ((Number(preferences.accentHue) % 360) + 360) % 360
  const chromaPercent = clamp(Number(preferences.accentChroma), 0, 100)
  const chroma = THEME_MIN_CHROMA + (chromaPercent / 100) * (THEME_MAX_CHROMA - THEME_MIN_CHROMA)
  return {
    hue,
    chromaPercent,
    chroma,
    base: `oklch(73% ${chroma.toFixed(3)} ${hue.toFixed(1)})`,
    strong: `oklch(80% ${(chroma * 0.92).toFixed(3)} ${hue.toFixed(1)})`,
    deep: `oklch(58% ${(chroma * 0.90).toFixed(3)} ${hue.toFixed(1)})`,
    hex: oklchToHex(THEME_LIGHTNESS, chroma, hue),
  }
}

type ThemePreferences = Pick<
  LauncherPreferences,
  | 'uiTheme'
  | 'themeAccentMode'
  | 'accentHue'
  | 'accentChroma'
  | 'themeIntensity'
  | 'themeContrast'
  | 'dynamicTheme'
  | 'dynamicThemeSpeed'
  | 'motionMode'
>

function setCommonSemanticAliases(root: HTMLElement) {
  root.style.setProperty('--ui-page-bg', 'var(--launcher-page-bg)')
  root.style.setProperty('--ui-page-bg-soft', 'var(--launcher-page-bg-soft)')
  root.style.setProperty('--ui-frame-bg', 'var(--launcher-frame-bg)')
  root.style.setProperty('--ui-sidebar-bg', 'var(--launcher-sidebar-bg)')
  root.style.setProperty('--ui-panel-bg', 'var(--theme-card-bg)')
  root.style.setProperty('--ui-panel-bg-soft', 'var(--theme-card-bg-soft)')
  root.style.setProperty('--ui-elevated-bg', 'var(--theme-elevated-bg)')
  root.style.setProperty('--ui-popup-bg', 'var(--theme-modal-bg)')
  root.style.setProperty('--ui-popup-bg-strong', 'var(--theme-modal-bg-strong)')
  root.style.setProperty('--ui-overlay-bg', 'var(--theme-overlay-bg)')
  root.style.setProperty('--ui-selection-bg', 'var(--theme-selected-bg)')
  root.style.setProperty('--ui-hover-bg', 'var(--theme-hover-bg)')
  root.style.setProperty('--ui-border', 'var(--line)')
  root.style.setProperty('--ui-border-strong', 'var(--line-strong)')
  root.style.setProperty('--ui-text', 'var(--text)')
  root.style.setProperty('--ui-text-strong', 'var(--text-strong)')
  root.style.setProperty('--ui-text-muted', 'var(--muted)')
  root.style.setProperty('--ui-text-subtle', 'var(--subtle)')
  root.style.setProperty('--ui-radius-sm', '6px')
  root.style.setProperty('--ui-radius-md', '10px')
  root.style.setProperty('--ui-radius-lg', '14px')
  root.style.setProperty('--ui-shadow-popup', '0 26px 80px rgba(0,0,0,.48)')
}

export function applyLauncherTheme(preferences: ThemePreferences) {
  if (typeof document === 'undefined') return

  const root = document.documentElement
  const profile = getUiThemeProfile(preferences.uiTheme)
  const customAccent = launcherAccent(preferences)
  const useNativeAccent = preferences.themeAccentMode === 'native' && profile.id !== 'default'
  const accent = useNativeAccent
    ? {
      hue: profile.nativePalette.hue,
      chromaPercent: profile.nativePalette.chroma,
      chroma: THEME_MIN_CHROMA + (profile.nativePalette.chroma / 100) * (THEME_MAX_CHROMA - THEME_MIN_CHROMA),
      base: profile.nativePalette.accent,
      strong: profile.nativePalette.accentStrong,
      deep: profile.nativePalette.accentDeep,
      hex: profile.nativePalette.accent,
    }
    : customAccent
  const intensity = clamp(Number(preferences.themeIntensity), 0, 100)
  const contrast = clamp(Number(preferences.themeContrast), 0, 100)
  const speed = clamp(Number(preferences.dynamicThemeSpeed), 0, 100)
  const dynamicThemeEnabled = preferences.dynamicTheme

  const intensity01 = intensity / 100
  const contrast01 = contrast / 100
  const pageTint = 12 + intensity01 * 20
  const chromeTint = 14 + intensity01 * 22
  const sidebarTint = 16 + intensity01 * 24
  const cardTint = 18 + intensity01 * 28
  const elevatedTint = 22 + intensity01 * 32
  const hoverTint = 10 + intensity01 * 12
  const selectedTint = 18 + intensity01 * 18
  const lineTint = 16 + contrast01 * 24
  const lineStrongTint = 30 + contrast01 * 30

  const pageL = 10.5 + (1 - contrast01) * 2.1
  const cardL = 15.4 + contrast01 * 3.5
  const elevatedL = 18.2 + contrast01 * 5.0
  const chromeL = 9.5 + (1 - contrast01) * 1.8
  const sidebarL = 9.0 + (1 - contrast01) * 1.6
  const cycleSeconds = 150 - speed * 1.15

  root.setAttribute('data-ui-theme', profile.id)
  root.setAttribute('data-theme-accent-mode', useNativeAccent ? 'native' : 'custom')
  root.setAttribute('data-theme-reference', profile.referenceVersion)
  root.style.setProperty('--theme-hue-start', accent.hue.toFixed(2))
  root.style.setProperty('--theme-runtime-hue', accent.hue.toFixed(2))
  root.style.setProperty('--theme-accent-chroma-value', accent.chroma.toFixed(4))
  root.style.setProperty('--theme-accent-hue', String(accent.hue))
  root.style.setProperty('--theme-accent-chroma', String(accent.chromaPercent))
  root.style.setProperty('--theme-intensity', String(intensity))
  root.style.setProperty('--theme-contrast', String(contrast))
  root.style.setProperty('--theme-cycle-duration', `${cycleSeconds.toFixed(1)}s`)

  root.style.setProperty('--theme-accent', useNativeAccent ? accent.base : `oklch(73% ${accent.chroma.toFixed(4)} var(--theme-runtime-hue))`)
  root.style.setProperty('--theme-accent-strong', useNativeAccent ? accent.strong : `oklch(81% ${(accent.chroma * 0.94).toFixed(4)} var(--theme-runtime-hue))`)
  root.style.setProperty('--theme-accent-deep', useNativeAccent ? accent.deep : `oklch(56% ${(accent.chroma * 0.88).toFixed(4)} var(--theme-runtime-hue))`)

    root.style.setProperty('--launcher-page-bg', `color-mix(in oklab, oklch(${pageL.toFixed(2)}% 0.008 var(--theme-runtime-hue)) ${(100 - pageTint).toFixed(1)}%, var(--theme-accent-deep) ${pageTint.toFixed(1)}%)`)
    root.style.setProperty('--launcher-page-bg-soft', `color-mix(in oklab, oklch(${(pageL + 1.8).toFixed(2)}% 0.010 var(--theme-runtime-hue)) ${(100 - pageTint - 2).toFixed(1)}%, var(--theme-accent) ${(pageTint + 2).toFixed(1)}%)`)
    root.style.setProperty('--launcher-chrome-bg', `color-mix(in oklab, oklch(${chromeL.toFixed(2)}% 0.007 var(--theme-runtime-hue)) ${(100 - chromeTint).toFixed(1)}%, var(--theme-accent-deep) ${chromeTint.toFixed(1)}%)`)
    root.style.setProperty('--launcher-chrome-bg-strong', `color-mix(in oklab, #05070a ${(100 - chromeTint + 2).toFixed(1)}%, var(--theme-accent-deep) ${(chromeTint - 2).toFixed(1)}%)`)
    root.style.setProperty('--launcher-sidebar-bg', `color-mix(in oklab, oklch(${sidebarL.toFixed(2)}% 0.008 var(--theme-runtime-hue)) ${(100 - sidebarTint).toFixed(1)}%, var(--theme-accent-deep) ${sidebarTint.toFixed(1)}%)`)
    root.style.setProperty('--launcher-frame-bg', 'var(--launcher-sidebar-bg)')
    root.style.setProperty('--launcher-corner-bg', 'var(--launcher-frame-bg)')
    root.style.setProperty('--theme-card-bg', `color-mix(in oklab, oklch(${cardL.toFixed(2)}% 0.010 var(--theme-runtime-hue)) ${(100 - cardTint).toFixed(1)}%, var(--theme-accent-deep) ${cardTint.toFixed(1)}%)`)
    root.style.setProperty('--theme-card-bg-soft', `color-mix(in oklab, oklch(${(cardL - 1.5).toFixed(2)}% 0.008 var(--theme-runtime-hue)) ${(100 - cardTint + 3).toFixed(1)}%, var(--theme-accent-deep) ${(cardTint - 3).toFixed(1)}%)`)
    root.style.setProperty('--theme-elevated-bg', `color-mix(in oklab, oklch(${elevatedL.toFixed(2)}% 0.012 var(--theme-runtime-hue)) ${(100 - elevatedTint).toFixed(1)}%, var(--theme-accent) ${elevatedTint.toFixed(1)}%)`)
    root.style.setProperty('--line', `color-mix(in oklab, transparent ${(100 - lineTint).toFixed(1)}%, var(--theme-accent) ${lineTint.toFixed(1)}%)`)
    root.style.setProperty('--line-strong', `color-mix(in oklab, transparent ${(100 - lineStrongTint).toFixed(1)}%, var(--theme-accent-strong) ${lineStrongTint.toFixed(1)}%)`)
    root.style.setProperty('--theme-selected-bg', `color-mix(in oklab, transparent ${(100 - selectedTint).toFixed(1)}%, var(--theme-accent) ${selectedTint.toFixed(1)}%)`)
    root.style.setProperty('--theme-hover-bg', `color-mix(in oklab, transparent ${(100 - hoverTint).toFixed(1)}%, var(--theme-accent) ${hoverTint.toFixed(1)}%)`)
    root.style.setProperty('--theme-scrollbar', `color-mix(in oklab, transparent 62%, var(--theme-accent) 38%)`)
    root.style.setProperty('--theme-control-bg', `color-mix(in oklab, var(--theme-card-bg) 84%, var(--theme-accent) 16%)`)
    root.style.setProperty('--theme-control-hover-bg', `color-mix(in oklab, var(--theme-card-bg) 76%, var(--theme-accent) 24%)`)
    root.style.setProperty('--theme-modal-bg', `color-mix(in oklab, var(--theme-card-bg) 84%, var(--launcher-page-bg) 16%)`)
    root.style.setProperty('--theme-modal-bg-strong', `color-mix(in oklab, var(--theme-elevated-bg) 58%, var(--theme-card-bg) 42%)`)
    root.style.setProperty('--theme-overlay-bg', `color-mix(in oklab, rgba(2, 5, 8, 0.78) 78%, var(--theme-accent-deep) 22%)`)
    root.style.setProperty('--theme-accent-surface', `color-mix(in oklab, transparent 84%, var(--theme-accent) 16%)`)
    root.style.setProperty('--theme-accent-surface-strong', `color-mix(in oklab, transparent 72%, var(--theme-accent) 28%)`)
    root.style.setProperty('--theme-glow-1', `color-mix(in oklab, transparent ${(78 - intensity01 * 18).toFixed(1)}%, var(--theme-accent) ${(22 + intensity01 * 18).toFixed(1)}%)`)
    root.style.setProperty('--theme-glow-2', `color-mix(in oklab, transparent ${(84 - intensity01 * 16).toFixed(1)}%, var(--theme-accent-strong) ${(16 + intensity01 * 16).toFixed(1)}%)`)
    root.style.setProperty('--theme-glow-3', `color-mix(in oklab, transparent ${(88 - intensity01 * 14).toFixed(1)}%, var(--theme-accent-deep) ${(12 + intensity01 * 14).toFixed(1)}%)`)
    root.style.setProperty('--text', '#c7ced3')
    root.style.setProperty('--text-strong', '#f3f4f2')
    root.style.setProperty('--muted', '#8a949d')
    root.style.setProperty('--subtle', '#68737c')
    root.style.setProperty('--launcher-content-radius', '18px')
    root.style.setProperty('--launcher-content-soft-line', 'rgba(142, 163, 179, 0.055)')

  root.style.setProperty('--bg', 'var(--launcher-chrome-bg)')
  root.style.setProperty('--surface', 'var(--theme-card-bg)')
  root.style.setProperty('--surface-strong', 'var(--theme-elevated-bg)')
  setCommonSemanticAliases(root)

  root.setAttribute('data-theme-dynamic', dynamicThemeEnabled ? 'true' : 'false')
  root.setAttribute('data-theme-motion', preferences.motionMode)
}
