// Controlled browser contracts, not a claim of real-market or native execution.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

const candidate: Schema['CandidateViewV1'] = {
  id: id(81), project_id: project.id, mandate_id: id(80), input_set_id: id(82), run_id: id(83),
  decision_asof: '2026-01-01T00:00:00Z', created_at: '2026-01-01T00:01:00Z',
  execution_status: 'SUCCEEDED', solver_status: 'OPTIMAL', evidence_status: 'VALID', origin: 'FIXTURE', reason_code: null,
  forecast_artifact_id: id(84), covariance_artifact_id: null, diagnostics_artifact_id: id(85), target_artifact_id: id(86),
  allocation_evaluation_id: null, cash_weight: '0', current_weights_source: 'FORWARD_SNAPSHOT', current_weights_artifact_id: id(87),
};
const evaluation: Schema['EvaluationView'] = {
  id: id(91), project_id: project.id, subject_alpha_version_id: null, subject_candidate_id: candidate.id,
  input_set_id: id(92), policy_id: id(93), run_id: id(94), evaluation_kind: 'PORTFOLIO',
  execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'REJECT',
  report_artifact_id: id(95), method_versions_artifact_id: id(95), origin: 'FIXTURE',
  concluded_at: '2026-01-03T00:00:00Z', valid_until: '2026-01-04T00:00:00Z', checked_at: '2026-09-20T00:00:00Z', unexpired_at_read: false,
};
function curve(): Schema['EquityCurveV1'] {
  return { schema_version: 1, project_id: project.id, candidate_id: candidate.id, evaluation_id: evaluation.id, run_id: evaluation.run_id, origin: 'FIXTURE',
    curve: { status: 'READY', source_artifact_id: id(96), series: {
      native_version: '0.63.0', base_currency: 'USD', starting_capital: '1000',
      period_start_ns: '1767225600000000000', period_end_ns: '1767398400000000000',
      source_point_count: '4', window_point_count: '4', resolution: 'NATIVE', sampled: false,
      points: [
        { timestamp_ns: '1767225600000000000', value: '1000', reason_code: null },
        { timestamp_ns: '1767225600123456789', value: '999.123456789012345678', reason_code: null },
        { timestamp_ns: '1767312000000000000', value: '0', reason_code: null },
        { timestamp_ns: '1767398400000000000', value: '-1.25', reason_code: null },
      ],
    } } };
}
async function enter(page: Page) {
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: candidate.id, exact: true }).click();
  await page.getByRole('button', { name: `评估 ${evaluation.id.slice(-8)}`, exact: true }).click();
  return page.getByRole('region', { name: '历史回测组合价值' });
}
async function setup(page: Page, response: (url: URL) => Schema['EquityCurveV1']) {
  const state = await fixture(page);
  const reads: URL[] = [];
  await page.route('**/api/v2/**', async route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path.endsWith('/portfolio-candidates')) return reply(route, { schema_version: 1, items: [candidate], next_cursor: null });
    if (path === `/api/v2/portfolio-candidates/${candidate.id}`) return reply(route, { header: candidate, members: [], targets: [] });
    if (path === `/api/v2/portfolio-candidates/${candidate.id}/evaluations`) return reply(route, { schema_version: 1, items: [evaluation], next_cursor: null });
    if (path === `/api/v2/evaluations/${evaluation.id}`) return reply(route, evaluation);
    if (path === `/api/v2/evaluations/${evaluation.id}/metrics`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === `/api/v2/evaluations/${evaluation.id}/equity-curve`) {
      expect(route.request().method()).toBe('GET'); reads.push(url); return reply(route, response(url));
    }
    return route.fallback();
  });
  return { state, reads };
}

for (const mode of ['light', 'dark'] as const) test(`real chart library preserves accessible historical evidence in ${mode}`, async ({ page, context }, info) => {
  await page.addInitScript(value => localStorage.setItem('quazonai.theme', value), mode);
  const { state, reads } = await setup(page, () => curve());
  const section = await enter(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
  await expect(section.getByRole('heading', { name: '组合价值（USD）' })).toBeVisible();
  await expect(section.getByTestId('equity-chart').locator('canvas')).toHaveCount(1);
  await section.getByText('明细（4）', { exact: true }).click();
  await expect(section.getByRole('cell', { name: '999.123456789012345678', exact: true })).toBeVisible();
  await expect(section.getByRole('cell', { name: '2026-01-01T00:00:00.123456789Z', exact: true })).toBeVisible();
  await expect(section.getByRole('cell', { name: '0', exact: true })).toBeVisible();
  await expect(section.getByRole('cell', { name: '-1.25', exact: true })).toBeVisible();
  const box = await section.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  await page.screenshot({ path: info.outputPath('equity-fixture.png'), fullPage: true });
  await context.setOffline(true);
  await expect(section.getByText('离线，权益数据暂不可读取', { exact: true })).toBeVisible();
  await expect(section.getByRole('button', { name: '查看区间' })).toBeDisabled();
  await context.setOffline(false);
  await expect(section.getByTestId('equity-chart').locator('canvas')).toHaveCount(1);
  expect(reads.length).toBeGreaterThan(0);
  expect(state.commands).toEqual([]);
  expect((await new AxeBuilder({ page }).include('section[aria-label="历史回测组合价值"]').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
});

for (const reason of ['LEGACY_SNAPSHOTS_UNAVAILABLE', 'NO_SIMULATION', 'SIMULATION_FAILED'] as const) test(`missing evidence is not a fabricated curve: ${reason}`, async ({ page }) => {
  await setup(page, () => ({ ...curve(), curve: { status: 'UNAVAILABLE', reason_code: reason } }));
  const section = await enter(page);
  const label = { LEGACY_SNAPSHOTS_UNAVAILABLE: '此历史回测没有权益快照', NO_SIMULATION: '本次研究未产生权益数据', SIMULATION_FAILED: '回测未完成' }[reason];
  await expect(section.getByText(label, { exact: true })).toBeVisible();
  await expect(section.getByTestId('equity-chart')).toHaveCount(0);
});

test('wrong run cannot appear as the selected curve', async ({ page }) => {
  await setup(page, () => ({ ...curve(), run_id: id(777) }));
  const section = await enter(page);
  await expect(section.getByText('权益数据与当前回测不一致。', { exact: true })).toBeVisible();
  await expect(section.getByTestId('equity-chart')).toHaveCount(0);
});

test('keyboard resolution selection and complete-range reset use the bounded read API', async ({ page }) => {
  const { reads } = await setup(page, url => {
    const value = curve();
    if (value.curve.status === 'READY' && url.searchParams.get('resolution') === 'DAY') value.curve.series.resolution = 'DAY';
    return value;
  });
  const section = await enter(page);
  const select = section.getByRole('combobox', { name: '权益显示粒度' });
  await select.focus(); await select.press('ArrowDown'); await select.press('ArrowDown'); await select.press('Enter');
  await expect.poll(() => reads.some(url => url.searchParams.get('resolution') !== 'AUTO')).toBe(true);
  await section.getByRole('button', { name: '完整区间', exact: true }).click();
  await expect(section.getByText('原始 · UTC', { exact: true })).toBeVisible();
  await expect(section.getByTestId('equity-chart').locator('canvas')).toHaveCount(1);
});
