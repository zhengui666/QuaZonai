import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

const proxy = { '/api/': { target: process.env.QUAZONAI_DEV_API_ORIGIN ?? 'http://127.0.0.1:8080', changeOrigin: false } };
export default defineConfig({
  envDir: false,
  plugins: [react()],
  cacheDir: process.env.QUAZONAI_WEB_CACHE_DIR ?? resolve(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'quazonai-web-vite'),
  optimizeDeps: { entries: ['index.html'] },
  server: { host: '127.0.0.1', port: 5173, strictPort: true, proxy },
  preview: { host: '127.0.0.1', strictPort: true, proxy },
  build: {
    target: ['es2022', 'safari16'], sourcemap: false,
    rollupOptions: { output: { manualChunks(id) {
      if (/\/node_modules\/(echarts|zrender)\//.test(id)) return 'charts';
      if (id.includes('/node_modules/')) return 'vendor';
    } } },
  },
  test: { include: ['src/*.test.ts'], environment: 'node' },
});
