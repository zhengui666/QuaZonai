import { test, expect } from '@playwright/test';

test.use({ baseURL: 'http://127.0.0.1:4180', serviceWorkers: 'allow' });

test('native service-worker activation preserves another tab draft and never caches private APIs', async ({ page, context }) => {
  test.setTimeout(60_000);
  await page.goto('/');
  await page.evaluate(async () => { await navigator.serviceWorker.ready; });
  await page.reload();
  await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await page.evaluate(async () => { await fetch('/api/fixture-private', { cache: 'no-store' }); });
  const cached = await page.evaluate(async () => {
    const result: string[] = [];
    for (const name of await caches.keys()) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) result.push(new URL(request.url).pathname);
    }
    return result;
  });
  expect(cached.length).toBeGreaterThan(0);
  expect(cached.some(path => path.startsWith('/api/') || path.startsWith('/health/'))).toBe(false);
  const initial = await page.evaluate(() => performance.timeOrigin);
  const dialogs: string[] = [];
  page.on('dialog', dialog => { dialogs.push(dialog.type()); void dialog.dismiss(); });
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill('另一个标签页不能清除的草稿');
  await page.request.post('/__fixture__/release');
  await page.evaluate(async () => { await (await navigator.serviceWorker.getRegistration())?.update(); });
  const prompt = page.getByRole('dialog', { name: '检测到新的前端版本' });
  await expect(prompt).toBeVisible();
  await expect(prompt.getByRole('button', { name: '确认更新', exact: true })).toBeDisabled();
  await prompt.getByRole('button', { name: '稍后', exact: true }).click();
  const other = await context.newPage();
  await other.goto('/');
  const otherInitial = await other.evaluate(() => performance.timeOrigin);
  await expect(other.getByRole('dialog', { name: '检测到新的前端版本' })).toBeVisible();
  await other.getByRole('button', { name: '确认更新', exact: true }).click();
  await other.waitForFunction(before => performance.timeOrigin !== before, otherInitial);
  await page.bringToFront();
  await expect(page.getByLabel('研究名称')).toHaveValue('另一个标签页不能清除的草稿');
  expect(await page.evaluate(() => performance.timeOrigin)).toBe(initial);
  expect(dialogs).toEqual([]);
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('button', { name: '放弃修改', exact: true }).click();
  await expect(page.getByLabel('研究名称')).not.toBeVisible();
  await page.getByRole('button', { name: '有新版本', exact: true }).click();
  await expect(prompt.getByRole('button', { name: '确认更新', exact: true })).toBeEnabled();
  await prompt.getByRole('button', { name: '确认更新', exact: true }).click();
  await page.waitForFunction(before => performance.timeOrigin !== before, initial);
  await context.setOffline(true);
  const offline = await page.evaluate(async () => {
    try { return (await fetch('/api/fixture-private')).status; } catch { return 'NETWORK_FAILURE'; }
  });
  expect(offline).toBe('NETWORK_FAILURE');
  await context.setOffline(false);
  await other.close();
});
