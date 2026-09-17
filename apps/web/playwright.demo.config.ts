import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests', testMatch: 'demo-complete.spec.ts',
  fullyParallel: false, forbidOnly: !!process.env.CI,
  retries: 0, workers: 1, timeout: 90_000, expect: { timeout: 10_000 },
  reporter: [['list'], ['json', { outputFile: 'test-results/demo-results.json' }]],
  use: {
    baseURL: 'http://127.0.0.1:4179', browserName: 'chromium', locale: 'zh-CN',
    timezoneId: 'UTC', reducedMotion: 'reduce', serviceWorkers: 'block',
    viewport: { width: 1440, height: 900 }, trace: 'off', screenshot: 'off', video: 'off',
  },
  webServer: {
    command: 'npm run demo:preview', url: 'http://127.0.0.1:4179', reuseExistingServer: false, timeout: 30_000,
  },
});
