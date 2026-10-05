import { expect, test } from '@playwright/test';
import type { Locator, Page } from '@playwright/test';
import type { Schema } from '../src/api';

// Synthetic read-only browser/transport fixtures, not scientific evidence. Real
// freeze/admission/replay and exact native bytes remain in the native harness.
const id = (suffix: number) => `01990000-0000-7000-8000-${String(suffix).padStart(12, '0')}`;
const cutoff = '2026-01-01T00:00:00.000001Z';
async function setup(page: Page) {
  const now = new Date().toISOString();
  const project: Schema['ProjectView'] = { id: id(1), name: '输入交互测试项目', description: 'Synthetic browser fixture',
    revision: '1', state: 'DRAFT', created_by: 'OPERATOR', root_lineage_id: id(1), current_brief_id: null,
    created_at: now, updated_at: now };
  const source: Schema['DataSourceView'] = { id: id(2), name: '输入交互测试来源', runtime_id: id(3),
    revision: '9007199254740993', enabled: true, native_catalog_ref: 'fixture/catalog', provider_kind: 'NAUTILUS_CATALOG', created_at: now, updated_at: now };
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
  const sealed: Schema['InputSetView'] = { header: { ...input.header, id: id(14), purpose: 'SEALED' }, items: [{
    id: id(15), ordinal: 0, item: { kind: 'ARTIFACT', artifact_id: id(16), role: 'REPORT' }, origin: 'FIXTURE',
  }] };
  const run: Schema['RunSnapshotV1'] = { schema_version: 1, id: id(20), project_id: project.id, input_set_id: input.header.id,
    kind: 'DATA_VALIDATE', state: 'RUNNING', revision: '1', current_attempt_no: 1, active_attempt_id: id(21),
    queued_at: now, started_at: now, deadline_at: new Date(Date.now() + 60_000).toISOString(), last_event_seq: '2' };
  const artifact: Schema['ArtifactView'] = { id: id(22), project_id: project.id, producer_run_id: run.id,
    producer_attempt_id: run.active_attempt_id, kind: 'REPORT', schema_name: 'synthetic.transport_bytes', schema_version: '1',
    created_by: 'RUNTIME', access_class: 'RESEARCH', origin: 'FIXTURE', byte_count: '4', media_type: 'application/octet-stream', created_at: now };
  const state = { writes: [] as string[], reads: [] as string[], sourceError: false, inputError: false,
    metadataMismatch: false, contentMismatch: false, input, sealed, project, dataset, source, run, artifact };
  await page.route('**/api/**', async route => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    const pageOf = (items: unknown[], next_cursor: string | null = null) => reply({ schema_version: 1, items, next_cursor });
    if (!['GET', 'HEAD'].includes(request.method())) {
      state.writes.push(`${request.method()} ${path}`);
      return route.abort('blockedbyclient');
    }
    state.reads.push(`${path}${url.search}`);
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') return pageOf([project, { ...project, id: id(11), name: '另一个项目', root_lineage_id: id(11) }]);
    if (path === `/api/v2/projects/${project.id}`) return reply(project);
    if (path === `/api/v2/projects/${project.id}/cycles`) return pageOf([]);
    if (path === '/api/v2/data/sources') return pageOf([source]);
    if (path === `/api/v2/data/sources/${source.id}`) return state.sourceError ? route.abort('failed') : reply(source);
    if (path === `/api/v2/data/revisions/${dataset.id}`) return reply(dataset);
    if (path === '/api/v2/input-sets') {
      if (state.inputError) return route.abort('failed');
      if (url.searchParams.get('project_id') !== project.id) return pageOf([]);
      return url.searchParams.get('cursor') ? pageOf([sealed.header]) : pageOf([input.header], id(30));
    }
    if (path === `/api/v2/input-sets/${input.header.id}`) return reply(input);
    if (path === `/api/v2/input-sets/${sealed.header.id}`) return reply(sealed);
    if (path === '/api/v2/runs') return pageOf([run, { ...run, id: id(24), input_set_id: id(25) }]);
    if (path === `/api/v2/runs/${run.id}`) return reply(run);
    if (path === `/api/v2/runs/${run.id}/events`) return route.fulfill({ contentType: 'text/event-stream', body: ': fixture\n\n' });
    if (path === '/api/v2/artifacts') return pageOf([artifact, { ...artifact, id: id(23), producer_attempt_id: id(26) }]);
    if (path === `/api/v2/artifacts/${artifact.id}`) return reply(state.metadataMismatch ? { ...artifact, producer_attempt_id: id(26) } : artifact);
    if (path === `/api/v2/artifacts/${artifact.id}/content`) return route.fulfill({ contentType: artifact.media_type, body: state.contentMismatch ? 'BAD' : 'DATA' });
    if (path === '/api/v2/data/universes') return pageOf([]);
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
  await expect(page.getByRole('button', { name: input.header.id, exact: true })).toBeVisible();
  return state;
}
async function choose(page: Page, locator: Locator, label: string) {
  await expect(locator).toBeEnabled(); await locator.click(); await page.getByTitle(label, { exact: true }).last().click();
}
async function noBusinessEditors(page: Page) {
  await expect(page.getByRole('button', { name: /新建冻结输入|请求数据质量验证|确认创建并冻结|确认排队数据验证|请求取消/ })).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: /创建并冻结项目输入|单独请求 DATA_VALIDATE/ })).toHaveCount(0);
}

test('research inputs retain exact provenance, paging and read-only run details', async ({ page }) => {
  const state = await setup(page);
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await expect(page.getByText('fixture-original-snapshot / 9007199254740993', { exact: true })).toBeVisible();
  await expect(page.getByText('该版本未构成合格真实 PIT 证据；结构验证成功也不会改变其资格。', { exact: true })).toBeVisible();
  await expect(page.getByText(cutoff, { exact: true }).first()).toBeVisible();
  await noBusinessEditors(page);
  const run = page.getByRole('row').filter({ hasText: state.run.id });
  await expect(page.getByRole('row').filter({ hasText: id(24) })).toHaveCount(0);
  await run.getByRole('button', { name: '运行详情', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '运行详情', exact: true });
  await expect(detail).toContainText(state.run.id);
  await noBusinessEditors(page);
  await page.keyboard.press('Escape');
  await expect(detail).toHaveCount(0);
  await page.getByRole('button', { name: '刷新冻结输入', exact: true }).click();
  await page.getByRole('button', { name: '下一页', exact: true }).first().click();
  await page.getByRole('button', { name: state.sealed.header.id, exact: true }).click();
  await expect(page.getByText(`产物 ID：${id(16)} · 来源：FIXTURE（这里只显示冻结身份，不读取内容）`, { exact: true })).toBeVisible();
  expect(state.reads.some(path => path.startsWith(`/api/v2/artifacts/${id(16)}`))).toBe(false);
  await page.getByRole('button', { name: '上一页', exact: true }).first().click();
  await expect(page.getByRole('button', { name: state.input.header.id, exact: true })).toBeVisible();
  expect(state.writes).toEqual([]);
});

test('shared Settings input reader changes projects without retaining another project details', async ({ page }) => {
  const state = await setup(page);
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  // Legitimate Settings source configuration remains available.
  await expect(page.getByRole('button', { name: '登记数据源', exact: true })).toBeEnabled();
  await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
  await choose(page, page.getByLabel('冻结输入所属研究项目', { exact: true }), `${state.project.name} · ${state.project.id}`);
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await expect(page.getByText('原始冻结输入详情', { exact: true })).toBeVisible();
  await noBusinessEditors(page);
  await choose(page, page.getByLabel('冻结输入所属研究项目', { exact: true }), `另一个项目 · ${id(11)}`);
  await expect(page.getByText('此项目尚无冻结输入。', { exact: true })).toBeVisible();
  await expect(page.getByText('原始冻结输入详情', { exact: true })).toHaveCount(0);
  expect(state.writes).toEqual([]);
});

test('failed provenance and stale-list reads recover without authoring fallback', async ({ page }) => {
  const state = await setup(page); state.sourceError = true;
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await expect(page.getByText('连接中断，未能读取数据；请重试', { exact: true })).toBeVisible();
  await expect(page.getByText(`数据版本：${state.dataset.id} · 元数据尚未载入`, { exact: true })).toBeVisible();
  state.sourceError = false;
  await page.getByRole('button', { name: '重新载入', exact: true }).click();
  await expect(page.getByText('fixture-original-snapshot / 9007199254740993', { exact: true })).toBeVisible();
  state.inputError = true;
  await page.getByRole('button', { name: '刷新冻结输入', exact: true }).click();
  await expect(page.getByText('数据未更新', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: state.input.header.id, exact: true })).toBeVisible();
  await noBusinessEditors(page);
  state.inputError = false;
  await page.getByRole('button', { name: '重新载入', exact: true }).click();
  await expect(page.getByText('数据未更新', { exact: true })).toHaveCount(0);
  expect(state.writes).toEqual([]);
});

test('artifact reads reject changed attempts and wrong byte counts before a successful read-only download', async ({ page }) => {
  const state = await setup(page);
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await page.getByRole('row').filter({ hasText: state.run.id }).getByRole('button', { name: '查看产物', exact: true }).click();
  const row = page.getByRole('row').filter({ hasText: state.artifact.id });
  await expect(row).toContainText(state.run.active_attempt_id!);
  await expect(page.getByRole('row').filter({ hasText: id(23) })).toHaveCount(0);
  await expect(page.getByText('当前运行尚未成功；这里的产物不作为已通过的数据质量结论。', { exact: true })).toBeVisible();
  const download = row.getByRole('button', { name: '下载原始产物', exact: true });
  state.metadataMismatch = true;
  await download.click();
  await expect(page.getByText('产物与原始运行/尝试绑定不一致', { exact: true })).toBeVisible();
  expect(state.reads.some(path => path === `/api/v2/artifacts/${state.artifact.id}/content`)).toBe(false);
  state.metadataMismatch = false; state.contentMismatch = true;
  await download.click();
  await expect(page.getByText('下载字节数与产物元数据不一致', { exact: true })).toBeVisible();
  state.contentMismatch = false;
  const downloaded = page.waitForEvent('download'); await download.click();
  const result = await downloaded;
  expect(await result.failure()).toBeNull(); expect(result.suggestedFilename()).toBe(`${state.artifact.id}.bin`);
  const stream = await result.createReadStream(); expect(stream).toBeTruthy();
  const chunks: Buffer[] = []; for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
  expect(Buffer.concat(chunks).toString()).toBe('DATA');
  await expect(download).toHaveAttribute('aria-busy', 'false');
  await noBusinessEditors(page);
  expect(state.writes).toEqual([]);
});

test('narrow offline view retains selected provenance and returns to research without an edit guard', async ({ page, context }) => {
  const state = await setup(page);
  await page.getByRole('button', { name: state.input.header.id, exact: true }).click();
  await expect(page.getByText('fixture-original-snapshot / 9007199254740993', { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(391);
  await context.setOffline(true);
  await expect(page.getByRole('button', { name: '刷新冻结输入', exact: true })).toBeDisabled();
  await expect(page.getByText('fixture-original-snapshot / 9007199254740993', { exact: true })).toBeVisible();
  await noBusinessEditors(page);
  await context.setOffline(false);
  await expect(page.getByRole('button', { name: '刷新冻结输入', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '返回研究列表', exact: true }).click();
  await expect(page.getByRole('button', { name: state.project.name, exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(state.writes).toEqual([]);
});
