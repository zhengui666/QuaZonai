// Synthetic presentation evidence only; native admission/queue/lease tests are in Rust.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { brief, fixture, id, problem, project, reply, run } from './fixtures';
import type { Captured } from './fixtures';
import type { Schema } from '../src/api';

const revision = '9007199254740993';
const stamp = '2026-09-08T00:00:00Z';
const context: Schema['BriefExecutionContextV1'] = { schema_version: 1, runtime_id: id(20), runtime_revision: revision,
  discovery_input_set_id: id(21), validation_input_set_id: id(22), sealed_input_set_id: id(23) };
const runtime: Schema['RuntimeView'] = { id: id(20), revision, configuration: { name: '原生测试 Runtime', endpoint: 'https://runtime.example',
  tls_policy: 'SYSTEM_CA', allowed_capabilities: ['DATA_VALIDATE'], enabled: true, development_http: false },
  protocol_version: 1, credential_configured: true, ca_configured: false, last_capability_snapshot_artifact_id: null, created_at: stamp, updated_at: stamp };
const profiles: Schema['CodexProfileViewV1'][] = [30, 31].map((tail, index) => ({ id: id(tail), name: index ? '审阅配置' : '研究配置',
  home_binding: `synthetic-${tail}`, profile_origin: 'OPERATOR_MOUNT', connection_mode: 'SYSTEM', custom_base_url: null,
  credential_configured: false, model_settings: { schema_version: 1, use_default_model_settings: true,
    saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false }, revision, created_at: stamp, updated_at: stamp }));
const inputs: Schema['InputSetSummary'][] = (['DISCOVERY', 'VALIDATION', 'SEALED'] as const).map((purpose, index) => ({
  id: id(21 + index), project_id: project.id, purpose, revision, decision_cutoff: stamp, frozen_at: stamp, created_at: stamp,
}));

async function setup(page: Page, options: { draft?: boolean; archived?: boolean; loseAck?: boolean; conflict?: boolean } = {}) {
  const base = await fixture(page);
  const state = { project: { ...project, state: options.archived ? 'ARCHIVED' : options.draft ? 'DRAFT' : 'ACTIVE',
    current_brief_id: options.draft ? null : id(10), archived_at: options.archived ? stamp : null } as Schema['ProjectView'],
    brief: { ...brief(options.draft ? 'DRAFT' : 'FROZEN'), revision }, writes: [] as Captured[],
    cycles: [] as Schema['CycleViewV1'][], profileRevision: revision, stale: false };
  const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: id(40), project_id: project.id, brief_id: id(10), ordinal: 1,
    revision: '1', trigger: 'OPERATOR', state: 'QUEUED', outcome: null, budget: state.brief.content.budget,
    reserved_experiments: 0, used_experiments: 0, reserved_cpu_seconds: '3600', initial_run_id: run.id,
    researcher_profile: { profile_id: id(30), expected_revision: revision }, reviewer_profile: { profile_id: id(31), expected_revision: revision },
    next_action: 'WAITING_FOR_DATA_VALIDATION', started_at: null, ended_at: null, created_at: stamp, available_actions: ['VIEW_BRIEF', 'VIEW_RUNS', 'VIEW_EXPERIMENTS'] };
  base.run = { ...run, kind: 'DATA_VALIDATE', cycle_id: cycle.id };
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname; const method = request.method();
    if (method !== 'GET') {
      if (![ `/api/v2/briefs/${id(10)}/freeze`, `/api/v2/projects/${project.id}`, `/api/v2/projects/${project.id}/cycles` ].includes(path)) return route.fallback();
      state.writes.push({ path, method, key: await request.headerValue('Idempotency-Key'), body: request.postDataJSON() as unknown });
      const calls = state.writes.filter(item => item.path === path).length;
      if (options.conflict || (options.loseAck && calls === 2)) return reply(route, problem('REVISION_CONFLICT', 409), 409);
      if (path.endsWith('/freeze')) { state.brief = { ...state.brief, state: 'FROZEN', frozen_at: stamp }; state.project.current_brief_id = state.brief.id; }
      if (path.endsWith('/cycles')) state.cycles = [cycle];
      if (options.loseAck && calls === 1) { state.profileRevision = '9007199254740994'; return route.abort('failed'); }
      if (method === 'PATCH') {
        state.project = { ...state.project, state: 'ACTIVE', revision: '9007199254740994' };
        return reply(route, { schema_version: 1, resource: state.project, replayed: false });
      }
      return reply(route, { schema_version: 1, replayed: calls > 1, resource: path.endsWith('/freeze')
        ? { schema_version: 1, brief: state.brief, execution_context: context }
        : { schema_version: 1, cycle, run: base.run } }, path.endsWith('/cycles') ? 202 : 200);
    }
    if (path === '/api/v2/projects') return reply(route, { schema_version: 1, items: [state.project], next_cursor: null });
    if (path === `/api/v2/projects/${project.id}`) return state.stale ? reply(route, problem('UNAVAILABLE', 503), 503) : reply(route, state.project);
    if (path.endsWith('/briefs')) return reply(route, { schema_version: 1, items: [state.brief], next_cursor: null });
    if (path === `/api/v2/briefs/${id(10)}`) return reply(route, state.brief);
    if (path.endsWith('/execution-context')) return reply(route, { schema_version: 1, brief: state.brief, execution_context: context });
    if (path.endsWith('/cycles')) return reply(route, { schema_version: 1, items: state.cycles, next_cursor: null });
    if (path === '/api/v2/integrations/runtimes') return reply(route, { schema_version: 1, items: [runtime], next_cursor: null });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(route, runtime);
    if (path === '/api/v2/input-sets') {
      expect(url.searchParams.get('project_id')).toBe(project.id);
      return reply(route, { schema_version: 1, items: [...inputs, { ...inputs[0], id: id(90), project_id: id(91) }], next_cursor: null });
    }
    if (path === '/api/v2/settings/codex') return reply(route, { schema_version: 1, items: profiles, next_cursor: null });
    const profile = profiles.find(item => path === `/api/v2/settings/codex/${item.id}`);
    if (profile) return reply(route, { ...profile, revision: state.profileRevision });
    return route.fallback();
  });
  await page.goto('/'); await page.getByRole('button', { name: project.name, exact: true }).click();
  return Object.assign(state, { cycle });
}

async function choose(page: Page, label: string, value: string) {
  const field = page.getByLabel(label);
  await field.click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: value }).click();
  await expect(field).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.ant-select-dropdown:visible')).toHaveCount(0);
}
async function freezeForm(page: Page) {
  await page.getByRole('button', { name: '冻结执行上下文', exact: true }).click();
  await choose(page, '选择执行 Runtime', runtime.id);
  for (const input of inputs) {
    const field = page.getByRole('combobox', { name: `选择 ${input.purpose} 输入集`, exact: true });
    await field.click();
    const options = page.locator('.ant-select-dropdown:visible .ant-select-item-option-content');
    await expect(options).toHaveCount(1); await expect(options).toContainText(input.id);
    await options.click(); await expect(field).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('.ant-select-dropdown:visible')).toHaveCount(0);
  }
}
async function startForm(page: Page) {
  await page.getByRole('button', { name: '启动新 Cycle', exact: true }).click();
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  await choose(page, '选择研究者 Codex 配置', id(30));
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  await choose(page, '选择独立 Reviewer Codex 配置', id(31));
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeEnabled();
}

test('draft project freezes exact versions, explicitly activates, starts and views the queued preparation', async ({ page }) => {
  const state = await setup(page, { draft: true });
  await freezeForm(page);
  await page.getByRole('button', { name: '确认冻结 Brief', exact: true }).click();
  await expect(page.getByText('Brief 已冻结，尚未启动研究。', { exact: true })).toBeVisible();
  expect(state.writes[0]?.body).toEqual({ schema_version: 1, expected_revision: revision, execution_context: context });
  expect(state.project.state).toBe('DRAFT');
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();
  await expect(page.getByRole('button', { name: '启动新 Cycle', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '修改项目状态', exact: true }).click();
  await choose(page, '项目状态', '启用');
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeHidden();
  expect(state.writes[1]?.body).toMatchObject({ expected_revision: revision, state: 'ACTIVE' });
  await startForm(page);
  await page.getByRole('button', { name: '确认启动 Cycle', exact: true }).click();
  await expect(page.getByText('Cycle 与准备运行已由服务器登记。', { exact: true })).toBeVisible();
  expect(state.writes[2]?.body).toEqual({ schema_version: 1, brief_id: id(10), expected_revision: '9007199254740994',
    researcher_profile: { profile_id: id(30), expected_revision: revision }, reviewer_profile: { profile_id: id(31), expected_revision: revision } });
  expect(state.writes.every(item => !!item.key)).toBe(true);
  await expect(page.getByRole('dialog').getByText('排队', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await expect(page.getByText(id(40), { exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '尚无周期结论 WAITING_FOR_DATA_VALIDATION', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '查看准备运行', exact: true }).click();
  await expect(page.getByRole('dialog').getByText(run.id, { exact: true })).toBeVisible();
  expect(state.writes).toHaveLength(3);
});

for (const draft of [true, false]) test(`${draft ? 'freeze' : 'start'} lost ACK then conflict retains the original request and key`, async ({ page, context: browser }) => {
  const state = await setup(page, { draft, loseAck: true });
  if (draft) await freezeForm(page); else await startForm(page);
  await page.getByRole('button', { name: draft ? '确认冻结 Brief' : '确认启动 Cycle', exact: true }).click();
  const retry = page.getByRole('button', { name: '重试同一请求', exact: true });
  await expect(retry).toBeVisible();
  await expect(page.getByText('请求结果尚未确认。', { exact: true })).toBeVisible();
  state.stale = true;
  await browser.setOffline(true); await expect(retry).toBeDisabled();
  await browser.setOffline(false);
  // The app intentionally uses networkMode=always (no reconnect refetch).
  // Returning to the visible tab is its configured background refresh trigger.
  await page.evaluate(() => window.dispatchEvent(new Event('visibilitychange')));
  await expect(page.getByText('服务返回了无法识别的响应（HTTP 503）。未将它当成空列表或成功结果。').first()).toBeVisible();
  await expect(retry).toBeEnabled();
  for (const field of await page.getByRole('dialog').getByRole('combobox').all()) await expect(field).toBeDisabled();
  await retry.click();
  await expect(page.getByText(/错误：REVISION_CONFLICT/)).toBeVisible();
  await expect(page.getByRole('button', { name: '关闭并重载最新记录', exact: true })).toHaveCount(0);
  state.stale = false;
  await expect(retry).toBeEnabled(); await retry.click();
  await expect(page.getByText(draft ? 'Brief 已冻结，尚未启动研究。' : 'Cycle 与准备运行已由服务器登记。', { exact: true })).toBeVisible();
  expect(state.writes).toHaveLength(3);
  expect(state.writes[1]).toEqual(state.writes[0]); expect(state.writes[2]).toEqual(state.writes[0]);
});

test('known revision rejection does not overwrite and requires reloading the record', async ({ page }) => {
  const state = await setup(page, { conflict: true }); await startForm(page);
  await page.getByRole('button', { name: '确认启动 Cycle', exact: true }).click();
  await expect(page.getByText(/错误：REVISION_CONFLICT/)).toBeVisible();
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '关闭并重载最新记录', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeHidden(); expect(state.writes).toHaveLength(1);
});

test('offline and failed project reread block spending without hiding the form', async ({ page, context: browser }) => {
  const state = await setup(page); await startForm(page);
  await browser.setOffline(true);
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  await expect(page.getByRole('combobox', { name: '选择研究者 Codex 配置', exact: true })).toBeDisabled();
  await browser.setOffline(false); await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '返回', exact: true }).click();
  state.stale = true;
  await page.getByRole('button', { name: '启动新 Cycle', exact: true }).click();
  await expect(page.getByText('服务返回了无法识别的响应（HTTP 503）。未将它当成空列表或成功结果。').first()).toBeVisible();
  await expect(page.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  expect(state.writes).toHaveLength(0);
});

test('archived projects cannot freeze or start and the startup dialog is accessible at each viewport', async ({ page }) => {
  const state = await setup(page, { archived: true, draft: true });
  await expect(page.getByRole('button', { name: '冻结执行上下文', exact: true })).toBeDisabled();
  state.brief = { ...state.brief, state: 'FROZEN', frozen_at: stamp };
  await page.reload(); await page.getByRole('button', { name: project.name, exact: true }).click();
  await expect(page.getByRole('button', { name: '启动新 Cycle', exact: true })).toBeDisabled();
  state.project = { ...state.project, state: 'ACTIVE', archived_at: null };
  await page.reload(); await page.getByRole('button', { name: project.name, exact: true }).click();
  await startForm(page);
  const result = await new AxeBuilder({ page }).include('.ant-modal').analyze();
  expect(result.violations).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(state.writes).toHaveLength(0);
});

const selection: Schema['CycleSelectionV1'] = { schema_version: 1, cycle_id: id(40), project_id: project.id,
  research_run_id: id(60), policy_id: id(61), created_at: stamp, status: 'INCONCLUSIVE',
  trial_count: '9007199254740993', eligible_count: '1', selected_count: '1', unfinished_count: '1',
  rule: { schema_version: 1, comparable_scope: 'FAMILY_LINEAGE', family_id: id(62), root_lineage_id: project.root_lineage_id,
    comparison_input_set_id: id(22), execution_assumptions_id: id(63), evaluation_kind: 'WALK_FORWARD',
    metric_code: 'PEARSON_IC', metric_scope: 'asset:0/fold:0', method_id: 'ndarray-stats.pearson_correlation', method_version: '0.7.0',
    unit: 'CORRELATION', frequency: '1-MINUTE-LAST-EXTERNAL;horizon=5', direction: 'MAXIMIZE', candidate_count: 2,
    tie_break: 'EXPERIMENT_ID_ASC', missing_required_metric: 'INCONCLUSIVE' } };
const selectedTrial: Schema['CycleSelectionTrialV1'] = { schema_version: 1, cycle_id: id(40), experiment_id: id(64),
  source_cycle_id: id(40), execution_run_id: id(67), compile_run_id: id(65), discovery_run_id: id(66), validation_run_id: id(67),
  alpha_version_id: id(68), review_alpha_version_id: id(74), evaluation_id: id(69), execution_state: 'SUCCEEDED', reason: 'ELIGIBLE', rank: '1', selected: true, unfinished: false,
  selection_metric: { schema_version: 1, evaluation_id: id(69), metric_code: selection.rule.metric_code, scope: selection.rule.metric_scope,
    value: 0, status: 'OK', reason_code: null, unit: selection.rule.unit, frequency: selection.rule.frequency,
    method_id: selection.rule.method_id, method_version: selection.rule.method_version, source_artifact_id: id(70),
    period_start: stamp, period_end: '2026-09-09T00:00:00Z', observation_count: '9007199254740993', annualization_factor: null, higher_is_better: true } };
const pendingTrial: Schema['CycleSelectionTrialV1'] = { ...selectedTrial, experiment_id: id(71), source_cycle_id: id(72), compile_run_id: null,
  discovery_run_id: null, validation_run_id: null, alpha_version_id: null, review_alpha_version_id: null, evaluation_id: null, execution_run_id: null, execution_state: null,
  reason: 'UNFINISHED', rank: null, selected: false, unfinished: true, selection_metric: null };

test('frozen selection preserves history, original metric zero and exact counts without a winner write', async ({ page }) => {
  const state = await setup(page);
  state.cycles = [{ ...state.cycle, available_actions: [...state.cycle.available_actions, 'VIEW_SELECTION'] }];
  let empty = false; const requested: string[] = [];
  await page.route('**/api/v2/cycles/*/selection**', async route => {
    const url = new URL(route.request().url()); requested.push(url.pathname + url.search);
    expect(route.request().method()).toBe('GET');
    if (url.pathname.endsWith('/selection')) return reply(route, empty ? { ...selection, cycle_id: id(73), trial_count: '0', eligible_count: '0', selected_count: '0', unfinished_count: '0' } : selection);
    const cursor = url.searchParams.get('cursor'); if (cursor) expect(cursor).toBe(selectedTrial.experiment_id);
    return reply(route, { schema_version: 1, items: empty ? [] : [cursor ? pendingTrial : selectedTrial], next_cursor: cursor || empty ? null : selectedTrial.experiment_id });
  });
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await page.getByRole('button', { name: '查看试验选择', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '冻结试验选择', exact: true });
  await expect(dialog.getByText('选择完成不是科学 PASS、Sealed 或可交付资格。', { exact: true })).toBeVisible();
  await expect(dialog.getByText('9007199254740993 / 1 / 1 / 1', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: '0', exact: true })).toBeVisible();
  await dialog.locator('.ant-table-row-expand-icon').click();
  await expect(dialog.getByText(selectedTrial.validation_run_id!, { exact: true })).toHaveCount(2);
  await expect(dialog.getByText('冻结审阅版本（非资格）', { exact: true })).toBeVisible();
  await expect(dialog.getByText(selectedTrial.review_alpha_version_id!, { exact: true })).toBeVisible();
  await expect(dialog.getByText('9007199254740993', { exact: true })).toBeVisible();
  await expect(dialog.getByText(selectedTrial.selection_metric!.source_artifact_id, { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"][aria-modal="true"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  const header = dialog.getByRole('row').filter({ has: page.getByRole('columnheader', { name: '原指标数值', exact: true }) });
  await dialog.getByRole('button', { name: '关闭', exact: true }).focus();
  await page.keyboard.press('Tab'); await expect(header).toBeFocused();
  const scroller = dialog.locator('.ant-table-content'); const before = await scroller.evaluate(element => element.scrollLeft);
  await page.keyboard.press('ArrowRight');
  // At the wide desktop viewport the table can fit without horizontal overflow.
  if (await scroller.evaluate(element => element.scrollWidth > element.clientWidth)) await expect.poll(() => scroller.evaluate(element => element.scrollLeft)).toBeGreaterThan(before);
  await dialog.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(dialog.getByRole('cell', { name: '缺值：UNFINISHED', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: '不参与排名', exact: true })).toBeVisible();
  await expect(dialog.getByRole('cell', { name: '0', exact: true })).toHaveCount(0);
  await dialog.getByRole('button', { name: '关闭', exact: true }).click();
  empty = true;
  state.cycles = [{ ...state.cycle, id: id(73), available_actions: ['VIEW_SELECTION'] }];
  await page.getByRole('button', { name: '刷新研究周期', exact: true }).click();
  await expect(page.getByText(id(73), { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '查看试验选择', exact: true }).click();
  await expect(dialog.getByText('本快照没有登记试验，不代表存在合格候选。', { exact: true })).toBeVisible();
  await expect(dialog.getByText('第 1 页（游标分页）', { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"][aria-modal="true"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(requested.some(path => path.includes(`cursor=${selectedTrial.experiment_id}`))).toBe(true);
  expect(state.writes).toHaveLength(0);
});

test('unformed or failed selection is not shown as an empty completed comparison', async ({ page }) => {
  const state = await setup(page); let formed = false;
  state.cycles = [state.cycle];
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await expect(page.getByText('尚未形成选择快照', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '查看试验选择', exact: true })).toHaveCount(0);
  state.cycles = [{ ...state.cycle, available_actions: ['VIEW_SELECTION'] }];
  await page.route('**/api/v2/cycles/*/selection', route => reply(route, formed ? selection : problem('NOT_FOUND', 404), formed ? 200 : 404));
  await page.route('**/api/v2/cycles/*/selection/trials*', route => reply(route, { schema_version: 1, items: [pendingTrial], next_cursor: null }));
  await page.getByRole('button', { name: '刷新研究周期', exact: true }).click();
  await page.getByRole('button', { name: '查看试验选择', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '冻结试验选择', exact: true });
  await expect(dialog.getByText(/错误：NOT_FOUND/)).toBeVisible();
  await expect(dialog.getByText('本快照没有登记试验，不代表存在合格候选。', { exact: true })).toHaveCount(0);
  formed = true;
  await dialog.getByRole('button', { name: '重新载入', exact: true }).click();
  await expect(dialog.getByRole('cell', { name: 'UNFINISHED', exact: true })).toBeVisible();
  expect(state.writes).toHaveLength(0);
});
