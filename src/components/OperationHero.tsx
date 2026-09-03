import { CheckCircle2, Download, Image as ImageIcon, Play } from 'lucide-react'
import { useLocale } from '../context/locale'
import { formatBytes } from '../lib/format'
import { assetUrlForId, firstMediaUrl, isTauriRuntime } from '../lib/gameMeta'
import type { GameDetail, GameSummary } from '../types'

export type OperationHeroProps = {
  game: GameSummary
  detail: GameDetail
  assets: Record<string, string>
  currentVersion: string
  latestVersion: string
  updateReady: boolean
  showVersionAction: boolean
  updateSize: number
  onUpdate: () => void
  onPlay: () => void
  onStop: () => void
  isJobRunning: boolean
  isGameRunning: boolean
  canUpdate: boolean
  installMode: boolean
  selectedVersion: string
}

export function OperationHero({
  game,
  detail,
  assets,
  currentVersion,
  latestVersion,
  updateReady,
  showVersionAction,
  updateSize,
  onUpdate,
  onPlay,
  onStop,
  isJobRunning,
  isGameRunning,
  canUpdate,
  installMode,
  selectedVersion,
}: OperationHeroProps) {
  const { t } = useLocale()
  const hero = assetUrlForId(game.heroAssetId, assets) || firstMediaUrl(detail, assets)
  const icon = assetUrlForId(game.iconAssetId, assets)
  const stateLabel = installMode
    ? t.library.readyToInstall
    : updateReady
      ? t.library.readyToUpdate
      : t.library.readyToPlay

  let playLabel = t.library.play.toUpperCase()
  let playClass = 'update-button hero-play-button'
  if (isGameRunning) {
    playLabel = 'RUNNING'
    playClass = 'update-button running-btn can-stop'
  } else if (isJobRunning) {
    playLabel = 'DOWNLOADING'
    playClass = 'update-button downloading-btn'
  }

  const updateDisabled = isGameRunning || isJobRunning || !canUpdate

  return (
    <section className="hero-panel">
      {hero ? <img src={hero} alt="" loading="eager" fetchPriority="high" decoding="async" /> : null}
      <div className="game-strip">
        <div className="game-emblem">
          {icon ? <img src={icon} alt="" decoding="async" loading="lazy" /> : <ImageIcon size={28} />}
        </div>
        <div>
          <h1>{game.title}</h1>
          <div className="version-row">
            <VersionStat label={t.library.currentVersion} value={currentVersion} />
            <VersionStat label={t.library.latestVersion} value={latestVersion} highlight />
            <VersionStat label={t.library.targetVersion} value={selectedVersion} />
            <div className="ready-state">
              <CheckCircle2 size={20} />
              <span>{stateLabel}</span>
              <small>{formatBytes(updateSize)}</small>
            </div>
          </div>
        </div>
        <div className="hero-action-group">
          {installMode ? (
            <button
              className={`update-button${isJobRunning ? ' downloading-btn' : ''}`}
              type="button"
              onClick={onUpdate}
              disabled={isJobRunning || !canUpdate}
            >
              <span>{isJobRunning ? 'DOWNLOADING' : (!isTauriRuntime() ? 'REMOTE INSTALL' : t.library.chooseInstall.toUpperCase())}</span>
              <Download size={18} />
            </button>
          ) : (
            <>
              <button
                className={playClass}
                type="button"
                onClick={isGameRunning ? onStop : onPlay}
                disabled={isJobRunning}
                data-stop-label={isGameRunning ? 'STOP' : undefined}
              >
                <span>{playLabel}</span>
                {isJobRunning ? <Download size={18} /> : <Play size={18} />}
              </button>
              {showVersionAction ? (
                <button className="update-button" type="button" onClick={onUpdate} disabled={updateDisabled}>
                  <span>{updateReady ? t.library.update.toUpperCase() : 'VERSIONS'}</span>
                  <Download size={18} />
                </button>
              ) : null}
            </>
          )}
        </div>
      </div>
    </section>
  )
}

function VersionStat({ label, value, highlight = false }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="version-stat">
      <small>{label}</small>
      <strong className={highlight ? 'gold-text' : ''}>{value}</strong>
    </div>
  )
}
