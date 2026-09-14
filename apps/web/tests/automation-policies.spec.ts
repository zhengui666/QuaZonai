// Controlled UI contracts, not actual automation authorization evidence.
import { expect, test } from '@playwright/test';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

test('Automation authorization preserves project revision and independent requirements', async ({ page }) => {
  await fixture(page);
  const mandate: Schema['MandateViewV1'] = { id: id(80), project_id: project.id, version: 1, created_at: '2026-09-13T00:00:00Z',
    content: { base_currency: 'USD', capital_assumption: '1000', universe_version_id: id(76), required_evaluation_policy_id: id(70), execution_assumptions_id: id(74), objective: 'MIN_RISK', risk_measure: 'VARIANCE', exposure_tolerance: '0.000001',
      constraints: { schema_version: 1, long_only: true, min_cash_weight: '0', max_cash_weight: '1', min_asset_weight: '0', max_asset_weight: '1', max_gross_exposure: '1', min_net_exposure: '0', max_net_exposure: '1', max_turnover_per_rebalance: '2', group_bounds: [], asset_overrides: [], transaction_costs_ref: id(77) },
      rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, session_offset_seconds: null, timezone: 'UTC', max_input_age_seconds: 60, target_ttl_seconds: 300 },
      covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
      alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
      optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1', parameters: { schema_version: 1, risk_aversion: '1', max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false, cvar_confidence: null, risk_budgeting: null } } } };

  const down: Schema['DownstreamView'] = { id: id(85), configuration: { name: 'Automation target', endpoint: 'https://downstream.invalid', accepted_package_versions: ['1'], environments: 'BOTH', enabled: true, development_http: false }, credential_configured: true, revision: '7', created_at: mandate.created_at, updated_at: mandate.created_at };
  const writes: { body: Schema['AutomationAuthorizeV1']; key: string | undefined }[] = [];
  await page.route('**/api/v2/**', route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === `/api/v2/projects/${project.id}`) return reply(route, { ...project, revision: '9007199254740993' });
    if (path === `/api/v2/projects/${project.id}/portfolio-mandates`) return reply(route, { schema_version: 1, items: [mandate], next_cursor: null });
    if (path === '/api/v2/integrations/downstreams') return reply(route, { schema_version: 1, items: [down], next_cursor: null });
    if (path === `/api/v2/projects/${project.id}/automation-policies`) {
      if (request.method() === 'GET') return reply(route, { schema_version: 1, items: [], next_cursor: null });
      const body = request.postDataJSON() as Schema['AutomationAuthorizeV1']; writes.push({ body, key: request.headers()['idempotency-key'] });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { id: id(800), project_id: project.id, content: body.content, created_at: mandate.created_at, authorized_at: mandate.created_at } }, 201);
    }
    return route.fallback();
  });
  const choose = async (label: string, option: string) => {
    await page.getByLabel(label, { exact: true }).click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: option }).click();
  };
  await page.goto('/'); await navigate(page, '交付'); await choose('选择交付所属项目', project.name);
  await page.getByRole('tab', { name: '自动化政策', exact: true }).click();
  await page.getByRole('button', { name: '冻结自动化政策', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: '冻结自动化政策', exact: true });
  await choose('自动化模式', 'AUTO_HANDOFF'); await choose('原组合配置', mandate.id); await choose('原下游', down.configuration.name);
  await page.getByLabel('每流最少 Paper 观察数', { exact: true }).fill('2');
  await page.getByLabel('每日最多候选再平衡数', { exact: true }).fill('3');
  await page.getByLabel('Paper 最少经过秒数', { exact: true }).fill('9007199254740993');
  await page.getByLabel('反馈最大年龄秒数', { exact: true }).fill('3600');
  for (const [title, threshold] of [['晋级指标', '0.1'], ['维持指标', '-0.1']]) {
    await page.getByLabel(`${title} 1 指标代码`, { exact: true }).fill('MEAN');
    await page.getByLabel(`${title} 1 Scope`, { exact: true }).fill('portfolio');
    await choose(`${title} 1 比较器`, 'GE');
    await page.getByLabel(`${title} 1 下端点`, { exact: true }).fill(threshold!);
    await page.getByLabel(`${title} 1 最少样本`, { exact: true }).fill('2');
    const method = page.getByLabel(`${title} 1 方法白名单`, { exact: true }); await method.fill('controlled/1'); await method.press('Enter'); await method.press('Escape');
  }
  await page.getByLabel('授权截止时间（本地时间）', { exact: true }).fill('2098-01-01T00:00');
  await drawer.getByRole('button', { name: '确认冻结政策', exact: true }).click();
  await drawer.getByRole('button', { name: '重试同一政策', exact: true }).click();
  await expect(drawer.getByText('原自动化政策已冻结。', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2); expect(writes[0]).toEqual(writes[1]);
  expect(writes[0]!.body.expected_project_revision).toBe('9007199254740993');
  expect(writes[0]!.body.content.minimum_paper_elapsed_seconds).toBe('9007199254740993');
  expect(writes[0]!.body.content.promotion_metric_requirements[0]!.threshold_low).toBe('0.1');
  expect(writes[0]!.body.content.degradation_metric_requirements[0]!.threshold_low).toBe('-0.1');
  expect(writes[0]!.body.content.enabled_for_new_rebalances).toBe(false);
});
