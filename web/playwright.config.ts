import { defineConfig, devices } from '@playwright/test'

const ci = process.env.CI !== undefined

export default defineConfig({
  testDir: './e2e',
  outputDir: './test-results',
  // Starts one fangst-dl and one Vite server for the whole run; see e2e/global-setup.ts.
  globalSetup: './e2e/global-setup.ts',
  workers: 1,
  fullyParallel: false,
  forbidOnly: ci,
  retries: 0,
  reporter: ci ? [['list'], ['github']] : 'list',
  use: {
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
