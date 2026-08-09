import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { CheckCircle2, ChevronRight, HardDrive, KeyRound, Loader2, X } from 'lucide-react'
import type { GameCatalog, GameInstallState, GameSummary } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { useSteamAppIds } from '../hooks/useSteamAppIds'
import { useLocale } from '../context/locale'
import { DenuvoActivationButton } from './DenuvoActivation'
import '../App.css'

const DENUVO_GAME_IDS = ['ea-sports-fc-26']

type OfflineSelection = {
  game: GameSummary
  appid?: number
  installed: boolean
}

export function OfflineActivation({
  catalog,
  assets
}: {
  catalog: GameCatalog
  assets: Record<string, string>
}) {
  const { t } = useLocale()
  const [installStates, setInstallStates] = useState<Record<string, GameInstallState>>({})
  const [selected, setSelected] = useState<OfflineSelection | null>(null)
  const [isResolving, setIsResolving] = useState(false)
  const { mapping } = useSteamAppIds()
  const offlineGames = catalog.games.filter((game) => DENUVO_GAME_IDS.includes(game.id))

  useEffect(() => {
    const gameIds = catalog.games
      .filter((game) => DENUVO_GAME_IDS.includes(game.id))
      .map((game) => game.id)
    if (gameIds.length === 0) return
    invoke<GameInstallState[]>('get_game_install_states', { gameIds })
      .then((states) => {
        setInstallStates(Object.fromEntries(states.map((state) => [state.gameId, state])))
      })
      .catch((error) => console.error('Unable to load offline activation install states:', error))
  }, [catalog.games])

  async function handleSelectGame(game: GameSummary) {
    const rawAppid = mapping[game.id] ?? game.appid
    const appid = rawAppid ? Number(rawAppid) : undefined

    setIsResolving(true)
    try {
      const launcherState = await invoke<GameInstallState>('get_game_install_state', { gameId: game.id })
      const installed = launcherState.installed && Boolean(launcherState.installPath)
      setInstallStates((current) => ({ ...current, [game.id]: launcherState }))
      setSelected({ game, appid, installed })
    } catch (error) {
      console.error('Unable to resolve offline activation game path:', error)
      setSelected({ game, appid, installed: false })
    } finally {
      setIsResolving(false)
    }
  }

  return (
    <div className="offline-activation-panel">
      <header className="offline-activation-header">
        <h1>{t.nav.offlineActivation}</h1>
        <p>{t.offlineActivation.description}</p>
      </header>

      <div className="offline-activation-grid">
        {offlineGames.map((game) => {
          const rawAppid = mapping[game.id] ?? game.appid
          const appid = rawAppid ? Number(rawAppid) : undefined
          const isInstalled = Boolean(installStates[game.id]?.installed && installStates[game.id]?.installPath)
          const hero = assetUrlForId(game.heroAssetId, assets)

          return (
            <button
              key={game.id}
              type="button"
              className="offline-activation-card"
              onClick={() => handleSelectGame(game)}
              disabled={isResolving}
            >
              <div className="offline-card-art">
                {hero && <img src={hero} alt="" />}
                <span className={`offline-card-state${isInstalled ? ' is-installed' : ''}`}>
                  {isInstalled ? <CheckCircle2 size={13} /> : <HardDrive size={13} />}
                  {isInstalled ? t.offlineActivation.installed : t.offlineActivation.notInstalled}
                </span>
              </div>
              <div className="offline-card-body">
                <strong>{game.title}</strong>
                {appid && <small>AppID: {appid}</small>}
                <span className="offline-card-open">
                  {t.offlineActivation.openDetails}
                  <ChevronRight size={15} />
                </span>
              </div>
            </button>
          )
        })}
      </div>

      {isResolving && (
        <div className="offline-resolving" role="status">
          <Loader2 size={18} className="spin" /> {t.offlineActivation.checkingInstall}
        </div>
      )}

      {selected && (
        <div className="offline-detail-overlay" onClick={() => setSelected(null)}>
          <section className="offline-detail-panel" onClick={(event) => event.stopPropagation()}>
            <button
              type="button"
              className="offline-detail-close"
              aria-label={t.help.close}
              onClick={() => setSelected(null)}
            >
              <X size={18} />
            </button>

            <div className="offline-detail-heading">
              <span className="offline-detail-icon"><KeyRound size={18} /></span>
              <div>
                <h2>{selected.game.title}</h2>
                <p>{selected.appid ? `AppID: ${selected.appid}` : t.offlineActivation.unknownAppId}</p>
              </div>
            </div>

            <div className={`offline-detail-status${selected.installed ? ' is-ready' : ''}`}>
              {selected.installed ? <CheckCircle2 size={17} /> : <HardDrive size={17} />}
              <div>
                <strong>{selected.installed ? t.offlineActivation.readyTitle : t.offlineActivation.missingTitle}</strong>
                <span>{selected.installed ? t.offlineActivation.readyBody : t.offlineActivation.missingBody}</span>
              </div>
            </div>

            {selected.installed ? (
              <div className="offline-detail-action">
                <DenuvoActivationButton gameId={selected.game.id} />
              </div>
            ) : (
              <div className="offline-detail-path-hint">{t.offlineActivation.installHint}</div>
            )}
          </section>
        </div>
      )}
    </div>
  )
}
