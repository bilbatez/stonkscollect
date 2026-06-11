/// <reference types="vitest/config" />
import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  // Load `.env`* files (see .env.example). `''` prefix = expose all keys here in
  // the Node config; the app itself still only sees `VITE_`-prefixed vars.
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.VITE_API_TARGET || 'http://localhost:8080'

  return {
  plugins: [react()],
  server: {
    // Dev: forward backend calls untouched. The backend serves both `/api/*`
    // and `/auth/*`, so proxy both and DON'T rewrite the path.
    proxy: {
      '/api': { target, changeOrigin: true },
      '/auth': { target, changeOrigin: true },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    // MUI's .mjs does an extensionless directory import of react-transition-group
    // that Vitest's native ESM resolver rejects; inline MUI so Vite transforms
    // and resolves it.
    server: { deps: { inline: [/@mui/, /@emotion/, 'react-transition-group'] } },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/main.tsx',
        'src/vite-env.d.ts',
        'src/**/*.test.{ts,tsx}',
        'src/test/**',
        // Canvas chart wrappers: untestable rendering glue (echarts needs a
        // real canvas), analogous to the backend's network glue.
        'src/charts/**',
        'src/types.ts',
      ],
      thresholds: {
        lines: 100,
        functions: 100,
        branches: 100,
        statements: 100,
      },
    },
  },
  }
})
