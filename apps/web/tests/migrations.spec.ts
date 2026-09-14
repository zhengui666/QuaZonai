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
      if (options.rejectRetry && writes.length === 2) return reply(route, problem('RECENT_AUTH_REQUIRED', 403), 403);
      return reply(route, { schema_version: 1, replayed: writes.length > 1, resource: { ...report, export_ref: body.export_ref, dry_run: body.dry_run } }, 202);
    }
    if (url.pathname.endsWith('/source')) return reply(route, options.mismatch ? { ...source, source_installation_id: id(299) } : source);
    if (url.pathname.endsWith('/mappings')) return reply(route, { schema_version: 1, items: [{ ...mapping, ...(url.searchParams.has('cursor') ? { id: id(207), key: { ...mapping.key, values: { account: '11111111-1111-4111-8111-111111111111', revision: '9007199254740994' } } } : {}) }], next_cursor: url.searchParams.has('cursor') ? null : mapping.id });
    if (url.pathname.endsWith(report.id)) return reply(route, report);
    return reply(route, { schema_version: 1, items: [report], next_cursor: null });
  });
  await page.goto('/'); await navigate(page, '设置'); await settingsCategory(page, '历史迁移');
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
  await expect(dialog.getByText('结果尚未确认，重试保留原编号、方式和幂等键。', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('已登记的导出编号')).toBeDisabled();
  await expect(dialog.getByRole('checkbox')).toBeDisabled();
  await dialog.getByRole('button', { name: '重试同一导入请求', exact: true }).click();
  if (rejectRetry) {
    await expect(dialog.getByText(/RECENT_AUTH_REQUIRED/)).toBeVisible();
    const verify = page.getByRole('dialog', { name: '重新验证敏感操作' });
    await expect(verify).toBeVisible();
    await verify.getByLabel('动态验证码').fill('123456');
    await verify.getByRole('button', { name: '确认验证', exact: true }).click();
    await expect(verify).toBeHidden();
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
