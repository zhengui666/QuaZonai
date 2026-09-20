// Controlled UI snapshots only, not native research or delivery evidence.
import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { brief, fixture, id, navigate, project, reply, run } from './fixtures';

const header: Schema['CandidateViewV1'] = {
  id: id(81), project_id: project.id, mandate_id: id(80), input_set_id: id(82), run_id: id(83),
  decision_asof: '2026-09-13T00:00:00Z', created_at: '2026-09-13T00:01:00Z',
  execution_status: 'SUCCEEDED', solver_status: 'OPTIMAL', evidence_status: 'VALID', origin: 'FIXTURE', reason_code: null,
  forecast_artifact_id: id(84), covariance_artifact_id: null, diagnostics_artifact_id: id(85), target_artifact_id: id(86),
  allocation_evaluation_id: null, cash_weight: '0', current_weights_source: 'FORWARD_SNAPSHOT', current_weights_artifact_id: id(87),
};

for (const source of ['FORWARD_SNAPSHOT', 'LAST_TARGET'] as const) test(`Build binds original sources, protects offline input and retries exact intent: ${source}`, async ({ page, context }) => {
  await fixture(page);
  const stamp = header.created_at;
  const frozen = brief();
  const mandate: Schema['MandateViewV1'] = { id: header.mandate_id, project_id: project.id, version: 1, created_at: stamp,
    content: { base_currency: 'USD', capital_assumption: '1000', universe_version_id: id(76), required_evaluation_policy_id: id(70), execution_assumptions_id: id(74), objective: 'MIN_RISK', risk_measure: 'VARIANCE', exposure_tolerance: '0.000001',
      constraints: { schema_version: 1, long_only: true, min_cash_weight: '0', max_cash_weight: '1', min_asset_weight: '0', max_asset_weight: '1', max_gross_exposure: '1', min_net_exposure: '0', max_net_exposure: '1', max_turnover_per_rebalance: '2', group_bounds: [], asset_overrides: [], transaction_costs_ref: id(77) },
      rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, session_offset_seconds: null, timezone: 'UTC', max_input_age_seconds: 60, target_ttl_seconds: 300 },
      covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
      alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
      optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1', parameters: { schema_version: 1, risk_aversion: '1', max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false, cvar_confidence: null, risk_budgeting: null } } } };
  const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: id(40), project_id: project.id, brief_id: frozen.id, ordinal: 1, revision: '1', trigger: 'OPERATOR', state: 'RUNNING', outcome: null, budget: frozen.content.budget, reserved_experiments: 0, used_experiments: 1, reserved_cpu_seconds: '0', initial_run_id: run.id, researcher_profile: { profile_id: id(30), expected_revision: '1' }, reviewer_profile: { profile_id: id(31), expected_revision: '1' }, next_action: 'WAITING_FOR_DATA_VALIDATION', started_at: stamp, ended_at: null, created_at: stamp, available_actions: ['VIEW_BRIEF', 'VIEW_RUNS', 'VIEW_EXPERIMENTS'] };
  const runtime: Schema['RuntimeView'] = { id: id(20), revision: '9007199254740993', protocol_version: 1, credential_configured: true, ca_configured: false, created_at: stamp, updated_at: stamp, configuration: { name: 'Controlled Study Runtime', endpoint: 'https://runtime.example', development_http: false, tls_policy: 'SYSTEM_CA', enabled: true, allowed_capabilities: ['PORTFOLIO_BUILD'] } };
const firstAlpha: Schema['AlphaView'] = { id: id(41), project_id: project.id, name: '合成 Alpha 历史', lifecycle: 'RESEARCH',
  active_version_id: id(42), active_version: '2147483647', revision: '9007199254740993', created_at: stamp, updated_at: stamp };
const firstVersion: Schema['AlphaVersionView'] = { id: id(42), project_id: project.id, alpha_id: firstAlpha.id, version: '2147483647',
  experiment_id: id(43), root_lineage_id: project.root_lineage_id, code_artifact_id: id(44), model_artifact_id: id(45),
  signal_contract_version: '1', signal_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '9007199254740993',
  forecast_unit: 'UNITLESS_SCORE', calibration_id: null, runtime_image_ref: 'controlled-original-image', origin: 'FIXTURE', created_at: stamp };

  const alphas = [firstAlpha, { ...firstAlpha, id: id(141), name: '第二个原 Alpha' }];
  const versions = [firstVersion, { ...firstVersion, id: id(142), alpha_id: id(141) }];
  const qualifications: Schema['QualificationView'][] = versions.map((version, i) => ({ id: id(160 + i), alpha_version_id: version.id, policy_id: id(70), qualifying_evaluation_id: id(170 + i), granted_at: stamp, valid_until: '2099-01-01T00:00:00Z', checked_at: stamp, created_at: stamp, grant_window_open: true, revocation: null }));
  const input: Schema['InputSetSummary'] = { id: id(181), project_id: project.id, purpose: 'FORWARD', revision: '1', decision_cutoff: stamp, frozen_at: stamp, created_at: stamp };
  const weights: Schema['DownstreamWeightsViewV1'] = { id: id(182), project_id: project.id, downstream_id: id(183), environment: 'PAPER', report_artifact_id: id(184), received_at: stamp, content: { schema_version: 1, source: { kind: 'FORWARD_SNAPSHOT', downstream_id: id(183), external_message_id: 'controlled-original' }, asof_ns: '9007199254740993', available_ns: '9007199254740994', valid_until_ns: '9007199254740995', base_currency: 'USD', cash_weight: '1', weights: [{ instrument_id: 'CONTROLLED.EXAMPLE', currency: 'USD', weight: '0' }] } };
  const writes: { body: unknown; key: string | null }[] = [];
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname;
    if (path === `/api/v2/projects/${project.id}/portfolio-mandates`) return reply(route, { schema_version: 1, items: [mandate], next_cursor: null });
    if (path === `/api/v2/portfolio-mandates/${mandate.id}`) return reply(route, mandate);
    if (path === `/api/v2/projects/${project.id}/cycles`) return reply(route, { schema_version: 1, items: [cycle], next_cursor: null });
    if (path === `/api/v2/cycles/${cycle.id}`) return reply(route, cycle);
    if (path === '/api/v2/integrations/runtimes') return reply(route, { schema_version: 1, items: [runtime], next_cursor: null });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(route, runtime);
    if (path === '/api/v2/input-sets') return reply(route, { schema_version: 1, items: [input], next_cursor: null });
    if (path === `/api/v2/input-sets/${input.id}`) return reply(route, { header: input, items: [{ id: id(186), ordinal: 0, item: { kind: 'DATASET', dataset_revision_id: id(187), role: 'FORWARD' }, origin: 'FIXTURE', pit_status: null }] });
    if (path.endsWith('/forward-weight-snapshots')) return reply(route, { schema_version: 1, items: url.searchParams.has('cursor') ? [weights] : [], next_cursor: url.searchParams.has('cursor') ? null : id(185) });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [{ ...header, origin: 'SYNTHETIC' }], next_cursor: null });
    if (path === '/api/v2/alphas') return reply(route, { schema_version: 1, items: alphas, next_cursor: null });
    for (const [i, alpha] of alphas.entries()) {
      if (path === `/api/v2/alphas/${alpha.id}/versions`) return reply(route, { schema_version: 1, items: [versions[i]], next_cursor: null });
      if (path === `/api/v2/alpha-versions/${versions[i]!.id}/qualifications`) return reply(route, { schema_version: 1, items: [qualifications[i]], next_cursor: null });
    }
    if (path === '/api/v2/portfolio-builds') {
      writes.push({ body: request.postDataJSON() as unknown, key: await request.headerValue('Idempotency-Key') });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { ...run, cycle_id: cycle.id, kind: 'PORTFOLIO_BUILD', input_set_id: input.id } }, 202);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  async function choose(label: string, value: string) {
    await page.getByRole('combobox', { name: label, exact: true }).click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: value }).click();
  }
  await choose('选择组合所属项目', project.name);
  await page.getByRole('button', { name: '配置 v1', exact: true }).click();
  await page.getByRole('button', { name: '请求组合构建', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '确认请求组合 Build', exact: true });
  const submit = dialog.getByRole('button', { name: '确认请求 Build', exact: true });
  await expect(submit).toBeDisabled();
  await choose('选择 Build Cycle', cycle.id); await choose('选择 Build Runtime', runtime.id); await choose('选择构建输入', input.id);
  if (source === 'LAST_TARGET') {
    await dialog.getByLabel('当前权重来源', { exact: true }).click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: '历史目标假设' }).click();
  } else await dialog.getByRole('button', { name: '载入更多选项', exact: true }).click();
  await choose('选择原权重记录', source === 'LAST_TARGET' ? header.id : weights.id);
  for (let i = 0; i < 2; i++) {
    await choose(`选择成员 ${i + 1} Alpha`, alphas[i]!.id);
    await choose(`选择成员 ${i + 1} 版本`, versions[i]!.id);
    await choose(`选择成员 ${i + 1} 资格`, qualifications[i]!.id);
    await dialog.getByLabel('预测聚合权重', { exact: true }).nth(i).fill(i === 0 ? '0.200000000000000001' : '0.799999999999999999');
  }
  await choose('选择成员 2 Alpha', alphas[0]!.id);
  await expect(dialog.getByRole('combobox', { name: '选择成员 2 资格', exact: true })).toBeDisabled();
  await choose('选择成员 2 版本', versions[0]!.id); await choose('选择成员 2 资格', qualifications[0]!.id);
  await submit.click(); await expect(dialog.getByText('需要 2 至 256 个不同 Alpha 的原资格。', { exact: true })).toBeVisible(); expect(writes).toEqual([]);
  await choose('选择成员 2 Alpha', alphas[1]!.id); await choose('选择成员 2 版本', versions[1]!.id); await choose('选择成员 2 资格', qualifications[1]!.id);
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  await context.setOffline(true); await expect(submit).toBeDisabled(); expect(writes).toEqual([]);
  await context.setOffline(false);
  await dialog.getByLabel('CPU 秒数上限', { exact: true }).fill('9007199254740993');
  await submit.click();
  await expect(dialog.getByText('提交结果未知，请重试当前操作', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('CPU 秒数上限', { exact: true })).toBeDisabled();
  runtime.revision = '9007199254740994';
  await dialog.getByRole('button', { name: '重试同一请求', exact: true }).click();
  await expect(dialog.getByText('构建已提交', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2); expect(writes[1]).toEqual(writes[0]); expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body).toEqual({ schema_version: 1, mandate_id: mandate.id, cycle_id: cycle.id, runtime_id: runtime.id, input_set_id: input.id, environment: 'PAPER', expected_runtime_revision: '9007199254740993', current_weights_source: source === 'LAST_TARGET' ? { kind: source, candidate_id: header.id } : { kind: source, snapshot_id: weights.id }, members: qualifications.map((q, i) => ({ qualification_id: q.id, ensemble_weight: i === 0 ? '0.200000000000000001' : '0.799999999999999999' })), limits: { schema_version: 1, experiments: 0, cpu_seconds: '9007199254740993', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' } });
});
