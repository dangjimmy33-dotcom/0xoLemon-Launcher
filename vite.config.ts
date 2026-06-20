import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import fs from 'fs'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    {
      name: 'cleanup-assets-for-tauri',
      closeBundle() {
        if (process.env.VERCEL !== '1') {
          // Delete heavy game assets from Vite's dist if building locally for Tauri.
          // Tauri serves them from its own .0xo packs, so keeping them in dist duplicates size.
          const assetsDir = path.resolve(__dirname, 'dist/assets')
          if (fs.existsSync(assetsDir)) {
            const files = fs.readdirSync(assetsDir)
            for (const file of files) {
              if (file.match(/^(grid|hero|logo|icon)-.*\.(png|jpg|jpeg|webp|gif|ico)$/i)) {
                fs.unlinkSync(path.join(assetsDir, file))
              }
            }
          }
        }
      }
    }
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
})
