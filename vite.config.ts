import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

import { VitePWA } from 'vite-plugin-pwa'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,woff2,ttf}'],
        runtimeCaching: [
          {
            urlPattern: /^https:\/\/firestore\.googleapis\.com\/.*/i,
            handler: 'NetworkFirst',
            options: {
              cacheName: 'firebase-firestore-cache',
              expiration: {
                maxEntries: 100,
                maxAgeSeconds: 60 * 60 * 24 * 7, // 7 days
              },
              cacheableResponse: {
                statuses: [0, 200],
              },
            },
          },
        ],
      },
      includeAssets: ['favicon.svg'],
      manifest: {
        name: '0xoLemon Store',
        short_name: '0xoLemon',
        description: '0xoLemon Store & Launcher',
        theme_color: '#0e1116',
        background_color: '#0e1116',
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
