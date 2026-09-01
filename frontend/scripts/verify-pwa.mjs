import { readFile } from 'node:fs/promises';
import { access } from 'node:fs/promises';
import { resolve } from 'node:path';

const dist = resolve('dist');
const manifest = JSON.parse(await readFile(resolve(dist, 'manifest.webmanifest'), 'utf8'));
const serviceWorker = await readFile(resolve(dist, 'sw.js'), 'utf8');

const requiredManifest = {
  name: 'QuaZonai Research Workbench',
  short_name: 'QuaZonai',
  display: 'standalone',
  orientation: 'any',
  start_url: '/',
  scope: '/',
};
for (const [key, expected] of Object.entries(requiredManifest)) {
  if (manifest[key] !== expected) throw new Error(`manifest.${key} must be ${JSON.stringify(expected)}`);
}

const icons = manifest.icons ?? [];
for (const [size, purpose] of [['192x192', undefined], ['512x512', undefined], ['512x512', 'maskable']]) {
  const icon = purpose === undefined
    ? icons.find((entry) => entry.sizes === size && entry.purpose === undefined)
    : icons.find((entry) => entry.sizes === size && entry.purpose === purpose);
  if (!icon) throw new Error(`manifest is missing ${size}${purpose ? ` ${purpose}` : ''} icon`);
  await access(resolve(dist, String(icon.src).replace(/^\//, '')));
}
await access(resolve(dist, 'icons/apple-touch-icon.png'));

if (!serviceWorker.includes('cleanupOutdatedCaches')) throw new Error('service worker must clean old caches');
const apiPathPolicy = serviceWorker.includes('/api/') || serviceWorker.includes('api(?:');
if (!serviceWorker.includes('NetworkOnly') || !apiPathPolicy) {
  throw new Error('service worker must keep API requests NetworkOnly');
}
const encodedTraversalPolicy = serviceWorker.toLowerCase().includes('%2e') && serviceWorker.toLowerCase().includes('%2f');
if (!encodedTraversalPolicy) throw new Error('service worker must deny encoded traversal navigation');
const precache = serviceWorker.match(/precacheAndRoute\(\[(.*?)\]\)/s)?.[1] ?? '';
if (precache.includes('.map')) throw new Error('source maps must not enter the static precache');

console.log('PWA manifest, icons, service-worker API policy, and static precache verified.');
