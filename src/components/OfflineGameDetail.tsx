import { Play, FolderInput, Trash2 } from 'lucide-react'
import type { GameSummary } from '../types'
import { useLocale } from '../context/LocaleContext'
import { assetUrlForId } from '../lib/gameMeta'

interface OfflineGameDetailProps {
  game: GameSummary
  assets: Record<string, string>
  onPlay: () => void
  onUninstall: () => void
  onOpenInstallOptions: () => void
  installState: 'none' | 'installing' | 'installed'
}

export function OfflineGameDetail({
  game,
  assets,
  onPlay,
  onUninstall,
  onOpenInstallOptions,
  installState
}: OfflineGameDetailProps) {
  const { t } = useLocale()
  const heroUrl = assetUrlForId(game.heroAssetId, assets)
  const logoUrl = assetUrlForId(game.logoAssetId, assets)

  return (
    <div className="game-detail-container offline-game-detail">
      {heroUrl && (
        <div 
          className="game-detail-hero"
          style={{ backgroundImage: `url("${heroUrl}")` }}
        />
      )}
      
      <div className="game-detail-content" style={{ marginTop: '30vh' }}>
        <div className="game-detail-header">
          {logoUrl ? (
            <img src={logoUrl} alt={game.title} className="game-detail-logo" />
          ) : (
            <h1 className="game-detail-title">{game.title}</h1>
          )}
          <div className="offline-badge" style={{
            display: 'inline-block',
            padding: '4px 8px',
            background: 'rgba(255,107,107,0.2)',
            color: '#ff6b6b',
            borderRadius: '4px',
            fontSize: '12px',
            fontWeight: 'bold',
            marginTop: '12px'
          }}>
            OFFLINE MODE
          </div>
        </div>

        <div className="game-detail-actions">
          {installState === 'installed' ? (
            <button className="primary-action play-btn" onClick={onPlay}>
              <Play size={20} fill="currentColor" />
              <span>{t.library.play}</span>
            </button>
          ) : installState === 'none' ? (
            <button className="primary-action install-btn" onClick={onOpenInstallOptions}>
              <FolderInput size={20} />
              <span>{t.library.install}</span>
            </button>
          ) : (
            <button className="primary-action" disabled>
              <span>Downloading...</span>
            </button>
          )}

          {installState === 'installed' && (
            <button className="secondary-action" onClick={onUninstall} title={t.library.uninstall}>
              <Trash2 size={20} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
