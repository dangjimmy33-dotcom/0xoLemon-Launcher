import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

import { VitePWA } from 'vite-plugin-pwa'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      // Registration is performed explicitly in main.tsx so the Vercel/PWA
      // build keeps offline support while the Tauri WebView never registers a
      // service worker on its localhost origin.
      injectRegister: false,
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,avif,webp,woff2,ttf}'],
        navigateFallback: '/index.html',
        runtimeCaching: [],
      },
      includeAssets: ['favicon.svg'],
      manifest: {
        name: '0xoLemon Launcher',
        short_name: '0xoLemon',
        description: 'Version-aware game delivery and authenticated Remote Web access.',
        theme_color: '#080b0e',
        background_color: '#080b0e',
        display: 'standalone',
        icons: [
          {
            src: 'favicon.svg',
            sizes: '192x192 512x512',
            type: 'image/svg+xml'
          }
        ]
      }
    })
  ],
  clearScreen: false,
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: 'firebase',
              test: /node_modules[\\/](?:@firebase|firebase)[\\/]/,
              priority: 20,
            },
          ],
        },
      },
    },
  },
  optimizeDeps: {
    exclude: [],
    entries: ['src/**/*.{ts,tsx,html}'],
  },
  server: {
    port: 1420,
    strictPort: true,
    fs: {
      deny: ['src-tauri'],
    },
    watch: {
      ignored: [
        '**/src-tauri/**',
        '**/node_modules/**',
      ],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
})
