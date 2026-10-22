import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Synthetic, contract-validated HTTP fixtures exercise the production UI only.
export default defineConfig({
  testDir: './tests', testMatch: 'no-json-interface.spec.ts', workers: 1, retries: 0,
  outputDir: join(tmpdir(), 'quazonai-no-json-ui'), forbidOnly: Boolean(process.env.CI),
  use: {
    baseURL: 'http://127.0.0.1:4175', trace: 'off', screenshot: 'off', video: 'off',
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
      ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {},
  },
  webServer: {
    command: 'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 4175 --strictPort',
    url: 'http://127.0.0.1:4175', reuseExistingServer: false,
  },
});
