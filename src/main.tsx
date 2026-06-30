import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import Overlay from './Overlay.tsx'
import { Analytics } from '@vercel/analytics/react'

const LEGACY_PWA_RELOAD_KEY = '0xolemon-legacy-pwa-cleared-v1'

async function clearLegacyPwaStateInTauri() {
  if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) return false

  let hadController = false

  try {
    if ('serviceWorker' in navigator) {
      hadController = Boolean(navigator.serviceWorker.controller)
      const registrations = await navigator.serviceWorker.getRegistrations()
      await Promise.all(registrations.map((registration) => registration.unregister()))
    }
  } catch (error) {
    console.warn('Unable to unregister legacy service workers:', error)
  }

  try {
    if ('caches' in window) {
      const cacheNames = await caches.keys()
      await Promise.all(cacheNames.map((cacheName) => caches.delete(cacheName)))
    }
  } catch (error) {
    console.warn('Unable to clear legacy PWA caches:', error)
  }

  if (hadController && sessionStorage.getItem(LEGACY_PWA_RELOAD_KEY) !== '1') {
    sessionStorage.setItem(LEGACY_PWA_RELOAD_KEY, '1')
    window.location.reload()
    return true
  }

  sessionStorage.removeItem(LEGACY_PWA_RELOAD_KEY)
  return false
}

import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'

async function bootstrap() {
  if (await clearLegacyPwaStateInTauri()) return

  let isOverlay = false;
  try {
    isOverlay = getCurrentWebviewWindow().label === 'overlay';
  } catch (e) {
    // Ignore error if not running in Tauri
  }

  if (isOverlay) {
    document.body.classList.add('is-overlay-window');
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      {isOverlay ? <Overlay /> : <App />}
      <Analytics />
    </StrictMode>,
  )

  // Show the Tauri window only after React has painted — prevents FOUC
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    document.addEventListener('contextmenu', e => {
      const target = e.target as HTMLElement
      if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA') return
      e.preventDefault()
    })
    
    import('@tauri-apps/api/webviewWindow').then(({ getCurrentWebviewWindow }) => {
      getCurrentWebviewWindow().show().catch(() => undefined)
    }).catch(() => undefined)
  }
}

void bootstrap()
