import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '~': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    // Vite matches proxy keys in order, so the daemon's /api/v1 must come before /api.
    proxy: {
      '/api/v1': {
        target: process.env.FANGST_DL_URL ?? 'http://127.0.0.1:7740',
        // The queue's live updates come over a WebSocket on /api/v1/events.
        ws: true,
      },
      // The old TypeScript API; replaced by the Rust APIs in milestone 1 and 2.
      '/api': 'http://localhost:5001',
    },
  },
  build: {
    sourcemap: true,
  },
})
