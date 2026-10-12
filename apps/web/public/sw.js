// Retirement worker for installations of the previous PWA.
self.addEventListener('install', () => { self.skipWaiting(); });
self.addEventListener('activate', event => {
  event.waitUntil(self.clients.claim().then(() => self.registration.unregister()));
});
