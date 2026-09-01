/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';

const apiPath = /^\/api(?:\/|$)/;
const traversalPath = /(?:^|\/)(?:\.|%2e){2}(?=\/|%2f|$)/i;

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'prompt',
      injectRegister: false,
      includeAssets: ['icons/apple-touch-icon.png'],
      manifest: {
        name: 'QuaZonai Research Workbench',
        short_name: 'QuaZonai',
        id: '/',
        start_url: '/',
        scope: '/',
        display: 'standalone',
        background_color: '#0a0f0e',
        theme_color: '#0a0f0e',
        orientation: 'any',
        icons: [
          { src: '/icons/pwa-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icons/pwa-512.png', sizes: '512x512', type: 'image/png' },
          { src: '/icons/pwa-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        ],
      },
      workbox: {
        cleanupOutdatedCaches: true,
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [apiPath, traversalPath],
        globPatterns: ['**/*.{js,css,html,png,svg,ico,woff,woff2}'],
        globIgnores: ['**/*.map'],
        runtimeCaching: [
          {
            urlPattern: ({ url }) => apiPath.test(url.pathname),
            handler: 'NetworkOnly',
          },
        ],
      },
    }),
  ],
  server: {
    port: 5173,
    proxy: { '/api': 'http://127.0.0.1:8000' },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2022',
  },
  test: {
    environment: 'jsdom',
    environmentOptions: { jsdom: { url: 'http://localhost/' } },
    setupFiles: ['./src/tests/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    exclude: ['e2e/**', 'node_modules/**', 'dist/**'],
    css: true,
  },
});
