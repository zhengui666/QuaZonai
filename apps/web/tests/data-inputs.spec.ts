import { expect, test } from '@playwright/test';
import type { Locator, Page } from '@playwright/test';
import type { Schema } from '../src/api';
import { readFileSync } from 'node:fs';
const capabilities: Schema['RuntimeCapabilitiesV1'] = JSON.parse(readFileSync(new URL('../../../tests/contracts/runtime-capabilities.fixture.json', import.meta.url), 'utf8'));

// Synthetic browser interaction/transport fixtures only. No terminal Run,
// scientific quality artifact or native execution is fabricated here. Real
// freeze/admission/replay acceptance belongs to the separate native harness.
const id = (suffix: number) => `01990000-0000-7000-8000-${String(suffix).padStart(12, '0')}`;
const cutoff = '2026-01-01T00:00:00.000001Z';
async function setup(page: Page) {
  const now = new Date().toISOString();
  const project: Schema['ProjectView'] = { id: id(1), name: '输入交互测试项目', description: 'Synthetic browser fixture',
    revision: '1', state: 'DRAFT', created_by: 'OPERATOR', root_lineage_id: id(1), current_brief_id: null,
    created_at: now, updated_at: now };
  const source: Schema['DataSourceView'] = { id: id(2), name: '输入交互测试来源', runtime_id: id(3),
    revision: '9007199254740993', enabled: true, native_catalog_ref: 'fixture/catalog', provider_kind: 'NAUTILUS_CATALOG', created_at: now, updated_at: now };
  const runtime: Schema['RuntimeView'] = { id: id(3), revision: '9007199254740993', protocol_version: 1,
    configuration: { name: '输入交互测试 Runtime', endpoint: 'https://fixture.invalid', tls_policy: 'SYSTEM_CA',
      enabled: true, development_http: false, allowed_capabilities: ['DATA_VALIDATE'] },
    ca_configured: false, credential_configured: true, created_at: now, updated_at: now };
  const dataset: Schema['DatasetView'] = { id: id(4), source_id: source.id, data_use_grant_id: id(5),
    universe_version_id: id(6), native_snapshot_ref: 'fixture-original-snapshot', storage_version: '9007199254740993',
    schema_version: '1', data_kind: 'BAR', partition: 'DISCOVERY', origin: 'FIXTURE', pit_status: 'UNVERIFIED',
    revision_policy: 'UNKNOWN', row_count: '9007199254740993', available_through: cutoff,
    event_start: '2025-01-01T00:00:00Z', event_end: '2025-12-31T00:00:00Z', timezone: 'UTC',
    quality_artifact_id: id(7), native_metadata_artifact_id: id(8), created_at: now, checked_at: now,
    license_state: 'ACTIVE', source_enabled: true, runtime_enabled: true };
  const input: Schema['InputSetView'] = { header: { id: id(9), project_id: project.id, purpose: 'DISCOVERY', revision: '1',
    decision_cutoff: cutoff, frozen_at: now, created_at: now }, items: [{ id: id(10), ordinal: 0,
      item: { kind: 'DATASET', dataset_revision_id: dataset.id, role: 'DISCOVERY' }, origin: 'FIXTURE', pit_status: 'UNVERIFIED' }] };
  const state = { writes: [] as { path: string; key?: string; body: unknown }[], hold: undefined as Promise<void> | undefined,
    stale: false, rejectRevision: false, sourceError: false, runtime, input, project, dataset };
  await page.route('**/api/**', async route => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    const pageOf = (items: unknown[]) => reply({ schema_version: 1, items, next_cursor: null });
    const problem = (code: string, status: number) => route.fulfill({ status, contentType: 'application/problem+json', body: JSON.stringify({
      type: `urn:quazonai:problem:${code.toLowerCase().replaceAll('_', '-')}`, title: code, code, status,
      detail: 'Fixture requires explicit reload', request_id: id(90), retryable: false, current_revision: runtime.revision,
      field_errors: [], safe_next_actions: [],
    }) });
    if (!['GET', 'HEAD'].includes(request.method())) {
      state.writes.push({ path, key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      if (state.hold) await state.hold;
      if (state.rejectRevision) return problem('REVISION_CONFLICT', 409);
      return route.abort('failed');
    }
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') return pageOf([project, { ...project, id: id(11), name: '另一个项目', root_lineage_id: id(11) }]);
    if (path === '/api/v2/data/sources') return pageOf([source]);
    if (path === `/api/v2/data/sources/${source.id}`) return state.sourceError ? route.abort('failed') : reply(source);
    if (path === '/api/v2/integrations/runtimes') return pageOf([runtime]);
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(runtime);
    if (path === `/api/v2/integrations/runtimes/${runtime.id}/readiness`) return reply({ schema_version: 1,
      runtime_id: runtime.id, integration_revision: runtime.revision, state: 'AVAILABLE', available_job_kinds: ['DATA_VALIDATE'],
      latest_observation: { id: id(12), runtime_id: runtime.id, integration_revision: runtime.revision, observed_at: now,
        valid_until: new Date(Date.now() + (state.stale ? -60000 : 60000)).toISOString(), snapshot_artifact_id: id(13),
        outcome: { status: 'AVAILABLE', capabilities: { ...capabilities, checked_at: now } } },
    });
    if (path === '/api/v2/data/revisions') return pageOf(url.searchParams.get('partition') === 'VALIDATION' ? [] : [dataset]);
    if (path === `/api/v2/data/revisions/${dataset.id}`) return reply(dataset);
    if (path === '/api/v2/input-sets') return pageOf(url.searchParams.get('project_id') === project.id
      ? [input.header, { ...input.header, id: id(14), purpose: 'SEALED' }] : []);
    if (path === `/api/v2/input-sets/${input.header.id}`) return reply(input);
    if (path === `/api/v2/input-sets/${id(14)}`) return reply({ ...input, header: { ...input.header, id: id(14), purpose: 'SEALED' }, items: input.items.map(entry => ({ ...entry, item: { ...entry.item, role: 'SEALED' } })) });
    if (['/api/v2/runs', '/api/v2/artifacts', '/api/v2/data/universes'].includes(path)) return pageOf([]);
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
  await choose(page, page.getByLabel('冻结输入所属研究项目', { exact: true }), `${project.name} · ${project.id}`);
  await expect(page.getByRole('button', { name: input.header.id, exact: true })).toBeVisible();
  return state;
}
async function choose(page: Page, locator: Locator, label: string) {
  await expect(locator).toBeEnabled(); await locator.click(); await page.getByTitle(label, { exact: true }).last().click();
}
async function create(page: Page, state: Awaited<ReturnType<typeof setup>>) {
  await page.getByRole('button', { name: '新建冻结输入', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '创建并冻结项目输入', exact: true });
  await choose(page, editor.getByLabel('选择冻结输入的 Runtime', { exact: true }), `${state.runtime.configuration.name} · ${state.runtime.id}`);
  await editor.getByLabel('决策截止（精确 UTC）', { exact: true }).fill(cutoff);
  await editor.getByRole('row').filter({ hasText: state.dataset.id }).getByRole('checkbox').check();
  return editor;
}
async function validate(page: Page, state: Awaited<ReturnType<typeof setup>>) {
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await page.getByRole('button', { name: '请求数据质量验证', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '单独请求 DATA_VALIDATE', exact: true });
  await choose(page, editor.getByLabel('确认实际数据 Runtime', { exact: true }), state.runtime.id);
  return editor;
}

test('unknown create outcome locks exact cutoff, body and key; pending/dismissal/double click are guarded', async ({ page }) => {
  const state = await setup(page); const editor = await create(page, state);
  let release!: () => void; state.hold = new Promise<void>(resolve => { release = resolve; });
  await editor.getByRole('button', { name: '确认创建并冻结', exact: true }).dblclick();
  await expect.poll(() => state.writes.length).toBe(1);
  await expect(editor.getByLabel('决策截止（精确 UTC）', { exact: true })).toBeDisabled();
  await editor.getByRole('button', { name: '返回', exact: true }).click(); await expect(editor).toBeVisible();
  release(); state.hold = undefined;
  await expect(editor.getByText(/提交结果未知：原请求与幂等键已保留/)).toBeVisible();
  await editor.getByRole('button', { name: '返回', exact: true }).click();
  await page.getByRole('button', { name: '继续编辑', exact: true }).click();
  await editor.getByRole('button', { name: '原样重试创建请求', exact: true }).click();
  await expect.poll(() => state.writes.length).toBe(2);
  expect(state.writes[1]).toEqual(state.writes[0]);
  expect(state.writes[0]?.body).toMatchObject({ decision_cutoff: cutoff, project_id: state.project.id, items: [{ dataset_revision_id: state.dataset.id }] });
});

test('purpose clears dependent datasets, project changes unmount details, and Sealed is metadata-only', async ({ page }) => {
  const state = await setup(page); const editor = await create(page, state);
  await choose(page, editor.getByLabel('冻结用途', { exact: true }), 'VALIDATION');
  await expect(editor.getByText('已选择 0 / 255 个数据版本')).toBeVisible();
  await editor.getByRole('button', { name: '返回', exact: true }).click(); await page.getByRole('button', { name: '确认放弃', exact: true }).click();
  await page.getByRole('button', { name: id(14), exact: true }).click();
  await expect(page.getByRole('button', { name: '请求数据质量验证', exact: true })).toBeDisabled();
  await expect(page.getByText('仅 DISCOVERY / VALIDATION 可请求独立数据验证')).toBeVisible();
  await choose(page, page.getByLabel('冻结输入所属研究项目', { exact: true }), `另一个项目 · ${id(11)}`);
  await expect(page.getByText('原始冻结输入详情', { exact: true })).toHaveCount(0);
  expect(state.writes).toHaveLength(0);
});

test('Runtime expiry blocks validation, reload is explicit and FIXTURE stays visibly unqualified', async ({ page }) => {
  const state = await setup(page); state.stale = true;
  const editor = await validate(page, state);
  await expect(editor.getByText('Runtime 探测已过期，请到集成重新探测')).toBeVisible();
  await expect(editor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeDisabled();
  state.stale = false;
  await editor.getByRole('button', { name: '重载 Runtime 与原始数据绑定', exact: true }).click();
  await expect(editor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeEnabled();
  await expect(page.getByText('该版本未构成合格真实 PIT 证据；结构验证成功也不会改变其资格。')).toBeVisible();
  expect(state.writes).toHaveLength(0);
});

test('unknown validation preserves original Runtime revision, exact counters and one request identity', async ({ page }) => {
  const state = await setup(page); const editor = await validate(page, state);
  await expect(editor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeEnabled();
  await editor.getByRole('button', { name: '确认排队数据验证', exact: true }).click();
  await expect(editor.getByText(/提交结果未知：原请求和幂等键已锁定/)).toBeVisible();
  state.runtime.revision = '9007199254740994'; state.stale = true;
  await expect(editor.getByLabel('CPU 总秒数（精确整数）', { exact: true })).toBeDisabled();
  await editor.getByRole('button', { name: '原样重试验证请求', exact: true }).click();
  await expect.poll(() => state.writes.length).toBe(2);
  expect(state.writes[1]).toEqual(state.writes[0]);
  expect(state.writes[0]?.body).toMatchObject({ expected_runtime_revision: '9007199254740993', input_set_id: state.input.header.id,
    limits: { cpu_seconds: '60', output_bytes: '1048576', experiments: 0 } });
});

test('confirmed stale revision blocks retry until explicit reload and reconfirmation', async ({ page }) => {
  const state = await setup(page); state.rejectRevision = true;
  const editor = await validate(page, state);
  await editor.getByRole('button', { name: '确认排队数据验证', exact: true }).click();
  await expect(editor.getByRole('button', { name: '原样重试验证请求', exact: true })).toBeDisabled();
  await editor.getByRole('button', { name: '重新载入后重新确认', exact: true }).click();
  await expect(editor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeDisabled();
  await choose(page, editor.getByLabel('确认实际数据 Runtime', { exact: true }), state.runtime.id);
  await expect(editor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeEnabled();
  expect(state.writes).toHaveLength(1);
});

test('offline submission and narrow editor retain all selections without horizontal page overflow', async ({ page, context }) => {
  const state = await setup(page); const editor = await create(page, state);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(391);
  await context.setOffline(true);
  await expect(editor.getByRole('button', { name: '确认创建并冻结', exact: true })).toBeDisabled();
  await expect(editor.getByLabel('决策截止（精确 UTC）', { exact: true })).toHaveValue(cutoff);
  await context.setOffline(false);
  await expect(editor.getByRole('button', { name: '确认创建并冻结', exact: true })).toBeEnabled();
  expect(state.writes).toHaveLength(0);
});
