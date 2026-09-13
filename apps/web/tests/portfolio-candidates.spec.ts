// Controlled UI snapshots only, not native research or delivery evidence.
import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

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

for (const wrongSubject of [false, true]) test(`candidate evaluations preserve original metrics and reject other subjects: wrong=${wrongSubject}`, async ({ page }) => {
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
      return reply(route, { schema_version: 1, items: [evaluation], next_cursor: evaluation.id });
    }
    if (path === `/api/v2/evaluations/${evaluation.id}`) return reply(route, wrongSubject ? { ...evaluation, subject_candidate_id: id(99) } : evaluation);
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
  const detail = page.getByRole('dialog', { name: '候选保持研究评估', exact: true });
  if (wrongSubject) {
    await expect(detail.getByText('请求未完成，请重试并检查服务状态。', { exact: true })).toBeVisible();
    await expect(detail.getByRole('table')).toHaveCount(0);
  } else {
    await expect(detail.getByText('缺值：PORTFOLIO_DAILY_RETURNS_UNAVAILABLE', { exact: true })).toBeVisible();
    await expect(detail.getByRole('cell', { name: '0', exact: true })).toBeVisible();
    await expect(detail.getByRole('cell', { name: '252', exact: true }).first()).toBeVisible();
    await expect(detail.getByText('nautilus-analysis.SharpeRatio / 0.63.0', { exact: true }).first()).toBeVisible();
    await expect(detail.getByText(/当时没有未过期有效期/)).toBeVisible();
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
  await expect(detail.getByText('历史状态不授予当前资格', { exact: true })).toBeVisible();
  await expect(detail.getByText('OPTIMAL', { exact: true })).toBeVisible();
  await expect(detail.getByText('0.123456789012345678', { exact: true }).first()).toBeVisible();
  if (empty) await expect(detail.getByText('无目标，不补造权重或现金。', { exact: true })).toBeVisible();
  else await expect(detail.getByText('CONTROLLED.EXAMPLE', { exact: true })).toBeVisible();
  await expect(detail.getByRole('button', { name: /审批|交付/ })).toHaveCount(0);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
});
