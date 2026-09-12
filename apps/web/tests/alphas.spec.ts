// SYNTHETIC presentation contracts only; native PG/HTTP/CLI checks live in Rust.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, problem, project, reply } from './fixtures';

const stamp = '2026-09-08T00:00:00Z';
const alpha: Schema['AlphaView'] = { id: id(41), project_id: project.id, name: '合成 Alpha 历史', lifecycle: 'RESEARCH',
  active_version_id: id(42), active_version: '2147483647', revision: '9007199254740993', created_at: stamp, updated_at: stamp };
const version: Schema['AlphaVersionView'] = { id: id(42), project_id: project.id, alpha_id: alpha.id, version: '2147483647',
  experiment_id: id(43), root_lineage_id: project.root_lineage_id, code_artifact_id: id(44), model_artifact_id: id(45),
  signal_contract_version: '1', signal_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '9007199254740993',
  forecast_unit: 'UNITLESS_SCORE', calibration_id: null, runtime_image_ref: 'controlled-original-image', origin: 'FIXTURE', created_at: stamp };
const evaluation: Schema['EvaluationView'] = { id: id(46), project_id: project.id, subject_alpha_version_id: version.id,
  subject_candidate_id: null, input_set_id: id(47), policy_id: id(48), run_id: id(49), evaluation_kind: 'WALK_FORWARD',
  execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'PASS', report_artifact_id: id(50), method_versions_artifact_id: id(50),
  origin: 'FIXTURE', concluded_at: stamp, valid_until: '2026-09-09T00:00:00Z', checked_at: '2026-09-12T00:00:00Z', unexpired_at_read: false };
const cancelled: Schema['EvaluationView'] = { ...evaluation, id: id(51), execution_status: 'CANCELLED', evidence_status: 'INCOMPLETE',
  decision: 'INCONCLUSIVE', valid_until: null };
const metric: Schema['MetricValueV1'] = { schema_version: 1, evaluation_id: evaluation.id, metric_code: 'correlation', scope: 'original-scope',
  value: 0, status: 'OK', reason_code: null, unit: 'UNITLESS_SCORE', period_start: stamp, period_end: '2026-09-09T00:00:00Z',
  observation_count: '9007199254740993', frequency: 'original-frequency', annualization_factor: null,
  method_id: 'native-method', method_version: '0.7.3', source_artifact_id: id(52), higher_is_better: true };
const missing: Schema['MetricValueV1'] = { ...metric, metric_code: 'missing-correlation', value: null,
  status: 'INSUFFICIENT_DATA', reason_code: 'TOO_FEW_ORIGINAL_ROWS', observation_count: '0' };

async function chooseProject(page: Page, name = project.name) {
  const field = page.getByRole('combobox', { name: '选择 Alpha 所属项目', exact: true });
  await field.click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: name }).click();
  await expect(field).toHaveAttribute('aria-expanded', 'false');
}
async function setup(page: Page) {
  const base = await fixture(page);
  base.projects.push({ ...project, id: id(2), name: '另一个项目' });
  const state = { paths: [] as string[], fail: false };
  await page.route('**/api/v2/**', async route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    state.paths.push(`${path}${url.search}`);
    if (path === '/api/v2/alphas') {
      if (state.fail) return reply(route, problem('UNAVAILABLE', 503), 503);
      return reply(route, { schema_version: 1, items: url.searchParams.get('project_id') === project.id ? [alpha] : [], next_cursor: null });
    }
    if (path === `/api/v2/alphas/${alpha.id}/versions`) return reply(route, { schema_version: 1, items: [version], next_cursor: null });
    if (path === `/api/v2/alphas/${alpha.id}/versions/${version.version}`) return reply(route, version);
    if (path === `/api/v2/alpha-versions/${version.id}/evaluations`) return reply(route, { schema_version: 1, items: [evaluation, cancelled], next_cursor: null });
    const item = [evaluation, cancelled].find(item => path === `/api/v2/evaluations/${item.id}`);
    if (item) return reply(route, item);
    if (path === `/api/v2/evaluations/${evaluation.id}/metrics`) {
      const cursor = url.searchParams.get('cursor');
      if (cursor !== null) expect(cursor).toBe(id(53));
      return reply(route, { schema_version: 1, items: [cursor ? missing : metric], next_cursor: cursor ? null : id(53) });
    }
    if (path === `/api/v2/evaluations/${cancelled.id}/metrics`) {
      expect(url.searchParams.has('cursor')).toBe(false);
      return reply(route, { schema_version: 1, items: [], next_cursor: null });
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, 'Alpha');
  return { state, base };
}

test('original Alpha, formal evidence and paged metrics retain zero, null, provenance and exact counts', async ({ page }) => {
  const { state, base } = await setup(page);
  await expect(page.getByText('请选择项目后查看已有 Alpha，不会自动选择或创建研究。', { exact: true })).toBeVisible();
  expect(state.paths.some(path => path.startsWith('/api/v2/alphas?'))).toBe(false);
  await chooseProject(page);
  await page.getByRole('button', { name: alpha.name, exact: true }).click();
  await page.getByRole('button', { name: `版本 ${version.version}`, exact: true }).click();
  await expect(page.getByText('未登记校准，不能据此认为已校准', { exact: true })).toBeVisible();
  await expect(page.getByText(/读取时已过期/)).toBeVisible();
  await page.getByRole('button', { name: `评估 ${evaluation.id.slice(-8)}`, exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '正式 Validation 评估', exact: true });
  await expect(dialog.getByText('SUCCEEDED / VALID / PASS', { exact: true })).toBeVisible();
  await expect(dialog.getByText('这是历史科学证据，不是资格或交付批准。', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: '0', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: '9007199254740993', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: 'native-method / 0.7.3', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: 'original-frequency', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: metric.source_artifact_id, exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const header = dialog.getByRole('row').filter({ has: page.getByRole('columnheader', { name: '原始数值', exact: true }) });
  await dialog.getByRole('button', { name: '关闭', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(header).toBeFocused();
  const scroller = dialog.locator('.ant-table-content');
  const before = await scroller.evaluate(element => element.scrollLeft);
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => scroller.evaluate(element => element.scrollLeft)).toBeGreaterThan(before);
  await dialog.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(dialog.getByRole('cell', { name: '缺值：TOO_FEW_ORIGINAL_ROWS', exact: true })).toBeVisible();
  await expect(dialog.getByText('第 2 页（游标分页）', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '关闭', exact: true }).click();
  await page.getByRole('button', { name: `评估 ${cancelled.id.slice(-8)}`, exact: true }).click();
  await expect(dialog.getByText('CANCELLED / INCOMPLETE / INCONCLUSIVE', { exact: true })).toBeVisible();
  await expect(dialog.getByText('本评估没有发表指标；不能把缺失解释成0或通过。', { exact: true })).toBeVisible();
  await expect(dialog.getByText('第 1 页（游标分页）', { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  await dialog.getByRole('button', { name: '关闭', exact: true }).click();
  await chooseProject(page, '另一个项目');
  await expect(page.getByText('本项目还没有 Alpha 登记；这不是无有效 Alpha 的科学结论。', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: `Alpha 版本 ${version.version}`, exact: true })).toHaveCount(0);
  expect(state.paths).toContain(`/api/v2/alphas/${alpha.id}/versions/${version.version}`);
  expect(state.paths.some(path => path.startsWith('/api/v2/artifacts/'))).toBe(false);
  expect(base.commands).toEqual([]);
});

test('failed read is not an empty Alpha result and recovery does not create a record', async ({ page }) => {
  const { state, base } = await setup(page); state.fail = true;
  await chooseProject(page);
  await expect(page.getByText(/未将它当成空列表或成功结果|错误：UNAVAILABLE/).first()).toBeVisible();
  await expect(page.getByText('本项目还没有 Alpha 登记；这不是无有效 Alpha 的科学结论。', { exact: true })).toHaveCount(0);
  state.fail = false;
  await page.getByRole('button', { name: '刷新 Alpha', exact: true }).click();
  await expect(page.getByRole('button', { name: alpha.name, exact: true })).toBeVisible();
  expect(base.commands).toEqual([]);
});
