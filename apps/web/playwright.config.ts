import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

const scratch = process.env.PI_SCRATCH_DIR ?? tmpdir();
const port = Number(process.env.QUAZONAI_WEB_E2E_PORT ?? 5181);
export default defineConfig({
  testDir: './tests', testMatch: ['workspace.spec.ts', 'real-service.spec.ts'], workers: 1, retries: 0, timeout: 45_000,
  outputDir: resolve(scratch, 'quazonai-web-playwright'), reporter: 'list',
  use: { actionTimeout: 10_000, baseURL: `http://127.0.0.1:${port}`, viewport: { width: 1440, height: 1000 }, screenshot: 'only-on-failure', trace: 'off' },
  webServer: {
    command: `npm run dev -- --port ${port}`,
    url: `http://127.0.0.1:${port}`, reuseExistingServer: !process.env.CI,
    env: { QUAZONAI_WEB_CACHE_DIR: resolve(scratch, 'quazonai-web-vite') },
  },
});
