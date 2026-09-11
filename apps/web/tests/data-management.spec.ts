import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { randomBytes } from 'node:crypto';
import AxeBuilder from '@axe-core/playwright';
import { fixture, id, navigate, reply, problem } from './fixtures';

// Controlled presentation data only. Credentials below are generated, disposable test values.
test.afterEach(async ({ page }, info) => {
  if (info.status === info.expectedStatus) return;
  // Only this controlled presentation suite: record geometry, never input values,
  // request bodies, cookies, localStorage or credential text.
  const geometry = await page.locator('.ant-select-dropdown, .ant-modal, .resource-select').evaluateAll(elements => elements.map(element => {
    const chain = [];
    for (let current: Element | null = element; current && chain.length < 8; current = current.parentElement) {
      const style = getComputedStyle(current);
      chain.push({ tag: current.tagName, classes: current.className, rectangle: current.getBoundingClientRect().toJSON(),
        position: style.position, transform: style.transform, overflow: style.overflow, top: style.top, left: style.left,
        scrollTop: current.scrollTop, scrollLeft: current.scrollLeft });
    }
    return { viewport: { width: innerWidth, height: innerHeight, scrollX, scrollY }, chain };
  }));
  await info.attach('native-select-geometry', { body: JSON.stringify(geometry, null, 2), contentType: 'application/json' });
});

const at = '2026-09-10T00:00:00Z';
const revision = '9007199254740993';
const runtime = {
  id: id(20), configuration: { name: '原生测试 Runtime', endpoint: 'https://runtime.example', tls_policy: 'SYSTEM_CA',
    allowed_capabilities: ['DATA_VALIDATE'], enabled: true, development_http: false }, protocol_version: 1,
  credential_configured: true, ca_configured: false, last_capability_snapshot_artifact_id: null,
  revision, created_at: at, updated_at: at,
};
const source = { id: id(30), name: '测试原生目录', runtime_id: runtime.id, native_catalog_ref: 'registered/native-fixture',
  provider_kind: 'NAUTILUS_CATALOG', enabled: true, revision, created_at: at, updated_at: at };
const grant = { id: id(40), source_id: source.id, version: '1', license_reference: '人工许可证明', evidence_artifact_id: id(70),
  allowed_uses: 'RESEARCH', valid_from: '2020-01-01T00:00:00Z', valid_until: '2030-01-01T00:00:00Z',
  created_at: at, license_state: 'ACTIVE', checked_at: at };
const universe = { id: id(60), name: '原生测试 Universe', registration_state: 'NATIVE_METADATA', membership_artifact_id: id(61), instrument_definitions_artifact_id: id(62),
  calendar_ref: 'fixture-calendar', calendar_version: '1', selection_asof: at, has_historical_membership: false,
  coverage_start: '2026-01-01T00:00:00Z', coverage_end: at, created_at: at };
const dataset = { id: id(50), source_id: source.id, data_use_grant_id: grant.id, native_snapshot_ref: 'immutable-fixture-snapshot',
  storage_version: 'native-v1', universe_version_id: universe.id, schema_version: '1', data_kind: 'BAR',
  partition: 'DISCOVERY', event_start: '2026-01-01T00:00:00Z', event_end: '2026-09-09T00:00:00Z', available_through: at,
  row_count: '100', timezone: 'UTC', quality_artifact_id: id(51), pit_status: 'UNVERIFIED', revision_policy: 'UNKNOWN',
  origin: 'FIXTURE', created_at: at, native_metadata_artifact_id: id(52), registration_observed_at: at,
  source_enabled: true, runtime_enabled: true, license_state: 'ACTIVE', checked_at: at };
type Seen = { path: string; key: string | undefined; body: Record<string, unknown> };

async function setup(page: Page, options: { loseFirstSource?: boolean; malformedSources?: boolean } = {}) {
  await fixture(page);
  const commands: Seen[] = [];
  let created = false;
  await page.route('**/api/v2/integrations/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON() as Record<string, unknown>;
      commands.push({ path, key: route.request().headers()['idempotency-key'], body });
      return reply(route, { schema_version: 1, resource: { ...runtime, configuration: body.configuration }, replayed: false }, 201);
    }
    if (path === '/api/v2/integrations/runtimes') return reply(route, { schema_version: 1, items: [runtime], next_cursor: null });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(route, runtime);
    if (path.endsWith('/readiness')) return reply(route, { schema_version: 1, runtime_id: runtime.id, integration_revision: revision,
      state: 'NOT_CHECKED', latest_observation: null, available_job_kinds: [] });
    if (path === '/api/v2/integrations/downstreams') return reply(route, { schema_version: 1, items: [], next_cursor: null });
    return reply(route, problem('NOT_FOUND', 404, '未登记的展示测试入口'), 404);
  });
  await page.route('**/api/v2/settings/credentials', async route => {
    const body = route.request().postDataJSON() as { intent: { purpose: string; label: string }; value: string };
    commands.push({ path: '/api/v2/settings/credentials', key: route.request().headers()['idempotency-key'], body });
    return reply(route, { schema_version: 1, resource: { id: id(80), purpose: body.intent.purpose, label: body.intent.label, created_at: at }, replayed: false }, 201);
  });
  await page.route('**/api/v2/data/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON() as Record<string, unknown>;
      commands.push({ path, key: route.request().headers()['idempotency-key'], body });
      if (path === '/api/v2/data/sources') {
        if (options.loseFirstSource && !created) { created = true; return route.abort('failed'); }
        return reply(route, { schema_version: 1, resource: { ...source, name: body.name, native_catalog_ref: body.native_catalog_ref }, replayed: created }, 201);
      }
      if (path === '/api/v2/data/revisions') return reply(route, { schema_version: 1, resource: dataset, replayed: false });
      return reply(route, problem('NOT_FOUND', 404, '未登记的展示测试写入口'), 404);
    }
    if (path === '/api/v2/data/sources') return reply(route, options.malformedSources ? { wrong: 'not an empty result' } : { schema_version: 1, items: [source], next_cursor: null });
    if (path === `/api/v2/data/sources/${source.id}`) return reply(route, source);
    if (path === `/api/v2/data/sources/${source.id}/grants`) return reply(route, { schema_version: 1, items: [grant], next_cursor: null });
    if (path === '/api/v2/data/revisions') return reply(route, { schema_version: 1, items: [dataset], next_cursor: null });
    if (path === `/api/v2/data/revisions/${dataset.id}`) return reply(route, dataset);
    if (path === '/api/v2/data/universes') return reply(route, { schema_version: 1, items: [universe], next_cursor: null });
    if (path.endsWith('/revocations')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    return reply(route, problem('NOT_FOUND', 404, '未登记的展示测试入口'), 404);
  });
  return commands;
}
async function openData(page: Page) {
  await page.goto('/'); await navigate(page, '设置');
  await page.getByRole('tab', { name: '数据与许可', exact: true }).click();
}

test('native registration keeps selected references and exact bigint revisions', async ({ page }) => {
  const commands = await setup(page); await openData(page);
  await page.getByRole('button', { name: '查看许可与版本登记' }).click();
  await page.getByRole('button', { name: '登记原生数据版本' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('combobox', { name: '选择当前有效的授权' }).click();
  await page.getByText('版本 1 · 人工许可证明 · 当前有效', { exact: true }).click();
  await dialog.getByLabel('原生存储版本', { exact: true }).fill('native-v1');
  await dialog.getByRole('button', { name: '读取真实元数据并登记' }).click();
  await expect(dialog).toBeHidden();
  const request = commands.find(command => command.path === '/api/v2/data/revisions');
  expect(request?.body).toEqual({ schema_version: 1, source_id: source.id, grant_id: grant.id,
    expected_source_revision: revision, expected_runtime_revision: revision,
    native_storage_version: 'native-v1', existing_universe_version_id: null });
  expect(request?.key).toBeTruthy();
  await page.getByRole('tab', { name: '已登记数据版本', exact: true }).click();
  await page.getByRole('button', { name: '查看版本证据' }).click();
  await expect(page.getByRole('dialog')).toContainText('PIT UNVERIFIED');
  await expect(page.getByRole('dialog')).toContainText('测试数据');
  await expect(page.getByRole('dialog')).toContainText('100');
  await page.getByRole('dialog').getByRole('button', { name: '关闭', exact: true }).last().click();
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
});

for (const preference of ['reduce', 'no-preference'] as const) {
  test(`native mouse selection remains usable across motion preference changes from ${preference}`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: preference });
    const commands = await setup(page); await openData(page);
    await page.getByRole('button', { name: '登记数据源', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('数据源名称').fill('原生动画偏好保持表单');
    await dialog.getByLabel('Runtime 原生目录登记键').fill('registered/motion');
    const select = dialog.getByRole('combobox', { name: '选择已登记的 Runtime' });
    for (const reducedMotion of [preference, preference === 'reduce' ? 'no-preference' : 'reduce'] as const) {
      await page.emulateMedia({ reducedMotion });
      await select.click();
      const popup = page.locator('.ant-select-dropdown:visible');
      const option = popup.getByText(runtime.configuration.name, { exact: true });
      await expect(option).toBeInViewport();
      await option.click();
      await expect(dialog.getByLabel('数据源名称')).toHaveValue('原生动画偏好保持表单');
      await expect(dialog.getByLabel('Runtime 原生目录登记键')).toHaveValue('registered/motion');
    }
    await dialog.getByRole('button', { name: '登记', exact: true }).click();
    await expect(dialog).toBeHidden();
    const writes = commands.filter(command => command.path === '/api/v2/data/sources');
    expect(writes).toHaveLength(1);
    expect(writes[0]?.body).toMatchObject({ runtime_id: runtime.id, native_catalog_ref: 'registered/motion' });
  });
}

test('native key validation rejects URL and traversal before sending a source', async ({ page }) => {
  const commands = await setup(page); await openData(page);
  await page.getByRole('button', { name: '登记数据源', exact: true }).click();
  const dialog = page.getByRole('dialog');
  const key = dialog.getByLabel('Runtime 原生目录登记键');
  for (const value of ['https://host/catalog', '../catalog', 'a//b']) {
    await key.fill(value); await key.blur();
    await dialog.getByRole('button', { name: '登记', exact: true }).click();
    await expect(dialog.getByText('登记键不得包含 URL、空目录段、点路径、查询标记或首尾空白，最多 512 个字符。')).toBeVisible();
    expect(commands.filter(command => command.path === '/api/v2/data/sources')).toHaveLength(0);
  }
});

test('historical Universe records are not presented as native registrations', async ({ page }) => {
  await setup(page);
  await page.route('**/api/v2/data/universes*', async route => reply(route, { schema_version: 1, next_cursor: null,
    items: [universe, { ...universe, id: id(63), name: '尚未核验的历史资产集', registration_state: 'LEGACY_UNVERIFIED' }] }));
  await openData(page);
  await page.getByRole('tab', { name: 'Universe 版本', exact: true }).click();
  await expect(page.getByRole('row').filter({ hasText: universe.name })).toContainText('有原生登记证据');
  await expect(page.getByRole('row').filter({ hasText: '尚未核验的历史资产集' })).toContainText('历史记录未核验');
  await expect(page.getByText('有原生登记证据只表示版本来源可追溯，不等于真实市场数据、PIT 已验证或研究合格。历史记录未核验时不会补造证据。')).toBeVisible();
});

test('lost source ACK reuses its exact request and idempotency key', async ({ page }) => {
  const commands = await setup(page, { loseFirstSource: true }); await openData(page);
  await page.getByRole('button', { name: '登记数据源', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('数据源名称').fill('不可变目录登记');
  await dialog.getByRole('combobox', { name: '选择已登记的 Runtime' }).click();
  await page.getByText(runtime.configuration.name, { exact: true }).click();
  await dialog.getByLabel('Runtime 原生目录登记键').fill('registered/new-native');
  await dialog.getByRole('button', { name: '登记', exact: true }).click();
  await expect.poll(() => commands.filter(command => command.path === '/api/v2/data/sources').length).toBe(1);
  await expect(dialog.getByRole('alert')).toBeVisible();
  await dialog.getByRole('button', { name: '登记', exact: true }).click();
  await expect(dialog).toBeHidden();
  const writes = commands.filter(command => command.path === '/api/v2/data/sources');
  expect(writes).toHaveLength(2);
  const [first, replay] = writes;
  if (!first || !replay) throw new Error('Expected both original and replayed data-source requests');
  expect(first.key).toBeTruthy(); expect(first.key).toBe(replay.key);
  expect(first.body).toEqual(replay.body);
  expect(first.body).not.toHaveProperty('origin');
});

test('native resource selection inherits the pending Form state without changing the submitted reference', async ({ page }) => {
  const commands = await setup(page);
  let release!: () => void;
  let entered!: () => void;
  const waiting = new Promise<void>(resolve => { release = resolve; });
  const sent = new Promise<void>(resolve => { entered = resolve; });
  await page.route('**/api/v2/data/sources', async route => {
    if (route.request().method() === 'POST') { entered(); await waiting; }
    await route.fallback();
  });
  try {
    await openData(page);
    await page.getByRole('button', { name: '登记数据源', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('数据源名称').fill('等待真实回执');
    const selector = dialog.getByRole('combobox', { name: '选择已登记的 Runtime' });
    await selector.click();
    await page.getByText(runtime.configuration.name, { exact: true }).click();
    await dialog.getByLabel('Runtime 原生目录登记键').fill('registered/pending');
    await dialog.getByRole('button', { name: '登记', exact: true }).click();
    await sent;
    await expect(selector).toBeDisabled();
    await expect(dialog.getByRole('button', { name: '刷新选项', exact: true })).toBeDisabled();
    release();
    await expect(dialog).toBeHidden();
    const writes = commands.filter(command => command.path === '/api/v2/data/sources');
    expect(writes).toHaveLength(1);
    expect(writes[0]?.body).toMatchObject({ runtime_id: runtime.id, native_catalog_ref: 'registered/pending' });
  } finally { release(); }
});

test('malformed successful data response cannot masquerade as an empty result', async ({ page }) => {
  await setup(page, { malformedSources: true }); await openData(page);
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByText('还没有数据源。先在集成设置登记 Runtime，再使用其目录登记键创建数据源。')).toBeHidden();
  await expect(page.getByRole('button', { name: '查看许可与版本登记' })).toHaveCount(0);
});

test('write-only Runtime credentials leave only references in saved configuration', async ({ page }) => {
  const commands = await setup(page); await page.goto('/'); await navigate(page, '设置');
  await page.getByRole('tab', { name: '原生集成', exact: true }).click();
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('名称', { exact: true }).fill('新的原生 Runtime');
  await dialog.getByLabel('Runtime HTTPS origin').fill('https://new-runtime.example');
  const secret = randomBytes(32).toString('hex');
  await dialog.getByLabel('新的 RUNTIME 凭据').fill('too-short');
  await expect(dialog.getByRole('button', { name: '登记凭据', exact: true })).toBeDisabled();
  await dialog.getByLabel('新的 RUNTIME 凭据').fill(secret);
  await dialog.getByRole('button', { name: '登记凭据', exact: true }).click();
  await expect.poll(async () => (await dialog.getByLabel('新的 RUNTIME 凭据').inputValue()).length).toBe(0);
  await expect(dialog).toContainText('新凭据已登记');
  const persisted = await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage }));
  expect(persisted.includes(secret), 'write-only credential must not enter browser storage').toBe(false);
  await dialog.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(dialog).toBeHidden();
  const request = commands.find(command => command.path === '/api/v2/integrations/runtimes');
  expect(request?.body).toMatchObject({ schema_version: 1, credential_ref: id(80), ca_certificate_ref: null,
    configuration: { tls_policy: 'SYSTEM_CA', development_http: false } });
  expect(JSON.stringify(request?.body).includes(secret), 'Runtime configuration must contain references only').toBe(false);
});
