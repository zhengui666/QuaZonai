import { expect, test, type Page } from '@playwright/test';

async function loadControlledShell(page: Page) {
  await page.goto('/');
  await page.waitForFunction(async () => (
    await navigator.serviceWorker.getRegistrations()
  ).some((registration) => registration.active?.state === 'activated'), undefined, { timeout: 15_000 });
  for (let attempt = 0; attempt < 2; attempt += 1) {
    await page.reload({ waitUntil: 'domcontentloaded' });
    try {
      await page.waitForFunction(() => navigator.serviceWorker.controller !== null, undefined, { timeout: 5_000 });
      return;
    } catch (error) {
      if (attempt === 1) throw error;
    }
  }
}

test.describe('installable web client', () => {
  test('publishes a complete manifest and service worker policy', async ({ page }) => {
    const manifestResponse = await page.request.get('/manifest.webmanifest');
    expect(manifestResponse.ok()).toBe(true);
    const manifest = await manifestResponse.json() as {
      name: string;
      short_name: string;
      display: string;
      orientation: string;
      start_url: string;
      scope: string;
      icons: Array<{ src: string; sizes: string; purpose?: string }>;
    };
    expect(manifest).toMatchObject({
      name: 'QuaZonai Research Workbench',
      short_name: 'QuaZonai',
      display: 'standalone',
      orientation: 'any',
      start_url: '/',
      scope: '/',
    });
    expect(manifest.icons).toEqual(expect.arrayContaining([
      expect.objectContaining({ sizes: '192x192' }),
      expect.objectContaining({ sizes: '512x512' }),
      expect.objectContaining({ sizes: '512x512', purpose: 'maskable' }),
    ]));

    await loadControlledShell(page);
    const workerState = await page.evaluate(async () => {
      const registration = (await navigator.serviceWorker.getRegistrations())[0];
      if (!registration) throw new Error('Service Worker registration is missing');
      const cacheNames = await caches.keys();
      const cachedUrls: string[] = [];
      for (const cacheName of cacheNames) {
        const cache = await caches.open(cacheName);
        for (const request of await cache.keys()) cachedUrls.push(request.url);
      }
      return { scope: registration.scope, cachedUrls };
    });
    expect(workerState.scope).toBe('http://127.0.0.1:4173/');
    expect(workerState.cachedUrls.some((url) => new URL(url).pathname.startsWith('/api/'))).toBe(false);
  });

  test('offline mode keeps the shell and explains that server data is unavailable', async ({ page, context }) => {
    await loadControlledShell(page);
    await context.setOffline(true);
    await page.evaluate(() => window.dispatchEvent(new Event('offline')));
    await expect(page.locator('.qz-pwa-banner')).toContainText('Connect to the QuaZonai server');
    await expect(page.locator('.qz-offline-shell')).toBeVisible();
    await expect(page.locator('.qz-app')).toHaveCount(0);
    await expect(page.locator('body')).toContainText('QuaZonai');
  });
});
