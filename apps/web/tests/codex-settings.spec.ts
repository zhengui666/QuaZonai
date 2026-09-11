import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fixture, id, navigate, problem, reply, settingsCategory } from './fixtures';
import type { Schema } from '../src/api';

// Presentation fixtures only. Actual native profiles, PostgreSQL authority and
// official Codex subprocess behavior are exercised by the Rust HTTP suites.
const stamp = '2026-09-11T00:00:00Z';
const revision = '9007199254740993';
const saved = { schema_version: 1, use_default_model_settings: true,
  saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false };
const profile = { id: id(91), name: '原生系统配置', home_binding: 'system-fixture',
  profile_origin: 'OPERATOR_MOUNT', connection_mode: 'SYSTEM', custom_base_url: null,
  credential_configured: false, model_settings: saved, revision, created_at: stamp, updated_at: stamp };
const nativeModel = { capability: { schema_version: 1, id: 'native-model-id', model: 'native-model', display_name: '原生目录模型',
  hidden: false, is_default: false, default_reasoning_effort: 'balanced',
  supported_reasoning_efforts: [{ reasoning_effort: 'balanced', description: '原生目录：平衡' },
    { reasoning_effort: 'exhaustive', description: '原生目录：深入' }], fetched_at: stamp, profile_revision: revision },
  service_tiers: [{ id: 'fast', name: 'Fast', description: '原生目录中的可选加速' }], default_service_tier: null };
const observed = { schema_version: 1, id: id(92), profile_id: profile.id, profile_revision: revision,
  observed_at: stamp, valid_until: '2099-01-01T00:00:00Z', outcome: { status: 'AVAILABLE', native_version: '0.144.4',
    account: { requires_openai_auth: false, authentication_kind: null, plan_type: null },
    effective: { model: 'native-model', provider: 'configured-native-provider', reasoning_effort: 'balanced', service_tier: null },
    models: [nativeModel] } };
type Write = { method: string; key: string | undefined; body: Record<string, unknown> };

async function setup(page: Page, options: { empty?: boolean; stale?: boolean; loseAck?: boolean; rejectRetry?: boolean; oldOverrides?: boolean; malformed?: boolean } = {}) {
  await fixture(page);
  const writes: Write[] = [];
  let current: Record<string, unknown> | undefined = options.empty ? undefined : structuredClone(profile);
  if (current && options.oldOverrides) current.model_settings = { ...saved, use_default_model_settings: false,
    saved_model: 'retired-native-model', saved_reasoning_effort: 'retired-native-effort', saved_fast_mode: true };
  await page.route('**/api/v2/settings/codex**', async route => {
    const method = route.request().method();
    const path = new URL(route.request().url()).pathname;
    if (method === 'GET') {
      if (options.malformed) return reply(route, { profiles: [] });
      if (path.endsWith(profile.id)) return reply(route, current ?? problem('NOT_FOUND', 404, '配置不存在'), current ? 200 : 404);
      return reply(route, { schema_version: 1, items: current ? [current] : [], next_cursor: null });
    }
    const body = route.request().postDataJSON() as Record<string, unknown>;
    writes.push({ method, key: route.request().headers()['idempotency-key'], body });
    if (options.loseAck && writes.length === 1) return route.abort('failed');
    if (options.rejectRetry && writes.length === 2) return reply(route, problem('REVISION_CONFLICT', 409, '重试被拒绝；此前请求结果仍需核对'), 409);
    current = { ...profile, name: body.name, model_settings: body.model_settings,
      home_binding: body.home_binding ?? profile.home_binding, profile_origin: body.profile_origin ?? profile.profile_origin };
    return reply(route, { schema_version: 1, resource: current, replayed: writes.length > 1 }, method === 'POST' ? 201 : 200);
  });
  await page.route('**/api/v2/codex/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/login') && route.request().method() === 'GET') return reply(route, null);
    if (path.endsWith('/homes')) return reply(route, [{ reference: 'system-fixture', label: '部署选定的系统目录', profile_origin: 'OPERATOR_MOUNT' }]);
    if (path.endsWith('/models') || path.endsWith('/account')) return reply(route, {
      schema_version: 1, profile_id: profile.id, profile_revision: revision,
      state: options.stale ? 'STALE' : 'AVAILABLE', observation: options.stale
        ? { ...observed, valid_until: stamp } : observed,
    });
    return reply(route, problem('NOT_FOUND', 404, '不存在的展示测试入口'), 404);
  });
  await page.goto('/');
  await navigate(page, '设置');
  await settingsCategory(page, 'Codex 模型与连接');
  return writes;
}

async function edit(page: Page) {
  await page.getByRole('button', { name: '查看 Codex 配置', exact: true }).click();
  await expect(page.getByText('configured-native-provider', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '修改 Codex 设置', exact: true }).click();
  return page.getByRole('dialog');
}

test('SYSTEM registration has no provider URL/key injection and keeps native model defaults', async ({ page }) => {
  const writes = await setup(page, { empty: true });
  await page.getByRole('button', { name: '登记 Codex 配置', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('配置名称').fill('使用原生订阅设置');
  await dialog.getByRole('combobox', { name: '部署登记的原生账号目录' }).click();
  await page.getByText('部署选定的系统目录（显式挂载）', { exact: true }).click();
  await expect(dialog.getByLabel('Provider HTTPS API 地址')).toHaveCount(0);
  await expect(dialog.getByRole('slider')).toHaveCount(0);
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes).toHaveLength(1);
  expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body).toEqual({ schema_version: 1, name: '使用原生订阅设置',
    home_binding: 'system-fixture', profile_origin: 'OPERATOR_MOUNT', connection: { mode: 'SYSTEM' }, model_settings: saved });
});

test('custom Provider uses its own write-only secret purpose rather than a TLS or Runtime credential', async ({ page }) => {
  const writes = await setup(page, { empty: true });
  let purpose: unknown;
  let registrations = 0;
  await page.route('**/api/v2/settings/credentials', async route => {
    const body = route.request().postDataJSON();
    purpose = body.intent?.purpose;
    registrations += 1;
    expect(route.request().headers()['idempotency-key']).toBeTruthy();
    return reply(route, { schema_version: 1, replayed: false, resource: {
      id: id(93), purpose: 'CUSTOM_PROVIDER', label: 'CUSTOM_PROVIDER service credential', created_at: stamp,
    } }, 201);
  });
  await page.getByRole('button', { name: '登记 Codex 配置', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('配置名称').fill('独立 Provider 配置');
  await dialog.getByRole('combobox', { name: '部署登记的原生账号目录' }).click();
  await page.getByText('部署选定的系统目录（显式挂载）', { exact: true }).click();
  await dialog.getByRole('radio', { name: '自定义 Provider', exact: true }).check();
  await dialog.getByLabel('Provider HTTPS API 地址').fill('https://provider.example/v1');
  const secret = dialog.getByLabel('新的 CUSTOM_PROVIDER 凭据', { exact: true });
  await secret.fill('controlled-provider-fixture');
  await dialog.getByRole('button', { name: '登记凭据', exact: true }).click();
  await expect(secret).toHaveValue('');
  await expect(dialog.getByText('新凭据已登记，提交配置后才会绑定。')).toBeVisible();
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(registrations).toBe(1);
  expect(purpose).toBe('CUSTOM_PROVIDER');
  expect(writes[0]?.body).toMatchObject({ connection: {
    mode: 'CUSTOM_PROVIDER', base_url: 'https://provider.example/v1', credential_ref: id(93),
  } });
  expect(JSON.stringify(writes)).not.toContain('controlled-provider-fixture');
});

test('effort-only mode uses the observed model and exact bigint revision without hard-coded effort names', async ({ page }) => {
  const writes = await setup(page);
  const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '使用 Codex 原生默认模型设置' }).click();
  const slider = dialog.getByRole('slider', { name: '保存的推理强度' });
  await slider.focus(); await slider.press('End');
  await expect(dialog.getByText('保存值：exhaustive', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes[0]?.body).toMatchObject({ schema_version: 1, expected_revision: revision,
    connection: { mode: 'SYSTEM' }, model_settings: { use_default_model_settings: false,
      saved_model: null, saved_reasoning_effort: 'exhaustive', saved_fast_mode: false } });
});

test('stale model observations cannot enable effort or fast overrides', async ({ page }) => {
  const writes = await setup(page, { stale: true });
  const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '使用 Codex 原生默认模型设置' }).click();
  await expect(dialog.getByRole('combobox', { name: '保存的模型覆盖' })).toBeDisabled();
  await expect(dialog.getByRole('slider')).toHaveCount(0);
  await expect(dialog.getByRole('switch', { name: '使用原生加速档位' })).toBeDisabled();
  await expect(dialog.getByText('模型能力尚未由当前配置的有效原生探测确认。')).toBeVisible();
  expect(writes).toHaveLength(0);
});

test('an unknown save ACK retains the exact original request and idempotency key', async ({ page }) => {
  const writes = await setup(page, { loseAck: true });
  const dialog = await edit(page);
  await dialog.getByLabel('配置名称').fill('原始保存意图');
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '重试同一保存请求', exact: true })).toBeVisible();
  await expect(dialog.getByLabel('配置名称')).toBeDisabled();
  await dialog.getByRole('button', { name: '重试同一保存请求', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes).toHaveLength(2);
  expect(writes[1]).toEqual(writes[0]);
});

test('a later conflict cannot unlock an earlier unconfirmed save for a different request', async ({ page }) => {
  const writes = await setup(page, { loseAck: true, rejectRetry: true });
  const dialog = await edit(page);
  await dialog.getByLabel('配置名称').fill('保留无法确认的原始请求');
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  const retry = dialog.getByRole('button', { name: '重试同一保存请求', exact: true });
  await expect(retry).toBeVisible();
  await retry.click();
  await expect(dialog.getByText('重试被拒绝；此前请求结果仍需核对')).toBeVisible();
  await expect(dialog.getByLabel('配置名称')).toBeDisabled();
  await expect(retry).toBeEnabled();
  await retry.click();
  await expect(dialog).toBeHidden();
  expect(writes).toHaveLength(3);
  expect(writes[1]).toEqual(writes[0]);
  expect(writes[2]).toEqual(writes[0]);
});

test('stale retired model settings can be cleared without inventing a replacement capability', async ({ page }) => {
  const writes = await setup(page, { stale: true, oldOverrides: true });
  const dialog = await edit(page);
  await expect(dialog.getByRole('combobox', { name: '保存的模型覆盖' })).toBeDisabled();
  await dialog.getByRole('button', { name: '清除模型覆盖', exact: true }).click();
  await dialog.getByRole('button', { name: '清除推理强度覆盖', exact: true }).click();
  const fast = dialog.getByRole('switch', { name: '使用原生加速档位' });
  await expect(fast).toBeEnabled();
  await fast.click();
  await expect(fast).not.toBeChecked();
  await expect(fast).toBeDisabled();
  await expect(dialog.getByRole('slider')).toHaveCount(0);
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes[0]?.body).toMatchObject({ expected_revision: revision,
    model_settings: { use_default_model_settings: false, saved_model: null,
      saved_reasoning_effort: null, saved_fast_mode: false } });
});

test('enabling native defaults preserves dormant overrides instead of silently deleting them', async ({ page }) => {
  const writes = await setup(page, { stale: true, oldOverrides: true });
  const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '使用 Codex 原生默认模型设置' }).click();
  await expect(dialog.getByRole('button', { name: '清除模型覆盖', exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: '保存 Codex 配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes[0]?.body).toMatchObject({ model_settings: { use_default_model_settings: true,
    saved_model: 'retired-native-model', saved_reasoning_effort: 'retired-native-effort', saved_fast_mode: true } });
});

test('a malformed profile list is an error, never an empty setup success', async ({ page }) => {
  await setup(page, { malformed: true });
  await expect(page.getByText('响应字段或合同版本不兼容。未将它当成空列表或成功操作。')).toBeVisible();
  await expect(page.getByText('尚无 Codex 配置。先在部署中登记原生账号目录，然后在这里选择 SYSTEM 或独立 Provider。')).toHaveCount(0);
});

test('Codex configuration is reachable and accessible at the current viewport', async ({ page }) => {
  await setup(page);
  await page.getByRole('button', { name: '查看 Codex 配置', exact: true }).click();
  await expect(page.getByText('configured-native-provider', { exact: true })).toBeVisible();
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
  await page.getByRole('button', { name: '修改 Codex 设置', exact: true }).click();
  const save = page.getByRole('dialog').getByRole('button', { name: '保存 Codex 配置', exact: true });
  await save.scrollIntoViewIfNeeded();
  await expect(save).toBeInViewport();
});

test('native login keeps the same request after lost ACK and distinguishes cancellation from completion', async ({ page }) => {
  await setup(page);
  const ref = { id: id(95), profile_id: profile.id, profile_revision: revision, action: 'LOGIN' as const,
    created_at: stamp, deadline_at: '2099-01-01T00:00:00Z' };
  const waiting: Schema['CodexAccountOperationV1'] = { schema_version: 1, operation: ref, state: 'WAITING', revision: '2',
    updated_at: stamp, finished_at: null, reason: null, account: null };
  let current: Schema['CodexAccountOperationV1'] | null = null;
  const writes: Write[] = [];
  await page.route('**/api/v2/codex/login**', async route => {
    if (route.request().method() === 'GET') return reply(route, current);
    const body = route.request().postDataJSON();
    writes.push({ method: route.request().method(), body, key: route.request().headers()['idempotency-key'] });
    if (new URL(route.request().url()).pathname.endsWith('/cancel')) {
      current = { ...waiting, state: 'CANCEL_REQUESTED', revision: '3' };
      return reply(route, { schema_version: 1, resource: current, replayed: false }, 202);
    }
    current = waiting;
    if (writes.length === 1) return route.abort('failed');
    if (writes.length === 2) return reply(route, problem('REVISION_CONFLICT', 409, '仍需保留首次账号请求'), 409);
    return reply(route, { schema_version: 1, acceptance: { schema_version: 1, resource: ref, replayed: true }, current,
      device_code: { verification_url: 'https://auth.example.invalid/codex/device', user_code: 'UI-FIXTURE-CODE' } }, 202);
  });
  await page.getByRole('button', { name: '查看 Codex 配置', exact: true }).click();
  await page.getByRole('button', { name: '登录 ChatGPT 账号', exact: true }).click();
  await page.getByRole('button', { name: '确认账号操作', exact: true }).click();
  const retry = page.getByRole('button', { name: '重试同一账号请求', exact: true });
  await expect(retry).toBeVisible();
  await expect(page.getByRole('button', { name: '注销原生账号', exact: true })).toBeDisabled();
  await retry.click();
  await expect(page.getByText('仍需保留首次账号请求', { exact: true })).toBeVisible();
  await retry.click();
  await expect(page.getByText('UI-FIXTURE-CODE', { exact: true })).toBeVisible();
  await expect(retry).toHaveCount(0);
  await page.getByRole('button', { name: '重新显示原生设备码', exact: true }).click();
  await expect.poll(() => writes.length).toBe(4);
  expect(writes[0]?.body).toEqual({ schema_version: 1, profile_id: profile.id, expected_revision: revision });
  for (const write of writes.slice(1)) expect(write).toEqual(writes[0]);
  await expect(page.getByRole('button', { name: '修改 Codex 设置', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '请求取消登录', exact: true }).click();
  await expect(page.getByText('已请求取消，尚未确认', { exact: true })).toBeVisible();
  await expect(page.getByText('取消已确认', { exact: true })).toHaveCount(0);
  await expect(page.getByText('UI-FIXTURE-CODE', { exact: true })).toHaveCount(0);
  expect(writes[4]?.body).toEqual({ schema_version: 1, operation_id: ref.id, expected_revision: '2' });
  current = { ...waiting, state: 'SUCCEEDED', revision: '4', finished_at: stamp, reason: 'NATIVE_LOGIN_COMPLETED',
    account: { authentication_kind: 'CHATGPT', requires_openai_auth: true, plan_type: 'pro' } };
  await expect(page.getByText('原生账号操作已完成', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '探测 Codex 连接与模型', exact: true })).toBeEnabled();
  expect(await page.evaluate(() => JSON.stringify([Object.entries(localStorage), Object.entries(sessionStorage)]))).not.toContain('UI-FIXTURE-CODE');
});

test('reload reads account state without restarting login or recovering a device code', async ({ page }) => {
  await setup(page);
  let writes = 0;
  await page.route('**/api/v2/codex/login**', async route => {
    if (route.request().method() !== 'GET') writes += 1;
    return reply(route, { schema_version: 1,
      operation: { id: id(96), profile_id: profile.id, profile_revision: revision, action: 'LOGIN', created_at: stamp, deadline_at: stamp },
      state: 'WAITING', revision: '2', updated_at: stamp, finished_at: null, reason: null, account: null });
  });
  await page.getByRole('button', { name: '查看 Codex 配置', exact: true }).click();
  await expect(page.getByText('不能确认账号是否已经变化。', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '登录 ChatGPT 账号', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: '重新显示原生设备码', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '刷新账号操作状态', exact: true }).click();
  expect(writes).toBe(0);
});
