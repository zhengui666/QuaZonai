import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';

// UI-only contract fixtures: no native account, OAuth traffic or backend success.
async function setup(page: Page) {
  const now = new Date().toISOString();
  const profile: Schema['CodexProfileViewV1'] = {
    id: '01990000-0000-7000-8000-000000000001', name: '研究员', revision: '9007199254740993',
    created_at: now, updated_at: now, connection_mode: 'SYSTEM', profile_origin: 'OPERATOR_MOUNT', home_binding: 'test-only',
    model_settings: { schema_version: 1, use_default_model_settings: true, saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false },
  };
  const view: Schema['CodexProbeViewV1'] = {
    schema_version: 1, id: '01990000-0000-7000-8000-000000000002', profile_id: profile.id, profile_revision: profile.revision,
    observed_at: now, valid_until: new Date(Date.now() + 60_000).toISOString(), outcome: { status: 'UNAVAILABLE', reason: 'AUTHENTICATION_REQUIRED' },
  };
  const state = {
    operation: null as Schema['CodexAccountOperationV1'] | null,
    starts: [] as { key: string | undefined; body: unknown }[], cancels: [] as { key: string | undefined; body: unknown }[],
    probes: 0, dropStart: false, dropCancel: false, stale: false,
  };
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown, status = 200) => route.fulfill({ status, body: JSON.stringify(json), contentType: 'application/json' });
    if (path === '/api/v2/projects') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/codex') return reply({ schema_version: 1, items: [profile], next_cursor: null });
    if (path === `/api/v2/settings/codex/${profile.id}`) return reply(profile);
    if (path === '/api/v2/codex/models') return reply({ schema_version: 1, profile_id: profile.id,
      profile_revision: profile.revision, state: state.stale ? 'STALE' : 'UNAVAILABLE', observation: view });
    if (path === '/api/v2/codex/probe') {
      state.probes++; state.stale = false;
      return reply({ schema_version: 1, replayed: false, resource: view });
    }
    if (path === '/api/v2/codex/login') return reply(state.operation);
    if (path === '/api/v2/codex/login/start') {
      state.starts.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      state.operation ??= { schema_version: 1, revision: '9007199254740994', state: 'WAITING', updated_at: now,
        operation: { id: '01990000-0000-7000-8000-000000000003', profile_id: profile.id, profile_revision: profile.revision,
          action: 'LOGIN', created_at: now, deadline_at: new Date(Date.now() + 900_000).toISOString() },
        finished_at: null, reason: null, account: null };
      if (state.dropStart) { state.dropStart = false; return route.abort('failed'); }
      return reply({ schema_version: 1, acceptance: { schema_version: 1, replayed: state.starts.length > 1, resource: state.operation.operation },
        current: state.operation, device_code: { verification_url: 'https://auth.openai.com/codex/device', user_code: 'TEST-ONLY' } }, 202);
    }
    if (path === '/api/v2/codex/login/cancel') {
      state.cancels.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      state.operation = { ...state.operation!, state: 'CANCEL_REQUESTED', revision: '9007199254740995' };
      if (state.dropCancel) { state.dropCancel = false; return route.abort('failed'); }
      return reply({ schema_version: 1, replayed: state.cancels.length > 1, resource: state.operation }, 202);
    }
    return route.abort('blockedbyclient');
  });
  const open = async () => {
    await page.goto('/');
    await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  };
  await open();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeEnabled();
  return { state, open };
}

test('lost login ACK reuses the original identity; success clears the code and refreshes models', async ({ page }, testInfo) => {
  const { state } = await setup(page); state.dropStart = true;
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveText('TEST-ONLY');
  expect(state.starts).toHaveLength(2); expect(state.starts[1]).toEqual(state.starts[0]);
  expect(state.starts[0]?.body).toMatchObject({ expected_revision: '9007199254740993' });
  await expect(page.getByRole('link', { name: '打开 ChatGPT 授权页面' })).toHaveAttribute('rel', 'noopener noreferrer');
  await expect(page.getByRole('button', { name: '模型设置' })).toBeDisabled();
  expect(await page.evaluate(() => JSON.stringify([localStorage, sessionStorage]))).not.toContain('TEST-ONLY');
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await page.screenshot({ path: testInfo.outputPath(`synthetic-login-${width}.png`), animations: 'disabled' });
  }
  state.operation = { ...state.operation!, state: 'SUCCEEDED', revision: '9007199254740995', reason: 'NATIVE_LOGIN_COMPLETED',
    account: { requires_openai_auth: true, authentication_kind: 'CHATGPT', plan_type: 'test-only' }, finished_at: new Date().toISOString() };
  state.stale = true;
  await expect(page.getByText('ChatGPT 登录成功', { exact: true })).toBeVisible();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveCount(0);
  await expect.poll(() => state.probes).toBe(1);
});

test('lost cancel ACK preserves the request and stays pending until native confirmation', async ({ page }) => {
  const { state } = await setup(page);
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toBeVisible();
  state.dropCancel = true;
  await page.getByRole('button', { name: '取消登录', exact: true }).click();
  await expect(page.getByText('正在取消，请等待确认')).toBeVisible();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '取消登录', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '取消登录', exact: true }).click();
  await expect.poll(() => state.cancels.length).toBe(2); expect(state.cancels[1]).toEqual(state.cancels[0]);
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeDisabled();
  state.operation = { ...state.operation!, state: 'CANCELLED', revision: '9007199254740996', reason: 'NATIVE_CANCEL_CONFIRMED', finished_at: new Date().toISOString() };
  await expect(page.getByText('登录已取消', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeEnabled();
});

test('reload restores only status, and a local deadline never invents a terminal result', async ({ page }) => {
  const { state, open } = await setup(page);
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toBeVisible();
  page.once('dialog', dialog => dialog.accept());
  await open();
  await expect(page.getByText('请在发起登录的页面完成授权，或取消后重新登录。')).toBeVisible();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveCount(0);
  expect(state.starts).toHaveLength(1);
  state.operation!.operation.deadline_at = new Date(Date.now() - 1000).toISOString();
  await expect(page.getByText('等待授权已超时，可重新登录；原操作结果仍待确认')).toBeVisible();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeEnabled();
  expect(state.operation!.state).toBe('WAITING');
});
