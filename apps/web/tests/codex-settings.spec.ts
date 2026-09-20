import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fixture, id, navigate, problem, reply } from './fixtures';

// Presentation fixtures only. Actual native profiles, PostgreSQL authority and
// official Codex subprocess behavior are exercised by the Rust HTTP suites.
const stamp = '2026-09-11T00:00:00Z';
const revision = '9007199254740993';
const saved = { schema_version: 1, use_default_model_settings: true,
  saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false };
const profile = { id: id(91), name: '原生系统配置', home_binding: 'local-researcher',
  profile_origin: 'OPERATOR_MOUNT', connection_mode: 'SYSTEM', model_settings: saved, revision, created_at: stamp, updated_at: stamp };
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

async function setup(page: Page, options: { empty?: boolean; stale?: boolean; loseAck?: boolean; rejectRetry?: boolean; oldOverrides?: boolean; malformed?: boolean; validUntil?: string; overrides?: { saved_model?: string | null; saved_reasoning_effort?: string | null; saved_fast_mode?: boolean; use_default_model_settings?: boolean } } = {}) {
  await fixture(page);
  const writes: Write[] = [];
  let current: Record<string, unknown> | undefined = options.empty ? undefined : structuredClone(profile);
  if (current && options.oldOverrides) current.model_settings = { ...saved, use_default_model_settings: false,
    saved_model: 'retired-native-model', saved_reasoning_effort: 'retired-native-effort', saved_fast_mode: true };
  if (current && options.overrides) current.model_settings = { ...saved, ...options.overrides };
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
    expect(method).toBe('PATCH');
    expect(Object.keys(body).sort()).toEqual(['expected_revision', 'model_settings', 'schema_version']);
    current = { ...profile, model_settings: body.model_settings };
    return reply(route, { schema_version: 1, resource: current, replayed: writes.length > 1 }, method === 'POST' ? 201 : 200);
  });
  await page.route('**/api/v2/codex/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/models') || path.endsWith('/account')) return reply(route, {
      schema_version: 1, profile_id: profile.id, profile_revision: revision,
      state: options.stale ? 'STALE' : 'AVAILABLE', observation: options.stale
        ? { ...observed, valid_until: stamp } : { ...observed, valid_until: options.validUntil ?? observed.valid_until },
    });
    return reply(route, problem('NOT_FOUND', 404, '不存在的展示测试入口'), 404);
  });
  await page.goto('/');
  await navigate(page, '设置');
  return writes;
}

async function edit(page: Page) {
  await expect(page.getByText('native-model', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '模型设置', exact: true }).click();
  return page.getByRole('dialog', { name: '模型设置', exact: true });
}

test('native settings have no profile registration, provider or login controls', async ({ page }) => {
  const writes = await setup(page);
  const dialog = await edit(page);
  for (const label of ['配置名称', 'Provider HTTPS API 地址', '部署登记的原生账号目录']) {
    await expect(dialog.getByLabel(label)).toHaveCount(0);
  }
  for (const name of ['登记 Codex 配置', '登录 ChatGPT 账号', '注销原生账号']) {
    await expect(page.getByRole('button', { name, exact: true })).toHaveCount(0);
  }
  await expect(dialog.getByRole('switch', { name: '本机默认', exact: true })).toBeChecked();
  await expect(dialog.getByRole('slider')).toBeDisabled();
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes).toHaveLength(1);
  expect(writes[0]?.body).toEqual({ schema_version: 1, expected_revision: revision, model_settings: saved });
});

test('reasoning-only override uses the native model, effort labels and exact bigint revision', async ({ page }) => {
  const writes = await setup(page); const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
  const slider = dialog.getByRole('slider', { name: '推理强度', exact: true });
  await expect(slider).toBeEnabled();
  await slider.focus(); await slider.press('End');
  await expect(slider).toHaveAttribute('aria-valuenow', '2');
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(writes[0]?.body).toEqual({ schema_version: 1, expected_revision: revision,
    model_settings: { ...saved, use_default_model_settings: false, saved_reasoning_effort: 'exhaustive' } });
});

test('stale catalogue cannot enable unsupported model, effort or acceleration', async ({ page }) => {
  const writes = await setup(page, { stale: true }); const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
  await expect(dialog.getByRole('combobox', { name: '模型', exact: true })).toBeDisabled();
  await expect(dialog.getByRole('slider')).toHaveCount(0);
  await expect(dialog.getByRole('switch', { name: '加速', exact: true })).toBeDisabled();
  await expect(dialog.getByText('模型目录未就绪')).toBeVisible();
  await expect(dialog.getByRole('button', { name: '保存', exact: true })).toBeDisabled();
  await dialog.locator('form').dispatchEvent('submit');
  expect(writes).toHaveLength(0);
});

for (const rejectRetry of [false, true]) {
  test(`unknown save retains its exact request across retry/conflict (${rejectRetry})`, async ({ page }) => {
    const writes = await setup(page, { loseAck: true, rejectRetry });
    const dialog = await edit(page);
    await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    const retry = dialog.getByRole('button', { name: '重试保存', exact: true });
    await expect(retry).toBeVisible();
    await expect(dialog.getByRole('switch', { name: '本机默认', exact: true })).toBeDisabled();
    await retry.click();
    if (rejectRetry) {
      await expect(dialog.getByText('重试被拒绝；此前请求结果仍需核对')).toBeVisible();
      await expect(dialog.getByRole('switch', { name: '本机默认', exact: true })).toBeDisabled();
      await retry.click();
    }
    await expect(dialog).toBeHidden();
    expect(writes).toHaveLength(rejectRetry ? 3 : 2);
    expect(writes[0]?.key).toBeTruthy();
    for (const write of writes.slice(1)) expect(write).toEqual(writes[0]);
  });
}

test('native defaults preserve saved overrides without inventing a replacement model', async ({ page }) => {
  const writes = await setup(page, { stale: true, oldOverrides: true }); const dialog = await edit(page);
  const defaults = dialog.getByRole('switch', { name: '本机默认', exact: true });
  const save = dialog.getByRole('button', { name: '保存', exact: true });
  await expect(save).toBeDisabled();
  await defaults.click();
  await expect(save).toBeEnabled();
  await defaults.click();
  await expect(save).toBeDisabled();
  await dialog.locator('form').dispatchEvent('submit');
  expect(writes).toHaveLength(0);
  await defaults.click();
  await save.click();
  await expect(dialog).toBeHidden();
  expect(writes[0]?.body).toEqual({ schema_version: 1, expected_revision: revision, model_settings: {
    ...saved, saved_model: 'retired-native-model', saved_reasoning_effort: 'retired-native-effort', saved_fast_mode: true } });
});

test('missing or malformed native profiles never become a registration screen', async ({ page }) => {
  await setup(page, { malformed: true });
  await expect(page.getByText('响应数据不兼容')).toBeVisible();
  await expect(page.getByRole('button', { name: '登记 Codex 配置', exact: true })).toHaveCount(0);
});

for (const mode of ['light', 'dark']) {
  test(`native model controls are accessible in ${mode} at the current viewport`, async ({ page }) => {
    await page.addInitScript(value => localStorage.setItem('quazonai.theme', value), mode);
    await setup(page); const dialog = await edit(page);
    const save = dialog.getByRole('button', { name: '保存', exact: true });
    await save.scrollIntoViewIfNeeded(); await expect(save).toBeInViewport();
    expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze()).violations).toEqual([]);
  });
}

for (const overrides of [
  { saved_model: 'retired-native-model' },
  { saved_reasoning_effort: 'retired-native-effort' },
]) {
  test(`fresh catalogue cannot activate unsupported saved settings ${JSON.stringify(overrides)}`, async ({ page }) => {
    const writes = await setup(page, { overrides });
    const dialog = await edit(page);
    await expect(dialog.getByRole('button', { name: '保存', exact: true })).toBeEnabled();
    await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
    await expect(dialog.getByRole('button', { name: '保存', exact: true })).toBeDisabled();
    await dialog.locator('form').dispatchEvent('submit');
    expect(writes).toHaveLength(0);
  });
}

test('catalogue expiry blocks a new save but cannot change an unknown original replay', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-09-20T12:00:00Z') });
  const writes = await setup(page, { loseAck: true, validUntil: '2026-09-20T12:01:00Z' });
  const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  const retry = dialog.getByRole('button', { name: '重试保存', exact: true });
  await expect(retry).toBeVisible();
  await page.clock.fastForward('02:00');
  await expect(retry).toBeEnabled();
  await retry.click();
  await expect(dialog).toBeHidden();
  expect(writes).toHaveLength(2);
  expect(writes[1]).toEqual(writes[0]);
});

test('an already open editor cannot submit a catalogue after its deadline', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-09-20T12:00:00Z') });
  const writes = await setup(page, { validUntil: '2026-09-20T12:01:00Z' });
  const dialog = await edit(page);
  await dialog.getByRole('switch', { name: '本机默认', exact: true }).click();
  const save = dialog.getByRole('button', { name: '保存', exact: true });
  await expect(save).toBeEnabled();
  await page.clock.fastForward('02:00');
  await expect(save).toBeDisabled();
  await dialog.locator('form').dispatchEvent('submit');
  expect(writes).toHaveLength(0);
});
