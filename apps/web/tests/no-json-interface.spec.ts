import { expect, test } from '@playwright/test';
import type { Page, Route } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import type { Schema } from '../src/api';

const now = '2026-10-22T00:00:00Z';
const projectId = '01990000-0000-7000-8000-000000000101';
const reportId = '01990000-0000-7000-8000-000000000102';
const recordId = '01990000-0000-7000-8000-000000000103';
const sourceId = '01990000-0000-7000-8000-000000000104';
const policyId = '01990000-0000-7000-8000-000000000105';
const exactInteger = '9007199254740993';
const serializedField = `{"name":"保留的原字段","count":${exactInteger},"nested":{"enabled":true,"empty":null},"items":["甲","乙"]}`;
const malformedField = '{"unrendered_payload":"incomplete"';
const multipartField = '{"fragment":"' + 'x'.repeat(24) + '"}';
const ndjsonField = '{"name":"第一条"}\n{"name":"第二条"}';
const fieldValues: Record<string, string> = {
  structured: serializedField, malformed: malformedField, multipart: multipartField, ndjson: ndjsonField,
};
const project: Schema['ProjectView'] = {
  id: projectId, root_lineage_id: projectId, name: '界面验收项目', description: '',
  state: 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: now, updated_at: now,
};
const report: Schema['HistoricalImportReportV1'] = {
  schema_version: 1, id: reportId, export_ref: reportId, source_installation_id: sourceId,
  dry_run: false, projected_rows: '1', new_rows: '1', existing_rows: '0', checked_relationships: '0',
  unverified_relationships: [], manual_review_required: false,
};
const source: Schema['HistoricalRowExportV1'] = {
  schema_version: 1, source_installation_id: sourceId, missing_tables: [], tables: [],
  inspection: { schema_version: 1, inspected_at: now, source_schema_version: 'legacy-1',
    foreign_keys: [], tables: [{ table: 'research', rows: '1', primary_key: ['id'], columns: [] }] },
};
const metric: Schema['MetricRequirementV1'] = {
  schema_version: 1, metric_code: 'RETURN', scope: 'PORTFOLIO', comparator: 'GE', threshold_low: '0.0100',
  threshold_high: null, minimum_observations: exactInteger, method_allowlist: ['native-statistics'], required: true,
};
const policy: Schema['AutomationPolicyViewV1'] = {
  id: policyId, project_id: projectId, authorized_at: now, created_at: now,
  content: { degradation_metric_requirements: [metric], downstream_id: sourceId,
    enabled_for_new_rebalances: false, mandate_id: reportId, max_feedback_age_seconds: '3600',
    max_rebalances_per_day: 2, minimum_paper_elapsed_seconds: '60', mode: 'AUTO_PAPER',
    promotion_metric_requirements: [metric], required_paper_observations: 5, valid_until: '2027-01-01T00:00:00Z' },
};

function reply(route: Route, value: unknown) {
  return route.fulfill({ contentType: 'application/json', body: JSON.stringify(value) });
}
const pageOf = (items: unknown[]) => ({ schema_version: 1, items, next_cursor: null });

async function setup(page: Page, error = false) {
  await page.route('**/api/**', route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    if (path === '/api/v2/auth/status') return reply(route, { schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply(route, { schema_version: 1, authenticated_at: now, expires_at: '2027-01-01T00:00:00Z' });
    if (path === '/api/v2/settings/codex') return reply(route, pageOf([]));
    if (path === '/api/v2/projects') {
      if (error) return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
        type: 'about:blank', title: 'Unavailable', status: 422, code: 'INVALID_REQUEST', request_id: reportId,
        detail: '{"message":"服务暂时不可用","payload":{"internal":true}}', retryable: true, safe_next_actions: [],
        field_errors: [{ field: 'project', code: 'INVALID', message: '{"message":"请重新载入项目","raw":{"x":1}}' }],
      }) });
      return reply(route, pageOf([project]));
    }
    if (path === `/api/v2/projects/${projectId}/releases`) return reply(route, pageOf([]));
    if (path === `/api/v2/projects/${projectId}/automation-policies`) return reply(route, pageOf([policy]));
    if (path === '/api/v2/migrations/reports') return reply(route, pageOf([report]));
    if (path === `/api/v2/migrations/reports/${reportId}`) return reply(route, report);
    if (path === `/api/v2/migrations/reports/${reportId}/source`) return reply(route, source);
    if (path === `/api/v2/migrations/reports/${reportId}/mappings`) return reply(route, pageOf([{
      id: recordId, first_import_id: reportId, disposition: 'READ_ONLY_HISTORY', created_at: now,
      key: { source_installation_id: sourceId, source_table: 'research', values: { id: exactInteger } },
    }]));
    if (path === `/api/v2/migrations/reports/${reportId}/records/${recordId}/fields`) return reply(route, {
      schema_version: 1, report_id: reportId, record_id: recordId,
      fields: Object.entries(fieldValues).map(([name, text]) => ({ name, character_count: String(text.length) })),
    });
    if (path === `/api/v2/migrations/reports/${reportId}/records/${recordId}/field`) {
      const name = url.searchParams.get('name')!; const text = fieldValues[name]!;
      const offset = Number(url.searchParams.get('offset') ?? '0');
      const chunk = name === 'multipart' ? text.slice(offset, offset + 20) : text;
      return reply(route, { schema_version: 1, report_id: reportId, record_id: recordId, name,
        offset: String(offset), total_characters: String(text.length), text: chunk,
        next_offset: offset + chunk.length < text.length ? String(offset + chunk.length) : null });
    }
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
}

async function navigate(page: Page, name: string) {
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  const toggle = page.getByRole('button', { name: '打开主导航' });
  const mobile = (page.viewportSize()?.width ?? 1280) < 992;
  if (mobile) await toggle.click();
  await page.getByRole('menuitem', { name, exact: true }).click();
  // Drawer restores focus to its trigger after closing. Wait for that transition
  // before opening a Select, otherwise focus restoration dismisses its popup.
  if (mobile) await expect(page.getByRole('dialog', { name: '主导航' })).toBeHidden();
}

async function noRawData(page: Page) {
  const text = await page.locator('body').innerText();
  expect(text).not.toMatch(/\bjson\b|[{}]|"[^"]+"\s*:/i);
  expect(await page.locator('pre').count()).toBe(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
}

for (const width of [1280, 390]) {
  test.describe(`${width}px`, () => {
    test.use({ viewport: { width, height: 900 } });

    test('policy details retain nested criteria as readable fields', async ({ page }) => {
      await setup(page);
      await navigate(page, '交付');
      await page.getByRole('combobox', { name: '选择交付所属项目' }).click();
      await page.getByText(`界面验收项目 · ${projectId}`, { exact: true }).last().click();
      await page.getByRole('tab', { name: '自动化政策' }).click();
      await page.locator('.ant-table-row-expand-icon').click();
      const details = page.getByRole('region', { name: '原自动化政策' });
      await expect(details).toContainText('晋级指标要求');
      await expect(details).toContainText(exactInteger);
      await expect(details).toContainText('0.0100');
      await expect(details).toContainText('native-statistics');
      await noRawData(page);
    });

    test('migration fields show readable content and preserve exact original downloads', async ({ page }) => {
      await setup(page);
      await navigate(page, '设置');
      if (width >= 768) await page.getByRole('tab', { name: '迁移' }).click();
      else {
        const category = page.getByRole('combobox', { name: '设置类别' });
        await category.focus();
        await category.press('ArrowDown');
        await expect(page.getByRole('option', { name: '迁移', exact: true })).toBeVisible();
        await category.press('ArrowUp');
        await expect(page.locator('.ant-select-item-option-active')).toHaveText('迁移');
        await category.press('Enter');
      }
      await page.getByRole('button', { name: reportId, exact: true }).click();
      const dialog = page.getByRole('dialog', { name: '历史导入报告' });
      await expect(dialog.getByRole('region', { name: '完整原主键' })).toContainText(exactInteger);
      await dialog.locator('.ant-table-row-expand-icon').click();
      await dialog.getByRole('button', { name: '查看字段 structured', exact: true }).click();
      const content = dialog.getByRole('region', { name: '原字段内容' });
      await expect(content).toContainText('保留的原字段');
      await expect(content).toContainText(exactInteger);
      await expect(content).toContainText('乙');
      const downloadEvent = page.waitForEvent('download');
      await dialog.getByRole('button', { name: '下载字段原文' }).click();
      const download = await downloadEvent;
      expect(await readFile((await download.path())!, 'utf8')).toBe(serializedField);
      await dialog.getByRole('button', { name: '查看字段 ndjson', exact: true }).click();
      await expect(content).toContainText('第一条');
      await expect(content).toContainText('第二条');
      await dialog.getByRole('button', { name: '查看字段 malformed', exact: true }).click();
      await expect(content).toBeVisible();
      await expect(dialog).not.toContainText('unrendered_payload');
      await dialog.getByRole('button', { name: '查看字段 multipart', exact: true }).click();
      await expect(dialog.getByText('字段内容较长，请下载当前段查看')).toBeVisible();
      await expect(dialog).not.toContainText('fragment');
      await expect(dialog.getByRole('button', { name: '下载此段原文' })).toBeVisible();
      await noRawData(page);
    });

    test('structured service errors remain readable without raw payloads', async ({ page }) => {
      await setup(page, true);
      await expect(page.getByText(/服务暂时不可用/)).toBeVisible();
      await expect(page.getByText(/请重新载入项目/)).toBeVisible();
      await expect(page.getByText(new RegExp(reportId))).toBeVisible();
      await noRawData(page);
    });
  });
}
