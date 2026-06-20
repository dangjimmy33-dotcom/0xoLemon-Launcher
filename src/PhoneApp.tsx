import { useState, useEffect } from 'react'
import { doc, onSnapshot, addDoc, collection, serverTimestamp } from 'firebase/firestore'
import { db } from './firebase'
import type { GameCatalog, GameInstallState, GameRuntimeState } from './types'
import { motion } from 'framer-motion'
import { assetUrlForId } from './lib/gameMeta'
import { gameHasTag } from './lib/gameTags'
import { Play, Download, LogOut, Monitor, Smartphone, Joystick } from 'lucide-react'

export function PhoneApp() {
  const [discordId, setDiscordId] = useState(() => localStorage.getItem('phone_discord_id') || '')
  const [catalog, setCatalog] = useState<GameCatalog | null>(null)
  const [installStates, setInstallStates] = useState<Record<string, GameInstallState>>({})
  const [runtimeStates, setRuntimeStates] = useState<GameRuntimeState[]>([])
  const [assets, setAssets] = useState<Record<string, string>>({})
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)

  useEffect(() => {
    if (!discordId) return
    const unsub = onSnapshot(doc(db, 'users', discordId, 'pc_state', 'current'), (snapshot) => {
      if (snapshot.exists()) {
        const data = snapshot.data()
        if (data.catalog) setCatalog(data.catalog)
        if (data.installStates) setInstallStates(data.installStates)
        if (data.runtimeStates) {
          if (Array.isArray(data.runtimeStates)) {
            setRuntimeStates(data.runtimeStates)
          } else {
            setRuntimeStates(Object.values(data.runtimeStates))
          }
        }
        if (data.assets) setAssets(data.assets)
      }
    })
    return unsub
  }, [discordId])

  const sendCommand = async (action: 'install' | 'launch', gameId: string) => {
    if (!discordId) return
    await addDoc(collection(db, 'users', discordId, 'commands'), {
      action,
      game_id: gameId,
      timestamp: serverTimestamp()
    })
    if (navigator.vibrate) navigator.vibrate(50)
  }

  if (!discordId) {
    return <LoginView onConnect={setDiscordId} />
  }

  if (!catalog) {
    return <LoadingView onDisconnect={() => setDiscordId('')} />
  }

  const selectedGame = selectedGameId ? catalog.games.find(g => g.id === selectedGameId) : catalog.games[0]

  return (
    <div style={{ backgroundColor: '#09090b', color: '#fff', minHeight: '100vh', fontFamily: 'system-ui, -apple-system, sans-serif', paddingBottom: 80, overflowX: 'hidden' }}>
      {/* Header */}
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '20px 24px', position: 'sticky', top: 0, background: 'rgba(9,9,11,0.8)', backdropFilter: 'blur(12px)', zIndex: 10 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ width: 10, height: 10, borderRadius: '50%', backgroundColor: '#22c55e', boxShadow: '0 0 10px #22c55e' }} />
          <h1 style={{ margin: 0, fontSize: 18, fontWeight: 600, letterSpacing: '-0.5px' }}>PC Connected</h1>
        </div>
        <button 
          onClick={() => { localStorage.removeItem('phone_discord_id'); setDiscordId(''); }}
          style={{ background: 'transparent', border: 'none', color: '#a1a1aa', cursor: 'pointer', padding: 8 }}
        >
          <LogOut size={20} />
        </button>
      </header>

      {/* Hero Section */}
      {selectedGame && (
        <HeroSection 
          key={selectedGame.id}
          game={selectedGame} 
          assets={assets} 
          installState={installStates[selectedGame.id]} 
          runtimeState={runtimeStates.find(s => s.gameId === selectedGame.id)}
          onCommand={sendCommand}
        />
      )}

      {/* Grid */}
      <div style={{ padding: '0 24px', marginTop: 32 }}>
        <h2 style={{ fontSize: 20, fontWeight: 700, marginBottom: 16, letterSpacing: '-0.5px' }}>Library</h2>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(140px, 1fr))', gap: 16 }}>
          {catalog.games.map(game => (
            <GameCard 
              key={game.id} 
              game={game} 
              assets={assets} 
              isSelected={game.id === selectedGameId}
              onClick={() => setSelectedGameId(game.id)}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function HeroSection({ game, assets, installState, runtimeState, onCommand }: any) {
  const heroUrl = assetUrlForId(game.heroAssetId, assets) || assetUrlForId(game.gridAssetId, assets)
  const isComingSoon = gameHasTag(game.id, 'coming soon')
  const isRunning = !!runtimeState?.running
  const isInstalled = !!installState?.installed

  return (
    <motion.div 
      initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5, ease: 'easeOut' }}
      style={{ margin: '0 24px', position: 'relative', borderRadius: 24, overflow: 'hidden', height: 380, boxShadow: '0 20px 40px rgba(0,0,0,0.4)' }}
    >
      {heroUrl ? (
        <div style={{ position: 'absolute', inset: 0, background: `url(${heroUrl}) center/cover no-repeat` }} />
      ) : (
        <div style={{ position: 'absolute', inset: 0, background: '#18181b', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <Joystick size={64} color="#3f3f46" />
        </div>
      )}
      <div style={{ position: 'absolute', inset: 0, background: 'linear-gradient(to top, rgba(9,9,11,1) 0%, rgba(9,9,11,0.2) 60%, rgba(9,9,11,0) 100%)' }} />
      
      <div style={{ position: 'absolute', bottom: 24, left: 24, right: 24 }}>
        <motion.h2 
          key={game.title}
          initial={{ opacity: 0, x: -10 }} animate={{ opacity: 1, x: 0 }}
          style={{ fontSize: 28, fontWeight: 800, margin: '0 0 4px 0', lineHeight: 1.1, textShadow: '0 2px 10px rgba(0,0,0,0.8)' }}
        >
          {game.title}
        </motion.h2>
        <p style={{ margin: '0 0 20px 0', color: '#d4d4d8', fontSize: 14 }}>{game.developer}</p>

        {isComingSoon ? (
          <button disabled style={{ width: '100%', padding: '16px', borderRadius: 16, background: 'rgba(255,255,255,0.1)', color: '#fff', border: '1px solid rgba(255,255,255,0.2)', fontWeight: 600, fontSize: 16, backdropFilter: 'blur(10px)' }}>
            Coming Soon
          </button>
        ) : isRunning ? (
          <button disabled style={{ width: '100%', padding: '16px', borderRadius: 16, background: '#22c55e', color: '#fff', border: 'none', fontWeight: 600, fontSize: 16, display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 8, boxShadow: '0 0 20px rgba(34,197,94,0.4)' }}>
            <Monitor size={20} /> Playing on PC
          </button>
        ) : isInstalled ? (
          <motion.button 
            whileTap={{ scale: 0.95 }} onClick={() => onCommand('launch', game.id)}
            style={{ width: '100%', padding: '16px', borderRadius: 16, background: '#fff', color: '#000', border: 'none', fontWeight: 700, fontSize: 16, display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 8 }}
          >
            <Play size={20} fill="currentColor" /> Launch on PC
          </motion.button>
        ) : (
          <motion.button 
            whileTap={{ scale: 0.95 }} onClick={() => onCommand('install', game.id)}
            style={{ width: '100%', padding: '16px', borderRadius: 16, background: '#3b82f6', color: '#fff', border: 'none', fontWeight: 700, fontSize: 16, display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 8 }}
          >
            <Download size={20} /> Remote Install
          </motion.button>
        )}
      </div>
    </motion.div>
  )
}

function GameCard({ game, assets, isSelected, onClick }: any) {
  const isComingSoon = gameHasTag(game.id, 'coming soon')
  const gridUrl = assetUrlForId(game.gridAssetId, assets)

  return (
    <motion.div 
      whileTap={!isComingSoon ? { scale: 0.95 } : {}}
      onClick={() => !isComingSoon && onClick()}
      style={{ 
        position: 'relative', 
        borderRadius: 16, 
        overflow: 'hidden', 
        aspectRatio: '2/3', 
        cursor: isComingSoon ? 'not-allowed' : 'pointer',
        border: isSelected ? '2px solid #fff' : '2px solid transparent',
        transition: 'border 0.2s',
        filter: isComingSoon ? 'brightness(0.5) grayscale(0.8)' : 'none',
        opacity: isComingSoon ? 0.7 : 1
      }}
    >
      {gridUrl ? (
        <img src={gridUrl} alt={game.title} style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
      ) : (
        <div style={{ width: '100%', height: '100%', background: '#18181b', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <Joystick size={32} color="#3f3f46" />
        </div>
      )}
      <div style={{ position: 'absolute', inset: 0, background: 'linear-gradient(to top, rgba(0,0,0,0.8) 0%, transparent 50%)' }} />
      <div style={{ position: 'absolute', bottom: 12, left: 12, right: 12 }}>
        <h3 style={{ margin: 0, fontSize: 13, fontWeight: 600, color: '#fff', textOverflow: 'ellipsis', overflow: 'hidden', whiteSpace: 'nowrap' }}>{game.title}</h3>
      </div>
    </motion.div>
  )
}

function LoginView({ onConnect }: { onConnect: (id: string) => void }) {
  const [val, setVal] = useState('')
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100vh', background: '#09090b', color: 'white', fontFamily: 'system-ui, sans-serif' }}>
      <motion.div initial={{ scale: 0.9, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} style={{ width: '80%', maxWidth: 320, background: '#18181b', padding: 32, borderRadius: 24, border: '1px solid #27272a', boxShadow: '0 20px 40px rgba(0,0,0,0.5)', textAlign: 'center' }}>
        <Smartphone size={48} color="#5865F2" style={{ marginBottom: 24 }} />
        <h2 style={{ margin: '0 0 8px 0', fontSize: 24, fontWeight: 700 }}>Remote Play</h2>
        <p style={{ margin: '0 0 24px 0', color: '#a1a1aa', fontSize: 14 }}>Enter your Discord ID to pair with your PC launcher.</p>
        <input 
          type="text" placeholder="Discord ID" value={val} onChange={e => setVal(e.target.value)}
          style={{ width: '100%', boxSizing: 'border-box', padding: '14px 16px', borderRadius: 12, border: '1px solid #3f3f46', background: '#09090b', color: '#fff', fontSize: 16, marginBottom: 16, outline: 'none' }}
        />
        <motion.button 
          whileTap={{ scale: 0.95 }} onClick={() => { if(val) { localStorage.setItem('phone_discord_id', val); onConnect(val); } }}
          style={{ width: '100%', padding: '14px', borderRadius: 12, background: '#5865F2', color: '#fff', border: 'none', fontWeight: 600, fontSize: 16 }}
        >
          Connect
        </motion.button>
      </motion.div>
    </div>
  )
}

function LoadingView({ onDisconnect }: { onDisconnect: () => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100vh', background: '#09090b', color: 'white', fontFamily: 'system-ui, sans-serif' }}>
      <motion.div animate={{ rotate: 360 }} transition={{ repeat: Infinity, duration: 1, ease: 'linear' }} style={{ width: 40, height: 40, borderRadius: '50%', border: '3px solid #27272a', borderTopColor: '#5865F2', marginBottom: 24 }} />
      <p style={{ color: '#a1a1aa', fontWeight: 500 }}>Waiting for PC...</p>
      <button onClick={onDisconnect} style={{ position: 'absolute', bottom: 40, padding: '10px 20px', borderRadius: 20, background: 'rgba(255,255,255,0.1)', color: '#fff', border: 'none', fontSize: 14 }}>Cancel</button>
    </div>
  )
}
