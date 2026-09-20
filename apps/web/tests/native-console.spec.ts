import { test, expect } from '@playwright/test';
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import type { Schema } from '../src/api';

type Fixture = {
  baseUrl: string; redactionsFile: string;
  phase: 'before-restart' | 'after-restart';
};
type Checkpoint = {
  key: string; request: Schema['ProjectCreate']; status: number;
  receipt: { schema_version: number; resource: Schema['ProjectView']; replayed: boolean };
};

function fixture(): Fixture {
  const path = process.env.QUAZONAI_WEB_E2E_FIXTURE;
  if (!path) throw new Error('Missing private native fixture; use the native-browser harness');
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (typeof value !== 'object' || value === null) throw new Error('Invalid private fixture');
  const fields = value as Record<string, unknown>;
  if (typeof fields.baseUrl !== 'string' || fields.baseUrl !== process.env.QUAZONAI_WEB_E2E_ORIGIN
    || !['before-restart', 'after-restart'].includes(String(fields.phase))
    || fields.redactionsFile !== resolve(dirname(path), 'redactions.jsonl')) {
    throw new Error('Private fixture fields do not match the test-owned runtime');
  }
  return fields as Fixture;
}

function rememberPrivateValue(config: Fixture, value: string) {
  // Only the harness reads this private redaction manifest; it is not an artifact.
  appendFileSync(config.redactionsFile, `${JSON.stringify(value)}\n`, { mode: 0o600 });
}

const config = fixture();
const sessionFile = resolve(dirname(config.redactionsFile), 'restart-browser.json');
const projectFile = resolve(dirname(config.redactionsFile), 'restart-project.json');
// This is Playwright's own original browser state, never a fabricated session.
// Each phase is executed once by the same private harness; no skipped cases.
test.use({ storageState: config.phase === 'after-restart' ? sessionFile : undefined });

test(config.phase === 'before-restart'
  ? 'packaged local entry, lost-ACK project retry, CSRF and both themes in three viewports'
  : 'new Rust process retains the original local session, project, receipt and theme',
async ({ page, context }) => {
  // Only assert presence, never print any credential on assertion failure.
  expect(['QUAZONAI_WEB_TEST_ADMIN_URL', 'DATABASE_URL', 'PGPASSWORD', 'GH_TOKEN', 'GITHUB_TOKEN']
    .some((key) => process.env[key] !== undefined)).toBe(false);
  if (config.phase === 'after-restart') {
    const saved: Checkpoint = JSON.parse(readFileSync(projectFile, 'utf8'));
    expect(typeof saved.key).toBe('string');
    expect(saved.receipt.replayed).toBe(false);
    expect(saved.receipt.resource.id).toMatch(/^[0-9a-f-]{36}$/);
    await page.goto('/');
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: '绑定你的验证器' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '登录', exact: true })).toHaveCount(0);
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(200);
    for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);

    await test.step('read the original project and replay its exact pre-restart command', async () => {
      const project = await page.request.get(`/api/v2/projects/${saved.receipt.resource.id}`);
      expect(project.status()).toBe(200);
      expect(await project.json()).toEqual(saved.receipt.resource);
      const replay = await page.request.post('/api/v2/projects', {
        headers: { Origin: config.baseUrl, 'Idempotency-Key': saved.key }, data: saved.request,
      });
      expect(replay.status()).toBe(saved.status);
      expect(await replay.json()).toEqual({ ...saved.receipt, replayed: true });
      const listing = await page.request.get('/api/v2/projects?limit=100');
      expect(listing.status()).toBe(200);
      const items: { items: Schema['ProjectView'][] } = await listing.json();
      expect(items.items).toHaveLength(1);
      expect(items.items[0]).toEqual(saved.receipt.resource);
      await expect(page.getByRole('row').filter({ hasText: saved.receipt.resource.name })).toHaveCount(1);
    });

    await test.step('retain the theme and expose no legacy login operations', async () => {
      await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
      await expect(page.getByRole('button', { name: '退出登录', exact: true })).toHaveCount(0);
      for (const path of ['/api/v2/bootstrap/start', '/api/v2/auth/login', '/api/v2/auth/verify']) {
        const response = await page.request.post(path, { headers: { Origin: config.baseUrl }, data: {} });
        expect([404, 405]).toContain(response.status());
      }
    });
    return;
  }

  const name = `Native browser ${randomUUID()}`;
  let projectId: string;
  let initialKey: string | undefined;
  let initialRequest: Schema['ProjectCreate'] | undefined;
  let checkpoint: Checkpoint | undefined;

  await test.step('enter the local workbench through the actual Rust API', async () => {
    const document = await page.goto('/');
    expect(document?.status()).toBe(200);
    expect(document?.headers()['content-security-policy']).toContain("default-src 'self'");
    expect(document?.headers()['x-content-type-options']).toBe('nosniff');
    expect(document?.headers()['cache-control']).toBe('no-cache');
    const missingAsset = await page.request.get('/assets/does-not-exist.js', { headers: { Accept: 'text/html' } });
    expect(missingAsset.status()).toBe(404);
    expect(await missingAsset.text()).not.toContain('<html');
    await expect(page.getByRole('heading', { name: '绑定你的验证器' })).toHaveCount(0);
    await expect(page.getByLabel('动态验证码')).toHaveCount(0);
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(200);
    const cookies = await context.cookies();
    expect(cookies.length).toBeGreaterThan(0);
    for (const cookie of cookies) expect(cookie.httpOnly).toBe(true);
  });

  await test.step('lose a real committed response and retry the same idempotency key', async () => {
    let committedStatus: number | undefined;
    let originalReceipt: Checkpoint['receipt'] | undefined;
    let intercepted = false;
    await page.route('**/api/v2/projects', async (route) => {
      if (route.request().method() !== 'POST' || intercepted) return route.continue();
      intercepted = true;
      initialKey = route.request().headers()['idempotency-key'];
      initialRequest = route.request().postDataJSON();
      // The real Rust transaction commits before we discard its ACK. No mock body.
      const upstream = await route.fetch({ maxRetries: 0, timeout: 20_000 });
      committedStatus = upstream.status();
      originalReceipt = await upstream.json();
      await upstream.dispose();
      await route.abort('failed');
    });
    await page.getByRole('button', { name: '新建研究', exact: true }).click();
    await page.getByLabel('研究名称').fill(name);
    await page.getByLabel('研究说明', { exact: true }).fill('Native browser acceptance; research only, no qualification claims.');
    await page.getByRole('button', { name: '保存项目', exact: true }).click();
    await expect(page.getByText(/连接中断，提交结果未知/)).toBeVisible();
    expect(committedStatus).toBeGreaterThanOrEqual(200);
    expect(committedStatus).toBeLessThan(300);
    expect(typeof initialKey).toBe('string');
    expect(originalReceipt).toMatchObject({
      schema_version: 1, replayed: false, resource: { name, revision: '1', state: 'DRAFT' },
    });
    if (!originalReceipt || !initialRequest || !initialKey || !committedStatus) {
      throw new Error('The first real commit must yield its original request and receipt');
    }
    checkpoint = { key: initialKey, request: initialRequest, status: committedStatus, receipt: originalReceipt };
    await page.unroute('**/api/v2/projects');
    const retryPromise = page.waitForResponse((response) =>
      new URL(response.url()).pathname === '/api/v2/projects' && response.request().method() === 'POST');
    await page.getByRole('button', { name: '保存项目', exact: true }).click();
    const retried = await retryPromise;
    expect(retried.ok()).toBe(true);
    expect(retried.request().headers()['idempotency-key']).toBe(initialKey);
    const receipt: Checkpoint['receipt'] = await retried.json();
    expect(receipt.schema_version).toBe(1);
    expect(receipt).toEqual({ ...originalReceipt, replayed: true });
    expect(retried.request().postDataJSON()).toEqual(initialRequest);
    await expect(page.getByRole('row').filter({ hasText: name })).toHaveCount(1);
    const response = await page.request.get('/api/v2/projects?limit=100');
    expect(response.status()).toBe(200);
    const listing: { items: { id: string; name: string; revision: unknown; state: string }[] } = await response.json();
    const matches = listing.items.filter((project) => project.name === name);
    expect(matches).toHaveLength(1);
    const created = matches[0];
    if (!created) throw new Error('Committed project missing from the real API listing');
    expect(created.revision).toBe('1');
    expect(created.state).toBe('DRAFT');
    expect(created.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    projectId = created.id;
    expect(projectId).toBe(receipt.resource.id);
  });

  await test.step('reject an authenticated cross-origin write without changing the database', async () => {
    const deniedName = `Rejected cross-origin ${randomUUID()}`;
    const denied = await page.request.post('/api/v2/projects', {
      headers: { Origin: 'https://attacker.invalid', 'Idempotency-Key': randomUUID() },
      data: { schema_version: 1, name: deniedName, description: '', fork_from_project_id: null },
    });
    expect(denied.status()).toBe(403);
    const listing: { items: { id: string; name: string }[] } = await (await page.request.get('/api/v2/projects?limit=100')).json();
    expect(listing.items.some((project) => project.name === deniedName)).toBe(false);
    expect(listing.items.some((project) => project.id === projectId)).toBe(true);
  });

  await test.step('keep both themes and the local editor inside three viewports', async () => {
    for (const mode of ['light', 'dark']) {
      if (await page.locator('html').getAttribute('data-theme') !== mode) {
        await page.getByRole('button', { name: mode === 'dark' ? '切换为深色主题' : '切换为浅色主题' }).click();
      }
      for (const viewport of [{ width: 1440, height: 900 }, { width: 768, height: 1024 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
      await page.getByRole('button', { name: '新建研究', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toBeVisible();
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
      await page.getByRole('button', { name: '取消', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toHaveCount(0);
      // Capture only the authenticated project surface, never enrollment/OTP UI.
      await expect(page.getByLabel('动态验证码')).toHaveCount(0);
      await expect(page.getByLabel('一次性初始化凭据')).toHaveCount(0);
      await page.locator('.console-layout').screenshot({
        path: resolve(dirname(config.redactionsFile), `projects-${mode}-${viewport.width}.png`),
        animations: 'disabled',
      });
      }
    }
  });

  await test.step('retain the original browser state and command only in private test storage', async () => {
    if (!checkpoint) throw new Error('The real committed project receipt is required for restart verification');
    const storage = await context.storageState();
    expect(storage.cookies.length).toBeGreaterThan(0);
    for (const cookie of storage.cookies) rememberPrivateValue(config, cookie.value);
    writeFileSync(sessionFile, JSON.stringify(storage), { mode: 0o600, flag: 'wx' });
    writeFileSync(projectFile, JSON.stringify(checkpoint), { mode: 0o600, flag: 'wx' });
  });
});
