import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { CheckCircle2, ChevronRight, HardDrive, KeyRound, Loader2, X } from 'lucide-react'
import type { GameCatalog, GameInstallState, GameSummary } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { useSteamAppIds } from '../hooks/useSteamAppIds'
import { useLocale } from '../context/LocaleContext'
import { DenuvoActivationButton } from './DenuvoActivation'
import '../App.css'

const DENUVO_GAME_IDS = ['ea-sports-fc-26']

type OfflineSelection = {
  game: GameSummary
  appid?: number
  gameDir: string
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
  const [installedApps, setInstalledApps] = useState<number[]>([])
  const [selected, setSelected] = useState<OfflineSelection | null>(null)
  const [isResolving, setIsResolving] = useState(false)
  const { mapping } = useSteamAppIds()

  useEffect(() => {
    invoke<number[]>('get_installed_steam_apps')
      .then(setInstalledApps)
      .catch(console.error)
  }, [])

  const offlineGames = catalog.games.filter((game) => DENUVO_GAME_IDS.includes(game.id))

  async function handleSelectGame(game: GameSummary) {
    const rawAppid = mapping[game.id] ?? game.appid
    const appid = rawAppid ? Number(rawAppid) : undefined
    const installed = Boolean(appid && installedApps.includes(appid))

    setIsResolving(true)
    try {
      let gameDir = ''

      // Prefer the launcher's registered install state, then fall back to the
      // Steam library path so games installed directly through Steam still work.
      try {
        const launcherState = await invoke<GameInstallState>('get_game_install_state', { gameId: game.id })
        if (launcherState.installed && launcherState.installPath) {
          gameDir = launcherState.installPath
        }
      } catch {
        // The Steam path fallback below covers games not managed by the launcher.
      }

      if (!gameDir && appid && installed) {
        gameDir = (await invoke<string | null>('get_steam_game_install_dir', { appid })) || ''
      }
      setSelected({ game, appid, gameDir, installed: Boolean(gameDir) })
    } catch (error) {
      console.error('Unable to resolve offline activation game path:', error)
      setSelected({ game, appid, gameDir: '', installed: false })
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
          const isInstalled = Boolean(appid && installedApps.includes(appid))
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

            {selected.installed && selected.gameDir ? (
              <div className="offline-detail-action">
                <DenuvoActivationButton
                  gameDir={selected.gameDir}
                  cfgPath={`${selected.gameDir}\\anadius.cfg`}
                  gameId={selected.game.id}
                />
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
