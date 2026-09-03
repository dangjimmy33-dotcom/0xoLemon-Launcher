import { useState, useCallback, useEffect, useMemo, useRef, memo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { stat } from '@tauri-apps/plugin-fs'
import {
  Upload,
  CheckCircle2,
  AlertCircle,
  Plus,
  Search,
  Trash2,
  HelpCircle,
  ChevronDown,
  ChevronUp,
  Image,
  Layers,
  Loader2,
  X,
} from 'lucide-react'
import './LuaInstaller.css'
import { useLocale } from '../context/locale'
import steamIconUrl from '../assets/steam.svg'
import steamDbIconUrl from '../assets/steamdb.png'
import { UnifiedSearchOverlay, UnifiedSearchResult } from './UnifiedSearchOverlay'

type InstallStatus = 'idle' | 'processing' | 'success' | 'error'

interface DroppedFile {
  name: string
  path: string
  size: number
}

interface InstalledLuaGame {
  appid: number
  channel: 'live' | 'locked' | string
}

const ITEMS_PER_PAGE = 12

const DigitalClock = memo(function DigitalClock() {
  const { locale } = useLocale()
  const [time, setTime] = useState(() => new Date())

  useEffect(() => {
    const timer = setInterval(() => setTime(new Date()), 1000)
    return () => clearInterval(timer)
  }, [])

  const formattedHoursMinutes = useMemo(() => {
    const hours = String(time.getHours()).padStart(2, '0')
    const minutes = String(time.getMinutes()).padStart(2, '0')
    return `${hours}:${minutes}`
  }, [time])

  const formattedSeconds = useMemo(() => {
    return String(time.getSeconds()).padStart(2, '0')
  }, [time])

  const formattedDate = useMemo(() => {
    try {
      const loc = locale === 'vi-VN' ? 'vi-VN' : 'en-US'
      return new Intl.DateTimeFormat(loc, {
        weekday: 'long',
        day: 'numeric',
        month: 'long',
      }).format(time)
    } catch {
      return time.toLocaleDateString()
    }
  }, [locale, time])

  return (
    <div className="lua-card lua-clock-card">
      <div className="lua-clock-center">
        <div className="lua-clock-digits">
          <span className="lua-clock-time">{formattedHoursMinutes}</span>
          <span className="lua-clock-seconds">{formattedSeconds}</span>
        </div>
        <div className="lua-clock-date">{formattedDate}</div>
      </div>
    </div>
  )
})

interface LuaGameCardProps {
  game: InstalledLuaGame
  coverBustKey: number
  isRemoving: boolean
  onRemove: (appid: number) => void
}

const LuaGameCard = memo(function LuaGameCard({
  game,
  coverBustKey,
  isRemoving,
  onRemove,
}: LuaGameCardProps) {
  const [imgSrc, setImgSrc] = useState(
    () => `https://cdn.cloudflare.steamstatic.com/steam/apps/${game.appid}/header.jpg?t=${coverBustKey}`
  )

  useEffect(() => {
    setImgSrc(`https://cdn.cloudflare.steamstatic.com/steam/apps/${game.appid}/header.jpg?t=${coverBustKey}`)
  }, [game.appid, coverBustKey])

  return (
    <article className="lua-game-card">
      <div className="lua-game-cover-box">
        <img
          src={imgSrc}
          alt={`AppID ${game.appid}`}
          loading="lazy"
          onError={() => {
            setImgSrc(`https://cdn.cloudflare.steamstatic.com/steam/apps/${game.appid}/capsule_616x353.jpg`)
          }}
        />
        <div className="lua-game-cover-overlay" />
        <span className="lua-game-appid-tag">ID: {game.appid}</span>
        {game.channel && (
          <span className={`lua-game-channel-tag is-${game.channel}`}>
            {game.channel}
          </span>
        )}
      </div>
      <div className="lua-game-card-footer">
        <span className="lua-game-title" title={`Steam App ID: ${game.appid}`}>
          App ID {game.appid}
        </span>
        <button
          type="button"
          className="lua-game-remove-btn"
          onClick={() => void onRemove(game.appid)}
          disabled={isRemoving}
          title="Remove from Steam"
        >
          {isRemoving ? <Loader2 size={14} className="is-spinning" /> : <Trash2 size={14} />}
        </button>
      </div>
    </article>
  )
})

export function LuaInstaller() {
  const { t } = useLocale()
  const copy = t.luaInstaller

  // --- Import / Dropzone State ---
  const [files, setFiles] = useState<(File | DroppedFile)[]>([])
  const [status, setStatus] = useState<InstallStatus>('idle')
  const [message, setMessage] = useState('')
  const [isDragOver, setIsDragOver] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  // --- Steam App ID Add State ---
  const [appIdInput, setAppIdInput] = useState('')
  const [isAddingAppId, setIsAddingAppId] = useState(false)
  const [appIdMessage, setAppIdMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)

  // --- Installed Games State ---
  const [installedGames, setInstalledGames] = useState<InstalledLuaGame[]>([])
  const [loadingGames, setLoadingGames] = useState(false)
  const [removingAppId, setRemovingAppId] = useState<number | null>(null)
  const [coverBustKey, setCoverBustKey] = useState<number>(Date.now())

  // --- Filter & Pagination State ---
  const [searchQuery, setSearchQuery] = useState('')
  const [luaInstallerSearchOverlayOpen, setLuaInstallerSearchOverlayOpen] = useState(false)
  const [sortBy, setSortBy] = useState<'recent' | 'az'>('recent')
  const [currentPage, setCurrentPage] = useState(1)
  const [instructionsOpen, setInstructionsOpen] = useState(false)

  // Load Installed Games from Backend
  const loadInstalledGames = useCallback(async () => {
    setLoadingGames(true)
    try {
      const games = await invoke<InstalledLuaGame[]>('get_lua_game_states')
      setInstalledGames(Array.isArray(games) ? games : [])
    } catch (error) {
      console.warn('Could not load Lua game states:', error)
    } finally {
      setLoadingGames(false)
    }
  }, [])

  useEffect(() => {
    void loadInstalledGames()
  }, [loadInstalledGames])

  useEffect(() => {
    const handleSearchShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setLuaInstallerSearchOverlayOpen(true)
      }
    }
    window.addEventListener('keydown', handleSearchShortcut)
    return () => window.removeEventListener('keydown', handleSearchShortcut)
  }, [])

  // Listen for Tauri native file drop events
  useEffect(() => {
    if (typeof window === 'undefined') return
    let unlisten: (() => void) | undefined

    const setupListener = async () => {
      try {
        const appWindow = getCurrentWindow()
        unlisten = await appWindow.onDragDropEvent(async (event) => {
          if (event.payload.type === 'over') {
            setIsDragOver(true)
          } else if (event.payload.type === 'drop') {
            setIsDragOver(false)
            const paths = event.payload.paths
            if (paths && paths.length > 0) {
              const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
              const validPaths = paths.filter((p) =>
                validExtensions.some((ext) => p.toLowerCase().endsWith(ext))
              )

              if (validPaths.length > 0) {
                const newFiles: DroppedFile[] = []
                for (const filePath of validPaths) {
                  const fileName = filePath.split(/[\\/]/).pop() || 'unknown'
                  try {
                    const fileInfo = await stat(filePath)
                    newFiles.push({ name: fileName, path: filePath, size: fileInfo.size })
                  } catch (err) {
                    console.error('[LuaInstaller] stat FAILED:', filePath, '→', err)
                  }
                }

                if (newFiles.length > 0) {
                  setFiles(newFiles)
                  setStatus('idle')
                  setMessage('')
                } else {
                  setStatus('error')
                  setMessage(copy.readFilesFailed)
                }
              } else {
                setStatus('error')
                setMessage(copy.invalidFileTypes)
              }
            }
          } else if (event.payload.type === 'leave') {
            setIsDragOver(false)
          }
        })
      } catch (error) {
        console.error('Failed to setup drag drop listener:', error)
      }
    }

    void setupListener()
    return () => {
      if (unlisten) unlisten()
    }
  }, [copy.invalidFileTypes, copy.readFilesFailed])

  // Web drag & drop handlers
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      setIsDragOver(false)

      if (!e.dataTransfer.files || e.dataTransfer.files.length === 0) {
        return
      }

      const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
      const validFiles = Array.from(e.dataTransfer.files).filter((f) =>
        validExtensions.some((ext) => f.name.toLowerCase().endsWith(ext))
      )

      if (validFiles.length > 0) {
        setFiles(validFiles)
        setStatus('idle')
        setMessage('')
      } else {
        setStatus('error')
        setMessage(copy.invalidFileTypes)
      }
    },
    [copy.invalidFileTypes]
  )

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(true)
  }, [])

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)
  }, [])

  const handleFileSelect = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      if (!e.target.files || e.target.files.length === 0) return

      const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
      const validFiles = Array.from(e.target.files).filter((f) =>
        validExtensions.some((ext) => f.name.toLowerCase().endsWith(ext))
      )

      if (validFiles.length > 0) {
        setFiles(validFiles)
        setStatus('idle')
        setMessage('')
      } else {
        setStatus('error')
        setMessage(copy.invalidFileTypes)
      }
    },
    [copy.invalidFileTypes]
  )

  // Handle Install Files
  const handleInstall = useCallback(async () => {
    if (files.length === 0) return

    setStatus('processing')
    setMessage(copy.installing)

    try {
      for (const f of files) {
        let fileData: string

        if ('path' in f && typeof f.path === 'string') {
          fileData = await invoke<string>('read_file_base64', { filepath: f.path })
        } else {
          const reader = new FileReader()
          fileData = await new Promise<string>((resolve, reject) => {
            reader.onload = () => resolve((reader.result as string).split(',')[1])
            reader.onerror = reject
            reader.readAsDataURL(f as File)
          })
        }

        await invoke('install_lua_from_zip', {
          appid: f.name,
          zipDataBase64: fileData,
        })
      }

      setStatus('success')
      setMessage(copy.installedSuccess)
      setFiles([])
      await loadInstalledGames()
    } catch (error) {
      setStatus('error')
      setMessage(copy.installFailed.replace('{error}', String(error)))
    }
  }, [copy.installedSuccess, copy.installFailed, copy.installing, files, loadInstalledGames])

  // Handle Add Steam App ID
  const handleAddAppId = useCallback(
    async (e?: React.FormEvent) => {
      if (e) e.preventDefault()
      const cleanAppId = appIdInput.trim()
      const numericId = parseInt(cleanAppId, 10)

      if (!cleanAppId || isNaN(numericId) || numericId <= 0) {
        setAppIdMessage({ type: 'error', text: copy.invalidAppId })
        return
      }

      setIsAddingAppId(true)
      setAppIdMessage(null)

      try {
        await invoke('add_to_steam', { appid: numericId, forceUpdate: false })
        setAppIdMessage({
          type: 'success',
          text: copy.addAppIdSuccess.replace('{appId}', String(numericId)),
        })
        setAppIdInput('')
        await loadInstalledGames()
      } catch (error) {
        setAppIdMessage({
          type: 'error',
          text: copy.addAppIdFailed.replace('{appId}', String(numericId)).replace('{error}', String(error)),
        })
      } finally {
        setIsAddingAppId(false)
      }
    },
    [appIdInput, copy.addAppIdFailed, copy.addAppIdSuccess, copy.invalidAppId, loadInstalledGames]
  )

  // Handle Restart Steam
  const handleRestartSteam = useCallback(async () => {
    try {
      await invoke('restart_steam')
      setMessage(copy.restartingSteam)
      setStatus('success')
    } catch (error) {
      setMessage(String(error))
      setStatus('error')
    }
  }, [copy.restartingSteam])

  // Handle Open SteamDB
  const handleOpenSteamDb = useCallback(async () => {
    const cleanAppId = appIdInput.trim()
    const url = cleanAppId && !isNaN(Number(cleanAppId))
      ? `https://steamdb.info/app/${cleanAppId}/`
      : 'https://steamdb.info/'

    try {
      await invoke('open_url', { url })
    } catch {
      window.open(url, '_blank')
    }
  }, [appIdInput])

  // Handle Remove Game
  const handleRemoveGame = async (appid: number) => {
    const confirmMsg = copy.removeGameConfirm.replace('{appId}', String(appid))
    if (!window.confirm(confirmMsg)) return

    setRemovingAppId(appid)
    try {
      await invoke('remove_from_steam', { appid })
      await loadInstalledGames()
    } catch (error) {
      alert(copy.removeGameFailed.replace('{appId}', String(appid)).replace('{error}', String(error)))
    } finally {
      setRemovingAppId(null)
    }
  }

  // Reload covers cache
  const handleReloadCovers = () => {
    setCoverBustKey(Date.now())
    void loadInstalledGames()
  }

  // Filtered & Sorted installed games
  const processedGames = useMemo(() => {
    let result = [...installedGames]

    if (searchQuery.trim()) {
      const q = searchQuery.trim().toLowerCase()
      result = result.filter((g) => String(g.appid).includes(q))
    }

    if (sortBy === 'az') {
      result.sort((a, b) => a.appid - b.appid)
    } else {
      // Recent (preserve reversed or incoming order)
      result.reverse()
    }

    return result
  }, [installedGames, searchQuery, sortBy])

  // Pagination calculation
  const totalPages = Math.max(1, Math.ceil(processedGames.length / ITEMS_PER_PAGE))
  const paginatedGames = useMemo(() => {
    const start = (currentPage - 1) * ITEMS_PER_PAGE
    return processedGames.slice(start, start + ITEMS_PER_PAGE)
  }, [currentPage, processedGames])

  // Ensure current page does not exceed total
  useEffect(() => {
    if (currentPage > totalPages) {
      setCurrentPage(totalPages)
    }
  }, [currentPage, totalPages])

  return (
    <div className="lua-installer-shell">
      <div className="lua-installer-wrapper">
        {/* =========================================================================
            TOP ROW (3 COLUMNS: IMPORT | CLOCK | APP ID OF STEAM)
           ========================================================================= */}
        <div className="lua-top-grid">
          {/* COLUMN 1: IMPORT */}
          <div className="lua-card lua-import-card">
            <div className="lua-card-header-label">{copy.importTitle}</div>

            <div
              className={`lua-import-dropzone ${isDragOver ? 'is-drag-over' : ''} ${files.length > 0 ? 'has-files' : ''}`}
              onDrop={handleDrop}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
            >
              <Upload size={28} className="lua-import-icon" />
              <strong className="lua-import-text">{copy.dragFilesHere}</strong>
              <span className="lua-import-formats">{copy.supportedFormats}</span>

              <input
                ref={fileInputRef}
                type="file"
                accept=".zip,.rar,.7z,.lua,.manifest"
                onChange={handleFileSelect}
                multiple
                style={{ display: 'none' }}
                id="lua-file-browse-input"
              />

              <button
                type="button"
                className="lua-browse-btn"
                onClick={() => fileInputRef.current?.click()}
              >
                {copy.browse}
              </button>
            </div>

            {/* Selected files indicator & install action */}
            {files.length > 0 && (
              <div className="lua-import-actions">
                <span className="lua-selected-count">
                  <Layers size={14} />
                  {copy.selectedFiles.replace('{count}', String(files.length))}
                </span>
                <div className="lua-btn-group">
                  <button
                    type="button"
                    className="lua-btn-install"
                    onClick={() => void handleInstall()}
                    disabled={status === 'processing'}
                  >
                    {status === 'processing' ? <Loader2 size={14} className="is-spinning" /> : <Upload size={14} />}
                    {status === 'processing' ? copy.installing : copy.install}
                  </button>
                  <button
                    type="button"
                    className="lua-btn-clear"
                    onClick={() => {
                      setFiles([])
                      setStatus('idle')
                      setMessage('')
                    }}
                    title={copy.clear}
                    aria-label={copy.clear}
                  >
                    <svg
                      width="14"
                      height="14"
                      viewBox="0 0 14 14"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      aria-hidden="true"
                      style={{ display: 'block', flexShrink: 0 }}
                    >
                      <line x1="2" y1="2" x2="12" y2="12" />
                      <line x1="12" y1="2" x2="2" y2="12" />
                    </svg>
                  </button>
                </div>
              </div>
            )}

            {/* Status alerts */}
            {message && (
              <div className={`lua-status-banner status-${status}`}>
                {status === 'processing' && <Loader2 size={15} className="is-spinning" />}
                {status === 'success' && <CheckCircle2 size={15} />}
                {status === 'error' && <AlertCircle size={15} />}
                <span>{message}</span>
              </div>
            )}
          </div>

          {/* COLUMN 2: DIGITAL CLOCK & DATE */}
          <DigitalClock />

          {/* COLUMN 3: STEAM APP ID */}
          <div className="lua-card lua-appid-card">
            <div className="lua-card-header-label">{copy.appIdTitle}</div>

            <form className="lua-appid-form" onSubmit={handleAddAppId}>
              <input
                type="text"
                className="lua-appid-input"
                value={appIdInput}
                onChange={(e) => setAppIdInput(e.target.value)}
                placeholder={copy.appIdPlaceholder}
                pattern="[0-9]*"
              />
              <button
                type="submit"
                className="lua-appid-add-btn"
                disabled={isAddingAppId || !appIdInput.trim()}
              >
                {isAddingAppId ? <Loader2 size={15} className="is-spinning" /> : <Plus size={15} strokeWidth={2.5} />}
                <span>{copy.add}</span>
              </button>
            </form>

            <p className="lua-appid-help">{copy.appIdHelp}</p>

            {/* AppID Message Notice */}
            {appIdMessage && (
              <div className={`lua-status-banner status-${appIdMessage.type}`}>
                {appIdMessage.type === 'success' ? <CheckCircle2 size={15} /> : <AlertCircle size={15} />}
                <span>{appIdMessage.text}</span>
              </div>
            )}

            {/* Secondary Action Buttons */}
            <div className="lua-appid-quick-actions">
              <button
                type="button"
                className="lua-action-btn"
                onClick={() => void handleRestartSteam()}
                title={copy.restartSteam}
              >
                <img src={steamIconUrl} alt="Steam" className="lua-action-btn-icon" />
                <span>{copy.restartSteam}</span>
              </button>
              <button
                type="button"
                className="lua-action-btn"
                onClick={() => void handleOpenSteamDb()}
                title={copy.steamDb}
              >
                <img src={steamDbIconUrl} alt="SteamDB" className="lua-action-btn-icon" />
                <span>{copy.steamDb}</span>
              </button>
            </div>
          </div>
        </div>

        <UnifiedSearchOverlay
          open={luaInstallerSearchOverlayOpen}
          query={searchQuery}
          onQueryChange={(value) => {
            setSearchQuery(value)
            setCurrentPage(1)
          }}
          onClose={() => setLuaInstallerSearchOverlayOpen(false)}
          onSubmit={() => setLuaInstallerSearchOverlayOpen(false)}
          placeholder={copy.searchPlaceholder}
          ariaLabel="Search installed Lua games"
          resultCount={processedGames.length}
          resultsHint="Installed Lua game states"
          discoveryTitle="Installed Lua games"
          discoveryHint="Search by exact Steam AppID. The installed-game list remains synchronized with this overlay."
          historyKey="0xo.luaInstallerSearchHistory"
        >
          {processedGames.length ? processedGames.slice(0, 36).map((game) => (
            <UnifiedSearchResult
              key={`lua-installer-search-${game.appid}`}
              title={`App ID ${game.appid}`}
              subtitle={`Steam AppID ${game.appid}`}
              matchLabel={game.channel ? `${game.channel} Lua channel` : 'Installed Lua state'}
              imageUrl={`https://cdn.cloudflare.steamstatic.com/steam/apps/${game.appid}/header.jpg?t=${coverBustKey}`}
              onClick={() => {
                setSearchQuery(String(game.appid))
                setCurrentPage(1)
                setLuaInstallerSearchOverlayOpen(false)
              }}
            />
          )) : (
            <div className="store-search-empty">
              <Search size={28} />
              <strong>{copy.noGamesFound}</strong>
              <span>Try another Steam AppID.</span>
            </div>
          )}
        </UnifiedSearchOverlay>

        {/* =========================================================================
            BOTTOM GAMES SECTION (TOOLBAR + GRID / EMPTY STATE + PAGINATION)
           ========================================================================= */}
        <div className="lua-games-section">
          {/* TOOLBAR */}
          <div className="lua-games-toolbar">
            <div className="lua-toolbar-left">
              <span className="lua-games-count-badge">
                {copy.gamesCount.replace('{count}', String(processedGames.length))}
              </span>
              <div className="lua-filter-pills" role="tablist">
                <button
                  type="button"
                  className={sortBy === 'recent' ? 'is-active' : ''}
                  onClick={() => setSortBy('recent')}
                >
                  {copy.recent}
                </button>
                <button
                  type="button"
                  className={sortBy === 'az' ? 'is-active' : ''}
                  onClick={() => setSortBy('az')}
                >
                  {copy.az}
                </button>
              </div>
            </div>

            <div className="lua-toolbar-right">
              <button
                type="button"
                className="lua-reload-covers-btn"
                onClick={handleReloadCovers}
                title={copy.reloadCovers}
              >
                <Image size={14} />
                <span>{copy.reloadCovers}</span>
              </button>

              <div className="lua-search-box">
                <Search size={14} className="lua-search-icon" />
                <input
                  type="text"
                  value={searchQuery}
                  onFocus={() => setLuaInstallerSearchOverlayOpen(true)}
                  onClick={() => setLuaInstallerSearchOverlayOpen(true)}
                  onChange={(e) => {
                    setSearchQuery(e.target.value)
                    setCurrentPage(1)
                  }}
                  placeholder={copy.searchPlaceholder}
                />
                {searchQuery && (
                  <button
                    type="button"
                    className="lua-search-clear"
                    onClick={() => setSearchQuery('')}
                  >
                    <X size={12} />
                  </button>
                )}
              </div>
            </div>
          </div>

          {/* MAIN GAMES CONTENT */}
          <div className="lua-games-content">
            {loadingGames ? (
              <div className="lua-games-loading">
                <Loader2 size={32} className="is-spinning" />
                <span>Loading...</span>
              </div>
            ) : paginatedGames.length === 0 ? (
              <div className="lua-games-empty">
                <p>{copy.noGamesFound}</p>
              </div>
            ) : (
              <div className="lua-games-grid">
                {paginatedGames.map((game) => (
                  <LuaGameCard
                    key={game.appid}
                    game={game}
                    coverBustKey={coverBustKey}
                    isRemoving={removingAppId === game.appid}
                    onRemove={handleRemoveGame}
                  />
                ))}
              </div>
            )}
          </div>

          {/* PAGINATION */}
          {totalPages > 1 && (
            <div className="lua-pagination">
              <button
                type="button"
                className="lua-page-btn"
                disabled={currentPage <= 1}
                onClick={() => setCurrentPage((p) => Math.max(1, p - 1))}
              >
                ‹
              </button>
              <span className="lua-page-indicator">
                {currentPage} / {totalPages}
              </span>
              <button
                type="button"
                className="lua-page-btn"
                disabled={currentPage >= totalPages}
                onClick={() => setCurrentPage((p) => Math.min(totalPages, p + 1))}
              >
                ›
              </button>
            </div>
          )}
        </div>

        {/* =========================================================================
            INSTRUCTIONS GUIDE SECTION (Collapsible accordion preserving user guide)
           ========================================================================= */}
        <div className="lua-instructions-wrapper">
          <button
            type="button"
            className="lua-instructions-toggle"
            onClick={() => setInstructionsOpen((open) => !open)}
          >
            <div className="lua-instructions-title">
              <HelpCircle size={16} />
              <span>{copy.instructionsTitle}</span>
            </div>
            {instructionsOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          </button>

          {instructionsOpen && (
            <div className="lua-instructions-body">
              <ol className="lua-instructions-steps">
                <li>{copy.instructionsStep1}</li>
                <li>{copy.instructionsStep2}</li>
                <li>{copy.instructionsStep3}</li>
                <li>{copy.instructionsStep4}</li>
                <li>{copy.instructionsStep5}</li>
              </ol>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
