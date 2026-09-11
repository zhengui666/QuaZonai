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
  return state;
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
