import { Database } from 'lucide-react'
import type { CloudSaveStatus, DiscordAuthUser, GameCatalog, GameDetail, GameInstallState, GameSummary, GameVersionInfo, JobJournal, JobLog, PhaseProgress, Snapshot, TabId, VerifyUiStatus } from '../types'
import { rollbackVersionFor, assetUrlForId } from '../lib/gameMeta'
import { TabEmptyState, ScopedTabEmptyState } from './layout'
import { StoreLibraryView } from './library'
import { InstallBar } from './install'
import { DownloadQueuePanel, JobCenter, JobLogPanel } from './downloads'
import { CachePanel, RollbackPanel, InstallSummaryPanel, ChangedFiles } from './panels'
import { TranslationsView } from './TranslationsView'
import { WhatsNewView } from './WhatsNewView'
import { OfflineActivation } from './OfflineActivation'
import { LuaInstaller } from './LuaInstaller'

export function ActiveView({
  activeTab,
  catalog,
  catalogLoadState,
  onRetryCatalog,
  selectedGame,
  selectedGameId,
  onSelectGame,
  onRequestAsset,
  detail,
  assets,
  snapshot,
  installPath,
  installTarget,
  scanStatus,
  selectedVersion,
  selectedCurrentVersion,
  selectedVersionInfo,
  selectedInstallState,
  verifyStatus,
  installMode,
  updateReady,
  showVersionAction,
  canUpdate,
  isJobRunning,
  isGameRunning,
  onBrowse,
  onScan,
  onPrimaryAction,
  onPlay,
  onStop,
  onVerify,
  onUninstall,
  job,
  hasJob,
  progress,
  phaseProgress,
  updateSize,
  isRunning,
  onOpenInstallOptions,
  onPause,
  onCancel,
  onResume,
  isPaused,
  logs,
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
  cacheBusy,
  onClearCache,
  discordUser,
}: {
  activeTab: TabId
  catalog: GameCatalog
  catalogLoadState: 'loading' | 'ready' | 'error'
  onRetryCatalog: () => void
  selectedGame: GameSummary | null
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
  onRequestAsset: (game: GameSummary, assetId: string | undefined, urgent?: boolean) => void
  detail: GameDetail | null
  assets: Record<string, string>
  snapshot: Snapshot
  installPath: string
  installTarget: string
  scanStatus: string
  selectedVersion: string
  selectedCurrentVersion: string
  selectedVersionInfo?: GameVersionInfo
  selectedInstallState?: GameInstallState
  verifyStatus: VerifyUiStatus | null
  installMode: boolean
  updateReady: boolean
  showVersionAction: boolean
  canUpdate: boolean
  isJobRunning: boolean
  isGameRunning: boolean
  onBrowse: () => void
  onScan: () => void
  onPrimaryAction: () => void
  onPlay: () => void
  onStop: () => void
  onVerify: () => void
  onUninstall: () => void
  job: JobJournal
  hasJob: boolean
  progress: number
  phaseProgress: PhaseProgress
  updateSize: number
  isRunning: boolean
  onOpenInstallOptions: () => void
  onPause: () => void
  onCancel: () => void
  onResume?: () => void
  isPaused: boolean
  logs: JobLog[]
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
  cacheBusy: boolean
  onClearCache: () => void
  discordUser?: DiscordAuthUser | null
}) {
  const hasSelectedDetail = Boolean(selectedGame && detail)

  if (activeTab === 'Store' || activeTab === 'Library') {
    return (
      <StoreLibraryView
        viewMode={activeTab === 'Store' ? 'store' : 'library'}
        catalog={catalog}
        catalogLoadState={catalogLoadState}
        onRetryCatalog={onRetryCatalog}
        selectedGame={selectedGame}
        selectedGameId={selectedGameId}
        onSelectGame={onSelectGame}
        onRequestAsset={onRequestAsset}
        onPrimaryAction={onPrimaryAction}
        onPlay={() => onPlay()}
        onStop={() => onStop()}
        onVerify={() => onVerify()}
        detail={detail}
        assets={assets}
        selectedVersion={selectedVersion}
        selectedCurrentVersion={selectedCurrentVersion}
        selectedVersionInfo={selectedVersionInfo}
        selectedInstallState={selectedInstallState}
        verifyStatus={verifyStatus}
        updateReady={updateReady}
        showVersionAction={showVersionAction}
        canUpdate={canUpdate}
        updateSize={updateSize}
        installSize={snapshot.installSize}
        temporarySpace={snapshot.temporarySpace}
        isJobRunning={isJobRunning}
        isGameRunning={isGameRunning}
        onUninstall={onUninstall}
        onOpenInstallOptions={onOpenInstallOptions}
        onOpenStore={onOpenStore}
        cloudSaveStatus={cloudSaveStatus}
        cloudSaveBusy={cloudSaveBusy}
        cloudLaunchBlocked={cloudLaunchBlocked}
        onToggleCloudSave={onToggleCloudSave}
        onAddCloudSaveFolder={onAddCloudSaveFolder}
        onSyncCloudSave={onSyncCloudSave}
        onResolveCloudConflict={onResolveCloudConflict}
        onRestoreCloudSnapshot={onRestoreCloudSnapshot}
        onLaunchWithoutCloudSync={onLaunchWithoutCloudSync}
        onConnectGoogleDrive={onConnectGoogleDrive}
        onDisconnectGoogleDrive={onDisconnectGoogleDrive}
        onBackupGoogleDrive={onBackupGoogleDrive}
        onRestoreMissingSaveFiles={onRestoreMissingSaveFiles}
        discordUser={discordUser}
      />
    )
  }

  if (activeTab === 'Translations') {
    if (!hasSelectedDetail) {
      return (
        <TabEmptyState
          activeTab={activeTab}
          catalog={catalog}
          onSelectGame={onSelectGame}
          assets={assets}
          onRequestAsset={onRequestAsset}
        />
      )
    }

    const activeGame = selectedGame || catalog.games.find(g => g.id === selectedGameId)
    if (activeGame && activeGame.heroAssetId && !assets[activeGame.heroAssetId]) {
      onRequestAsset(activeGame, activeGame.heroAssetId)
    }

    return <TranslationsView
      gameId={selectedGameId ?? undefined}
      gameTitle={activeGame?.title}
      gameBanner={assetUrlForId(activeGame?.heroAssetId, assets)}
      onVerify={onVerify}
      onBack={() => onSelectGame(null)}
    />
  }

  if (activeTab === 'Cache') {
    return (
      <section className="single-view cache-tab-view">
        <CachePanel snapshot={snapshot} busy={cacheBusy} onClear={onClearCache} />
        {selectedGame && detail ? (
          <>
            <RollbackPanel snapshot={snapshot} rollbackVersion={rollbackVersionFor(detail, selectedVersion)} />
            {installMode ? (
              <InstallSummaryPanel
                selectedVersion={selectedVersion}
                downloadSize={updateSize}
                installSize={snapshot.installSize}
                temporarySpace={snapshot.temporarySpace}
              />
            ) : (
              <ChangedFiles files={snapshot.changedFiles} />
            )}
          </>
        ) : (
          <ScopedTabEmptyState
            icon={<Database size={34} />}
            title="No game selected"
            body="Choose a game in Library to inspect rollback and changed-file cache state."
          />
        )}
      </section>
    )
  }

  if (activeTab === 'What\'s New!') {
    return <WhatsNewView />
  }

  if (activeTab === 'Offline Activation') {
    return <OfflineActivation catalog={catalog} assets={assets} />
  }

  if (activeTab === 'Lua Installer') {
    return <LuaInstaller />
  }

  if (activeTab === 'Downloads' || activeTab === 'Updates') {
    if (!hasSelectedDetail && activeTab !== 'Downloads') {
      return (
        <TabEmptyState
          activeTab={activeTab}
          catalog={catalog}
          onSelectGame={onSelectGame}
          assets={assets}
          onRequestAsset={onRequestAsset}
        />
      )
    }

    return (
      <section className="content-grid single-main">
        <div className="main-column">
          {!installMode || hasJob ? (
            <InstallBar
              installPath={installPath}
              installTarget={installTarget}
              scanStatus={scanStatus}
              installMode={installMode}
              onBrowse={onBrowse}
              onScan={onScan}
            />
          ) : null}
          {activeTab === 'Downloads' ? (
            <DownloadQueuePanel
              gameTitle={selectedGame?.title ?? 'Selected game'}
              job={job}
              hasJob={hasJob}
              progress={progress}
              phaseProgress={phaseProgress}
              selectedVersion={selectedVersion}
              downloadSize={updateSize}
              isRunning={isRunning}
              isPaused={isPaused}
              onOpenOptions={onOpenInstallOptions}
              onPause={onPause}
              onCancel={onCancel}
              onResume={onResume}
            />
          ) : null}
          {hasJob || activeTab === 'Updates' ? (
            <>
              <JobCenter
                job={job}
                hasJob={hasJob}
                progress={progress}
                phaseProgress={phaseProgress}
                onPause={onPause}
                onCancel={onCancel}
                isPaused={isPaused}
                showControls={activeTab !== 'Downloads'}
              />
              {hasJob ? <JobLogPanel logs={logs} /> : null}
            </>
          ) : null}
        </div>
      </section>
    )
  }

  return null
}
