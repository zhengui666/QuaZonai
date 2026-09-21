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

const evaluation: Schema['EvaluationView'] = {
  id: id(91), project_id: project.id, subject_alpha_version_id: null, subject_candidate_id: header.id,
  input_set_id: id(92), policy_id: id(93), run_id: id(94), evaluation_kind: 'FORWARD',
  execution_status: 'SUCCEEDED', evidence_status: 'INCOMPLETE', decision: 'INCONCLUSIVE',
  report_artifact_id: id(95), method_versions_artifact_id: id(95), origin: 'FIXTURE',
  concluded_at: header.created_at, valid_until: '2026-09-13T00:05:00Z', checked_at: '2026-09-14T00:00:00Z', unexpired_at_read: false,
};
const metric: Schema['MetricValueV1'] = {
  schema_version: 1, evaluation_id: evaluation.id, metric_code: 'PORTFOLIO_SHARPE_RATIO', scope: 'portfolio', value: 0,
  status: 'OK', reason_code: null, unit: 'RATIO', frequency: 'UTC_DAY', annualization_factor: 252,
  period_start: header.decision_asof, period_end: header.created_at, observation_count: '2',
  method_id: 'nautilus-analysis.SharpeRatio', method_version: '0.63.0', source_artifact_id: id(96), higher_is_better: true,
};

for (const hasPlan of [false, true]) test(`Study requires a frozen plan and retries exact bounded intent: plan=${hasPlan}`, async ({ page, context }) => {
  await fixture(page);
  const stamp = header.created_at;
  const frozen = brief();
  const requirement: Schema['MetricRequirementV1'] = { schema_version: 1, metric_code: 'PORTFOLIO_SHARPE_RATIO', scope: 'portfolio', comparator: 'GE', threshold_low: '0', threshold_high: null, minimum_observations: '2', required: true, method_allowlist: ['nautilus-analysis.SharpeRatio'] };
  const policy: Schema['EvaluationPolicyView'] = { id: id(70), project_id: project.id, version: 1, created_at: stamp, question: 'Controlled Study UI',
    metric_requirements: [requirement], portfolio_metric_requirements: [requirement], minimum_observations: 2, maximum_missing_fraction: '0', maximum_sealed_uses_per_lineage: 1, require_real_data: true, required_capabilities: [], validity_seconds: '60',
    portfolio_study_plan: hasPlan ? { schema_version: 1, input_set_id: id(71), evaluation_start: stamp, manual_cutoffs: [stamp, '2026-09-13T00:02:00Z'] } : null,
    selection_rule: { schema_version: 1, family_id: id(72), root_lineage_id: project.root_lineage_id, comparable_scope: 'FAMILY_LINEAGE', comparison_input_set_id: id(73), execution_assumptions_id: id(74), evaluation_kind: 'WALK_FORWARD', metric_code: 'IC', metric_scope: 'asset:0', method_id: 'controlled', method_version: '1', unit: 'SCORE', frequency: 'DAY', direction: 'MAXIMIZE', candidate_count: 2, missing_required_metric: 'INCONCLUSIVE', tie_break: 'EXPERIMENT_ID_ASC' },
    split_policy: { schema_version: 1, kind: 'WALK_FORWARD', train_size: '2', test_size: '1', step_size: '1', purge_observations: '0', embargo_observations: '0', sealed_revision_id: id(75), interval_validation_required: true } };
  const mandate: Schema['MandateViewV1'] = { id: header.mandate_id, project_id: project.id, version: 1, created_at: stamp,
    content: { base_currency: 'USD', capital_assumption: '1000', universe_version_id: id(76), required_evaluation_policy_id: policy.id, execution_assumptions_id: id(74), objective: 'MIN_RISK', risk_measure: 'VARIANCE', exposure_tolerance: '0.000001',
      constraints: { schema_version: 1, long_only: true, min_cash_weight: '0', max_cash_weight: '1', min_asset_weight: '0', max_asset_weight: '1', max_gross_exposure: '1', min_net_exposure: '0', max_net_exposure: '1', max_turnover_per_rebalance: '2', group_bounds: [], asset_overrides: [], transaction_costs_ref: id(77) },
      rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, session_offset_seconds: null, timezone: 'UTC', max_input_age_seconds: 60, target_ttl_seconds: 300 },
      covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
      alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
      optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1', parameters: { schema_version: 1, risk_aversion: '1', max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false, cvar_confidence: null, risk_budgeting: null } } } };
  const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: id(40), project_id: project.id, brief_id: frozen.id, ordinal: 1, revision: '1', trigger: 'OPERATOR', state: 'RUNNING', outcome: null, budget: frozen.content.budget, reserved_experiments: 0, used_experiments: 1, reserved_cpu_seconds: '0', initial_run_id: run.id, researcher_profile: { profile_id: id(30), expected_revision: '1' }, reviewer_profile: { profile_id: id(31), expected_revision: '1' }, next_action: 'WAITING_FOR_DATA_VALIDATION', started_at: stamp, ended_at: null, created_at: stamp, available_actions: ['VIEW_BRIEF', 'VIEW_RUNS', 'VIEW_EXPERIMENTS'] };
  const runtime: Schema['RuntimeView'] = { id: id(20), revision: '9007199254740993', protocol_version: 1, credential_configured: true, ca_configured: false, created_at: stamp, updated_at: stamp, configuration: { name: 'Controlled Study Runtime', endpoint: 'https://runtime.example', development_http: false, tls_policy: 'SYSTEM_CA', enabled: true, allowed_capabilities: ['PORTFOLIO_SIMULATE'] } };
  const writes: { body: unknown; key: string | null }[] = [];
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [header], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${header.id}`) return reply(route, { header, members: [], targets: [] });
    if (path === `/api/v2/portfolio-candidates/${header.id}/evaluations`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === `/api/v2/portfolio-mandates/${mandate.id}`) return reply(route, mandate);
    if (path === `/api/v2/evaluation-policies/${policy.id}`) return reply(route, policy);
    if (path === `/api/v2/projects/${project.id}/cycles`) return reply(route, { schema_version: 1, items: [cycle], next_cursor: null });
    if (path === `/api/v2/cycles/${cycle.id}`) return reply(route, cycle);
    if (path === '/api/v2/integrations/runtimes') return reply(route, { schema_version: 1, items: [runtime], next_cursor: null });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(route, runtime);
    if (path === '/api/v2/portfolio-studies') {
      writes.push({ body: request.postDataJSON() as unknown, key: await request.headerValue('Idempotency-Key') });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { ...run, cycle_id: cycle.id, kind: 'PORTFOLIO_SIMULATE', input_set_id: id(71) } }, 202);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: header.id, exact: true }).click();
  await page.getByRole('button', { name: '请求组合 Study', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '确认请求组合 Study', exact: true });
  const submit = dialog.getByRole('button', { name: '确认请求 Study', exact: true });
  await expect(submit).toBeDisabled();
  for (const [label, value] of [['选择 Study Cycle', cycle.id], ['选择 Study Runtime', runtime.id]]) {
    await dialog.getByRole('combobox', { name: label, exact: true }).click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: value }).click();
  }
  if (!hasPlan) {
    await expect(dialog.getByText('原政策没有 Study 计划，不能启动研究。', { exact: true })).toBeVisible();
    await expect(submit).toBeDisabled(); expect(writes).toEqual([]); return;
  }
  await expect(submit).toBeEnabled();
  await context.setOffline(true);
  await expect(submit).toBeDisabled();
  await context.setOffline(false);
  await dialog.getByLabel('CPU 秒数上限', { exact: true }).fill('9007199254740993');
  await submit.click();
  await expect(dialog.getByText('提交结果未知，请重试当前操作', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('CPU 秒数上限', { exact: true })).toBeDisabled();
  runtime.revision = '9007199254740994';
  await dialog.getByRole('button', { name: '重试同一请求', exact: true }).click();
  await expect(dialog.getByText('研究已提交', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2); expect(writes[1]).toEqual(writes[0]); expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body).toEqual({ schema_version: 1, candidate_id: header.id, cycle_id: cycle.id, runtime_id: runtime.id, expected_runtime_revision: '9007199254740993', limits: { schema_version: 1, experiments: 0, cpu_seconds: '9007199254740993', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' } });
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
});

for (const kind of ['FORWARD', 'PORTFOLIO'] as const) for (const wrongSubject of [false, true]) test(`candidate evaluations preserve original metrics and reject other subjects: kind=${kind}, wrong=${wrongSubject}`, async ({ page }) => {
  const original = { ...evaluation, evaluation_kind: kind };
  await fixture(page);
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [header], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${header.id}`) return reply(route, { header, members: [], targets: [] });
    if (path === `/api/v2/portfolio-candidates/${header.id}/evaluations`) {
      expect(route.request().method()).toBe('GET');
      if (url.searchParams.has('cursor')) return route.abort('failed');
      return reply(route, { schema_version: 1, items: [original], next_cursor: evaluation.id });
    }
    if (path === `/api/v2/evaluations/${evaluation.id}`) return reply(route, wrongSubject ? { ...original, subject_candidate_id: id(99) } : original);
    if (path === `/api/v2/evaluations/${evaluation.id}/metrics`) {
      expect(wrongSubject).toBe(false);
      return reply(route, { schema_version: 1, items: [metric, { ...metric, metric_code: 'MISSING_SHARPE', value: null, status: 'INSUFFICIENT_DATA', reason_code: 'PORTFOLIO_DAILY_RETURNS_UNAVAILABLE' }], next_cursor: null });
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: header.id, exact: true }).click();
  const candidate = page.getByRole('dialog', { name: '不可变候选快照', exact: true });
  await candidate.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(candidate.getByRole('button', { name: '上一页', exact: true })).toBeEnabled();
  await candidate.getByRole('button', { name: '上一页', exact: true }).click();
  await candidate.getByRole('button', { name: `评估 ${evaluation.id.slice(-8)}`, exact: true }).click();
  const detail = page.getByRole('dialog', { name: '候选研究评估', exact: true });
  if (wrongSubject) {
    await expect(detail.getByText('请求失败，请重试', { exact: true })).toBeVisible();
    await expect(detail.getByRole('table')).toHaveCount(0);
  } else {
    await expect(detail.getByText(kind, { exact: true })).toBeVisible();
    await expect(detail.getByText('缺值：PORTFOLIO_DAILY_RETURNS_UNAVAILABLE', { exact: true })).toBeVisible();
    await expect(detail.getByRole('cell', { name: '0', exact: true })).toBeVisible();
    await expect(detail.getByRole('cell', { name: '252', exact: true }).first()).toBeVisible();
    await expect(detail.getByText('nautilus-analysis.SharpeRatio / 0.63.0', { exact: true }).first()).toBeVisible();
    await expect(detail.getByText(/无有效期/)).toBeVisible();
    await expect(detail.getByRole('button', { name: /审批|交付|模拟/ })).toHaveCount(0);
    expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  }
});

test('candidate pagination can return to original page after the next read fails', async ({ page }) => {
  await fixture(page);
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (url.pathname.endsWith('/portfolio-candidates')) {
      if (url.searchParams.has('cursor')) return route.abort('failed');
      return reply(route, { schema_version: 1, items: [header], next_cursor: header.id });
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  const panel = page.getByRole('tabpanel', { name: '候选快照' });
  await panel.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(panel.getByRole('button', { name: '上一页', exact: true })).toBeEnabled();
  await panel.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(panel.getByRole('button', { name: header.id, exact: true })).toBeVisible();
});

for (const empty of [false, true]) test(`candidate snapshot preserves original facts without authorizing delivery: empty=${empty}`, async ({ page }) => {
  await fixture(page);
  const original: Schema['CandidateDetailV1'] = {
    header: empty ? { ...header, evidence_status: 'INVALID', reason_code: 'PORTFOLIO_SOURCE_NO_LONGER_ELIGIBLE', target_artifact_id: null, cash_weight: null } : header,
    members: [{ alpha_version_id: id(88), qualification_id: id(89), ensemble_weight: '0.123456789012345678', calibration_id: null, forecast_unit: 'RETURN_PER_HORIZON', coverage_fraction: '1' }],
    targets: empty ? [] : [{ instrument_id: 'CONTROLLED.EXAMPLE', target_weight: '0.123456789012345678', currency: 'USD', asof: header.decision_asof, valid_until: '2026-09-13T00:05:00Z' }],
  };
  await page.route('**/api/v2/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [original.header], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${header.id}/evaluations`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${header.id}`) { expect(route.request().method()).toBe('GET'); return reply(route, original); }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: header.id, exact: true }).click();
  const detail = page.getByRole('dialog', { name: '不可变候选快照', exact: true });
  await expect(detail.getByText('历史状态不授予当前资格', { exact: true })).toHaveCount(0);
  await expect(detail.getByText('OPTIMAL', { exact: true })).toBeVisible();
  await expect(detail.getByText('0.123456789012345678', { exact: true }).first()).toBeVisible();
  if (empty) await expect(detail.getByText('暂无目标', { exact: true })).toBeVisible();
  else await expect(detail.getByText('CONTROLLED.EXAMPLE', { exact: true })).toBeVisible();
  await expect(detail.getByRole('button', { name: /审批|交付/ })).toHaveCount(0);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
});

for (const kind of ['PORTFOLIO', 'FORWARD'] as const) test(`Release freezes exact independent evaluation with lost-response retry: ${kind}`, async ({ page, context }) => {
  await fixture(page);
  const original: Schema['EvaluationView'] = { ...evaluation, evaluation_kind: kind, execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'PASS', origin: 'REAL', valid_until: '2030-09-13T00:00:00Z', unexpired_at_read: true };
  const release: Schema['ReleaseViewV1'] = { id: id(101), project_id: project.id, candidate_id: header.id, mandate_id: header.mandate_id, evaluation_id: original.id, package_artifact_id: id(102), package_schema_version: '1', market_capability_version: 'controlled-market/1', environment: 'REAL', asof: header.decision_asof, valid_from: header.created_at, valid_until: original.valid_until!, created_at: header.created_at };
  const writes: { body: unknown; key: string | null }[] = [];
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [{ ...header, origin: 'REAL' }], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${header.id}`) return reply(route, { header: { ...header, origin: 'REAL' }, members: [], targets: [] });
    if (path === `/api/v2/portfolio-candidates/${header.id}/evaluations`) return reply(route, { schema_version: 1, items: [original], next_cursor: null });
    if (path === '/api/v2/releases' && request.method() === 'POST') {
      writes.push({ body: request.postDataJSON() as unknown, key: await request.headerValue('Idempotency-Key') });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: release }, 201);
    }
    if (path === `/api/v2/releases/${release.id}`) return reply(route, release);
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: header.id, exact: true }).click();
  const freeze = page.getByRole('button', { name: '冻结目标包', exact: true });
  if (kind === 'FORWARD') { await expect(freeze).toBeDisabled(); expect(writes).toEqual([]); return; }
  await freeze.click();
  const dialog = page.getByRole('dialog', { name: '确认冻结目标包', exact: true });
  const submit = dialog.getByRole('button', { name: '确认冻结 Release', exact: true });
  await context.setOffline(true); await expect(submit).toBeDisabled();
  await context.setOffline(false); await submit.click();
  await expect(dialog.getByText('提交结果未知，请重试当前操作', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '重试同一冻结请求', exact: true }).click();
  await expect(dialog.getByText('原目标包已冻结。', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2); expect(writes[1]).toEqual(writes[0]); expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body).toEqual({ schema_version: 1, candidate_id: header.id, evaluation_id: original.id });
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  await dialog.getByRole('button', { name: '查看原目标包', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '原始目标包版本', exact: true });
  await expect(detail.getByText(release.package_artifact_id, { exact: true })).toBeVisible();
});
