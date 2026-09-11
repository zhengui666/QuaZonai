import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests', testIgnore: '**/native-console.spec.ts',
  fullyParallel: true, forbidOnly: !!process.env.CI,
  retries: 0, workers: 2, timeout: 30_000, expect: { timeout: 10_000 },
  reporter: [['list'], ['json', { outputFile: 'test-results/results.json' }]],
  use: {
    baseURL: 'http://127.0.0.1:4173', browserName: 'chromium', locale: 'zh-CN',
    timezoneId: 'UTC', reducedMotion: 'reduce', serviceWorkers: 'block',
    trace: 'off', screenshot: 'off', video: 'off',
  },
  projects: [
    { name: 'desktop-1440', use: { viewport: { width: 1440, height: 900 } } },
    { name: 'tablet-768', use: { viewport: { width: 768, height: 1024 } } },
    { name: 'mobile-390', use: { viewport: { width: 390, height: 844 } } },
  ],
  webServer: [
    { command: 'npm run preview', url: 'http://127.0.0.1:4173', reuseExistingServer: false, timeout: 30_000 },
    { command: 'node tests/pwa-server.mjs', url: 'http://127.0.0.1:4180/__fixture__/health', reuseExistingServer: false, timeout: 30_000 },
  ],
});
