import { test, expect } from '@playwright/test';
import { appendFileSync, readFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import * as OTPAuth from 'otpauth';

type Fixture = { baseUrl: string; capabilityId: string; capability: string; redactionsFile: string };

function fixture(): Fixture {
  const path = process.env.QUAZONAI_WEB_E2E_FIXTURE;
  if (!path) throw new Error('Missing private native fixture; use the native-browser harness');
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (typeof value !== 'object' || value === null) throw new Error('Invalid private fixture');
  const fields = value as Record<string, unknown>;
  if (typeof fields.baseUrl !== 'string' || fields.baseUrl !== process.env.QUAZONAI_WEB_E2E_ORIGIN
    || typeof fields.capabilityId !== 'string' || typeof fields.capability !== 'string'
    || fields.redactionsFile !== resolve(dirname(path), 'redactions.jsonl')) {
    throw new Error('Private fixture fields do not match the test-owned runtime');
  }
  return fields as Fixture;
}

function rememberPrivateValue(config: Fixture, value: string) {
  // Only the harness reads this private redaction manifest; it is not an artifact.
  appendFileSync(config.redactionsFile, `${JSON.stringify(value)}\n`, { mode: 0o600 });
}

test('real TOTP enrollment, lost-ACK project retry, CSRF, mobile layout and logout', async ({ page, context }) => {
  const config = fixture();
  // Only assert presence, never print any credential on assertion failure.
  expect(['QUAZONAI_WEB_TEST_ADMIN_URL', 'DATABASE_URL', 'PGPASSWORD', 'GH_TOKEN', 'GITHUB_TOKEN']
    .some((key) => process.env[key] !== undefined)).toBe(false);
  const name = `Native browser ${randomUUID()}`;
  let projectId: string;
  let initialKey: string | undefined;

  await test.step('bind the first operator through the actual Rust API', async () => {
    await page.goto('/');
    await expect(page.getByRole('heading', { name: '绑定你的验证器' })).toBeVisible();
    await page.getByLabel('初始化凭据编号').fill(config.capabilityId);
    await page.getByLabel('一次性初始化凭据').fill(config.capability);
    const startPromise = page.waitForResponse((response) =>
      new URL(response.url()).pathname === '/api/v2/bootstrap/start' && response.request().method() === 'POST');
    await page.getByRole('button', { name: '显示绑定二维码', exact: true }).click();
    const started = await startPromise;
    expect(started.ok()).toBe(true);
    const body: unknown = await started.json();
    if (typeof body !== 'object' || body === null || !('provisioning_uri' in body)
      || typeof body.provisioning_uri !== 'string') {
      throw new Error('Bootstrap start did not return its documented provisioning URI');
    }
    rememberPrivateValue(config, body.provisioning_uri);
    const totp = OTPAuth.URI.parse(body.provisioning_uri);
    if (!(totp instanceof OTPAuth.TOTP)) throw new Error('Bootstrap did not issue a TOTP provisioning URI');
    rememberPrivateValue(config, totp.secret.base32);
    const code = totp.generate();
    rememberPrivateValue(config, code);
    await expect(page.getByLabel('动态验证码')).toHaveAttribute('aria-required', 'true');
    await page.getByLabel('动态验证码').fill(code);
    const finishPromise = page.waitForResponse((response) =>
      new URL(response.url()).pathname === '/api/v2/bootstrap/confirm' && response.request().method() === 'POST');
    await page.getByRole('button', { name: '确认绑定并登录', exact: true }).click();
    const confirmed = await finishPromise;
    const sent = confirmed.request().postDataJSON() as Record<string, unknown>;
    expect({
      schema: sent.schema_version === 1,
      enrollment: typeof sent.enrollment_id === 'string',
      code: typeof sent.code === 'string' && /^[0-9]{6}$/.test(sent.code),
      trust: sent.trust_device === false,
      label: sent.device_label === null || typeof sent.device_label === 'string',
    }, 'Browser must send the complete native authentication contract').toEqual({
      schema: true, enrollment: true, code: true, trust: true, label: true,
    });
    if (!confirmed.ok()) {
      const rejected: unknown = await confirmed.json().catch(() => null);
      const code = typeof rejected === 'object' && rejected !== null && 'code' in rejected
        && typeof rejected.code === 'string' && /^[A-Z_]{1,80}$/.test(rejected.code)
        ? rejected.code : 'UNEXPECTED_ERROR_CONTRACT';
      // Never include request bodies, response payloads or provisioning data.
      throw new Error(`Native enrollment confirmation: HTTP ${confirmed.status()}, ${code}`);
    }
    await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(200);
    const cookies = await context.cookies();
    expect(cookies.length).toBeGreaterThan(0);
    for (const cookie of cookies) expect(cookie.httpOnly).toBe(true);
  });

  await test.step('lose a real committed response and retry the same idempotency key', async () => {
    let committedStatus: number | undefined;
    let intercepted = false;
    await page.route('**/api/v2/projects', async (route) => {
      if (route.request().method() !== 'POST' || intercepted) return route.continue();
      intercepted = true;
      initialKey = route.request().headers()['idempotency-key'];
      // The real Rust transaction commits before we discard its ACK. No mock body.
      const upstream = await route.fetch({ maxRetries: 0, timeout: 20_000 });
      committedStatus = upstream.status();
      await upstream.dispose();
      await route.abort('failed');
    });
    await page.getByRole('button', { name: '新建研究', exact: true }).click();
    await page.getByLabel('研究名称').fill(name);
    await page.getByLabel('研究说明', { exact: true }).fill('Native browser acceptance; research only, no qualification claims.');
    await page.getByRole('button', { name: '保存项目', exact: true }).click();
    await expect(page.getByText(/连接中断，尚不能确定操作是否已提交/)).toBeVisible();
    expect(committedStatus).toBeGreaterThanOrEqual(200);
    expect(committedStatus).toBeLessThan(300);
    expect(typeof initialKey).toBe('string');
    await page.unroute('**/api/v2/projects');
    const retryPromise = page.waitForResponse((response) =>
      new URL(response.url()).pathname === '/api/v2/projects' && response.request().method() === 'POST');
    await page.getByRole('button', { name: '保存项目', exact: true }).click();
    const retried = await retryPromise;
    expect(retried.ok()).toBe(true);
    expect(retried.request().headers()['idempotency-key']).toBe(initialKey);
    const receipt: unknown = await retried.json();
    expect(typeof receipt === 'object' && receipt !== null && 'replayed' in receipt && receipt.replayed === true).toBe(true);
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

  await test.step('keep authenticated native UI and its editor inside three viewports', async () => {
    for (const viewport of [{ width: 1440, height: 900 }, { width: 768, height: 1024 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
      await page.getByRole('button', { name: '新建研究', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toBeVisible();
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width + 1);
      await page.getByRole('button', { name: '取消', exact: true }).click();
      await expect(page.getByLabel('研究名称')).toHaveCount(0);
    }
  });

  await test.step('revoke the session on confirmed logout and deny further writes', async () => {
    await page.getByRole('button', { name: '退出登录', exact: true }).click();
    await page.getByRole('button', { name: '确认退出', exact: true }).click();
    await expect(page.getByRole('button', { name: '登录', exact: true })).toBeVisible();
    expect((await page.request.get('/api/v2/auth/session')).status()).toBe(401);
    const denied = await page.request.post('/api/v2/projects', {
      headers: { Origin: config.baseUrl, 'Idempotency-Key': randomUUID() },
      data: { schema_version: 1, name: 'Must not exist after logout', description: '', fork_from_project_id: null },
    });
    expect(denied.status()).toBe(401);
    await expect(page.getByText(name, { exact: true })).toHaveCount(0);
  });
});
