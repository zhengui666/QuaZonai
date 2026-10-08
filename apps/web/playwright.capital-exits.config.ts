import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
// Component interactions against controlled transport fixtures. Native API,
// schema generation and Sandbox business-path acceptance are separate gates.
export default defineConfig({
  testDir: './tests', testMatch: ['capital-exits.spec.ts'], workers: 1, retries: 0,
  outputDir: join(tmpdir(), 'quazonai-capital-exit-ui'),
  use: { baseURL: 'http://127.0.0.1:4176', trace: 'off', screenshot: 'only-on-failure', video: 'off',
    launchOptions: process.env.CHROMIUM_BIN ? { executablePath: process.env.CHROMIUM_BIN } : {} },
  webServer: { command: 'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 4176 --strictPort', url: 'http://127.0.0.1:4176', reuseExistingServer: false },
});
