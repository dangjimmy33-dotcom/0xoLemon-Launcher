import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Check, CloudDownload, Database, KeyRound, RefreshCw, X } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { useLocale } from '../context/locale'
import './LuaShop.css'
import type {
  LuaSourceCandidate,
  LuaSourceOperation,
  LuaSourceProvider,
  LuaSourceScanResult,
} from '../types'

type LuaSourcePickerDialogProps = {
  appid: number
  gameName: string
  operation: LuaSourceOperation
  preferredProvider?: LuaSourceProvider | null
  onClose: () => void
  onConfirm: (provider: LuaSourceProvider) => Promise<void>
}

function sourceUsable(source: LuaSourceCandidate) {
  return source.enabled
    && (source.available || source.onDemand)
    && (!source.requiresKey || source.keyReady)
    && source.errorCode !== 'HUBCAP_KEY_INVALID'
}

export function LuaSourcePickerDialog({
  appid,
  gameName,
  operation,
  preferredProvider = null,
  onClose,
  onConfirm,
}: LuaSourcePickerDialogProps) {
  const { t } = useLocale()
  const copy = t.luaShop.sourcePicker
  const dialogRef = useRef<HTMLElement>(null)
  const [result, setResult] = useState<LuaSourceScanResult | null>(null)
  const [selected, setSelected] = useState<LuaSourceProvider | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    setLoading(true)
    setError(null)
    void invoke<LuaSourceScanResult>('scan_lua_sources', {
      request: { appid, operation },
    }).then((scan) => {
      if (!active) return
      setResult(scan)
      if (operation !== 'add' && preferredProvider) {
        const preferred = scan.sources.find((source) => source.provider === preferredProvider)
        if (preferred && sourceUsable(preferred)) setSelected(preferredProvider)
      }
    }).catch((reason) => {
      if (active) setError(`${copy.scanFailed} ${String(reason)}`)
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [appid, copy.scanFailed, operation, preferredProvider])

  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null
    window.requestAnimationFrame(() => dialogRef.current?.querySelector<HTMLElement>('button')?.focus())
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) onClose()
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      previousFocus?.focus()
    }
  }, [busy, onClose])

  const selectedSource = useMemo(
    () => result?.sources.find((source) => source.provider === selected) ?? null,
    [result, selected],
  )
  const title = operation === 'add'
    ? copy.addTitle
    : operation === 'update'
      ? copy.updateTitle
      : copy.syncTitle
  const confirmLabel = operation === 'add'
    ? copy.confirmAdd
    : operation === 'update'
      ? copy.confirmUpdate
      : copy.confirmSync

  const confirm = async () => {
    if (!selected || !selectedSource || !sourceUsable(selectedSource)) return
    setBusy(true)
    setError(null)
    try {
      await onConfirm(selected)
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(false)
    }
  }

  return createPortal(
    <div className="lua-source-picker-backdrop" role="presentation" onMouseDown={() => !busy && onClose()}>
      <section
        ref={dialogRef}
        className="lua-source-picker"
        role="dialog"
        aria-modal="true"
        aria-labelledby="lua-source-picker-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="lua-source-picker-header">
          <div className="lua-source-picker-icon"><CloudDownload size={20} /></div>
          <div>
            <h2 id="lua-source-picker-title">{title}</h2>
            <p>{gameName} · AppID {appid}</p>
          </div>
          <button type="button" onClick={onClose} disabled={busy} aria-label={copy.cancel}><X size={18} /></button>
        </header>

        <div className="lua-source-picker-body">
          <p className="lua-source-picker-description">{copy.description}</p>
          {loading ? (
            <div className="lua-source-picker-loading"><RefreshCw size={18} className="spin" /> {copy.loading}</div>
          ) : (
            <div className="lua-source-picker-grid">
              {result?.sources.map((source) => {
                const usable = sourceUsable(source)
                const isSelected = selected === source.provider
                const status = !source.enabled
                  ? copy.disabled
                  : source.requiresKey && !source.keyReady
                    ? copy.keyRequired
                    : source.available
                      ? copy.available
                      : source.onDemand
                        ? copy.onDemand
                        : copy.unavailable
                return (
                  <button
                    type="button"
                    key={source.provider}
                    className={`lua-source-option${isSelected ? ' is-selected' : ''}`}
                    disabled={!usable || busy}
                    aria-pressed={isSelected}
                    onClick={() => setSelected(source.provider)}
                  >
                    <span className="lua-source-option-icon">
                      {source.provider === 'hubcap' ? <KeyRound size={18} /> : <Database size={18} />}
                    </span>
                    <span className="lua-source-option-copy">
                      <strong>{copy.providers[source.provider]}</strong>
                      <small>{status}</small>
                      <small>{source.requiresKey ? copy.quotaNotice : copy.freeSource}</small>
                      {source.revision && <code>{copy.revision}: {source.revision.slice(0, 18)}</code>}
                      {source.errorCode && !usable && <code>{source.errorCode}</code>}
                    </span>
                    <span className="lua-source-option-flags">
                      {source.recommended && <em>{copy.recommended}</em>}
                      {isSelected && <span><Check size={14} /> {copy.selected}</span>}
                    </span>
                  </button>
                )
              })}
            </div>
          )}
          {error && <div className="lua-source-picker-error">{error}</div>}
        </div>

        <footer className="lua-source-picker-footer">
          <button type="button" onClick={onClose} disabled={busy}>{copy.cancel}</button>
          <button
            type="button"
            className="primary"
            disabled={!selectedSource || !sourceUsable(selectedSource) || busy}
            onClick={() => void confirm()}
          >
            {busy && <RefreshCw size={15} className="spin" />}
            {confirmLabel}
          </button>
        </footer>
      </section>
    </div>,
    document.body,
  )
}
