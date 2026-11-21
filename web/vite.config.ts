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
    proxy: {
      // The old TypeScript API; replaced by the Rust APIs in milestone 1 and 2.
      '/api': 'http://localhost:5001',
    },
  },
  build: {
    sourcemap: true,
  },
})
