import { test, expect, type Request, type Response, type Page, type Route } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import type { Schema } from '../src/api';
import { fixture, loginNative, rememberPrivateValue } from './native-auth-support';

type Checkpoint = {
  key: string; request: Schema['ProjectCreate']; status: number;
  receipt: { schema_version: number; resource: Schema['ProjectView']; replayed: boolean };
};

const config = fixture();
const sessionFile = resolve(dirname(config.redactionsFile), 'restart-browser.json');
const projectFile = resolve(dirname(config.redactionsFile), 'restart-project.json');
// This is Playwright's own original browser state, never a fabricated session.
// Each phase is executed once by the same private harness; no skipped cases.
test.use({ storageState: config.phase === 'after-restart' ? sessionFile : undefined });
test.beforeEach(async ({ page }) => {
  if (config.phase === 'before-restart') await loginNative(page, config);
});

// Test-only command preparation. The real Rust service commits before this
// fixture drops the ACK; observation components never issue these writes.
async function nativeCommandWithLostAck<T>(page: Page, path: string, body: unknown, key: string) {
  const committed: { status: number; receipt: T }[] = [];
  const match = (url: URL) => url.pathname === path;
  const dropAcknowledgement = async (route: Route) => {
    if (route.request().method() !== 'POST') return route.continue();
    expect(route.request().headers()['idempotency-key']).toBe(key);
    expect(route.request().postDataJSON()).toEqual(body);
    const upstream = await route.fetch({ maxRetries: 0, timeout: 20_000 });
    committed.push({ status: upstream.status(), receipt: await upstream.json() });
    await upstream.dispose();
    await route.abort('failed');
  };
  await page.context().route(match, dropAcknowledgement);
  try {
    const lost = await page.evaluate(async ({ path, body, key }) => {
      try {
        await fetch(path, { method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'Idempotency-Key': key }, body: JSON.stringify(body) });
        return false;
      } catch { return true; }
    }, { path, body, key });
    expect(lost).toBe(true);
    expect(committed).toHaveLength(1);
    const original = committed[0];
    if (!original) throw new Error('The real native command did not commit before ACK loss');
    expect(original.status).toBeGreaterThanOrEqual(200);
    expect(original.status).toBeLessThan(300);
    return original;
  } finally { await page.context().unroute(match, dropAcknowledgement); }
}

test(config.phase === 'before-restart'
  ? 'packaged read-only entry, native API lost-ACK retry, CSRF and both themes in three viewports'
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
    await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
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
      await expect(page.getByRole('article').filter({ hasText: saved.receipt.resource.name })).toHaveCount(1);
    });

    await test.step('retain the theme and expose no legacy login operations', async () => {
      await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
      await expect(page.getByRole('button', { name: '退出登录', exact: true })).toBeVisible();
      for (const path of ['/api/v2/bootstrap/start', '/api/v2/auth/verify']) {
        const response = await page.request.post(path, { headers: { Origin: config.baseUrl }, data: {} });
        expect([404, 405]).toContain(response.status());
      }
    });
    return;
  }

  // Exercise the 120-character limit, including an unbroken segment on mobile.
  const name = `Native browser ${randomUUID()} ${'x'.repeat(68)}`;
  let projectId: string;
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
    await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(200);
    const cookies = await context.cookies();
    expect(cookies.length).toBeGreaterThan(0);
    for (const cookie of cookies) expect(cookie.httpOnly).toBe(true);
  });

  await test.step('prepare a real project through the native API and recover its lost ACK', async () => {
    const key = randomUUID();
    const request: Schema['ProjectCreate'] = { schema_version: 1, name,
      description: 'Native browser acceptance; research only, no qualification claims.', fork_from_project_id: null };
    const committed = await nativeCommandWithLostAck<Checkpoint['receipt']>(page, '/api/v2/projects', request, key);
    expect(committed.receipt).toMatchObject({ schema_version: 1, replayed: false, resource: { name, revision: '1', state: 'DRAFT' } });
    checkpoint = { key, request, status: committed.status, receipt: committed.receipt };
    const retried = await page.request.post('/api/v2/projects', { headers: { Origin: config.baseUrl, 'Idempotency-Key': key }, data: request });
    expect(retried.status()).toBe(committed.status);
    const receipt: Checkpoint['receipt'] = await retried.json();
    expect(receipt).toEqual({ ...committed.receipt, replayed: true });
    const listing = await page.request.get('/api/v2/projects?limit=100');
    expect(listing.status()).toBe(200);
    const projects: { items: Schema['ProjectView'][] } = await listing.json();
    const matches = projects.items.filter(project => project.name === name);
    expect(matches).toHaveLength(1);
    expect(matches[0]).toEqual(receipt.resource);
    projectId = receipt.resource.id;
    expect(projectId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    await page.reload();
    await expect(page.getByRole('article').filter({ hasText: name })).toHaveCount(1);
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toHaveCount(0);
  });

  await test.step('read producer sections from the real project without browser business commands', async () => {
    if (!checkpoint) throw new Error('The original committed project is required for observation checks');
    await page.setViewportSize({ width: 1440, height: 1000 });
    const writes: string[] = [];
    const reads: { path: string; status: number }[] = [];
    const observeRequest = (request: Request) => {
      const path = new URL(request.url()).pathname;
      if (path.startsWith('/api/') && !['GET', 'HEAD', 'OPTIONS'].includes(request.method())) writes.push(`${request.method()} ${path}`);
    };
    const observeResponse = (response: Response) => {
      if (response.request().method() === 'GET') reads.push({ path: new URL(response.url()).pathname, status: response.status() });
    };
    page.on('request', observeRequest); page.on('response', observeResponse);
    const projectPath = `/api/v2/projects/${projectId}`;
    const forbidden = /^(请求封存评估|新建组合配置|保存不可变配置|请求组合构建|请求组合 Study|冻结目标包|新建执行假设|新建评估政策|冻结自动化政策|审批此目标包|人工拒绝与重新考虑|登记 Offer|撤销审批|撤销政策)$/;
    async function observe(path: string) {
      await expect.poll(() => reads.some(read => read.path === path && read.status === 200)).toBe(true);
      await expect(page.locator('.ant-skeleton:visible, .ant-spin-spinning:visible, .ant-btn-loading:visible')).toHaveCount(0);
      await expect(page.getByRole('button', { name: forbidden })).toHaveCount(0);
      await expect(page.getByRole('dialog').locator('form, button[type=submit]')).toHaveCount(0);
      expect(writes).toEqual([]);
    }
    try {
      for (const section of [
        { label: 'Alpha', picker: '选择 Alpha 所属项目', firstPath: '/api/v2/alphas', refresh: '刷新 Alpha', tabs: [] },
        { label: '组合', picker: '选择组合所属项目', firstPath: `${projectPath}/portfolio-mandates`, refresh: '刷新配置', tabs: [
          ['执行假设', `${projectPath}/execution-assumptions`], ['候选快照', `${projectPath}/portfolio-candidates`], ['评估政策', '/api/v2/evaluation-policies'],
        ] },
        { label: '交付', picker: '选择交付所属项目', firstPath: `${projectPath}/releases`, refresh: '刷新目标包', tabs: [
          ['交付记录', `${projectPath}/handoffs`], ['Forward 证据', `${projectPath}/forward`], ['观察与唤醒', `${projectPath}/forward-observations`], ['自动化政策', `${projectPath}/automation-policies`],
        ] },
      ]) {
        await page.getByRole('menuitem', { name: section.label, exact: true }).click();
        const selector = page.getByRole('combobox', { name: section.picker, exact: true });
        await expect(selector).toBeEnabled(); await selector.click();
        await page.getByText(`${name} · ${projectId}`, { exact: true }).last().click();
        await observe(section.firstPath);
        const beforeRefresh = reads.filter(read => read.path === section.firstPath).length;
        await page.getByRole('button', { name: section.refresh, exact: true }).click();
        await expect.poll(() => reads.filter(read => read.path === section.firstPath).length).toBeGreaterThan(beforeRefresh);
        await observe(section.firstPath);
        for (const [label, path] of section.tabs) {
          await page.getByRole('tab', { name: label!, exact: true }).click();
          await observe(path!);
        }
      }
      await page.getByRole('tab', { name: '观察与唤醒', exact: true }).click();
      await page.getByRole('tab', { name: 'Wake 记录', exact: true }).click();
      await observe(`${projectPath}/wakes`);
      // Return navigation must not submit a producer command or recreate the project.
      await page.getByRole('menuitem', { name: '研究', exact: true }).click();
      await expect(page.getByRole('article').filter({ hasText: name })).toHaveCount(1);
      expect(writes).toEqual([]);
    } finally {
      page.off('request', observeRequest); page.off('response', observeResponse);
    }
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

  await test.step('keep read-only project navigation and both themes inside three viewports', async () => {
    if (!checkpoint) throw new Error('The original project is required for responsive observation checks');
    const originalProject = checkpoint.receipt.resource;
    for (const mode of ['light', 'dark']) {
      if (await page.locator('html').getAttribute('data-theme') !== mode) {
        await page.getByRole('button', { name: mode === 'dark' ? '切换为深色主题' : '切换为浅色主题' }).click();
      }
      for (const viewport of [{ width: 1440, height: 900 }, { width: 768, height: 1024 }, { width: 390, height: 844 }]) {
        await page.setViewportSize(viewport);
        await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
        await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
        await expect(page.getByRole('button', { name: /^(新建研究|创建第一个研究|编辑)$/ })).toHaveCount(0);
        const search = page.getByRole('textbox', { name: '搜索本页研究项目', exact: true });
        await search.fill(name); await expect(page.getByRole('article')).toHaveCount(1);
        await search.fill('No matching observed project');
        await expect(page.getByRole('heading', { name: '本页没有匹配的项目', exact: true })).toBeVisible();
        await page.getByRole('button', { name: '清除搜索', exact: true }).click();
        await expect(page.getByRole('heading', { name: '研究项目', exact: true })).toBeVisible();
        await expect(page.getByRole('article')).toHaveCount(1);
        if (viewport.width < 768) {
          const row = page.getByRole('article').filter({ has: page.getByRole('button', { name, exact: true }) });
          await expect(row).toHaveCount(1);
          // The workbench permits normal vertical scrolling; the complete card
          // must fit once brought into view, without horizontal clipping.
          await row.scrollIntoViewIfNeeded();
          // Assert every complete value/action before click() can auto-scroll
          // a clipped control. Page overflow alone misses an inner table scroll.
          for (const control of [
            row.getByRole('button', { name, exact: true }),
            row.getByText('草稿', { exact: true }),
            row.getByText('下一步：建立研究 Brief', { exact: true }),
            row.locator('time'),
            row.getByRole('button', { name: '进入研究', exact: true }),
          ]) await expect(control).toBeInViewport({ ratio: 1 });
          await expect(row.locator('time')).toHaveAttribute('datetime', originalProject.updated_at);
          await expect(row.locator('time')).toHaveText(/\d/);
          await expect.poll(() => row.evaluate(element =>
            element.scrollWidth <= element.clientWidth + 1 && element.scrollLeft === 0)).toBe(true);

          await row.getByRole('button', { name, exact: true }).click();
          await expect(page.getByRole('heading', { name, exact: true })).toBeVisible();
          for (const width of [768, 1440, 390]) {
            await page.setViewportSize({ width, height: viewport.height });
            await expect(page.getByRole('heading', { name, exact: true })).toBeVisible();
            await expect(page.getByRole('button', { name: '修改项目状态', exact: true })).toHaveCount(0);
          }
          // A second real tab changes the theme while the original read view
          // stays selected; resizing or theme propagation cannot edit its record.
          const themePage = await context.newPage();
          try {
            await themePage.goto('/');
            await themePage.getByRole('button', { name: mode === 'light' ? '切换为深色主题' : '切换为浅色主题' }).click();
            await expect(page.locator('html')).toHaveAttribute('data-theme', mode === 'light' ? 'dark' : 'light');
            await expect(page.getByRole('heading', { name, exact: true })).toBeVisible();
            await themePage.getByRole('button', { name: mode === 'light' ? '切换为浅色主题' : '切换为深色主题' }).click();
            await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
          } finally { await themePage.close(); }
          const unchanged = await page.request.get(`/api/v2/projects/${projectId}`);
          expect(unchanged.status()).toBe(200);
          expect(await unchanged.json()).toEqual(originalProject);
          await page.getByRole('button', { name: '返回研究列表', exact: true }).click();
          await row.scrollIntoViewIfNeeded();
          await expect(row.getByRole('button', { name: '进入研究', exact: true })).toBeInViewport({ ratio: 1 });
        }
        await page.evaluate(() => window.scrollTo(0, 0));
        // Capture only the project surface, never browser session material.
        await expect(page.getByLabel('动态验证码')).toHaveCount(0);
        await expect(page.getByLabel('一次性初始化凭据')).toHaveCount(0);
        await page.locator('.console-layout').screenshot({
          path: resolve(dirname(config.redactionsFile), `projects-${mode}-${viewport.width}.png`),
          animations: 'disabled',
        });
        if (mode === 'light' && viewport.width === 1440) {
          const cdp = await context.newCDPSession(page);
          try {
            await cdp.send('DOM.enable'); await cdp.send('CSS.enable');
            const { root } = await cdp.send('DOM.getDocument');
            const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: '.research-heading h1' });
            const { fonts } = await cdp.send('CSS.getPlatformFontsForNode', { nodeId });
            const cjkSansFamilies = process.platform === 'linux'
              ? ['WenQuanYi Zen Hei']
              : ['PingFang SC', 'Microsoft YaHei', 'WenQuanYi Zen Hei', 'Noto Sans CJK SC'];
            expect(fonts.some(font => font.glyphCount >= 2 && cjkSansFamilies.includes(font.familyName))).toBe(true);
            writeFileSync(resolve(dirname(config.redactionsFile), 'cjk-fonts.json'), JSON.stringify({ platform: process.platform, fonts }, null, 2));
          } finally { await cdp.detach(); }
        }
        await page.getByRole('button', { name, exact: true }).click();
        await expect(page.getByRole('heading', { name: '当前 Brief', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: '尚无研究周期', exact: true })).toBeVisible();
        for (const [tab, surface] of [['工作概览', 'project-workspace'], ['研究 Brief', 'project-briefs'], ['冻结输入', 'project-inputs'], ['研究周期', 'project-cycles']] as const) {
          if (viewport.width >= 768) await page.getByRole('tab', { name: tab, exact: true }).click();
          else {
            await page.getByLabel('研究项目章节', { exact: true }).click();
            await page.getByTitle(tab, { exact: true }).last().click();
          }
          await expect(page.locator('.ant-skeleton:visible')).toHaveCount(0);
          await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
          if (tab === '冻结输入') {
            await expect(page.getByLabel('冻结输入所属研究项目', { exact: true })).toHaveCount(0);
            await expect(page.getByRole('button', { name: '新建冻结输入', exact: true })).toHaveCount(0);
          }
          await page.evaluate(() => window.scrollTo(0, 0));
          await page.locator('.console-layout').screenshot({ path: resolve(dirname(config.redactionsFile), `${surface}-${mode}-${viewport.width}.png`), animations: 'disabled' });
        }
        await page.getByRole('button', { name: '返回研究列表', exact: true }).click();
      }
    }
  });

  await test.step('prepare a real protocol-only report with a lost ACK and read its exact stored evidence', async () => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    const content = readFileSync(resolve('..', '..', 'tests', 'fixtures', 'agent-evaluation', 'unrun-v1.json'), 'utf8');
    const body: Schema['ArtifactCreate'] = { schema_version: 1, project_id: projectId, kind: 'REPORT', content };
    const key = randomUUID();
    type Receipt = { schema_version: number; replayed: boolean; resource: Schema['ArtifactView'] };
    const committed = await nativeCommandWithLostAck<Receipt>(page, '/api/v2/artifacts', body, key);
    expect(committed.status).toBe(201);
    expect(committed.receipt.replayed).toBe(false);
    expect(committed.receipt.resource.project_id).toBe(projectId);
    expect(committed.receipt.resource.kind).toBe('REPORT');
    const retry = await page.request.post('/api/v2/artifacts', { headers: { Origin: config.baseUrl, 'Idempotency-Key': key }, data: body });
    expect(retry.status()).toBe(201);
    expect(await retry.json()).toEqual({ ...committed.receipt, replayed: true });
    const artifactId = committed.receipt.resource.id;
    const stored = await page.request.get(`/api/v2/artifacts/${artifactId}/agent-evaluation`);
    expect(stored.status()).toBe(200);
    expect(await stored.json()).toEqual(JSON.parse(content));
    const writes: string[] = [];
    const observe = (request: Request) => {
      const path = new URL(request.url()).pathname;
      if (path.startsWith('/api/') && !['GET', 'HEAD', 'OPTIONS'].includes(request.method())) writes.push(`${request.method()} ${path}`);
    };
    page.on('request', observe);
    try {
      await page.getByRole('button', { name, exact: true }).click();
      await page.getByRole('tab', { name: 'Agent 评估', exact: true }).click();
      await expect(page.getByRole('button', { name: '上传报告', exact: true })).toHaveCount(0);
      await expect(page.locator('input[type=file]')).toHaveCount(0);
      await page.getByRole('button', { name: artifactId, exact: true }).click();
      const detail = page.getByRole('dialog', { name: 'Agent 评估详情' });
      await expect(detail.getByText('PROTOCOL_ONLY：协议测试，不是实际模型评估')).toBeVisible();
      await expect(detail.getByText('UNRUN: 2', { exact: true })).toBeVisible();
      await expect(detail.getByText('gpt-6-luna / max', { exact: true })).toBeVisible();
      await expect(detail.getByText('未知（未观察到）', { exact: true })).toHaveCount(2);
      await detail.locator('.ant-drawer-close').click();
      await expect(detail).toHaveCount(0);
      await page.getByRole('button', { name: artifactId, exact: true }).click();
      await expect(detail.getByText('UNRUN: 2', { exact: true })).toBeVisible();
      await detail.locator('.ant-drawer-close').click();
      await page.getByRole('button', { name: '返回研究列表', exact: true }).click();
      expect(writes).toEqual([]);
    } finally { page.off('request', observe); }
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
    await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
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

  test('the deployed service worker excludes API data and protects a pending Settings command during updates', async ({ page, context }) => {
    const workerFile = resolve(dirname(config.redactionsFile), 'release/web/sw.js');
    const originalWorker = readFileSync(workerFile, 'utf8');
    const writes: string[] = [];
    page.on('request', request => {
      if (new URL(request.url()).pathname.startsWith('/api/') && !['GET', 'HEAD', 'OPTIONS'].includes(request.method())) {
        writes.push(`${request.method()} ${new URL(request.url()).pathname}`);
      }
    });
    let releaseAck!: () => void;
    const heldAck = new Promise<void>(resolve => { releaseAck = resolve; });
    const committed: { body: unknown; key: string | undefined; result: Schema['CodexAccountStartV1'] }[] = [];
    try {
      await page.goto('/');
      await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
      for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
      await page.evaluate(async () => { await navigator.serviceWorker.ready; });
      await page.reload();
      await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
      const caches = await page.evaluate(async () => {
        const names = await window.caches.keys(); const urls: string[] = [];
        for (const name of names) {
          const cache = await window.caches.open(name);
          urls.push(...(await cache.keys()).map(request => new URL(request.url()).pathname));
        }
        return { names, urls };
      });
      expect(caches.names.length).toBeGreaterThan(0);
      expect(caches.urls.some(path => path.startsWith('/assets/'))).toBe(true);
      expect(caches.urls.some(path => path.startsWith('/api/') || path.startsWith('/health/'))).toBe(false);
      await page.getByRole('menuitem', { name: '设置', exact: true }).click();
      const login = page.getByRole('button', { name: '登录 ChatGPT', exact: true });
      await expect(login).toBeEnabled();
      await context.setOffline(true);
      await expect(page.getByText('离线，无法提交操作', { exact: true })).toBeVisible();
      await expect(login).toBeDisabled(); expect(writes).toEqual([]);
      await context.setOffline(false);
      await expect(login).toBeEnabled(); expect(writes).toEqual([]);
      await context.route('**/api/v2/codex/login/start', async route => {
        const response = await route.fetch({ maxRetries: 0 });
        expect(response.status()).toBe(202);
        const result: Schema['CodexAccountStartV1'] = await response.json();
        // The fixture's real unavailable deployment must never look like a successful login.
        expect(result.current.state).toBe('FAILED'); expect(result.current.reason).toBe('DEPLOYMENT_UNAVAILABLE');
        expect(result.device_code == null).toBe(true);
        committed.push({ body: route.request().postDataJSON(), key: route.request().headers()['idempotency-key'], result });
        await response.dispose(); await heldAck; await route.abort('failed');
      });
      await login.click();
      await expect.poll(() => committed.length).toBe(1);
      // Hold the real committed Settings acknowledgement while Workbox discovers
      // the actual changed installed worker. There is no fabricated Worker or API success.
      appendFileSync(workerFile, `\n// Native update ${randomUUID()}\n`);
      await page.evaluate(async () => { await (await navigator.serviceWorker.ready).update(); });
      await expect(page.getByRole('dialog', { name: '检测到新的前端版本' })).toBeVisible();
      await expect(page.getByRole('button', { name: '确认更新', exact: true })).toBeDisabled();
      await expect(page.getByText('请先保存或取消当前编辑', { exact: true })).toBeVisible();
      await page.getByRole('button', { name: '稍后', exact: true }).click();
      await expect(login).toBeDisabled();
      releaseAck();
      await expect(page.getByRole('button', { name: '重试当前操作', exact: true })).toBeVisible();
      await page.getByRole('button', { name: '有新版本', exact: true }).click();
      await expect(page.getByRole('button', { name: '确认更新', exact: true })).toBeDisabled();
      await page.getByRole('button', { name: '稍后', exact: true }).click();
      await context.unroute('**/api/v2/codex/login/start');
      const retried = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/codex/login/start');
      await page.getByRole('button', { name: '重试当前操作', exact: true }).click();
      const response = await retried;
      expect(response.status()).toBe(202);
      expect(response.request().headers()['idempotency-key']).toBe(committed[0]!.key);
      expect(response.request().postDataJSON()).toEqual(committed[0]!.body);
      const replay: Schema['CodexAccountStartV1'] = await response.json();
      expect(replay.acceptance).toEqual({ ...committed[0]!.result.acceptance, replayed: true });
      await expect(login).toBeEnabled();
      await page.getByRole('button', { name: '有新版本', exact: true }).click();
      await expect(page.getByRole('button', { name: '确认更新', exact: true })).toBeEnabled();
      const reloaded = page.waitForEvent('load');
      await page.getByRole('button', { name: '确认更新', exact: true }).click(); await reloaded;
      await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
      const saved: Checkpoint = JSON.parse(readFileSync(projectFile, 'utf8'));
      const listing = await page.request.get('/api/v2/projects?limit=100');
      expect(listing.status()).toBe(200);
      const body: { items: Schema['ProjectView'][] } = await listing.json();
      expect(body.items).toEqual([saved.receipt.resource]);
      expect(writes).toEqual(['POST /api/v2/codex/login/start', 'POST /api/v2/codex/login/start']);
    } finally {
      releaseAck(); await context.setOffline(false);
      await context.unroute('**/api/v2/codex/login/start');
      writeFileSync(workerFile, originalWorker);
    }
  });
}
