// SYNTHETIC presentation contracts only; native PG/HTTP/CLI checks live in Rust.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { brief, fixture, id, navigate, problem, project, reply, run } from './fixtures';

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
const calibration: Schema['CalibrationView'] = { id: id(55), alpha_version_id: version.id,
  estimator_kind: 'linregress.affine_ols', estimator_version: '0.5.4', model_artifact_id: id(56), train_input_set_id: evaluation.input_set_id,
  fit_end_available_at: '2026-09-08T00:00:00.000001Z', output_unit: 'RETURN_PER_HORIZON', horizon_kind: version.horizon_kind,
  horizon_value: '9007199254740993', validation: { ...evaluation, subject_alpha_version_id: id(54), decision: 'REJECT' }, created_at: stamp };
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
  const state = { paths: [] as string[], fail: false, calibrated: false, calibrationFailed: false };
  await page.route('**/api/v2/**', async route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    state.paths.push(`${path}${url.search}`);
    if (path === '/api/v2/alphas') {
      if (state.fail) return reply(route, problem('UNAVAILABLE', 503), 503);
      return reply(route, { schema_version: 1, items: url.searchParams.get('project_id') === project.id ? [alpha] : [], next_cursor: null });
    }
    const current = state.calibrated ? { ...version, calibration_id: calibration.id } : version;
    if (path === `/api/v2/alphas/${alpha.id}/versions`) return reply(route, { schema_version: 1, items: [current], next_cursor: null });
    if (path === `/api/v2/alphas/${alpha.id}/versions/${version.version}`) return reply(route, current);
    if (path === `/api/v2/alpha-versions/${version.id}/calibration`) return state.calibrationFailed ? reply(route, problem('UNAVAILABLE', 503), 503) : reply(route, calibration);
    if (path === `/api/v2/alpha-versions/${version.id}/evaluations`) return reply(route, { schema_version: 1, items: state.calibrated ? [] : [evaluation, cancelled], next_cursor: null });
    const item = [state.calibrated ? calibration.validation : evaluation, cancelled].find(item => path === `/api/v2/evaluations/${item.id}`);
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

test('Sealed admission requires an explicit Cycle and retries the identical frozen request after a lost reply', async ({ page }) => {
  const { state } = await setup(page); state.calibrated = true;
  const frozenBrief = brief();
  const context: Schema['BriefExecutionContextV1'] = { schema_version: 1, runtime_id: id(20), runtime_revision: '9007199254740993',
    discovery_input_set_id: id(21), validation_input_set_id: id(22), sealed_input_set_id: id(23) };
  const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: id(40), project_id: project.id, brief_id: frozenBrief.id,
    ordinal: 1, revision: '1', trigger: 'OPERATOR', state: 'RUNNING', outcome: null, budget: frozenBrief.content.budget,
    reserved_experiments: 0, used_experiments: 1, reserved_cpu_seconds: '0', initial_run_id: run.id,
    researcher_profile: { profile_id: id(30), expected_revision: '1' }, reviewer_profile: { profile_id: id(31), expected_revision: '1' },
    next_action: 'WAITING_FOR_DATA_VALIDATION', started_at: stamp, ended_at: null, created_at: stamp,
    available_actions: ['VIEW_BRIEF', 'VIEW_RUNS', 'VIEW_EXPERIMENTS'] };
  const writes: { body: unknown; key: string | null }[] = [];
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path === `/api/v2/alpha-versions/${version.id}/evaluations` && request.method() === 'POST') {
      writes.push({ body: request.postDataJSON() as unknown, key: await request.headerValue('Idempotency-Key') });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { ...run, cycle_id: cycle.id, kind: 'ALPHA_EVALUATE', input_set_id: context.sealed_input_set_id } }, 202);
    }
    if (path === `/api/v2/projects/${project.id}/cycles`) return reply(route, { schema_version: 1, items: [cycle], next_cursor: null });
    if (path === `/api/v2/cycles/${cycle.id}`) return reply(route, cycle);
    if (path === `/api/v2/briefs/${frozenBrief.id}/execution-context`) return reply(route, { schema_version: 1, brief: frozenBrief, execution_context: context });
    return route.fallback();
  });
  await chooseProject(page);
  await page.getByRole('button', { name: alpha.name, exact: true }).click();
  await page.getByRole('button', { name: `版本 ${version.version}`, exact: true }).click();
  await page.getByRole('button', { name: '请求封存评估', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '确认请求封存评估', exact: true });
  await expect(dialog.getByRole('button', { name: '确认请求评估', exact: true })).toBeDisabled();
  expect(writes).toEqual([]);
  await dialog.getByRole('combobox', { name: '选择评估 Cycle', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: cycle.id }).click();
  await dialog.getByLabel('CPU 秒数上限', { exact: true }).fill('9007199254740993');
  await dialog.getByRole('button', { name: '确认请求评估', exact: true }).click();
  await expect(dialog.getByText('请求结果尚未确认。', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('CPU 秒数上限', { exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: '重试同一请求', exact: true }).click();
  await expect(dialog.getByText('封存评估 Run 已登记。', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2);
  const first = writes[0]; if (!first) throw new Error('Sealed request was not captured');
  expect(first.key).toBeTruthy(); expect(writes[1]).toEqual(first);
  expect(first.body).toEqual({ schema_version: 1, cycle_id: cycle.id, policy_id: frozenBrief.content.evaluation_policy_id,
    input_set_id: context.sealed_input_set_id, runtime_id: context.runtime_id, expected_runtime_revision: context.runtime_revision,
    limits: { schema_version: 1, experiments: 0, cpu_seconds: '9007199254740993', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' } });
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
});

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

test('calibrated version exposes only its original metadata and does not inherit a source verdict', async ({ page }) => {
  const { state, base } = await setup(page); state.calibrated = true; state.calibrationFailed = true;
  await chooseProject(page);
  await page.getByRole('button', { name: alpha.name, exact: true }).click();
  await page.getByRole('button', { name: `版本 ${version.version}`, exact: true }).click();
  await expect(page.getByText('还没有可披露的正式 Validation 评估；不包含 Sealed，也不代表验证通过。', { exact: true })).toBeVisible();
  expect(state.paths.some(path => path.endsWith('/calibration'))).toBe(false);
  await page.getByRole('button', { name: '查看冻结校准来源', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '冻结校准来源', exact: true });
  await expect(dialog.getByText(/错误：UNAVAILABLE/)).toBeVisible();
  await expect(dialog.getByText('新版本附加校准，不继承源版本评估或资格。', { exact: true })).toHaveCount(0);
  state.calibrationFailed = false;
  await dialog.getByRole('button', { name: '重新载入', exact: true }).click();
  await expect(dialog.getByText('新版本附加校准，不继承源版本评估或资格。', { exact: true })).toBeVisible();
  await expect(dialog.getByText(calibration.fit_end_available_at, { exact: true })).toBeVisible();
  await expect(dialog.getByText('FIXED_BARS · 9007199254740993 · RETURN_PER_HORIZON', { exact: true })).toBeVisible();
  await expect(dialog.getByText('SUCCEEDED / VALID / REJECT', { exact: true })).toBeVisible();
  await expect(dialog.getByText(/读取时已过期/)).toBeVisible();
  await expect(dialog.getByText(id(54), { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await dialog.getByRole('button', { name: '查看源版本原评估', exact: true }).click();
  const source = page.getByRole('dialog', { name: '正式 Validation 评估', exact: true });
  await expect(source.getByText(id(54), { exact: true })).toBeVisible();
  await expect(source.getByText('SUCCEEDED / VALID / REJECT', { exact: true })).toBeVisible();
  await source.getByRole('button', { name: '关闭', exact: true }).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: '关闭', exact: true }).click();
  await expect(page.getByRole('button', { name: `评估 ${evaluation.id.slice(-8)}`, exact: true })).toHaveCount(0);
  expect(state.paths.some(path => path.startsWith('/api/v2/artifacts/'))).toBe(false);
  expect(base.commands).toEqual([]);
});
