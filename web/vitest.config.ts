import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    // Cap forks for 16 GB hosts (jsdom workers are ~0.5 GiB each; see docs/testing.md memory caps).
    maxWorkers: 4,
  },
})
