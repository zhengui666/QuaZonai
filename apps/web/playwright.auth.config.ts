import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Synthetic HTTP contracts test browser interactions only. The separate native
// harness remains responsible for real Rust/PostgreSQL acceptance.
export default defineConfig({
  testDir: './tests', testMatch: 'chatgpt-auth.spec.ts', workers: 1, retries: 0,
  outputDir: join(tmpdir(), 'quazonai-chatgpt-auth-ui'), forbidOnly: Boolean(process.env.CI),
  use: { baseURL: 'http://127.0.0.1:4174', trace: 'off', screenshot: 'off', video: 'off' },
  webServer: { command: 'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 4174 --strictPort',
    url: 'http://127.0.0.1:4174', reuseExistingServer: false },
});
