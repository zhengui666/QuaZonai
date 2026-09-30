import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Transport fixtures exercise real Vite dev prebundling and production chunks.
// The separate native suite still owns Rust/PostgreSQL/PWA account acceptance.
export default defineConfig({
  testDir: './tests', testMatch: ['validator-loading.spec.ts'], workers: 1, retries: 0,
  timeout: 60_000, expect: { timeout: 15_000 }, forbidOnly: Boolean(process.env.CI),
  outputDir: join(tmpdir(), 'quazonai-validator-ui'),
  use: { serviceWorkers: 'block', trace: 'off', screenshot: 'off', video: 'off' },
  projects: [
    { name: 'normal-dev-prebundle', use: { baseURL: 'http://127.0.0.1:4175' } },
    { name: 'production-chunks', use: { baseURL: 'http://127.0.0.1:4176' } },
  ],
  webServer: [
    { command: 'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 4175 --strictPort --force',
      url: 'http://127.0.0.1:4175', reuseExistingServer: false, timeout: 120_000 },
    { command: 'node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port 4176 --strictPort',
      url: 'http://127.0.0.1:4176', reuseExistingServer: false },
  ],
});
