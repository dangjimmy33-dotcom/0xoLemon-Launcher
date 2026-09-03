import {
  Download,
  FolderOpen,
  Heart,
  Library,
  Play,
  RefreshCcw,
  Settings2,
  ShieldCheck,
  Square,
  Trash2,
} from 'lucide-react'
import { useLocale } from '../../context/locale'
import type { GameDetail, GameInstallState, GameSummary, VerifyUiStatus } from '../../types'
import { formatBytes } from '../../lib/format'
import { SteamLibraryActivity } from './SteamLibraryActivity'

type SteamLibraryDetailProps = {
  game: GameSummary
  detail: GameDetail
  assets: Record<string, string>
  installState?: GameInstallState
  displayedVersion: string
  heroUrl?: string
  logoUrl?: string
  coverUrl?: string
  downloadSize: number
  installed: boolean
  updateReady: boolean
  installing: boolean
  playing: boolean
  verifying: boolean
  installBlocked: boolean
  favorite: boolean
  showVersionAction: boolean
  verifyStatus: VerifyUiStatus | null
  onInstall: () => void
  onPlay: () => void
  onStop: () => void
  onUpdate: () => void
  onVersions: () => void
  onVerify: () => void
  onBrowse: () => void
  onUninstall: () => void
  onToggleFavorite: () => void
  onOpenStore: () => void
}

export function SteamLibraryDetail({
  game,
  detail,
  assets,
  installState,
  displayedVersion,
  heroUrl,
  logoUrl,
  coverUrl,
  downloadSize,
  installed,
  updateReady,
  installing,
  playing,
  verifying,
  installBlocked,
  favorite,
  showVersionAction,
  verifyStatus,
  onInstall,
  onPlay,
  onStop,
  onUpdate,
  onVersions,
  onVerify,
  onBrowse,
  onUninstall,
  onToggleFavorite,
  onOpenStore,
}: SteamLibraryDetailProps) {
  const { t } = useLocale()
  const primary = playing
    ? { label: 'Stop', icon: <Square size={18} fill="currentColor" />, action: onStop, state: 'stop' }
    : installing
      ? { label: 'Downloading', icon: <Download size={19} />, action: onInstall, state: 'busy' }
      : installed && updateReady
        ? { label: t.library.update, icon: <RefreshCcw size={19} />, action: onUpdate, state: 'update' }
        : installed
          ? { label: t.library.play, icon: <Play size={20} fill="currentColor" />, action: onPlay, state: 'play' }
          : { label: t.library.chooseInstall, icon: <Download size={19} />, action: onInstall, state: 'install' }
  const primaryDisabled = installing || installBlocked
  const features = detail.categories.slice(0, 4)
  const verificationText = verifying
    ? `${Math.round((verifyStatus?.percent ?? 0) * 100)}%`
    : t.library.verifyIntegrity

  return (
    <main className="steam-library-detail-surface" aria-label={`${game.title} library page`}>
      <section className="steam-library-detail-hero">
        {heroUrl ? (
          <img className="steam-library-detail-hero-art" src={heroUrl} alt="" loading="eager" decoding="async" />
        ) : (
          <div className="steam-library-detail-hero-placeholder"><Library size={48} /></div>
        )}
        <div className="steam-library-detail-hero-vignette" />
        <div className="steam-library-detail-brand">
          {logoUrl ? <img src={logoUrl} alt={game.title} loading="eager" decoding="async" /> : <h1>{game.title}</h1>}
        </div>
      </section>

      <section className="steam-library-detail-actionbar" aria-label="Game actions">
        <button
          type="button"
          className="steam-library-primary-action"
          data-action-state={primary.state}
          disabled={primaryDisabled}
          onClick={primary.action}
        >
          {primary.icon}<span>{primary.label}</span>
        </button>
        <div className="steam-library-action-summary">
          <strong>{game.title}</strong>
          <span>
            {installed ? `Installed ${displayedVersion}` : `In Library · ${displayedVersion}`}
            {downloadSize > 0 ? ` · ${formatBytes(downloadSize)}` : ''}
          </span>
        </div>
        <div className="steam-library-action-tools">
          {showVersionAction && installed ? (
            <button type="button" onClick={onVersions} disabled={installBlocked || installing} title="Manage versions">
              <Settings2 size={18} /><span>Versions</span>
            </button>
          ) : null}
          <button type="button" onClick={onVerify} disabled={!installed || verifying || installBlocked} title={verificationText}>
            <ShieldCheck size={18} /><span>{verificationText}</span>
          </button>
          <button type="button" onClick={onBrowse} disabled={!installed || installBlocked} title="Browse local files">
            <FolderOpen size={18} /><span>Browse</span>
          </button>
          <button
            type="button"
            className={favorite ? 'is-favorite' : ''}
            onClick={onToggleFavorite}
            aria-pressed={favorite}
            title={favorite ? 'Remove from favorites' : 'Add to favorites'}
          >
            <Heart size={18} fill={favorite ? 'currentColor' : 'none'} />
            <span>{favorite ? 'Favorite' : 'Add favorite'}</span>
          </button>
        </div>
      </section>

      <section className="steam-library-detail-overview">
        {coverUrl ? <img className="steam-library-detail-cover" src={coverUrl} alt="" loading="eager" decoding="async" /> : null}
        <div className="steam-library-detail-description">
          <p>{detail.shortDescription || game.subtitle}</p>
          <dl>
            <div><dt>Developer</dt><dd>{detail.developers.join(', ') || game.developer}</dd></div>
            <div><dt>Publisher</dt><dd>{detail.publishers.join(', ') || game.publisher}</dd></div>
            <div><dt>Release date</dt><dd>{detail.releaseDate || 'Available now'}</dd></div>
          </dl>
        </div>
        <div className="steam-library-detail-features" aria-label="Game features">
          {(features.length > 0 ? features : ['Launcher library']).map((feature) => <span key={feature}>{feature}</span>)}
        </div>
      </section>

      <nav className="steam-library-detail-links" aria-label="Game links">
        <button type="button" onClick={onOpenStore}>Store Page</button>
        <span>Community Hub</span>
        <span>Discussions</span>
        <span>Guides</span>
        <span>Support</span>
      </nav>

      <SteamLibraryActivity
        game={game}
        detail={detail}
        assets={assets}
        installState={installState}
        displayedVersion={displayedVersion}
      />

      {installed ? (
        <footer className="steam-library-detail-danger-zone">
          <button type="button" onClick={onUninstall} disabled={installBlocked || installing || playing}>
            <Trash2 size={15} /> {t.library.uninstall}
          </button>
        </footer>
      ) : null}
    </main>
  )
}
