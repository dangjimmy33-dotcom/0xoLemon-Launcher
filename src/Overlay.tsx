import { useEffect, useState } from 'react'
import { Settings, Users, Gamepad2, Mic, Headphones, X, MessageSquare, Trophy, Activity } from 'lucide-react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import './Overlay.css'

export default function Overlay() {
  const [time, setTime] = useState(new Date())

  useEffect(() => {
    const timer = setInterval(() => setTime(new Date()), 1000)
    return () => clearInterval(timer)
  }, [])

  return (
    <div className="overlay-wrapper">
      {/* Top pill navigation */}
      <div className="overlay-top-pill">
        <div className="pill-group">
          <button className="pill-btn active"><Gamepad2 size={20} /> <span>0xoLemon</span></button>
          <button className="pill-btn"><Users size={20} /> <span>Friends</span></button>
          <button className="pill-btn"><MessageSquare size={20} /> <span>Chat</span></button>
          <button className="pill-btn"><Trophy size={20} /> <span>Achievements</span></button>
          <button className="pill-btn"><Activity size={20} /> <span>Performance</span></button>
          <button className="pill-btn"><Settings size={20} /> <span>Settings</span></button>
        </div>
        <div className="pill-divider" />
        <div className="pill-group">
          <button className="pill-btn icon-only"><Mic size={20} /></button>
          <button className="pill-btn icon-only"><Headphones size={20} /></button>
          <button className="pill-btn icon-only danger" onClick={async () => {
            const win = getCurrentWebviewWindow()
            await win.setIgnoreCursorEvents(true)
            await win.hide()
          }}><X size={20} /></button>
        </div>
      </div>

      {/* Left sidebar - Friends/Voice */}
      <div className="overlay-widget left-widget">
        <div className="widget-header">
          <span className="widget-title">General Voice</span>
          <Settings size={16} className="text-gray" />
        </div>
        <div className="widget-content">
          <div className="voice-user">
            <div className="avatar"><img src="https://i.pravatar.cc/150?u=1" alt="avatar" /></div>
            <span className="user-name">Player One</span>
            <Mic size={14} className="icon-active" />
          </div>
          <div className="voice-user">
            <div className="avatar"><img src="https://i.pravatar.cc/150?u=2" alt="avatar" /></div>
            <span className="user-name">Lone Wanderer</span>
          </div>
          <div className="voice-user">
            <div className="avatar"><img src="https://i.pravatar.cc/150?u=3" alt="avatar" /></div>
            <span className="user-name">SnarkyBeard</span>
          </div>
        </div>
        <div className="widget-footer">
          <span className="status-text text-green">Voice Connected</span>
        </div>
      </div>

      {/* Right panel - Performance / Info */}
      <div className="overlay-widget right-widget">
         <div className="time-display">
            <h2>{time.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</h2>
            <p>{time.toLocaleDateString([], { weekday: 'long', month: 'short', day: 'numeric' })}</p>
         </div>
         <div className="performance-stats">
            <div className="stat-row"><span>FPS</span><span>144</span></div>
            <div className="stat-row"><span>CPU</span><span>45%</span></div>
            <div className="stat-row"><span>GPU</span><span>82%</span></div>
            <div className="stat-row"><span>RAM</span><span>12GB</span></div>
         </div>
      </div>
      
      <div className="overlay-toast">
        Press <b>Shift + F1</b> to close overlay
      </div>
    </div>
  )
}
