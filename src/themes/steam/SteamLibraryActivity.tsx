import { Clock3, FileText, Trophy } from 'lucide-react'
import type { GameDetail, GameInstallState, GameSummary } from '../../types'
import { assetUrlForId } from '../../lib/gameMeta'

type SteamLibraryActivityProps = {
  game: GameSummary
  detail: GameDetail
  assets: Record<string, string>
  installState?: GameInstallState
  displayedVersion: string
}

export function SteamLibraryActivity({
  game,
  detail,
  assets,
  installState,
  displayedVersion,
}: SteamLibraryActivityProps) {
  const achievements = detail.achievements
    .map((achievement) => ({
      ...achievement,
      iconUrl: assetUrlForId(achievement.iconAssetId, assets),
    }))
    .filter((achievement) => Boolean(achievement.iconUrl))
    .slice(0, 7)
  const isInstalled = Boolean(installState?.installed)
  const eventTitle = isInstalled
    ? `${game.title} is ready to play`
    : `${game.title} is in your library`
  const eventDescription = isInstalled
    ? `Version ${displayedVersion} is installed and available from this library.`
    : 'Choose Install when you are ready. Download and installation progress will appear in Downloads.'

  return (
    <section className="steam-library-activity-layout" aria-label={`${game.title} library activity`}>
      <div className="steam-library-activity-main">
        <h2 className="steam-library-section-heading">Activity</h2>
        <div className="steam-library-activity-composer">
          Say something about this game to your friends...
        </div>

        <article className="steam-library-activity-card">
          <header>
            <span>Library activity</span>
            <time>{detail.releaseDate || 'Available now'}</time>
          </header>
          <div className="steam-library-activity-event">
            <div className="steam-library-activity-icon" aria-hidden="true">
              <Clock3 size={22} />
            </div>
            <div>
              <h3>{eventTitle}</h3>
              <p>{eventDescription}</p>
              {detail.shortDescription ? <small>{detail.shortDescription}</small> : null}
            </div>
          </div>
        </article>
      </div>

      <aside className="steam-library-activity-sidebar" aria-label="Game summary">
        <section className="steam-library-side-panel steam-library-achievements-panel">
          <header>
            <div>
              <Trophy size={16} />
              <strong>Achievements</strong>
            </div>
            <span>{detail.achievements.length} total</span>
          </header>
          {achievements.length > 0 ? (
            <div className="steam-library-achievement-strip">
              {achievements.map((achievement) => (
                <img
                  key={achievement.id}
                  src={achievement.iconUrl}
                  alt={achievement.name}
                  title={achievement.name}
                  loading="lazy"
                  decoding="async"
                />
              ))}
              {detail.achievements.length > achievements.length ? (
                <span>+{detail.achievements.length - achievements.length}</span>
              ) : null}
            </div>
          ) : (
            <p className="steam-library-side-empty">No achievement data is available for this game.</p>
          )}
        </section>

        <section className="steam-library-side-panel steam-library-notes-panel">
          <header>
            <div>
              <FileText size={16} />
              <strong>Notes</strong>
            </div>
          </header>
          <p className="steam-library-note-empty">No notes for this game.</p>
        </section>

        <section className="steam-library-side-panel steam-library-game-info-panel">
          <header><strong>Game information</strong></header>
          <dl>
            <div><dt>Developer</dt><dd>{detail.developers.join(', ') || game.developer}</dd></div>
            <div><dt>Publisher</dt><dd>{detail.publishers.join(', ') || game.publisher}</dd></div>
            <div><dt>Version</dt><dd>{displayedVersion}</dd></div>
          </dl>
        </section>
      </aside>
    </section>
  )
}
