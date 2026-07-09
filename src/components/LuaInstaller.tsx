import { useState, useCallback, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { stat } from '@tauri-apps/plugin-fs'
import { Upload, RefreshCw, CheckCircle, XCircle, AlertCircle } from 'lucide-react'
import './LuaInstaller.css'

type InstallStatus = 'idle' | 'processing' | 'success' | 'error'

interface DroppedFile {
  name: string
  path: string
  size: number
}

export function LuaInstaller() {
  const [file, setFile] = useState<File | DroppedFile | null>(null)
  const [status, setStatus] = useState<InstallStatus>('idle')
  const [message, setMessage] = useState('')
  const [isDragOver, setIsDragOver] = useState(false)

  // Listen for Tauri file drop events
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
              const filePath = paths[0]
              const validExtensions = ['.zip', '.rar', '.7z', '.lua']
              const hasValidExt = validExtensions.some(ext => filePath.toLowerCase().endsWith(ext))

              if (hasValidExt) {
                const fileName = filePath.split(/[\\/]/).pop() || 'unknown'

                try {
                  // Use @tauri-apps/plugin-fs stat to get file metadata
                  const fileInfo = await stat(filePath)

                  console.log('[LuaInstaller] stat SUCCESS:', filePath, '→', fileInfo.size, 'bytes')
                  setFile({
                    name: fileName,
                    path: filePath,
                    size: fileInfo.size
                  })
                  setStatus('idle')
                  setMessage('')
                } catch (err) {
                  console.error('[LuaInstaller] stat FAILED:', filePath, '→', err)
                  setFile({
                    name: fileName,
                    path: filePath,
                    size: 0
                  })
                  setStatus('idle')
                  setMessage('')
                }
              } else {
                setStatus('error')
                setMessage('Please drop a valid file (.zip, .rar, .7z, or .lua)')
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
  }, [])

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)

    const droppedFile = e.dataTransfer.files[0]
    if (!droppedFile) {
      setStatus('error')
      setMessage('No file detected')
      return
    }

    // Accept .zip, .rar, .7z, .lua files
    const validExtensions = ['.zip', '.rar', '.7z', '.lua']
    const hasValidExt = validExtensions.some(ext => droppedFile.name.toLowerCase().endsWith(ext))

    if (hasValidExt) {
      setFile(droppedFile)
      setStatus('idle')
      setMessage('')
    } else {
      setStatus('error')
      setMessage('Please drop a valid file (.zip, .rar, .7z, or .lua)')
    }
  }, [])

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

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0]
    if (!selectedFile) return

    const validExtensions = ['.zip', '.rar', '.7z', '.lua']
    const hasValidExt = validExtensions.some(ext => selectedFile.name.toLowerCase().endsWith(ext))

    if (hasValidExt) {
      setFile(selectedFile)
      setStatus('idle')
      setMessage('')
    } else {
      setStatus('error')
      setMessage('Please select a valid file (.zip, .rar, .7z, or .lua)')
    }
  }, [])

  const handleInstall = useCallback(async () => {
    if (!file) return

    setStatus('processing')
    setMessage('Installing Lua script...')

    try {
      let fileData: string

      // Check if it's a browser File object or DroppedFile
      if ('path' in file && typeof file.path === 'string') {
        // DroppedFile from Tauri drag & drop - read from backend
        fileData = await invoke<string>('read_file_base64', { filepath: file.path })
      } else {
        // Regular browser File object - read in frontend
        const reader = new FileReader()
        fileData = await new Promise<string>((resolve, reject) => {
          reader.onload = () => resolve((reader.result as string).split(',')[1])
          reader.onerror = reject
          reader.readAsDataURL(file as File)
        })
      }

      // Extract appid from filename (e.g., "123456.zip" -> "123456")
      const appid = file.name.replace(/\.(zip|rar|7z|lua)$/, '')

      // Call backend to install
      await invoke('install_lua_from_zip', {
        appid,
        zipDataBase64: fileData
      })

      setStatus('success')
      setMessage('Lua script installed successfully! Restart Steam to apply changes.')
    } catch (error) {
      setStatus('error')
      setMessage(`Installation failed: ${error}`)
    }
  }, [file])

  const handleRestartSteam = useCallback(async () => {
    try {
      await invoke('restart_steam')
      setMessage('Steam is restarting...')
    } catch (error) {
      setMessage(`Failed to restart Steam: ${error}`)
    }
  }, [])

  return (
    <div className="lua-installer-container">
      <div className="lua-installer-content">
        <header className="lua-installer-header">
          <h1>Lua Installer</h1>
          <p>Install .lua files, .manifest files, or .zip archives to Steam</p>
          <button
            type="button"
            className="restart-steam-btn"
            onClick={handleRestartSteam}
            title="Restart Steam"
          >
            <RefreshCw size={16} />
            Restart Steam
          </button>
        </header>

        <div
          className={`drop-zone ${isDragOver ? 'drag-over' : ''} ${file ? 'has-file' : ''}`}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          {file ? (
            <div className="file-info">
              <Upload size={48} />
              <span className="file-name">{file.name}</span>
              <span className="file-size">
                {file.size >= 1024 * 1024
                  ? `${(file.size / (1024 * 1024)).toFixed(2)} MB`
                  : `${(file.size / 1024).toFixed(2)} KB`
                }
              </span>
            </div>
          ) : (
            <div className="drop-placeholder">
              <Upload size={48} />
              <p>Drop .zip, .rar, .7z, or .lua file here</p>
              <p className="drop-hint">Supported: ZIP archives, RAR, 7Z, Lua scripts</p>
              <input
                type="file"
                accept=".zip,.rar,.7z,.lua"
                onChange={handleFileSelect}
                style={{ display: 'none' }}
                id="file-input"
              />
              <label htmlFor="file-input" className="browse-btn">
                Browse Files
              </label>
            </div>
          )}
        </div>

        {message && (
          <div className={`status-message status-${status}`}>
            {status === 'processing' && <AlertCircle size={20} />}
            {status === 'success' && <CheckCircle size={20} />}
            {status === 'error' && <XCircle size={20} />}
            <span>{message}</span>
          </div>
        )}

        <div className="actions">
          <button
            type="button"
            className="install-btn primary-control"
            onClick={handleInstall}
            disabled={!file || status === 'processing'}
          >
            {status === 'processing' ? 'Installing...' : 'Install'}
          </button>
          {file && (
            <button
              type="button"
              className="clear-btn"
              onClick={() => {
                setFile(null)
                setStatus('idle')
                setMessage('')
              }}
              disabled={status === 'processing'}
            >
              Clear
            </button>
          )}
        </div>

        <div className="instructions">
          <h3>Instructions:</h3>
          <ol>
            <li>Download the {'{appid}'}.zip file for your game</li>
            <li>Drag and drop the zip file into the area above</li>
            <li>Click "Install" to setup the Lua script</li>
            <li>Click "Restart Steam" to apply changes</li>
            <li>The game will appear in your Steam library</li>
          </ol>
        </div>
      </div>
    </div>
  )
}
