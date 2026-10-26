import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fork } from 'node:child_process';
import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import type { FactorView, ModelCatalog, ObservationView, Page as RecordPage, PortfolioView, ResearchDetail, ResearchSummary } from '../../../packages/contracts/src/index.ts';

// Real HTTP + official Faux Provider + actual Durable SQLite. The reused scientific
// subprocess is explicitly TEST_ONLY protocol; retained native/core CI owns real science.
async function service(downstream?: string) {
  const child = fork(fileURLToPath(new URL('../../service/test/browser-server.ts', import.meta.url)), [], { execArgv: ['--experimental-strip-types'], stdio: ['ignore', 'ignore', 'pipe', 'ipc'], env: { ...process.env, QZ_BROWSER_STATIC_DIR: fileURLToPath(new URL('../dist', import.meta.url)), ...(downstream ? { QZ_BROWSER_DOWNSTREAM: downstream } : {}) } });
  let stderr = ''; child.stderr?.on('data', chunk => { stderr += chunk; });
  function response<T extends { type: string }>(type: string): Promise<T> {
    return new Promise((resolve, reject) => {
      const exit = (code: number | null) => { cleanup(); reject(new Error(`Disposable service exited (${code}): ${stderr}`)); };
      const message = (value: T) => { if (value.type === 'error') { cleanup(); reject(new Error('Disposable service operation failed')); } else if (value.type === type) { cleanup(); resolve(value); } };
      const cleanup = () => { child.off('exit', exit); child.off('message', message); };
      child.once('exit', exit); child.on('message', message);
    });
  }
  const ready = await response<{ type: string; url: string }>('ready');
  return { url: ready.url,
    async restart() { const reply = response('restart'); child.send({ type: 'restart' }); await reply; },
    async calls() { const reply = response<{ type: string; calls: number }>('stats'); child.send({ type: 'stats' }); return (await reply).calls; },
    async close() { if (child.exitCode !== null) return; const exited = new Promise<void>(resolve => child.once('exit', () => resolve())); child.send({ type: 'close' }); await exited; },
  };
}
async function owner(page: Page, url: string) {
  await page.goto(url); await expect(page.getByRole('heading', { name: '设置工作台密码' })).toBeVisible();
  await page.getByLabel('密码', { exact: true }).fill('browser-owner-test-only'); await page.getByLabel('确认密码', { exact: true }).fill('browser-owner-test-only'); await page.getByRole('button', { name: '创建密码' }).click(); await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
}
async function start(page: Page, url: string, goal: string) {
  await page.getByRole('button', { name: '新建研究', exact: true }).click(); await page.getByLabel('研究名称', { exact: true }).fill(goal); await page.getByLabel('研究问题', { exact: true }).fill(goal); await page.getByLabel('经济假设与依据', { exact: true }).fill('Controlled HTTP hypothesis; protocol coverage, not scientific qualification.');
  await page.getByLabel('数据集', { exact: true }).click(); await page.getByRole('option', { name: 'Protocol dataset / USD', exact: true }).click();
  const catalog = await (await page.request.get(`${url}/api/v3/models`)).json() as ModelCatalog;
  const model = catalog.models.find(model => model.provider === 'faux')!; expect(model).toBeTruthy();
  await page.getByLabel('模型', { exact: true }).click(); await page.getByRole('option', { name: `${model.name} / ${model.provider}`, exact: true }).click();
  await page.getByLabel('试验预算', { exact: true }).fill('1'); await page.getByRole('button', { name: '开始研究', exact: true }).click();
}

test('production Web talks to a disposable real HTTP service through setup, evidence, backtest and original receipt recovery', async ({ page }, info) => {
  test.setTimeout(90_000);
  let offers = 0; let receipt: Record<string, unknown> = {};
  const clock = (BigInt(Date.now()) * 1_000_000n + 1n).toString();
  const remote = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.method === 'POST' && request.url === '/targets') { let body = ''; for await (const chunk of request) body += chunk; receipt = JSON.parse(body); offers++; request.socket.destroy(); return; }
    if (request.url?.startsWith('/targets/')) response.end(JSON.stringify({ ...receipt, status: 'ACKNOWLEDGED', externalId: 'browser-protocol-receipt', acknowledgedAt: '2026-10-01T00:00:00.000000001Z' }));
    else if (request.url === '/capabilities') response.end(JSON.stringify({ clientId: 'browser-client', accountId: 'browser-account', mode: 'paper', currency: 'USD', capitalExit: { version: 1, supported: false, readable: true } }));
    else if (request.url === '/capital-exits') response.end(JSON.stringify({ clientId: 'browser-client', accountId: 'browser-account', mode: 'paper', currency: 'USD', accountSourceId: '01900000-0000-7000-8000-000000000001', observationId: '01900000-0000-7000-8000-000000000002', controlRevision: '1', reserved: '0', exits: [] }));
    else if (request.url === '/account') response.end(JSON.stringify({ clientId: 'browser-client', accountId: 'browser-account', sequence: '9007199254740997', nativeClockNs: clock, currency: 'USD', equity: null, cash: null, positions: [] }));
    else { response.statusCode = 404; response.end('{}'); }
  });
  await new Promise<void>(resolve => remote.listen(0, '127.0.0.1', resolve));
  const remoteAddress = remote.address(); if (!remoteAddress || typeof remoteAddress === 'string') throw new Error('Test downstream address missing');
  const state = await service(`http://127.0.0.1:${remoteAddress.port}`); const url = state.url;
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  try {
    // No page.route / API interception in this test. Assets use the real service too.
    await owner(page, url); await expect(page.getByRole('heading', { name: '尚无研究' })).toBeVisible(); await start(page, url, 'HTTP protocol study, not scientific qualification');
    await expect(page.locator('.metric-primary')).toContainText('分数恒定'); await expect(page.locator('.metric-primary')).toContainText('不可用');
    const studies = await (await page.request.get(`${url}/api/v3/research`)).json() as RecordPage<ResearchSummary>; expect(studies.items).toHaveLength(1);
    const id = studies.items[0]!.id; const original = await (await page.request.get(`${url}/api/v3/research/${id}`)).json() as ResearchDetail;
    expect(original.input.specification?.name).toBe('HTTP protocol study, not scientific qualification'); expect(original.input.specification?.rationale).toContain('Controlled HTTP hypothesis'); expect(original.criterion?.status).toBe('insufficient'); expect(original.submission?.status).toBe('done'); expect(original.final?.status).toBe('COMPUTED'); expect(original.tasks.some(task => task.outcome === 'completed')).toBe(true); expect(await state.calls()).toBe(1);
    await expect(page.locator('body')).not.toContainText(String(original.input.data.catalog)); await expect(page.locator('pre')).toHaveCount(0);
    await page.getByRole('button', { name: /所选试验/ }).click(); await expect(page.getByRole('button', { name: `收起试验 ${original.final!.trialId}` })).toHaveAttribute('aria-expanded', 'true'); await expect(page.locator('.trial-detail')).toContainText('Controlled protocol'); await page.getByRole('tab', { name: '证据', exact: true }).click(); await page.screenshot({ path: info.outputPath('actual-http-research.png'), fullPage: true });
    await page.getByRole('button', { name: '创建组合', exact: true }).click(); await page.getByLabel('组合名称', { exact: true }).fill('HTTP protocol account; not native evidence'); await page.getByLabel('本金', { exact: true }).fill('1000.000000000000000001'); await page.getByRole('button', { name: '运行回测' }).click();
    await expect(page.locator('.error-notice')).toContainText('研究请求的独立指标不可用'); expect(await (await page.request.get(`${url}/api/v3/portfolios`)).json()).toEqual([]); expect(offers).toBe(0);
    const factor = (await (await page.request.get(`${url}/api/v3/factors`)).json() as FactorView[])[0]!; expect(factor.combinable).toBe(false); expect(factor.reason).toBe('research_criterion_insufficient');
    await page.goto(`${url}/#/factors/${factor.id}`); await expect(page.getByRole('button', { name: '加入组合', exact: true })).toBeDisabled(); await expect(page.locator('.capacity-section')).toContainText('research_criterion_insufficient');
    // Keep the target/receipt protocol coverage on the intentional legacy path, not fabricated structured evidence.
    const legacy = await page.request.post(`${url}/api/v3/research`, { data: { requestId: 'TEST_ONLY-legacy-transport', goal: 'Legacy HTTP transport protocol, not scientific qualification', model: original.input.model, datasetId: original.datasetId, maxTrials: 1 } }); expect(legacy.status()).toBe(202); const legacyId = (await legacy.json()).conversationId as number;
    await page.goto(`${url}/#/research/${legacyId}`); await page.getByRole('button', { name: '创建组合', exact: true }).click();
    await page.locator('.ant-select-selection-item').filter({ hasText: `#${id} / ${original.title}` }).locator('.ant-select-selection-item-remove').click();
    await page.getByLabel(`研究 ${legacyId} 权重`, { exact: true }).fill('1'); await page.getByLabel('组合名称', { exact: true }).fill('Legacy HTTP protocol account; not native evidence'); await page.getByLabel('本金', { exact: true }).fill('1000.000000000000000001'); await page.getByRole('button', { name: '运行回测' }).click();
    await expect(page.locator('[data-testid="equity-chart"] svg')).toBeVisible(); await page.getByText('查看数据', { exact: true }).click(); await expect(page.getByRole('cell', { name: '1000.000000000000000001', exact: true })).toBeVisible(); const portfolios = await (await page.request.get(`${url}/api/v3/portfolios`)).json() as PortfolioView[]; expect(portfolios).toHaveLength(1); expect(portfolios[0]!.weights).toEqual([{ researchId: legacyId, weight: '1' }]);
    await page.getByRole('button', { name: '交付目标', exact: true }).click(); await page.getByLabel('连接', { exact: true }).click(); await page.getByRole('option', { name: 'Protocol-only paper connection' }).click(); await page.getByRole('button', { name: '交付并观测' }).click(); await expect(page.locator('.receipt-section')).toContainText('待核对'); await expect(page.locator('.snapshot-section')).toContainText('尚无账户快照'); expect(offers).toBe(1);
    const before = (await (await page.request.get(`${url}/api/v3/observations`)).json() as ObservationView[])[0]!; expect(before.receipt?.status).toBe('UNCERTAIN'); await state.restart(); await page.reload(); await expect(page.locator('.receipt-section')).toContainText('待核对'); await page.getByRole('button', { name: '核对回执' }).click(); await expect(page.locator('.receipt-section')).toContainText('已接收'); await expect(page.getByRole('heading', { name: '权益数值未提供' })).toBeVisible(); await expect(page.locator('[data-testid="equity-chart"]')).toHaveCount(0); await expect(page.locator('.positions-section')).toContainText('无持仓');
    await page.getByText('来源与身份', { exact: true }).click(); await expect(page.locator('.receipt-section .provenance-disclosure')).toContainText('9007199254740997'); await expect(page.locator('.receipt-section .provenance-disclosure')).toContainText(clock);
    const after = (await (await page.request.get(`${url}/api/v3/observations`)).json() as ObservationView[])[0]!;
    expect(after.receipt?.id).toBe(before.receipt?.id); expect(after.receipt?.expiresAt).toBe(before.receipt?.expiresAt); expect(after.equity).toBeNull(); expect(after.cash).toBeNull(); expect(after.provenance?.sequence).toBe('9007199254740997'); expect(offers).toBe(1); expect(await state.calls()).toBe(2);
    await page.getByRole('button', { name: '停止观测', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '停止观测', exact: true }).click(); await expect(page.getByRole('button', { name: '停止观测', exact: true })).toBeDisabled(); await expect(page.locator('.receipt-section')).toContainText('已接收'); expect(offers).toBe(1); expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze()).violations).toEqual([]); expect(errors).toEqual([]); await page.screenshot({ path: info.outputPath('actual-http-observation.png'), fullPage: true });
  } finally { await state.close(); await new Promise<void>(resolve => remote.close(() => resolve())); }
});

test('late original admission cannot erase a newer pending command', async ({ page }) => {
  test.setTimeout(90_000); const state = await service(); const url = state.url; const bodies: string[] = []; let releaseA!: () => void; let releaseB!: () => void; const heldA = new Promise<void>(resolve => { releaseA = resolve; }); const heldB = new Promise<void>(resolve => { releaseB = resolve; }); let lateFinished = false;
  try {
    await owner(page, url);
    await page.route(`${url}/api/v3/research`, async route => {
      if (route.request().method() !== 'POST') return route.continue();
      const raw = route.request().postData()!; bodies.push(raw); const ordinal = bodies.length; const response = await route.fetch(); expect(response.status()).toBe(202);
      if (ordinal === 1) await heldA; if (ordinal === 3) await heldB;
      await route.fulfill({ response }).catch(() => undefined); if (ordinal === 1) lateFinished = true;
    });
    await start(page, url, 'First delayed original'); await expect.poll(() => bodies.length).toBe(1); await page.getByRole('link', { name: '设置', exact: true }).click(); await page.getByRole('navigation', { name: '设置分类' }).getByRole('link', { name: '数据', exact: true }).click(); await expect(page.getByRole('heading', { name: '数据', exact: true, level: 2 })).toBeVisible(); await page.getByRole('navigation', { name: '主要工作区' }).getByRole('link', { name: '研究', exact: true }).click(); await page.getByRole('button', { name: '新建研究', exact: true }).click(); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('First delayed original'); await page.getByRole('button', { name: '核对提交', exact: true }).click(); await expect(page.getByRole('heading', { name: 'First delayed original', level: 1, exact: true })).toBeVisible();
    await start(page, url, 'Second pending original'); await expect.poll(() => bodies.length).toBe(3); const second = bodies[2]!;
    releaseA(); await expect.poll(() => lateFinished).toBe(true); await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    expect(await page.evaluate(() => sessionStorage.getItem('quazonai.command.research.v3'))).toBe(second); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('Second pending original');
    await page.reload(); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('Second pending original'); await expect(page.getByLabel('研究问题', { exact: true })).toBeDisabled(); await page.getByRole('button', { name: '核对提交', exact: true }).click(); await expect(page.getByRole('heading', { name: 'Second pending original', level: 1, exact: true })).toBeVisible();
    expect(bodies[1]).toBe(bodies[0]); expect(bodies[3]).toBe(second); expect((await (await page.request.get(`${url}/api/v3/research`)).json() as RecordPage<ResearchSummary>).items).toHaveLength(2); expect(await state.calls()).toBe(2);
  } finally { releaseA(); releaseB(); await state.close(); }
});

test('a deliberately lost real HTTP admission response resolves the same durable study after reload', async ({ page }) => {
  const state = await service(); const url = state.url; const bodies: string[] = [];
  try {
    await owner(page, url);
    await page.route(`${url}/api/v3/research`, async route => { if (route.request().method() !== 'POST') return route.continue(); bodies.push(route.request().postData()!); const response = await route.fetch(); expect(response.status()).toBe(202); if (bodies.length === 1) return route.abort('failed'); return route.fulfill({ response }); });
    await start(page, url, 'Lost HTTP response protocol only'); await expect(page.getByRole('button', { name: '核对提交', exact: true })).toBeVisible(); await expect(page.locator('.pending-command')).toContainText('提交结果待核对'); await page.reload(); await expect(page.getByLabel('研究问题', { exact: true })).toHaveValue('Lost HTTP response protocol only'); await expect(page.getByLabel('研究问题', { exact: true })).toBeDisabled(); await page.getByRole('button', { name: '核对提交', exact: true }).click(); await expect(page.getByRole('heading', { name: 'Lost HTTP response protocol only', exact: true, level: 1 })).toBeVisible(); await expect(page.locator('.metric-primary')).toContainText('分数恒定'); expect(bodies).toHaveLength(2); expect(bodies[1]).toBe(bodies[0]); expect(await state.calls()).toBe(1); const studies = await (await page.request.get(`${url}/api/v3/research`)).json() as RecordPage<ResearchSummary>; expect(studies.items).toHaveLength(1);
  } finally { await state.close(); }
});

test('object deletion confirms the original resource, reconciles a lost response and persists after restart', async ({ page }) => {
  test.setTimeout(90_000); const state = await service(); const url = state.url;
  try {
    await owner(page, url);
    const models = await (await page.request.get(`${url}/api/v3/research/models`)).json() as ModelCatalog['models'];
    const model = models.find(item => item.provider === 'faux')!;
    const datasets = await (await page.request.get(`${url}/api/v3/datasets`)).json() as { id: string }[];
    const created = await page.request.post(`${url}/api/v3/research`, { data: { requestId: 'TEST_ONLY-delete-original-study', goal: 'Deletion HTTP lifecycle, not scientific qualification', model: { provider: model.provider, modelId: model.modelId }, datasetId: datasets[0]!.id, maxTrials: 1 } });
    expect(created.status()).toBe(202);
    let study!: ResearchDetail;
    await expect.poll(async () => {
      const studies = await (await page.request.get(`${url}/api/v3/research`)).json() as RecordPage<ResearchSummary>;
      if (!studies.items[0]) return false;
      study = await (await page.request.get(`${url}/api/v3/research/${studies.items[0].id}`)).json() as ResearchDetail;
      return study.final?.status === 'COMPUTED' && study.tasks.every(task => task.status === 'terminal');
    }).toBe(true);
    const factors = await (await page.request.get(`${url}/api/v3/factors`)).json() as FactorView[];
    expect(factors).toHaveLength(1); const factor = factors[0]!;
    const build = await page.request.post(`${url}/api/v3/portfolios`, { data: { requestId: 'TEST_ONLY-delete-original-portfolio', name: 'Disposable deletion protocol portfolio', datasetId: study.datasetId, researchIds: [study.id], weights: [{ researchId: study.id, weight: '1' }], capital: '1000' } });
    expect(build.status()).toBe(202); const portfolio = await build.json() as PortfolioView;
    await expect.poll(async () => ((await (await page.request.get(`${url}/api/v3/portfolios/${portfolio.id}`)).json()) as PortfolioView).status).toBe('COMPUTED');
    await page.goto(`${url}/#/portfolios/${portfolio.id}`);
    await page.getByRole('button', { name: '删除组合', exact: true }).click();
    const dialog = page.getByRole('dialog'); await expect(dialog).toContainText('原始试验、证据与回执历史保留');
    await dialog.getByRole('button', { name: '取消', exact: true }).click();
    expect((await page.request.get(`${url}/api/v3/portfolios/${portfolio.id}`)).status()).toBe(200);
    await page.getByRole('button', { name: '删除组合', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '确认删除', exact: true }).click();
    await expect(page).toHaveURL(`${url}/#/portfolios`); await expect(page.getByRole('heading', { name: '尚无策略组合', exact: true })).toBeVisible();
    await page.goto(`${url}/#/factors/${factor.id}`); await page.getByRole('button', { name: '删除因子', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '确认删除', exact: true }).click();
    await expect(page).toHaveURL(`${url}/#/factors`); await expect(page.getByRole('heading', { name: '尚无匹配原子因子', exact: true })).toBeVisible();
    const deletes: string[] = [];
    await page.route(`${url}/api/v3/research/${study.id}`, async route => {
      if (route.request().method() !== 'DELETE') return route.continue();
      deletes.push(route.request().url()); const response = await route.fetch(); expect(response.status()).toBe(200);
      if (deletes.length === 1) return route.abort('failed'); return route.fulfill({ response });
    });
    await page.goto(`${url}/#/research/${study.id}`); await page.getByRole('button', { name: '删除研究', exact: true }).click(); await page.getByRole('dialog').getByRole('button', { name: '确认删除', exact: true }).click();
    await expect(page.getByRole('dialog')).toContainText('连接失败');
    const originalDeletion = await page.evaluate(() => sessionStorage.getItem('quazonai.draft.pending-deletions'));
    expect(JSON.parse(originalDeletion!)).toEqual([{ kind: 'research', id: String(study.id), path: `/research/${study.id}` }]);
    await page.reload();
    const recovery = page.getByRole('region', { name: '待核对删除' });
    await expect(recovery).toContainText(String(study.id));
    await expect(page.locator('.study-document .error-notice')).toContainText('服务或记录不存在');
    expect(deletes).toHaveLength(1); // Reload and a 404 read never send another DELETE or claim its success.
    expect(await page.evaluate(() => sessionStorage.getItem('quazonai.draft.pending-deletions'))).toBe(originalDeletion);
    await recovery.getByRole('button', { name: '核对删除研究', exact: true }).click();
    await page.getByRole('dialog').getByRole('button', { name: '核对原删除', exact: true }).click();
    await expect(page).toHaveURL(`${url}/#/research`); expect(deletes).toHaveLength(2); expect(deletes[0]).toBe(deletes[1]); await expect(page.getByRole('heading', { name: '尚无研究', exact: true })).toBeVisible();
    await expect(recovery).toHaveCount(0); expect(await page.evaluate(() => sessionStorage.getItem('quazonai.draft.pending-deletions'))).toBeNull();
    await state.restart(); await page.reload(); await expect(page.getByRole('heading', { name: '尚无研究', exact: true })).toBeVisible();
    expect((await page.request.get(`${url}/api/v3/research/${study.id}`)).status()).toBe(404);
    expect(await (await page.request.get(`${url}/api/v3/factors`)).json()).toEqual([]); expect(await (await page.request.get(`${url}/api/v3/portfolios`)).json()).toEqual([]);
  } finally { await state.close(); }
});

test('settings configures independent remote instances through real HTTP and preserves them across reload and restart', async ({ page }, info) => {
  test.setTimeout(90_000);
  const state = await service(); const url = state.url;
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  try {
    await owner(page, url);
    await page.getByRole('link', { name: '设置', exact: true }).click();
    await expect(page.getByRole('heading', { name: '设置', exact: true, level: 1 })).toBeVisible();
    await expect(page.getByRole('link', { name: '模型设置', exact: true })).toBeVisible();
    await page.getByRole('link', { name: '实例连接', exact: true }).click();
    const before = await (await page.request.get(`${url}/api/v3/settings/instances`)).json();
    await page.getByRole('switch', { name: '启用回测连接', exact: true }).check();
    const backtest = page.locator('.instance-backtest');
    await backtest.getByLabel('实例标识', { exact: true }).fill('science-remote');
    await backtest.getByLabel('服务地址', { exact: true }).fill('https://science.example/nautilus');
    await backtest.getByLabel('访问令牌', { exact: true }).fill('test-science-token');
    await expect(backtest.getByLabel('访问令牌', { exact: true })).toHaveAttribute('type', 'password');
    for (const [mode, name] of [['paper', '独立模拟盘'], ['live', '独立实盘']] as const) {
      await page.getByRole('button', { name: '添加连接', exact: true }).click();
      const connection = page.locator('.instance-connection').last();
      await connection.getByLabel('名称', { exact: true }).fill(name);
      await connection.getByLabel('实例标识', { exact: true }).fill(`${mode}-remote`);
      const modeSelect = connection.getByLabel('模式', { exact: true });
      await modeSelect.click();
      const listboxId = await modeSelect.getAttribute('aria-controls'); expect(listboxId).toBeTruthy();
      await page.locator(`[id="${listboxId}"]`).getByRole('option', { name: mode === 'paper' ? '模拟盘' : '实盘', exact: true }).click();
      await connection.getByLabel('服务地址', { exact: true }).fill(`https://${mode}.example/nautilus`);
      await connection.getByLabel('客户端标识', { exact: true }).fill(`qz-${mode}`);
      await connection.getByLabel('账户标识', { exact: true }).fill(`${mode}-account`);
      await connection.getByLabel('访问令牌', { exact: true }).fill(`test-${mode}-token`);
      await expect(connection.getByLabel('访问令牌', { exact: true })).toHaveAttribute('type', 'password');
    }
    await page.getByRole('button', { name: '保存实例连接', exact: true }).click();
    await expect(page.getByText('已保存', { exact: true })).toBeVisible();
    const saved = await (await page.request.get(`${url}/api/v3/settings/instances`)).json();
    expect(saved).toEqual({ revision: before.revision + 1, backtest: { id: 'science-remote', url: 'https://science.example/nautilus', token: 'test-science-token' }, connections: [
      { id: 'paper-remote', name: '独立模拟盘', mode: 'paper', url: 'https://paper.example/nautilus', clientId: 'qz-paper', accountId: 'paper-account', token: 'test-paper-token' },
      { id: 'live-remote', name: '独立实盘', mode: 'live', url: 'https://live.example/nautilus', clientId: 'qz-live', accountId: 'live-account', token: 'test-live-token' },
    ] });
    await page.reload(); await expect(backtest.getByLabel('服务地址', { exact: true })).toHaveValue(saved.backtest.url);
    await expect(page.locator('.instance-connection')).toHaveCount(2);
    await expect(page.getByRole('button', { name: '保存实例连接', exact: true })).toBeDisabled();
    await state.restart(); await page.reload();
    await expect(backtest.getByLabel('访问令牌', { exact: true })).toHaveValue('test-science-token');
    await expect(backtest.getByLabel('访问令牌', { exact: true })).toHaveAttribute('type', 'password');
    for (const [index, mode] of ['paper', 'live'].entries()) {
      const token = page.locator('.instance-connection').nth(index).getByLabel('访问令牌', { exact: true });
      await expect(token).toHaveValue(`test-${mode}-token`); await expect(token).toHaveAttribute('type', 'password');
    }
    await expect(page.locator('.instance-connection').nth(0).getByLabel('账户标识', { exact: true })).toHaveValue('paper-account');
    await expect(page.locator('.instance-connection').nth(1).getByLabel('账户标识', { exact: true })).toHaveValue('live-account');
    expect(await (await page.request.get(`${url}/api/v3/settings/instances`)).json()).toEqual(saved);
    const workspace = await (await page.request.get(`${url}/api/v3/workspace`)).json();
    expect(workspace.connections.map((value: { id: string }) => value.id)).toEqual(['paper-remote', 'live-remote']);
    await page.screenshot({ path: info.outputPath('settings-instances-real-http-1440.png'), fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole('button', { name: '打开导航', exact: true }).click();
    await page.getByRole('dialog').getByRole('link', { name: '设置', exact: true }).click();
    await page.getByRole('link', { name: '实例连接', exact: true }).click();
    await expect(page.getByRole('button', { name: '保存实例连接', exact: true })).toBeVisible();
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze()).violations).toEqual([]);
    await page.screenshot({ path: info.outputPath('settings-instances-real-http-390.png'), fullPage: true });
    await page.getByRole('link', { name: '扩展', exact: true }).click();
    await expect(page.locator('.extension-list')).toBeVisible();
    await page.goto(`${url}/#/extensions`); await expect(page.getByRole('heading', { name: '设置', exact: true, level: 1 })).toBeVisible();
    await expect(page.locator('.extension-list')).toBeVisible();
    expect(errors).toEqual([]);
  } finally { await state.close(); }
});
