import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';

// UI-only contract fixtures: no native account, OAuth traffic or backend success.
async function setup(page: Page, ready = false) {
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
  const profiles = [profile, { ...structuredClone(profile), id: '01990000-0000-7000-8000-000000000004', name: '独立审阅员' }];
  function observed(current: Schema['CodexProfileViewV1']): Schema['CodexProbeViewV1'] {
    return { ...view, profile_id: current.id, profile_revision: current.revision,
      outcome: ready ? { status: 'AVAILABLE', native_version: '0.156.1', native_default_model: 'research-model',
        account: { requires_openai_auth: true, authentication_kind: 'CHATGPT', plan_type: 'test-only' },
        effective: { model: current.model_settings.saved_model ?? 'research-model', provider: 'openai',
          reasoning_effort: current.model_settings.saved_reasoning_effort ?? 'low',
          service_tier: current.model_settings.saved_fast_mode ? 'priority' : 'default' },
        models: ['research-model', 'review-model'].map(model => ({
          capability: { schema_version: 1, id: model, model, display_name: model, hidden: false, is_default: false,
            profile_revision: current.revision, fetched_at: now, default_reasoning_effort: 'low',
            supported_reasoning_efforts: ['low', 'high'].map(reasoning_effort => ({ reasoning_effort, description: 'Test only' })) },
          service_tiers: [{ id: 'priority', name: '加速', description: 'Test only' }], default_service_tier: null,
        })) } : view.outcome,
    };
  }
  const views = new Map(profiles.map(current => [current.id, observed(current)]));
  const state = {
    operation: null as Schema['CodexAccountOperationV1'] | null,
    starts: [] as { key: string | undefined; body: unknown }[], cancels: [] as { key: string | undefined; body: unknown }[],
    probes: 0, dropStart: false, rejectStart: false, terminalStart: false, dropCancel: false, stale: false,
    probeRequests: [] as { key: string | undefined; body: Schema['CodexProbeRequestV1'] }[],
    dropProbeAfterCommit: false, holdProbe: undefined as Promise<void> | undefined,
    holdStart: undefined as Promise<void> | undefined,
    holdSave: undefined as Promise<void> | undefined, dropSaveAfterCommit: false, rejectSave: false,
    saveKeys: [] as (string | undefined)[],
    saves: [] as { id: string; body: Schema['CodexProfileUpdateV1'] }[],
  };
  const saveResults = new Map<string, Schema['CodexProfileViewV1']>();
  const probeResults = new Map<string, Schema['CodexProbeViewV1']>();
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown, status = 200) => route.fulfill({ status, body: JSON.stringify(json), contentType: 'application/json' });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/codex') return reply({ schema_version: 1, items: profiles, next_cursor: null });
    const current = profiles.find(item => path === `/api/v2/settings/codex/${item.id}`);
    if (current) {
      if (request.method() !== 'PATCH') return reply(current);
      const body: Schema['CodexProfileUpdateV1'] = request.postDataJSON();
      state.saves.push({ id: current.id, body });
      const key = request.headers()['idempotency-key']; state.saveKeys.push(key);
      if (state.rejectSave) {
        state.rejectSave = false;
        return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
          type: 'about:blank', title: 'Invalid model settings', status: 422, code: 'INVALID_MODEL_SETTINGS',
          detail: 'Test model setting rejected', request_id: profile.id, retryable: false,
          safe_next_actions: [], field_errors: [],
        }) });
      }
      if (state.holdSave) { await state.holdSave; state.holdSave = undefined; }
      if (key && saveResults.has(key)) return reply({ schema_version: 1, replayed: true, resource: saveResults.get(key) });
      current.model_settings = body.model_settings; current.revision = (BigInt(current.revision) + 1n).toString();
      if (key) saveResults.set(key, structuredClone(current));
      if (state.dropSaveAfterCommit) { state.dropSaveAfterCommit = false; return route.abort('failed'); }
      return reply({ schema_version: 1, replayed: false, resource: current });
    }
    if (path === '/api/v2/codex/models') {
      const selected = profiles.find(item => item.id === new URL(request.url()).searchParams.get('profile_id'))!;
      const observation = views.get(selected.id)!;
      return reply({ schema_version: 1, profile_id: selected.id, profile_revision: selected.revision,
        state: state.stale || observation.profile_revision !== selected.revision ? 'STALE' : ready ? 'AVAILABLE' : 'UNAVAILABLE', observation });
    }
    if (path === '/api/v2/codex/probe') {
      state.probes++; state.stale = false;
      const body: Schema['CodexProbeRequestV1'] = request.postDataJSON();
      const key = request.headers()['idempotency-key']; state.probeRequests.push({ key, body });
      if (state.holdProbe) { await state.holdProbe; state.holdProbe = undefined; }
      if (key && probeResults.has(key)) return reply({ schema_version: 1, replayed: true, resource: probeResults.get(key) });
      const selected = profiles.find(item => item.id === body.profile_id)!;
      const observation = observed(selected); views.set(selected.id, observation);
      if (key) probeResults.set(key, observation);
      if (state.dropProbeAfterCommit) { state.dropProbeAfterCommit = false; return route.abort('failed'); }
      return reply({ schema_version: 1, replayed: false, resource: observation });
    }
    if (path === '/api/v2/codex/login') return reply(state.operation);
    if (path === '/api/v2/codex/login/start') {
      state.starts.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      state.operation ??= { schema_version: 1, revision: '9007199254740994', state: 'WAITING', updated_at: now,
        operation: { id: '01990000-0000-7000-8000-000000000003', profile_id: profile.id, profile_revision: profile.revision,
          action: 'LOGIN', created_at: now, deadline_at: new Date(Date.now() + 900_000).toISOString() },
        finished_at: null, reason: null, account: null };
      if (state.terminalStart) state.operation = { ...state.operation, state: 'FAILED', reason: 'DEPLOYMENT_UNAVAILABLE', finished_at: now };
      if (state.holdStart) { await state.holdStart; state.holdStart = undefined; }
      if (state.rejectStart) {
        state.rejectStart = false; state.operation = null;
        return route.fulfill({ status: 409, contentType: 'application/problem+json', body: JSON.stringify({
          type: 'about:blank', title: 'Profile changed', status: 409, code: 'PROFILE_CHANGED',
          detail: 'Profile changed before login', request_id: profile.id, retryable: false,
          safe_next_actions: ['RELOAD'], field_errors: [],
        }) });
      }
      if (state.dropStart) { state.dropStart = false; return route.abort('failed'); }
      return reply({ schema_version: 1, acceptance: { schema_version: 1, replayed: state.starts.length > 1, resource: state.operation.operation },
        current: state.operation, device_code: state.terminalStart ? null : { verification_url: 'https://auth.openai.com/codex/device', user_code: 'TEST-ONLY' } }, 202);
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
  return { state, open, profiles };
}

test('one shared account keeps model, reasoning and speed independent for each role', async ({ page }, testInfo) => {
  const { state, open, profiles } = await setup(page, true);
  await expect(page.getByText('共享 ChatGPT 账号', { exact: true })).toBeVisible();
  await expect(page.getByText('已登录 ChatGPT', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toHaveCount(1);
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toHaveCSS('color', 'rgb(255, 255, 255)');
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).hover();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toHaveCSS('color', 'rgb(255, 255, 255)');
  expect((await new AxeBuilder({ page }).include('button[aria-label="登录 ChatGPT"]').withRules(['color-contrast']).analyze()).violations).toEqual([]);
  const originalReviewer = structuredClone(profiles[1]);
  for (const [index, name, model, effort, fast] of [
    [0, '研究员', 'research-model', 'high', true],
    [1, '独立审阅员', 'review-model', 'low', false],
  ] as const) {
    if (index) {
      await page.getByRole('combobox', { name: 'Codex 角色' }).click();
      await page.getByText(name, { exact: true }).last().click();
    }
    const savesBefore = state.saves.length;
    await page.getByRole('switch', { name: '本机默认', exact: true }).click();
    await expect.poll(() => state.saves.length).toBe(savesBefore + 1);
    await expect(page.getByRole('combobox', { name: '模型', exact: true })).toBeEnabled();
    await page.getByRole('combobox', { name: '模型', exact: true }).click();
    await page.getByText(model, { exact: true }).last().click();
    await expect.poll(() => state.saves.length).toBe(savesBefore + 2);
    const slider = page.getByRole('slider', { name: '推理强度' });
    await expect(slider).toBeEnabled();
    await slider.focus(); await slider.press('Home');
    await slider.press(effort === 'high' ? 'End' : 'ArrowRight');
    await expect.poll(() => state.saves.length).toBe(savesBefore + 3);
    const speed = page.getByRole('switch', { name: '速度', exact: true });
    await expect(speed).not.toBeChecked();
    if (fast) await speed.click();
    await expect.poll(() => state.saves.length).toBe(savesBefore + (fast ? 4 : 3));
    expect(state.saves.at(-1)).toEqual({ id: profiles[index]!.id, body: {
      schema_version: 1, expected_revision: (BigInt('9007199254740993') + BigInt(fast ? 3 : 2)).toString(),
      model_settings: { schema_version: 1, use_default_model_settings: false, saved_model: model, saved_reasoning_effort: effort, saved_fast_mode: fast },
    } });
    if (!index) expect(profiles[1]).toEqual(originalReviewer);
  }
  await open();
  await expect(page.getByText('research-model / high', { exact: true })).toBeVisible();
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('独立审阅员', { exact: true }).last().click();
  await expect(page.getByText('review-model / low', { exact: true })).toBeVisible();
  await expect(page.getByText('已登录 ChatGPT', { exact: true })).toBeVisible();
  expect(state.starts).toHaveLength(0);
  expect(profiles[0]!.model_settings.saved_fast_mode).toBe(true);
  expect(profiles[1]!.model_settings.saved_fast_mode).toBe(false);
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await page.screenshot({ path: testInfo.outputPath(`synthetic-shared-roles-${width}.png`), animations: 'disabled' });
  }
});

test('navigation does not block an in-flight model autosave', async ({ page }) => {
  const { state, profiles } = await setup(page, true);
  let release!: () => void;
  state.holdSave = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect.poll(() => state.saves.length).toBe(1);
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  await expect(page.getByRole('heading', { name: '鉴权管理' })).toBeVisible();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  release();
  await expect.poll(() => profiles[0]!.model_settings.use_default_model_settings).toBe(false);
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await expect(page.getByRole('switch', { name: '本机默认' })).not.toBeChecked();
});

test('account actions wait for a model autosave on the same role', async ({ page }) => {
  const { state } = await setup(page, true);
  let release!: () => void;
  state.holdSave = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect.poll(() => state.saves.length).toBe(1);
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '刷新', exact: true })).toBeDisabled();
  expect(state.starts).toHaveLength(0);
  release();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeEnabled();
});

test('a lost probe response keeps its identity after switching roles', async ({ page }) => {
  const { state } = await setup(page, true);
  state.dropProbeAfterCommit = true;
  let release!: () => void;
  state.holdProbe = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect.poll(() => state.probes).toBe(1);
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('独立审阅员', { exact: true }).last().click();
  release();
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('研究员', { exact: true }).last().click();
  await expect(page.getByText('连接中断，提交结果未知；请重试当前操作')).toBeVisible();
  expect(state.probes).toBe(1);
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect.poll(() => state.probes).toBe(2);
  expect(state.probeRequests[1]).toEqual(state.probeRequests[0]);
});

test('switching roles keeps each autosave independent while a write is pending', async ({ page }) => {
  const { state, profiles } = await setup(page, true);
  let release!: () => void;
  state.holdSave = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect.poll(() => state.saves.length).toBe(1);
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('独立审阅员', { exact: true }).last().click();
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '刷新', exact: true })).toBeEnabled();
  const reviewerDefault = page.getByRole('switch', { name: '本机默认' });
  await expect(reviewerDefault).toBeEnabled();
  await reviewerDefault.click();
  await expect.poll(() => state.saves.length).toBe(2);
  expect(state.saves.map(save => save.id)).toEqual(profiles.map(profile => profile.id));
  release();
  await expect.poll(() => profiles.every(profile => !profile.model_settings.use_default_model_settings)).toBe(true);
});

test('a lost model-save response retries the identical write', async ({ page }) => {
  const { state, profiles } = await setup(page, true);
  state.dropSaveAfterCommit = true;
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '重试', exact: true }).click();
  await expect(page.getByRole('switch', { name: '本机默认' })).not.toBeChecked();
  expect(state.saves).toHaveLength(2); expect(state.saves[1]).toEqual(state.saves[0]);
  expect(state.saveKeys[1]).toBe(state.saveKeys[0]);
  expect(profiles[0]!.revision).toBe('9007199254740994');
});

test('a rejected reasoning edit restores the authoritative slider position', async ({ page }) => {
  const { state } = await setup(page, true);
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect(page.getByRole('slider', { name: '推理强度' })).toBeEnabled();
  state.rejectSave = true;
  const slider = page.getByRole('slider', { name: '推理强度' });
  await slider.focus(); await slider.press('End');
  await expect(page.getByText('Test model setting rejected')).toBeVisible();
  await expect(slider).toHaveAttribute('aria-valuenow', '0');
  await expect(page.getByText('推理强度：本机默认')).toBeVisible();
});

test('an uncertain model autosave keeps its retry identity after navigation', async ({ page }) => {
  const { state } = await setup(page, true);
  state.dropSaveAfterCommit = true;
  await page.getByRole('switch', { name: '本机默认' }).click();
  await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('button', { name: '重试', exact: true }).click();
  await expect(page.getByRole('switch', { name: '本机默认' })).not.toBeChecked();
  expect(state.saves).toHaveLength(2);
  expect(state.saves[1]).toEqual(state.saves[0]);
  expect(state.saveKeys[1]).toBe(state.saveKeys[0]);
});

test('the ChatGPT device challenge survives Settings navigation in memory', async ({ page }) => {
  const { state } = await setup(page);
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveText('TEST-ONLY');
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveText('TEST-ONLY');
  expect(state.starts).toHaveLength(1);
});

test('lost login ACK reuses the original identity; success clears the code and refreshes models', async ({ page }, testInfo) => {
  const { state } = await setup(page); state.dropStart = true;
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveText('TEST-ONLY');
  expect(state.starts).toHaveLength(2); expect(state.starts[1]).toEqual(state.starts[0]);
  expect(state.starts[0]?.body).toMatchObject({ expected_revision: '9007199254740993' });
  await expect(page.getByRole('link', { name: '打开 ChatGPT 授权页面' })).toHaveAttribute('rel', 'noopener noreferrer');
  await expect(page.getByRole('switch', { name: '本机默认' })).toBeDisabled();
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

test('a terminal native failure after a lost login ACK still offers the same-key retry', async ({ page }) => {
  const { state } = await setup(page);
  state.terminalStart = true; state.dropStart = true;
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect.poll(() => state.starts.length).toBe(2);
  expect(state.starts[1]).toEqual(state.starts[0]);
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('a definite login rejection remains visible after navigation', async ({ page }) => {
  const { state } = await setup(page);
  state.rejectStart = true;
  let release!: () => void;
  state.holdStart = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect.poll(() => state.starts.length).toBe(1);
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  release();
  await page.getByRole('tab', { name: 'Codex' }).click();
  await expect(page.getByText('Profile changed before login')).toBeVisible();
});

test('a terminal operation reconciles the previous role after navigation', async ({ page }) => {
  const { state } = await setup(page, true);
  state.dropStart = true;
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('独立审阅员', { exact: true }).last().click();
  state.operation = { ...state.operation!, state: 'FAILED', reason: 'DEPLOYMENT_UNAVAILABLE', finished_at: new Date().toISOString(),
    revision: '9007199254740995' };
  await expect.poll(() => state.starts.length).toBe(2);
  expect(state.starts[1]).toEqual(state.starts[0]);
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('a late lost login ACK reconciles after switching roles', async ({ page }) => {
  const { state } = await setup(page, true);
  state.dropStart = true;
  let release!: () => void;
  state.holdStart = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect.poll(() => state.starts.length).toBe(1);
  await page.getByRole('combobox', { name: 'Codex 角色' }).click();
  await page.getByText('独立审阅员', { exact: true }).last().click();
  state.operation = { ...state.operation!, state: 'FAILED', reason: 'DEPLOYMENT_UNAVAILABLE', finished_at: new Date().toISOString(),
    revision: '9007199254740995' };
  await expect(page.getByText('Codex 运行环境不可用，请检查部署配置')).toBeVisible();
  release();
  await expect.poll(() => state.starts.length).toBe(2);
  expect(state.starts[1]).toEqual(state.starts[0]);
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('a successful background login replay clears its stale network error', async ({ page }) => {
  const { state } = await setup(page);
  state.dropStart = true;
  let release!: () => void;
  state.holdStart = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('button', { name: '登录 ChatGPT', exact: true }).click();
  await expect.poll(() => state.starts.length).toBe(1);
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  release();
  await expect.poll(() => state.starts.length).toBe(2);
  await page.getByRole('tab', { name: 'Codex' }).click();
  await expect(page.getByLabel('ChatGPT 授权码')).toHaveText('TEST-ONLY');
  await expect(page.getByText('连接中断，提交结果未知；请重试当前操作')).toHaveCount(0);
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
