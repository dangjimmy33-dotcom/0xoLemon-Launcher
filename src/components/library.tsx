import { useEffect, useMemo, useRef, useState, cloneElement } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { BookOpen, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, Download, FolderOpen, HardDrive, Image as ImageIcon, Library, Play, RefreshCcw, Search, ShieldCheck, ShoppingBag, Trophy, X } from 'lucide-react'
import { TutorialModal } from './TutorialModal'
import { enUS as t } from '../i18n/en-US'
import type { CloudSaveStatus, GameAchievement, GameCatalog, GameDetail, GameSummary, GameInstallState, GameVersionInfo, VerifyUiStatus } from '../types'
import { assetUrlForId, firstMediaUrl, isCarouselMedia, mediaPriority, processDescriptionHtml, isTauriRuntime } from '../lib/gameMeta'
import { formatBytes } from '../lib/format'
import { getGameTags, gameHasTag } from '../lib/gameTags'
import { GameDetailsPanel, InstallSummaryPanel } from './panels'
import { CloudSavePanel } from './CloudSavePanel'
import { GameChat } from './GameChat'
import { useRealtimeConfig } from '../hooks/useRealtimeConfig'
import { useFirestoreDetail } from '../hooks/useFirestoreDetail'

function LazyGameCardImage({
  game,
  assetId,
  url,
  variant,
  onRequestAsset,
}: {
  game: GameSummary
  assetId: string | undefined
  url: string | undefined
  variant: 'compact' | 'browse'
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (url || !assetId) return
    const el = ref.current
    if (!el || typeof IntersectionObserver === 'undefined') {
      onRequestAsset(game, assetId)
      return
    }
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        onRequestAsset(game, assetId)
        observer.disconnect()
      }
    }, { rootMargin: '260px 0px 260px 0px', threshold: 0.01 })
    observer.observe(el)
    return () => observer.disconnect()
  }, [assetId, game, onRequestAsset, url])

  if (url) {
    return <img src={url} alt="" loading="lazy" />
  }

  return (
    <div className="asset-placeholder" ref={ref}>
      <ImageIcon size={variant === 'browse' ? 34 : 26} />
    </div>
  )
}

function CatalogLoadingView({ viewMode }: { viewMode: 'store' | 'library' }) {
  return (
    <section className="library-browse-view library-loading-view" aria-busy="true" aria-label={`Loading ${viewMode}`}>
      <header className="library-browse-toolbar">
        <div className="library-browse-heading">
          <strong>{viewMode === 'store' ? t.nav.store : t.nav.library}</strong>
        </div>
        <div className="library-search-skeleton" aria-hidden="true" />
      </header>
      <div className="library-browse-grid" aria-hidden="true">
        {Array.from({ length: 6 }, (_, index) => (
          <div className="library-card-skeleton" key={index}>
            <div />
            <span />
            <small />
          </div>
        ))}
      </div>
    </section>
  )
}

function CatalogUnavailableView({ viewMode, onRetry }: { viewMode: 'store' | 'library'; onRetry: () => void }) {
  return (
    <section className="library-browse-view">
      <header className="library-browse-toolbar">
        <div className="library-browse-heading">
          <strong>{viewMode === 'store' ? t.nav.store : t.nav.library}</strong>
        </div>
      </header>
      <div className="library-unavailable">
        <CircleAlert size={28} />
        <strong>{viewMode === 'store' ? 'Store unavailable' : 'Library unavailable'}</strong>
        <span>Please try again.</span>
        <button type="button" onClick={onRetry}>
          <RefreshCcw size={16} />
          Try again
        </button>
      </div>
    </section>
  )
}

function GameDetailLoadingView({
  game,
  assets,
  onBack,
}: {
  game: GameSummary
  assets: Record<string, string>
  onBack: () => void
}) {
  const hero = assetUrlForId(game.heroAssetId, assets)
  const icon = assetUrlForId(game.iconAssetId, assets) || assetUrlForId(game.gridAssetId, assets)

  return (
    <section className="game-detail-loading-view" aria-busy="true" aria-label={`Opening ${game.title}`}>
      <button className="back-to-library" type="button" onClick={onBack}>
        <ChevronLeft size={16} />
        Back
      </button>
      <div className="detail-loading-layout" aria-hidden="true">
        <div className="detail-loading-main">
          <div className={`detail-loading-hero${hero ? ' has-image' : ''}`}>
            {hero ? <img src={hero} alt="" /> : null}
            <div className="detail-loading-shade" />
            <div className="detail-loading-title">
              {icon ? <img src={icon} alt="" /> : <div className="detail-loading-icon" />}
              <div>
                <strong>{game.title}</strong>
                <span />
              </div>
            </div>
          </div>
          <div className="detail-loading-row">
            <span />
            <span />
          </div>
        </div>
        <aside className="detail-loading-side">
          <div />
          <div />
          <div />
        </aside>
      </div>
    </section>
  )
}

function HoverCardPopup({
  game,
  assets,
  pos,
  onRequestAsset,
}: {
  game: GameSummary
  assets: Record<string, string>
  pos: { top: number; left: number; right: number; alignRight: boolean }
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
}) {
  const detail = useFirestoreDetail(game.id)
  
  const videoMedia = detail?.media?.find(
    (m) => m.mimeType?.startsWith('video/') || m.role?.startsWith('video'),
  )
  const videoAssetId = videoMedia?.assetId

  const thumbMedia = detail?.media?.find(
    (m) => m.role === 'video-thumb' || m.role === 'video-thumbnail' || m.role === 'video-poster',
  )
  const thumbAssetId = thumbMedia?.assetId

  useEffect(() => {
    if (videoAssetId) {
      onRequestAsset(game, videoAssetId, true)
    }
    if (thumbAssetId) {
      onRequestAsset(game, thumbAssetId, true)
    }
    onRequestAsset(game, game.heroAssetId, true)
  }, [game, videoAssetId, thumbAssetId, onRequestAsset])

  const videoUrl = videoAssetId ? assetUrlForId(videoAssetId, assets) : null
  const thumbUrl = thumbAssetId ? assetUrlForId(thumbAssetId, assets) : null
  const hero = assetUrlForId(game.heroAssetId, assets)
  const posterUrl = thumbUrl || hero || undefined
  const tags = getGameTags(game)

  const style: React.CSSProperties = {
    position: 'absolute',
    top: pos.top,
    zIndex: 9999,
  }
  if (pos.alignRight) {
    style.right = pos.right
  } else {
    style.left = pos.left
  }
  
  const description = detail?.shortDescription || game.subtitle || ''

  return (
    <div className="hover-card-portal" style={style}>
      <div className="hover-card-media">
        {videoUrl ? (
          <video src={videoUrl} autoPlay loop muted playsInline poster={posterUrl} />
        ) : hero ? (
          <img src={hero} alt="" />
        ) : (
          <div className="hover-card-placeholder" />
        )}
      </div>
      <div className="hover-card-info">
        <div className="hover-card-header">
          <strong>{game.title}</strong>
        </div>
        <div className="hover-card-dev">{game.developer}</div>
        <div className="hover-card-tags">
          {tags.map((t) => (
            <i key={t.id} className={`tone-${t.tone}`}>
              {t.label}
            </i>
          ))}
        </div>
        {description ? <p className="hover-card-desc">{description}</p> : null}
      </div>
    </div>
  )
}

function GameHoverCard({
  game,
  assets,
  onRequestAsset,
  children,
}: {
  game: GameSummary
  assets: Record<string, string>
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  children: React.ReactElement
}) {
  const [hovered, setHovered] = useState(false)
  const [show, setShow] = useState(false)
  const [pos, setPos] = useState({ top: 0, left: 0, right: 0, alignRight: false })
  const anchorRef = useRef<HTMLElement>(null)

  useEffect(() => {
    if (!hovered) {
      setShow(false)
      return
    }
    const timer = setTimeout(() => {
      if (anchorRef.current) {
        const rect = anchorRef.current.getBoundingClientRect()
        const spaceRight = window.innerWidth - rect.right
        const spaceLeft = rect.left
        const alignRight = spaceRight < 340 && spaceLeft > 340
        setPos({
          top: rect.top + window.scrollY,
          left: rect.right + 10,
          right: window.innerWidth - rect.left + 10,
          alignRight,
        })
      }
      setShow(true)
    }, 600)
    return () => clearTimeout(timer)
  }, [hovered])

  const clonedChild = cloneElement(children, {
    ref: anchorRef,
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
  } as any)

  return (
    <>
      {clonedChild}
      {show && createPortal(<HoverCardPopup game={game} assets={assets} pos={pos} onRequestAsset={onRequestAsset} />, document.body)}
    </>
  )
}

import type { DiscordAuthUser } from '../types'

export function StoreLibraryView({
  viewMode,
  catalog,
  catalogLoadState,
  onRetryCatalog,
  selectedGame,
  selectedGameId,
  onSelectGame,
  onRequestAsset,
  detail,
  assets,
  selectedVersion,
  selectedCurrentVersion,
  selectedVersionInfo,
  selectedInstallState,
  verifyStatus,
  updateReady,
  showVersionAction,
  canUpdate,
  updateSize,
  installSize,
  temporarySpace,
  isJobRunning,
  isGameRunning,
  onPrimaryAction,
  onPlay,
  onVerify,
  onUninstall,
  onOpenInstallOptions,
  onOpenStore,
  cloudSaveStatus,
  cloudSaveBusy,
  cloudLaunchBlocked,
  onToggleCloudSave,
  onAddCloudSaveFolder,
  onSyncCloudSave,
  onResolveCloudConflict,
  onRestoreCloudSnapshot,
  onLaunchWithoutCloudSync,
  onConnectGoogleDrive,
  onDisconnectGoogleDrive,
  onBackupGoogleDrive,
  onRestoreMissingSaveFiles,
  discordUser,
}: {
  viewMode: 'store' | 'library'
  catalog: GameCatalog
  catalogLoadState: 'loading' | 'ready' | 'error'
  onRetryCatalog: () => void
  selectedGame: GameSummary | null
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  detail: GameDetail | null
  assets: Record<string, string>
  selectedVersion: string
  selectedCurrentVersion: string
  selectedVersionInfo?: GameVersionInfo
  selectedInstallState?: GameInstallState
  verifyStatus: VerifyUiStatus | null
  updateReady: boolean
  showVersionAction: boolean
  canUpdate: boolean
  updateSize: number
  installSize: number
  temporarySpace: number
  isJobRunning: boolean
  isGameRunning: boolean
  onPrimaryAction: () => void
  onPlay: () => void
  onVerify: () => void
  onUninstall: () => void
  onOpenInstallOptions: () => void
  onOpenStore: () => void
  cloudSaveStatus: CloudSaveStatus | null
  cloudSaveBusy: boolean
  cloudLaunchBlocked: boolean
  onToggleCloudSave: (enabled: boolean) => void
  onAddCloudSaveFolder: () => void
  onSyncCloudSave: () => void
  onResolveCloudConflict: (conflictId: string, resolution: 'local' | 'cloud') => void
  onRestoreCloudSnapshot: (snapshotId: string) => void
  onLaunchWithoutCloudSync: () => void
  onConnectGoogleDrive: () => void
  onDisconnectGoogleDrive: () => void
  onBackupGoogleDrive: () => void
  onRestoreMissingSaveFiles: () => void
  discordUser?: DiscordAuthUser | null
}) {
  const [query, setQuery] = useState('')
  const [tutorialVisible, setTutorialVisible] = useState(false)
  const realtimeConfig = useRealtimeConfig()

  useEffect(() => {
    if (selectedGame && selectedInstallState?.installed && selectedGame.id.includes('among')) {
      const shownKey = `tutorial_shown_${selectedGame.id}`
      if (localStorage.getItem(shownKey) !== 'true') {
        localStorage.setItem(shownKey, 'true')
        const timer = window.setTimeout(() => setTutorialVisible(true), 0)
        return () => window.clearTimeout(timer)
      }
    }
    return undefined
  }, [selectedGame, selectedInstallState?.installed])

  const visibleGames = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return catalog.games
    return catalog.games.filter((game) =>
      [game.title, game.subtitle, game.developer, game.publisher].some((value) => value.toLowerCase().includes(needle)),
    )
  }, [catalog.games, query])
  const actionDockRef = useRef<HTMLDivElement>(null)
  const [stickyVisible, setStickyVisible] = useState(false)

  useEffect(() => {
    if (!selectedGameId || !detail?.gameId || typeof IntersectionObserver === 'undefined') {
      return
    }

    const el = actionDockRef.current
    if (!el) {
      return
    }

    const observer = new IntersectionObserver(([entry]) => setStickyVisible(!entry.isIntersecting), {
      threshold: 0,
      rootMargin: '-64px 0px 0px 0px',
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [selectedGameId, detail?.gameId])

  const renderGameCard = (game: GameSummary, variant: 'compact' | 'browse') => {
    const tags = getGameTags(game)
    const isComingSoon = gameHasTag(game, 'coming soon')
    return (
      <button
        className={[
          'store-game-card',
          variant === 'browse' ? 'browse-game-card' : '',
          game.id === selectedGameId ? 'active' : '',
          isComingSoon ? 'coming-soon' : '',
        ]
          .filter(Boolean)
          .join(' ')}
        key={game.id}
        type="button"
        disabled={isComingSoon}
        onClick={() => !isComingSoon && onSelectGame(game.id)}
      >
      <div className="store-game-card-media">
        <LazyGameCardImage
          game={game}
          assetId={game.gridAssetId}
          url={assetUrlForId(game.gridAssetId, assets)}
          variant={variant}
          onRequestAsset={onRequestAsset}
        />
        {tags.length > 0 ? (
          <div className="game-card-tags" aria-label="Game tags">
            {tags.map((tag) => (
              <i className={`game-card-tag tone-${tag.tone}`} key={tag.id}>{tag.label}</i>
            ))}
          </div>
        ) : null}
      </div>
      <span>
        <strong>{game.title}</strong>
        <small>{game.developer}</small>
      </span>
    </button>
    )
  }

  if (!selectedGame) {
    if (catalogLoadState === 'loading' && catalog.games.length === 0) {
      return <CatalogLoadingView viewMode={viewMode} />
    }

    if (catalogLoadState === 'error' && catalog.games.length === 0) {
      return <CatalogUnavailableView viewMode={viewMode} onRetry={onRetryCatalog} />
    }

    return (
      <section className="library-browse-view">
        <header className="library-browse-toolbar">
          <div className="library-browse-heading">
            <strong>{viewMode === 'store' ? 'Store' : 'Installed games'}</strong>
            <span>
              {visibleGames.length} game{visibleGames.length === 1 ? '' : 's'}
            </span>
          </div>
          <label className="store-search">
            <Search size={16} />
            <input aria-label="Search games" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search..." />
          </label>
        </header>

        <div className="library-browse-grid">
          {visibleGames.map((game) => (
            <GameHoverCard key={game.id} game={game} assets={assets} onRequestAsset={onRequestAsset}>
              {renderGameCard(game, 'browse') as React.ReactElement}
            </GameHoverCard>
          ))}
          {visibleGames.length === 0 && viewMode === 'library' ? (
            <div className="library-empty-inline library-empty-installed">
              <Library size={28} />
              <strong>No installed games</strong>
              <span>Games installed from Store will appear here.</span>
              <button type="button" onClick={onOpenStore}>
                <ShoppingBag size={15} />
                Open Store
              </button>
            </div>
          ) : visibleGames.length === 0 ? (
            <div className="library-empty-inline">
              <Search size={24} />
              <strong>No matching games</strong>
            </div>
          ) : null}
        </div>
      </section>
    )
  }

  if (!detail) {
    return <GameDetailLoadingView game={selectedGame} assets={assets} onBack={() => onSelectGame(null)} />
  }

  const hero = assetUrlForId(selectedGame.heroAssetId, assets) || firstMediaUrl(detail, assets)
  const logo = assetUrlForId(selectedGame.logoAssetId, assets)
  const installed = Boolean(selectedInstallState?.installed)
  const isVerifying = verifyStatus?.state === 'running'

  const isDownloading = isJobRunning
  const isPlaying = isGameRunning

  let actionLabel: string = installed ? t.library.play : (!isTauriRuntime() ? 'Remote Install' : t.library.chooseInstall)
  let actionClass = 'primary-control'
  let primaryDisabled = false
  const stateLabel = !installed ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay

  if (isPlaying) {
    actionLabel = 'Running'
    actionClass = 'primary-control running-btn'
    primaryDisabled = true
  } else if (isDownloading) {
    actionLabel = 'Downloading'
    actionClass = 'primary-control downloading-btn'
    primaryDisabled = true
  } else if (!installed) {
    actionLabel = !isTauriRuntime() ? 'Remote Install' : t.library.chooseInstall
    primaryDisabled = !canUpdate
  }

  const primaryActionBtn = !installed ? onOpenInstallOptions : onPlay
  const primaryIcon = isPlaying
    ? <Play size={17} />
    : isDownloading
      ? <Download size={17} />
      : installed
        ? <Play size={17} />
        : <Download size={17} />
  const updateDisabled = !canUpdate || isDownloading || isPlaying
  const displayedVersion = installed ? selectedCurrentVersion : selectedVersion
  const downloadSize = updateSize || selectedVersionInfo?.sizeBytes || 0
  const verifyLabel = isVerifying ? 'Verifying...' : t.library.verifyIntegrity
  const VerifyIcon = verifyStatus?.state === 'failed' ? CircleAlert : ShieldCheck
  const missingCount = verifyStatus?.missingFiles?.length ?? 0
  const changedCount = verifyStatus?.mismatchedFiles?.length ?? 0

  const gridAsset = assetUrlForId(selectedGame.gridAssetId, assets)
  const iconAsset = assetUrlForId(selectedGame.iconAssetId, assets)

  const livePlayers = realtimeConfig.livePlayerCount?.[selectedGame.id]

  return (
    <section className="game-detail-view">
      {/* ── Sticky Floating Bar ── */}
      <div className={`sticky-action-bar${stickyVisible ? ' visible' : ''}`}>
        {(iconAsset || gridAsset) && (
          <img
            className="sticky-bar-icon"
            src={iconAsset || gridAsset}
            alt=""
          />
        )}
        <div className="sticky-bar-info">
          <strong>{detail.title}</strong>
          <span>{displayedVersion} (Build 23244517) {livePlayers !== undefined ? `• ${livePlayers.toLocaleString()} Playing` : ''}</span>
        </div>
        <div className="sticky-bar-actions">
          {installed && selectedGame?.id.includes('among') && (
            <button type="button" onClick={() => setTutorialVisible(true)}>
              <BookOpen size={15} />
              Tutorial
            </button>
          )}
          {installed && (
            <button type="button" onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
              <FolderOpen size={15} />
              Browse
            </button>
          )}
          <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
            <VerifyIcon size={15} />
            {verifyLabel}
          </button>
          <button
            className={actionClass}
            type="button"
            onClick={primaryActionBtn}
            disabled={primaryDisabled}
          >
            {primaryIcon}
            {actionLabel}
          </button>
          {installed && showVersionAction ? (
            <button
              className="update-control"
              type="button"
              onClick={onPrimaryAction}
              disabled={updateDisabled}
            >
              <Download size={15} />
              {updateReady ? t.library.update : 'Versions'}
            </button>
          ) : null}
          {installed ? (
            <button className="danger-control" type="button" onClick={onUninstall}>
              <X size={15} />
              {t.library.uninstall}
            </button>
          ) : null}
        </div>
      </div>

      <section className="game-detail-main">
        <button className="back-to-library" type="button" onClick={() => onSelectGame(null)}>
          <ChevronLeft size={16} />
          Back
        </button>
        <div className="detail-hero">
          {hero ? <img src={hero} alt="" loading="eager" /> : <div className="detail-placeholder"><ImageIcon size={40} /></div>}
          <div className="detail-hero-shade" />
          <div className="detail-copy">
            <span className="storage-pill">
              <HardDrive size={14} />
              {detail.install?.storageLabel || 'HDD'}
            </span>
            {logo ? <img className="detail-logo" src={logo} alt={detail.title} /> : <h1>{detail.title}</h1>}
            <p>{detail.shortDescription}</p>
            <div className="library-meta-row">
              <span>Version {displayedVersion} (Build 23244517)</span>
              <span>{formatBytes(downloadSize)}</span>
              {detail.install?.supportsResume ? <span>{t.library.resumeSupported}</span> : null}
              {livePlayers !== undefined ? <span className="live-players-badge"><span className="pulse-dot"></span>{livePlayers.toLocaleString()} Online</span> : null}
            </div>
          </div>
          <div className="store-action-dock" ref={actionDockRef}>
            {installed && selectedGame?.id.includes('among') && (
              <button type="button" onClick={() => setTutorialVisible(true)}>
                <BookOpen size={15} />
                Tutorial
              </button>
            )}
            {installed && (
              <button type="button" onClick={() => selectedInstallState?.installPath && invoke('open_folder', { path: selectedInstallState.installPath })}>
                <FolderOpen size={15} />
                Browse
              </button>
            )}
            <button type="button" onClick={onVerify} disabled={!installed || isVerifying}>
              <VerifyIcon size={17} />
              {verifyLabel}
            </button>
            <button className={actionClass} type="button" onClick={primaryActionBtn} disabled={primaryDisabled}>
              {primaryIcon}
              {actionLabel}
            </button>
            {installed && showVersionAction ? (
              <button className="update-control" type="button" onClick={onPrimaryAction} disabled={updateDisabled}>
                <Download size={17} />
                {updateReady ? t.library.update : 'Versions'}
              </button>
            ) : null}
            {installed ? (
              <button className="danger-control" type="button" onClick={onUninstall}>
                <X size={17} />
                {t.library.uninstall}
              </button>
            ) : null}
          </div>
        </div>
        <MediaRail detail={detail} assets={assets} />

        <section className="detail-body">
          <div className="detail-description">
            <h2>{detail.title}</h2>
            <div
              className="description-html"
              dangerouslySetInnerHTML={{ __html: processDescriptionHtml(detail.detailedDescription, assets) }}
            />
          </div>
        </section>
      </section>

      <aside className="store-info-column">
        <section className="panel status-card">
          <header className="side-header">
            <CheckCircle2 size={17} />
            <strong>{stateLabel}</strong>
          </header>
          <dl className="metric-list">
            <div>
              <dt>{t.library.currentVersion}</dt>
              <dd>{installed ? selectedCurrentVersion : t.library.notInstalled}</dd>
            </div>
            <div>
              <dt>{t.library.latestVersion}</dt>
              <dd>{selectedGame.latestVersion}</dd>
            </div>
            <div>
              <dt>{t.library.targetVersion}</dt>
              <dd>{selectedVersion}</dd>
            </div>
            <div>
              <dt>Install size</dt>
              <dd>{formatBytes(downloadSize)}</dd>
            </div>
          </dl>
        </section>
        <InstallSummaryPanel
          selectedVersion={selectedVersion}
          downloadSize={downloadSize}
          installSize={installSize || selectedVersionInfo?.sizeBytes || downloadSize}
          temporarySpace={temporarySpace || selectedVersionInfo?.sizeBytes || downloadSize}
        />
        {verifyStatus ? (
          <section className={`panel verify-feedback ${verifyStatus.state}`}>
            <header className="side-header">
              <VerifyIcon size={17} />
              <strong>{isVerifying ? 'Verifying install' : 'Verify result'}</strong>
            </header>
            <p>{verifyStatus.message}</p>
            {verifyStatus.state === 'failed' ? (
              <div className="verify-count-summary">
                <span>
                  <strong>{missingCount}</strong>
                  missing
                </span>
                <span>
                  <strong>{changedCount}</strong>
                  changed
                </span>
              </div>
            ) : null}
            <div className="verify-progress">
              <div className="mini-track">
                <span style={{ width: `${Math.round((verifyStatus.percent ?? 0) * 100)}%` }} />
              </div>
              <small>
                {Math.round((verifyStatus.percent ?? 0) * 100)}%
                {verifyStatus.totalBytes ? ` - ${formatBytes(verifyStatus.checkedBytes ?? 0)} / ${formatBytes(verifyStatus.totalBytes)}` : ''}
              </small>
            </div>
            {verifyStatus.currentFile ? <small className="verify-current-file">{verifyStatus.currentFile}</small> : null}
          </section>
        ) : null}
        <GameDetailsPanel detail={detail} />
        {installed && viewMode === 'library' ? (
          <CloudSavePanel
            status={cloudSaveStatus}
            busy={cloudSaveBusy}
            launchBlocked={cloudLaunchBlocked}
            onToggle={onToggleCloudSave}
            onAddFolder={onAddCloudSaveFolder}
            onSync={onSyncCloudSave}
            onResolve={onResolveCloudConflict}
            onRestore={onRestoreCloudSnapshot}
            onLaunchWithoutSync={onLaunchWithoutCloudSync}
            onConnectGoogleDrive={onConnectGoogleDrive}
            onDisconnectGoogleDrive={onDisconnectGoogleDrive}
            onBackupGoogleDrive={onBackupGoogleDrive}
            onRestoreMissingFiles={onRestoreMissingSaveFiles}
          />
        ) : null}
        <AchievementPreview achievements={detail.achievements} assets={assets} />
        <GameChat gameId={detail.gameId} discordUser={discordUser} />
      </aside>
      {tutorialVisible && selectedGame ? (
        <TutorialModal
          gameId={selectedGame.id}
          onClose={() => setTutorialVisible(false)}
        />
      ) : null}
    </section>
  )
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
  isJobRunning,
  isGameRunning,
  canUpdate,
  installMode,
  selectedVersion,
}: {
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
  isJobRunning: boolean
  isGameRunning: boolean
  canUpdate: boolean
  installMode: boolean
  selectedVersion: string
}) {
  const hero = assetUrlForId(game.heroAssetId, assets) || firstMediaUrl(detail, assets)
  const stateLabel = installMode ? t.library.readyToInstall : updateReady ? t.library.readyToUpdate : t.library.readyToPlay

  let playLabel = t.library.play.toUpperCase()
  let playClass = 'update-button hero-play-button'
  if (isGameRunning) {
    playLabel = 'RUNNING'
    playClass = 'update-button running-btn'
  } else if (isJobRunning) {
    playLabel = 'DOWNLOADING'
    playClass = 'update-button downloading-btn'
  }

  const playDisabled = isGameRunning || isJobRunning
  const updateDisabled = isGameRunning || isJobRunning || !canUpdate

  return (
    <section className="hero-panel">
      {hero ? <img src={hero} alt="" loading="eager" /> : null}
      <div className="game-strip">
        <div className="game-emblem">
          {assetUrlForId(game.iconAssetId, assets) ? <img src={assetUrlForId(game.iconAssetId, assets)} alt="" /> : <ImageIcon size={28} />}
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
              <button className={playClass} type="button" onClick={onPlay} disabled={playDisabled}>
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

export function VersionStat({ label, value, highlight = false }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="version-stat">
      <small>{label}</small>
      <strong className={highlight ? 'gold-text' : ''}>{value}</strong>
    </div>
  )
}

export function MediaRail({ detail, assets }: { detail: GameDetail; assets: Record<string, string> }) {
  const safeMedia = Array.isArray(detail.media) ? detail.media : []

  // Build a thumb map: video item id -> thumbnail URL
  // e.g. "movie-00" -> URL from item with id "movie-thumb-00"
  const videoThumbMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (const item of safeMedia) {
      // Handle all possible thumbnail role names (Firestore may use any of these)
      const isThumbRole =
        item.role === 'video-thumb' ||
        item.role === 'video-thumbnail' ||
        item.role === 'video-poster'
      const url = isThumbRole ? assetUrlForId(item.assetId, assets) : undefined
      if (!url) continue
      // Derive the video id from the thumb id:
      //   "movie-thumb-0"     → "movie-0"
      //   "movie-thumbnail-0" → "movie-0"
      //   "movie-poster-0"    → "movie-0"
      const videoId = item.id
        .replace(/^movie-thumb-/, 'movie-')
        .replace(/^movie-thumbnail-/, 'movie-')
        .replace(/^movie-poster-/, 'movie-')
      if (!map[videoId]) map[videoId] = url  // first wins
    }
    return map
  }, [safeMedia, assets])


  const media = safeMedia
    .filter((item) => isCarouselMedia(item) && assetUrlForId(item.assetId, assets))
    .sort((left, right) => mediaPriority(left) - mediaPriority(right))
    .map((item) => ({ ...item, url: assetUrlForId(item.assetId, assets)! }))
  const [activeIndex, setActiveIndex] = useState(0)

  if (media.length === 0) {
    return null
  }
  const safeActiveIndex = Math.min(activeIndex, media.length - 1)
  const active = media[safeActiveIndex]
  const activeIsVideo = active.mimeType.startsWith('video/') || active.role === 'video' || active.role === 'video-preview'
  const go = (direction: -1 | 1) => {
    setActiveIndex((current) => (current + direction + media.length) % media.length)
  }

  return (
    <section className="media-section media-carousel-section">
      <header>
        <strong>{t.library.media}</strong>
        <small>
          {media.length} items - {detail.metadataSource}
        </small>
      </header>
      <div className="media-carousel">
        <div className="media-stage">
          {activeIsVideo ? (
            <video src={active.url} controls muted preload="metadata" poster={videoThumbMap[active.id]} />
          ) : (
            <>
              <img src={active.url} alt="" loading="lazy" />
              {active.role === 'video-preview' ? (
                <span className="media-play-badge" aria-hidden="true">
                  <Play size={22} />
                </span>
              ) : null}
            </>
          )}
          <button className="media-nav prev" type="button" onClick={() => go(-1)} aria-label="Previous media">
            <ChevronLeft size={22} />
          </button>
          <button className="media-nav next" type="button" onClick={() => go(1)} aria-label="Next media">
            <ChevronRight size={22} />
          </button>
          <div className="media-stage-caption">
            <strong>{active.title}</strong>
            <span>{active.role}</span>
          </div>
        </div>
        <div className="media-thumb-rail">
          {media.map((item, index) => {
            const isVideo = item.mimeType.startsWith('video/') || item.role === 'video' || item.role === 'video-preview'
            const thumbUrl = isVideo ? (videoThumbMap[item.id] ?? null) : null

            return (
              <button
                className={index === safeActiveIndex ? 'media-thumb active' : 'media-thumb'}
                key={item.id}
                type="button"
                onClick={() => setActiveIndex(index)}
              >
                {isVideo ? (
                  <span className="image-video-thumb">
                    {thumbUrl ? (
                      <img src={thumbUrl} alt="" loading="lazy" />
                    ) : (
                      <span className="video-thumb-placeholder"><Play size={24} /></span>
                    )}
                    <Play size={16} className="video-thumb-overlay" />
                  </span>
                ) : (
                  <img src={item.url} alt="" loading="lazy" />
                )}
              </button>
            )
          })}
        </div>
      </div>
      <div className="media-rail legacy-hidden">
        {media.map((item) => (
          <article key={item.id}>
            {item.mimeType.startsWith('video/') ? (
              <video src={item.url} muted controls />
            ) : (
              <img src={item.url} alt="" loading="lazy" />
            )}
            <span>{item.title}</span>
          </article>
        ))}
      </div>
    </section>
  )
}

export function AchievementPreview({
  achievements,
  assets,
}: {
  achievements: GameAchievement[]
  assets: Record<string, string>
}) {
  const [showAll, setShowAll] = useState(false)
  const safeAchievements = Array.isArray(achievements) ? achievements : []
  const available = safeAchievements.filter((achievement) => assetUrlForId(achievement.iconAssetId, assets))
  const preview = available.slice(0, 10)

  // KHẮC PHỤC CẢNH BÁO LỖI 'any': Định nghĩa kiểu dữ liệu chuẩn cho Lenis
  useEffect(() => {
    interface LenisWindow {
      __lenis?: {
        stop: () => void
        start: () => void
      }
    }
    const lenis = (window as unknown as LenisWindow).__lenis
    
    if (showAll) {
      lenis?.stop()
    } else {
      lenis?.start()
    }
    return () => {
      lenis?.start()
    }
  }, [showAll])

  if (available.length === 0) {
    return null
  }

  return (
    <section className="achievement-section">
      <header>
        <strong>{t.library.achievements}</strong>
        <div className="achievement-header-actions">
          <small>{safeAchievements.length} total</small>
          {/* Thêm aria-label để sửa cảnh báo vàng của ESLint */}
          <button type="button" aria-label="See all achievements" onClick={() => setShowAll(true)}>
            <Trophy size={15} />
            See all
          </button>
        </div>
      </header>

      {/* SỬA LỖI ĐÈ/CHỒNG CHÉO: Thêm gridAutoRows: 'max-content' */}
      <div className="achievement-grid" style={{ gridAutoRows: 'max-content' }}>
        {preview.map((achievement) => (
          <article key={achievement.id}>
            <img src={assetUrlForId(achievement.iconAssetId, assets)} alt="" loading="lazy" />
            <div>
              <strong>{achievement.name}</strong>
              <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
            </div>
          </article>
        ))}
      </div>

      {/* Dùng createPortal đẩy popup ra ngoài cùng <body> */}
      {/* Thêm dấu chấm than (!) vào document.body! để báo cho TS biết nó chắc chắn tồn tại */}
      {showAll && typeof document !== 'undefined' ? createPortal(
        <div className="dialog-backdrop" style={{ zIndex: 99999 }} role="presentation" onClick={() => setShowAll(false)}>
          <section className="achievement-modal achievement-modal--enter" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <header>
              <div>
                <strong>{t.library.achievements}</strong>
                <span>{available.length} achievement entries</span>
              </div>
              <button type="button" aria-label="Close" onClick={() => setShowAll(false)}>
                <X size={17} />
              </button>
            </header>
            
            {/* SỬA LỖI ĐÈ/CHỒNG CHÉO: Thêm style={{ gridAutoRows: 'max-content' }} */}
            <div className="achievement-all-grid" data-lenis-prevent="true" style={{ gridAutoRows: 'max-content' }}>
              {available.map((achievement) => (
                <article key={achievement.id}>
                  <img src={assetUrlForId(achievement.iconAssetId, assets)} alt="" loading="lazy" />
                  <div>
                    <strong>{achievement.name}</strong>
                    <small>{achievement.hidden ? 'Hidden' : achievement.description}</small>
                  </div>
                </article>
              ))}
            </div>
          </section>
        </div>,
        document.body!
      ) : null}
    </section>
  )
}