import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Trash2 } from 'lucide-react'

interface SteamGameInfo {
  name: string
  header_image: string
}

// Simple in-memory cache so we don't re-fetch on re-renders
const nameCache = new Map<string, SteamGameInfo>()

async function fetchGameInfo(appid: string): Promise<SteamGameInfo | null> {
  if (nameCache.has(appid)) return nameCache.get(appid)!

  // 1. Try Rust command first (no CORS, uses obfuscated API key)
  try {
    const result = await invoke<SteamGameInfo>('fetch_steam_game_name', { appid: parseInt(appid) })
    if (result?.name) {
      nameCache.set(appid, result)
      return result
    }
  } catch (_) {
    // Rust command not available (dev mode / hot-reload) — fall through
  }

  // 2. Fallback: Steam storefront API (no key needed for basic name lookup)
  try {
    const res = await fetch(
      `https://store.steampowered.com/api/appdetails?appids=${appid}&filters=basic`,
      { signal: AbortSignal.timeout(6000) }
    )
    if (res.ok) {
      const data = await res.json()
      const entry = data?.[appid]
      if (entry?.success && entry?.data?.name) {
        const info: SteamGameInfo = {
          name: entry.data.name,
          header_image: entry.data.header_image || `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`
        }
        nameCache.set(appid, info)
        return info
      }
    }
  } catch (_) { /* network error */ }

  return null
}

export function LuaGameItem({ appid, onRemoved, onNameLoaded }: { appid: string, onRemoved: () => void, onNameLoaded?: (name: string) => void }) {
  const [info, setInfo] = useState<SteamGameInfo | null>(nameCache.get(appid) ?? null)
  const [isLoading, setIsLoading] = useState(!nameCache.has(appid))

  useEffect(() => {
    if (nameCache.has(appid)) {
      onNameLoaded?.(nameCache.get(appid)!.name)
      return
    }
    let mounted = true
    fetchGameInfo(appid).then(result => {
      if (mounted) {
        setInfo(result)
        setIsLoading(false)
        if (result?.name) onNameLoaded?.(result.name)
      }
    })
    return () => { mounted = false }
  }, [appid, onNameLoaded])

  const handleRemove = async () => {
    const displayName = info?.name || appid
    if (confirm(`Are you sure you want to remove lua for ${displayName}?`)) {
      try {
        await invoke('remove_from_steam', { appid: parseInt(appid) })
        onRemoved()
      } catch(e) {
        alert('Failed to remove: ' + e)
      }
    }
  }

  const imageUrl = info?.header_image
    || `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/header.jpg`

  return (
    <div className="lua-game-row" style={{
      display: 'flex',
      alignItems: 'center',
      background: 'rgba(255,255,255,0.04)',
      borderRadius: '8px',
      border: '1px solid rgba(255,255,255,0.06)',
      overflow: 'hidden',
      transition: 'background 0.15s',
      height: '52px',
      flexShrink: 0,
    }}
    onMouseEnter={e => (e.currentTarget.style.background = 'rgba(255,255,255,0.07)')}
    onMouseLeave={e => (e.currentTarget.style.background = 'rgba(255,255,255,0.04)')}
    >
      {/* Thumbnail */}
      <div style={{ width: '92px', height: '52px', flexShrink: 0, background: 'rgba(0,0,0,0.3)', overflow: 'hidden' }}>
        {isLoading ? (
          <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <div style={{ width: '18px', height: '18px', border: '2px solid rgba(255,255,255,0.1)', borderTopColor: 'rgba(255,255,255,0.5)', borderRadius: '50%', animation: 'spin 0.8s linear infinite' }} />
          </div>
        ) : (
          <img
            src={imageUrl}
            alt={info?.name || appid}
            style={{ width: '100%', height: '100%', objectFit: 'cover', display: 'block' }}
            onError={(e) => {
              const t = e.target as HTMLImageElement
              if (!t.src.includes('capsule_231x87')) {
                t.src = `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${appid}/capsule_231x87.jpg`
              } else {
                t.style.opacity = '0'
              }
            }}
          />
        )}
      </div>

      {/* Name + AppID */}
      <div style={{ flex: 1, padding: '0 12px', minWidth: 0 }}>
        <div style={{ fontSize: '13px', fontWeight: 600, color: '#ddd', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
          {isLoading ? <span style={{ color: '#555' }}>Loading...</span> : (info?.name || <span style={{ color: '#666', fontStyle: 'italic' }}>AppID {appid}</span>)}
        </div>
        <div style={{ fontSize: '11px', color: '#555', fontFamily: 'monospace', marginTop: '2px' }}>
          {appid}.lua
        </div>
      </div>

      {/* Delete */}
      <button
        onClick={handleRemove}
        title="Remove this lua"
        style={{
          background: 'transparent', color: '#ef4444', border: 'none',
          padding: '0 14px', height: '100%', cursor: 'pointer',
          display: 'flex', alignItems: 'center', flexShrink: 0,
          transition: 'background 0.15s'
        }}
        onMouseEnter={e => (e.currentTarget.style.background = 'rgba(239,68,68,0.12)')}
        onMouseLeave={e => (e.currentTarget.style.background = 'transparent')}
      >
        <Trash2 size={15} />
      </button>
    </div>
  )
}
