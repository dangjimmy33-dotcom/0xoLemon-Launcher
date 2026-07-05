import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { CheckCircle2 } from 'lucide-react'
import type { GameCatalog } from '../types'
import { assetUrlForId } from '../lib/gameMeta'
import { useSteamAppIds } from '../hooks/useSteamAppIds'
import '../App.css'

const OFFLINE_GAMES = [
  "007 First Light", "Atomfall", "Atomic Heart", "Black Myth: Wukong",
  "Borderlands 4", "Bravely Default Flying Fairy", "Civilization VII",
  "CODE VEIN II", "Construction Simulator", "Crimson Desert",
  "Demon Slayer The Hinokami Chronicles", "Demon Slayer The Hinokami Chronicles 2",
  "Digimon Story Time Stranger", "DIRT 5", "DRAGON QUEST VII Reimagined",
  "Dragon's Dogma 2", "EA SPORTS FC™ 24", "EA SPORTS FC™ 25",
  "EA SPORTS™ WRC", "Enotria: The Last Song", "Expeditions: A MudRunner Game",
  "F1® 22", "F1® 23", "F1® 24", "F1® Manager 2024", "Faaast Penguin",
  "Fatal Fury: City of the Wolves", "FINAL FANTASY XVI",
  "Football Manager 2024", "Hello Neighbor 3", "Hi-Fi RUSH",
  "Hogwarts Legacy", "inZOI", "Judgment", "Lies of P",
  "Like a Dragon: Infinite Wealth", "Like a Dragon: Ishin!",
  "Like a Dragon: Pirate Yakuza in Hawaii", "Like a Dragon Gaiden: The Man Who Erased His Name",
  "Lost Judgment", "LUMINES REMASTERED", "Madden NFL 24",
  "Madden NFL 25", "Marvel's Midnight Suns", "Mecha BREAK",
  "Metaphor: ReFantazio", "Monster Hunter Wilds",
  "Mortal Kombat 1", "Need for Speed™ Unbound", "Neva",
  "New Arc Line", "PAYDAY 3", "Persona 3 Reload",
  "Persona 4 Golden", "Persona 5 Royal", "Planet Coaster 2",
  "Rogue Point", "Romance of the Three Kingdoms 8 Remake",
  "Sniper Elite: Resistance", "Sonic X Shadow Generations",
  "Soul Hackers 2", "Star Wars Jedi: Survivor", "Tails of Iron 2: Whiskers of Winter",
  "Taxi Life: A City Driving Simulator", "The First Berserker: Khazan",
  "Total War: WARHAMMER III", "Two Point Campus", "Two Point Museum",
  "Undisputed", "Warhammer 40,000: Chaos Gate - Daemonhunters",
  "Warhammer Age of Sigmar", "Yakuza Kiwami 3 & Dark Ties"
]

export function OfflineActivation({
  catalog,
  assets
}: {
  catalog: GameCatalog
  assets: Record<string, string>
}) {
  const [installedApps, setInstalledApps] = useState<number[]>([])
  const { mapping } = useSteamAppIds()

  useEffect(() => {
    invoke<number[]>('get_installed_steam_apps')
      .then(setInstalledApps)
      .catch(console.error)
  }, [])

  // Filter games from catalog that match the offline games list
  const offlineGames = catalog.games.filter(g => OFFLINE_GAMES.includes(g.title))

  return (
    <div className="offline-activation-panel" style={{ padding: '20px', color: 'white' }}>
      <h1>Offline Activation</h1>
      <p style={{ opacity: 0.7, marginBottom: '20px' }}>Activate your Steam games for offline play.</p>
      <div className="game-grid" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '20px' }}>
        {offlineGames.map(game => {
          const appid = mapping[game.id]
          const isInstalled = !!appid && installedApps.includes(appid)
          const hero = assetUrlForId(game.heroAssetId, assets)

          return (
            <div key={game.id} className="game-card" style={{ background: 'rgba(255,255,255,0.05)', borderRadius: '8px', overflow: 'hidden' }}>
              <img src={hero || ''} alt={game.title} style={{ width: '100%', height: '120px', objectFit: 'cover' }} />
              <div style={{ padding: '10px' }}>
                <h3 style={{ fontSize: '14px', margin: '0 0 6px 0', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{game.title}</h3>
                {appid && (
                  <span style={{ color: '#888', fontSize: '11px', display: 'block', marginBottom: '4px' }}>AppID: {appid}</span>
                )}
                {isInstalled ? (
                  <span style={{ color: '#4ade80', fontSize: '12px', display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <CheckCircle2 size={14} /> Installed on Steam
                  </span>
                ) : (
                  <span style={{ color: '#888', fontSize: '12px' }}>Not installed</span>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
