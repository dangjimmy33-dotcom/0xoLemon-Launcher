import { useState, useCallback, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { KeyRound, CheckCircle, Loader2, AlertCircle } from 'lucide-react'

interface Props {
  gameDir: string
  cfgPath: string
  gameId: string
}

export function DenuvoActivationButton({ gameDir, cfgPath, gameId }: Props) {
  const [status, setStatus] = useState<'idle' | 'downloading' | 'extracting' | 'scanning' | 'generating' | 'success' | 'error'>('idle')
  const [message, setMessage] = useState<string>('')
  const [progress, setProgress] = useState<number>(0)

  // This component should only be used for EA SPORTS FC 26
  if (gameId !== 'ea-sports-fc-26') {
    return null
  }

  useEffect(() => {
    const unlistenDownload = listen<{ bytes_downloaded: number, total_bytes: number }>('magic_download_progress', (event) => {
      const { bytes_downloaded, total_bytes } = event.payload
      if (total_bytes > 0) {
        setProgress(Math.round((bytes_downloaded / total_bytes) * 100))
      }
    })
    
    const unlistenExtract = listen<string>('magic_extract_progress', () => {
      setStatus('extracting')
      setMessage('Đang giải nén dữ liệu...')
    })

    return () => {
      unlistenDownload.then(f => f())
      unlistenExtract.then(f => f())
    }
  }, [])

  const handleActivate = useCallback(async () => {
    setStatus('downloading')
    setProgress(0)
    
    try {
      // 0. Download and extract Magic File v1.6.5
      setMessage('Đang tải Magic File (Version 1.6.5)...')
      await invoke('download_and_extract_magic_file', { gameDir })

      setStatus('scanning')
      // 1. Delete old tickets to ensure we get a fresh one
      setMessage('Đang dọn dẹp file Ticket cũ...')
      await invoke('delete_denuvo_tickets', { gameDir })

      // 2. Launch game to generate ticket (silently if possible)
      setMessage('Đang khởi chạy game để lấy Ticket mới (chờ 5-10s)...')
      await invoke('launch_game_executable', { gameDir, exeName: 'FC26.exe', silent: true })
      
      // Wait for Denuvo to generate the ticket and close the stub
      await new Promise(r => setTimeout(r, 6000))
      
      // 3. Scan for ticket
      setMessage('Đang quét và lấy dữ liệu Ticket...')
      const ticketContent = await invoke<string>('scan_for_denuvo_ticket', { gameDir })
      
      setStatus('generating')
      setMessage('Đang kết nối Server để tạo Token...')
      
      // 4. Ask server for token
      const serverUrl = 'http://127.0.0.1:3030' // Configurable later
      const res = await invoke<any>('get_denuvo_token_from_server', { 
        ticketContent, 
        serverUrl 
      })
      
      if (!res.success || !res.token) {
        throw new Error(res.message || 'Server không thể tạo Token.')
      }

      // 5. Apply token to anadius.cfg
      setMessage('Đang ghi Token vào cấu hình (anadius.cfg)...')
      await invoke('apply_denuvo_token_to_cfg', { 
        cfgPath, 
        token: res.token 
      })
      
      // 6. Relaunch the game directly to play
      setMessage('Kích hoạt thành công! Đang vào game...')
      await invoke('launch_game_executable', { gameDir, exeName: 'FC26.exe', silent: false })
      
      setStatus('success')
      
      // Reset after a while
      setTimeout(() => {
        setStatus('idle')
        setProgress(0)
      }, 5000)
    } catch (e: any) {
      console.error(e)
      setStatus('error')
      setMessage(String(e))
    }
  }, [gameDir, cfgPath])

  const isLoading = ['downloading', 'extracting', 'scanning', 'generating'].includes(status)
  const disableButton = isLoading || status === 'success'

  return (
    <div className="denuvo-activation" style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginLeft: '12px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <button 
          className={`secondary-control ${isLoading ? 'loading' : ''}`}
          type="button" 
          onClick={handleActivate}
          disabled={disableButton}
        >
          {status === 'success' ? <CheckCircle size={15} /> : <KeyRound size={15} />}
          <span>Active Denuvo</span>
        </button>
        
        {status !== 'idle' && (
          <div style={{ fontSize: '13px', color: status === 'error' ? 'var(--danger-color, #ff4d4f)' : status === 'success' ? 'var(--success-color, #52c41a)' : 'var(--text-secondary)' }}>
            {isLoading ? <Loader2 size={13} className="spin" style={{ display: 'inline', marginRight: '4px' }} /> : null}
            {status === 'error' ? <AlertCircle size={13} style={{ display: 'inline', marginRight: '4px' }} /> : null}
            {message}
          </div>
        )}
      </div>

      {(status === 'downloading' || status === 'extracting') && (
        <div style={{ width: '100%', maxWidth: '300px', background: 'var(--surface-sunken)', height: '6px', borderRadius: '4px', overflow: 'hidden' }}>
          <div 
            style={{ 
              height: '100%', 
              background: 'var(--brand-color)', 
              width: `${progress}%`,
              transition: 'width 0.2s ease-out'
            }} 
          />
          <div style={{ fontSize: '11px', color: 'var(--text-muted)', marginTop: '4px', textAlign: 'right' }}>
            {status === 'downloading' ? `${progress}%` : 'Giải nén...'}
          </div>
        </div>
      )}
    </div>
  )
}
