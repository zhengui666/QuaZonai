import { expect, test } from '@playwright/test';

const fixtures: Record<string, unknown> = {
  '/api/v1/readiness': { SYSTEM_READY: true, RESEARCH_READY: true, PAPER_HANDOFF_READY: true, LIVE_HANDOFF_READY: false },
  '/api/v1/system/health': { live: true, ready: true, database: { state: 'READY' }, worker: { state: 'READY' }, agent_worker: { state: 'READY' }, evaluator: { state: 'READY' }, codex: { state: 'READY' }, storage: { state: 'READY' } },
  '/api/v1/research-programs': [{ id: 'program-1', title: 'Post-earnings drift', state: 'ACTIVE', branch_count: 3, mission_count: 9, alpha_count: 2, updated_at: '2026-08-24T08:00:00Z', charter: { original_idea_text: 'Study post-earnings drift in liquid US equities.', research_question: 'Does post-earnings drift remain predictive after costs?', market_scope: 'US Equities', prediction_horizon: '1D', explicit_exclusions: ['microcaps'] } }],
  '/api/v1/research-programs/program-1': { id: 'program-1', title: 'Post-earnings drift', state: 'ACTIVE', branch_count: 3, mission_count: 9, alpha_count: 2, charter: { original_idea_text: 'Study post-earnings drift in liquid US equities.', research_question: 'Does post-earnings drift remain predictive after costs?', market_scope: 'US Equities', prediction_horizon: '1D', explicit_exclusions: ['microcaps'] } },
  '/api/v1/research-programs/program-1/missions': [{ id: 'm-1', type: 'HYPOTHESIS', state: 'SUCCEEDED', objective: 'Define drift hypothesis', dependencies: [] }, { id: 'm-2', type: 'ALPHA_DISCOVERY', state: 'RUNNING', objective: 'Evaluate candidate signals', dependencies: ['m-1'] }],
  '/api/v1/research-programs/program-1/activity': [{ id: 1, kind: 'EVIDENCE_UPDATED', created_at: '2026-08-24T08:00:00Z', mission_id: 'm-2', payload: { summary: 'Discovery evidence updated.', ohlc: [{ time: '2026-08-20', open: 100, high: 104, low: 99, close: 103, volume: 1000 }, { time: '2026-08-21', open: 103, high: 106, low: 101, close: 102, volume: 1200 }, { time: '2026-08-22', open: 102, high: 108, low: 102, close: 107, volume: 1500 }] } }],
  '/api/v1/alpha-library': [{ id: 'alpha-1', name: 'PEAD residual drift', role: 'PRIMARY_ALPHA', state: 'ACTIVE', degradation_state: 'HEALTHY', universe: 'US Equities', horizon: '1D', metrics: { search_adjusted_quality: .72 } }],
  '/api/v1/alpha-library/alpha-1': { id: 'alpha-1', name: 'PEAD residual drift', role: 'PRIMARY_ALPHA', state: 'ACTIVE', degradation_state: 'HEALTHY', universe: 'US Equities', horizon: '1D', metrics: { search_adjusted_quality: .72 }, lineage: [{ id: 'model-1', label: 'Alpha model v4', relationship: 'MODEL' }, { id: 'cal-1', label: 'Calibration v2', relationship: 'CALIBRATION' }] },
  '/api/v1/portfolio-mandates': [{ id: 'mandate-1', name: 'Core Growth', enabled: true, latest_version_id: 'mv-1', spec_json: { objective: 'Risk-adjusted long-term growth' } }],
  '/api/v1/portfolio-programs': [{ id: 'pp-1', mandate_version_id: 'mv-1', mandate_name: 'Core Growth', state: 'CANDIDATE_READY', candidate_count: 2, current_candidate_id: 'candidate-1' }],
  '/api/v1/portfolio-candidates/candidate-1': { id: 'candidate-1', portfolio_program_id: 'pp-1', mandate_name: 'Core Growth', state: 'READY', policy_version: 'risk-parity-v2', risk_model_version: 'cross-universe-v3', cost_model_version: 'cost-v4', capacity_model_version: 'capacity-v2', constraint_set_version: 'core-v5', rebalance_policy_version: 'weekly-v2', evaluation_episode_id: 'episode-1', members: [{ alpha_qualification_id: 'alpha-1', alpha_name: 'PEAD residual drift', role: 'PRIMARY_ALPHA', target_weight: .45, universe: 'US Equities' }, { alpha_qualification_id: 'alpha-2', alpha_name: 'Crypto carry diversifier', role: 'DIVERSIFIER_ALPHA', target_weight: .15, universe: 'Crypto Spot' }], metrics: { search_adjusted_quality: .78, redundancy_edges: [] } },
  '/api/v1/approvals': [{ id: 'approval-1', candidate_id: 'candidate-1', purpose: 'PAPER', state: 'PENDING', valid_until: '2026-08-30T00:00:00Z', recommendation_rationale: 'Independent evidence is stable and the candidate materially improves the current frontier.', candidate: { id: 'candidate-1', portfolio_program_id: 'pp-1', mandate_name: 'Core Growth', state: 'READY' }, evidence_summary: { search_adjusted_quality: .78, drawdown_stability: .82 }, risk_summary: { tail_dependence: .23 }, capital_context: { base_currency: 'USD', deployable_capital: 100000, observed_at: '2026-08-24T08:00:00Z' } }],
  '/api/v1/handoffs': [{ id: 'handoff-1', candidate_id: 'candidate-0', purpose: 'PAPER', downstream_name: 'Paper Lab', state: 'CLAIMED', feedback_state: 'IN_PROGRESS', claim_deadline: '2026-08-30T00:00:00Z', package_contract_version: '1', feedback_contract_version: '1' }],
  '/api/v1/universes': [{ id: 'universe-1', universe_key: 'US_EQUITIES', version_no: 1, name: 'US Equities', state: 'ACTIVE' }],
  '/api/v1/datasets': [{ id: 'dataset-1', partition: 'DISCOVERY', universe_name: 'US Equities', row_count: 250000, quality_state: 'VALID', point_in_time_state: 'VALID', created_at: '2026-08-24T08:00:00Z' }],
  '/api/v1/data-sources': [{ id: 'source-1', name: 'Market Data Primary', provider: 'Approved provider', state: 'ACTIVE', preflight_state: 'READY', fields: ['event_time', 'available_time', 'close', 'volume'], update_cadence: '1h' }],
  '/api/v1/downstream-systems': [{ id: 'downstream-paper', name: 'Paper Lab', environment_type: 'PAPER', enabled: true, preflight_state: 'READY', package_contract_version: '1', feedback_contract_version: '1' }, { id: 'downstream-live', name: 'Live Primary', environment_type: 'LIVE', enabled: true, preflight_state: 'READY', package_contract_version: '1', feedback_contract_version: '1' }],
  '/api/v1/plugin-releases': [],
};

async function mockApi(page: import('@playwright/test').Page) {
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    if (route.request().method() !== 'GET') {
      return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ state: 'APPROVED', id: 'mutation-result' }) });
    }
    const body = fixtures[url.pathname];
    return route.fulfill({ status: body === undefined ? 404 : 200, contentType: 'application/json', body: JSON.stringify(body ?? { error: { code: 'NOT_FOUND', message: 'Not found' } }) });
  });
  // Playwright evaluates the most recently registered matching route first.
  await page.route('**/api/v1/events/stream', async (route) => route.fulfill({ status: 200, contentType: 'text/event-stream', body: 'id: 1\ndata: {"kind":"ALPHA_QUALIFIED","created_at":"2026-08-24T08:00:00Z"}\n\n' }));
}

test.beforeEach(async ({ page }) => { await mockApi(page); });

test('core research cockpit navigation and public visualization components render', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Research command center' })).toBeVisible();
  await page.locator('.qz-sidebar').getByRole('link', { name: 'Research' }).click();
  await expect(page.getByText('Post-earnings drift')).toBeVisible();
  await page.getByRole('link', { name: /Open/ }).click();
  await expect(page.getByRole('img', { name: 'Candlestick market data chart' })).toBeVisible();
  await expect(page.getByText('Alpha Discovery · Running')).toBeVisible();
});

test('approval only offers compatible Paper downstream and handoff has no stop control after claim', async ({ page }) => {
  await page.goto('/approvals');
  await expect(page.getByText(/materially improves the current frontier/i)).toBeVisible();
  await page.getByRole('combobox').click();
  const listbox = page.getByRole('listbox');
  await expect(listbox.getByRole('option', { name: /Paper Lab/ })).toBeVisible();
  await expect(listbox.getByRole('option', { name: /Live Primary/ })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.getByRole('link', { name: 'Handoff & Feedback' }).click();
  await expect(page.getByText('Downstream owns runtime')).toBeVisible();
  await expect(page.getByRole('button', { name: /stop|undeploy|close position/i })).toHaveCount(0);
});

test('mobile shell preserves primary actions without desktop sidebar', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.locator('.qz-mobile-nav')).toBeVisible();
  await expect(page.getByRole('link', { name: /Ideas/ })).toBeVisible();
});
