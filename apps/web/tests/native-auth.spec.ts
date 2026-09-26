import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { randomBytes } from 'node:crypto';
import { fixture, loginNative, rememberPrivateValue } from './native-auth-support';

const config = fixture();
async function openAuthSettings(page: Page) {
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '鉴权管理', exact: true }).click();
  await expect(page.getByRole('heading', { name: '鉴权管理', exact: true })).toBeVisible();
}
async function changePassword(page: Page, current: string, next: string) {
  await page.getByLabel('当前密码', { exact: true }).fill(current);
  await page.getByLabel('新密码', { exact: true }).fill(next);
  await page.getByLabel('确认新密码', { exact: true }).fill(next);
  await page.getByRole('button', { name: '修改密码并重新登录', exact: true }).click();
  await expect(page.getByRole('heading', { name: '登录 QuaZonai', exact: true })).toBeVisible();
}

if (config.phase === 'before-restart') {
  test('real password setup, session persistence, password change and CLI device revocation', async ({ page, context, browser }) => {
    test.setTimeout(180_000);
    const protectedRequests: string[] = [];
    page.on('request', request => {
      const path = new URL(request.url()).pathname;
      if (path.startsWith('/api/') && !path.startsWith('/api/v2/auth/')) protectedRequests.push(path);
    });
    const fresh = await page.request.get('/api/v2/auth/status');
    expect(await fresh.json()).toEqual({ schema_version: 1, setup_required: true });
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(401);
    expect((await page.request.get('/api/v2/projects')).status()).toBe(401);
    await page.goto('/');
    await expect(page.getByRole('heading', { name: '设置登录密码', exact: true })).toBeVisible();
    await expect(page.getByRole('checkbox', { name: '记住本设备 30 天' })).not.toBeChecked();
    expect(protectedRequests).toEqual([]);
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
      const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
      expect(audit.violations).toEqual([]);
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.getByLabel('设置密码', { exact: true }).fill(config.password);
    await page.getByLabel('确认密码', { exact: true }).fill(config.password);
    await page.getByRole('button', { name: '设置密码并登录', exact: true }).click();
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
    const shortSession = await (await page.request.get('/api/v2/auth/session')).json();
    expect(Date.parse(shortSession.expires_at) - Date.parse(shortSession.authenticated_at)).toBe(12 * 60 * 60 * 1000);
    const shortCookies = await context.cookies();
    expect(shortCookies.length).toBeGreaterThan(0);
    for (const cookie of shortCookies) {
      rememberPrivateValue(config, cookie.value);
      expect(cookie.httpOnly).toBe(true); expect(cookie.sameSite).toBe('Strict'); expect(cookie.expires).toBe(-1);
    }
    await page.getByRole('button', { name: '退出登录', exact: true }).click();
    await expect(page.getByLabel('登录密码', { exact: true })).toBeVisible();
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(401);
    await page.reload();
    await expect(page.getByLabel('登录密码', { exact: true })).toBeVisible();
    const wrong = randomBytes(24).toString('hex'); rememberPrivateValue(config, wrong);
    await page.getByLabel('登录密码', { exact: true }).fill(wrong);
    const denied = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/auth/login');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    expect((await denied).status()).toBe(401);
    await expect(page.getByLabel('登录密码', { exact: true })).toHaveValue('');
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toHaveCount(0);
    await loginNative(page, config);
    const remembered = await (await page.request.get('/api/v2/auth/session')).json();
    expect(Date.parse(remembered.expires_at) - Date.parse(remembered.authenticated_at)).toBe(30 * 24 * 60 * 60 * 1000);
    for (const cookie of await context.cookies()) expect(cookie.expires).toBeGreaterThan(Date.now() / 1000 + 29 * 86400);
    expect(await page.evaluate(() => document.cookie)).toBe('');
    expect(await page.evaluate(() => JSON.stringify([localStorage, sessionStorage]))).not.toContain(config.password);

    const secondBrowser = await browser.newContext({ baseURL: config.baseUrl, storageState: await context.storageState(), viewport: { width: 1440, height: 1000 } });
    const cli = await browser.newContext({ baseURL: config.baseUrl });
    try {
      const secondPage = await secondBrowser.newPage();
      await secondPage.goto('/');
      await expect(secondPage.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      const connected = await cli.request.post('/api/v2/auth/cli/login', {
        headers: { Origin: config.baseUrl },
        data: { schema_version: 1, password: config.password, name: 'Native acceptance machine' },
      });
      expect(connected.status()).toBe(201);
      const device = await connected.json(); rememberPrivateValue(config, device.token);
      const headers = { Authorization: `Bearer ${device.token}` };
      expect((await cli.request.get('/api/v2/auth/cli/session', { headers })).status()).toBe(200);
      await openAuthSettings(page);
      await expect(page.getByRole('row').filter({ hasText: 'Native acceptance machine' })).toHaveCount(1);
      const newPassword = randomBytes(24).toString('hex'); rememberPrivateValue(config, newPassword);
      await page.getByLabel('当前密码', { exact: true }).fill(wrong);
      await page.getByLabel('新密码', { exact: true }).fill(newPassword);
      await page.getByLabel('确认新密码', { exact: true }).fill(newPassword);
      const wrongCurrent = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/auth/password');
      await page.getByRole('button', { name: '修改密码并重新登录', exact: true }).click();
      expect((await wrongCurrent).status()).toBe(401);
      await expect(page.getByLabel('当前密码', { exact: true })).toHaveValue('');
      expect((await page.request.get('/api/v2/auth/session')).status()).toBe(200);
      await changePassword(page, config.password, newPassword);
      await secondPage.getByRole('button', { name: '刷新', exact: true }).click();
      await expect(secondPage.getByRole('heading', { name: '登录 QuaZonai', exact: true })).toBeVisible();
      await expect(secondPage.getByRole('button', { name: '新建研究', exact: true })).toHaveCount(0);
      expect((await cli.request.get('/api/v2/auth/cli/session', { headers })).status()).toBe(200);
      await loginNative(page, config, true, newPassword);
      await openAuthSettings(page);
      await page.getByRole('row').filter({ hasText: 'Native acceptance machine' }).getByRole('button', { name: '删除机器', exact: true }).click();
      await page.getByRole('dialog').getByRole('button', { name: '删除机器', exact: true }).click();
      await expect(page.getByRole('row').filter({ hasText: 'Native acceptance machine' })).toHaveCount(0);
      expect((await cli.request.get('/api/v2/auth/cli/session', { headers })).status()).toBe(401);
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: 1000 });
        await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
        const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
        expect(audit.violations).toEqual([]);
      }
      // Leave the same password for the console/restart checks; each test logs in normally.
      await changePassword(page, newPassword, config.password);
    } finally { await secondBrowser.close(); await cli.close(); }
  });
}
