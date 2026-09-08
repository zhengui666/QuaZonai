import { defineConfig, devices } from '@playwright/test';
import { dirname, isAbsolute, resolve } from 'node:path';

const origin = process.env.QUAZONAI_WEB_E2E_ORIGIN;
const fixture = process.env.QUAZONAI_WEB_E2E_FIXTURE;
if (!origin || !fixture || !isAbsolute(fixture)) {
  throw new Error('Run npm run test:e2e:native; a fresh private Rust/PostgreSQL fixture is required');
}
const url = new URL(origin);
if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port
  || url.username || url.password || url.pathname !== '/' || url.search || url.hash) {
  throw new Error('Native browser acceptance requires its own loopback HTTP origin');
}

export default defineConfig({
  testDir: './tests',
  testMatch: '**/native-console.spec.ts',
  // Playwright can emit error-context.md even with screenshots/trace disabled.
  // Keep every raw failure artifact inside the harness-owned private directory.
  outputDir: resolve(dirname(fixture), 'playwright-output'),
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: Boolean(process.env.CI),
  timeout: 120_000,
  expect: { timeout: 20_000 },
  reporter: [['list']],
  use: {
    ...devices['Desktop Chrome'],
    baseURL: url.origin,
    viewport: { width: 1440, height: 1000 },
    trace: 'off', screenshot: 'off', video: 'off',
  },
  projects: [{ name: 'real-rust-postgres', use: { browserName: 'chromium' } }],
});
