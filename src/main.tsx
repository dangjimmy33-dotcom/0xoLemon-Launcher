import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { PhoneApp } from './PhoneApp.tsx'
import { isTauriRuntime } from './lib/gameMeta.ts'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isTauriRuntime() ? <App /> : <PhoneApp />}
  </StrictMode>,
)

// Show the Tauri window only after React has painted — prevents FOUC
if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
  import('@tauri-apps/api/webviewWindow').then(({ getCurrentWebviewWindow }) => {
    getCurrentWebviewWindow().show().catch(() => undefined)
  }).catch(() => undefined)
}
