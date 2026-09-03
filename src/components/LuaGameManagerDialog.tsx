import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Archive, Check, CheckCircle2, ChevronDown, CircleAlert, Download, FolderCog, Pin, RefreshCw, RotateCcw, ShieldCheck, Trash2, Undo2, X } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { useLocale } from '../context/locale'
import type {
  LuaGameChannel,
  LuaGameManagerState,
  LuaGameState,
  LuaDriftResolution,
  LuaFileDriftReport,
  LuaRuntimeTargetScan,
  LuaVariantEntry,
} from '../types'
import { checkLuaFileDrift, resolveLuaDrift } from '../lib/luaDrift'

type ManagerBuild = {
  build_id: string
  version: string | null
  build_date?: string
  patch_title?: string
  manifests: Array<{ depot_id: number; manifest_gid: string }>
  manifest_available?: boolean
}

type ManagerBuilds = {
  builds: ManagerBuild[]
  has_key: boolean
}

type LuaGameManagerDialogProps = {
  appid: number | null
  gameName: string
  onClose: () => void
  onState: (state: LuaGameState) => void
  onSync: (appid: string) => Promise<void>
  onSwitchLive: (appid: string) => Promise<void>
  onRemove: (appid: string) => void
  onRestartSteam: () => Promise<void>
}

function sourceLabel(
  state: LuaGameState,
  labels: { local: string; community: string; curated: string },
) {
  const provider = state.selectedSource ?? state.sourceProvider ?? 'none'
  if (provider === 'none') return labels.local
  if (provider === 'huggingFace') {
    return state.selectedVariant === 'community' ? labels.community : labels.curated
  }
  if (provider === 'community') return labels.community
  if (provider === 'curated') return labels.curated
  return provider.charAt(0).toUpperCase() + provider.slice(1)
}

export function LuaGameManagerDialog({
  appid,
  gameName,
  onClose,
  onState,
  onSync,
  onSwitchLive,
  onRemove,
  onRestartSteam,
}: LuaGameManagerDialogProps) {
  const { t, locale } = useLocale()
  const managerText = t.luaShop.manager
  const dialogRef = useRef<HTMLElement>(null)
  const buildMenuRef = useRef<HTMLDivElement>(null)
  const buildMenuOpenRef = useRef(false)
  const [manager, setManager] = useState<LuaGameManagerState | null>(null)
  const [builds, setBuilds] = useState<ManagerBuild[]>([])
  const [selectedChannel, setSelectedChannel] = useState<LuaGameChannel>('live')
  const [selectedBuildId, setSelectedBuildId] = useState('')
  const [buildMenuOpen, setBuildMenuOpen] = useState(false)
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const busyActionRef = useRef<string | null>(null)
  const onCloseRef = useRef(onClose)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [variants, setVariants] = useState<LuaVariantEntry[]>([])
  const [drift, setDrift] = useState<LuaFileDriftReport | null>(null)
  const [runtimeScan, setRuntimeScan] = useState<LuaRuntimeTargetScan | null>(null)

  useEffect(() => {
    busyActionRef.current = busyAction
  }, [busyAction])

  useEffect(() => {
    buildMenuOpenRef.current = buildMenuOpen
  }, [buildMenuOpen])

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  const reload = useCallback(async () => {
    if (!appid) return
    const [state, entries, driftReport] = await Promise.all([
      invoke<LuaGameManagerState>('get_lua_game_manager_state', { appid }),
      invoke<LuaVariantEntry[]>('list_lua_variants', { appId: appid }),
      checkLuaFileDrift(appid),
    ])
    setManager(state)
    setVariants(entries)
    setDrift(driftReport)
    setSelectedChannel(state.game.channel)
    setSelectedBuildId(state.game.pinnedBuildId ?? '')
    onState(state.game)
  }, [appid, onState])

  useEffect(() => {
    if (!appid) return
    let active = true
    void Promise.allSettled([
      invoke<LuaGameManagerState>('get_lua_game_manager_state', { appid }),
      invoke<ManagerBuilds>('lua_shop_get_game_builds', { appid, gameName }),
      invoke<LuaVariantEntry[]>('list_lua_variants', { appId: appid }),
      checkLuaFileDrift(appid),
    ]).then(([managerResult, buildsResult, variantsResult, driftResult]) => {
      if (!active) return
      if (managerResult.status === 'fulfilled') {
        setManager(managerResult.value)
        setSelectedChannel(managerResult.value.game.channel)
        setSelectedBuildId(managerResult.value.game.pinnedBuildId ?? '')
        onState(managerResult.value.game)
      } else {
        setError(String(managerResult.reason))
      }
      if (buildsResult.status === 'fulfilled') setBuilds(buildsResult.value.builds)
      if (variantsResult.status === 'fulfilled') setVariants(variantsResult.value)
      if (driftResult.status === 'fulfilled') setDrift(driftResult.value)
      setBusyAction(null)
    })
    return () => {
      active = false
    }
  }, [appid, gameName, onState])

  useEffect(() => {
    if (!appid) return
    const previousFocus = document.activeElement as HTMLElement | null
    const dialog = dialogRef.current
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
    ) ?? [])
    window.requestAnimationFrame(() => focusable()[0]?.focus())
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busyActionRef.current) {
        event.preventDefault()
        if (buildMenuOpenRef.current) {
          setBuildMenuOpen(false)
          return
        }
        onCloseRef.current()
        return
      }
      if (event.key !== 'Tab') return
      const items = focusable()
      if (items.length === 0) return
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
      previousFocus?.focus()
    }
  }, [appid])

  useEffect(() => {
    if (!buildMenuOpen) return
    const onPointerDown = (event: PointerEvent) => {
      if (!buildMenuRef.current?.contains(event.target as Node)) setBuildMenuOpen(false)
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [buildMenuOpen])

  const compatibleBuilds = useMemo(
    () => builds.filter((build) => build.manifest_available !== false && build.manifests.length > 0),
    [builds],
  )
  const selectedBuild = useMemo(
    () => compatibleBuilds.find((build) => build.build_id === selectedBuildId) ?? null,
    [compatibleBuilds, selectedBuildId],
  )

  const formatBuildMetadata = useCallback((build: ManagerBuild) => {
    const date = build.build_date
      ? new Date(/^\d+$/.test(build.build_date) ? Number(build.build_date) * 1000 : build.build_date).toLocaleDateString()
      : ''
    return [date, build.patch_title].filter(Boolean).join(' · ')
  }, [])

  const applyChannel = async () => {
    if (!appid || !manager) return
    const reviewingLegacy = manager.game.migrationState === 'reviewRequired'
    setBusyAction('channel')
    setError(null)
    setSuccess(null)
    try {
      if (selectedChannel === 'live') {
        const provider = manager.game.selectedSource
        if (!provider) {
          await onSwitchLive(String(appid))
          return
        }
        const state = await invoke<LuaGameState>('set_lua_game_channel', {
          request: {
            appid,
            channel: 'live',
            buildId: null,
            conflictResolution: 'restoreLive',
            provider,
            restartSteamIfNeeded: false,
          },
        })
        onState(state)
        await reload()
        if (state.requiresSteamRestart) {
          await onRestartSteam()
        }
        return
      }
      if (reviewingLegacy && selectedChannel === 'locked' && !selectedBuildId) {
        const states = await invoke<LuaGameState[]>('resolve_legacy_lua_games', {
          decisions: [{ appid, action: 'keepLocked' }],
        })
        const state = states.find((candidate) => candidate.appid === appid)
        if (state) onState(state)
        await reload()
        return
      }
      const state = await invoke<LuaGameState>('set_lua_game_channel', {
        request: {
          appid,
          channel: selectedChannel,
          buildId: selectedChannel === 'locked' && selectedBuildId ? selectedBuildId : null,
          conflictResolution: null,
          // Historical BuildIDs displayed here are the verified HF builds.
          // Passing the provider explicitly prevents an implicit cross-source
          // fallback when the currently installed Lua came from another source.
          provider: selectedBuildId ? 'huggingFace' : null,
          restartSteamIfNeeded: true,
        },
      })
      onState(state)
      await reload()
      setSuccess(
        state.requiresSteamRestart
          ? managerText.appliedNeedsRestart
          : selectedBuildId
            ? managerText.appliedVersion.replace('{buildId}', selectedBuildId)
            : managerText.appliedChannel,
      )
    } catch (reason) {
      setError(String(reason))
      try {
        await reload()
      } catch {
        // Preserve the channel error; reload is only a best-effort state sync.
      }
    } finally {
      setBusyAction(null)
    }
  }

  const sync = async () => {
    if (!appid) return
    setBusyAction('sync')
    setError(null)
    setSuccess(null)
    try {
      await onSync(String(appid))
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const captureVariant = async () => {
    if (!appid) return
    setBusyAction('captureVariant')
    setError(null)
    setSuccess(null)
    try {
      await invoke<LuaVariantEntry>('capture_lua_variant', { appId: appid, reason: 'manual' })
      setVariants(await invoke<LuaVariantEntry[]>('list_lua_variants', { appId: appid }))
      setSuccess(locale === 'vi-VN' ? 'Đã lưu nguyên byte Lua hiện tại vào Variant Vault.' : 'Captured the exact active Lua bytes in Variant Vault.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const refreshDrift = async () => {
    if (!appid) return
    setBusyAction('checkDrift')
    setError(null)
    try {
      setDrift(await checkLuaFileDrift(appid))
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const chooseDriftResolution = async (resolution: LuaDriftResolution) => {
    if (!appid || !manager) return
    setBusyAction(`drift:${resolution}`)
    setError(null)
    setSuccess(null)
    try {
      const result = await resolveLuaDrift({
        appId: appid,
        resolution,
        provider: manager.game.selectedSource,
        requestId: crypto.randomUUID(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      })
      onState(result.game)
      await reload()
      setSuccess(
        resolution === 'keepExternal'
          ? (locale === 'vi-VN' ? 'Đã giữ local và lưu exact bytes vào Variant Vault.' : 'Kept the local file and captured its exact bytes in Variant Vault.')
          : (locale === 'vi-VN' ? 'Đã capture external rồi áp dụng provider bằng transaction được quản lý.' : 'Captured the external file and applied the provider update through the managed path.'),
      )
    } catch (reason) {
      setError(String(reason))
      try { setDrift(await checkLuaFileDrift(appid)) } catch { /* keep the resolution error */ }
    } finally {
      setBusyAction(null)
    }
  }

  const restoreVariant = async (variant: LuaVariantEntry) => {
    if (!appid || variant.validationStatus === 'recoveryOnly') return
    setBusyAction(`restore:${variant.id}`)
    setError(null)
    setSuccess(null)
    try {
      const state = await invoke<LuaGameState>('restore_lua_variant', {
        request: { appId: appid, sha256: variant.sha256 },
      })
      onState(state)
      await reload()
      setSuccess(locale === 'vi-VN' ? 'Đã khôi phục Lua variant bằng transaction an toàn.' : 'Restored the Lua variant through a managed transaction.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const pinVariant = async (variant: LuaVariantEntry) => {
    if (!appid) return
    setBusyAction(`pin:${variant.id}`)
    setError(null)
    try {
      setVariants(await invoke<LuaVariantEntry[]>('pin_lua_variant', {
        appId: appid,
        sha256: variant.sha256,
        pinned: !variant.pinned,
      }))
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const exportVariant = async (variant: LuaVariantEntry) => {
    if (!appid) return
    setBusyAction(`export:${variant.id}`)
    setError(null)
    setSuccess(null)
    try {
      const destination = await invoke<string | null>('export_lua_variant', {
        appId: appid,
        sha256: variant.sha256,
      })
      if (destination) {
        setSuccess(locale === 'vi-VN' ? `Đã xuất exact bytes tới ${destination}` : `Exported exact bytes to ${destination}`)
      }
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const scanRuntimeTarget = async () => {
    if (!appid || busyAction) return
    setBusyAction('scanRuntime')
    setError(null)
    setSuccess(null)
    try {
      const scan = await invoke<LuaRuntimeTargetScan>('scan_lua_runtime_target', { appId: appid })
      setRuntimeScan(scan)
      // Checking a game must stay lightweight. Building a GSE/UC plan verifies bundled
      // payload hashes and is intentionally deferred to the explicit preview action.
      setSuccess(locale === 'vi-VN' ? 'Đã kiểm tra game ở chế độ chỉ đọc; chưa có file game nào bị thay đổi.' : 'Game check completed in read-only mode; no game file was changed.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const approveRuntimeTarget = async () => {
    if (!appid || !runtimeScan || busyAction) return
    setBusyAction('approveRuntime')
    setError(null)
    setSuccess(null)
    try {
      const scan = await invoke<LuaRuntimeTargetScan>('approve_lua_runtime_target', {
        appId: appid,
        expectedFingerprint: runtimeScan.approvalFingerprint,
      })
      setRuntimeScan(scan)
      setSuccess(locale === 'vi-VN' ? 'Đã lưu local approval theo fingerprint. Approval sẽ tự hết hạn nếu EXE/DLL thay đổi.' : 'Local fingerprint approval saved. It expires automatically when an EXE or DLL changes.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const revokeRuntimeApproval = async () => {
    if (!appid || busyAction) return
    setBusyAction('revokeRuntime')
    setError(null)
    setSuccess(null)
    try {
      await invoke<boolean>('revoke_lua_runtime_target_approval', { appId: appid })
      const scan = await invoke<LuaRuntimeTargetScan>('scan_lua_runtime_target', { appId: appid })
      setRuntimeScan(scan)
      setSuccess(locale === 'vi-VN' ? 'Đã thu hồi local approval; không có file game nào bị thay đổi.' : 'Local approval revoked; no game file was changed.')
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  // Runtime & compatibility scanning is dormant in dialog UI
  void [scanRuntimeTarget, approveRuntimeTarget, revokeRuntimeApproval, ShieldCheck]

  if (!appid) return null
  const game = manager?.game
  const sourceUnavailable = game?.sourceState === 'unavailable'
  const runtimeActive = game?.runtimeState === 'active'
  const channelChanged = Boolean(game && (
    game.migrationState === 'reviewRequired'
    ||
    selectedChannel !== game.channel
    || (selectedChannel === 'locked' && selectedBuildId !== (game.pinnedBuildId ?? ''))
  ))
  const exactBuildSelected = selectedChannel === 'locked' && Boolean(selectedBuildId)
  const exactBuildChangesSource = exactBuildSelected
    && game?.selectedSource !== 'huggingFace'
  const canSelectLocked = Boolean(
    manager?.canSwitchLocked
    || compatibleBuilds.length > 0
    || game?.channel === 'locked',
  )

  return createPortal(
    <div className="lua-manager-backdrop" role="presentation" onMouseDown={() => !busyAction && onClose()}>
      <section
        ref={dialogRef}
        className="lua-manager-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="lua-manager-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="lua-manager-header">
          <div className="lua-manager-heading-icon"><FolderCog size={20} /></div>
          <div>
            <h2 id="lua-manager-title">{gameName}</h2>
            <p>AppID {appid} · {managerText.title}</p>
          </div>
          <button type="button" onClick={onClose} disabled={Boolean(busyAction)} aria-label={managerText.close}><X size={18} /></button>
        </header>

        {!manager ? (
          <div className="lua-manager-loading"><RefreshCw size={20} className="spin" /> {managerText.loading}</div>
        ) : (
          <div className="lua-manager-body">
            <section className="lua-manager-status-row">
              <div>
                <span>{managerText.runtime}</span>
                <strong className={`state-${game?.runtimeState}`}>
                  {runtimeActive ? <Check size={14} /> : <CircleAlert size={14} />}
                  {runtimeActive ? managerText.activeInSteam : game?.runtimeState}
                </strong>
              </div>
              <div>
                <span>{managerText.source}</span>
                <strong className={`state-${game?.sourceState}`}>
                  {sourceUnavailable
                    ? (runtimeActive ? managerText.sourceUnavailableActive : managerText.noLiveSource)
                    : sourceLabel(game!, managerText.providers)}
                </strong>
              </div>
            </section>

            <section className="lua-manager-section">
              <div className="lua-manager-section-title">
                <div><h3>{managerText.updateChannel}</h3><p>{managerText.updateChannelDescription}</p></div>
              </div>
              <div className="lua-manager-segments" role="group" aria-label="Lua update channel">
                <button
                  type="button"
                  className={selectedChannel === 'live' ? 'active' : ''}
                  disabled={!manager.canSwitchLive && game?.channel !== 'live'}
                  title={!manager.canSwitchLive && game?.channel !== 'live' ? managerText.noLiveSource : undefined}
                  onClick={() => setSelectedChannel('live')}
                >
                  {t.luaShop.liveChannel}
                </button>
                <button
                  type="button"
                  className={selectedChannel === 'locked' ? 'active' : ''}
                  disabled={!canSelectLocked}
                  title={!canSelectLocked ? 'No exact BuildID is available for this game.' : undefined}
                  onClick={() => setSelectedChannel('locked')}
                >
                  {t.luaShop.lockedChannel}
                </button>
              </div>
              {selectedChannel === 'locked' && (
                <div className="lua-manager-build-select" ref={buildMenuRef}>
                  <span>BuildID</span>
                  <button
                    type="button"
                    className="lua-manager-build-trigger"
                    aria-haspopup="listbox"
                    aria-expanded={buildMenuOpen}
                    aria-controls="lua-manager-build-options"
                    onClick={() => setBuildMenuOpen((open) => !open)}
                  >
                    <span className="lua-manager-build-trigger-copy">
                      <strong>
                        {selectedBuild
                          ? selectedBuild.version || `Build ${selectedBuild.build_id}`
                          : selectedBuildId
                            ? `Build ${selectedBuildId}`
                            : (managerText.currentVersion || 'Current version (Pinned)')}
                      </strong>
                      <small>
                        {selectedBuild
                          ? `BuildID ${selectedBuild.build_id}${formatBuildMetadata(selectedBuild) ? ` · ${formatBuildMetadata(selectedBuild)}` : ''}`
                          : selectedBuildId
                            ? `BuildID ${selectedBuildId}`
                            : (managerText.currentVersionDescription || 'Lock manifests at current version')}
                      </small>
                    </span>
                    <ChevronDown size={17} className={buildMenuOpen ? 'is-open' : ''} aria-hidden="true" />
                  </button>
                  {buildMenuOpen && (
                    <div id="lua-manager-build-options" className="lua-manager-build-menu" role="listbox" aria-label="BuildID">
                      <button
                        type="button"
                        role="option"
                        aria-selected={!selectedBuildId}
                        className={`lua-manager-build-option${!selectedBuildId ? ' active' : ''}`}
                        onClick={() => {
                          setSelectedBuildId('')
                          setBuildMenuOpen(false)
                        }}
                      >
                        <span className="lua-manager-build-check" aria-hidden="true">
                          {!selectedBuildId ? <CheckCircle2 size={15} /> : null}
                        </span>
                        <span>
                          <strong>{managerText.currentVersion || 'Current version (Pinned)'}</strong>
                          <small>{managerText.currentVersionDescription || 'Lock manifests at current version'}</small>
                        </span>
                      </button>
                      {compatibleBuilds.map((build) => {
                        const active = build.build_id === selectedBuildId
                        return (
                          <button
                            type="button"
                            role="option"
                            aria-selected={active}
                            className={`lua-manager-build-option${active ? ' active' : ''}`}
                            key={build.build_id}
                            onClick={() => {
                              setSelectedBuildId(build.build_id)
                              setBuildMenuOpen(false)
                            }}
                          >
                            <span className="lua-manager-build-check" aria-hidden="true">
                              {active ? <CheckCircle2 size={15} /> : null}
                            </span>
                            <span>
                              <strong>{build.version || `Build ${build.build_id}`}</strong>
                              <small>BuildID {build.build_id}{formatBuildMetadata(build) ? ` · ${formatBuildMetadata(build)}` : ''}</small>
                            </span>
                          </button>
                        )
                      })}
                    </div>
                  )}
                </div>
              )}
              {exactBuildSelected && (
                <div className={`lua-manager-source-notice${exactBuildChangesSource ? ' changes-source' : ''}`}>
                  <CircleAlert size={15} />
                  <span>
                    {exactBuildChangesSource
                      ? managerText.exactBuildChangesSource.replace(
                        '{source}',
                        sourceLabel(game!, managerText.providers),
                      )
                      : managerText.exactBuildSource}
                  </span>
                </div>
              )}
              <button type="button" className="lua-manager-apply" disabled={!channelChanged || Boolean(busyAction)} onClick={() => void applyChannel()}>
                {busyAction === 'channel'
                  ? managerText.applying
                  : exactBuildSelected
                    ? managerText.applyVersion
                    : managerText.applyChannel}
              </button>
            </section>

            <section className="lua-manager-section lua-drift-panel" aria-labelledby="lua-drift-title">
              <div className="lua-manager-section-title">
                <div>
                  <h3 id="lua-drift-title">{locale === 'vi-VN' ? 'External edit' : 'External edit'}</h3>
                  <p>{locale === 'vi-VN' ? 'Check chỉ hash file Lua thật, không thay đổi nội dung.' : 'Check hashes the real Lua file without changing it.'}</p>
                </div>
                <button type="button" disabled={Boolean(busyAction)} onClick={() => void refreshDrift()}>
                  <RefreshCw className={busyAction === 'checkDrift' ? 'spin' : ''} />
                  Check
                </button>
              </div>
              {drift && (
                <div className={`lua-drift-result${drift.drifted ? ' is-drifted' : ''}`}>
                  <div>
                    {drift.drifted ? <CircleAlert size={16} /> : <CheckCircle2 size={16} />}
                    <span>
                      {drift.drifted
                        ? (locale === 'vi-VN' ? 'File thật đã khác baseline được quản lý.' : 'The real file differs from its managed baseline.')
                        : (locale === 'vi-VN' ? 'Không phát hiện drift.' : 'No drift detected.')}
                    </span>
                    <code title={drift.actualSha256 ?? ''}>{drift.actualSha256?.slice(0, 12) ?? 'missing'}</code>
                    <small>{drift.encodingStatus} · {drift.validLua ? 'valid' : 'recovery only'}</small>
                  </div>
                  {drift.drifted && (
                    <div className="lua-drift-actions">
                      <button type="button" className="primary" disabled={Boolean(busyAction)} onClick={() => void chooseDriftResolution('captureExternalAndApply')}>
                        {locale === 'vi-VN' ? 'Capture external rồi apply' : 'Capture external and apply'}
                      </button>
                      <button type="button" disabled={Boolean(busyAction)} onClick={() => void chooseDriftResolution('keepExternal')}>
                        {locale === 'vi-VN' ? 'Giữ local, bỏ update' : 'Keep local, skip update'}
                      </button>
                      <button type="button" disabled={Boolean(busyAction)} onClick={() => void chooseDriftResolution('restoreManagedAndApply')}>
                        {locale === 'vi-VN' ? 'Restore managed rồi apply' : 'Restore managed and apply'}
                      </button>
                    </div>
                  )}
                </div>
              )}
            </section>

            <section className="lua-manager-section lua-variant-vault" aria-labelledby="lua-variant-vault-title">
              <div className="lua-manager-section-title lua-variant-vault-heading">
                <div>
                  <h3 id="lua-variant-vault-title">Lua Variant Vault</h3>
                  <p>{locale === 'vi-VN' ? 'Lưu exact bytes trước update, đổi provider/channel và restore.' : 'Exact-byte recovery points captured before updates, provider/channel changes and restores.'}</p>
                </div>
                <button type="button" disabled={!manager.fileExists || Boolean(busyAction)} onClick={() => void captureVariant()}>
                  {busyAction === 'captureVariant' ? <RefreshCw className="spin" /> : <Archive />}
                  {locale === 'vi-VN' ? 'Chụp hiện tại' : 'Capture active'}
                </button>
              </div>
              <div className="lua-variant-list">
                {variants.slice(0, 10).map((variant) => (
                  <article
                    key={variant.id}
                    className={`${variant.validationStatus === 'recoveryOnly' ? 'is-recovery-only' : ''}${manager.activeSha256 === variant.sha256 ? ' is-active' : ''}`}
                  >
                    <div>
                      <strong>
                        {variant.provider ?? variant.source ?? 'Local'} · {variant.channel ?? 'raw'}
                        {manager.activeSha256 === variant.sha256 ? ` · ${locale === 'vi-VN' ? 'đang dùng' : 'active'}` : ''}
                      </strong>
                      <span>{new Date(variant.capturedAt).toLocaleString(locale)} · {variant.sha256.slice(0, 12)}</span>
                      <small>
                        {variant.captureReason} · {variant.encodingStatus} · {variant.buildId ?? variant.revision?.slice(0, 16) ?? (locale === 'vi-VN' ? 'không rõ revision' : 'unknown revision')} · {variant.byteLength.toLocaleString(locale)} B
                        {variant.validationStatus === 'recoveryOnly' ? ' · recovery only' : ' · ready to apply'}
                      </small>
                    </div>
                    <div>
                      <button type="button" className={variant.pinned ? 'is-pinned' : ''} disabled={Boolean(busyAction)} aria-label={variant.pinned ? 'Unpin variant' : 'Pin variant'} onClick={() => void pinVariant(variant)}><Pin /></button>
                      <button
                        type="button"
                        disabled={Boolean(busyAction)}
                        title={locale === 'vi-VN' ? 'Xuất exact bytes để phục hồi thủ công' : 'Export exact bytes for manual recovery'}
                        aria-label={locale === 'vi-VN' ? 'Xuất Lua variant' : 'Export Lua variant'}
                        onClick={() => void exportVariant(variant)}
                      >
                        {busyAction === `export:${variant.id}` ? <RefreshCw className="spin" /> : <Download />}
                      </button>
                      <button
                        type="button"
                        disabled={variant.validationStatus === 'recoveryOnly' || Boolean(busyAction)}
                        title={variant.validationStatus === 'recoveryOnly' ? 'Invalid encoding or validation: automatic apply is disabled.' : undefined}
                        onClick={() => void restoreVariant(variant)}
                      >
                        {busyAction === `restore:${variant.id}` ? <RefreshCw className="spin" /> : <Undo2 />}
                        {locale === 'vi-VN' ? 'Khôi phục' : 'Restore'}
                      </button>
                    </div>
                  </article>
                ))}
                {variants.length === 0 ? <p className="lua-variant-empty">{locale === 'vi-VN' ? 'Chưa có variant. Vault sẽ tự capture trước lần ghi đè tiếp theo.' : 'No variants yet. The vault captures automatically before the next overwrite.'}</p> : null}
              </div>
            </section>

            <section className="lua-manager-details">
              <div><span>{managerText.provider}</span><strong>{sourceLabel(game!, managerText.providers)}</strong></div>
              <div><span>{managerText.revision}</span><strong title={game?.sourceRevision ?? ''}>{game?.sourceRevision?.slice(0, 16) || managerText.local}</strong></div>
              <div><span>{managerText.lastSync}</span><strong>{game?.lastSyncAt ? new Date(game.lastSyncAt).toLocaleString() : managerText.notSynced}</strong></div>
              <div><span>{managerText.userOverrides}</span><strong>{manager.hasUserOverrides ? managerText.preserved : managerText.none}</strong></div>
              <div><span>{managerText.luaFile}</span><strong title={manager.luaPath}>{manager.fileExists ? managerText.present : managerText.missing}</strong></div>
              <div><span>{managerText.steamRestart}</span><strong>{game?.requiresSteamRestart ? managerText.required : managerText.notRequired}</strong></div>
            </section>

            {game?.migrationState === 'reviewRequired' && (
              <div className="lua-manager-warning">
                <CircleAlert size={16} />
                <span>{t.luaShop.reviewLegacyHint}</span>
              </div>
            )}

            {game?.sharedDepotConflicts.length ? (
              <div className="lua-manager-warning">
                <CircleAlert size={16} />
                {t.luaShop.sharedDepotLocked.replace('{depots}', game.sharedDepotConflicts.join(', '))}
              </div>
            ) : null}
            {success && <div className="lua-manager-success"><CheckCircle2 size={16} /> {success}</div>}
            {error && <div className="lua-manager-error">{error}</div>}
          </div>
        )}

        <footer className="lua-manager-footer">
          <button type="button" className="danger" disabled={!manager || Boolean(busyAction)} onClick={() => { onClose(); onRemove(String(appid)) }}>
            <Trash2 size={16} /> {t.luaShop.removeFromSteam}
          </button>
          <div>
            {game?.requiresSteamRestart && (
              <button type="button" disabled={Boolean(busyAction)} onClick={() => void onRestartSteam()}>
                <RotateCcw size={16} /> {managerText.restartSteam}
              </button>
            )}
            <button type="button" className="primary" disabled={!manager || Boolean(busyAction)} onClick={() => void sync()}>
              <RefreshCw size={16} className={busyAction === 'sync' ? 'spin' : ''} />
              {game?.updateAvailable ? t.luaShop.update : managerText.changeSource}
            </button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  )
}
