import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Analytics } from '@vercel/analytics/react'
import { WebApp } from './WebApp'
import './web.css'

async function registerWebPwa() {
  if (!('serviceWorker' in navigator)) return
  try {
    const { registerSW } = await import('virtual:pwa-register')
    registerSW({ immediate: true })
  } catch (error) {
    console.warn('Unable to register the web service worker:', error)
  }
}

export async function bootstrapWeb() {
  document.documentElement.dataset.surface = 'web'
  const analyticsEnabled = !['127.0.0.1', 'localhost'].includes(window.location.hostname)
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <WebApp />
      {analyticsEnabled ? <Analytics /> : null}
    </StrictMode>,
  )
  void registerWebPwa()
}
