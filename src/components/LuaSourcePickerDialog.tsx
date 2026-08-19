import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Check, CloudDownload, Database, HelpCircle, Info, KeyRound, RefreshCw, Star, X } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { useLocale } from '../context/locale'
import { getLuaSourceMeta } from '../lib/luaSourcesMeta'
import './LuaShop.css'
import type {
  LuaGameChannel,
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
  preferredChannel?: LuaGameChannel | null
  forcedChannel?: LuaGameChannel | null
  onClose: () => void
  onConfirm: (provider: LuaSourceProvider, statSteamId: string | null, channel: LuaGameChannel) => Promise<void>
}

function sourceUsable(source: LuaSourceCandidate) {
  return source.enabled
    && (source.available || source.onDemand)
    && (!source.requiresKey || source.keyReady)
    && source.errorCode !== 'HUBCAP_KEY_INVALID'
}

function renderStars(stars: number) {
  const fullStars = Math.floor(stars)
  const hasHalf = stars % 1 !== 0
  return (
    <span className="lua-source-stars" title={`${stars} / 5 sao`}>
      {Array.from({ length: fullStars }).map((_, i) => (
        <Star key={`full-${i}`} size={11} className="star-filled" />
      ))}
      {hasHalf && <Star size={11} className="star-half" />}
      {Array.from({ length: 5 - Math.ceil(stars) }).map((_, i) => (
        <Star key={`empty-${i}`} size={11} className="star-empty" />
      ))}
      <span className="lua-source-stars-text">{stars.toFixed(1)}</span>
    </span>
  )
}

export function LuaSourcePickerDialog({
  appid,
  gameName,
  operation,
  preferredProvider = null,
  preferredChannel = null,
  forcedChannel = null,
  onClose,
  onConfirm,
}: LuaSourcePickerDialogProps) {
  const { t } = useLocale()
  const copy = t.luaShop.sourcePicker
  const dialogRef = useRef<HTMLElement>(null)
  const [result, setResult] = useState<LuaSourceScanResult | null>(null)
  const [selected, setSelected] = useState<LuaSourceProvider | null>(null)
  const [selectedChannel, setSelectedChannel] = useState<LuaGameChannel | null>(forcedChannel ?? preferredChannel ?? null)
  const [step, setStep] = useState<'source' | 'channel'>('source')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [statSteamId, setStatSteamId] = useState('')
  const [fetchingId, setFetchingId] = useState(false)
  const [infoProvider, setInfoProvider] = useState<string | null>(null)

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
      } else {
        // Auto-select recommended or first available source
        const recommended = scan.sources.find((s) => s.recommended && sourceUsable(s))
        const firstAvail = scan.sources.find((s) => sourceUsable(s))
        if (recommended) setSelected(recommended.provider)
        else if (firstAvail) setSelected(firstAvail.provider)
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
      if (event.key === 'Escape' && !busy) {
        if (infoProvider) setInfoProvider(null)
        else onClose()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      previousFocus?.focus()
    }
  }, [busy, infoProvider, onClose])

  const selectedSource = useMemo(
    () => result?.sources.find((source) => source.provider === selected) ?? null,
    [result, selected],
  )
  const selectedMeta = useMemo(
    () => selectedSource ? getLuaSourceMeta(selectedSource.provider, copy) : null,
    [copy, selectedSource],
  )

  useEffect(() => {
    if (forcedChannel) {
      setSelectedChannel(forcedChannel)
      return
    }
    if (!selectedMeta) {
      setSelectedChannel(null)
      return
    }
    if (selectedMeta.sourceType === 'live') setSelectedChannel('live')
    else if (selectedMeta.sourceType === 'locked') setSelectedChannel('locked')
    else setSelectedChannel(preferredChannel ?? null)
  }, [forcedChannel, preferredChannel, selectedMeta])
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

  const canAdvance = useMemo(() => {
    if (!selected || !selectedSource || !selectedMeta) return false
    if (!sourceUsable(selectedSource)) return false
    if (operation === 'sync' && !selectedSource.available && !selectedSource.onDemand) return false
    return true
  }, [operation, selected, selectedMeta, selectedSource])

  const needsChannelStep = Boolean(!forcedChannel && selectedMeta?.sourceType === 'hybrid')

  const canConfirm = useMemo(() => {
    if (!canAdvance || !selectedMeta || !selectedChannel) return false
    if (selectedMeta.sourceType === 'live' && selectedChannel !== 'live') return false
    if (selectedMeta.sourceType === 'locked' && selectedChannel !== 'locked') return false
    return true
  }, [canAdvance, selectedChannel, selectedMeta])

  const confirm = async () => {
    if (!selected || !selectedMeta) return
    if (needsChannelStep && step === 'source') {
      setSelectedChannel(preferredChannel ?? null)
      setStep('channel')
      return
    }
    if (!canConfirm || !selectedChannel) return
    setBusy(true)
    setError(null)
    try {
      await onConfirm(selected, statSteamId.trim() || null, selectedChannel)
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(false)
    }
  }

  const autoFetchSteamId = async () => {
    setFetchingId(true)
    setError(null)
    try {
      const id = await invoke<string>('fetch_donor_steamid', { appid })
      setStatSteamId(id)
    } catch (e) {
      setError(`Auto-fetch failed: ${e}`)
    } finally {
      setFetchingId(false)
    }
  }

  const activeMeta = infoProvider ? getLuaSourceMeta(infoProvider, copy) : null

  return createPortal(
    <div className="lua-source-picker-backdrop" role="presentation" onMouseDown={() => !busy && !infoProvider && onClose()}>
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
          <div className="lua-source-picker-desc-row">
            <p className="lua-source-picker-description">{copy.description}</p>
          </div>

          {step === 'source' ? (loading ? (
            <div className="lua-source-picker-loading"><RefreshCw size={18} className="spin" /> {copy.loading}</div>
          ) : (
            <div className="lua-source-picker-grid">
              {result?.sources.map((source) => {
                const usable = sourceUsable(source)
                const isSelected = selected === source.provider
                const meta = getLuaSourceMeta(source.provider, copy)
                const usesLuaToolsNetwork = source.provider === 'luie'
                const status = !source.enabled
                  ? copy.disabled
                  : source.requiresKey && !source.keyReady
                    ? copy.keyRequired
                    : source.errorCode === 'LUATOOLS_DISCOVERY_DEFERRED'
                      ? copy.luaToolsDiscoveryDeferred
                      : source.available
                        ? copy.available
                        : source.onDemand
                          ? copy.onDemand
                          : copy.unavailable

                return (
                  <div
                    key={source.provider}
                    className={`lua-source-option-card${isSelected ? ' is-selected' : ''}${!usable ? ' is-disabled' : ''}`}
                    onClick={() => { if (usable && !busy) { setSelected(source.provider); setStep('source') } }}
                    role="button"
                    tabIndex={usable ? 0 : -1}
                    aria-pressed={isSelected}
                    onKeyDown={(e) => {
                      if ((e.key === ' ' || e.key === 'Enter') && usable && !busy) {
                        e.preventDefault()
                        setSelected(source.provider)
                        setStep('source')
                      }
                    }}
                  >
                    <div className="lua-source-option-head">
                      <span className="lua-source-option-icon">
                        {source.provider === 'hubcap' ? <KeyRound size={16} /> : <Database size={16} />}
                      </span>
                      <strong className="lua-source-option-title">
                        {meta.displayName}
                      </strong>
                      <div className="lua-source-option-head-actions">
                        <button
                          type="button"
                          className="lua-source-info-btn"
                          title="Xem thông tin chi tiết nguồn"
                          onClick={(e) => {
                            e.stopPropagation()
                            setInfoProvider(source.provider)
                          }}
                        >
                          <HelpCircle size={14} />
                        </button>
                        {source.recommended && <em className="flag-recommended">{copy.recommended}</em>}
                        {isSelected && <span className="flag-selected"><Check size={13} /> {copy.selected}</span>}
                      </div>
                    </div>

                    <div className="lua-source-option-meta-row">
                      <span className={`lua-source-type-pill ${meta.sourceType}`}>
                        {meta.sourceType === 'live' && (
                          <>
                            <span className="live-dot" /> LIVE
                          </>
                        )}
                        {meta.sourceType === 'locked' && (
                          <>
                            <span className="locked-icon">🔒</span> LOCKED
                          </>
                        )}
                        {meta.sourceType === 'hybrid' && (
                          <>
                            <span className="live-dot" /> LIVE & LOCKED
                          </>
                        )}
                      </span>
                      {renderStars(meta.stars)}
                    </div>

                    <div className="lua-source-option-copy">
                      <small className="lua-source-status-text">{status}</small>
                      <small className="lua-source-quota-text">
                        {source.provider === 'ryuu'
                          ? 'Requires your Ryuu auth key'
                          : source.provider === 'twentyTwoCloud'
                            ? source.keyReady
                              ? 'Direct API configured · paid DepotBox key'
                              : 'Free Web mode · API key optional'
                          : source.requiresKey
                            ? copy.quotaNotice
                            : usesLuaToolsNetwork
                            ? copy.luaToolsAccountRequired
                            : copy.freeSource}
                      </small>
                      {source.revision && <code title={source.revision}>{copy.revision}: {source.revision.slice(0, 18)}</code>}
                      {source.errorCode && !usable && (
                        <code className="lua-source-error-code">
                          {source.errorCode?.startsWith('LUATOOLS_DISCOVERY_') ? copy.luaToolsDiscoveryUnavailable : source.errorCode}
                        </code>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )) : (
            <div className="lua-source-channel-step">
              <div className="lua-source-channel-step-heading">
                <strong>{copy.installModeTitle}</strong>
                <small>{copy.installModeDesc}</small>
              </div>
              {selectedMeta && (
                <div className="lua-source-channel-step-source">
                  <Database size={16} />
                  <span>{selectedMeta.displayName}</span>
                  <span className="lua-source-type-pill hybrid"><span className="live-dot" /> LIVE & LOCKED</span>
                </div>
              )}
              <div className="lua-source-channel-choice-buttons lua-source-channel-step-buttons" role="group" aria-label={copy.installModeTitle}>
                <button
                  type="button"
                  className={selectedChannel === 'live' ? 'is-active live' : ''}
                  onClick={() => setSelectedChannel('live')}
                  disabled={busy}
                >
                  <span className="live-dot" /> LIVE
                </button>
                <button
                  type="button"
                  className={selectedChannel === 'locked' ? 'is-active locked' : ''}
                  onClick={() => setSelectedChannel('locked')}
                  disabled={busy}
                >
                  <span className="locked-icon">🔒</span> LOCKED
                </button>
              </div>
            </div>
          )}

          {error && <div className="lua-source-picker-error">{error}</div>}

          {operation === 'sync' && selectedSource && !sourceUsable(selectedSource) && (
            <div className="lua-source-picker-warning">
              ⚠️ Nguồn này hiện không có sẵn nội dung cho game này. Không thể đồng bộ.
            </div>
          )}

          <div className="lua-source-picker-steamid">
            <div className="lua-source-picker-steamid-header">
              <label htmlFor="stat-steam-id">Donor SteamID64 for Achievements (Optional):</label>
              <button 
                type="button" 
                className="lua-source-picker-steamid-autofetch"
                onClick={autoFetchSteamId} 
                disabled={busy || fetchingId}
              >
                {fetchingId ? 'Fetching...' : '⚡ Auto Fetch'}
              </button>
            </div>
            <input 
              className="lua-source-picker-steamid-input"
              id="stat-steam-id"
              type="text" 
              value={statSteamId} 
              onChange={(e) => setStatSteamId(e.target.value)} 
              placeholder="e.g. 76561198271243904"
              disabled={busy || fetchingId}
            />
          </div>
        </div>

        <footer className="lua-source-picker-footer">
          <button
            type="button"
            onClick={() => step === 'channel' ? setStep('source') : onClose()}
            disabled={busy}
          >
            {step === 'channel' ? copy.back : copy.cancel}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || (needsChannelStep && step === 'source' ? !canAdvance : !canConfirm)}
            onClick={() => void confirm()}
          >
            {busy && <RefreshCw size={15} className="spin" />}
            {needsChannelStep && step === 'source' ? copy.continue : confirmLabel}
          </button>
        </footer>

        {/* Source Info Modal */}
        {activeMeta && (
          <div className="lua-source-info-modal-backdrop" onClick={() => setInfoProvider(null)}>
            <div className="lua-source-info-modal" onClick={(e) => e.stopPropagation()}>
              <header className="lua-source-info-modal-header">
                <div>
                  <div className="lua-source-info-title-row">
                    <h3>{activeMeta.displayName}</h3>
                    <span className={`lua-source-type-pill ${activeMeta.sourceType}`}>
                      {activeMeta.sourceTypeLabel}
                    </span>
                  </div>
                  <div className="lua-source-info-rank-row">
                    <span className="lua-source-info-rank">{activeMeta.rankLabel}</span>
                    {renderStars(activeMeta.stars)}
                  </div>
                </div>
                <button type="button" onClick={() => setInfoProvider(null)} aria-label="Close"><X size={16} /></button>
              </header>

              <div className="lua-source-info-modal-body">
                <div className="lua-source-info-section">
                  <h4><Info size={14} /> {copy.infoModal.overview}</h4>
                  <p>{activeMeta.summary}</p>
                  <p className="lua-source-info-subtext">{activeMeta.details}</p>
                </div>

                <div className="lua-source-info-section">
                  <h4>⚡ {copy.infoModal.roleAndComplement}</h4>
                  <p className="lua-source-info-highlight">{activeMeta.complementarity}</p>
                </div>

                <div className="lua-source-info-section">
                  <h4>💡 {copy.infoModal.whenToUse}</h4>
                  <p>{activeMeta.whenToUse}</p>
                </div>
              </div>

              <footer className="lua-source-info-modal-footer">
                <button type="button" className="primary" onClick={() => setInfoProvider(null)}>
                  {copy.infoModal.gotIt}
                </button>
              </footer>
            </div>
          </div>
        )}
      </section>
    </div>,
    document.body,
  )
}
