import { defineConfig, devices } from '@playwright/test'

// E2E runs against a built preview server locally; in CI/compose point
// PLAYWRIGHT_BASE_URL at the running stack instead.
const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:4173'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  reporter: 'list',
  use: {
    baseURL,
    trace: 'on-first-retry',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: process.env.PLAYWRIGHT_BASE_URL
    ? undefined
    : {
        command: 'npm run build && npm run preview -- --port 4173',
        url: baseURL,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
})
