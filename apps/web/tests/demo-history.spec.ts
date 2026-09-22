// Browser acceptance of SYNTHETIC history; not evidence of numerical execution.
import { expect, test } from '@playwright/test';
import { navigate } from './fixtures';

test.use({ baseURL: 'http://127.0.0.1:4179' });

test('synthetic two-Alpha history keeps expired qualification and original portfolio evidence visible', async ({ page }) => {
  test.setTimeout(60_000);
  const failures: string[] = [];
  const writes: string[] = [];
  page.on('pageerror', error => failures.push(error.message));
  page.on('response', response => {
    if (new URL(response.url()).pathname.startsWith('/api/') && !response.ok()) failures.push(`${response.status()} ${response.url()}`);
  });
  page.on('request', request => {
    if (new URL(request.url()).pathname.startsWith('/api/') && request.method() !== 'GET') writes.push(request.method());
  });
  await page.goto('/');
  await navigate(page, 'Alpha');
  await page.getByRole('combobox', { name: '选择 Alpha 所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  for (const n of [1, 2]) {
    await page.getByRole('button', { name: `SYNTHETIC · 历史展示 ${n}`, exact: true }).click();
    await page.getByRole('button', { name: '版本 1', exact: true }).click();
    await page.getByRole('button', { name: '查看原资格历史', exact: true }).click();
    const qualification = page.getByRole('dialog', { name: '原资格历史', exact: true });
    await expect(qualification.getByRole('cell', { name: /^观察时刻未开放 / })).toBeVisible();
    await expect(qualification.getByText('没有撤销记录', { exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(qualification).toBeHidden();
    await page.getByRole('button', { name: `评估 0000042${n - 1}`, exact: true }).click();
    const evaluation = page.getByRole('dialog', { name: '正式 Validation 评估', exact: true });
    await expect(evaluation.getByText('SUCCEEDED / VALID / PASS', { exact: true })).toBeVisible();
    await expect(evaluation.getByText('FIXTURE', { exact: true })).toBeVisible();
    await expect(evaluation.getByText(/无有效期/)).toBeVisible();
  await expect(evaluation.getByText('暂无指标', { exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(evaluation).toBeHidden();
    await expect(page.getByRole('button', { name: `评估 0000049${n - 1}`, exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: '返回 Alpha 列表', exact: true }).click();
  }
  await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: '01990000-0000-7000-8000-000000000500', exact: true }).click();
  const candidate = page.getByRole('dialog', { name: '不可变候选快照', exact: true });
  for (const n of [0, 1]) await expect(candidate.getByRole('cell', { name: `01990000-0000-7000-8000-00000000041${n}`, exact: true })).toBeVisible();
  await expect(candidate.getByRole('cell', { name: '0.5', exact: true })).toHaveCount(2);
  await expect(candidate.getByRole('cell', { name: 'SYNTHETIC.EXAMPLE', exact: true })).toBeVisible();
  const equityResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/evaluations/01990000-0000-7000-8000-000000000506/equity-curve');
  await candidate.getByRole('button', { name: '评估 00000506', exact: true }).click();
  const evaluation = page.getByRole('dialog', { name: '候选研究评估', exact: true });
  await expect(evaluation.getByText('PORTFOLIO', { exact: true })).toBeVisible();
  await expect(evaluation.getByText('FIXTURE', { exact: true })).toBeVisible();
  await expect(evaluation.getByText('01990000-0000-7000-8000-000000000500', { exact: true })).toBeVisible();
  await expect(evaluation.getByText(/无有效期/)).toBeVisible();
  const equity = await equityResponse;
  expect(equity.status()).toBe(200);
  expect(await equity.json()).toMatchObject({ origin: 'FIXTURE', curve: { status: 'UNAVAILABLE', reason_code: 'NO_SIMULATION' } });
  const history = evaluation.getByRole('region', { name: '历史回测组合价值', exact: true });
  await expect(history.getByText('本次研究未产生权益数据', { exact: true })).toBeVisible();
  await expect(history.getByTestId('equity-chart')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(evaluation).toBeHidden();
  await expect(candidate.getByRole('button', { name: '冻结目标包', exact: true })).toBeDisabled();
  expect(failures).toEqual([]);
  expect(writes).toEqual([]);
});

test('new synthetic draft can be created, edited and opened without inheriting research', async ({ page }, testInfo) => {
  const name = `SYNTHETIC · 临时项目 ${testInfo.project.name}`;
  await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  const create = page.getByRole('dialog', { name: '新建研究项目', exact: true });
  await create.getByLabel('研究名称', { exact: true }).fill(name);
  await create.getByLabel('研究说明', { exact: true }).fill('只在此预览内存中保存');
  const created = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/projects' && response.request().method() === 'POST');
  await create.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(create).toBeHidden();
  const project = (await (await created).json()).resource.id as string;
  for (const route of ['artifacts', 'experiments', 'input-sets']) {
    const response = await page.request.get(`/api/v2/${route}`, { params: { project_id: project, limit: 50 } });
    expect(response.status()).toBe(200);
    expect(await response.json()).toEqual({ schema_version: 1, items: [], next_cursor: null });
    expect((await page.request.get(`/api/v2/${route}`, { params: { project_id: project, limit: 0 } })).status()).toBe(422);
  }

  const row = page.getByRole('row').filter({ has: page.getByRole('button', { name, exact: true }) });
  await expect(row.getByText('草稿', { exact: true })).toBeVisible();
  await row.getByRole('button', { name: '编辑', exact: true }).click();
  const edit = page.getByRole('dialog', { name: '编辑研究项目', exact: true });
  await edit.getByLabel('研究说明', { exact: true }).fill('修改后的临时说明');
  await edit.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(edit).toBeHidden();
  await page.getByRole('button', { name, exact: true }).click();
  await expect(page.getByText('修改后的临时说明', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await expect(page.getByText('暂无研究周期', { exact: true })).toBeVisible();
  await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: name }).click();
  await expect(page.getByRole('button', { name: 'Release 00000510', exact: true })).toBeHidden();
  await page.getByRole('tab', { name: '交付记录', exact: true }).click();
  await expect(page.getByText('暂无交付记录', { exact: true })).toBeVisible();
  await navigate(page, '研究');
  for (const state of ['暂停', '归档']) {
    const row = page.getByRole('row').filter({ has: page.getByRole('button', { name, exact: true }) });
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    const editor = page.getByRole('dialog', { name: '编辑研究项目', exact: true });
    await editor.getByLabel('项目状态', { exact: true }).click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: state }).click();
    await editor.getByRole('button', { name: '保存项目', exact: true }).click();
    await expect(editor).toBeHidden();
    await expect(row.getByText(state, { exact: true })).toBeVisible();
  }
});

test('synthetic Brief fork and edit preserve the frozen version', async ({ page }, testInfo) => {
  const hypothesis = `SYNTHETIC draft ${testInfo.project.name}`;
  await page.goto('/');
  await page.getByRole('button', { name: 'SYNTHETIC · 双 Alpha 研究示例', exact: true }).click();
  await page.getByRole('button', { name: '查看冻结版本', exact: true }).click();
  await page.getByRole('button', { name: '以此创建新版本', exact: true }).click();
  const editor = page.getByRole('dialog', { name: 'Brief · 版本 1 的新草稿', exact: true });
  await editor.getByLabel('可检验的假设', { exact: true }).fill(hypothesis);
  const response = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname.endsWith('/briefs'));
  await editor.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  const created = await response; expect(created.status()).toBe(201);
  const { resource } = await created.json();
  await expect(editor).toBeHidden();
  const row = page.getByRole('row').filter({ hasText: hypothesis });
  await row.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  const editing = page.getByRole('dialog', { name: `Brief · 版本 ${resource.version}`, exact: true });
  await editing.getByLabel('可检验的假设', { exact: true }).fill(`${hypothesis} revised`);
  await editing.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(editing).toBeHidden();
  await expect(page.getByText(`${hypothesis} revised`, { exact: true })).toBeVisible();
  const inputs = page.waitForResponse(r => new URL(r.url()).pathname === '/api/v2/input-sets');
  await row.getByRole('button', { name: '冻结执行上下文', exact: true }).click();
  expect((await inputs).status()).toBe(200);
  const preparation = page.getByRole('dialog', { name: '冻结 Brief 执行上下文', exact: true });
  await expect(preparation.getByRole('button', { name: '确认冻结 Brief', exact: true })).toBeDisabled();
  await preparation.getByRole('button', { name: '返回', exact: true }).click();
  await expect(preparation).toBeHidden();
  await page.getByRole('button', { name: '查看冻结版本', exact: true }).click();
  const frozen = page.getByRole('dialog', { name: 'Brief · 版本 1', exact: true });
  await expect(frozen.getByLabel('可检验的假设', { exact: true })).toHaveValue('SYNTHETIC：比较两种合成信号。');
  await expect(frozen.getByText('只读版本', { exact: true })).toBeVisible();
  const denied = await page.request.post(`/api/v2/briefs/${resource.id}/freeze`, { data: {} });
  expect(denied.status()).toBe(422);
});

test('frozen synthetic execution context is visible without enabling a real Cycle', async ({ page }) => {
  const projectId = '01990000-0000-7000-8000-000000000001';
  const path = `/api/v2/projects/${projectId}`;
  // Other viewport workers share this in-memory scene; honor native CAS conflicts.
  for (let attempt = 0; attempt < 3; attempt++) {
    const current = await (await page.request.get(path)).json();
    if (current.state === 'ACTIVE') break;
    const changed = await page.request.patch(path, { headers: { 'Idempotency-Key': `context-${Date.now()}-${attempt}` }, data: {
      schema_version: 1, expected_revision: current.revision, name: current.name, description: current.description, state: 'ACTIVE',
    } });
    if (changed.status() === 409) continue;
    expect(changed.status()).toBe(200); break;
  }
  expect((await (await page.request.get(path)).json()).state).toBe('ACTIVE');
  const failed: string[] = []; const starts: string[] = [];
  page.on('response', response => { if (new URL(response.url()).pathname.startsWith('/api/') && !response.ok()) failed.push(String(response.status())); });
  page.on('request', request => { if (request.method() === 'POST' && new URL(request.url()).pathname.endsWith('/cycles')) starts.push(request.url()); });
  await page.goto('/');
  await page.getByRole('button', { name: 'SYNTHETIC · 双 Alpha 研究示例', exact: true }).click();
  await page.getByRole('button', { name: '启动新 Cycle', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '确认启动研究 Cycle', exact: true });
  await expect(dialog.getByText('冻结 Runtime / 修订', { exact: true })).toBeVisible();
  for (const role of ['DISCOVERY', 'VALIDATION', 'SEALED']) await expect(dialog.getByText(`${role} 输入`, { exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '确认启动 Cycle', exact: true })).toBeDisabled();
  expect(starts).toEqual([]); expect(failed).toEqual([]);
});