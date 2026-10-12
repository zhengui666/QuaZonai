import { expect, test, type Page as BrowserPage, type Route } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { DatasetView, ExtensionView, ModelView, ObservationView, PortfolioView, ResearchDetail, ResearchSummary, SessionView, SettingsView, UsageView } from '../../../packages/contracts/src/index.ts';

// Tiny test-only responses. No production samples or scientific/backend acceptance claims.
async function contract(page: BrowserPage, setup = false) {
  const at = '2026-10-01T08:00:00Z';
  const model: ModelView = { provider: 'test', modelId: 'test-model', name: '测试模型', reasoning: true };
  const dataset: DatasetView = { id: 'test-data', name: '测试数据', assetClass: 'equity', currency: 'CNY', source: 'test', start: null, end: null, ready: true };
  let session: SessionView = { setupRequired: setup, authenticated: !setup };
  let settings: SettingsView = { name: 'QuaZonai', defaultModel: model, maxTrials: 4, taskTimeoutSeconds: 60, nativeTimeoutSeconds: 120, connections: [
    { id: 'paper-test', name: '测试模拟连接', mode: 'paper', connected: false }, { id: 'live-test', name: '测试实盘连接', mode: 'live', connected: false },
  ] };
  let studies: ResearchSummary[] = [
    { id: 7, title: '测试 · 低波动与动量', status: 'RUNNING', trials: 2, model: '测试模型', createdAt: at, updatedAt: at },
    { id: 8, title: '测试 · 横截面价值', status: 'COMPUTED', trials: 4, model: '测试模型', createdAt: at, updatedAt: at },
    { id: 9, title: '测试 · ' + 'LongResearchName'.repeat(7), status: 'FAILED', trials: 1, model: '测试模型', createdAt: null, updatedAt: null },
  ];
  const detail = (): ResearchDetail => ({ ...studies[0]!, input: { goal: 'test', model, data: { hidden: 'SHOULD_NOT_RENDER' }, policy: {} }, final: { status: 'PENDING', trialId: null, reason: 'SHOULD_NOT_RENDER', metrics: [{ key: 'ic', label: 'IC', value: null }] }, trialRecords: [{ id: 11, hypothesis: '测试 · 流动性假设', status: 'COMPUTED', metrics: [{ key: 'ic', label: 'IC', value: 0.04 }], error: null }], tasks: [{ id: 12, name: '测试计算', status: 'running', error: null }] });
  const portfolio: PortfolioView = { id: 'p1', name: '测试 · 多因子组合', researchIds: [7, 8], datasetId: dataset.id, status: 'COMPUTED', capital: '100000.25', currency: 'CNY', createdAt: at, taskId: 12, error: null, metrics: [{ key: 'return', label: '收益率', value: '3.25', unit: '%' }], curve: [
    { at: '2026-09-01', value: '100000.25' }, { at: '2026-09-02', value: null }, { at: '2026-09-03', value: '101000.10' }, { at: '2026-09-04', value: '103250.26' },
  ], targets: [{ instrumentId: 'TEST.CN', weight: '0.25' }] };
  let portfolios = [portfolio];
  const observation: ObservationView = { id: 'o1', name: '测试 · 模拟账户', portfolioId: portfolio.id, mode: 'paper', status: 'OBSERVING', currency: 'CNY', equity: '103250.26', cash: null, updatedAt: at, positions: [{ instrumentId: 'TEST.CN', quantity: '100', value: null, pnl: '0' }], curve: portfolio.curve, receipt: { id: 'test-receipt', status: 'ACKNOWLEDGED', externalId: null, expiresAt: at, acknowledgedAt: null }, error: null };
  let observations = [observation];
  let extensions: ExtensionView[] = [{ name: '测试原生扩展', kind: 'native', version: '1.0', enabled: false, state: 'disabled', error: null }];
  const usage: UsageView = { activeTasks: 1, inputTokens: null, outputTokens: 2400, cost: null };
  const writes: { path: string; method: string; raw: string | null }[] = [];
  let researchAttempts = 0; let portfolioAttempts = 0; let missingExtensions = false;
  let heldResearch: Promise<void> | undefined;
  await page.route('**/api/v3/**', async (route: Route) => {
    const request = route.request(); const path = new URL(request.url()).pathname.slice('/api/v3'.length); const method = request.method();
    const reply = (json: unknown, status = 200) => route.fulfill({ status, json });
    if (method !== 'GET') writes.push({ path, method, raw: request.postData() });
    if (path === '/session') {
      if (method === 'DELETE') { session = { setupRequired: false, authenticated: false }; return route.fulfill({ status: 204 }); }
      return reply(session);
    }
    if (path === '/session/setup' || path === '/session/login') { session = { setupRequired: false, authenticated: true }; return reply(session); }
    if (path === '/settings') { if (method === 'PATCH') settings = { ...settings, ...request.postDataJSON() }; return reply(settings); }
    if (path === '/research/models') return reply([model]);
    if (path === '/datasets') return reply([dataset]);
    if (path === '/usage') return reply(usage);
    if (path === '/research') {
      if (method === 'POST') { if (++researchAttempts === 1) return route.abort('failed'); return reply({ conversationId: 7, submissionId: 17 }, 202); }
      if (heldResearch) await heldResearch;
      return reply({ items: studies });
    }
    if (path === '/research/7/stop') { studies[0]!.status = 'STOPPED'; return reply(detail(), 202); }
    if (path === '/research/7') return reply(detail());
    if (path === '/portfolios') {
      if (method === 'POST') {
        if (++portfolioAttempts === 1) return reply({ error: 'unsupported_evidence' }, 422);
        const value = { ...portfolio, id: 'p2', name: request.postDataJSON().name }; portfolios = [...portfolios, value]; return reply(value, 202);
      }
      return reply(portfolios);
    }
    if (path.startsWith('/portfolios/')) return reply(portfolios.find(value => value.id === path.split('/')[2]));
    if (path === '/observations') {
      if (method === 'POST') { const input = request.postDataJSON(); const value = { ...observation, id: 'o2', mode: input.mode }; observations = [...observations, value]; return reply(value, 202); }
      return reply(observations);
    }
    if (path.startsWith('/observations/')) { const value = observations.find(item => item.id === path.split('/')[2])!; if (path.endsWith('/stop')) value.status = 'STOPPED'; else value.cash = '1000.25'; return reply(value); }
    if (path === '/extensions') return missingExtensions ? reply({ error: 'not_found' }, 404) : reply(extensions);
    if (path.startsWith('/extensions/')) { extensions = [{ ...extensions[0]!, enabled: request.postDataJSON().enabled, state: 'pending_restart' }]; return reply(extensions[0]); }
    return reply({ error: 'not_found' }, 404);
  });
  return { writes, empty: () => { studies = []; portfolios = []; observations = []; extensions = []; }, missing: () => { missingExtensions = true; }, hold: () => { let release!: () => void; heldResearch = new Promise<void>(resolve => { release = resolve; }); return () => { heldResearch = undefined; release(); }; } };
}
async function choose(page: BrowserPage, label: string, option: string) { await page.getByLabel(label, { exact: true }).click(); await page.getByRole('option', { name: option, exact: true }).click(); }
async function audit(page: BrowserPage) {
  await expect(page.locator('.ant-select-dropdown:visible')).toHaveCount(0);
  await expect(page.locator('.ant-message-notice')).toHaveCount(0);
  expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze()).violations).toEqual([]);
}
async function fits(page: BrowserPage, width: number) { await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width); }

test('auth setup/login and persisted research retry keep the exact v3 body', async ({ page }, info) => {
  const state = await contract(page, true); await page.goto('/');
  await expect(page.getByRole('heading', { name: '设置密码', exact: true })).toBeVisible();
  expect(state.writes).toEqual([]); await audit(page);
  await page.getByLabel('密码', { exact: true }).fill('test-only-password'); await page.getByLabel('确认密码', { exact: true }).fill('test-only-password');
  await page.getByRole('button', { name: '设置并登录', exact: true }).click(); await expect(page.getByRole('heading', { name: '研究看板', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '退出登录', exact: true }).click(); await expect(page.getByRole('heading', { name: '欢迎回来' })).toBeVisible();
  await page.getByLabel('密码', { exact: true }).fill('test-only-password'); await page.getByRole('checkbox', { name: '记住登录' }).check(); await page.getByRole('button', { name: '登录', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究看板', exact: true })).toBeVisible(); await expect(page.locator('.study-card')).toHaveCount(3);
  await choose(page, '研究状态', '进行中'); await expect(page.locator('.study-card')).toHaveCount(1); await choose(page, '研究状态', '全部状态'); await expect(page.locator('.study-card')).toHaveCount(3);
  await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('research-1440.png'), fullPage: true }); await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究目标', { exact: true }).fill('测试目标'); await choose(page, '数据集', '测试数据 · CNY'); await choose(page, '模型', '测试模型 · test');
  await page.getByRole('button', { name: '开始研究', exact: true }).click(); await expect(page.getByText('连接失败', { exact: true })).toBeVisible();
  const first = state.writes.find(value => value.path === '/research')!.raw!; const original = JSON.parse(first);
  expect(original).toEqual({ requestId: expect.any(String), goal: '测试目标', model: { provider: 'test', modelId: 'test-model' }, datasetId: 'test-data', maxTrials: 4 });
  await page.reload(); await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await expect(page.getByLabel('研究目标', { exact: true })).toHaveValue('测试目标'); await expect(page.getByLabel('研究目标', { exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '重试提交', exact: true }).click(); await expect(page.getByRole('heading', { name: '测试 · 低波动与动量', exact: true })).toBeVisible();
  expect(state.writes.filter(value => value.path === '/research').map(value => value.raw)).toEqual([first, first]);
  await expect(page.locator('body')).not.toContainText('SHOULD_NOT_RENDER'); await expect(page.locator('pre')).toHaveCount(0);
  await page.getByRole('button', { name: '停止研究', exact: true }).click(); await expect(page.getByText('已停止', { exact: true }).last()).toBeVisible(); await expect(page.getByRole('button', { name: '停止研究' })).toBeDisabled();
  await fits(page, 1440); await page.screenshot({ path: info.outputPath('research-detail-1440.png'), fullPage: true });
});

test('portfolio, account observation, settings and extension actions use real DTOs', async ({ page }, info) => {
  const state = await contract(page); await page.goto('/#portfolios');
  await expect(page.getByRole('heading', { name: '共享账户回测' })).toBeVisible(); await expect(page.locator('[data-testid="equity-chart"] canvas')).toBeVisible();
  await page.getByText('曲线明细', { exact: true }).click(); await expect(page.getByRole('cell', { name: '100000.25', exact: true })).toBeVisible(); await expect(page.getByRole('cell', { name: '—', exact: true })).toBeVisible();
  await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('portfolios-1440.png'), fullPage: true });
  await page.getByRole('button', { name: '新建组合' }).click(); await page.getByLabel('组合名称', { exact: true }).fill('测试新组合'); await choose(page, '研究', '#7 · 测试 · 低波动与动量'); await page.keyboard.press('Escape');
  await choose(page, '数据集', '测试数据 · CNY'); await page.getByLabel('初始资本', { exact: true }).fill('100000.25'); await page.getByRole('button', { name: '开始回测' }).click();
  await expect(page.getByText('研究证据不支持', { exact: true })).toBeVisible(); await page.getByRole('button', { name: '开始回测' }).click(); await expect(page.getByRole('dialog')).toHaveCount(0);
  const portfolioRequest = JSON.parse(state.writes.find(value => value.path === '/portfolios')!.raw!); expect(portfolioRequest).toMatchObject({ researchIds: [7], weights: [{ researchId: 7, weight: '1' }], datasetId: 'test-data', capital: '100000.25', name: '测试新组合' });
  await page.getByRole('link', { name: '实盘/模拟盘观测', exact: true }).click(); await expect(page.getByRole('heading', { name: '账户曲线' })).toBeVisible();
  await page.getByRole('button', { name: '新建观测' }).click(); await choose(page, '策略组合', '测试 · 多因子组合'); await page.getByLabel('连接', { exact: true }).click();
  await expect(page.getByRole('option', { name: '测试实盘连接' })).toHaveCount(0); await expect(page.getByRole('option', { name: '测试模拟连接' })).toBeVisible(); await page.getByRole('option', { name: '测试模拟连接' }).click();
  await page.getByRole('button', { name: '开始观测' }).click(); await expect(page.getByRole('dialog')).toHaveCount(0); expect(JSON.parse(state.writes.find(value => value.path === '/observations')!.raw!)).toMatchObject({ portfolioId: 'p1', mode: 'paper', connectionId: 'paper-test' });
  await page.getByRole('button', { name: '刷新账户' }).click(); await expect(page.locator('.account-grid')).toContainText('1,000.25'); await page.getByRole('button', { name: '停止观测' }).click(); await expect(page.getByRole('button', { name: '停止观测' })).toBeDisabled();
  await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('observations-1440.png'), fullPage: true });
  await page.getByRole('tab', { name: '实盘', exact: true }).click(); await expect(page.getByText('暂无实盘观测', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /下单|买入|卖出/ })).toHaveCount(0);
  await page.getByRole('link', { name: '设置', exact: true }).click(); await page.getByLabel('应用名称', { exact: true }).fill('测试研究室'); await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByText('已保存', { exact: true })).toBeVisible(); await expect(page.getByRole('button', { name: '保存设置' })).toBeDisabled(); await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('settings-1440.png'), fullPage: true });
  expect(Object.keys(JSON.parse(state.writes.find(value => value.method === 'PATCH' && value.path === '/settings')!.raw!)).sort()).toEqual(['defaultModel', 'maxTrials', 'name', 'nativeTimeoutSeconds', 'taskTimeoutSeconds']);
  await page.getByRole('link', { name: '扩展', exact: true }).click(); await page.getByRole('switch', { name: '启用 测试原生扩展' }).click(); await expect(page.getByText('待重启', { exact: true })).toBeVisible(); await expect(page.getByRole('switch')).toBeChecked();
  await fits(page, 1440); await audit(page); await page.screenshot({ path: info.outputPath('extensions-1440.png'), fullPage: true });
});

test('390px keyboard navigation, reduced motion and honest loading/empty/missing states', async ({ page }, info) => {
  await page.setViewportSize({ width: 390, height: 844 }); await page.emulateMedia({ reducedMotion: 'reduce' }); const state = await contract(page); const release = state.hold();
  await page.goto('/'); await expect(page.getByRole('heading', { name: '研究看板', exact: true })).toBeVisible(); await expect(page.getByRole('status', { name: '加载中' }).first()).toBeVisible(); release();
  await expect(page.locator('.study-card')).toHaveCount(3); await page.keyboard.press('Tab'); await expect(page.getByRole('link', { name: '跳至内容' })).toBeFocused(); await expect(page.getByRole('link', { name: '跳至内容' })).toHaveCSS('outline-style', 'solid');
  await page.keyboard.press('Enter'); await expect(page.locator('#main-content')).toBeFocused();
  await expect(page.locator('.page')).toHaveCSS('animation-name', 'none'); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath('research-390.png'), fullPage: true });
  await page.getByRole('button', { name: '新建研究', exact: true }).click(); await expect(page.getByLabel('研究目标', { exact: true })).toBeFocused(); await page.keyboard.press('Tab'); await expect(page.getByLabel('数据集', { exact: true })).toBeFocused(); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath('research-form-390.png'), fullPage: true }); await page.getByRole('dialog').getByRole('button', { name: '关闭', exact: true }).click();
  for (const [index, [label, title, image]] of [['策略组合', '策略组合', 'portfolios'], ['实盘/模拟盘观测', '盘面观测', 'observations'], ['设置', '设置', 'settings'], ['扩展', '扩展', 'extensions']].entries()) {
    await page.getByRole('button', { name: '打开主导航' }).press('Enter'); const menu = page.getByRole('dialog', { name: '主导航' }); await expect(menu.getByRole('link')).toHaveCount(5);
    await menu.getByRole('link', { name: '研究看板', exact: true }).focus(); for (let step = 0; step <= index; step++) await page.keyboard.press('Tab');
    const link = menu.getByRole('link', { name: label!, exact: true }); await expect(link).toBeFocused(); await expect(link).toHaveCSS('outline-style', 'solid'); await link.press('Enter'); await expect(menu).toBeHidden();
    await expect(page.getByRole('heading', { name: title!, exact: true })).toBeVisible(); await expect(page.locator('.loading-state')).toHaveCount(0); await fits(page, 390); await audit(page); await page.screenshot({ path: info.outputPath(`${image}-390.png`), fullPage: true });
  }
  state.empty(); await page.goto('/#research'); await page.reload(); await expect(page.getByText('暂无研究', { exact: true })).toBeVisible();
  for (const [hash, text] of [['portfolios', '暂无策略组合'], ['observations', '暂无模拟盘观测'], ['extensions', '暂无已安装扩展']]) { await page.goto('/#' + hash); await page.reload(); await expect(page.getByText(text!, { exact: true })).toBeVisible(); await fits(page, 390); }
  state.missing(); await page.getByRole('button', { name: '刷新扩展', exact: true }).click(); await expect(page.getByRole('alert')).toContainText('记录不存在'); await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible();
  await expect(page.locator('pre')).toHaveCount(0); expect(state.writes).toEqual([]);
});
