import { expect, test, type Page } from '@playwright/test';
import { id, source, exit, preview } from '../src/capital-exit-fixtures';
import type { ExitView } from '../src/capital-exit-state';
const endpoint = '/tests/fixtures/capital-exits.html';
async function fixture(page: Page, mode: 'normal' | 'unknown' | 'blocked' | 'conflict' = 'normal', storedExit?: ExitView) {
  const writes: { path: string; key: string | undefined; body: any }[] = [];
  let latest = storedExit; let startCount = 0; let previewCount = 0; let latestPreview = preview;
  const future = new Date(Date.now() + 3600_000).toISOString();
  const initial = { ...exit, state: 'REQUESTED' as const, funds: { ...exit.funds, evidence_asof: new Date().toISOString(), evidence_valid_until: future } };
  const current = { source: { binding: { native_account_id: 'CASH-001', environment: 'PAPER' }, id: source }, valuation: 'PRICED', latest_snapshot: { id, received_at: new Date().toISOString(), observation: { snapshot: { ts_event: '1791331200000000000' } } } };
  // Only the local component server and intercepted fixture transport are used.
  // Abort native API/proxy and external requests even if the UI regresses.
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.origin !== 'http://127.0.0.1:4176' || url.pathname.startsWith('/api/') || url.pathname.startsWith('/health/')) await route.abort();
    else await route.continue();
  });
  if (storedExit) await page.addInitScript(({ project, account, intent }) => {
    sessionStorage.setItem(`quazonai:capital-exit:v1:${project}:${account}`, JSON.stringify({ version: 1, lastIntent: intent }));
  }, { project: id, account: source, intent: storedExit.id });
  await page.route('**/fixture-api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (request.method() === 'GET') {
      await route.fulfill({ json: path.endsWith('/current') ? current : latest ?? initial }); return;
    }
    const body = request.postDataJSON(); writes.push({ path, key: request.headers()['idempotency-key'], body });
    if (path.endsWith('/capital-exit-previews')) {
      latestPreview = { ...preview, id: ++previewCount === 1 ? id : source, original_observation_id: body.expected_source_observation_id, valid_until: future, scope: body.scope, policy: body.policy, funds: { ...preview.funds, requested: { amount: body.scope.amount, currency: body.scope.currency } }, capability: mode === 'blocked' ? 'BLOCKED' : 'SUPPORTED', reason_codes: mode === 'blocked' ? ['account_owner_binding_unavailable'] : [] };
      await route.fulfill({ status: 201, json: { resource: latestPreview } }); return;
    }
    if (path.endsWith('/capital-exits')) {
      startCount++;
      if (mode === 'conflict') { await route.fulfill({ status: 409, json: { code: 'STALE_PREVIEW', detail: 'Preview changed; refresh required', status: 409, request_id: id, field_errors: [] } }); return; }
      latest = initial;
      if (mode === 'unknown' && startCount === 1) { await route.abort(); return; }
    } else if (body.action === 'CANCEL') latest = { ...initial, state: 'CANCELLED_RESERVED', last_phase: 'REDUCING' };
    else if (body.action === 'PAUSE') latest = { ...initial, state: 'PAUSED', last_phase: 'REDUCING' };
    else if (body.action === 'RESUME') latest = { ...(latest ?? initial), state: 'REQUESTED', preview_id: latestPreview.id, scope: latestPreview.scope, policy: latestPreview.policy };
    await route.fulfill({ status: 202, json: { resource: latest } });
  });
  return { writes, setExit: (value: ExitView) => { latest = value; }, setObservation: (value: string) => { current.latest_snapshot.id = value; } };
}
async function plan(page: Page) {
  await page.getByLabel('申请退出金额', { exact: true }).fill('9007199254740993.123456789');
  await page.getByLabel('账户抵押币种', { exact: true }).fill('USD');
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await expect(page.getByText('原始退出影响预览', { exact: true })).toBeVisible();
}
test('duplicate start, close and reopen retain one intent; cancel retains partial funds', async ({ page }) => {
  const { writes } = await fixture(page); await page.goto(endpoint); await plan(page);
  await page.getByRole('checkbox', { name: '我已核对账户' }).check();
  await page.getByRole('button', { name: '开始退出', exact: true }).dblclick();
  await expect(page.getByText('已请求，等待原生确认', { exact: true })).toBeVisible();
  expect(writes.filter(write => write.path.endsWith('/capital-exits'))).toHaveLength(1);
  await page.getByRole('button', { name: /^关\s*闭$/ }).click();
  await page.getByRole('button', { name: '重新打开账户' }).click();
  await expect(page.getByText('禁止再投入', { exact: true })).toBeVisible();
  expect(writes.filter(write => write.body.action === 'CANCEL')).toHaveLength(0);
  await page.getByRole('button', { name: '取消后续退出', exact: true }).click();
  await page.getByRole('checkbox', { name: '确认此操作及上述后果' }).check();
  await page.getByRole('button', { name: '提交此操作', exact: true }).click();
  await expect(page.getByText('后续退出已取消，资金仍保留', { exact: true })).toBeVisible();
  expect(writes.at(-1)?.body.action).toBe('CANCEL');
  await expect(page.getByText('10 USD', { exact: true }).first()).toBeVisible();
});
test('lost response and browser reload replay only original body/key', async ({ page }) => {
  const { writes } = await fixture(page, 'unknown'); await page.goto(endpoint); await plan(page);
  await page.getByRole('checkbox', { name: '我已核对账户' }).check(); await page.getByRole('button', { name: '开始退出', exact: true }).click();
  await expect(page.getByText('原操作结果未知；不要创建新请求', { exact: true })).toBeVisible();
  const original = writes.at(-1)!; await page.reload();
  await expect(page.getByRole('button', { name: '按原请求核对结果', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '按原请求核对结果', exact: true }).click();
  await expect(page.getByText('已请求，等待原生确认', { exact: true })).toBeVisible();
  expect(writes.at(-1)?.key).toBe(original.key); expect(writes.at(-1)?.body).toEqual(original.body);
  expect(writes.at(-1)?.body.expected_account_control_revision).toBe('9007199254740993');
});
test('blocked capabilities and stale start conflicts require a fresh plan', async ({ page }) => {
  const { writes } = await fixture(page, 'conflict'); await page.goto(endpoint); await plan(page);
  await page.getByRole('checkbox', { name: '我已核对账户' }).check(); await page.getByRole('button', { name: '开始退出', exact: true }).click();
  await expect(page.getByText('Preview changed; refresh required', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '开始退出', exact: true })).toHaveCount(0);
  expect(writes.filter(write => write.path.endsWith('/capital-exits'))).toHaveLength(1);
});
test('mobile retains constraints and honest blocked simulation without horizontal scrolling', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); await fixture(page, 'blocked'); await page.goto(endpoint); await plan(page);
  await expect(page.getByText('account_owner_binding_unavailable', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '开始退出', exact: true })).toBeDisabled();
  await expect(page.getByText('PAPER / SANDBOX · 模拟', { exact: true }).first()).toBeVisible();
  const overflow = await page.locator('.ant-drawer-body').evaluate(element => element.scrollWidth > element.clientWidth + 1);
  expect(overflow).toBe(false);
});

function boundedRecord(deadline: string): ExitView {
  return {
    ...exit,
    scope: { kind: 'PORTFOLIO_SCOPE', amount: exit.scope.amount, currency: 'USD', stream_ids: [id, source], release_ids: [exit.id, id] },
    funds: { ...exit.funds, reserved_amount: { amount: '9007199254740992.000000001', currency: 'USD' } },
    policy: {
      kind: 'BOUNDED_LIMIT', deadline,
      legs: [{ instrument_id: 'BTC-USD.NATIVE', maximum_reduction_quantity: '9007199254740993.123456789', minimum_sell_price: '1234567890123456.123456789' }],
      max_execution_cost: { amount: '9007199254740991.123456789', currency: 'USD', reference_evidence_id: source },
    },
  };
}
async function restoredFields(page: Page, record: ExitView) {
  if (record.policy.kind !== 'BOUNDED_LIMIT' || record.scope.kind !== 'PORTFOLIO_SCOPE') throw new Error('bounded portfolio fixture required');
  const leg = record.policy.legs[0]!;
  for (const [label, value] of [
    ['申请退出金额', record.funds.reserved_amount.amount], ['账户抵押币种', record.funds.reserved_amount.currency],
    ['已有目标流 ID（以逗号或换行分隔）', record.scope.stream_ids.join('\n')], ['已有发布 ID（以逗号或换行分隔）', record.scope.release_ids.join('\n')],
    ['原生交易标的 ID', leg.instrument_id], ['最大累计减仓数量', leg.maximum_reduction_quantity], ['最低卖出价格', leg.minimum_sell_price],
    ['截止时间（RFC 3339，含时区）', record.policy.deadline], ['最大增量执行成本（同币种）', record.policy.max_execution_cost.amount],
    ['原始价格 / 成本参考证据 ID', record.policy.max_execution_cost.reference_evidence_id],
  ]) await expect(page.getByLabel(label!, { exact: true })).toHaveValue(value!);
  await expect(page.getByText('指定已有组合目标流', { exact: true })).toBeVisible();
  await expect(page.getByText('允许明确约束的限价减仓', { exact: true })).toBeVisible();
}

test('return, close and reopen restore every exact bounded field without sending cancel', async ({ page }) => {
  const record = boundedRecord('2099-10-09T02:03:04.123456789+02:30');
  const { writes } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await page.getByRole('button', { name: '继续退出', exact: true }).click(); await restoredFields(page, record);
  await page.getByLabel('最大累计减仓数量', { exact: true }).fill('1');
  await page.getByLabel('最大增量执行成本（同币种）', { exact: true }).fill('0');
  await page.getByRole('button', { name: '返回详情', exact: true }).click();
  await page.getByRole('button', { name: '继续退出', exact: true }).click(); await restoredFields(page, record);
  await page.getByRole('button', { name: /^关\s*闭$/ }).click();
  await page.getByRole('button', { name: '重新打开账户', exact: true }).click();
  await page.getByRole('button', { name: '继续退出', exact: true }).click(); await restoredFields(page, record);
  await page.getByRole('button', { name: '返回详情', exact: true }).click();
  await page.getByRole('button', { name: '取消后续退出', exact: true }).click();
  await page.getByRole('button', { name: '返回详情', exact: true }).click();
  expect(writes).toHaveLength(0);
});

test('expired restored deadline stays unchanged and blocks preview and resume until manually edited', async ({ page }) => {
  const record = boundedRecord('2020-10-07T02:03:04.123456789+02:30');
  const { writes } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await page.getByRole('button', { name: '继续退出', exact: true }).click(); await restoredFields(page, record);
  await expect(page.getByText('截止时间已过期，请手动修改后重新预览', { exact: true })).toBeVisible();
  await page.getByRole('checkbox', { name: '确认此操作及上述后果' }).check();
  await expect(page.getByRole('button', { name: '提交此操作', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await expect(page.getByText('请输入带时区的未来时间', { exact: true })).toBeVisible();
  await restoredFields(page, record); expect(writes).toHaveLength(0);
  const deadline = '2099-10-09T02:03:04.123456789+02:30';
  await page.getByLabel('截止时间（RFC 3339，含时区）', { exact: true }).fill(deadline);
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await expect(page.getByText('原始退出影响预览', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(1);
  expect(writes[0]!.body.policy).toEqual({ ...record.policy, deadline });
  expect(writes[0]!.body.scope).toEqual({ ...record.scope, amount: record.funds.reserved_amount.amount });
});

for (const state of ['REQUESTED', 'REDUCING'] as const) test(`${state} cannot request resume`, async ({ page }) => {
  const { writes } = await fixture(page, 'normal', { ...exit, state }); await page.goto(endpoint);
  const resume = page.getByRole('button', { name: '继续退出', exact: true });
  await expect(resume).toBeDisabled();
  await resume.evaluate(element => (element as HTMLButtonElement).click());
  await expect(page.getByRole('button', { name: '预览影响', exact: true })).toHaveCount(0);
  expect(writes).toHaveLength(0);
});

for (const state of ['CANCELLED_RESERVED', 'CANCELLING_EXIT', 'COMPLETED'] as const) test(`${state} cannot request pause`, async ({ page }) => {
  const { writes } = await fixture(page, 'normal', { ...exit, state }); await page.goto(endpoint);
  const pause = page.getByRole('button', { name: '暂停退出', exact: true });
  if (state === 'COMPLETED') {
    await expect(page.getByRole('button', { name: '新建资金退出', exact: true })).toBeVisible();
    await expect(pause).toHaveCount(0);
  } else await expect(pause).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test('new evidence and every new preview require fresh plan and action confirmation before one resume', async ({ page }) => {
  const record = boundedRecord('2099-10-09T02:03:04.123456789+02:30');
  const { writes, setObservation } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await page.getByRole('button', { name: '继续退出', exact: true }).click();
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  const approval = page.getByRole('checkbox', { name: '我已核对账户' });
  const confirmation = page.getByRole('checkbox', { name: '确认此操作及上述后果' });
  const submit = page.getByRole('button', { name: '提交此操作', exact: true });
  await approval.check(); await confirmation.check(); await expect(submit).toBeEnabled();
  setObservation(source);
  await page.getByRole('button', { name: '刷新可提证据', exact: true }).click();
  await expect(approval).toBeDisabled(); await expect(submit).toBeDisabled();
  await expect(page.getByRole('button', { name: '预览影响', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await expect(approval).toBeEnabled(); await expect(approval).not.toBeChecked(); await expect(confirmation).not.toBeChecked();
  await expect(submit).toBeDisabled();
  await approval.check(); await expect(submit).toBeDisabled();
  await confirmation.check(); await expect(submit).toBeEnabled();
  await submit.dblclick();
  await expect(page.getByText('已请求，等待原生确认', { exact: true })).toBeVisible();
  const resumes = writes.filter(write => write.body.action === 'RESUME');
  expect(resumes).toHaveLength(1);
  expect(resumes[0]!.body).toEqual({ schema_version: 1, action: 'RESUME', expected_revision: record.revision, preview_id: source });
  expect(writes.filter(write => write.body.action === 'CANCEL')).toHaveLength(0);
});

test('a state change while confirming resume prevents submitting the stale action', async ({ page }) => {
  const record = boundedRecord('2099-10-09T02:03:04.123456789+02:30');
  const { writes, setExit } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await page.getByRole('button', { name: '继续退出', exact: true }).click();
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await page.getByRole('checkbox', { name: '我已核对账户' }).check();
  await page.getByRole('checkbox', { name: '确认此操作及上述后果' }).check();
  await expect(page.getByRole('button', { name: '提交此操作', exact: true })).toBeEnabled();
  setExit({ ...record, state: 'REDUCING' });
  await page.getByRole('button', { name: '刷新可提证据', exact: true }).click();
  await expect(page.getByText('正在限价减仓', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '提交此操作', exact: true })).toBeDisabled();
  expect(writes.filter(write => write.body.action === 'RESUME')).toHaveLength(0);
});

test('deadline expiring after approval blocks resume while the preview is still valid', async ({ page }) => {
  const clock = Date.now();
  await page.clock.install({ time: new Date(clock) });
  const record = boundedRecord(new Date(clock + 120_000).toISOString());
  const { writes } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await page.getByRole('button', { name: '继续退出', exact: true }).click();
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await page.getByRole('checkbox', { name: '我已核对账户' }).check();
  await page.getByRole('checkbox', { name: '确认此操作及上述后果' }).check();
  await expect(page.getByRole('button', { name: '提交此操作', exact: true })).toBeEnabled();
  await page.clock.fastForward(121_000);
  await expect(page.getByText('截止时间已过期，请手动修改后重新预览', { exact: true })).toBeVisible();
  await expect(page.getByText('预览尚在有效期内', { exact: false })).toBeVisible();
  await expect(page.getByRole('button', { name: '提交此操作', exact: true })).toBeDisabled();
  expect(writes.filter(write => write.body.action === 'RESUME')).toHaveLength(0);
});

test('an unsupported multiple-leg record cannot silently become a single-leg resume', async ({ page }) => {
  const record = boundedRecord('2099-10-09T02:03:04.123456789+02:30');
  if (record.policy.kind !== 'BOUNDED_LIMIT') throw new Error('bounded fixture required');
  record.policy.legs.push({ ...record.policy.legs[0]!, instrument_id: 'ETH-USD.NATIVE' });
  const { writes } = await fixture(page, 'normal', record); await page.goto(endpoint);
  await expect(page.getByText('当前仅支持单腿限价减仓，无法继续此退出', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '继续退出', exact: true })).toBeDisabled();
  expect(writes).toHaveLength(0);
});
