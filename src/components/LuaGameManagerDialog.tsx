import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Check, CheckCircle2, ChevronDown, CircleAlert, FolderCog, RefreshCw, RotateCcw, Trash2, X } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { useLocale } from '../context/locale'
import type {
  LuaGameChannel,
  LuaGameManagerState,
  LuaGameState,
} from '../types'

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
  const { t } = useLocale()
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
    const state = await invoke<LuaGameManagerState>('get_lua_game_manager_state', { appid })
    setManager(state)
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
    ]).then(([managerResult, buildsResult]) => {
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
    if (selectedChannel === 'locked' && !selectedBuildId && !reviewingLegacy) {
      setError(managerText.chooseBuildError)
      return
    }
    setBusyAction('channel')
    setError(null)
    try {
      if (selectedChannel === 'live') {
        await onSwitchLive(String(appid))
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
          buildId: selectedChannel === 'locked' ? selectedBuildId : null,
          conflictResolution: null,
          provider: null,
        },
      })
      onState(state)
      await reload()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

  const sync = async () => {
    if (!appid) return
    setBusyAction('sync')
    setError(null)
    try {
      await onSync(String(appid))
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusyAction(null)
    }
  }

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
                <button type="button" className={selectedChannel === 'locked' ? 'active' : ''} onClick={() => setSelectedChannel('locked')}>
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
                      <strong>{selectedBuild ? selectedBuild.version || `Build ${selectedBuild.build_id}` : managerText.chooseBuild}</strong>
                      <small>{selectedBuild ? `BuildID ${selectedBuild.build_id}${formatBuildMetadata(selectedBuild) ? ` · ${formatBuildMetadata(selectedBuild)}` : ''}` : managerText.chooseBuild}</small>
                    </span>
                    <ChevronDown size={17} className={buildMenuOpen ? 'is-open' : ''} aria-hidden="true" />
                  </button>
                  {buildMenuOpen && (
                    <div id="lua-manager-build-options" className="lua-manager-build-menu" role="listbox" aria-label="BuildID">
                      {compatibleBuilds.length === 0 ? (
                        <div className="lua-manager-build-empty">{managerText.chooseBuildError}</div>
                      ) : compatibleBuilds.map((build) => {
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
              <button type="button" className="lua-manager-apply" disabled={!channelChanged || Boolean(busyAction)} onClick={() => void applyChannel()}>
                {busyAction === 'channel' ? managerText.applying : managerText.applyChannel}
              </button>
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
              {game?.updateAvailable ? t.luaShop.update : t.luaShop.sync}
            </button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  )
}
