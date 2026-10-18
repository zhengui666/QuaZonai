import { expect, test, type Page as BrowserPage, type Route } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFile } from 'node:fs/promises';
import type { DatasetView, ExtensionView, ModelLoginView, ModelView, ObservationView, PortfolioView, ProviderView, ResearchDetail, ResearchSummary, SessionView, SettingsView, UsageView, WorkspaceView } from '../../../packages/contracts/src/index.ts';

// Controlled transport only. Real HTTP/durable integration is in real-service.spec.ts.
async function contract(page: BrowserPage, setup = false) {
  const at = '2026-10-01T08:00:00Z';
  const model: ModelView = { provider: 'test', modelId: 'test-model', name: '测试模型', reasoning: true, thinkingLevels: ['off', 'low', 'medium', 'high'], effectiveThinkingLevel: 'medium' };
  let availableModels = [model];
  const dataset: DatasetView = { id: 'test-data', name: '测试数据', assetClass: 'equity', currency: 'CNY', source: 'test', start: null, end: null, ready: true };
  let session: SessionView = { setupRequired: setup, authenticated: !setup };
  let settings: SettingsView = { defaultModel: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'medium' };
  const workspace: WorkspaceView = { name: 'QuaZonai', connections: [{ id: 'paper-test', name: '测试模拟连接', mode: 'paper', connected: false }, { id: 'live-test', name: '测试实盘连接', mode: 'live', connected: false }] };
  const providers: ProviderView[] = [{ id: 'test', name: '测试模型服务', configured: true, authTypes: ['api_key'], login: null, error: null }, { id: 'new', name: '待连接服务', configured: false, authTypes: ['api_key', 'oauth'], login: null, error: null }];
  let login: ModelLoginView | null = null; let authAttempts = 0; let missingLogin = false;
  let studies: ResearchSummary[] = [
    { id: 7, title: '测试 · 低波动与动量', status: 'RUNNING', trials: 2, model: 'test/test-model', createdAt: at, updatedAt: at },
    { id: 8, title: '测试 · 横截面价值', status: 'COMPUTED', trials: 3, model: 'test/test-model', createdAt: at, updatedAt: at },
    { id: 9, title: '测试 · ' + 'LongResearchName'.repeat(7), status: 'FAILED', trials: 1, model: 'test/test-model', createdAt: null, updatedAt: null },
  ];
  const stopped = new Set<number>(); const inactive = new Set<number>(); const admitted = new Map<string, { id: number; raw: string; input: Record<string, any> }>();
  const finalMetrics = [{ key: 'HELDOUT.pearson_ic', label: 'Pearson IC', value: null, status: 'CONSTANT_SCORE', partition: 'HELDOUT', component: 0 }, { key: 'HELDOUT.counts.included', label: 'Included', value: '9007199254740995', partition: 'HELDOUT', component: 0 }, { key: 'HELDOUT.counts.excluded', label: 'Excluded', value: '0', partition: 'HELDOUT', component: 0 }];
  const detail = (id: number): ResearchDetail | undefined => {
    const summary = studies.find(item => item.id === id); if (!summary) return;
    const original = [...admitted.values()].find(item => item.id === id)?.input; const active = !stopped.has(id) && !inactive.has(id) && (id === 7 || !!original); const unanswered = id === 9 || inactive.has(id);
    return { ...summary, status: stopped.has(id) ? 'STOPPED' : summary.status, input: { goal: summary.title, model, data: { hidden: 'SHOULD_NOT_RENDER' }, policy: { max_trials: original?.maxTrials ?? 3, hidden: 'PRIVATE_POLICY_SHOULD_NOT_RENDER' } }, datasetId: dataset.id, dataset: { id: dataset.id, name: dataset.name, source: dataset.source, currency: dataset.currency, assetClass: dataset.assetClass, start: null, end: null }, submission: { id: id + 10, status: active ? 'placed' : unanswered ? 'unanswered' : 'done', ...(unanswered ? { reason: 'budget_exhausted' } : {}) },
      final: stopped.has(id) ? { status: 'STOPPED', trialId: null, reason: 'Owner requested stop', metrics: [] } : id === 8 ? { status: 'COMPUTED', trialId: 11, reason: 'Original selection rationale', metrics: finalMetrics } : id === 9 ? { status: 'FAILED', trialId: 11, reason: 'Original failed selection rationale', error: 'independent_test_failure', metrics: [] } : null,
      trialRecords: original ? [] : [{ id: 11, hypothesis: '测试 · 流动性假设', status: 'COMPUTED', metrics: [{ key: 'VALIDATION.pearson_ic', label: 'IC', partition: 'VALIDATION', value: 0.04 }], error: null }, { id: 12, hypothesis: '测试 · 失败的假设', status: 'FAILED', metrics: [], error: 'original_trial_failure' }, { id: 13, hypothesis: '测试 · 被拒绝的假设', status: 'REJECTED', metrics: [], error: 'original_trial_rejection' }],
      tasks: [{ id: 21, name: '测试计算', status: active ? 'running' : 'terminal', ...(!active ? { outcome: stopped.has(id) ? 'aborted' : unanswered ? 'failed' : 'completed' } : {}), error: unanswered ? 'task_failed' : null }],
    };
  };
  const portfolio: PortfolioView = { id: 'p1', name: '测试 · 多因子组合', researchIds: [8], weights: [{ researchId: 8, weight: '1' }], datasetId: dataset.id, status: 'COMPUTED', capital: '100000.25', currency: 'CNY', createdAt: at, taskId: 12, error: null, metrics: [{ key: 'native_return', label: 'native_return', value: '3.25', unit: '%' }], curve: [{ at: '2026-09-01', value: '100000.250000000000000001' }, { at: '2026-09-02', value: null }, { at: '2026-09-03', value: '101000.10' }, { at: '2026-09-04', value: '103250.26' }], targets: [{ instrumentId: 'TEST.CN', weight: '0.250000000000000001' }], targetExpiresAt: '2026-12-01T00:00:00.000000001Z', targetExpired: false };
  let portfolios = [portfolio];
  const observation: ObservationView = { id: 'o1', name: '测试 · 模拟账户', portfolioId: portfolio.id, mode: 'paper', status: 'OBSERVING', currency: 'CNY', equity: '103250.26', cash: null, updatedAt: at, connectionId: 'paper-test', connectionName: '测试模拟连接', provenance: { clientId: 'test-client', accountId: 'test-account', sequence: '9007199254740997', nativeClockNs: '1800000000000000001' }, positions: [{ instrumentId: 'TEST.CN', quantity: '100', value: null, pnl: '0' }], curve: portfolio.curve, receipt: { id: 'test-receipt', status: 'ACKNOWLEDGED', externalId: null, expiresAt: at, acknowledgedAt: null }, error: null };
  let observations = [observation];
  let extensions: ExtensionView[] = [{ name: '测试原生扩展', kind: 'native', version: '1.0', enabled: false, state: 'disabled', error: null }, { name: 'qz.science', kind: 'science', version: null, enabled: true, state: 'loaded', required: true, error: null }];
  const usage: UsageView = { activeTasks: 1, inputTokens: null, outputTokens: 2400, cost: null };
  const writes: { path: string; method: string; raw: string | null }[] = [];
  let researchAttempts = 0; let portfolioAttempts = 0; let missingExtensions = false; let pagination = false;
  let heldResearch: Promise<void> | undefined; let heldAuth: Promise<void> | undefined;
  await page.route('**/api/v3/**', async (route: Route) => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname.slice('/api/v3'.length); const method = request.method();
    const reply = (json: unknown, status = 200) => route.fulfill({ status, json });
    if (method !== 'GET') writes.push({ path, method, raw: request.postData() });
    if (path === '/session') { if (method === 'DELETE') { session = { setupRequired: false, authenticated: false }; return route.fulfill({ status: 204 }); } return reply(session); }
    if (path === '/session/setup' || path === '/session/login') { session = { setupRequired: false, authenticated: true }; return reply(session); }
    if (path === '/workspace') return reply(workspace);
    if (path === '/settings') { if (method === 'PATCH') settings = { ...settings, ...request.postDataJSON() }; return reply(settings); }
    if (path === '/models') return reply({ providers, models: availableModels });
    if (path === '/models/login' && method === 'POST') { const input = request.postDataJSON(); login = { id: input.requestId, providerId: input.providerId, type: input.type, status: 'running', prompt: { id: 1, type: input.type === 'oauth' ? 'text' : 'secret', message: input.type === 'oauth' ? '可选域名（留空使用默认）' : '测试 API Key' }, events: [], error: null }; providers[1]!.login = { id: login.id, type: login.type }; return reply(login, 202); }
    if (path.startsWith('/models/login/')) {
      if (method === 'DELETE') { if (login) { login.status = 'cancelled'; login.prompt = null; } providers[1]!.login = null; return reply(login); }
      if (missingLogin) return reply({ error: 'model_login_not_found' }, 404);
      if (path.endsWith('/respond')) { if (heldAuth) await heldAuth; if (++authAttempts === 1) return route.abort('failed'); login!.status = 'done'; login!.prompt = null; providers[1]!.configured = true; providers[1]!.login = null; }
      return reply(login);
    }
    if (path.startsWith('/models/providers/') && method === 'DELETE') { providers.find(provider => provider.id === path.split('/').at(-1))!.configured = false; return reply({ configured: false }); }
    if (path === '/datasets') return reply([dataset]);
    if (path === '/usage') return reply(usage);
    if (path === '/research') {
      if (method === 'POST') {
        const input = request.postDataJSON(); let record = admitted.get(input.requestId);
        if (record && record.raw !== request.postData()) return reply({ error: 'request_id_conflict' }, 409);
        if (!record) { record = { id: 10 + admitted.size, raw: request.postData()!, input }; admitted.set(input.requestId, record); studies.unshift({ id: record.id, title: input.goal, model: `${input.model.provider}/${input.model.modelId}`, status: 'RUNNING', trials: 0, createdAt: null, updatedAt: null }); }
        if (++researchAttempts === 1) return route.abort('failed'); return reply({ conversationId: record.id, submissionId: record.id + 10 }, 202);
      }
      if (heldResearch) await heldResearch;
      if (pagination) return reply(url.searchParams.has('cursor') ? { items: [studies[0], { ...studies[0], id: 6, title: '后页研究' }] } : { items: studies, cursor: { before: 7 } });
      return reply({ items: studies });
    }
    if (/^\/research\/\d+\/stop$/.test(path)) { const id = Number(path.split('/')[2]); stopped.add(id); return reply(detail(id), 202); }
    if (/^\/research\/\d+$/.test(path)) { const value = detail(Number(path.split('/')[2])); return value ? reply(value) : reply({ error: 'research_not_found' }, 404); }
    if (path === '/portfolios') { if (method === 'POST') { if (++portfolioAttempts === 1) return reply({ error: 'unsupported_evidence' }, 422); const input = request.postDataJSON(); const value = { ...portfolio, id: 'p2', name: input.name, researchIds: input.researchIds, weights: input.weights, capital: input.capital, status: 'QUEUED', metrics: [], curve: [], targets: [] }; portfolios = [...portfolios, value]; return reply(value, 202); } return reply(portfolios); }
    if (path.startsWith('/portfolios/')) return reply(portfolios.find(value => value.id === path.split('/')[2]));
    if (path === '/observations') { if (method === 'POST') { const input = request.postDataJSON(); const value: ObservationView = { ...observation, id: 'o2', mode: input.mode, status: 'QUEUED', equity: null, cash: null, updatedAt: null, positions: [], curve: [], receipt: { ...observation.receipt!, id: 'original-new-receipt', status: 'PENDING' } }; observations = [...observations, value]; return reply(value, 202); } return reply(observations); }
    if (path.startsWith('/observations/')) { const value = observations.find(item => item.id === path.split('/')[2])!; if (path.endsWith('/stop')) value.status = 'STOPPED'; else { if (value.status !== 'STOPPED') Object.assign(value, { status: 'OBSERVING', cash: '1000.250000000000000001', equity: observation.equity, updatedAt: at, curve: observation.curve, positions: observation.positions }); value.receipt!.status = 'ACKNOWLEDGED'; } return reply(value); }
    if (path === '/extensions') return missingExtensions ? reply({ error: 'not_found' }, 404) : reply(extensions);
    if (path.startsWith('/extensions/')) { const item = extensions.find(item => item.name === decodeURIComponent(path.split('/')[2]!))!; Object.assign(item, { enabled: request.postDataJSON().enabled, state: 'pending_restart' }); return reply(item); }
    return reply({ error: 'not_found' }, 404);
  });
  return { writes, observation, portfolio, idle: () => { inactive.add(7); studies.find(item => item.id === 7)!.status = 'IDLE'; }, hideModels: () => { availableModels = []; settings = { defaultModel: null, thinkingLevel: null }; }, paginate: () => { pagination = true; }, loseLogin: () => { missingLogin = true; }, holdAuth: () => { let release!: () => void; heldAuth = new Promise<void>(resolve => { release = resolve; }); return () => { heldAuth = undefined; release(); }; }, empty: () => { studies = []; portfolios = []; observations = []; extensions = []; }, missing: () => { missingExtensions = true; }, hold: () => { let release!: () => void; heldResearch = new Promise<void>(resolve => { release = resolve; }); return () => { heldResearch = undefined; release(); }; } };
}
async function choose(page: BrowserPage, label: string, option: string) { await page.getByLabel(label, { exact: true }).click(); await page.getByRole('option', { name: option, exact: true }).click(); }
async function audit(page: BrowserPage) {
  await expect(page.locator('.ant-select-dropdown:visible')).toHaveCount(0);
  // Inspect settled surfaces, not alpha-blended entrance frames; ignore perpetual busy glyphs.
  await page.evaluate(async () => { await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))); await Promise.all(document.getAnimations().filter(animation => Number.isFinite(animation.effect?.getComputedTiming().iterations)).map(animation => animation.finished.catch(() => undefined))); });
  expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze()).violations).toEqual([]);
}
async function fits(page: BrowserPage, width: number) { await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width); expect(await page.locator('.study-row').evaluateAll(rows => rows.filter(row => row.clientWidth && row.getBoundingClientRect().right > row.parentElement!.getBoundingClientRect().right + 1).length)).toBe(0); }

test('owner setup/login and uncertain research preserve the exact body after changed defaults and reload', async ({ page }, info) => {
  const state = await contract(page, true); await page.goto('/');
  await expect(page.getByRole('heading', { name: '设置工作台密码', exact: true })).toBeVisible(); expect(state.writes).toEqual([]); await audit(page);
  await page.getByLabel('密码', { exact: true }).fill('test-only-password'); await page.getByLabel('确认密码', { exact: true }).fill('different-test-password'); await page.getByRole('button', { name: '创建密码', exact: true }).click(); await expect(page.getByText('两次密码不一致')).toBeVisible(); expect(state.writes).toEqual([]);
  await page.getByLabel('确认密码', { exact: true }).fill('test-only-password'); await page.getByRole('button', { name: '创建密码', exact: true }).click(); await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '退出登录', exact: true }).click(); await expect(page.getByRole('heading', { name: '登录工作台' })).toBeVisible(); await page.getByLabel('密码', { exact: true }).fill('test-only-password'); await page.getByRole('checkbox', { name: '记住登录' }).check(); await page.getByRole('button', { name: '登录', exact: true }).click();
  await expect(page.locator('.study-row')).toHaveCount(3); await choose(page, '研究状态', '进行中'); await expect(page.locator('.study-row')).toHaveCount(1); await choose(page, '研究状态', '全部状态'); await expect(page.locator('.study-row')).toHaveCount(3); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('research-1440.png'), fullPage: true });
  await page.getByRole('button', { name: '新建研究', exact: true }).click(); await page.getByLabel('研究问题', { exact: true }).fill('测试目标'); await choose(page, '数据集', '测试数据 / CNY'); await choose(page, '模型', '测试模型 / test'); await page.getByLabel('试验预算', { exact: true }).fill('3'); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect(page.getByText('连接失败', { exact: true })).toBeVisible();
  const first = state.writes.find(value => value.path === '/research')!.raw!; expect(JSON.parse(first)).toEqual({ requestId: expect.any(String), goal: '测试目标', model: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'medium', datasetId: 'test-data', maxTrials: 3 });
  state.hideModels(); await page.reload(); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('测试目标'); await expect(page.getByLabel('研究问题', { exact: true })).toBeDisabled(); await expect(page.locator('.authoring-page')).not.toContainText('"provider"'); await page.getByRole('button', { name: '核对提交', exact: true }).click(); await expect(page.getByRole('heading', { name: '测试目标', exact: true, level: 1 })).toBeVisible(); expect(state.writes.filter(value => value.path === '/research').map(value => value.raw)).toEqual([first, first]);
  await expect(page.locator('body')).not.toContainText('SHOULD_NOT_RENDER'); await expect(page.locator('pre')).toHaveCount(0); await page.getByRole('button', { name: '研究操作' }).click(); await page.getByRole('menuitem', { name: '停止研究' }).click(); await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click(); expect(state.writes.filter(value => value.path.endsWith('/stop'))).toHaveLength(0);
  await page.getByRole('button', { name: '研究操作' }).click(); await page.getByRole('menuitem', { name: '停止研究' }).click(); await page.getByRole('dialog').getByRole('button', { name: '停止研究', exact: true }).click(); await expect(page.locator('.document-identity')).toContainText('已停止'); await fits(page, 1440); await page.screenshot({ path: info.outputPath('research-detail-1440.png'), fullPage: true });
});

test('an idle unanswered study cannot be relabelled as an owner stop', async ({ page }) => {
  const state = await contract(page); state.idle(); await page.goto('/#/research/7'); await expect(page.locator('.document-identity')).toContainText('当前无执行'); await page.getByRole('button', { name: '研究操作', exact: true }).click(); await expect(page.getByRole('menuitem', { name: '停止研究', exact: true })).toHaveCount(0); expect(state.writes).toEqual([]);
});

test('request conflicts require explicit editing and retain downloadable original bytes', async ({ page }, info) => {
  await contract(page); const bodies: string[] = [];
  await page.route('**/api/v3/research', route => { if (route.request().method() !== 'POST') return route.fallback(); bodies.push(route.request().postData()!); return bodies.length === 1 ? route.fulfill({ status: 409, json: { error: 'request_id_conflict' } }) : route.fallback(); });
  await page.goto('/#/research/new'); await page.getByLabel('研究问题', { exact: true }).fill('Original conflicted question'); await choose(page, '数据集', '测试数据 / CNY'); await choose(page, '模型', '测试模型 / test'); await page.getByLabel('试验预算', { exact: true }).fill('1'); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect(page.getByLabel('研究问题', { exact: true })).toBeDisabled(); await expect(page.getByRole('button', { name: '重新编辑', exact: true })).toBeEnabled(); const original = bodies[0]!;
  await page.getByRole('button', { name: '重新编辑', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click(); expect(bodies).toHaveLength(1); expect(await page.evaluate(() => sessionStorage.getItem('quazonai.command.research.v3'))).toBe(original);
  await page.getByRole('button', { name: '重新编辑', exact: true }).click(); const dialog = page.getByRole('dialog'); await expect(dialog).toContainText('不取消原提交'); await dialog.getByRole('button', { name: '保留原提交并重新编辑', exact: true }).click(); await expect(page.getByLabel('研究问题', { exact: true })).toBeEnabled(); await page.locator('.previous-command > summary').click(); const downloaded = page.waitForEvent('download'); await page.getByRole('link', { name: '下载原提交', exact: true }).click(); const downloadedPath = await (await downloaded).path(); expect(await readFile(downloadedPath!, 'utf8')).toBe(original); await page.setViewportSize({ width: 320, height: 844 }); await fits(page, 320); await audit(page); await page.setViewportSize({ width: 390, height: 844 }); await fits(page, 390); await page.screenshot({ path: info.outputPath('original-command-mobile.png'), fullPage: true });
  await page.reload(); await expect(page.locator('.previous-command')).toBeVisible(); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('Original conflicted question'); await page.getByLabel('研究问题', { exact: true }).fill('Explicit corrected question'); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect.poll(() => bodies.length).toBe(2); expect(JSON.parse(bodies[1]!).requestId).not.toBe(JSON.parse(original).requestId); expect(await page.evaluate(() => sessionStorage.getItem('quazonai.command.research.v3'))).toBe(bodies[1]); await expect(page.locator('pre')).toHaveCount(0);
});

test('portfolio initialization failure keeps the restored draft unavailable until a successful retry', async ({ page }) => {
  await contract(page); let available = false; await page.route('**/api/v3/datasets', route => available ? route.fallback() : route.fulfill({ status: 503, json: { error: 'temporary_unavailable' } })); await page.addInitScript(() => sessionStorage.setItem('quazonai.draft.portfolio', JSON.stringify({ name: 'Preserved portfolio draft', researchIds: [8], weights: { 8: '1' }, capital: '1000.250000000000000001', datasetId: 'test-data' })));
  await page.goto('/#/portfolios/new?research=8'); await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible(); await expect(page.getByLabel('组合名称', { exact: true })).not.toBeVisible(); available = true; await page.getByRole('button', { name: '重试', exact: true }).click(); await expect(page.getByLabel('组合名称', { exact: true })).toHaveValue('Preserved portfolio draft'); await expect(page.getByLabel('本金', { exact: true })).toHaveValue('1000.250000000000000001'); await page.getByLabel('组合名称', { exact: true }).fill('New explicit edit'); await expect(page.getByLabel('组合名称', { exact: true })).toHaveValue('New explicit edit');
});

test('observation initialization failure preserves its draft until workspace retry succeeds', async ({ page }) => {
  await contract(page); let available = false; await page.route('**/api/v3/workspace', route => available ? route.fallback() : route.fulfill({ status: 503, json: { error: 'temporary_unavailable' } })); await page.addInitScript(() => sessionStorage.setItem('quazonai.draft.observation', JSON.stringify({ portfolioId: 'p1', mode: 'paper', connectionId: 'paper-test' })));
  await page.goto('/#/observations/new'); await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible(); await expect(page.getByLabel('组合', { exact: true })).not.toBeVisible(); available = true; await page.getByRole('button', { name: '重试', exact: true }).click(); const selected = page.locator('.ant-form-item').filter({ has: page.getByLabel('组合', { exact: true }) }); await expect(selected).toContainText('测试 · 多因子组合'); await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('连接', { exact: true }) })).toContainText('测试模拟连接');
});

test('an initiating-study failure must recover its captured dataset before authoring starts', async ({ page }) => {
  await contract(page); let available = false; await page.route('**/api/v3/research/8', route => available ? route.fallback() : route.fulfill({ status: 503, json: { error: 'temporary_unavailable' } })); await page.goto('/#/portfolios/new?research=8'); await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible(); await expect(page.getByLabel('组合名称', { exact: true })).not.toBeVisible(); available = true; await page.getByRole('button', { name: '重试', exact: true }).click(); await expect(page.getByLabel('组合名称', { exact: true })).toBeVisible(); await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('数据集', { exact: true }) })).toContainText('测试数据 / CNY'); await expect(page.getByLabel('研究 8 权重', { exact: true })).toHaveValue('1');
});

test('portfolio authoring waits for the initiating study before exposing restored fields', async ({ page }) => {
  await contract(page); let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/api/v3/research/8', async route => { await held; return route.fallback(); });
  await page.addInitScript(() => sessionStorage.setItem('quazonai.draft.portfolio', JSON.stringify({ name: 'Original owner draft', researchIds: [7], weights: { 7: '1' }, capital: '1000.25', datasetId: 'test-data' })));
  const datasets = page.waitForResponse(response => response.url().endsWith('/api/v3/datasets')); const studies = page.waitForResponse(response => response.url().endsWith('/api/v3/research'));
  try { await page.goto('/#/portfolios/new?research=8'); await Promise.all([datasets, studies]); await expect(page.getByLabel('组合名称', { exact: true })).not.toBeVisible(); release(); await expect(page.getByLabel('组合名称', { exact: true })).toHaveValue('Original owner draft'); await page.getByLabel('组合名称', { exact: true }).fill('Fresh owner edit'); await page.getByLabel('本金', { exact: true }).fill('2000.250000000000000001'); await expect(page.getByLabel('研究 8 权重', { exact: true })).toBeVisible(); await expect(page.getByLabel('组合名称', { exact: true })).toHaveValue('Fresh owner edit'); await expect(page.getByLabel('本金', { exact: true })).toHaveValue('2000.250000000000000001'); }
  finally { release(); }
});

for (const beforeReplay of [true, false]) test(`a remounted command unlocks after definitive rejection ${beforeReplay ? 'before' : 'during'} replay`, async ({ page }) => {
  await contract(page); let first!: () => void; let second!: () => void; let firstFinished = false; const heldFirst = new Promise<void>(resolve => { first = resolve; }); const heldSecond = new Promise<void>(resolve => { second = resolve; }); const bodies: string[] = [];
  await page.route('**/api/v3/research', async route => { if (route.request().method() !== 'POST') return route.fallback(); bodies.push(route.request().postData()!); const ordinal = bodies.length; await (ordinal === 1 ? heldFirst : heldSecond); await route.fulfill({ status: 409, json: { error: 'model_unavailable' } }); if (ordinal === 1) firstFinished = true; });
  try {
    await page.goto('/#/research/new'); await page.getByLabel('研究问题', { exact: true }).fill('Definitively rejected original'); await choose(page, '数据集', '测试数据 / CNY'); await choose(page, '模型', '测试模型 / test'); await page.getByLabel('试验预算', { exact: true }).fill('1'); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect.poll(() => bodies.length).toBe(1); await page.getByRole('navigation', { name: '主要工作区' }).getByRole('link', { name: '数据', exact: true }).click(); await expect(page.getByRole('heading', { name: '数据', level: 1, exact: true })).toBeVisible(); await page.getByRole('navigation', { name: '主要工作区' }).getByRole('link', { name: '研究', exact: true }).click(); await page.getByRole('button', { name: '新建研究', exact: true }).click();
    if (beforeReplay) { first(); await expect.poll(() => firstFinished).toBe(true); await expect(page.getByLabel('研究问题', { exact: true })).toBeEnabled(); await expect(page.getByRole('button', { name: '核对提交', exact: true })).toHaveCount(0); expect(bodies).toHaveLength(1); expect(await page.evaluate(() => sessionStorage.getItem('quazonai.command.research.v3'))).toBeNull(); return; }
    await page.getByRole('button', { name: '核对提交', exact: true }).click(); await expect.poll(() => bodies.length).toBe(2); first(); await expect.poll(() => firstFinished).toBe(true); await expect(page.getByLabel('研究问题', { exact: true })).toBeDisabled(); second(); await expect(page.getByLabel('研究问题', { exact: true })).toBeEnabled(); expect(bodies[1]).toBe(bodies[0]); expect(await page.evaluate(() => sessionStorage.getItem('quazonai.command.research.v3'))).toBeNull(); await expect(page.getByRole('button', { name: '开始研究', exact: true })).toBeVisible();
  } finally { first(); second(); }
});

test('a study entry merges its identity into an existing portfolio draft without normalizing weights', async ({ page }) => {
  const state = await contract(page); await page.addInitScript(() => sessionStorage.setItem('quazonai.draft.portfolio', JSON.stringify({ name: 'Existing owner draft', datasetId: 'test-data', researchIds: [7], weights: { 7: '0.750000000000000001' }, capital: '1000.250000000000000001' }))); await page.goto('/#/research/8'); await page.getByRole('button', { name: '创建组合', exact: true }).click(); await expect(page.getByLabel('组合名称', { exact: true })).toHaveValue('Existing owner draft'); await expect(page.getByLabel('研究 7 权重', { exact: true })).toHaveValue('0.750000000000000001'); await expect(page.getByLabel('研究 8 权重', { exact: true })).toHaveValue(''); await expect(page.getByLabel('本金', { exact: true })).toHaveValue('1000.250000000000000001'); await expect(page.locator('.allocation-total')).toContainText('未完整填写'); expect(state.writes).toEqual([]);
});

test('stopped settled receipts have no refresh action but an uncertain original remains reconcilable', async ({ page }) => {
  const state = await contract(page); state.observation.status = 'STOPPED';
  for (const status of ['ACKNOWLEDGED', 'REJECTED', 'EXPIRED', 'REVOKED', 'NOT_OFFERED'] as const) { state.observation.receipt!.status = status; await page.goto('/#/observations/o1'); await page.reload(); await expect(page.locator('.record-context')).toContainText('已停止'); await expect(page.getByRole('button', { name: '核对回执', exact: true })).toHaveCount(0); await expect(page.getByRole('button', { name: '更新快照', exact: true })).toHaveCount(0); }
  expect(state.writes).toEqual([]); state.observation.receipt!.status = 'UNCERTAIN'; await page.reload(); await page.getByRole('button', { name: '核对回执', exact: true }).click(); await expect(page.locator('.receipt-section')).toContainText('已接收'); await expect(page.locator('.record-context')).toContainText('已停止'); await expect(page.getByRole('button', { name: '核对回执', exact: true })).toHaveCount(0); expect(state.writes.map(item => item.path)).toEqual(['/observations/o1/refresh']);
});

test('an acknowledged observing binding does not claim an unavailable account snapshot', async ({ page }) => {
  const state = await contract(page); Object.assign(state.observation, { equity: null, cash: null, updatedAt: null, provenance: null, positions: [], curve: [], error: 'account_unavailable' }); await page.goto('/#/observations'); await expect(page.getByRole('row').filter({ hasText: '测试 · 模拟账户' })).toContainText('观测中'); await expect(page.locator('body')).not.toContainText('已获取快照'); await page.getByRole('link', { name: '测试 · 模拟账户', exact: true }).click(); await expect(page.locator('.record-context')).toContainText('观测中'); await expect(page.locator('.receipt-section')).toContainText('已接收'); await expect(page.locator('.snapshot-values').getByText('未提供', { exact: true })).toHaveCount(2); await expect(page.locator('.snapshot-empty')).toContainText('尚无账户快照'); await expect(page.getByRole('button', { name: '更新快照', exact: true })).toBeEnabled(); expect(state.writes).toEqual([]);
});

test('independent null metrics, original selected/failed trials, errors and terminal outcomes stay distinct', async ({ page }, info) => {
  await contract(page); await page.goto('/#/research/8'); await expect(page.getByRole('heading', { name: '测试 · 横截面价值', exact: true, level: 1 })).toBeVisible(); await expect(page.locator('.metric-primary')).toContainText('不可用'); await expect(page.locator('.metric-primary')).toContainText('分数恒定'); await expect(page.locator('.metric-grid')).toContainText('9007199254740995'); await expect(page.locator('.metric-grid')).toContainText('0');
  await page.getByRole('button', { name: '所选试验 #11' }).click(); await expect(page.getByRole('button', { name: '收起试验 11' })).toHaveAttribute('aria-expanded', 'true'); await expect(page.locator('.trial-detail:visible')).toContainText('测试 · 流动性假设'); await page.getByRole('button', { name: '展开试验 12' }).click(); await expect(page.locator('.trial-detail:visible')).toHaveCount(1); await expect(page.locator('.trial-detail:visible')).toContainText('original_trial_failure'); await expect(page.getByRole('button', { name: '展开试验 11' })).toHaveAttribute('aria-expanded', 'false'); await page.reload(); await expect(page.getByRole('tab', { name: /试验/ })).toHaveAttribute('aria-selected', 'true'); await expect(page.getByRole('button', { name: '收起试验 12' })).toHaveAttribute('aria-expanded', 'true'); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('trials-1440.png'), fullPage: true });
  await page.goto('/#/research/9'); await expect(page.getByText('独立评估失败', { exact: true }).first()).toBeVisible(); await page.locator('.recorded-error summary').click(); await expect(page.locator('.recorded-error')).toContainText('independent_test_failure'); await page.locator('.reason-disclosure summary').click(); await expect(page.locator('.reason-disclosure')).toContainText('Original failed selection rationale'); await page.getByRole('tab', { name: '输入与执行' }).click(); await expect(page.getByRole('cell', { name: '失败', exact: true })).toBeVisible(); await expect(page.locator('body')).toContainText('budget_exhausted'); await expect(page.locator('body')).not.toContainText('SHOULD_NOT_RENDER');
});

test('portfolio exact inputs, native curve data, target-only observation, explicit defaults and extension state', async ({ page }, info) => {
  const state = await contract(page); await page.goto('/#portfolios'); await page.getByRole('link', { name: '测试 · 多因子组合', exact: true }).click(); await expect(page.locator('[data-testid="equity-chart"] svg')).toBeVisible(); await page.getByText('查看数据', { exact: true }).click(); await expect(page.getByRole('cell', { name: '100000.250000000000000001', exact: true })).toBeVisible(); await expect(page.getByRole('cell', { name: '未提供', exact: true })).toBeVisible(); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('portfolio-result-1440.png'), fullPage: true });
  await page.goto('/#/portfolios/new'); await page.getByLabel('组合名称', { exact: true }).fill('测试新组合'); await choose(page, '原始研究', '#8 / 测试 · 横截面价值'); await page.keyboard.press('Escape'); await choose(page, '数据集', '测试数据 / CNY'); await page.getByLabel('本金', { exact: true }).fill('100000.250000000000000001'); await expect(page.getByLabel('研究 8 权重', { exact: true })).toHaveValue('1'); await page.getByRole('button', { name: '运行回测' }).click(); await expect(page.getByText('研究证据不支持', { exact: true })).toBeVisible(); await page.getByRole('button', { name: '运行回测' }).click(); await expect(page.getByRole('heading', { name: '测试新组合' })).toBeVisible(); await expect(page.locator('.document-toolbar')).toContainText('待执行'); await expect(page.locator('[data-testid="equity-chart"]')).toHaveCount(0);
  const commands = state.writes.filter(value => value.path === '/portfolios'); expect(JSON.parse(commands[0]!.raw!)).toMatchObject({ researchIds: [8], weights: [{ researchId: 8, weight: '1' }], datasetId: 'test-data', capital: '100000.250000000000000001', name: '测试新组合' }); expect(JSON.parse(commands[0]!.raw!).requestId).not.toBe(JSON.parse(commands[1]!.raw!).requestId);
  await page.getByRole('link', { name: '目标与观测', exact: true }).click(); await page.getByRole('button', { name: '新建观测' }).click(); await choose(page, '组合', '测试 · 多因子组合'); await page.getByLabel('连接', { exact: true }).click(); await expect(page.getByRole('option', { name: '测试实盘连接' })).toHaveCount(0); await page.getByRole('option', { name: '测试模拟连接' }).click(); await page.getByRole('button', { name: '交付并观测' }).click(); await expect(page.locator('.receipt-section')).toContainText('待接收'); await expect(page.locator('.snapshot-section')).toContainText('尚无账户快照'); expect(JSON.parse(state.writes.find(value => value.path === '/observations')!.raw!)).toMatchObject({ portfolioId: 'p1', mode: 'paper', connectionId: 'paper-test' });
  await page.getByRole('button', { name: '核对回执' }).click(); await expect(page.locator('.snapshot-values')).toContainText('1000.250000000000000001'); await expect(page.locator('.receipt-section')).toContainText('已接收'); await page.getByRole('button', { name: '停止观测', exact: true }).click(); const stop = page.getByRole('dialog'); await expect(stop).toContainText('仅停止本地观测，不撤回下游目标'); await stop.getByRole('button', { name: '停止观测', exact: true }).click(); await expect(page.getByRole('button', { name: '停止观测', exact: true })).toBeDisabled(); await expect(page.locator('.receipt-section')).toContainText('已接收'); expect(state.writes.filter(value => value.path === '/observations')).toHaveLength(1); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('observation-1440.png'), fullPage: true });
  await page.goto('/#/observations?mode=live'); await expect(page.getByRole('heading', { name: '尚无实盘观测' })).toBeVisible(); await expect(page.getByRole('button', { name: /下单|买入|卖出/ })).toHaveCount(0);
  await page.getByRole('link', { name: '模型设置', exact: true }).click(); await expect(page.getByLabel(/超时|最大试验|内存|应用名称/)).toHaveCount(0); await choose(page, '推理强度', '高'); await page.getByRole('button', { name: '刷新目录' }).click(); await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理强度', { exact: true }) })).toContainText('高'); expect(state.writes.filter(value => value.path === '/settings')).toHaveLength(0); await page.getByRole('button', { name: '保存设置' }).click(); await expect(page.getByText('已保存', { exact: true })).toBeVisible(); await expect(page.getByRole('button', { name: '保存设置' })).toBeDisabled(); expect(JSON.parse(state.writes.find(value => value.method === 'PATCH' && value.path === '/settings')!.raw!)).toEqual({ defaultModel: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'high' }); await audit(page); await page.screenshot({ path: info.outputPath('settings-1440.png'), fullPage: true });
  await page.getByRole('link', { name: '扩展', exact: true }).click(); await expect(page.getByRole('switch', { name: '启用 qz.science' })).toHaveCount(0); await page.getByRole('switch', { name: '启用 测试原生扩展' }).click(); await expect(page.getByText('待重启', { exact: true })).toBeVisible(); await expect(page.getByRole('switch')).toBeChecked(); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('extensions-1440.png'), fullPage: true });
});

test('model provider secrets stay transient; uncertain prompt response retries original body and cancel is explicit', async ({ page }) => {
  const state = await contract(page); await page.goto('/#settings'); await page.getByLabel('搜索模型服务').fill('待连接'); await expect(page.locator('.provider-row')).toHaveCount(1); await page.getByRole('button', { name: '配置 API Key 待连接服务', exact: true }).click();
  const dialog = page.getByRole('dialog'); await expect(dialog.getByLabel('测试 API Key', { exact: true })).toBeFocused(); await expect(dialog.getByRole('button', { name: '确认连接', exact: true })).toBeDisabled(); await dialog.getByLabel('测试 API Key', { exact: true }).fill('synthetic-browser-secret-only'); await dialog.getByRole('button', { name: '确认连接', exact: true }).click(); await expect(dialog.getByRole('button', { name: '重试当前响应', exact: true })).toBeVisible(); await audit(page);
  expect(await page.evaluate(() => JSON.stringify([localStorage, sessionStorage]))).not.toContain('synthetic-browser-secret-only'); await dialog.getByRole('button', { name: '重试当前响应', exact: true }).click(); await expect(dialog.getByText('模型配置已保存', { exact: true })).toBeVisible(); expect(state.writes.filter(write => write.path.endsWith('/respond')).map(write => write.raw)).toEqual([JSON.stringify({ promptId: 1, value: 'synthetic-browser-secret-only' }), JSON.stringify({ promptId: 1, value: 'synthetic-browser-secret-only' })]); await expect(dialog.locator('input')).toHaveCount(0); await dialog.getByRole('button', { name: '关闭', exact: true }).last().click();
  await page.getByRole('button', { name: '配置 API Key 待连接服务', exact: true }).click(); await expect(dialog.getByLabel('测试 API Key', { exact: true })).toBeVisible(); await dialog.getByRole('button', { name: '取消连接', exact: true }).click(); await expect(dialog).toHaveCount(0); expect(state.writes.filter(write => write.method === 'DELETE' && write.path.startsWith('/models/login/'))).toHaveLength(1);
});

test('official text prompts accept blank defaults and an unacknowledged response remains cancellable', async ({ page }) => {
  const state = await contract(page); await page.goto('/#settings'); await page.getByRole('button', { name: '登录 待连接服务', exact: true }).click(); const dialog = page.getByRole('dialog'); await expect(dialog.getByLabel('可选域名（留空使用默认）', { exact: true })).toBeVisible(); await expect(dialog.getByRole('button', { name: '确认连接', exact: true })).toBeEnabled(); await dialog.getByRole('button', { name: '确认连接', exact: true }).click(); await dialog.getByRole('button', { name: '重试当前响应', exact: true }).click(); await expect(dialog.getByText('模型配置已保存', { exact: true })).toBeVisible(); expect(JSON.parse(state.writes.find(write => write.path.endsWith('/respond'))!.raw!).value).toBe(''); await dialog.getByRole('button', { name: '关闭', exact: true }).last().click();
  const release = state.holdAuth(); try { await page.getByRole('button', { name: '配置 API Key 待连接服务', exact: true }).click(); await dialog.getByLabel('测试 API Key', { exact: true }).fill('synthetic-unacknowledged-only'); await dialog.getByRole('button', { name: '确认连接', exact: true }).click(); await expect.poll(() => state.writes.filter(write => write.path.endsWith('/respond')).length).toBe(3); await dialog.getByRole('button', { name: '取消连接', exact: true }).click(); await expect(dialog).toHaveCount(0); const start = state.writes.filter(write => write.path === '/models/login').at(-1)!; expect(state.writes.filter(write => write.method === 'DELETE').at(-1)!.path).toBe(`/models/login/${JSON.parse(start.raw!).requestId}`); } finally { release(); }
});

test('cursor filtering preserves later pages and a vanished provider login ends visibly', async ({ page }) => {
  const state = await contract(page); state.paginate(); await page.goto('/#/research'); await page.getByLabel('搜索已加载研究').fill('后页'); await expect(page.getByRole('heading', { name: '没有匹配的研究' })).toBeVisible(); await page.getByRole('button', { name: '加载更多' }).click(); await expect(page.locator('.study-row')).toHaveCount(1); await expect(page.locator('.collection-scope')).toContainText('已加载 4 条'); await page.getByRole('button', { name: '清除筛选' }).click(); await expect(page.locator('.study-row')).toHaveCount(4);
  await page.goto('/#/settings'); await page.getByRole('button', { name: '配置 API Key 待连接服务', exact: true }).click(); await expect(page.getByRole('dialog').getByLabel('测试 API Key')).toBeVisible(); state.loseLogin(); await expect(page.getByText('登录已结束', { exact: true })).toBeVisible(); await expect(page.getByRole('dialog').locator('input')).toHaveCount(0); await expect(page.getByRole('status', { name: '等待授权' })).toHaveCount(0);
});

test('390px keyboard navigation, reduced motion and distinct loading/empty/missing states', async ({ page }, info) => {
  await page.setViewportSize({ width: 390, height: 844 }); await page.emulateMedia({ reducedMotion: 'reduce' }); const state = await contract(page); const release = state.hold(); await page.goto('/'); await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible(); await expect(page.getByRole('status', { name: '加载中' }).first()).toBeVisible(); release(); await expect(page.locator('.study-row')).toHaveCount(3);
  await page.getByRole('link', { name: '跳到内容' }).focus(); await expect(page.getByRole('link', { name: '跳到内容' })).toHaveCSS('outline-style', 'solid'); await page.keyboard.press('Enter'); await expect(page.locator('#main-content')).toBeFocused(); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath('research-390.png'), fullPage: true });
  await page.getByRole('button', { name: '新建研究', exact: true }).click(); await expect(page.getByLabel('研究问题', { exact: true })).toBeFocused(); await page.keyboard.press('Tab'); await expect(page.getByLabel('数据集', { exact: true })).toBeFocused(); await page.keyboard.press('Escape'); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath('research-form-390.png'), fullPage: true }); await page.getByRole('button', { name: '取消', exact: true }).click();
  for (const [label, image] of [['数据', 'data'], ['组合回测', 'portfolios'], ['目标与观测', 'observations'], ['模型设置', 'settings'], ['扩展', 'extensions']]) { await page.getByRole('button', { name: '打开导航' }).press('Enter'); const menu = page.getByRole('dialog'); await expect(menu.getByRole('link')).toHaveCount(6); const link = menu.getByRole('link', { name: label!, exact: true }); await link.focus(); await expect(link).toHaveCSS('outline-style', 'solid'); await link.press('Enter'); await expect(menu).toBeHidden(); await expect(page.getByRole('heading', { name: label!, exact: true })).toBeVisible(); await expect(page.locator('.loading-state')).toHaveCount(0); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath(`${image}-390.png`), fullPage: true }); }
  state.empty(); for (const [path, text] of [['research', '尚无研究'], ['portfolios', '尚无策略组合'], ['observations', '尚无观测'], ['extensions', '没有已发现扩展']]) { await page.goto('/#/' + path); await page.reload(); await expect(page.getByRole('heading', { name: text!, exact: true })).toBeVisible(); await fits(page, 390); }
  state.missing(); await page.getByRole('button', { name: '刷新目录', exact: true }).click(); await expect(page.getByRole('alert')).toContainText('记录不存在'); await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible(); await expect(page.locator('pre')).toHaveCount(0); expect(state.writes).toEqual([]);
});

test('dataset facts and explicit preselection return to the originating workspace', async ({ page }) => {
  const state = await contract(page); await page.goto('/#/data'); await page.getByRole('button', { name: '测试数据', exact: true }).click(); const facts = page.getByRole('dialog'); await expect(facts).toContainText('test-data'); await expect(facts).toContainText('CNY'); await audit(page); await facts.getByRole('button', { name: '用于研究' }).click(); await expect(page.getByRole('heading', { name: '新建研究' })).toBeVisible(); await expect(page.locator('.dataset-context')).toContainText('test-data'); await page.getByRole('button', { name: '取消', exact: true }).click(); await expect(page.getByRole('heading', { name: '数据', exact: true, level: 1 })).toBeVisible(); expect(state.writes).toEqual([]);
});

test('a held account refresh remains stoppable and cannot revive a stopped observation', async ({ page }) => {
  const state = await contract(page); let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let finished = false;
  await page.route('**/api/v3/observations/o1/refresh', async route => { const old = structuredClone(state.observation); await held; await route.fulfill({ json: old }).catch(() => undefined); finished = true; });
  try {
    await page.goto('/#/observations/o1'); await page.getByRole('button', { name: '更新快照', exact: true }).click(); await expect(page.getByRole('button', { name: '更新中', exact: true })).toBeVisible(); await expect(page.getByRole('button', { name: '停止观测', exact: true })).toBeEnabled(); await page.getByRole('button', { name: '停止观测', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '停止观测', exact: true }).click(); await expect(page.getByRole('button', { name: '停止观测', exact: true })).toBeDisabled();
    release(); await expect.poll(() => finished).toBe(true); await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))); expect(await page.locator('.record-context').innerText()).toContain('已停止'); expect(state.writes.filter(write => write.path === '/observations/o1/stop')).toHaveLength(1); await expect(page.locator('.receipt-section')).toContainText('已接收');
  } finally { release(); }
});

test('provider disconnect remains serialized through credential removal and catalog refresh', async ({ page }) => {
  await contract(page); let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let calls = 0;
  await page.route('**/api/v3/models/providers/test', async route => { calls++; await held; return route.fallback(); });
  try { await page.goto('/#/settings'); const provider = page.locator('.provider-row').filter({ hasText: '测试模型服务' }); await provider.getByRole('button', { name: '断开', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '断开模型服务', exact: true }).click(); await expect.poll(() => calls).toBe(1); await expect(provider.getByRole('button', { name: '断开', exact: true })).toBeDisabled(); await expect(provider.getByRole('button', { name: '配置 API Key 测试模型服务', exact: true })).toBeDisabled(); await page.getByRole('link', { name: '数据', exact: true }).click(); await expect(page.getByRole('heading', { name: '数据', exact: true, level: 1 })).toBeVisible(); await page.getByRole('link', { name: '模型设置', exact: true }).click(); await expect(provider.getByRole('button', { name: '断开', exact: true })).toBeDisabled(); expect(calls).toBe(1); release(); await expect(provider.getByRole('button', { name: '断开', exact: true })).toHaveCount(0); await expect(provider.getByRole('button', { name: '配置 API Key 测试模型服务', exact: true })).toBeEnabled(); expect(calls).toBe(1); }
  finally { release(); }
});

test('long native histories retain every exact table record without one SVG symbol per point', async ({ page }) => {
  const state = await contract(page); state.portfolio.curve = Array.from({ length: 201 }, (_, index) => ({ at: new Date(Date.UTC(2026, 9, 1) + index * 60_000).toISOString(), value: `${1000 + index}.000000000000000001` })); await page.goto('/#/portfolios/p1'); await expect(page.getByTestId('equity-chart').locator('svg')).toBeVisible(); expect(await page.getByTestId('equity-chart').locator('svg path').count()).toBeLessThan(200); await page.locator('.curve-details > summary').click(); await expect(page.locator('.curve-details .ant-table-row')).toHaveCount(20); await page.locator('.curve-details .ant-pagination-item-11').click(); await expect(page.locator('.curve-details .ant-table-row')).toHaveCount(1); await expect(page.locator('.curve-details')).toContainText('1200.000000000000000001');
});

test('backtest performance orders old histories and links return, equity, drawdown and zoom across themes', async ({ page }, info) => {
  const state = await contract(page); state.portfolio.capital = '1000';
  const chronological = Array.from({ length: 32 }, (_, i) => ({ at: new Date(Date.UTC(2026, 8, i + 1)).toISOString(), value: `${1000 + i * 2 - (i > 15 && i < 23 ? 20 : 0)}.00` }));
  state.portfolio.curve = [...chronological.filter((_, i) => i % 2 === 0), ...chronological.filter((_, i) => i % 2 === 1)];
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('/#/portfolios/p1'); await expect(page.getByRole('heading', { name: '回测表现', exact: true })).toBeVisible();
  await expect(page.getByRole('radio', { name: '累计收益', exact: true })).toBeChecked(); await expect(page.locator('.equity-summary')).toContainText('+6.20%');
  await expect(page.getByTestId('equity-chart')).toHaveClass(/with-drawdown/); await expect(page.getByTestId('equity-chart').locator('svg')).toBeVisible();
  // A sorted time axis must never draw back toward an earlier x coordinate.
  const linePath = page.getByTestId('equity-chart').locator('path[stroke-width="2"]').first();
  const path = await linePath.getAttribute('d');
  const xs = [...path!.matchAll(/[ML]\s*(-?[\d.]+)[,\s]/g)].map(match => Number(match[1]));
  expect(xs.length).toBeGreaterThan(20); expect(xs.every((x, i) => i === 0 || x >= xs[i - 1]!)).toBe(true);
  await page.locator('.equity-heading label').filter({ hasText: '账户权益' }).click(); await expect(page.getByRole('radio', { name: '账户权益', exact: true })).toBeChecked();
  await page.getByRole('radio', { name: '账户权益', exact: true }).press('ArrowLeft'); await expect(page.getByRole('radio', { name: '累计收益', exact: true })).toBeChecked();
  await page.locator('.curve-details > summary').click(); await expect(page.locator('.curve-details .ant-table-row').first()).toContainText('2026-09-01'); await page.locator('.curve-details > summary').click();
  const bounds = (await page.getByTestId('equity-chart').boundingBox())!;
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + 80);
  await expect(page.getByTestId('equity-chart').locator('text').filter({ hasText: /账户权益  / })).toBeVisible();
  await page.mouse.move(bounds.x + bounds.width - 16, bounds.y + bounds.height - 16); await page.mouse.down(); await page.mouse.move(bounds.x + bounds.width * 0.65, bounds.y + bounds.height - 16, { steps: 8 }); await page.mouse.up();
  await expect.poll(() => linePath.getAttribute('d')).not.toBe(path);
  await page.getByRole('button', { name: '重置区间', exact: true }).click();
  await expect.poll(() => linePath.getAttribute('d')).toBe(path);
  for (const dark of [false, true]) {
    await page.getByRole('button', { name: '外观设置', exact: true }).click(); await page.getByRole('menuitem', { name: dark ? '深色' : '浅色', exact: true }).click();
    await expect(page.getByRole('menu')).not.toBeVisible(); await page.getByRole('heading', { name: '回测表现', exact: true }).click();
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 1000 }); await fits(page, width); await expect(page.getByTestId('equity-chart').locator('svg')).toBeVisible();
      await page.locator('.equity-section').screenshot({ path: info.outputPath(`backtest-${dark ? 'dark' : 'light'}-${width}.png`) });
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
  }
  await audit(page); expect(errors).toEqual([]);
});

test('exact precision and every co-located clock remain readable; invalid rows do not unsort valid rows', async ({ page }) => {
  const state = await contract(page); state.portfolio.capital = '100';
  const amount = '9007199254740993.000000000000000000001';
  state.portfolio.curve = [
    { at: '2026-01-02T00:00:00Z', value: amount },
    { at: '2026-01-01T00:00:00.000000003Z', value: '102' },
    { at: '2026-01-01T00:00:00.000000001Z', value: '100.123456789012345678901' },
    { at: '2026-01-01T00:00:00.000000002Z', value: '101' },
  ];
  await page.goto('/#/portfolios/p1'); await expect(page.locator('.equity-summary')).toContainText('9,007,199,254,740,993.000000000000000000001');
  await expect(page.getByTestId('equity-chart')).toHaveClass(/with-drawdown/);
  await page.getByTestId('equity-chart').scrollIntoViewIfNeeded();
  const bounds = (await page.getByTestId('equity-chart').boundingBox())!; await page.mouse.move(bounds.x + bounds.width * 0.25, bounds.y + 80);
  for (const suffix of ['001', '002', '003']) await expect(page.getByTestId('equity-chart').locator('text').filter({ hasText: `2026-01-01 00:00:00.000000${suffix} UTC` })).toBeVisible();
  state.portfolio.curve.unshift({ at: 'invalid-clock', value: '99' }); await page.reload();
  await expect(page.getByText('1 个记录时间无效，未绘制；原始记录见下方数据。', { exact: true })).toBeVisible();
  await page.locator('.curve-details > summary').click();
  await expect(page.locator('.curve-details .ant-table-row').first()).toContainText('2026-01-01 00:00:00.000000001');
  await expect(page.locator('.curve-details .ant-table-row').last()).toContainText('invalid-clock');
});

test('refresh clears a pristine default model when another client removes it', async ({ page }) => {
  const state = await contract(page); let cleared = false;
  await page.route('**/api/v3/settings', route => route.fulfill({ json: { defaultModel: cleared ? null : { provider: 'test', modelId: 'test-model' }, thinkingLevel: cleared ? null : 'medium' } }));
  await page.goto('/#/settings'); const field = page.locator('.ant-form-item').filter({ has: page.getByLabel('默认模型', { exact: true }) }); await expect(field).toContainText('测试模型'); await expect(page.getByRole('button', { name: '保存设置', exact: true })).toBeDisabled(); cleared = true; await page.getByRole('button', { name: '刷新目录', exact: true }).click(); await expect(field).not.toContainText('测试模型'); await expect(page.getByRole('button', { name: '保存设置', exact: true })).toBeDisabled(); await page.reload(); await expect(field).not.toContainText('测试模型'); expect(state.writes).toEqual([]);
});

test('late settings response cannot overwrite a newer save or discard a later draft', async ({ page }) => {
  await contract(page); let current: SettingsView = { defaultModel: { provider: 'test', modelId: 'test-model' }, thinkingLevel: 'medium' }; let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let patches = 0;
  await page.route('**/api/v3/settings', async route => { if (route.request().method() === 'PATCH') { current = route.request().postDataJSON(); const snapshot = structuredClone(current); if (++patches === 1) await held; return route.fulfill({ json: snapshot }); } return route.fulfill({ json: current }); });
  try {
    await page.goto('/#/settings'); await choose(page, '推理强度', '高'); await page.getByRole('button', { name: '保存设置', exact: true }).click(); await expect.poll(() => patches).toBe(1); await page.getByRole('link', { name: '数据', exact: true }).click(); await page.getByRole('link', { name: '模型设置', exact: true }).click(); await choose(page, '推理强度', '低'); await page.getByRole('button', { name: '保存设置', exact: true }).click(); await expect(page.getByText('已保存', { exact: true })).toBeVisible(); await choose(page, '推理强度', '中'); await expect(page.getByText('未保存', { exact: true })).toBeVisible();
    const refreshed = page.waitForResponse(response => response.url().endsWith('/api/v3/settings') && response.request().method() === 'GET'); release(); await refreshed; await page.reload(); await expect(page.locator('.ant-form-item').filter({ has: page.getByLabel('推理强度', { exact: true }) })).toContainText('中'); await expect(page.getByText('未保存', { exact: true })).toBeVisible(); expect(current.thinkingLevel).toBe('low'); expect(patches).toBe(2);
  } finally { release(); }
});

for (const budget of ['0', '9007199254740991']) test(`explicit trial budget ${budget} is serialized without a UI cap`, async ({ page }) => {
  const state = await contract(page); await page.goto('/#/research/new'); await page.getByLabel('研究问题', { exact: true }).fill('Explicit budget protocol'); await choose(page, '数据集', '测试数据 / CNY');
  await page.getByLabel('试验预算', { exact: true }).fill('1.5'); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect(page.getByText('请输入非负安全整数', { exact: true })).toBeVisible(); expect(state.writes).toEqual([]); await page.getByLabel('试验预算', { exact: true }).fill(budget); await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect(page.getByRole('button', { name: '核对提交', exact: true })).toBeVisible(); expect(JSON.parse(state.writes.find(write => write.path === '/research')!.raw!).maxTrials).toBe(Number(budget));
});

test('light/dark and 320–1024 layouts retain exact long facts without page overflow', async ({ page }, info) => {
  await contract(page); const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  for (const dark of [false, true]) {
    await page.goto('/#/settings'); await page.getByText(dark ? '深色' : '浅色', { exact: true }).click(); await expect(page.getByRole('radio', { name: dark ? '深色' : '浅色', exact: true })).toBeChecked();
    for (const width of [320, 390, 768, 1024]) {
      await page.setViewportSize({ width, height: 844 });
      for (const route of ['research/8', 'research/9?tab=inputs', 'data', 'portfolios/p1', 'observations/o1', 'settings', 'extensions']) {
        await page.goto('/#/' + route); const title = route === 'research/8' ? '测试 · 横截面价值' : route.startsWith('research/9') ? '测试 · ' + 'LongResearchName'.repeat(7) : route.startsWith('portfolios') ? '测试 · 多因子组合' : route.startsWith('observations') ? '目标与观测' : route === 'data' ? '数据' : route === 'settings' ? '模型设置' : '扩展'; await expect(page.getByRole('heading', { name: title, exact: true, level: 1 })).toBeVisible(); await expect(page.locator('.loading-state')).toHaveCount(0); if (route.startsWith('research/9')) await expect(page.getByRole('tab', { name: '输入与执行' })).toHaveAttribute('aria-selected', 'true'); if (route.startsWith('observations')) await expect(page.locator('.observation-document')).toBeVisible(); await fits(page, width);
      }
    }
    await page.setViewportSize({ width: 1440, height: 900 }); await page.goto('/#/research/8'); await expect(page.locator('.metric-grid')).toBeVisible(); await audit(page); await page.screenshot({ path: info.outputPath(`research-${dark ? 'dark' : 'light'}-1440.png`), fullPage: true });
  }
  expect(errors).toEqual([]);
});

test('recovered research keeps original identity separate from current execution', async ({ page }) => {
  await contract(page);
  await page.route('**/api/v3/research/7', route => route.fulfill({ json: {
    id: 7, title: '原研究恢复身份', status: 'RUNNING', trials: 18, model: 'test/test-model', createdAt: null, updatedAt: null,
    input: { goal: '原研究恢复身份', model: { provider: 'test', modelId: 'test-model' }, data: {}, policy: { max_trials: 100 } },
    originalSubmission: { id: 17, requestId: 'owner-original-request', status: 'unanswered', reason: 'model_error' },
    submission: { id: 80, requestId: 'qz.research.resume:7:17', status: 'placed' },
    trialRecords: [], tasks: [{ id: 81, name: 'pi.generation', status: 'running', error: null }], final: null,
  } }));
  await page.goto('/#/research/7'); await page.getByRole('tab', { name: '输入与执行', exact: true }).click();
  const original = page.locator('section').filter({ has: page.getByRole('heading', { name: '原始身份', exact: true }) });
  await expect(original).toContainText('owner-original-request'); await expect(original).not.toContainText('qz.research.resume');
  const current = page.locator('section').filter({ has: page.getByRole('heading', { name: '当前执行', exact: true }) });
  await expect(current).toContainText('80'); await expect(current).toContainText('qz.research.resume:7:17');
});

test('continued research displays the prior unselected stop without replacing original identity', async ({ page }) => {
  await contract(page);
  await page.route('**/api/v3/research/7', route => route.fulfill({ json: {
    id: 7, title: '原研究停止后续作', status: 'RUNNING', trials: 123, model: 'test/test-model', createdAt: null, updatedAt: null,
    input: { goal: '原研究停止后续作', model: { provider: 'test', modelId: 'test-model' }, data: {}, policy: { max_trials: 299941 } },
    originalSubmission: { id: 17, requestId: 'owner-original-request', status: 'unanswered', reason: 'model_error' },
    submission: { id: 90, requestId: 'qz.research.resume:7:80', status: 'placed' },
    previousStops: [{ previousSubmissionId: 80, final: { taskId: 89, status: 'STOPPED', trialId: null, result: null, comparedTrials: [82], reason: 'INCOMPLETE_EXECUTION: response boundary is not a scientific stop' } }],
    trialRecords: [], tasks: [{ id: 91, name: 'pi.generation', status: 'running', error: null }], final: null,
  } }));
  await page.goto('/#/research/7'); await page.getByRole('tab', { name: '输入与执行', exact: true }).click();
  const history = page.locator('section').filter({ has: page.getByRole('heading', { name: '历史停止记录', exact: true }) });
  await expect(history).toContainText('80'); await expect(history).toContainText('89'); await expect(history).toContainText('INCOMPLETE_EXECUTION: response boundary is not a scientific stop');
  const original = page.locator('section').filter({ has: page.getByRole('heading', { name: '原始身份', exact: true }) });
  await expect(original).toContainText('owner-original-request'); await expect(original).not.toContainText('qz.research.resume');
  await expect(page.getByRole('heading', { name: '当前执行', exact: true })).toBeVisible();
});
