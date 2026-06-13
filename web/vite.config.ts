import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 5001,
    proxy: {
      '/api': {
        target: process.env.VITE_API_URL ?? 'http://localhost:5000',
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('error', () => {});
        },
      },
      '/ws': {
        target: process.env.VITE_API_URL ?? 'http://localhost:5000',
        changeOrigin: true,
        ws: true,
        configure: (proxy) => {
          // Upstream may close WS early (e.g. after end frame) or restart during dev.
          proxy.on('error', () => {});
          proxy.on('close', () => {});
          proxy.on('proxyReqWs', (_proxyReq, _req, socket) => {
            socket.on('error', () => {});
          });
        },
      },
    },
  },
})
