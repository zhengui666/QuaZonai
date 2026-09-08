import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';

// Tooling-only origin: never injected into the browser bundle. Native browser
// tests allocate their own backend port instead of stopping an existing server.
const apiOrigin = new URL(process.env.QUAZONAI_DEV_API_ORIGIN ?? 'http://127.0.0.1:8080');
if (apiOrigin.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(apiOrigin.hostname)
  || apiOrigin.username || apiOrigin.password || apiOrigin.pathname !== '/' || apiOrigin.search || apiOrigin.hash) {
  throw new Error('QUAZONAI_DEV_API_ORIGIN must be a credential-free loopback HTTP origin');
}
const proxy = {
  // PUBLIC_URL must match the browser origin. Keep Host and Origin intact so
  // Rust, not this development proxy, enforces authentication and CSRF policy.
  '/api/': { target: apiOrigin.origin, changeOrigin: false },
  '/health/': { target: apiOrigin.origin, changeOrigin: false },
};

export default defineConfig({
  // This same-origin UI has no browser secrets or .env-based backend settings.
  envDir: false,
  plugins: [
    react(),
    VitePWA({
      registerType: 'prompt',
      injectRegister: false,
      includeAssets: ['icon.svg'],
      manifest: {
        id: '/', name: 'QuaZonai 研究工作台', short_name: 'QuaZonai',
        description: '有证据、受预算约束的量化研究工作台。不是券商订单执行器。',
        lang: 'zh-CN', start_url: '/', scope: '/', display: 'standalone',
        theme_color: '#141b2d', background_color: '#f6f7fa',
        icons: [{ src: '/icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' }],
      },
      workbox: {
        // Only build-owned static files enter the precache. No authenticated URL
        // is ever a runtime cache key, nor is an API error converted into HTML.
        globPatterns: ['**/*.{js,css,html,svg,webmanifest}'],
        maximumFileSizeToCacheInBytes: 3 * 1024 * 1024,
        navigateFallback: null,
        cleanupOutdatedCaches: true,
        skipWaiting: false,
        clientsClaim: false,
        runtimeCaching: [{
          urlPattern: ({ url }) => url.pathname.startsWith('/api/') || url.pathname.startsWith('/health/'),
          handler: 'NetworkOnly', method: 'GET',
        }],
      },
      devOptions: { enabled: false },
    }),
  ],
  server: {
    host: '127.0.0.1', port: 5173, strictPort: true,
    proxy,
  },
  preview: { host: '127.0.0.1', strictPort: true, proxy },
  build: { target: ['es2022', 'safari16'], sourcemap: false,
    commonjsOptions: { include: [/node_modules/, /generated\/responses\.cjs$/] },
  },
  test: { include: ['src/**/*.test.ts'], environment: 'node', restoreMocks: true },
});
