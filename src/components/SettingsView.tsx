import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { useCallback, useState, useEffect, useRef } from 'react'
import { useLocale, type Locale } from '../context/locale'
import {
  ChevronDown, Bell,
  Clock3,
  Cloud,
  Download,
  FolderOpen,
  Gamepad2,
  Gauge,
  HardDrive,
  Info,
  MonitorCog,
  PanelTop,
  RefreshCcw,
  RotateCcw,
  Settings,
  ShieldCheck,
  Sparkles,
  CircleAlert,
  CircleHelp,
  TriangleAlert,
  Database,
  ExternalLink,
  KeyRound,
  Loader2,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { DEFAULT_LAUNCHER_PREFERENCES, type LauncherPreferences, type NotificationCategory } from '../lib/preferences'
import { launcherAccent } from '../lib/theme'
import type {
  HubcapKeyState,
  LauncherSettings,
  LuaSourceSettingsState,
  NativeCoreSettings,
  SteamEnvironmentInfo,
} from '../types'
import { ConfirmDialog } from './ConfirmDialog'

function CustomSelect<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (value: T) => void
  options: { value: T; label: string }[]
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const selectedLabel = options.find((o) => o.value === value)?.label ?? value

  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  return (
    <div className={`cs-wrap${open ? ' is-open' : ''}`} ref={ref}>
      <button type="button" className="cs-trigger" onClick={() => setOpen((v) => !v)}>
        <span>{selectedLabel}</span>
        <ChevronDown size={14} className="cs-chevron" />
      </button>
      <div className="cs-dropdown">
        <div className="cs-list">
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              className={`cs-option${opt.value === value ? ' is-selected' : ''}`}
              onClick={() => { onChange(opt.value); setOpen(false) }}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
}) {
  return (
    <button
      type="button"
      className={checked ? 'settings-toggle is-on' : 'settings-toggle'}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  )
}



function ThemeRange({
  label,
  value,
  min = 0,
  max = 100,
  step = 1,
  suffix = '%',
  onChange,
}: {
  label: string
  value: number
  min?: number
  max?: number
  step?: number
  suffix?: string
  onChange: (value: number) => void
}) {
  const safeValue = Math.min(max, Math.max(min, Number(value) || 0))
  const progress = ((safeValue - min) / Math.max(1, max - min)) * 100
  return (
    <label className="theme-range-row">
      <span className="theme-range-label">{label}<b>{Math.round(safeValue)}{suffix}</b></span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={safeValue}
        style={{ '--range-progress': `${progress}%` } as CSSProperties}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
    </label>
  )
}

const ACCENT_PRESETS = [
  { hue: 82, label: 'Amber' },
  { hue: 35, label: 'Coral' },
  { hue: 155, label: 'Mint' },
  { hue: 195, label: 'Teal' },
  { hue: 235, label: 'Blue' },
  { hue: 275, label: 'Violet' },
  { hue: 315, label: 'Rose' },
] as const

function AccentTonePicker({
  hue,
  chroma,
  themeIntensity,
  themeContrast,
  dynamicTheme,
  dynamicThemeSpeed,
  onHueChange,
  onChromaChange,
  onThemeIntensityChange,
  onThemeContrastChange,
  onDynamicThemeChange,
  onDynamicThemeSpeedChange,
  labels,
}: {
  hue: number
  chroma: number
  themeIntensity: number
  themeContrast: number
  dynamicTheme: boolean
  dynamicThemeSpeed: number
  onHueChange: (value: number) => void
  onChromaChange: (value: number) => void
  onThemeIntensityChange: (value: number) => void
  onThemeContrastChange: (value: number) => void
  onDynamicThemeChange: (value: boolean) => void
  onDynamicThemeSpeedChange: (value: number) => void
  labels: {
    hue: string
    saturation: string
    intensity: string
    contrast: string
    dynamic: string
    dynamicDesc: string
    speed: string
    default: string
    preview: string
  }
}) {
  const wheelRef = useRef<HTMLDivElement>(null)
  const safeHue = ((Number(hue) % 360) + 360) % 360
  const safeChroma = Math.min(100, Math.max(0, Number(chroma) || 0))
  const accent = launcherAccent({ accentHue: safeHue, accentChroma: safeChroma })
  const radians = (safeHue * Math.PI) / 180
  const radius = safeChroma / 100
  const thumbX = 50 + Math.cos(radians) * radius * 47
  const thumbY = 50 + Math.sin(radians) * radius * 47

  const updateFromPoint = useCallback((clientX: number, clientY: number) => {
    const wheel = wheelRef.current
    if (!wheel) return
    const rect = wheel.getBoundingClientRect()
    const cx = rect.left + rect.width / 2
    const cy = rect.top + rect.height / 2
    const dx = clientX - cx
    const dy = clientY - cy
    const maxRadius = Math.max(1, Math.min(rect.width, rect.height) / 2)
    const normalizedRadius = Math.min(1, Math.hypot(dx, dy) / maxRadius)
    const nextHue = (Math.atan2(dy, dx) * 180 / Math.PI + 360) % 360
    onHueChange(Math.round(nextHue * 10) / 10)
    onChromaChange(Math.round(normalizedRadius * 1000) / 10)
  }, [onHueChange, onChromaChange])

  const onPointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId)
    updateFromPoint(event.clientX, event.clientY)
  }, [updateFromPoint])

  const onPointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return
    updateFromPoint(event.clientX, event.clientY)
  }, [updateFromPoint])

  const onPointerUp = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
  }, [])

  const onWheelKeyDown = useCallback((event: ReactKeyboardEvent<HTMLDivElement>) => {
    const hueStep = event.shiftKey ? 10 : 2
    const chromaStep = event.shiftKey ? 5 : 2
    if (event.key === 'ArrowLeft') {
      event.preventDefault()
      onHueChange((safeHue - hueStep + 360) % 360)
    } else if (event.key === 'ArrowRight') {
      event.preventDefault()
      onHueChange((safeHue + hueStep) % 360)
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      onChromaChange(Math.min(100, safeChroma + chromaStep))
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      onChromaChange(Math.max(0, safeChroma - chromaStep))
    } else if (event.key === 'Home') {
      event.preventDefault()
      onChromaChange(0)
    } else if (event.key === 'End') {
      event.preventDefault()
      onChromaChange(100)
    }
  }, [onChromaChange, onHueChange, safeChroma, safeHue])

  return (
    <div
      className="accent-tone-picker color-studio"
      style={{
        '--accent-preview': accent.base,
        '--wheel-x': `${thumbX}%`,
        '--wheel-y': `${thumbY}%`,
      } as CSSProperties}
    >
      <div className="accent-tone-preview">
        <span aria-hidden="true" />
        <div>
          <strong>0xoLemon</strong>
          <small>{labels.preview}</small>
        </div>
        <code className="accent-color-hex">{accent.hex}</code>
      </div>

      <div className="accent-color-wheel-wrap">
        <div
          ref={wheelRef}
          className="accent-color-wheel"
          role="slider"
          tabIndex={0}
          aria-label="Launcher color wheel"
          aria-valuemin={0}
          aria-valuemax={360}
          aria-valuenow={Math.round(safeHue)}
          aria-valuetext={`${Math.round(safeHue)} degrees, ${Math.round(safeChroma)} percent saturation`}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onKeyDown={onWheelKeyDown}
        >
          <span className="accent-color-wheel-thumb" aria-hidden="true" />
        </div>
        <div className="accent-color-wheel-readout">
          <span>{labels.hue} <b>{Math.round(safeHue)}°</b></span>
          <span>{labels.saturation} <b>{Math.round(safeChroma)}%</b></span>
        </div>
      </div>

      <div className="accent-tone-presets" aria-label="Accent presets">
        {ACCENT_PRESETS.map((preset) => (
          <button
            key={preset.hue}
            type="button"
            className={Math.abs(safeHue - preset.hue) < 2 ? 'is-active' : ''}
            style={{ '--swatch-hue': preset.hue } as CSSProperties}
            aria-label={preset.label}
            title={preset.label}
            onClick={() => onHueChange(preset.hue)}
          />
        ))}
      </div>

      <div className="theme-range-grid">
        <ThemeRange label={labels.hue} value={safeHue} max={360} suffix="°" onChange={onHueChange} />
        <ThemeRange label={labels.saturation} value={safeChroma} onChange={onChromaChange} />
        <ThemeRange label={labels.intensity} value={themeIntensity} onChange={onThemeIntensityChange} />
        <ThemeRange label={labels.contrast} value={themeContrast} onChange={onThemeContrastChange} />
      </div>

      <div className="dynamic-theme-panel">
        <div>
          <strong>{labels.dynamic}</strong>
          <span>{labels.dynamicDesc}</span>
        </div>
        <Toggle checked={dynamicTheme} onChange={onDynamicThemeChange} label={labels.dynamic} />
      </div>

      {dynamicTheme && (
        <div className="dynamic-theme-speed">
          <ThemeRange label={labels.speed} value={dynamicThemeSpeed} onChange={onDynamicThemeSpeedChange} />
        </div>
      )}

      <button
        type="button"
        className="accent-tone-reset"
        onClick={() => {
          onHueChange(DEFAULT_LAUNCHER_PREFERENCES.accentHue)
          onChromaChange(DEFAULT_LAUNCHER_PREFERENCES.accentChroma)
          onThemeIntensityChange(DEFAULT_LAUNCHER_PREFERENCES.themeIntensity)
          onThemeContrastChange(DEFAULT_LAUNCHER_PREFERENCES.themeContrast)
          onDynamicThemeChange(DEFAULT_LAUNCHER_PREFERENCES.dynamicTheme)
          onDynamicThemeSpeedChange(DEFAULT_LAUNCHER_PREFERENCES.dynamicThemeSpeed)
        }}
      >
        <RotateCcw size={13} /> {labels.default || 'Default'}
      </button>
    </div>
  )
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-copy">
        <div className="settings-row-title-line">
          <strong>{title}</strong>
        </div>
        <span>{description}</span>
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  )
}

export function LuaGameModeToggle({
  steamEnvironment,
  onEnabledChange,
}: {
  steamEnvironment: SteamEnvironmentInfo | null
  onEnabledChange: (enabled: boolean) => void
}) {
  const { t } = useLocale()
  const [enabled, setEnabled] = useState(false)
  const [loading, setLoading] = useState(false)
  const [showEnableConfirm, setShowEnableConfirm] = useState(false)
  // null = unknown/checking, true = defender ON (bad), false = defender OFF (good)
  const [defenderOn, setDefenderOn] = useState<boolean | null>(null)
  const [defenderChecked, setDefenderChecked] = useState(false)

  const checkDefender = useCallback(async () => {
    setDefenderChecked(false)
    try {
      const status = await invoke<boolean | null>('check_defender_realtime_status')
      setDefenderOn(status ?? null)
    } catch {
      setDefenderOn(null)
    }
    setDefenderChecked(true)
  }, [])

  const checkStatus = useCallback(async () => {
    try {
      const isEnabled = await invoke<boolean>('is_lua_game_mode_enabled')
      setEnabled(isEnabled)
      onEnabledChange(isEnabled)
      if (isEnabled) {
        await checkDefender()
      }
    } catch (e) {
      console.error('Failed to check lua-game mode status', e)
    }
  }, [checkDefender, onEnabledChange])

  useEffect(() => {
    const timer = window.setTimeout(() => void checkStatus(), 0)
    return () => window.clearTimeout(timer)
  }, [checkStatus])

  const showToast = (title: string, msg: string, severity: 'success' | 'error' | 'info' | 'warning' = 'info') => {
    window.dispatchEvent(new CustomEvent('0xo-toast', {
      detail: {
        category: 'launcher',
        severity,
        title,
        message: msg,
        dedupeKey: 'lua-game-mode',
      }
    }))
  }

  const handleToggle = async (checked: boolean) => {
    if (checked) {
      // Show warning before enabling
      setShowEnableConfirm(true)
      return
    }

    // Disable without confirmation
    void performDisable()
  }

  const performEnable = async () => {
    setShowEnableConfirm(false)
    setLoading(true)
    try {
      showToast(t.settings.luaGameMode, t.settings.luaGameModeInstalling, 'info')
      await invoke('enable_lua_game_mode')
      const actualState = await invoke<boolean>('is_lua_game_mode_enabled')
      if (!actualState) {
        throw new Error('Steam hooks were copied but did not pass the installed-state check')
      }
      setEnabled(actualState)
      onEnabledChange(actualState)
      showToast(t.settings.luaGameMode, t.settings.luaGameModeInstallSuccess, 'success')
      void checkDefender()
    } catch (e) {
      console.error(e)
      showToast(t.settings.luaGameMode, t.settings.luaGameModeInstallError + ': ' + String(e), 'error')
    } finally {
      setLoading(false)
    }
  }

  const performDisable = async () => {
    setLoading(true)
    try {
      showToast(t.settings.luaGameMode, t.settings.luaGameModeUninstalling, 'info')
      await invoke('disable_lua_game_mode')
      const actualState = await invoke<boolean>('is_lua_game_mode_enabled')
      setEnabled(actualState)
      onEnabledChange(actualState)
      if (actualState) {
        throw new Error('Steam hooks are still present after the removal attempt')
      }
      setDefenderOn(null)
      setDefenderChecked(false)
      showToast(t.settings.luaGameMode, t.settings.luaGameModeUninstallSuccess, 'success')
    } catch (e) {
      const errorMsg = String(e)
      console.error(e)

      // Check if error is about Steam running
      if (errorMsg.toLowerCase().includes('steam is still running')) {
        showToast(
          t.settings.luaGameMode,
          errorMsg,
          'warning'
        )
      } else {
        showToast(t.settings.luaGameMode, t.settings.luaGameModeUninstallError + ': ' + errorMsg, 'error')
      }
    } finally {
      setLoading(false)
    }
  }

  if (!steamEnvironment?.installed) return null

  return (
    <div style={{
      marginTop: '16px',
      padding: '16px',
      background: 'rgba(255,215,0,0.05)',
      border: '1px solid rgba(255,215,0,0.2)',
      borderRadius: '8px'
    }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: '12px', marginBottom: '12px' }}>
        <Sparkles size={18} style={{ color: '#ffd700', marginTop: '2px' }} />
        <div style={{ flex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '8px' }}>
            <strong style={{ color: '#ffd700' }}>{t.settings.luaGameMode}</strong>
            <button
                type="button"
                className={enabled ? 'settings-toggle is-on' : 'settings-toggle'}
                role="switch"
                aria-checked={enabled}
                disabled={loading}
                style={{ opacity: loading ? 0.5 : 1, cursor: loading ? 'not-allowed' : 'pointer' }}
                onClick={() => !loading && handleToggle(!enabled)}
              >
                <span />
              </button>
          </div>
          <p style={{ fontSize: '13px', color: '#aaa', lineHeight: '1.5', marginBottom: '8px' }}>
            {t.settings.luaGameModeDesc}
          </p>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            padding: '8px 12px',
            background: 'rgba(255,193,7,0.1)',
            border: '1px solid rgba(255,193,7,0.3)',
            borderRadius: '6px'
          }}>
            <TriangleAlert size={14} style={{ color: '#ffc107' }} />
            <span style={{ fontSize: '12px', color: '#ffc107', lineHeight: '1.4' }}>
              {t.settings.luaGameModeWarning}
            </span>
          </div>

          {/* Windows Defender status — only show when enabled */}
          {enabled && defenderChecked && (
            <div style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: '8px',
              marginTop: '8px',
              padding: '8px 12px',
              background: defenderOn === true
                ? 'rgba(220,53,69,0.12)'
                : defenderOn === false
                  ? 'rgba(40,167,69,0.12)'
                  : 'rgba(108,117,125,0.12)',
              border: `1px solid ${defenderOn === true ? 'rgba(220,53,69,0.4)' : defenderOn === false ? 'rgba(40,167,69,0.4)' : 'rgba(108,117,125,0.3)'}`,
              borderRadius: '6px',
            }}>
              <span style={{ fontSize: '13px', lineHeight: '1.5', color: defenderOn === true ? '#ff6b6b' : defenderOn === false ? '#51cf66' : '#aaa' }}>
                {defenderOn === true
                  ? t.settings.defenderRealtimeOn
                  : defenderOn === false
                    ? t.settings.defenderRealtimeOff
                    : t.settings.defenderCheckFailed}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Enable Lua-Game Mode Confirmation */}
      {showEnableConfirm && (
        <ConfirmDialog
          title={t.settings.luaGameMode}
          message={t.settings.luaGameModeWarning}
          confirmText="Enable"
          cancelText="Cancel"
          variant="warning"
          onConfirm={performEnable}
          onCancel={() => setShowEnableConfirm(false)}
        />
      )}
    </div>
  )
}

function SteamAutoInstallSettings() {
  const { t } = useLocale()
  const [autoInstall, setAutoInstall] = useState(() => localStorage.getItem('steamAutoInstall') === 'true')
  const [skipConfirm, setSkipConfirm] = useState(() => localStorage.getItem('steamSkipRestartConfirm') === 'true')

  return (
    <>
      <SettingRow
        title={t.library.autoInstallAfterRestart}
        description={t.library.autoInstallSettingDesc}
      >
        <Toggle
          checked={autoInstall}
          onChange={(val) => {
            setAutoInstall(val)
            localStorage.setItem('steamAutoInstall', String(val))
          }}
          label="Auto Install"
        />
      </SettingRow>
      <SettingRow
        title={t.library.skipRestartConfirmSetting}
        description={t.library.skipRestartConfirmSettingDesc}
      >
        <Toggle
          checked={skipConfirm}
          onChange={(val) => {
            setSkipConfirm(val)
            localStorage.setItem('steamSkipRestartConfirm', String(val))
          }}
          label="Skip Confirm"
        />
      </SettingRow>
    </>
  )
}

function LuaSourcesSettings() {
  const { t } = useLocale()
  const keyInputRef = useRef<HTMLInputElement>(null)
  const ryuuInputRef = useRef<HTMLInputElement>(null)
  const depotboxInputRef = useRef<HTMLInputElement>(null)
  const manifesthubInputRef = useRef<HTMLInputElement>(null)
  const [sources, setSources] = useState<LuaSourceSettingsState | null>(null)
  const [nativeCore, setNativeCore] = useState<NativeCoreSettings | null>(null)
  const [busy, setBusy] = useState<string | null>('load')
  const [hubcapMessage, setHubcapMessage] = useState<string | null>(null)
  const [ryuuMessage, setRyuuMessage] = useState<string | null>(null)
  const [depotboxMessage, setDepotboxMessage] = useState<string | null>(null)
  const [manifesthubMessage, setManifesthubMessage] = useState<string | null>(null)
  const [preferenceMessage, setPreferenceMessage] = useState<string | null>(null)

  const reload = useCallback(async () => {
    setBusy('load')
    try {
      const [sourceState, coreState] = await Promise.all([
        invoke<LuaSourceSettingsState>('get_lua_source_settings'),
        invoke<NativeCoreSettings>('get_native_core_settings'),
      ])
      setSources(sourceState)
      setNativeCore(coreState)
      setHubcapMessage(null)
      setRyuuMessage(null)
      setDepotboxMessage(null)
      setManifesthubMessage(null)
    } catch (error) {
      setPreferenceMessage(String(error))
    } finally {
      setBusy(null)
    }
  }, [])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void reload()
    }, 0)
    return () => window.clearTimeout(timer)
  }, [reload])

  const saveKey = async () => {
    const rawKey = keyInputRef.current?.value.trim() || ''
    if (!rawKey) {
      setHubcapMessage(t.settings.hubcapKeyRequired)
      return
    }
    setBusy('save-key')
    try {
      const hubcap = await invoke<HubcapKeyState>('save_hubcap_api_key', { apiKey: rawKey })
      if (keyInputRef.current) keyInputRef.current.value = ''
      setSources((current) => current ? { ...current, hubcap } : current)
      setHubcapMessage(t.settings.hubcapKeySaved)
    } catch (error) {
      setHubcapMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const testKey = async () => {
    setBusy('test-key')
    try {
      const hubcap = await invoke<HubcapKeyState>('refresh_hubcap_key_state')
      setSources((current) => current ? { ...current, hubcap } : current)
      setHubcapMessage(hubcap.valid ? t.settings.hubcapKeyValid : hubcap.lastError || t.settings.hubcapKeyInvalid)
    } catch (error) {
      setHubcapMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const clearKey = async () => {
    setBusy('clear-key')
    try {
      await invoke('clear_hubcap_api_key')
      if (keyInputRef.current) keyInputRef.current.value = ''
      await reload()
      setHubcapMessage(t.settings.hubcapKeyCleared)
    } catch (error) {
      setHubcapMessage(String(error))
      setBusy(null)
    }
  }

  const saveRyuuKey = async () => {
    const rawKey = ryuuInputRef.current?.value.trim() || ''
    if (!rawKey) {
      setRyuuMessage(t.settings.ryuuKeyPlaceholder)
      return
    }
    setBusy('save-ryuu-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('save_ryuu_auth_key', { apiKey: rawKey })
      if (ryuuInputRef.current) ryuuInputRef.current.value = ''
      setSources(next)
      setRyuuMessage(t.settings.ryuuKeySaved)
    } catch (error) {
      setRyuuMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const clearRyuuKey = async () => {
    setBusy('clear-ryuu-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('clear_ryuu_auth_key')
      if (ryuuInputRef.current) ryuuInputRef.current.value = ''
      setSources(next)
      setRyuuMessage(t.settings.ryuuKeyCleared)
    } catch (error) {
      setRyuuMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const saveDepotboxKey = async () => {
    const rawKey = depotboxInputRef.current?.value.trim() || ''
    if (!rawKey) {
      setDepotboxMessage(t.settings.depotboxKeyRequired)
      return
    }
    setBusy('save-depotbox-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('save_depotbox_api_key', { apiKey: rawKey })
      if (depotboxInputRef.current) depotboxInputRef.current.value = ''
      setSources(next)
      setDepotboxMessage(t.settings.depotboxKeySaved)
    } catch (error) {
      setDepotboxMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const clearDepotboxKey = async () => {
    setBusy('clear-depotbox-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('clear_depotbox_api_key')
      if (depotboxInputRef.current) depotboxInputRef.current.value = ''
      setSources(next)
      setDepotboxMessage(t.settings.depotboxKeyCleared)
    } catch (error) {
      setDepotboxMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const saveManifestHubKey = async () => {
    const rawKey = manifesthubInputRef.current?.value.trim() ?? ''
    if (!rawKey) {
      setManifesthubMessage(t.settings.manifesthubPlaceholder)
      return
    }
    setBusy('save-manifesthub-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('save_manifesthub_api_key', { apiKey: rawKey })
      if (manifesthubInputRef.current) manifesthubInputRef.current.value = ''
      setSources(next)
      setManifesthubMessage(t.settings.manifesthubSuccess)
    } catch (error) {
      setManifesthubMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const testManifestHubKey = async () => {
    setBusy('test-manifesthub-key')
    try {
      const ok = await invoke<boolean>('test_manifesthub_api_key')
      setManifesthubMessage(ok ? t.settings.manifesthubSuccess : t.settings.manifesthubFailed)
    } catch (error) {
      setManifesthubMessage(t.settings.manifesthubFailed)
    } finally {
      setBusy(null)
    }
  }

  const clearManifestHubKey = async () => {
    setBusy('clear-manifesthub-key')
    try {
      const next = await invoke<LuaSourceSettingsState>('clear_manifesthub_api_key')
      if (manifesthubInputRef.current) manifesthubInputRef.current.value = ''
      setSources(next)
      setManifesthubMessage(t.settings.hubcapKeyCleared)
    } catch (error) {
      setManifesthubMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const setSourcePreference = async (
    key: 'sushiEnabled' | 'githubMirrorsEnabled' | 'openluaEnabled' | 'steamtoolsEnabled' | 'ryuuEnabled' | 'luieEnabled' | 'twentyTwoCloudEnabled' | 'skyflareEnabled',
    value: boolean
  ) => {
    if (!sources) return
    const request = { ...sources, [key]: value }
    setBusy(key)
    try {
      const next = await invoke<LuaSourceSettingsState>('set_lua_source_preferences', {
        request: {
          sushiEnabled: request.sushiEnabled,
          githubMirrorsEnabled: request.githubMirrorsEnabled,
          openluaEnabled: request.openluaEnabled,
          steamtoolsEnabled: request.steamtoolsEnabled,
          ryuuEnabled: request.ryuuEnabled,
          luieEnabled: request.luieEnabled,
          twentyTwoCloudEnabled: request.twentyTwoCloudEnabled,
          skyflareEnabled: request.skyflareEnabled,
        },
      })
      setSources(next)
      setPreferenceMessage(null)
    } catch (error) {
      setPreferenceMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const setStatsLookup = async (enabled: boolean) => {
    setBusy('stats')
    try {
      setNativeCore(await invoke<NativeCoreSettings>('set_native_core_stats_api', { enabled }))
      setPreferenceMessage(null)
    } catch (error) {
      setPreferenceMessage(String(error))
    } finally {
      setBusy(null)
    }
  }

  const hubcap = sources?.hubcap
  return (
    <section className="settings-group" id="lua-sources">
      <header>
        <KeyRound size={18} />
        <div>
          <strong>{t.settings.luaSources}</strong>
          <span>{t.settings.luaSourcesDesc}</span>
        </div>
      </header>
      <div className="settings-group-body">
        <div className="lua-source-key-panel">
          <div className="lua-source-key-heading">
            <div>
              <strong>{t.settings.hubcapApiKey}</strong>
              <span>{t.settings.hubcapApiKeyDesc}</span>
            </div>
            <span className={`settings-status-pill ${hubcap?.valid ? 'is-online' : ''}`}>
              {busy === 'load'
                ? t.settings.steamChecking
                : hubcap?.configured
                  ? hubcap.maskedKey || t.settings.hubcapConfigured
                  : t.settings.hubcapNotConfigured}
            </span>
          </div>
          <div className="lua-source-key-input">
            <input
              ref={keyInputRef}
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={t.settings.hubcapKeyPlaceholder}
              aria-label={t.settings.hubcapApiKey}
            />
            <button type="button" onClick={() => void saveKey()} disabled={Boolean(busy)}>
              {busy === 'save-key' ? <Loader2 size={14} className="spin" /> : null}
              {t.settings.saveKey}
            </button>
          </div>
          <div className="settings-action-row">
            <button type="button" className="settings-secondary-button" onClick={() => void testKey()} disabled={!hubcap?.configured || Boolean(busy)}>
              <RefreshCcw size={14} />
              {t.settings.testKey}
            </button>
            <button type="button" className="settings-secondary-button" onClick={() => void clearKey()} disabled={!hubcap?.configured || Boolean(busy)}>
              {t.settings.clearKey}
            </button>
            <button type="button" className="settings-secondary-button" onClick={() => void openUrl('https://hubcapmanifest.com/')}>
              <ExternalLink size={14} />
              {t.settings.openHubcap}
            </button>
          </div>
          {hubcap?.configured && (
            <div className="lua-source-usage-grid">
              <span>{t.settings.dailyHubcapQuota}<strong>{hubcap.daily.remaining ?? '-'}/{hubcap.daily.limit ?? '-'}</strong></span>
              <span>{t.settings.singleManifestQuota}<strong>{hubcap.single.remaining ?? '-'}/{hubcap.single.limit ?? '-'}</strong></span>
              <span>{t.settings.bundleQuota}<strong>{hubcap.bundle.remaining ?? '-'}/{hubcap.bundle.limit ?? '-'}</strong></span>
              <span>{t.settings.workshopQuota}<strong>{hubcap.workshop.remaining ?? '-'}/{hubcap.workshop.limit ?? '-'}</strong></span>
              <span>{t.settings.serviceStatus}<strong>{hubcap.serviceReady ? t.settings.ready : t.settings.unavailable}</strong></span>
              <span>{t.settings.keyExpiry}<strong>{hubcap.expiresAt ? new Date(hubcap.expiresAt).toLocaleString() : t.settings.unknown}</strong></span>
            </div>
          )}
          <div className="lua-source-security">
            <ShieldCheck size={16} />
            <div>
              <strong>{t.settings.securityBestPractices}</strong>
              <span>{t.settings.securityNeverShare}</span>
              <span>{t.settings.securityStoreSecurely}</span>
              <span>{t.settings.securityExpiry}</span>
              <span>{t.settings.securityRevoke}</span>
            </div>
          </div>
          {hubcapMessage && <div className="lua-source-message">{hubcapMessage}</div>}
        </div>

        <div className="lua-source-key-panel" style={{ marginTop: '12px' }}>
          <div className="lua-source-key-heading">
            <div>
              <strong>{t.settings.ryuuApiKey}</strong>
              <span>{t.settings.ryuuApiKeyDesc}</span>
            </div>
            <span className={`settings-status-pill ${sources?.ryuuConfigured ? 'is-online' : ''}`}>
              {sources?.ryuuConfigured ? sources.ryuuKey || t.settings.hubcapConfigured : t.settings.hubcapNotConfigured}
            </span>
          </div>
          <div className="lua-source-key-input">
            <input
              ref={ryuuInputRef}
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={t.settings.ryuuKeyPlaceholder}
              aria-label={t.settings.ryuuApiKey}
            />
            <button type="button" onClick={() => void saveRyuuKey()} disabled={Boolean(busy)}>
              {busy === 'save-ryuu-key' ? <Loader2 size={14} className="spin" /> : null}
              {t.settings.saveKey}
            </button>
          </div>
          <div className="settings-action-row">
            <button type="button" className="settings-secondary-button" onClick={() => void clearRyuuKey()} disabled={!sources?.ryuuConfigured || Boolean(busy)}>
              {t.settings.clearKey}
            </button>
            <button type="button" className="settings-secondary-button" onClick={() => void openUrl('https://generator.ryuu.lol/api')}>
              <ExternalLink size={14} />
              {t.settings.openRyuuApi}
            </button>
          </div>
          {ryuuMessage && <div className="lua-source-message">{ryuuMessage}</div>}
        </div>

        <div className="lua-source-key-panel" style={{ marginTop: '12px' }}>
          <div className="lua-source-key-heading">
            <div>
              <strong>{t.settings.depotboxApiKey}</strong>
              <span>{t.settings.depotboxApiKeyDesc}</span>
            </div>
            <span className={`settings-status-pill ${sources?.depotboxConfigured ? 'is-online' : ''}`}>
              {sources?.depotboxConfigured
                ? `${t.settings.depotboxDirectApi}: ${sources.depotboxKey || t.settings.hubcapConfigured}`
                : t.settings.depotboxFreeWeb}
            </span>
          </div>
          <div className="lua-source-key-input">
            <input
              ref={depotboxInputRef}
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={t.settings.depotboxKeyPlaceholder}
              aria-label={t.settings.depotboxApiKey}
            />
            <button type="button" onClick={() => void saveDepotboxKey()} disabled={Boolean(busy)}>
              {busy === 'save-depotbox-key' ? <Loader2 size={14} className="spin" /> : null}
              {t.settings.saveKey}
            </button>
          </div>
          <div className="settings-action-row">
            <button type="button" className="settings-secondary-button" onClick={() => void clearDepotboxKey()} disabled={!sources?.depotboxConfigured || Boolean(busy)}>
              {t.settings.clearKey}
            </button>
            <button type="button" className="settings-secondary-button" onClick={() => void openUrl('https://depotbox.org/pricing')}>
              <ExternalLink size={14} />
              {t.settings.depotboxPricing}
            </button>
            <button type="button" className="settings-secondary-button" onClick={() => void openUrl('https://depotbox.org/api-docs')}>
              <ExternalLink size={14} />
              {t.settings.depotboxApiDocs}
            </button>
          </div>
          <div className="lua-source-message">
            {sources?.depotboxConfigured ? t.settings.depotboxDirectApiHelp : t.settings.depotboxFreeWebHelp}
          </div>
          {depotboxMessage && <div className="lua-source-message">{depotboxMessage}</div>}
        </div>

        <div className="lua-source-key-panel" style={{ marginTop: '12px' }}>
          <div className="lua-source-key-heading">
            <div>
              <strong>{t.settings.manifesthubTitle}</strong>
              <span>{t.settings.manifesthubDesc}</span>
            </div>
            <span className={`settings-status-pill ${sources?.manifesthubConfigured ? 'is-online' : ''}`}>
              {sources?.manifesthubConfigured
                ? sources.manifesthubKey || t.settings.manifesthubConfigured
                : t.settings.manifesthubNotConfigured}
            </span>
          </div>
          <div className="lua-source-key-input">
            <input
              ref={manifesthubInputRef}
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={t.settings.manifesthubPlaceholder}
              aria-label={t.settings.manifesthubTitle}
            />
            <button type="button" onClick={() => void saveManifestHubKey()} disabled={Boolean(busy)}>
              {busy === 'save-manifesthub-key' ? <Loader2 size={14} className="spin" /> : null}
              {t.settings.manifesthubSave}
            </button>
          </div>
          <div className="settings-action-row">
            <button
              type="button"
              className="settings-secondary-button"
              onClick={() => void testManifestHubKey()}
              disabled={!sources?.manifesthubConfigured || Boolean(busy)}
            >
              <RefreshCcw size={14} />
              {t.settings.manifesthubTest}
            </button>
            <button
              type="button"
              className="settings-secondary-button"
              onClick={() => void clearManifestHubKey()}
              disabled={!sources?.manifesthubConfigured || Boolean(busy)}
            >
              {t.settings.manifesthubClear}
            </button>
            <button
              type="button"
              className="settings-secondary-button"
              onClick={() => void openUrl('https://manifesthub2.filegear-sg.me/')}
            >
              <ExternalLink size={14} />
              {t.settings.manifesthubGetKey}
            </button>
          </div>
          {manifesthubMessage && <div className="lua-source-message">{manifesthubMessage}</div>}
        </div>

        <SettingRow title={t.settings.statsLookup} description={t.settings.statsLookupDesc}>
          <Toggle
            checked={nativeCore?.statsApiEnabled ?? true}
            onChange={(checked) => void setStatsLookup(checked)}
            label={t.settings.statsLookup}
          />
        </SettingRow>
        <SettingRow title={t.settings.sushiFallback} description={t.settings.sushiFallbackDesc}>
          <Toggle
            checked={sources?.sushiEnabled ?? true}
            onChange={(checked) => void setSourcePreference('sushiEnabled', checked)}
            label={t.settings.sushiFallback}
          />
        </SettingRow>
        <SettingRow title={t.settings.githubMirrorsFallback} description={t.settings.githubMirrorsFallbackDesc}>
          <Toggle
            checked={sources?.githubMirrorsEnabled ?? true}
            onChange={(checked) => void setSourcePreference('githubMirrorsEnabled', checked)}
            label={t.settings.githubMirrorsFallback}
          />
        </SettingRow>
        <SettingRow title={t.settings.openluaFallback} description={t.settings.openluaFallbackDesc}>
          <Toggle
            checked={sources?.openluaEnabled ?? true}
            onChange={(checked) => void setSourcePreference('openluaEnabled', checked)}
            label={t.settings.openluaFallback}
          />
        </SettingRow>
        <SettingRow title={t.settings.steamtoolsFallback} description={t.settings.steamtoolsFallbackDesc}>
          <Toggle
            checked={sources?.steamtoolsEnabled ?? true}
            onChange={(checked) => void setSourcePreference('steamtoolsEnabled', checked)}
            label={t.settings.steamtoolsFallback}
          />
        </SettingRow>
        <SettingRow title={t.settings.ryuuFallback} description={t.settings.ryuuFallbackDesc}>
          <Toggle
            checked={sources?.ryuuEnabled ?? false}
            onChange={(checked) => void setSourcePreference('ryuuEnabled', checked)}
            label={t.settings.ryuuFallback}
          />
        </SettingRow>
        <SettingRow title={t.settings.luieSource} description={t.settings.luieSourceDesc}>
          <Toggle
            checked={sources?.luieEnabled ?? true}
            onChange={(checked) => void setSourcePreference('luieEnabled', checked)}
            label={t.settings.luieSource}
          />
        </SettingRow>
        <SettingRow title={t.settings.twentyTwoCloudSource} description={t.settings.twentyTwoCloudSourceDesc}>
          <Toggle
            checked={sources?.twentyTwoCloudEnabled ?? true}
            onChange={(checked) => void setSourcePreference('twentyTwoCloudEnabled', checked)}
            label={t.settings.twentyTwoCloudSource}
          />
        </SettingRow>
        <SettingRow title={t.settings.skyflareSource} description={t.settings.skyflareSourceDesc}>
          <Toggle
            checked={sources?.skyflareEnabled ?? true}
            onChange={(checked) => void setSourcePreference('skyflareEnabled', checked)}
            label={t.settings.skyflareSource}
          />
        </SettingRow>
        <div className="lua-source-risk-note">
          <Database size={15} />
          <span>{t.settings.communitySourceWarning}</span>
        </div>
        {preferenceMessage && <div className="lua-source-message">{preferenceMessage}</div>}
      </div>
    </section>
  )
}

export function SettingsView({
  preferences,
  launcherSettings,
  onChange,
  onLauncherSettingChange,
  onChooseLibrary,
  onOpenLibrary,
  onOpenCache,
  onCheckForUpdates,
  onLuaGameModeChange,
  steamEnvironment,
  steamStatus,
  onRefreshSteam,
  onOpenSteam,
  onRestartSteam,
  onOpenBigPicture,
  onReset,
  onResetOnboarding,
  onOpenHelpCenter,
  onManageNotifications,
  appVersion,
  updateStatus,
}: {
  preferences: LauncherPreferences
  launcherSettings: LauncherSettings
  onChange: <K extends keyof LauncherPreferences>(key: K, value: LauncherPreferences[K]) => void
  onLauncherSettingChange: <K extends keyof LauncherSettings>(key: K, value: LauncherSettings[K]) => void
  onChooseLibrary: () => void
  onOpenLibrary: () => void
  onOpenCache: () => void
  onChooseCloudRoot: () => void
  onOpenCloudRoot: () => void
  onCheckForUpdates: () => void
  onLuaGameModeChange: (enabled: boolean) => void
  steamEnvironment: SteamEnvironmentInfo | null
  steamStatus: string | null
  onRefreshSteam: () => void
  onOpenSteam: () => void
  onRestartSteam: () => void
  onOpenBigPicture: () => void
  onReset: () => void
  onResetOnboarding: () => void
  onOpenHelpCenter: () => void
  onManageNotifications: () => void
  appVersion: string
  updateStatus: string | null
}) {
  const { locale, setLocale, t } = useLocale()
  return (
    <section className="settings-view settings-view-global">
      <header className="settings-page-header">
        <div>
          <span className="settings-page-icon">
            <Settings size={21} />
          </span>
          <div>
            <h1>{t.settings.title}</h1>
            <p>{t.settings.subtitle}</p>
          </div>
        </div>
        <button type="button" className="settings-reset" onClick={onReset}>
          <RotateCcw size={15} />
          {t.settings.restoreDefaults}
        </button>
      </header>

      <div className="settings-sections">
        <section className="settings-group">
          <header>
            <MonitorCog size={18} />
            <div>
              <strong>{t.settings.general}</strong>
              <span>{t.settings.generalDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.openLauncherOn} description={t.settings.openLauncherOnDesc}>
              <CustomSelect
                value={preferences.startupPage}
                onChange={(v) => onChange('startupPage', v)}
                options={[
                  { value: 'Home', label: t.nav.home },
                  { value: 'Store', label: t.nav.store },
                  { value: 'Library', label: t.nav.library },
                  { value: 'Updates', label: t.nav.updates },
                  { value: 'Downloads', label: t.nav.downloads },
                  { value: 'CloudRedirect', label: t.nav.cloudRedirect },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.closeButton} description={t.settings.closeButtonDesc}>
              <CustomSelect
                value={preferences.closeBehavior}
                onChange={(v) => onChange('closeBehavior', v)}
                options={[
                  { value: 'exit', label: t.settings.closeExit },
                  { value: 'minimize', label: t.settings.closeMinimize },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmUninstall} description={t.settings.confirmUninstallDesc}>
              <Toggle
                checked={preferences.confirmBeforeUninstall}
                onChange={(checked) => onChange('confirmBeforeUninstall', checked)}
                label={t.settings.confirmUninstall}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmCancelCleanup} description={t.settings.confirmCancelCleanupDesc}>
              <Toggle
                checked={preferences.confirmBeforeCancelCleanup}
                onChange={(checked) => onChange('confirmBeforeCancelCleanup', checked)}
                label={t.settings.confirmCancelCleanup}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmClearCache} description={t.settings.confirmClearCacheDesc}>
              <Toggle
                checked={preferences.confirmBeforeClearCache}
                onChange={(checked) => onChange('confirmBeforeClearCache', checked)}
                label={t.settings.confirmClearCache}
              />
            </SettingRow>
            <SettingRow title={t.settings.confirmCloudRestore} description={t.settings.confirmCloudRestoreDesc}>
              <Toggle
                checked={preferences.confirmBeforeCloudRestore}
                onChange={(checked) => onChange('confirmBeforeCloudRestore', checked)}
                label={t.settings.confirmCloudRestore}
              />
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <PanelTop size={18} />
            <div>
              <strong>{t.settings.homeLayout}</strong>
              <span>{t.settings.homeLayoutDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showContinuePlaying', t.settings.showContinuePlaying, t.settings.showContinuePlayingDesc],
              ['showRecentGames', t.settings.showRecentGames, t.settings.showRecentGamesDesc],
              ['showActiveTasks', t.settings.showActiveTasks, t.settings.showActiveTasksDesc],
              ['showDiscordCard', t.settings.showDiscordCard, t.settings.showDiscordCardDesc],
              ['showDonateCard', t.settings.showDonateCard, t.settings.showDonateCardDesc],
              ['carouselAutoplay', t.settings.carouselAutoplay, t.settings.carouselAutoplayDesc],
            ] as const).map(([key, title, description]) => (
              <SettingRow key={key} title={title} description={description}>
                <Toggle checked={preferences[key]} onChange={(checked) => onChange(key, checked)} label={title} />
              </SettingRow>
            ))}
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Cloud size={18} />
            <div>
              <strong>{t.settings.cloudSaves}</strong>
              <span>{t.settings.cloudSavesDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title="Lưu trữ Cloud Save"
              description="Launcher tự quản lý dữ liệu trong vùng riêng của Google Drive; không cần chọn thư mục thủ công."
            >
              <div className="settings-static-value">
                <ShieldCheck size={14} />
                Tự động · appDataFolder
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.cloudSaveProvider}
              description={t.settings.cloudSaveProviderDesc}
            >
              <div className="settings-static-value">
                <Cloud size={14} />
                {t.settings.cloudSaveProviderValue}
              </div>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group" id="steam-integration">
          <header>
            <Gamepad2 size={18} />
            <div>
              <strong>{t.settings.steamIntegration}</strong>
              <span>{t.settings.steamIntegrationDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow
              title={t.settings.steamClient}
              description={steamStatus ?? steamEnvironment?.rootPath ?? t.settings.steamClientDefault}
            >
              <div className={`settings-status-pill ${steamEnvironment?.running ? 'is-online' : ''}`}>
                {steamEnvironment
                  ? steamEnvironment.installed
                    ? steamEnvironment.running
                      ? `${t.settings.steamInstalled} · ${t.settings.steamRunning}`
                      : `${t.settings.steamInstalled} · ${t.settings.steamStopped}`
                    : t.settings.steamNotDetected
                  : t.settings.steamChecking}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamLibraries}
              description={
                steamEnvironment?.libraryPaths.length
                  ? steamEnvironment.libraryPaths.join(' · ')
                  : t.settings.steamLibrariesDesc
              }
            >
              <div className="settings-static-value">
                <HardDrive size={14} />
                {steamEnvironment?.libraryPaths.length ?? 0} {t.settings.steamLibrariesDetected}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamProfile}
              description={
                steamEnvironment?.activeAccountId
                  ? `Account ${steamEnvironment.activeAccountId} · UI language ${steamEnvironment.uiLanguage ?? 'unknown'}`
                  : t.settings.steamProfileDesc
              }
            >
              <div className="settings-static-value">
                {steamEnvironment?.pendingShortcutActions ?? 0} {t.settings.steamShortcutsQueued}
              </div>
            </SettingRow>
            <SettingRow
              title={t.settings.steamInterface}
              description={t.settings.steamInterfaceDesc}
            >
              <div className="settings-action-row">
                <button type="button" className="settings-secondary-button" onClick={onRefreshSteam}>
                  <RefreshCcw size={15} />
                  {t.settings.refresh}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenSteam}>
                  <MonitorCog size={15} />
                  {t.settings.openSteam}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onRestartSteam}>
                  <RotateCcw size={15} />
                  {t.settings.restartSteam}
                </button>
                <button type="button" className="settings-secondary-button" onClick={onOpenBigPicture}>
                  <Gamepad2 size={15} />
                  {t.settings.bigPicture}
                </button>
              </div>
            </SettingRow>
            <SteamAutoInstallSettings />
            <LuaGameModeToggle
              steamEnvironment={steamEnvironment}
              onEnabledChange={onLuaGameModeChange}
            />
          </div>
        </section>

        <LuaSourcesSettings />


        <section className="settings-group">
          <header>
            <HardDrive size={18} />
            <div>
              <strong>{t.settings.downloadsStorage}</strong>
              <span>{t.settings.downloadsStorageDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.defaultLibrary} description={t.settings.defaultLibraryDesc}>
              <div className="settings-path-control">
                <span title={preferences.defaultLibraryRoot}>{preferences.defaultLibraryRoot}</span>
                <button type="button" onClick={onOpenLibrary} title="Open folder">
                  <FolderOpen size={15} />
                </button>
                <button type="button" onClick={onChooseLibrary}>{t.settings.change}</button>
              </div>
            </SettingRow>
            <SettingRow title={t.settings.openDownloadsOnStart} description={t.settings.openDownloadsOnStartDesc}>
              <Toggle
                checked={preferences.openDownloadsOnJobStart}
                onChange={(checked) => onChange('openDownloadsOnJobStart', checked)}
                label={t.settings.openDownloadsOnStart}
              />
            </SettingRow>
            <SettingRow title={t.settings.gameUpdateMode} description={t.settings.gameUpdateModeDesc}>
              <CustomSelect
                value={launcherSettings.gameUpdateMode}
                onChange={(v) => onLauncherSettingChange('gameUpdateMode', v)}
                options={[
                  { value: 'automatic', label: t.settings.updateAutomatic },
                  { value: 'scheduled', label: t.settings.updateScheduled },
                  { value: 'manual', label: t.settings.updateManual },
                ]}
              />
            </SettingRow>
            {launcherSettings.gameUpdateMode === 'scheduled' ? (
              <SettingRow title={t.settings.updateWindow} description={t.settings.updateWindowDesc}>
                <div className="settings-time-range">
                  <input
                    type="time"
                    value={launcherSettings.gameUpdateScheduleStart}
                    onChange={(event) =>
                      onLauncherSettingChange('gameUpdateScheduleStart', event.target.value)
                    }
                  />
                  <span>{t.settings.updateWindowTo}</span>
                  <input
                    type="time"
                    value={launcherSettings.gameUpdateScheduleEnd}
                    onChange={(event) =>
                      onLauncherSettingChange('gameUpdateScheduleEnd', event.target.value)
                    }
                  />
                </div>
              </SettingRow>
            ) : null}
            <SettingRow
              title={t.settings.downloadProfile}
              description={`${launcherSettings.downloadWorkers} workers · ${launcherSettings.downloadQueueMb} MiB memory budget`}
            >
              <CustomSelect
                value={launcherSettings.downloadProfile}
                onChange={(v) => onLauncherSettingChange('downloadProfile', v)}
                options={[
                  { value: 'eco', label: 'Eco' },
                  { value: 'balanced', label: 'Balanced' },
                  { value: 'turbo', label: 'Turbo' },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.downloaderV2} description={t.settings.downloaderV2Desc}>
              <Toggle
                checked={launcherSettings.directToStaging}
                onChange={(checked) => onLauncherSettingChange('directToStaging', checked)}
                label={t.settings.downloaderV2}
              />
            </SettingRow>
            <SettingRow title={t.settings.pauseBeforeLaunch} description={t.settings.pauseBeforeLaunchDesc}>
              <Toggle
                checked={preferences.pauseDownloadsBeforeLaunch}
                onChange={(checked) => onChange('pauseDownloadsBeforeLaunch', checked)}
                label={t.settings.pauseBeforeLaunch}
              />
            </SettingRow>
            <SettingRow title={t.settings.chunkCache} description={t.settings.chunkCacheDesc}>
              <button type="button" className="settings-secondary-button" onClick={onOpenCache}>
                <Gauge size={15} />
                {t.settings.manageCache}
              </button>
            </SettingRow>
            <SettingRow title={t.settings.resetAppData} description={t.settings.resetAppDataDesc}>
              <button
                type="button"
                style={{ background: '#e02424', color: 'white', border: 'none', padding: '8px 16px', borderRadius: '6px', fontWeight: 'bold', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: '8px' }}
                onClick={() => {
                  if (confirm('Are you sure you want to clear all app data and restart?')) {
                    localStorage.clear();
                    sessionStorage.clear();
                    window.location.reload();
                  }
                }}
              >
                <CircleAlert size={16} />
                {t.settings.clearCacheRestart}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Sparkles size={18} />
            <div>
              <strong>{t.settings.language}</strong>
              <span>{t.settings.languageDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.displayLanguage} description={t.settings.displayLanguageDesc}>
              <select
                className="settings-select"
                value={locale}
                onChange={(e) => setLocale(e.target.value as Locale)}
              >
                <option value="en-US">English</option>
                <option value="vi-VN">Tiếng Việt</option>
              </select>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Sparkles size={18} />
            <div>
              <strong>{t.settings.appearance}</strong>
              <span>{t.settings.appearanceDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.languageLabel} description={t.settings.languageLabelDesc}>
              <CustomSelect
                value={locale}
                onChange={(v) => setLocale(v as Locale)}
                options={[
                  { value: 'en-US', label: 'English' },
                  { value: 'vi-VN', label: 'Tiếng Việt' },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.accentTone || 'Accent tone'} description={t.settings.accentToneDesc || 'Customize the launcher accent while keeping contrast and surfaces subdued.'}>
              <AccentTonePicker
                hue={preferences.accentHue}
                chroma={preferences.accentChroma}
                themeIntensity={preferences.themeIntensity}
                themeContrast={preferences.themeContrast}
                dynamicTheme={preferences.dynamicTheme}
                dynamicThemeSpeed={preferences.dynamicThemeSpeed}
                onHueChange={(value) => onChange('accentHue', value)}
                onChromaChange={(value) => onChange('accentChroma', value)}
                onThemeIntensityChange={(value) => onChange('themeIntensity', value)}
                onThemeContrastChange={(value) => onChange('themeContrast', value)}
                onDynamicThemeChange={(value) => onChange('dynamicTheme', value)}
                onDynamicThemeSpeedChange={(value) => onChange('dynamicThemeSpeed', value)}
                labels={{
                  hue: t.settings.themeHue || 'Hue',
                  saturation: t.settings.themeSaturation || 'Saturation',
                  intensity: t.settings.themeIntensity || 'Theme intensity',
                  contrast: t.settings.themeContrast || 'Contrast',
                  dynamic: t.settings.dynamicTheme || 'Dynamic theme',
                  dynamicDesc: t.settings.dynamicThemeDesc || 'Smoothly cycle the launcher palette through the color wheel.',
                  speed: t.settings.dynamicThemeSpeed || 'Cycle speed',
                  default: t.settings.defaultAccent || 'Default',
                  preview: t.settings.themePreview || 'Launcher palette preview',
                }}
              />
            </SettingRow>
            <SettingRow title={t.settings.motion} description={t.settings.motionDesc}>
              <CustomSelect
                value={preferences.motionMode}
                onChange={(v) => onChange('motionMode', v)}
                options={[
                  { value: 'full', label: t.settings.motionFull },
                  { value: 'system', label: t.settings.motionSystem },
                  { value: 'reduced', label: t.settings.motionReduced },
                ]}
              />
            </SettingRow>
            <SettingRow title={t.settings.glassEffects} description={t.settings.glassEffectsDesc}>
              <Toggle checked={preferences.glassEffects} onChange={(value) => onChange('glassEffects', value)} label={t.settings.glassEffects} />
            </SettingRow>
            <SettingRow title={t.settings.scrollEffects} description={t.settings.scrollEffectsDesc}>
              <Toggle checked={preferences.scrollEffects} onChange={(value) => onChange('scrollEffects', value)} label={t.settings.scrollEffects} />
            </SettingRow>
            <SettingRow title={t.settings.hoverHints} description={t.settings.hoverHintsDesc}>
              <Toggle checked={preferences.hoverHints} onChange={(value) => onChange('hoverHints', value)} label={t.settings.hoverHints} />
            </SettingRow>
            <SettingRow title={t.settings.installSound} description={t.settings.installSoundDesc}>
              <Toggle
                checked={preferences.playInstallCompleteSound}
                onChange={(checked) => onChange('playInstallCompleteSound', checked)}
                label={t.settings.installSound}
              />
            </SettingRow>
            <SettingRow title={t.settings.onboarding} description={t.settings.onboardingDesc}>
              <button type="button" className="settings-secondary-button" onClick={onResetOnboarding}>
                <RefreshCcw size={15} /> {t.settings.replayIntro}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <Clock3 size={18} />
            <div>
              <strong>{t.settings.statusBar}</strong>
              <span>{t.settings.statusBarDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            {([
              ['showClock', t.settings.clock, t.settings.clockDesc],
              ['showDate', t.settings.date, t.settings.dateDesc],
              ['showNetworkStatus', t.settings.networkStatus, t.settings.networkStatusDesc],
              ['showDownloadIndicator', t.settings.downloadIndicator, t.settings.downloadIndicatorDesc],
              ['showNotificationBell', t.settings.notificationBell, t.settings.notificationBellDesc],
            ] as const).map(([key, title, description]) => (
              <SettingRow key={key} title={title} description={description}>
                <Toggle checked={preferences[key]} onChange={(value) => onChange(key, value)} label={title} />
              </SettingRow>
            ))}
            <SettingRow title={t.settings.clockFormat} description={t.settings.clockFormatDesc}>
              <CustomSelect
                value={preferences.clockFormat}
                onChange={(v) => onChange('clockFormat', v)}
                options={[
                  { value: 'system', label: t.settings.clockSystem },
                  { value: '12h', label: t.settings.clock12h },
                  { value: '24h', label: t.settings.clock24h },
                ]}
              />
            </SettingRow>
          </div>
        </section>

        <section className="settings-group" id="notification-settings">
          <header>
            <Bell size={18} />
            <div>
              <strong>{t.settings.notifications}</strong>
              <span>{t.settings.notificationsDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.inAppNotifications} description={t.settings.inAppNotificationsDesc}>
              <Toggle checked={preferences.inAppNotifications} onChange={(value) => onChange('inAppNotifications', value)} label={t.settings.inAppNotifications} />
            </SettingRow>
            <SettingRow title={t.settings.windowsNotifications} description={t.settings.windowsNotificationsDesc}>
              <Toggle checked={preferences.windowsNotifications} onChange={(value) => onChange('windowsNotifications', value)} label={t.settings.windowsNotifications} />
            </SettingRow>
            <SettingRow title={t.settings.notificationSound} description={t.settings.notificationSoundDesc}>
              <Toggle checked={preferences.notificationSound} onChange={(value) => onChange('notificationSound', value)} label={t.settings.notificationSound} />
            </SettingRow>
            <SettingRow title={t.settings.doNotDisturb} description={t.settings.doNotDisturbDesc}>
              <Toggle checked={preferences.doNotDisturbWhilePlaying} onChange={(value) => onChange('doNotDisturbWhilePlaying', value)} label={t.settings.doNotDisturb} />
            </SettingRow>
            {([
              ['launcher', t.settings.notifCatLauncher],
              ['installs', t.settings.notifCatInstalls],
              ['downloads', t.settings.notifCatDownloads],
              ['cloudSaves', t.settings.notifCatCloudSaves],
              ['storage', t.settings.notifCatStorage],
              ['achievements', t.settings.notifCatAchievements],
              ['errors', t.settings.notifCatErrors],
            ] as Array<[NotificationCategory, string]>).map(([category, label]) => (
              <SettingRow key={category} title={label} description={t.settings.notifCatAllow.replace('{label}', label.toLowerCase())}>
                <Toggle
                  checked={preferences.notificationCategories[category]}
                  onChange={(value) =>
                    onChange('notificationCategories', {
                      ...preferences.notificationCategories,
                      [category]: value,
                    })
                  }
                  label={label}
                />
              </SettingRow>
            ))}
            <SettingRow title={t.settings.manageHistory} description={t.settings.manageHistoryDesc}>
              <button type="button" className="settings-secondary-button" onClick={onManageNotifications}>
                <Bell size={15} /> {t.settings.manageHistoryBtn}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <CircleHelp size={18} />
            <div>
              <strong>{t.help.centerTitle}</strong>
              <span>{t.help.centerSubtitle}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.help.centerTitle} description={t.help.centerSubtitle}>
              <button type="button" className="settings-secondary-button" onClick={onOpenHelpCenter}>
                <CircleHelp size={15} /> {t.help.openHelpCenter}
              </button>
            </SettingRow>
            <SettingRow title={t.help.replayTour} description={t.help.replayTourDesc}>
              <button type="button" className="settings-secondary-button" onClick={onResetOnboarding}>
                <RefreshCcw size={15} /> {t.help.replayTour}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <RefreshCcw size={18} />
            <div>
              <strong>{t.settings.launcherUpdates}</strong>
              <span>{t.settings.launcherUpdatesDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.autoCheckUpdates} description={t.settings.autoCheckUpdatesDesc}>
              <Toggle
                checked={preferences.autoCheckLauncherUpdates}
                onChange={(checked) => onChange('autoCheckLauncherUpdates', checked)}
                label={t.settings.autoCheckUpdates}
              />
            </SettingRow>
            <SettingRow title={t.settings.updateChannel} description={t.settings.updateChannelDesc}>
              <div className="settings-static-value">{t.settings.updateChannelValue}</div>
            </SettingRow>
            <SettingRow title={t.settings.checkNow} description={updateStatus ?? t.settings.checkNowDefault}>
              <button type="button" className="settings-secondary-button" onClick={onCheckForUpdates}>
                <RefreshCcw size={15} />
                {t.settings.checkForUpdates}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-group">
          <header>
            <CircleAlert size={18} style={{ color: '#ef4444' }} />
            <div>
              <strong style={{ color: '#ef4444' }}>{t.settings.dangerZone}</strong>
              <span>{t.settings.dangerZoneDesc}</span>
            </div>
          </header>
          <div className="settings-group-body">
            <SettingRow title={t.settings.resetLauncherData} description={t.settings.resetLauncherDataDesc}>
              <button
                type="button"
                className="settings-secondary-button"
                style={{ borderColor: '#ef4444', color: '#ef4444' }}
                onClick={async () => {
                  if (confirm("Bạn có chắc chắn muốn xóa toàn bộ dữ liệu cấu hình và đăng nhập (Google Drive, Discord...) của Launcher không? Việc này không thể hoàn tác.")) {
                    try {
                      await invoke('clear_launcher_config')
                      localStorage.clear()
                      alert("Đã xóa toàn bộ cấu hình! Launcher sẽ tắt bây giờ, vui lòng mở lại.")
                      await invoke('exit_app')
                    } catch (e) {
                      console.error(e)
                      alert("Lỗi khi xóa cấu hình: " + e)
                    }
                  }
                }}
              >
                {t.settings.resetData}
              </button>
            </SettingRow>
          </div>
        </section>

        <section className="settings-about-card">
          <Info size={18} />
          <div>
            <strong>0xoLemon Launcher</strong>
            <span>{t.settings.aboutVersion.replace('{version}', appVersion)}</span>
          </div>
          <Download size={17} />
        </section>
      </div>
    </section>
  )
}
