import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ArrowDown, ArrowUp, CircleAlert, LoaderCircle, Plus, RefreshCw, RotateCcw, Save, Trash2, X } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type {
  SteamLaunchApplyResult,
  SteamLaunchMod,
  SteamLaunchOption,
  SteamLaunchState,
} from '../types'
import './SteamLaunchOptionsDialog.css'

type SteamLaunchOptionsDialogProps = {
  locale: string
  initialAppId?: number | null
  onClose: () => void
}

const emptyOption = (index: number): SteamLaunchOption => ({
  index: String(index),
  sourceIndex: '',
  executable: '',
  arguments: '',
  workingDir: '',
  description: '',
  launchType: 'default',
  osList: 'windows',
  osArch: '',
  betaKey: '',
  ownsDlc: '',
})

function renumber(options: SteamLaunchOption[]): SteamLaunchOption[] {
  return options.map((option, index) => ({ ...option, index: String(index) }))
}

export default function SteamLaunchOptionsDialog({ locale, initialAppId, onClose }: SteamLaunchOptionsDialogProps) {
  const vi = locale === 'vi-VN'
  const dialogRef = useRef<HTMLElement>(null)
  const closeRef = useRef(onClose)
  const busyRef = useRef(false)
  const [appIdInput, setAppIdInput] = useState(initialAppId ? String(initialAppId) : '')
  const [state, setState] = useState<SteamLaunchState | null>(null)
  const [options, setOptions] = useState<SteamLaunchOption[]>([])
  const [mods, setMods] = useState<SteamLaunchMod[]>([])
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => { closeRef.current = onClose }, [onClose])
  useEffect(() => { busyRef.current = Boolean(busy) }, [busy])

  useEffect(() => {
    void invoke<SteamLaunchMod[]>('list_steam_launch_mods').then(setMods).catch(() => undefined)
  }, [])

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    const focusable = () => Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ) ?? [])
    window.requestAnimationFrame(() => focusable()[0]?.focus())
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busyRef.current) {
        event.preventDefault()
        closeRef.current()
        return
      }
      if (event.key !== 'Tab') return
      const items = focusable()
      if (!items.length) return
      const first = items[0]
      const last = items[items.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      previous?.focus()
    }
  }, [])

  const appId = Number(appIdInput)
  const validAppId = Number.isSafeInteger(appId) && appId > 0
  const activeMod = useMemo(() => mods.find((mod) => mod.appId === appId) ?? null, [appId, mods])

  const refreshMods = useCallback(async () => {
    setMods(await invoke<SteamLaunchMod[]>('list_steam_launch_mods'))
  }, [])

  const load = useCallback(async () => {
    if (!validAppId) return
    setBusy('load')
    setError(null)
    setNotice(null)
    try {
      const next = await invoke<SteamLaunchState>('read_steam_launch_options', { appId })
      setState(next)
      setOptions(next.current)
      await refreshMods()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }, [appId, refreshMods, validAppId])

  const updateOption = (index: number, field: keyof SteamLaunchOption, value: string) => {
    setOptions((current) => current.map((option, itemIndex) => (
      itemIndex === index ? { ...option, [field]: value } : option
    )))
  }

  const move = (index: number, offset: number) => {
    setOptions((current) => {
      const target = index + offset
      if (target < 0 || target >= current.length) return current
      const next = [...current]
      ;[next[index], next[target]] = [next[target], next[index]]
      return renumber(next)
    })
  }

  const stage = async () => {
    if (!state) return
    setBusy('stage')
    setError(null)
    setNotice(null)
    try {
      await invoke<SteamLaunchMod>('stage_steam_launch_options', {
        appId: state.appId,
        changeNumber: state.changeNumber,
        desired: renumber(options),
      })
      await refreshMods()
      setNotice(vi ? 'Đã stage. appinfo.vdf chưa bị thay đổi.' : 'Staged. appinfo.vdf has not been touched yet.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }

  const apply = async (reapply = false) => {
    if (!state) return
    setBusy(reapply ? 'reapply' : 'apply')
    setError(null)
    setNotice(null)
    try {
      const command = reapply ? 'reapply_steam_launch_mods' : 'apply_staged_steam_launch_options'
      const result = await invoke<SteamLaunchApplyResult>(command, { request: { appIds: [state.appId] } })
      setNotice(vi
        ? `Đã áp dụng qua transaction ${result.transactionId?.slice(0, 8) ?? ''}. Bạn có thể mở lại Steam.`
        : `Applied through transaction ${result.transactionId?.slice(0, 8) ?? ''}. You can start Steam again.`)
      await load()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }

  const restore = async () => {
    if (!state) return
    setBusy('restore')
    setError(null)
    setNotice(null)
    try {
      await invoke<SteamLaunchApplyResult>('restore_steam_launch_options', { appId: state.appId })
      setNotice(vi ? 'Đã khôi phục snapshot gốc và xóa đúng mod entry.' : 'Restored the current original snapshot and removed its mod entry.')
      await load()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }

  const restartSteam = async () => {
    setBusy('steam')
    setError(null)
    try {
      await invoke('restart_steam')
      await load()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }

  return createPortal(
    <div className="steam-launch-backdrop" role="presentation" onMouseDown={() => !busy && onClose()}>
      <section ref={dialogRef} className="steam-launch-dialog" role="dialog" aria-modal="true" aria-labelledby="steam-launch-title" onMouseDown={(event) => event.stopPropagation()}>
        <header>
          <div>
            <span>TOOLS · STEAM</span>
            <h2 id="steam-launch-title">Steam Launch Options</h2>
            <p>{vi ? 'Stage trước, chỉ Apply khi bạn đã đóng Steam thủ công.' : 'Stage first; Apply only after you close Steam manually.'}</p>
          </div>
          <button type="button" disabled={Boolean(busy)} onClick={onClose} aria-label="Close"><X /></button>
        </header>

        <div className="steam-launch-toolbar">
          <label>
            AppID
            <input inputMode="numeric" value={appIdInput} onChange={(event) => { setAppIdInput(event.target.value.replace(/\D/g, '')); setState(null); setOptions([]) }} placeholder="1245620" />
          </label>
          <button type="button" disabled={!validAppId || Boolean(busy)} onClick={() => void load()}>{busy === 'load' ? <LoaderCircle className="spin" /> : <RefreshCw />} Load</button>
          {state ? <div><span>{state.format.toUpperCase()}</span><span>change {state.changeNumber}</span><span className={state.steamRunning ? 'is-running' : 'is-closed'}>{state.steamRunning ? 'Steam running' : 'Steam closed'}</span></div> : null}
        </div>

        {error ? <div className="steam-launch-message is-error"><CircleAlert /> <span>{error}</span></div> : null}
        {notice ? <div className="steam-launch-message is-success"><Save /> <span>{notice}</span></div> : null}
        {activeMod?.driftState === 'drifted' ? <div className="steam-launch-message is-warning"><CircleAlert /> <span>{vi ? 'Steam đã ghi lại record này. Hãy kiểm tra rồi Re-apply.' : 'Steam rewrote this record. Review it, then Re-apply.'}</span></div> : null}
        {activeMod?.driftState === 'rebaseReviewRequired' ? <div className="steam-launch-message is-warning"><CircleAlert /> <span>{vi ? 'changeNumber đã đổi nhưng desired vẫn còn live; cần review original trước lần ghi tiếp.' : 'changeNumber changed while desired remains live; review the original snapshot before another write.'}</span></div> : null}

        <div className="steam-launch-body">
          {!state ? (
            <div className="steam-launch-empty"><Save /><strong>{vi ? 'Nhập AppID để đọc record từ appinfo.vdf' : 'Enter an AppID to read its appinfo.vdf record'}</strong><span>{vi ? 'Codec chạy streaming ngoài main thread.' : 'The streaming codec runs off the main thread.'}</span></div>
          ) : (
            <>
              <div className="steam-launch-list-heading">
                <div><strong>{options.length} options</strong><span>{state.installDir ?? 'install dir unavailable'}</span></div>
                <button type="button" disabled={Boolean(busy)} onClick={() => setOptions((current) => [...current, emptyOption(current.length)])}><Plus /> Add option</button>
              </div>
              <div className="steam-launch-list">
                {options.map((option, index) => (
                  <article key={`${option.sourceIndex}:${index}`}>
                    <div className="steam-launch-option-number">{index + 1}</div>
                    <div className="steam-launch-fields">
                      <label className="is-wide">Executable<input value={option.executable} onChange={(event) => updateOption(index, 'executable', event.target.value)} placeholder="game.exe" /></label>
                      <label className="is-wide">Arguments<input value={option.arguments} onChange={(event) => updateOption(index, 'arguments', event.target.value)} placeholder="-windowed" /></label>
                      <label>Working directory<input value={option.workingDir} onChange={(event) => updateOption(index, 'workingDir', event.target.value)} /></label>
                      <label>Description<input value={option.description} onChange={(event) => updateOption(index, 'description', event.target.value)} /></label>
                      <label>OS list<input value={option.osList} onChange={(event) => updateOption(index, 'osList', event.target.value)} placeholder="windows" /></label>
                      <label>Type<input value={option.launchType} onChange={(event) => updateOption(index, 'launchType', event.target.value)} placeholder="default" /></label>
                    </div>
                    <div className="steam-launch-option-actions">
                      <button type="button" disabled={index === 0 || Boolean(busy)} onClick={() => move(index, -1)} aria-label="Move up"><ArrowUp /></button>
                      <button type="button" disabled={index + 1 === options.length || Boolean(busy)} onClick={() => move(index, 1)} aria-label="Move down"><ArrowDown /></button>
                      <button type="button" disabled={Boolean(busy)} onClick={() => setOptions((current) => renumber(current.filter((_, itemIndex) => itemIndex !== index)))} aria-label="Remove"><Trash2 /></button>
                    </div>
                  </article>
                ))}
              </div>
            </>
          )}
        </div>

        <footer>
          <div>
            <button type="button" disabled={!activeMod || state?.steamRunning || Boolean(busy)} onClick={() => void restore()}><RotateCcw /> {vi ? 'Khôi phục gốc' : 'Restore original'}</button>
            <button type="button" disabled={Boolean(busy)} onClick={() => void restartSteam()}><RefreshCw /> Restart Steam</button>
          </div>
          <div>
            <button type="button" disabled={!state || Boolean(busy)} onClick={() => void stage()}>{busy === 'stage' ? <LoaderCircle className="spin" /> : <Save />} Stage</button>
            {activeMod?.driftState === 'drifted' ? <button type="button" className="is-primary" disabled={state?.steamRunning || Boolean(busy)} title={state?.steamRunning ? 'Close Steam manually first.' : undefined} onClick={() => void apply(true)}>{busy === 'reapply' ? <LoaderCircle className="spin" /> : <RefreshCw />} Re-apply</button> : null}
            <button
              type="button"
              className="is-primary"
              disabled={!activeMod || state?.steamRunning || activeMod?.driftState === 'rebaseReviewRequired' || Boolean(busy)}
              title={state?.steamRunning
                ? 'Close Steam manually first.'
                : activeMod?.driftState === 'rebaseReviewRequired'
                  ? 'Reload, review, and Stage explicitly before applying this rebased record.'
                  : undefined}
              onClick={() => void apply(false)}
            >
              {busy === 'apply' ? <LoaderCircle className="spin" /> : <Save />} Apply
            </button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  )
}
