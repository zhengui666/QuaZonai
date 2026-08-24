import { expect, test, type Page, type Route } from '@playwright/test';

interface HarnessState {
  programs: Array<Record<string, unknown>>;
  approvals: Array<Record<string, unknown>>;
  handoffs: Array<Record<string, unknown>>;
  dataSources: Array<Record<string, unknown>>;
  researchReady: boolean;
}

const base = {
  health: { live: true, ready: true, database: { state: 'READY' }, worker: { state: 'READY' }, agent_worker: { state: 'READY' }, evaluator: { state: 'READY' }, codex: { state: 'READY' }, storage: { state: 'READY' } },
  alphas: [{ id: 'alpha-1', name: 'PEAD residual drift', role: 'PRIMARY_ALPHA', state: 'ACTIVE', degradation_state: 'HEALTHY', universe: 'US Equities', horizon: '1D', metrics: { search_adjusted_quality: .72 } }],
  mandates: [{ id: 'mandate-1', name: 'Core Growth', enabled: true, latest_version_id: 'mv-1', spec_json: { objective: 'Risk-adjusted long-term growth' } }],
  portfolioPrograms: [{ id: 'pp-1', mandate_version_id: 'mv-1', mandate_name: 'Core Growth', state: 'CANDIDATE_READY', candidate_count: 2, current_candidate_id: 'candidate-1' }],
  candidate: { id: 'candidate-1', portfolio_program_id: 'pp-1', mandate_name: 'Core Growth', state: 'READY', members: [{ alpha_qualification_id: 'alpha-1', alpha_name: 'PEAD residual drift', role: 'PRIMARY_ALPHA', target_weight: .45, universe: 'US Equities' }], metrics: { search_adjusted_quality: .78 } },
  downstreams: [{ id: 'downstream-paper', name: 'Paper Lab', environment_type: 'PAPER', enabled: true, preflight_state: 'READY', package_contract_version: '1', feedback_contract_version: '1' }, { id: 'downstream-live', name: 'Live Primary', environment_type: 'LIVE', enabled: true, preflight_state: 'READY', package_contract_version: '1', feedback_contract_version: '1' }],
  universes: [{ id: 'universe-1', universe_key: 'US_EQUITIES', version_no: 1, name: 'US Equities', state: 'ACTIVE' }],
  datasets: [{ id: 'dataset-1', partition: 'DISCOVERY', universe_name: 'US Equities', row_count: 250000, quality_state: 'VALID', point_in_time_state: 'VALID', created_at: '2026-08-24T08:00:00Z' }],
};

function json(route: Route, body: unknown, status = 200) { return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) }); }

async function installHarness(page: Page, state: HarnessState) {
  await page.route('**/api/v1/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (method === 'POST' && path === '/api/v1/ideas/preview') return json(route, { charter: { original_idea_text: 'Test PEAD', research_question: 'Does PEAD persist after realistic costs?', market_scope: 'US Equities', prediction_horizon: '1D', explicit_exclusions: ['microcaps'] }, clarification_required: false, overlap: null });
    if (method === 'POST' && path === '/api/v1/research-programs') {
      const program = { id: 'program-created', title: 'PEAD after costs', state: 'ACTIVE', branch_count: 1, mission_count: 1, alpha_count: 0, charter: { original_idea_text: 'Test PEAD', research_question: 'Does PEAD persist after realistic costs?', market_scope: 'US Equities', prediction_horizon: '1D' } };
      state.programs = [program];
      return json(route, program);
    }
    if (method === 'POST' && path === '/api/v1/approvals/approval-1/approve') {
      state.approvals = state.approvals.map((item) => ({ ...item, state: 'APPROVED' }));
      state.handoffs = [{ id: 'handoff-created', candidate_id: 'candidate-1', purpose: 'PAPER', downstream_name: 'Paper Lab', state: 'AVAILABLE', feedback_state: 'PENDING', claim_deadline: '2026-08-30T00:00:00Z', package_contract_version: '1', feedback_contract_version: '1' }];
      return json(route, { id: 'approval-1', state: 'APPROVED' });
    }
    if (method === 'POST' && path === '/api/v1/data-sources') {
      const payload = request.postDataJSON() as Record<string, unknown>;
      const source = { id: 'source-created', ...payload, state: 'ACTIVE', preflight_state: 'READY' };
      state.dataSources = [source];
      state.researchReady = true;
      return json(route, source);
    }

    if (path === '/api/v1/readiness') return json(route, { SYSTEM_READY: true, RESEARCH_READY: state.researchReady, PAPER_HANDOFF_READY: true, LIVE_HANDOFF_READY: false });
    if (path === '/api/v1/system/health') return json(route, base.health);
    if (path === '/api/v1/research-programs') return json(route, state.programs);
    if (path === '/api/v1/research-programs/program-created') return json(route, state.programs[0]);
    if (path === '/api/v1/research-programs/program-created/missions') return json(route, [{ id: 'mission-created', program_id: 'program-created', type: 'ALPHA_DISCOVERY', role: 'ALPHA_RESEARCHER', state: 'RUNNING', objective: 'Discover cost-adjusted PEAD alpha', dependencies: [] }]);
    if (path === '/api/v1/research-programs/program-created/activity') return json(route, [{ id: 1, kind: 'MISSION_STARTED', created_at: '2026-08-24T08:00:00Z', mission_id: 'mission-created', payload: { summary: 'Alpha discovery started.' } }]);
    if (path === '/api/v1/alpha-library') return json(route, base.alphas);
    if (path === '/api/v1/portfolio-mandates') return json(route, base.mandates);
    if (path === '/api/v1/portfolio-programs') return json(route, base.portfolioPrograms);
    if (path === '/api/v1/portfolio-candidates/candidate-1') return json(route, base.candidate);
    if (path === '/api/v1/approvals') return json(route, state.approvals);
    if (path === '/api/v1/handoffs') return json(route, state.handoffs);
    if (path === '/api/v1/universes') return json(route, base.universes);
    if (path === '/api/v1/datasets') return json(route, base.datasets);
    if (path === '/api/v1/data-sources') return json(route, state.dataSources);
    if (path === '/api/v1/downstream-systems') return json(route, base.downstreams);
    if (path === '/api/v1/plugin-releases') return json(route, []);
    return json(route, { error: { code: 'NOT_FOUND', message: 'Not found' } }, 404);
  });
  await page.route('**/api/v1/events/stream', async (route) => route.fulfill({ status: 200, contentType: 'text/event-stream', body: 'id: 1\ndata: {"kind":"ALPHA_QUALIFIED","created_at":"2026-08-24T08:00:00Z"}\n\n' }));
}

function initialState(): HarnessState {
  return {
    programs: [],
    approvals: [{ id: 'approval-1', candidate_id: 'candidate-1', purpose: 'PAPER', state: 'PENDING', valid_until: '2026-08-30T00:00:00Z', recommendation_rationale: 'Independent evidence is stable and the candidate materially improves the current frontier.', candidate: { id: 'candidate-1', portfolio_program_id: 'pp-1', mandate_name: 'Core Growth', state: 'READY' }, evidence_summary: { search_adjusted_quality: .78 }, risk_summary: { tail_dependence: .23 }, cost_summary: { turnover_cost_bps: 7 }, capacity_summary: { capacity_ratio: .72 }, changes_summary: { changed: 'Risk-adjusted edge improved' }, capital_context: { base_currency: 'USD', deployable_capital: 100000, observed_at: '2026-08-24T08:00:00Z' } }],
    handoffs: [],
    dataSources: [],
    researchReady: false,
  };
}

test('Flow 1: create idea -> research program -> mission appears', async ({ page }) => {
  await installHarness(page, initialState());
  await page.goto('/ideas');
  await page.getByLabel('What should the research system investigate?').fill('Test post-earnings drift in liquid US equities after realistic costs.');
  await page.getByRole('button', { name: 'Preview research charter' }).click();
  await expect(page.getByText('Does PEAD persist after realistic costs?')).toBeVisible();
  await page.getByRole('button', { name: 'Start Research' }).click();
  await expect(page).toHaveURL(/\/research\/program-created$/);
  await expect(page.getByText(/Alpha Discovery · Running/i)).toBeVisible();
});

test('Flow 2: candidate ready -> approve -> handoff available', async ({ page }) => {
  const state = initialState();
  await installHarness(page, state);
  await page.goto('/approval');
  await expect(page.getByText(/materially improves the current frontier/i)).toBeVisible();
  await page.getByRole('button', { name: 'Approve' }).click();
  await page.getByRole('link', { name: 'Handoff Center' }).click();
  await expect(page.getByText('Available', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /stop|undeploy|close position|buy|sell/i })).toHaveCount(0);
});

test('Flow 3: create datasource -> readiness update', async ({ page }) => {
  const state = initialState();
  await installHarness(page, state);
  await page.goto('/admin');
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('NO');
  await page.getByRole('button', { name: /Register data source/ }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill('Primary PIT Data');
  await dialog.getByLabel('Provider').fill('Approved provider');
  await dialog.getByLabel('Canonical fields').fill('event_time, available_time, close, volume');
  await dialog.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Primary PIT Data')).toBeVisible();
  await expect(researchReady).toContainText('YES');
});
