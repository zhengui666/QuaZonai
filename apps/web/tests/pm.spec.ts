import { expect, test, type Page } from '@playwright/test';

// Controlled HTTP presentation and original-command protocol. These fixtures
// deliberately establish no investment performance, model quality or capacity.
async function pmTransport(page: Page) {
  const writes: { path: string; raw: string }[] = []; let starts = 0; let revision = '9007199254740993';
  const view = () => ({
    mandate: { id: 'mandate:test/opaque', revision, ownerGoal: '测试目标：比较净收益与下行风险', baseCurrency: 'USD', operatingAuthority: 'RESEARCH_ONLY', unresolvedIntent: [] },
    activePolicy: {}, lastCycle: { id: 'cycle:test', state: 'COMPLETED', asOfNs: '1790000000000000000' },
    currentDecision: { id: `decision:${revision}`, strategyVersionId: 'strategy:test', ensembleRevisionId: 'ensemble:test', action: 'NEEDS_EVIDENCE', status: 'CANDIDATE',
      accountBasis: { equity: null, valuation: { origin: 'DERIVED', reportValue: { amount: '1215.448835', currency: 'USD' }, asOfNs: '1790000000000000000' } },
      reasonCodes: ['comparable_native_metrics_missing'], validUntilNs: null, capital: null, assetTargets: [], strategyAllocations: [], comparisonRef: null },
    campaigns: [], strategies: [], assessments: [], researchTrials: [], budgetMovements: [], datasetRevisions: [], capabilities: [], comparisons: [], history: [], cycles: [], outcomes: [], retirements: [],
    ensembles: [{ id: 'ensemble:test', members: [{ factorVersionId: 'factor:actual-training-fit', weight: revision === '9007199254740993' ? '0.3' : '0.7', economicFamilyId: 'family:test' }] }],
    capacityEstimates: [{ id: 'capacity:test', level: 'CONDITIONAL_FEASIBILITY', status: 'INSUFFICIENT', asOfNs: '1790000000000000000', minimumTestedCapital: null,
      maximumTestedCapital: null, supportedCapitalUpperBound: null, curve: [], missingEvidence: ['economic_impact_and_exit_capacity_unverified'], assumptions: [] }],
    gaps: [{ id: 'gap:impact', state: 'OPEN', reasonCode: 'economic_impact_and_exit_capacity_unverified', requiredCapability: 'market_impact', permittedRemedy: 'REFRESH_SOURCE' }],
  });
  await page.route('**/api/v3/**', async route => {
    const req = route.request(); const path = new URL(req.url()).pathname.slice('/api/v3'.length);
    const reply = (json: unknown, status = 200) => route.fulfill({ status, json });
    if (req.method() === 'POST') writes.push({ path, raw: req.postData()! });
    if (path === '/session') return reply({ setupRequired: false, authenticated: true });
    if (path === '/datasets') return reply([{ id: 'data:test', name: '测试数据', currency: 'USD', assetClass: 'equity', source: 'test', start: null, end: null, ready: true }]);
    if (path === '/research/models') return reply([{ provider: 'test', modelId: 'test-model', name: '测试模型', reasoning: true, thinkingLevels: ['max'] }]);
    if (path === '/settings') return reply({ defaultModel: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'max', serviceTier: null });
    if (path === '/observations') return reply([]);
    if (path === '/mandates') {
      if (req.method() === 'POST') { if (++starts === 1) return route.abort('failed'); return reply(view(), 202); }
      return reply([view()]);
    }
    if (path === '/mandates/mandate%3Atest%2Fopaque') return reply(view());
    if (path === '/mandates/mandate%3Atest%2Fopaque/recompute') { revision = '9007199254740994'; return reply(view(), 202); }
    return reply({ error: 'not_found' }, 404);
  });
  return { writes, advance: () => { revision = '9007199254740994'; } };
}

test('PM intake preserves one original request after a lost response, without asking for factor weights or capital grids', async ({ page }) => {
  const state = await pmTransport(page); await page.goto('/#/pm/new');
  await page.getByLabel('投资目标', { exact: true }).fill('希望按净收益优先比较，不虚构流动性。');
  await page.getByLabel('研究本金', { exact: true }).fill('3000.000000000000000001');
  await page.getByLabel('研究试验预算', { exact: true }).fill('12');
  await page.getByRole('button', { name: '开始研究' }).click();
  await expect(page.getByRole('button', { name: '核对原提交' })).toBeVisible();
  expect(state.writes).toHaveLength(1);
  const original = JSON.parse(state.writes[0]!.raw);
  expect(original.funding.capital.amount).toBe('3000.000000000000000001');
  expect(original.businessPreference).toBe('FRONTIER');
  expect(original).not.toHaveProperty('weights'); expect(original).not.toHaveProperty('capitalGrid');
  await page.reload();
  await expect(page.getByLabel('投资目标', { exact: true })).toHaveValue(original.goal);
  await page.getByRole('button', { name: '核对原提交' }).click();
  await expect(page.getByRole('heading', { name: '当前研究结论' })).toBeVisible();
  expect(state.writes).toHaveLength(2); expect(state.writes[1]!.raw).toBe(state.writes[0]!.raw);
});

test('PM owner surface retains unknown capacity and refreshes learned proportions with exact revision commands', async ({ page }) => {
  const state = await pmTransport(page); await page.goto('/#/pm/mandate%3Atest%2Fopaque');
  await expect(page.getByRole('heading', { name: '因子组合比例' })).toBeVisible();
  await expect(page.getByText('原余额派生估值', { exact: true })).toBeVisible();
  await expect(page.getByText('1215.448835', { exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '30.00%', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '重算当前证据' }).click();
  await expect(page.getByRole('cell', { name: '70.00%', exact: true })).toBeVisible();
  const command = JSON.parse(state.writes[0]!.raw); expect(command.expectedRevision).toBe('9007199254740993');
  await page.getByRole('tab', { name: '容量与可行资金' }).click();
  await expect(page.getByText('有证据支持的上界', { exact: true })).toBeVisible();
  const capacity = page.locator('.pm-section').filter({ has: page.getByRole('heading', { name: '条件可执行资金' }) });
  await expect(capacity.getByText('未确定', { exact: true })).toHaveCount(3);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});
