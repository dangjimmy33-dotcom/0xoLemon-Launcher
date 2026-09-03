import { useState, useEffect, useRef, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
  Download,
  FolderDown,
  Search,
  RefreshCw,
  Folder,
  Layers,
  KeyRound,
  ShieldCheck,
  CheckCircle2,
  AlertCircle,
  XCircle,
  Terminal,
  ExternalLink,
  Sliders,
  Loader2,
  HardDrive,
  Pause,
  Play,
  ArrowRightLeft,
} from 'lucide-react'
import { useLocale } from '../context/locale'
import type {
  DepotGameItem,
  DepotGameDetail,
  DepotDownloadProgressEvent,
  DepotDownloaderStatus,
  DepotInstallState,
} from '../types'
import './DepotDownloaderView.css'
import { UnifiedSearchOverlay, UnifiedSearchResult } from './UnifiedSearchOverlay'

export function DepotDownloaderView({ defaultLibraryRoot }: { defaultLibraryRoot: string }) {
  const { locale } = useLocale()
  const isVi = locale.startsWith('vi')

  // Catalog state
  const [catalog, setCatalog] = useState<DepotGameItem[]>([])
  const [loadingCatalog, setLoadingCatalog] = useState(true)
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [depotSearchOverlayOpen, setDepotSearchOverlayOpen] = useState(false)

  // Selected game & builds
  const [selectedGame, setSelectedGame] = useState<DepotGameItem | null>(null)
  const [gameDetail, setGameDetail] = useState<DepotGameDetail | null>(null)
  const [loadingDetail, setLoadingDetail] = useState(false)
  const [detailError, setDetailError] = useState<string | null>(null)

  // Configuration for download
  const [selectedBuildId, setSelectedBuildId] = useState<string>('')
  const [targetDir, setTargetDir] = useState<string>('')
  const [maxConcurrency, setMaxConcurrency] = useState<number>(64)
  const [verifyAll, setVerifyAll] = useState<boolean>(true)

  // Download runtime state
  const [isDownloading, setIsDownloading] = useState(false)
  const [isPaused, setIsPaused] = useState(false)
  const [installState, setInstallState] = useState<DepotInstallState | null>(null)
  const [installStateRefreshSeq, setInstallStateRefreshSeq] = useState(0)
  const [downloadProgress, setDownloadProgress] = useState<number>(0)
  const [currentDepotText, setCurrentDepotText] = useState<string>('')
  const [statusMessage, setStatusMessage] = useState<string>('')
  const [downloadLogs, setDownloadLogs] = useState<string[]>([])
  const [downloadSuccess, setDownloadSuccess] = useState<boolean | null>(null)
  const [showLogs, setShowLogs] = useState<boolean>(true)

  const terminalBodyRef = useRef<HTMLDivElement>(null)
  const detailRequestSeqRef = useRef(0)

  // Load catalog on mount
  const fetchCatalog = async () => {
    setLoadingCatalog(true)
    setCatalogError(null)
    try {
      const items = await invoke<DepotGameItem[]>('depot_downloader_get_catalog')
      setCatalog(items)
    } catch (err: any) {
      setCatalogError(err?.toString() || 'Lỗi tải danh mục kho Depot.')
    } finally {
      setLoadingCatalog(false)
    }
  }

  useEffect(() => {
    fetchCatalog()
  }, [])

  useEffect(() => {
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setDepotSearchOverlayOpen(true)
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [])

  // Check ongoing status
  useEffect(() => {
    invoke<DepotDownloaderStatus>('depot_downloader_get_status')
      .then((status) => {
        if (status.isDownloading || status.isPaused) {
          setIsDownloading(status.isDownloading)
          setIsPaused(status.isPaused)
          if (status.destinationDir) setTargetDir(status.destinationDir)
          if (status.activeBuildId) setSelectedBuildId(status.activeBuildId)
        }
      })
      .catch(() => {})
  }, [])

  // Listen for progress events
  useEffect(() => {
    const unlisten = listen<DepotDownloadProgressEvent>('depot-download-progress', (event) => {
      const payload = event.payload
      if (!payload) return

      if (payload.eventType === 'start' || payload.eventType === 'resumed') {
        setIsDownloading(true)
        setIsPaused(false)
        setDownloadSuccess(null)
        setDownloadProgress(0)
        setStatusMessage(payload.message || 'Bắt đầu tải...')
      } else if (payload.eventType === 'depot-start') {
        setCurrentDepotText(payload.message || `Đang tải depot ${payload.depotId}...`)
        if (payload.progressPercent != null) {
          setDownloadProgress(payload.progressPercent)
        }
      } else if (payload.eventType === 'log') {
        if (payload.message) {
          setDownloadLogs((prev) => [...prev.slice(-300), payload.message!])
        }
        if (payload.progressPercent != null) {
          setDownloadProgress(payload.progressPercent)
        }
      } else if (payload.eventType === 'depot-done') {
        if (payload.progressPercent != null) {
          setDownloadProgress(payload.progressPercent)
        }
      } else if (payload.eventType === 'paused') {
        setIsDownloading(false)
        setIsPaused(true)
        setDownloadSuccess(null)
        setStatusMessage(payload.message || (isVi ? 'Đã tạm dừng. Dữ liệu hiện có được giữ nguyên.' : 'Paused. Existing data is preserved.'))
      } else if (payload.eventType === 'complete') {
        setIsDownloading(false)
        setIsPaused(false)
        setDownloadProgress(100)
        setDownloadSuccess(true)
        setStatusMessage(payload.message || 'Tải hoàn tất thành công!')
        setInstallStateRefreshSeq((value) => value + 1)
      } else if (payload.eventType === 'error') {
        setIsDownloading(false)
        setIsPaused(false)
        setDownloadSuccess(false)
        setStatusMessage(payload.message || 'Lỗi tải depot.')
      } else if (payload.eventType === 'cancelled') {
        setIsDownloading(false)
        setIsPaused(false)
        setDownloadSuccess(false)
        setStatusMessage(payload.message || 'Đã hủy tải.')
      }
    })

    return () => {
      unlisten.then((fn) => fn()).catch(() => {})
    }
  }, [isVi])

  // Read launcher-owned committed BuildID metadata for this working copy.
  // DepotDownloader's own .DepotDownloader/depot.config remains untouched and
  // authoritative for its chunk/manifest resume logic.
  useEffect(() => {
    if (!gameDetail || !targetDir.trim()) {
      setInstallState(null)
      return
    }

    let disposed = false
    invoke<DepotInstallState>('depot_downloader_get_install_state', {
      appid: gameDetail.appid,
      destinationDir: targetDir,
    })
      .then((state) => {
        if (!disposed) setInstallState(state)
      })
      .catch(() => {
        if (!disposed) setInstallState(null)
      })

    return () => {
      disposed = true
    }
  }, [gameDetail, targetDir, installStateRefreshSeq])

  // Keep live log scrolling inside the terminal only so ancestor containers never move.
  useEffect(() => {
    if (!showLogs || !terminalBodyRef.current) return
    terminalBodyRef.current.scrollTop = terminalBodyRef.current.scrollHeight
  }, [downloadLogs, showLogs])

  // Select a game from catalog. The request sequence prevents a slower
  // response from a previous click from overwriting the current selection.
  const handleSelectGame = async (game: DepotGameItem) => {
    if (isDownloading || isPaused) return
    const requestId = ++detailRequestSeqRef.current

    setSelectedGame(game)
    setGameDetail(null)
    setSelectedBuildId('')
    setInstallState(null)
    setLoadingDetail(true)
    setDetailError(null)
    setDownloadSuccess(null)
    setStatusMessage('')
    setCurrentDepotText('')
    setDownloadLogs([])

    // Default target path — use the launcher's configured library root
    const safeTitle = game.title.replace(/[\/:*?"<>|]/g, '_').trim()
    const libraryBase = defaultLibraryRoot.replace(/[\\/]+$/, '')
    setTargetDir(`${libraryBase}\\${safeTitle}`)

    try {
      const detail = await invoke<DepotGameDetail>('depot_downloader_get_game_detail', {
        appid: game.appid,
        folderName: game.folderName,
      })
      if (detailRequestSeqRef.current !== requestId) return

      setGameDetail(detail)
      if (detail.builds.length > 0) {
        setSelectedBuildId(detail.builds[0].buildId)
      }
    } catch (err: any) {
      if (detailRequestSeqRef.current !== requestId) return
      setDetailError(err?.toString() || 'Không thể lấy thông tin phiên bản game.')
    } finally {
      if (detailRequestSeqRef.current === requestId) {
        setLoadingDetail(false)
      }
    }
  }

  // Browse destination folder
  const handleBrowseDir = async () => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: isVi ? 'Chọn thư mục lưu game' : 'Select Game Destination Directory',
      })
      if (selected && typeof selected === 'string') {
        setTargetDir(selected)
      }
    } catch (e) {
      console.error('Directory browse failed:', e)
    }
  }

  // Start from one canonical detail snapshot. This prevents a game card from
  // being combined with a BuildID resolved for a different game.
  const handleStartDownload = async () => {
    if (!gameDetail || !selectedBuildId || !targetDir) return
    const build = gameDetail.builds.find((item) => item.buildId === selectedBuildId)
    if (!build) {
      setDownloadSuccess(false)
      setStatusMessage(
        isVi
          ? 'Build đã chọn không còn thuộc game hiện tại. Hãy chọn lại phiên bản.'
          : 'The selected build no longer belongs to this game. Please select the build again.',
      )
      return
    }

    setIsDownloading(true)
    setIsPaused(false)
    setDownloadSuccess(null)
    setDownloadLogs([])
    setDownloadProgress(0)
    setStatusMessage(isVi ? 'Đang khởi chạy tiến trình tải...' : 'Starting download pipeline...')

    try {
      await invoke('depot_downloader_start_download', {
        appid: gameDetail.appid,
        folderName: gameDetail.folderName,
        buildId: build.buildId,
        destinationDir: targetDir,
        maxDownloads: maxConcurrency,
        verifyAll,
      })
    } catch (err: any) {
      setIsDownloading(false)
      setIsPaused(false)
      setDownloadSuccess(false)
      setStatusMessage(err?.toString() || (isVi ? 'Không thể bắt đầu tải.' : 'Failed to start download.'))
    }
  }

  // Pause keeps game files, .DepotDownloader, staging and cached manifests.
  // Resume launches the exact same immutable job; DepotDownloader then verifies
  // the interrupted working copy and reuses chunks that are already valid.
  const handlePauseDownload = async () => {
    try {
      await invoke('depot_downloader_pause_download')
      setStatusMessage(isVi ? 'Đang tạm dừng an toàn...' : 'Pausing safely...')
    } catch (err: any) {
      setStatusMessage(err?.toString() || (isVi ? 'Không thể tạm dừng.' : 'Could not pause download.'))
    }
  }

  const handleResumeDownload = async () => {
    try {
      await invoke('depot_downloader_resume_download')
      setIsPaused(false)
      setIsDownloading(true)
      setDownloadSuccess(null)
      setStatusMessage(isVi ? 'Đang kiểm tra dữ liệu đã có và tiếp tục tải...' : 'Verifying existing data and resuming...')
    } catch (err: any) {
      setStatusMessage(err?.toString() || (isVi ? 'Không thể tiếp tục tải.' : 'Could not resume download.'))
    }
  }

  // Cancel download
  const handleCancelDownload = async () => {
    try {
      await invoke('depot_downloader_cancel_download')
      setIsPaused(false)
      setIsDownloading(false)
      setStatusMessage(isVi ? 'Đang gửi yêu cầu hủy tải...' : 'Sending cancel request...')
    } catch (err) {
      console.error('Cancel error:', err)
    }
  }

  // Open destination folder
  const handleOpenFolder = async () => {
    if (!targetDir) return
    try {
      await invoke('open_folder', { path: targetDir })
    } catch (e) {
      console.error('Failed to open directory:', e)
    }
  }

  // Filtered games
  const filteredCatalog = useMemo(() => {
    const q = searchQuery.toLowerCase().trim()
    if (!q) return catalog
    return catalog.filter(
      (item) =>
        item.title.toLowerCase().includes(q) ||
        item.appid.toString().includes(q) ||
        item.folderName.toLowerCase().includes(q)
    )
  }, [catalog, searchQuery])

  const selectedBuild = useMemo(() => {
    return gameDetail?.builds.find((b) => b.buildId === selectedBuildId) ?? null
  }, [gameDetail, selectedBuildId])

  const isActiveDownload = isDownloading || isPaused
  const installedBuildId = installState?.installedBuildId ?? ''
  const isVersionSwitch = Boolean(installedBuildId && selectedBuildId && installedBuildId !== selectedBuildId)
  const isCurrentBuild = Boolean(installedBuildId && selectedBuildId && installedBuildId === selectedBuildId)

  return (
    <div className="depot-downloader-container">
      {/* ── Top Header Bar ── */}
      <header className="depot-downloader-header">
        <div className="depot-header-brand">
          <div className="depot-header-icon-wrap">
            <FolderDown size={22} className="depot-header-icon" />
          </div>
          <div>
            <div className="depot-header-title-row">
              <h2>Depot Downloader</h2>
              <span className="depot-header-badge">Steam Clean Depots</span>
              <span className="depot-header-badge is-net9">Self-contained .NET 9</span>
            </div>
            <p className="depot-header-subtitle">
              {isVi
                ? 'Tải trực tiếp các bản build & depot sạch nguyên bản từ Steam thông qua DepotDownloaderMod (dùng chung kho manifests & keys).'
                : 'Directly download clean Steam builds and depots via DepotDownloaderMod with shared manifests and decryption keys.'}
            </p>
          </div>
        </div>

        <div className="depot-header-actions">
          <div className="depot-search-box">
            <Search size={14} className="depot-search-icon" />
            <input
              type="text"
              placeholder={isVi ? 'Tìm kiếm game hoặc AppID...' : 'Search game or AppID...'}
              value={searchQuery}
              onFocus={() => setDepotSearchOverlayOpen(true)}
              onClick={() => setDepotSearchOverlayOpen(true)}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            {searchQuery && (
              <button
                type="button"
                className="depot-search-clear"
                onClick={() => setSearchQuery('')}
              >
                &times;
              </button>
            )}
          </div>

          <button
            type="button"
            className="depot-refresh-btn"
            onClick={fetchCatalog}
            disabled={loadingCatalog}
            title={isVi ? 'Làm mới kho' : 'Refresh catalog'}
          >
            <RefreshCw size={14} className={loadingCatalog ? 'spin' : ''} />
            <span>{isVi ? 'Làm mới' : 'Refresh'}</span>
          </button>
        </div>
      </header>

      <UnifiedSearchOverlay
        open={depotSearchOverlayOpen}
        query={searchQuery}
        onQueryChange={setSearchQuery}
        onClose={() => setDepotSearchOverlayOpen(false)}
        onSubmit={() => setDepotSearchOverlayOpen(false)}
        placeholder={isVi ? 'Tìm kiếm game hoặc AppID...' : 'Search game or AppID...'}
        ariaLabel={isVi ? 'Tìm kiếm Depot Downloader' : 'Search Depot Downloader'}
        resultCount={filteredCatalog.length}
        resultsHint={isVi ? 'Kho build và depot hiện có' : 'Available clean build and depot catalog'}
        discoveryTitle={isVi ? 'Game trong kho Depot' : 'Depot catalog discovery'}
        discoveryHint={isVi ? 'Tìm theo tên game, AppID hoặc tên thư mục.' : 'Search by game title, Steam AppID, or repository folder name.'}
        historyKey="0xo.depotDownloaderSearchHistory"
      >
        {filteredCatalog.length ? filteredCatalog.slice(0, 36).map((item) => (
          <UnifiedSearchResult
            key={`depot-search-${item.appid}`}
            title={item.title}
            subtitle={`AppID ${item.appid}`}
            matchLabel={item.folderName}
            imageUrl={item.bannerUrl || null}
            onClick={() => {
              setSelectedGame(item)
              setDepotSearchOverlayOpen(false)
            }}
          />
        )) : (
          <div className="store-search-empty">
            <Search size={28} />
            <strong>{isVi ? 'Không có game phù hợp' : 'No matching games'}</strong>
            <span>{isVi ? 'Thử tên khác hoặc nhập AppID chính xác.' : 'Try another title or exact Steam AppID.'}</span>
          </div>
        )}
      </UnifiedSearchOverlay>

      {/* ── Main Layout Body ── */}
      <div className="depot-main-layout">
        {/* Left Side: Game Catalog Grid */}
        <section className="depot-catalog-pane">
          <div className="depot-pane-title-bar">
            <span>{isVi ? 'Danh mục Game trong Kho' : 'Available Games'}</span>
            <span className="depot-count-pill">{filteredCatalog.length} {isVi ? 'game' : 'games'}</span>
          </div>

          {loadingCatalog ? (
            <div className="depot-pane-loading">
              <Loader2 size={28} className="spin" />
              <span>{isVi ? 'Đang đọc danh sách kho Depot...' : 'Fetching depot repository catalog...'}</span>
            </div>
          ) : catalogError ? (
            <div className="depot-pane-error">
              <AlertCircle size={24} />
              <span>{catalogError}</span>
              <button type="button" onClick={fetchCatalog} className="depot-btn-secondary">
                {isVi ? 'Thử lại' : 'Retry'}
              </button>
            </div>
          ) : filteredCatalog.length === 0 ? (
            <div className="depot-pane-empty">
              <HardDrive size={32} />
              <p>{isVi ? 'Không tìm thấy tựa game nào phù hợp.' : 'No matching games found.'}</p>
            </div>
          ) : (
            <div className="depot-cards-grid">
              {filteredCatalog.map((item) => {
                const isSelected = selectedGame?.appid === item.appid
                return (
                  <button
                    key={item.appid}
                    type="button"
                    className={`depot-game-card ${isSelected ? 'is-selected' : ''}`}
                    onClick={() => handleSelectGame(item)}
                    disabled={isActiveDownload}
                  >
                    <div className="depot-card-thumb">
                      {item.bannerUrl ? (
                        <img
                          src={item.bannerUrl}
                          alt={item.title}
                          onError={(e) => {
                            ;(e.currentTarget as HTMLElement).style.display = 'none'
                          }}
                        />
                      ) : null}
                      <div className="depot-card-overlay">
                        <span className="depot-appid-tag">AppID {item.appid}</span>
                      </div>
                    </div>
                    <div className="depot-card-info">
                      <strong className="depot-card-title">{item.title}</strong>
                      <span className="depot-card-sub">{item.folderName}</span>
                    </div>
                  </button>
                )
              })}
            </div>
          )}
        </section>

        {/* Right Side: Selected Game Details & Downloader Configuration */}
        <section className="depot-inspector-pane">
          {!selectedGame ? (
            <div className="depot-no-selection">
              <FolderDown size={48} className="depot-no-sel-icon" />
              <h3>{isVi ? 'Chưa chọn Game' : 'No Game Selected'}</h3>
              <p>
                {isVi
                  ? 'Hãy chọn một tựa game từ danh mục bên trái để duyệt các phiên bản (BuildID) và tiến hành tải về máy.'
                  : 'Select a game from the catalog on the left to inspect available builds and start downloading.'}
              </p>
            </div>
          ) : loadingDetail ? (
            <div className="depot-pane-loading">
              <Loader2 size={28} className="spin" />
              <span>{isVi ? `Đang nạp dữ liệu ${selectedGame.title}...` : `Loading ${selectedGame.title} build data...`}</span>
            </div>
          ) : detailError ? (
            <div className="depot-pane-error">
              <AlertCircle size={24} />
              <span>{detailError}</span>
            </div>
          ) : gameDetail ? (
            <div className="depot-detail-scroll">
              {/* Game Hero Card */}
              <div className="depot-game-hero">
                <img
                  src={selectedGame.bannerUrl || ''}
                  alt=""
                  className="depot-hero-banner"
                  onError={(e) => ((e.currentTarget as HTMLElement).style.display = 'none')}
                />
                <div className="depot-hero-content">
                  <div className="depot-hero-tags">
                    <span className="depot-tag-appid">AppID {gameDetail.appid}</span>
                    {gameDetail.hasKey ? (
                      <span className="depot-tag-key is-ready">
                        <ShieldCheck size={12} /> Key Ready
                      </span>
                    ) : (
                      <span className="depot-tag-key is-missing">
                        <KeyRound size={12} /> No Key
                      </span>
                    )}
                  </div>
                  <h2 className="depot-hero-title">{gameDetail.title}</h2>
                  <span className="depot-hero-folder">{gameDetail.folderName}</span>
                </div>
              </div>

              {/* Version & Build Selector */}
              <div className="depot-config-card">
                <div className="depot-card-heading">
                  <Layers size={16} />
                  <strong>{isVi ? 'Chọn Phiên Bản & Build' : 'Select Version & Build'}</strong>
                </div>

                {gameDetail.builds.length === 0 ? (
                  <div className="depot-alert-box is-warning">
                    <AlertCircle size={16} />
                    <span>{isVi ? 'Không tìm thấy BuildID nào trong thư mục game này.' : 'No BuildIDs found in this game folder.'}</span>
                  </div>
                ) : (
                  <div className="depot-form-group">
                    <label>{isVi ? 'Phiên bản BuildID có sẵn:' : 'Available BuildID:'}</label>
                    <select
                      value={selectedBuildId}
                      onChange={(e) => setSelectedBuildId(e.target.value)}
                      disabled={isActiveDownload}
                      className="depot-select"
                    >
                      {gameDetail.builds.map((build, idx) => (
                        <option key={build.buildId} value={build.buildId}>
                          {idx === 0 ? '★ ' : ''}BuildID {build.buildId}
                          {build.version ? ` · (${build.version})` : ''}
                          {build.buildDate ? ` · [${build.buildDate}]` : ''}
                          {` · (${build.manifests.length} depots)`}
                        </option>
                      ))}
                    </select>
                  </div>
                )}

                {installedBuildId ? (
                  <div className={`depot-version-state ${isVersionSwitch ? 'is-switch' : 'is-current'}`}>
                    <ArrowRightLeft size={15} />
                    <div>
                      <strong>
                        {isVersionSwitch
                          ? (isVi
                            ? `Đang cài BuildID ${installedBuildId} → sẽ chuyển sang ${selectedBuildId}`
                            : `Installed BuildID ${installedBuildId} → switch to ${selectedBuildId}`)
                          : (isVi
                            ? `BuildID ${installedBuildId} đang được cài`
                            : `BuildID ${installedBuildId} is installed`)}
                      </strong>
                      <span>
                        {isVersionSwitch
                          ? (isVi
                            ? 'Launcher giữ nguyên .DepotDownloader, staging và manifest cũ; dữ liệu hợp lệ được tái sử dụng và Verify được ép bật khi đổi version.'
                            : 'The launcher preserves .DepotDownloader, staging and old manifests; valid data is reused and verification is forced for a version switch.')
                          : (isVi
                            ? 'Có thể chọn BuildID khác để upgrade/downgrade trên cùng working copy.'
                            : 'Choose another BuildID to upgrade or downgrade the same working copy.')}
                      </span>
                    </div>
                  </div>
                ) : installState?.hasDepotState ? (
                  <div className="depot-version-state is-detected">
                    <HardDrive size={15} />
                    <div>
                      <strong>{isVi ? 'Đã phát hiện .DepotDownloader hiện có' : 'Existing .DepotDownloader state detected'}</strong>
                      <span>{isVi ? 'BuildID sẽ được ghi nhận sau lần tải hoàn tất tiếp theo; dữ liệu hiện có không bị xóa.' : 'The BuildID will be recorded after the next completed run; existing data is not deleted.'}</span>
                    </div>
                  </div>
                ) : null}

                {/* Depots manifest list preview */}
                {selectedBuild && (
                  <div className="depot-manifests-table">
                    <div className="depot-manifest-row is-header">
                      <span>Depot ID</span>
                      <span>Manifest GID</span>
                      <span>File Manifest</span>
                    </div>
                    {selectedBuild.manifests.map((m) => (
                      <div key={m.depotId} className="depot-manifest-row">
                        <span className="depot-id-mono">{m.depotId}</span>
                        <span className="depot-gid-mono">{m.manifestGid}</span>
                        <span className="depot-file-mono">{m.manifestFile}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Download Destination & Options */}
              <div className="depot-config-card">
                <div className="depot-card-heading">
                  <Sliders size={16} />
                  <strong>{isVi ? 'Cấu Hình Đường Dẫn & Tải Về' : 'Download Path & Concurrency'}</strong>
                </div>

                <div className="depot-form-group">
                  <label>{isVi ? 'Thư mục đích lưu game:' : 'Destination Directory:'}</label>
                  <div className="depot-path-input-group">
                    <input
                      type="text"
                      value={targetDir}
                      onChange={(e) => setTargetDir(e.target.value)}
                      disabled={isActiveDownload}
                      placeholder="D:\Games\MyGame"
                      className="depot-input"
                    />
                    <button
                      type="button"
                      onClick={handleBrowseDir}
                      disabled={isActiveDownload}
                      className="depot-btn-browse"
                      title={isVi ? 'Duyệt thư mục' : 'Browse Folder'}
                    >
                      <Folder size={15} />
                      <span>{isVi ? 'Chọn...' : 'Browse...'}</span>
                    </button>
                  </div>
                </div>

                <div className="depot-options-grid">
                  <div className="depot-form-group">
                    <label>{isVi ? 'Số luồng tải đồng thời (Threads):' : 'Max Concurrent Downloads:'}</label>
                    <select
                      value={maxConcurrency}
                      onChange={(e) => setMaxConcurrency(Number(e.target.value))}
                      disabled={isActiveDownload}
                      className="depot-select"
                    >
                      <option value={16}>16 threads ({isVi ? 'Tiêu chuẩn' : 'Standard'})</option>
                      <option value={32}>32 threads ({isVi ? 'Nhanh' : 'Fast'})</option>
                      <option value={64}>64 threads ({isVi ? 'Rất nhanh - Khuyên dùng' : 'Recommended'})</option>
                      <option value={128}>128 threads ({isVi ? 'Cực nhanh' : 'High Performance'})</option>
                      <option value={256}>256 threads ({isVi ? 'Tối đa' : 'Maximum'})</option>
                    </select>
                  </div>

                  <div className="depot-form-group">
                    <label>{isVi ? 'Kiểm tra toàn vẹn (Verify Chunks):' : 'Chunk Verification:'}</label>
                    <label className="depot-checkbox-label">
                      <input
                        type="checkbox"
                        checked={verifyAll || isVersionSwitch}
                        onChange={(e) => setVerifyAll(e.target.checked)}
                        disabled={isActiveDownload || isVersionSwitch}
                      />
                      <span>{isVersionSwitch
                          ? (isVi ? 'Verify bắt buộc khi đổi BuildID để tái sử dụng đúng chunk và sửa phần khác biệt' : 'Verification is forced when switching BuildID so valid chunks can be reused safely')
                          : (isVi ? 'Bật --verify-all để kiểm tra mã SHA từng khối' : 'Enable --verify-all chunk validation')}</span>
                    </label>
                  </div>
                </div>
              </div>

              {/* Action & Download Progress Bar */}
              <div className="depot-action-card">
                {isActiveDownload ? (
                  <div className="depot-download-live-pane">
                    <div className="depot-live-header">
                      <div className="depot-live-title-wrap">
                        {isPaused ? <Pause size={18} className="depot-pulse-icon" /> : <Loader2 size={18} className="spin depot-pulse-icon" />}
                        <div>
                          <strong>{statusMessage || (isPaused ? (isVi ? 'Đã tạm dừng' : 'Paused') : (isVi ? 'Đang tải game...' : 'Downloading game...'))}</strong>
                          <span className="depot-live-sub">{currentDepotText}</span>
                        </div>
                      </div>
                      <span className="depot-pct-display">{downloadProgress.toFixed(1)}%</span>
                    </div>

                    <div className="depot-progress-track">
                      <div
                        className={`depot-progress-fill ${isPaused ? '' : 'is-animated'}`}
                        style={{ width: `${Math.max(1, Math.min(100, downloadProgress))}%` }}
                      />
                    </div>

                    <div className="depot-live-controls">
                      <div className="depot-live-primary-controls">
                        {isPaused ? (
                          <button type="button" onClick={handleResumeDownload} className="depot-btn-resume">
                            <Play size={15} />
                            <span>{isVi ? 'Tiếp tục' : 'Resume'}</span>
                          </button>
                        ) : (
                          <button type="button" onClick={handlePauseDownload} className="depot-btn-pause">
                            <Pause size={15} />
                            <span>{isVi ? 'Tạm dừng' : 'Pause'}</span>
                          </button>
                        )}
                      <button
                        type="button"
                        onClick={handleCancelDownload}
                        className="depot-btn-cancel"
                      >
                        <XCircle size={15} />
                        <span>{isVi ? 'Hủy tải xuống' : 'Cancel Download'}</span>
                      </button>
                      </div>

                      <button
                        type="button"
                        onClick={() => setShowLogs(!showLogs)}
                        className="depot-btn-toggle-logs"
                      >
                        <Terminal size={14} />
                        <span>{showLogs ? (isVi ? 'Ẩn Logs' : 'Hide Logs') : (isVi ? 'Hiện Logs' : 'Show Logs')}</span>
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="depot-idle-actions">
                    {downloadSuccess === true ? (
                      <div className="depot-success-banner">
                        <CheckCircle2 size={20} className="depot-success-icon" />
                        <div>
                          <strong>{isVi ? 'Tải game thành công!' : 'Download Completed!'}</strong>
                          <span>{isVi ? 'Toàn bộ depots đã được ghi đầy đủ vào thư mục.' : 'All depots have been written to destination folder.'}</span>
                        </div>
                        <button type="button" onClick={handleOpenFolder} className="depot-btn-open-dir">
                          <ExternalLink size={14} />
                          <span>{isVi ? 'Mở Thư Mục' : 'Open Folder'}</span>
                        </button>
                      </div>
                    ) : downloadSuccess === false ? (
                      <div className="depot-error-banner">
                        <XCircle size={20} />
                        <div>
                          <strong>{isVi ? 'Tải thất bại hoặc bị hủy.' : 'Download failed or cancelled.'}</strong>
                          <span>{statusMessage}</span>
                        </div>
                      </div>
                    ) : null}

                    <div className="depot-start-btn-row">
                      <button
                        type="button"
                        onClick={handleStartDownload}
                        disabled={isActiveDownload || !selectedBuildId || !targetDir || !gameDetail.hasKey}
                        className="depot-btn-primary-start"
                      >
                        {isVersionSwitch ? <ArrowRightLeft size={18} /> : <Download size={18} />}
                        <span>{isVersionSwitch
                          ? (isVi ? `Chuyển sang BuildID ${selectedBuildId}` : `Switch to BuildID ${selectedBuildId}`)
                          : isCurrentBuild
                            ? (isVi ? `Kiểm tra / sửa BuildID ${selectedBuildId}` : `Verify / repair BuildID ${selectedBuildId}`)
                            : (isVi ? 'Tải game' : 'Download game')}</span>
                      </button>

                      {targetDir && (
                        <button
                          type="button"
                          onClick={handleOpenFolder}
                          className="depot-btn-secondary"
                        >
                          <Folder size={15} />
                          <span>{isVi ? 'Mở Thư Mục' : 'Open Folder'}</span>
                        </button>
                      )}
                    </div>
                  </div>
                )}

                {/* Console Log Terminal */}
                {showLogs && downloadLogs.length > 0 && (
                  <div className="depot-terminal-console">
                    <div className="depot-terminal-header">
                      <div className="depot-term-dot is-red" />
                      <div className="depot-term-dot is-yellow" />
                      <div className="depot-term-dot is-green" />
                      <span className="depot-term-title">DepotDownloaderMod Output Stream</span>
                    </div>
                    <div className="depot-terminal-body" ref={terminalBodyRef}>
                      {downloadLogs.map((line, idx) => (
                        <div key={idx} className="depot-log-line">
                          {line}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          ) : null}
        </section>
      </div>
    </div>
  )
}
