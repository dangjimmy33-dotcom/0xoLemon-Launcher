import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, CloudOff, FolderSync, TriangleAlert } from 'lucide-react'
import type { CloudSaveStatus, GameCatalog, GameInstallState } from '../types'
import { isTauriRuntime } from '../lib/gameMeta'

export function CloudSavesOverview({
  catalog,
  installStates,
  assets,
  onOpenGame,
  onRequestAsset,
}: {
  catalog: GameCatalog
  installStates: Record<string, GameInstallState>
  assets: Record<string, string>
  onOpenGame: (gameId: string) => void
  onRequestAsset: (gameId: string, assetId: string, urgent?: boolean) => void
}) {
  const installed = useMemo(
    () => catalog.games.filter((game) => installStates[game.id]?.installed),
    [catalog.games, installStates],
  )
  const installedIds = useMemo(() => installed.map((game) => game.id).join('|'), [installed])
  const [statuses, setStatuses] = useState<Record<string, CloudSaveStatus>>({})

  useEffect(() => {
    for (const game of installed) onRequestAsset(game.id, game.gridAssetId)
    if (!isTauriRuntime()) return
    let disposed = false
    Promise.all(
      installed.map(async (game) => {
        const status = await invoke<CloudSaveStatus>('get_cloud_save_status', { gameId: game.id })
        return [game.id, status] as const
      }),
    )
      .then((entries) => {
        if (!disposed) setStatuses(Object.fromEntries(entries))
      })
      .catch(() => undefined)
    return () => {
      disposed = true
    }
  }, [installed, installedIds, onRequestAsset])

  return (
    <section className="cloud-overview">
      <header>
        <div className="cloud-overview-icon"><Cloud size={22} /></div>
        <div>
          <h1>Cloud Saves</h1>
          <p>Backup status and conflicts for installed games.</p>
        </div>
      </header>
      {installed.length === 0 ? (
        <div className="cloud-overview-empty">
          <CloudOff size={28} />
          <strong>No installed games</strong>
          <span>Cloud save controls become available from each installed game in Library.</span>
        </div>
      ) : (
        <div className="cloud-overview-list">
          {installed.map((game) => {
            const status = statuses[game.id]
            const hasConflict = Boolean(status?.conflicts.length)
            return (
              <button type="button" key={game.id} onClick={() => onOpenGame(game.id)}>
                {assets[game.gridAssetId] ? <img src={assets[game.gridAssetId]} alt="" /> : <div />}
                <span>
                  <strong>{game.title}</strong>
                  <small>{status?.lastMessage || (status?.enabled ? 'Cloud save ready' : 'Cloud save is disabled')}</small>
                </span>
                <em className={hasConflict ? 'is-conflict' : status?.enabled ? 'is-ready' : ''}>
                  {hasConflict ? <TriangleAlert size={15} /> : <FolderSync size={15} />}
                  {hasConflict ? `${status.conflicts.length} conflict${status.conflicts.length === 1 ? '' : 's'}` : status?.enabled ? 'Enabled' : 'Disabled'}
                </em>
              </button>
            )
          })}
        </div>
      )}
    </section>
  )
}
