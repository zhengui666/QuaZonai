// Controlled presentation fixtures; native authorization and mapping are tested in PostgreSQL.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fixture, id, navigate, problem, reply, settingsCategory } from './fixtures';
import type { Schema } from '../src/api';
const report: Schema['HistoricalImportReportV1'] = { schema_version: 1, id: id(201), export_ref: id(202), source_installation_id: id(203), dry_run: false, projected_rows: '9007199254740993', new_rows: '1', existing_rows: '9007199254740992', checked_relationships: '1', unverified_relationships: ['public.events:old_relation'], manual_review_required: true };
const source: Schema['HistoricalRowExportV1'] = { schema_version: 1, source_installation_id: report.source_installation_id, missing_tables: ['jobs'],
  inspection: { schema_version: 1, source_schema_version: '0029_portfolio_candidate_exposure', inspected_at: '2026-09-15T00:00:00Z', tables: [], foreign_keys: [] },
  tables: [{ table: 'events', source_rows: '9007199254740993', projected_rows: '9007199254740993', object_ref: id(204), byte_count: '1024', columns: ['id','kind'], excluded_columns: [{ column: 'payload', reason: 'UNREVIEWED_FIELDS' }], unsupported_schema: false }] };
const mapping: Schema['HistoricalMappingViewV1'] = { id: id(205), key: { source_installation_id: report.source_installation_id, source_table: 'events', values: { id: '9007199254740993' } }, first_import_id: id(206), disposition: 'READ_ONLY_HISTORY', created_at: '2026-09-15T00:00:00Z' };
type Write = { body: Record<string, unknown>; key: string | undefined };
async function setup(page: Page, options: { lose?: boolean; rejectRetry?: boolean; mismatch?: boolean } = {}) {
  await fixture(page); const writes: Write[] = [];
  await page.route('**/api/v2/migrations/**', async route => {
    const url = new URL(route.request().url());
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON() as Record<string, unknown>;
      writes.push({ body, key: route.request().headers()['idempotency-key'] });
      if (options.lose && writes.length === 1) return route.abort('failed');
      if (options.rejectRetry && writes.length === 2) return reply(route, problem('FORBIDDEN', 403), 403);
      return reply(route, { schema_version: 1, replayed: writes.length > 1, resource: { ...report, export_ref: body.export_ref, dry_run: body.dry_run } }, 202);
    }
    if (url.pathname.endsWith('/source')) return reply(route, options.mismatch ? { ...source, source_installation_id: id(299) } : source);
    if (url.pathname.endsWith('/mappings')) return reply(route, { schema_version: 1, items: [{ ...mapping, ...(url.searchParams.has('cursor') ? { id: id(207), key: { ...mapping.key, values: { account: '11111111-1111-4111-8111-111111111111', revision: '9007199254740994' } } } : {}) }], next_cursor: url.searchParams.has('cursor') ? null : mapping.id });
    if (url.pathname.endsWith(report.id)) return reply(route, report);
    return reply(route, { schema_version: 1, items: [report], next_cursor: null });
  });
  await page.goto('/'); await navigate(page, '设置'); await settingsCategory(page, '迁移');
  return writes;
}
test('historical source counts exclusions and original keys stay exact across mapping pages', async ({ page }) => {
  await setup(page);
  await page.getByRole('button', { name: report.id, exact: true }).click();
  const drawer = page.getByRole('dialog');
  await expect(drawer.getByText('9007199254740993 / 1 / 9007199254740992', { exact: true })).toBeVisible();
  await expect(drawer.getByText('payload：UNREVIEWED_FIELDS', { exact: true })).toBeVisible();
  await expect(drawer.getByText('{"id":"9007199254740993"}', { exact: true })).toBeVisible();
  await drawer.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(drawer.getByText('{"account":"11111111-1111-4111-8111-111111111111","revision":"9007199254740994"}', { exact: true })).toBeVisible();
  await expect(drawer.getByRole('button', { name: '下一页', exact: true })).toBeDisabled();
  await drawer.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(drawer.getByText('{"id":"9007199254740993"}', { exact: true })).toBeVisible();
  const audit = await new AxeBuilder({ page }).include('.ant-drawer-body').analyze();
  expect(audit.violations.filter(v => ['serious','critical'].includes(v.impact ?? ''))).toEqual([]);
});
for (const rejectRetry of [false,true]) test(`unknown import preserves original input and idempotency after retry: ${rejectRetry}`, async ({ page }) => {
  const writes = await setup(page, { lose: true, rejectRetry });
  await page.getByRole('button', { name: '导入历史投影', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('已登记的导出编号').fill(report.export_ref);
  await expect(dialog.getByRole('checkbox')).toBeChecked();
  await dialog.getByRole('button', { name: '提交导入请求', exact: true }).click();
  await expect(dialog.getByText('提交结果未知，请重试当前操作', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('已登记的导出编号')).toBeDisabled();
  await expect(dialog.getByRole('checkbox')).toBeDisabled();
  await dialog.getByRole('button', { name: '重试同一导入请求', exact: true }).click();
  if (rejectRetry) {
    await expect(dialog.getByText(/FORBIDDEN/)).toBeVisible();
    await expect(page.getByRole('dialog', { name: '重新验证敏感操作' })).toHaveCount(0);
    await expect(page.getByLabel('动态验证码')).toHaveCount(0);
    await expect(dialog.getByLabel('已登记的导出编号')).toBeDisabled();
    await dialog.getByRole('button', { name: '重试同一导入请求', exact: true }).click();
  }
  await expect(dialog.getByText('试运行报告已保存', { exact: true })).toBeVisible();
  expect(writes.length).toBe(rejectRetry ? 3 : 2);
  expect(writes[0]?.key).toBeTruthy();
  expect(writes.every(w => w.key === writes[0]?.key && JSON.stringify(w.body) === JSON.stringify(writes[0]?.body))).toBe(true);
  expect(writes[0]?.body).toEqual({ schema_version: 1, export_ref: report.export_ref, dry_run: true });
});
test('mismatched source installation cannot expose unrelated mapping detail', async ({ page }) => {
  await setup(page, { mismatch: true });
  await page.getByRole('button', { name: report.id, exact: true }).click();
  const drawer = page.getByRole('dialog');
  await expect(drawer.getByRole('alert')).toBeVisible();
  await expect(drawer.getByText('原身份映射', { exact: true })).toHaveCount(0);
  await expect(drawer.getByText('payload：UNREVIEWED_FIELDS', { exact: true })).toHaveCount(0);
});

test('field viewer preserves Unicode pages and distinguishes SQL NULL from empty text', async ({ page }) => {
  await setup(page);
  const first = '汉🙂'.repeat(8192); const last = 'e\u0301\n<script>plain text</script>';
  const total = String(Array.from(first + last).length);
  const fields = [{ name: 'answer_text', character_count: total }, { name: 'nullable', character_count: null }, { name: 'empty', character_count: '0' }];
  await page.route('**/api/v2/migrations/reports/*/records/*/fields', route => reply(route, { schema_version: 1, report_id: report.id, record_id: mapping.id, fields }));
  await page.route('**/api/v2/migrations/reports/*/records/*/field?*', route => {
    const query = new URL(route.request().url()).searchParams; const name = query.get('name'); const offset = query.get('offset') ?? '0';
    const selected = fields.find(f => f.name === name)!;
    return reply(route, { schema_version: 1, report_id: report.id, record_id: mapping.id, name, offset, total_characters: selected.character_count,
      text: name === 'nullable' ? null : name === 'empty' ? '' : offset === '0' ? first : last, next_offset: name === 'answer_text' && offset === '0' ? '16384' : null });
  });
  await page.getByRole('button', { name: report.id, exact: true }).click();
  const drawer = page.getByRole('dialog');
  await drawer.getByRole('button', { name: '展开行', exact: true }).click();
  await drawer.getByRole('button', { name: '查看字段 answer_text', exact: true }).click();
  await expect(drawer.getByLabel('原字段内容')).toHaveText(first);
  await drawer.getByRole('button', { name: '下一页', exact: true }).first().click();
  await expect(drawer.getByLabel('原字段内容')).toHaveText(last);
  await expect(drawer.locator('pre script')).toHaveCount(0);
  await drawer.getByRole('button', { name: '查看字段 nullable', exact: true }).click();
  await expect(drawer.getByText('SQL NULL（无值）', { exact: true })).toBeVisible();
  await drawer.getByRole('button', { name: '查看字段 empty', exact: true }).click();
  await expect(drawer.getByText('空字符串（0字符）', { exact: true })).toBeVisible();
});

for (const dry of [false, true]) test(`historical artifact coverage selection and original binary download: dry=${dry}`, async ({ page }) => {
  await setup(page);
  const bytes = Buffer.from([0,255,128,10]);
  const item: Schema['HistoricalArtifactResultV1'] = { id: id(221), report_id: report.id, identity: { kind: 'ARTIFACT', source_table: 'mission_artifacts', source_id: '11111111-1111-4111-8111-111111111111' }, record_id: dry ? null : id(222), source_outcome: 'COPIED', verified_readable: true, stored: !dry, byte_count: '4' };
  await page.route(`**/api/v2/migrations/reports/${report.id}`, route => reply(route, { ...report, dry_run: dry }));
  await page.route('**/api/v2/migrations/reports/*/artifacts/summary', route => reply(route, { schema_version: 1, report_id: report.id, source_records: '3', projected_records: '3', selected_records: '2', readable_records: '1', stored_records: dry ? '0' : '1' }));
  await page.route('**/api/v2/migrations/reports/*/artifacts?*', route => {
    const next = new URL(route.request().url()).searchParams.has('cursor');
    return reply(route, { schema_version: 1, items: next ? [{ ...item, id: id(225), record_id: dry ? null : id(226), source_outcome: null, verified_readable: false, stored: false, byte_count: null }] : [item, { ...item, id: id(223), record_id: dry ? null : id(224), source_outcome: 'SEALED_RETAINED', verified_readable: false, stored: false, byte_count: null }], next_cursor: next ? null : id(223) });
  });
  let reads = 0;
  await page.route('**/api/v2/migrations/reports/*/artifacts/*/content', route => { reads++; return route.fulfill({ status: 200, contentType: 'application/octet-stream', body: bytes }); });
  await page.getByRole('button', { name: report.id, exact: true }).click();
  await page.getByText('历史附件与覆盖情况', { exact: true }).click();
  const section = page.getByRole('region', { name: '历史附件', exact: true });
  await expect(section.getByText('3 / 3 / 2', { exact: true })).toBeVisible();
  await expect(section.getByText('保留密封，不读取', { exact: true })).toBeVisible();
  const buttons = section.getByRole('button', { name: /^下载副本/ });
  await expect(buttons.nth(1)).toBeDisabled();
  if (dry) await expect(buttons.first()).toBeDisabled();
  else {
    const download = page.waitForEvent('download'); await buttons.first().click(); const file = await download;
    expect(file.suggestedFilename()).toBe(`${id(222)}.bin`);
    const stream = await file.createReadStream(); const chunks: Buffer[] = []; for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
    expect(Buffer.concat(chunks)).toEqual(bytes);
  }
  expect(reads).toBe(dry ? 0 : 1);
  await section.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(section.getByText('未选择', { exact: true })).toBeVisible();
  await expect(section.getByRole('button', { name: /^下载副本/ })).toBeDisabled();
  const audit = await new AxeBuilder({ page }).include('.ant-drawer-body').analyze();
  expect(audit.violations.filter(v => ['serious','critical'].includes(v.impact ?? ''))).toEqual([]);
});
