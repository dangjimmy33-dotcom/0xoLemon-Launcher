import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import Overlay from './Overlay.tsx'
import { Analytics } from '@vercel/analytics/react'
import { LocaleProvider } from './context/LocaleContext'

function isTauriRuntime() {
  if (typeof window === 'undefined') return false

  const tauriWindow = window as Window & {
    __TAURI__?: unknown
    __TAURI_INTERNALS__?: unknown
  }

  return Boolean(
    tauriWindow.__TAURI__ ||
    tauriWindow.__TAURI_INTERNALS__ ||
    window.location.protocol === 'tauri:' ||
    window.location.protocol === 'asset:' ||
    window.location.hostname === 'tauri.localhost'
  )
}

async function clearLegacyPwaStateInTauri() {
  if (!isTauriRuntime()) return

  // A service worker registered on the localhost origin can survive a launcher
  // update and serve an old HTML/asset graph. Remove it, but never reload the
  // WebView during bootstrap: WebView2 reloads at this stage have caused blank
  // windows / STATUS_ACCESS_VIOLATION on some Windows machines.
  try {
    if ('serviceWorker' in navigator) {
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
}

import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'

async function bootstrap() {
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
      <LocaleProvider>
        {isOverlay ? <Overlay /> : <App />}
        <Analytics />
      </LocaleProvider>
    </StrictMode>,
  )

  // Never block first paint on legacy cache cleanup. Some damaged WebView2
  // profiles can leave service-worker/cache promises pending indefinitely.
  void clearLegacyPwaStateInTauri()

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

function renderBootstrapFailure(error: unknown) {
  console.error('Launcher bootstrap failed:', error)

  const root = document.getElementById('root')
  if (root) {
    root.replaceChildren()
    const panel = document.createElement('main')
    panel.style.cssText = [
      'min-height:100vh',
      'display:grid',
      'place-items:center',
      'background:#080d12',
      'color:#f5f7fa',
      'font-family:Inter,Segoe UI,sans-serif',
      'padding:32px',
      'box-sizing:border-box',
    ].join(';')

    const card = document.createElement('section')
    card.style.cssText = 'max-width:680px;border:1px solid #34404b;background:#0e151c;padding:24px;border-radius:10px'

    const title = document.createElement('h1')
    title.textContent = '0xoLemon could not finish starting / Không thể khởi động launcher'
    title.style.cssText = 'font-size:20px;margin:0 0 12px'

    const message = document.createElement('p')
    message.textContent = 'Please close 0xoLemon in Task Manager and open it again. If this repeats, send the download-debug.log file to support. / Hãy tắt 0xoLemon trong Task Manager rồi mở lại. Nếu vẫn lỗi, gửi file download-debug.log cho hỗ trợ.'
    message.style.cssText = 'line-height:1.6;color:#b9c3cc;margin:0 0 14px'

    const detail = document.createElement('pre')
    detail.textContent = String(error)
    detail.style.cssText = 'white-space:pre-wrap;word-break:break-word;color:#ffb4a9;background:#090e13;padding:12px;border-radius:6px;margin:0'

    card.append(title, message, detail)
    panel.append(card)
    root.append(panel)
  }

  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    import('@tauri-apps/api/webviewWindow')
      .then(async ({ getCurrentWebviewWindow }) => {
        const current = getCurrentWebviewWindow()
        await current.show().catch(() => undefined)
        await current.unminimize().catch(() => undefined)
        await current.setFocus().catch(() => undefined)
      })
      .catch(() => undefined)
  }
}

void bootstrap().catch(renderBootstrapFailure)
