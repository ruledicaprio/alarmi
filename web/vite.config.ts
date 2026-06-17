import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// base './' so the built dist works when served by bht-api (same origin).
// dev proxy forwards API calls to the local bht-api.
export default defineConfig({
  plugins: [react()],
  base: './',
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
      '/ingest': 'http://localhost:8080',
    },
  },
})
