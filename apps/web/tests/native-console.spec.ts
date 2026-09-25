import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
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
      // The real Rust transaction commits before we discard its acknowledgement.
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
        // Capture only the project surface, never browser session material.
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


// Both cases run before the controlled restart. The restart phase reuses only
// the original persistent checkpoint; it does not recreate any project.
if (config.phase === 'before-restart') {
  test('ChatGPT login uses the real account receipt across a lost acknowledgement and reload', async ({ page, context }) => {
    await page.goto('/');
    await page.getByRole('menuitem', { name: '设置', exact: true }).click();
    const login = page.getByRole('button', { name: '登录 ChatGPT', exact: true });
    await expect(login).toBeEnabled();
    for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
    const profiles: Schema['Page_CodexProfileViewV1'] = await (await page.request.get('/api/v2/settings/codex?limit=100')).json();
    expect(profiles.items).toHaveLength(2);
    const profile = profiles.items[0]!;
    const latestPath = `/api/v2/codex/login?profile_id=${profile.id}`;
    expect(await (await page.request.get(latestPath)).json()).toBeNull();
    await context.setOffline(true);
    await expect(login).toBeDisabled();
    await context.setOffline(false);
    await expect(login).toBeEnabled();
    let requestKey: string | undefined;
    let requestBody: unknown;
    let accepted: Schema['CodexAccountStartV1'] | undefined;
    await page.route('**/api/v2/codex/login/start', async route => {
      requestKey = route.request().headers()['idempotency-key'];
      requestBody = route.request().postDataJSON();
      const response = await route.fetch({ maxRetries: 0 });
      expect(response.status()).toBe(202);
      accepted = await response.json();
      // Real unavailable deployment, never fabricated OAuth/native success.
      expect(accepted?.device_code == null).toBe(true);
      expect(accepted?.current.state).toBe('FAILED');
      expect(accepted?.current.reason).toBe('DEPLOYMENT_UNAVAILABLE');
      await response.dispose();
      await route.abort('failed');
    });
    await login.click();
    await expect(page.getByRole('button', { name: '重试当前操作' })).toBeVisible();
    await expect(login).toBeDisabled();
    await page.unroute('**/api/v2/codex/login/start');
    const replayed = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/codex/login/start');
    await page.getByRole('button', { name: '重试当前操作' }).click();
    const response = await replayed;
    expect(response.status()).toBe(202);
    expect(response.request().headers()['idempotency-key']).toBe(requestKey);
    expect(response.request().postDataJSON()).toEqual(requestBody);
    const replay: Schema['CodexAccountStartV1'] = await response.json();
    expect(replay.acceptance).toEqual({ ...accepted!.acceptance, replayed: true });
    await expect(login).toBeEnabled();
    await expect(page.getByLabel('ChatGPT 授权码')).toHaveCount(0);
    await expect(page.getByText('ChatGPT 登录成功', { exact: true })).toHaveCount(0);
    for (const role of profiles.items) {
      const latest = await (await page.request.get(`/api/v2/codex/login?profile_id=${role.id}`)).json();
      expect(latest).toEqual(replay.current);
    }
    await page.reload();
    await page.getByRole('menuitem', { name: '设置', exact: true }).click();
    await expect(login).toBeEnabled();
    expect(await (await page.request.get(latestPath)).json()).toEqual(replay.current);
    await expect(page.getByText('Codex 运行环境不可用，请检查部署配置').first()).toBeVisible();
  });

  test('all native pages remain accessible in both themes and three viewports', async ({ page, context }) => {
    test.setTimeout(180_000);
    await page.goto('/');
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
    for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
    for (const mode of ['light', 'dark']) {
      if (await page.locator('html').getAttribute('data-theme') !== mode) {
        await page.getByRole('button', { name: mode === 'dark' ? '切换为深色主题' : '切换为浅色主题' }).click();
      }
      await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
      for (const viewport of [{ width: 1440, height: 900 }, { width: 768, height: 1024 }, { width: 390, height: 844 }]) {
        await page.setViewportSize(viewport);
        for (const label of ['研究', 'Alpha', '组合', '交付', '运行', '设置']) {
          if (viewport.width < 992) {
            await page.getByRole('button', { name: '打开主导航' }).click();
          }
          await page.getByRole('menuitem', { name: label, exact: true }).click();
          await expect(page.getByRole('heading', { level: 1, name: label, exact: true })).toBeVisible();
          if (viewport.width < 992) {
            await expect(page.getByRole('dialog', { name: '主导航', exact: true })).toBeHidden();
          }
          // Audit the completed real query, not a button's disabled-to-enabled
          // transition. Wait on browser animation completion, not a fixed delay.
          await expect(page.locator('.ant-select-loading:visible, .ant-skeleton:visible, .ant-spin-spinning:visible, .ant-btn-loading:visible')).toHaveCount(0);
          await page.evaluate(async () => {
            await Promise.all(document.getAnimations()
              .filter(animation => animation.effect?.getTiming().iterations !== Infinity)
              .map(animation => animation.finished.catch(() => {})));
          });
          await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth))
            .toBeLessThanOrEqual(viewport.width + 1);
          const result = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
          expect.soft(result.violations, `${label} / ${mode} at ${viewport.width}px`).toEqual([]);
          if (label === '设置') await page.locator('.console-layout').screenshot({
            path: resolve(dirname(config.redactionsFile), `codex-${mode}-${viewport.width}.png`), animations: 'disabled',
          });
        }
      }
    }
  });

  test('the deployed service worker keeps API data out of caches and protects unsaved edits during updates', async ({ page, context }) => {
    const workerFile = resolve(dirname(config.redactionsFile), 'release/web/sw.js');
    const originalWorker = readFileSync(workerFile, 'utf8');
    const writes: string[] = [];
    page.on('request', request => {
      if (new URL(request.url()).pathname.startsWith('/api/') && !['GET', 'HEAD', 'OPTIONS'].includes(request.method())) {
        writes.push(`${request.method()} ${new URL(request.url()).pathname}`);
      }
    });
    try {
      await page.goto('/');
      await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
      await page.evaluate(async () => { await navigator.serviceWorker.ready; });
      // Workbox deliberately does not claim an already-open document.
      await page.reload();
      await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
      await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      const caches = await page.evaluate(async () => {
        const names = await window.caches.keys();
        const urls: string[] = [];
        for (const name of names) {
          const cache = await window.caches.open(name);
          urls.push(...(await cache.keys()).map(request => new URL(request.url).pathname));
        }
        return { names, urls };
      });
      expect(caches.names.length).toBeGreaterThan(0);
      expect(caches.urls.some(path => path.startsWith('/assets/'))).toBe(true);
      expect(caches.urls.some(path => path.startsWith('/api/') || path.startsWith('/health/'))).toBe(false);

      await page.getByRole('button', { name: '新建研究', exact: true }).click();
      await page.getByLabel('研究名称').fill('Unsaved native PWA edit');
      await context.setOffline(true);
      await expect(page.getByText('离线，无法提交操作', { exact: true })).toBeVisible();
      await expect(page.getByRole('button', { name: '保存项目', exact: true })).toBeDisabled();
      expect(writes).toEqual([]);
      await context.setOffline(false);
      await expect(page.getByText('离线，无法提交操作', { exact: true })).toHaveCount(0);
      await expect(page.getByLabel('研究名称')).toHaveValue('Unsaved native PWA edit');
      expect(writes).toEqual([]);

      // Update the actual test-owned installed script served by Caddy. There is
      // no fabricated Worker, navigator override, route fulfilment or API peer.
      appendFileSync(workerFile, `\n// Native update ${randomUUID()}\n`);
      await page.evaluate(async () => { await (await navigator.serviceWorker.ready).update(); });
      await expect(page.getByRole('dialog', { name: '检测到新的前端版本' })).toBeVisible();
      await expect(page.getByRole('button', { name: '确认更新', exact: true })).toBeDisabled();
      await expect(page.getByText('请先保存或取消当前编辑', { exact: true })).toBeVisible();
      await page.getByRole('button', { name: '稍后', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toHaveValue('Unsaved native PWA edit');
      await page.getByRole('button', { name: '取消', exact: true }).click();
      await page.getByRole('button', { name: '放弃修改', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toHaveCount(0);
      await page.getByRole('button', { name: '有新版本', exact: true }).click();
      await expect(page.getByRole('button', { name: '确认更新', exact: true })).toBeEnabled();
      const reloaded = page.waitForEvent('load');
      await page.getByRole('button', { name: '确认更新', exact: true }).click();
      await reloaded;
      await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      const saved: Checkpoint = JSON.parse(readFileSync(projectFile, 'utf8'));
      const listing = await page.request.get('/api/v2/projects?limit=100');
      expect(listing.status()).toBe(200);
      const body: { items: Schema['ProjectView'][] } = await listing.json();
      expect(body.items).toEqual([saved.receipt.resource]);
      expect(writes).toEqual([]);
    } finally {
      await context.setOffline(false);
      writeFileSync(workerFile, originalWorker);
    }
  });
}
