import { defineConfig, devices } from '@playwright/test';

/**
 * Browser tests. The main set (`e2e`) runs against the static export (`pnpm ui
 * build` first): the UI works on a made-up graph in memory outside the app, so
 * each test starts from the same graph and nothing is shared between them. A
 * smaller set (`e2e-dev`) runs against `next dev`, which, unlike the export,
 * renders pages on a server, as the app does in development.
 */
export default defineConfig({
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? [['list'], ['github']] : 'list',
  use: { trace: 'retain-on-failure' },
  webServer: [
    {
      command: 'pnpm exec serve out --listen 4173 --no-clipboard',
      url: 'http://localhost:4173',
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'pnpm exec next dev --port 4174',
      url: 'http://localhost:4174',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
  ],
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], baseURL: 'http://localhost:4173' },
    },
    {
      name: 'dev',
      testDir: 'e2e-dev',
      use: { ...devices['Desktop Chrome'], baseURL: 'http://localhost:4174' },
    },
  ],
});
