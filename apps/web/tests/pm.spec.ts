import { expect, test, type Page } from '@playwright/test';
import type { ModelView } from '../../../packages/contracts/src/index.ts';

// Controlled HTTP presentation and original-command protocol. These fixtures
// deliberately establish no investment performance, model quality or capacity.
async function pmTransport(page: Page, needsClarification = false, forward = false, fast = false) {
  const standard: ModelView = { provider: 'test', modelId: 'test-model', name: '测试模型', reasoning: true, thinkingLevels: ['max'], effectiveThinkingLevel: 'max', serviceTiers: [] };
  const subscription: ModelView = { provider: 'openai-codex', modelId: 'test-subscription', name: '订阅模型', reasoning: true, thinkingLevels: ['medium', 'high', 'max'], effectiveThinkingLevel: 'medium', serviceTiers: ['default', 'priority'] };
  const preferred = fast ? subscription : standard;
  const deletionPaths: string[] = []; let deleted = false;
  const writes: { path: string; raw: string }[] = []; let starts = 0; let revision = '9007199254740993'; let clarified = !needsClarification; let clarifications = 0;
  const view = () => ({
    mandate: { id: 'mandate:test/opaque', revision, ownerGoal: '测试目标：比较净收益与下行风险', baseCurrency: 'USD', operatingAuthority: forward ? 'AUTHORIZED_TARGETS' : 'RESEARCH_ONLY', unresolvedIntent: clarified ? [] : ['请明确杠杆边界'] },
    activePolicy: {}, lastCycle: { id: 'cycle:test', state: 'COMPLETED', asOfNs: '1790000000000000000' },
    currentDecision: { id: `decision:${revision}`, strategyVersionId: 'strategy:test', ensembleRevisionId: 'ensemble:test', action: forward ? 'REBALANCE' : 'NEEDS_EVIDENCE', status: forward ? 'CURRENT' : 'CANDIDATE',
      accountBasis: { equity: null, valuation: { origin: 'DERIVED', reportValue: { amount: '1215.448835', currency: 'USD' }, asOfNs: '1790000000000000000' } },
      reasonCodes: ['comparable_native_metrics_missing'], validUntilNs: forward ? (BigInt(Date.now() + 60_000) * 1_000_000n).toString() : null, capital: null, assetTargets: forward ? [{ instrumentId: 'ASSET.SIM', navWeight: '0.4', targetNotional: { amount: '400', currency: 'USD' }, currentNotional: { amount: '300', currency: 'USD' }, netTradeNotional: { amount: '100', currency: 'USD' }, executionEstimate: { resultingQuantity: '3.995', resultingNotional: { amount: '399.5', currency: 'USD' }, netTradeQuantity: '0.995', netTradeNotional: { amount: '99.5', currency: 'USD' }, evidenceRef: 'native:test' } }] : [], strategyAllocations: [], comparisonRef: forward ? 'comparison:forward' : null },
    campaigns: [], strategies: forward ? [{ id: 'strategy:test', pipelineAssessmentId: 'assessment:shadow' }] : [], assessments: forward ? [{ id: 'assessment:shadow', strategyVersionId: 'strategy:test', stage: 'SHADOW', outcome: 'READY', reasonCodes: [] }] : [], researchTrials: [], budgetMovements: [], datasetRevisions: [], capabilities: [], comparisons: forward ? [{ id: 'comparison:forward', evaluationContext: 'FORWARD_MODEL_ESTIMATE', scope: { interval: { startNs: '1790000000000000000', endExclusiveNs: '1790000060000000000' } }, candidates: [], selectedCandidateId: null, reasonCodes: [], modeledComparison: { method: 'MATCHED_LINEAR_RETURN_VARIANCE_V1', basis: 'FORWARD_MODEL_ESTIMATE', forecastUnit: 'RETURN_PER_HORIZON', methodPolicyRef: 'policy:test', horizonRef: 'horizon:test', riskAversion: '1', nativeEvidenceRef: 'native:test', startingStateRef: 'state:test', asOfNs: '1790000000000000000', availableAtNs: '1790000000000000000', hold: { grossExpectedReturn: 0.01, linearTurnoverCost: 0, netExpectedReturn: 0.01, variance: 0.001, objectiveValue: -0.009 }, proposed: { grossExpectedReturn: 0.05, linearTurnoverCost: 0.001, netExpectedReturn: 0.049, variance: 0.002, objectiveValue: -0.047 }, targetBlockReason: null, limitations: [] } }] : [], history: [], cycles: [], outcomes: [], retirements: [],
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
    if (path === '/research/models') return reply(fast ? [standard, subscription] : [standard]);
    if (path === '/settings') return reply({ defaultModel: { provider: preferred.provider, modelId: preferred.modelId }, thinkingLevel: 'max', serviceTier: fast ? 'priority' : null });
    if (path === '/observations') return reply([]);
    if (path === '/mandates') {
      if (req.method() === 'POST') { if (++starts === 1) return route.abort('failed'); return reply(view(), 202); }
      return reply(deleted ? [] : [view()]);
    }
    if (path === '/mandates/mandate%3Atest%2Fopaque') {
      if (req.method() === 'DELETE') {
        deletionPaths.push(path);
        if (deletionPaths.length === 1) return reply({ error: 'pm_delete_tasks_active' }, 409);
        deleted = true;
        return reply({ kind: 'mandate', id: 'mandate:test/opaque', deletedAt: '2026-10-25T00:00:00.000Z' });
      }
      return deleted ? reply({ error: 'mandate_not_found' }, 404) : reply(view());
    }
    if (path === '/mandates/mandate%3Atest%2Fopaque/recompute') { revision = '9007199254740994'; return reply(view(), 202); }
    if (path === '/mandates/mandate%3Atest%2Fopaque/intent-clarifications') {
      if (++clarifications === 1) return route.abort('failed');
      clarified = true; revision = '9007199254740994'; return reply(view(), 202);
    }
    return reply({ error: 'not_found' }, 404);
  });
  return { writes, deletionPaths, advance: () => { revision = '9007199254740994'; } };
}

test('PM intake preserves one original request after a lost response, without asking for factor weights or capital grids', async ({ page }) => {
  const state = await pmTransport(page, false, false, true); await page.goto('/#/pm/new');
  await page.getByLabel('投资目标', { exact: true }).fill('希望按净收益优先比较，不虚构流动性。');
  await page.getByLabel('研究本金', { exact: true }).fill('3000.000000000000000001');
  await page.getByLabel('研究试验预算', { exact: true }).fill('12');
  await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理强度', { exact: true }) })).toContainText('最高');
  await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理速度', { exact: true }) })).toContainText('Fast');
  await choose(page, '推理强度', '高'); await choose(page, '推理速度', '标准');
  await page.reload();
  await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理强度', { exact: true }) })).toContainText('高');
  await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理速度', { exact: true }) })).toContainText('标准');
  await page.getByRole('button', { name: '开始研究' }).click();
  await expect(page.getByRole('button', { name: '核对原提交' })).toBeVisible();
  expect(state.writes).toHaveLength(1);
  const original = JSON.parse(state.writes[0]!.raw);
  expect(original.funding.capital.amount).toBe('3000.000000000000000001');
  expect(original.businessPreference).toBe('FRONTIER');
  expect(original).toMatchObject({ model: { provider: 'openai-codex', modelId: 'test-subscription' }, thinkingLevel: 'high', serviceTier: 'default' });
  expect(original).not.toHaveProperty('weights'); expect(original).not.toHaveProperty('capitalGrid');
  await page.reload();
  await expect(page.getByLabel('投资目标', { exact: true })).toHaveValue(original.goal);
  await expect(page.getByLabel('推理强度', { exact: true })).toBeDisabled();
  await expect(page.getByLabel('推理速度', { exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '核对原提交' }).click();
  await expect(page.getByRole('heading', { name: '当前研究结论' })).toBeVisible();
  expect(state.writes).toHaveLength(2); expect(state.writes[1]!.raw).toBe(state.writes[0]!.raw);
});

async function choose(page: Page, label: string, option: string) { await page.getByLabel(label, { exact: true }).click(); await page.getByRole('option', { name: option, exact: true }).click(); }

test('PM switching model clears unsupported speed and resets strength before freezing the command', async ({ page }) => {
  const state = await pmTransport(page, false, false, true); await page.goto('/#/pm/new');
  await choose(page, '推理强度', '高'); await choose(page, '研究模型', '测试模型 / test');
  await expect(page.getByLabel('推理速度', { exact: true })).toBeDisabled();
  await expect(page.getByText('当前模型服务不支持速度配置', { exact: true })).toBeVisible();
  await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理强度', { exact: true }) })).toContainText('最高');
  await page.getByLabel('投资目标', { exact: true }).fill('使用当前模型的可用能力');
  await page.getByLabel('研究本金', { exact: true }).fill('3000');
  await page.getByLabel('研究试验预算', { exact: true }).fill('1');
  await page.getByRole('button', { name: '开始研究' }).click();
  await expect(page.getByRole('button', { name: '核对原提交' })).toBeVisible();
  const original = JSON.parse(state.writes[0]!.raw);
  expect(original).toMatchObject({ model: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'max' });
  expect(original).not.toHaveProperty('serviceTier');
});

test('PM clarification retains the exact owner statement and original command through lost response and reload', async ({ page }) => {
  const state = await pmTransport(page, true); await page.goto('/#/pm/mandate%3Atest%2Fopaque');
  await page.getByText('澄清投资意图', { exact: true }).click();
  await page.getByLabel('补充说明', { exact: true }).fill('仅使用自有资金开展研究。');
  await page.getByLabel('明确为不使用杠杆、不借入资金', { exact: true }).check();
  const ambiguity = page.getByRole('checkbox', { name: '请明确杠杆边界', exact: true });
  await ambiguity.click(); await expect(ambiguity).toBeChecked();
  await page.getByRole('button', { name: '保存意图澄清', exact: true }).click();
  await expect(page.getByRole('button', { name: '核对原澄清请求', exact: true })).toBeVisible();
  const original = state.writes[0]!.raw; const body = JSON.parse(original);
  expect(body.statement).toBe('仅使用自有资金开展研究。\n不使用杠杆');
  expect(body.boundaries).toEqual([{ kind: 'LEVERAGE', sourceStatement: '不使用杠杆', value: { maximumGrossNavRatio: '1', borrowingAllowed: false } }]);
  expect(body.expectedRevision).toBe('9007199254740993');
  expect(body.resolvedAmbiguities).toEqual(['请明确杠杆边界']);
  await page.reload(); await page.getByText('澄清投资意图', { exact: true }).click();
  await expect(page.getByLabel('补充说明', { exact: true })).toHaveValue('仅使用自有资金开展研究。');
  await page.getByRole('button', { name: '核对原澄清请求', exact: true }).click();
  await expect(page.getByText('澄清投资意图', { exact: true })).toHaveCount(0);
  expect(state.writes[1]!.raw).toBe(original);
});

test('PM owner surface retains unknown capacity and refreshes learned proportions with exact revision commands', async ({ page }) => {
  const state = await pmTransport(page); await page.goto('/#/pm/mandate%3Atest%2Fopaque');
  await expect(page.getByRole('heading', { name: '因子组合比例' })).toBeVisible();
  await expect(page.getByText('原余额派生估值', { exact: true })).toBeVisible();
  await expect(page.getByRole('definition').filter({ hasText: '1215.448835 USD' })).toBeVisible();
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

// This controls presentation only; the timestamp and SHADOW assessment below
// are not evidence from a market, account producer or qualification protocol.
test('PM forward presentation separates current modeled targets from historical performance and target authorization', async ({ page }) => {
  await pmTransport(page, false, true); await page.goto('/#/pm/mandate%3Atest%2Fopaque');
  await expect(page.getByRole('heading', { name: '当前模型配置', exact: true })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: '目标金额', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '400 USD', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '300 USD', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '100 USD', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '按交易单位预计的调整', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '399.5 USD', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '99.5 USD', exact: true })).toBeVisible();
  await expect(page.getByText('这是当前模型估计，尚未观测到该区间的收益。', { exact: false })).toBeVisible();
  await expect(page.getByText('历史比较选出的配置', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: '同一预测期限下的模型估计', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '4.90%', exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: '0.002', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '等待运营证据', exact: true })).toBeDisabled();
});


test('PM deletion preserves an opaque identity, permits cancellation and explains an active-task refusal before retry', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const state = await pmTransport(page);
  await page.goto('/#/pm/mandate%3Atest%2Fopaque');
  await page.getByRole('button', { name: '删除投资决策', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click();
  expect(state.deletionPaths).toEqual([]);
  await page.getByRole('button', { name: '删除投资决策', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('mandate:test/opaque');
  await page.getByRole('dialog').getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('投资决策仍有活动任务');
  await expect(page).toHaveURL(/#\/pm\/mandate%3Atest%2Fopaque$/);
  await page.getByRole('dialog').getByRole('button', { name: '重试删除', exact: true }).click();
  await expect(page).toHaveURL(/#\/pm$/); await expect(page.getByRole('heading', { name: '尚无投资目标', exact: true })).toBeVisible();
  expect(state.deletionPaths).toEqual(['/mandates/mandate%3Atest%2Fopaque', '/mandates/mandate%3Atest%2Fopaque']);
});
