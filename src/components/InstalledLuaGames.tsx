import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Trash2, RefreshCw } from 'lucide-react'

interface InstalledLuaGame {
  appid: string
  lua_exists: boolean
  manifest_exists: boolean
}

export function InstalledLuaGames() {
  const [games, setGames] = useState<InstalledLuaGame[]>([])
  const [loading, setLoading] = useState(true)
  const [removing, setRemoving] = useState<string | null>(null)

  const loadGames = async () => {
    setLoading(true)
    try {
      const installed = await invoke<InstalledLuaGame[]>('get_installed_lua_games')
      setGames(installed)
    } catch (error) {
      console.error('Failed to load installed games:', error)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadGames()
  }, [])

  const handleRemove = async (appid: string) => {
    if (!confirm(`Remove game ${appid} from Steam?`)) return

    setRemoving(appid)
    try {
      await invoke('remove_from_steam', { appid: parseInt(appid) })
      await loadGames()
    } catch (error) {
      alert(`Failed to remove: ${error}`)
    } finally {
      setRemoving(null)
    }
  }

  const getSteamImageUrl = (appid: string) => {
    return `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/header.jpg`
  }

  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: '60px 20px', color: '#888' }}>
        <RefreshCw size={32} style={{ animation: 'spin 1s linear infinite' }} />
        <p style={{ marginTop: '16px' }}>Loading installed games...</p>
      </div>
    )
  }

  if (games.length === 0) {
    return (
      <div style={{ textAlign: 'center', padding: '60px 20px', color: '#888' }}>
        <p>No Lua games installed yet.</p>
        <p style={{ fontSize: '14px', marginTop: '8px' }}>Install a game from the "Install" tab to see it here.</p>
      </div>
    )
  }

  return (
    <div style={{ padding: '20px' }}>
      <div style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        marginBottom: '20px'
      }}>
        <h2 style={{ margin: 0, fontSize: '18px', color: '#fff' }}>
          Installed Lua Games ({games.length})
        </h2>
        <button
          type="button"
          onClick={loadGames}
          style={{
            padding: '8px 12px',
            background: 'rgba(255,255,255,0.05)',
            border: '1px solid rgba(255,255,255,0.1)',
            borderRadius: '6px',
            color: '#fff',
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            fontSize: '14px'
          }}
        >
          <RefreshCw size={14} />
          Refresh
        </button>
      </div>

      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
        gap: '16px'
      }}>
        {games.map((game) => (
          <div
            key={game.appid}
            style={{
              background: 'rgba(255,255,255,0.03)',
              border: '1px solid rgba(255,255,255,0.1)',
              borderRadius: '8px',
              overflow: 'hidden',
              transition: 'all 0.2s ease'
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = 'rgba(255,255,255,0.05)'
              e.currentTarget.style.borderColor = 'rgba(255,255,255,0.2)'
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'rgba(255,255,255,0.03)'
              e.currentTarget.style.borderColor = 'rgba(255,255,255,0.1)'
            }}
          >
            <img
              src={getSteamImageUrl(game.appid)}
              alt={`Game ${game.appid}`}
              style={{
                width: '100%',
                height: '140px',
                objectFit: 'cover',
                background: '#1a1d20'
              }}
              onError={(e) => {
                e.currentTarget.src = 'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="460" height="215"%3E%3Crect fill="%231a1d20" width="460" height="215"/%3E%3Ctext fill="%23666" font-family="sans-serif" font-size="24" x="50%25" y="50%25" text-anchor="middle" dominant-baseline="middle"%3ENo Image%3C/text%3E%3C/svg%3E'
              }}
            />
            
            <div style={{ padding: '12px' }}>
              <div style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center'
              }}>
                <div>
                  <div style={{ color: '#fff', fontWeight: '600', marginBottom: '4px' }}>
                    AppID: {game.appid}
                  </div>
                  <div style={{ fontSize: '12px', color: '#888' }}>
                    {game.lua_exists && <span>✓ Lua </span>}
                    {game.manifest_exists && <span>✓ Manifest</span>}
                  </div>
                </div>

                <button
                  type="button"
                  onClick={() => handleRemove(game.appid)}
                  disabled={removing === game.appid}
                  style={{
                    padding: '8px 12px',
                    background: 'rgba(255,50,50,0.1)',
                    border: '1px solid rgba(255,50,50,0.3)',
                    borderRadius: '6px',
                    color: '#ff6b6b',
                    cursor: removing === game.appid ? 'not-allowed' : 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    fontSize: '13px',
                    opacity: removing === game.appid ? 0.5 : 1,
                    transition: 'all 0.2s'
                  }}
                  onMouseEnter={(e) => {
                    if (removing !== game.appid) {
                      e.currentTarget.style.background = 'rgba(255,50,50,0.2)'
                      e.currentTarget.style.borderColor = 'rgba(255,50,50,0.5)'
                    }
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = 'rgba(255,50,50,0.1)'
                    e.currentTarget.style.borderColor = 'rgba(255,50,50,0.3)'
                  }}
                >
                  <Trash2 size={14} />
                  {removing === game.appid ? 'Removing...' : 'Remove'}
                </button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
