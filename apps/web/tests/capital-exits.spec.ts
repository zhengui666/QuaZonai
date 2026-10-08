import { expect, test, type Page } from '@playwright/test';
import { id, source, exit, preview } from '../src/capital-exit-fixtures';
const endpoint = '/tests/fixtures/capital-exits.html';
async function fixture(page: Page, mode: 'normal' | 'unknown' | 'blocked' | 'conflict' = 'normal') {
  const writes: { path: string; key: string | undefined; body: any }[] = [];
  let latest: typeof exit | undefined; let startCount = 0;
  const future = new Date(Date.now() + 3600_000).toISOString();
  const initial = { ...exit, state: 'REQUESTED' as const, funds: { ...exit.funds, evidence_asof: new Date().toISOString(), evidence_valid_until: future } };
  const current = { source: { binding: { native_account_id: 'CASH-001', environment: 'PAPER' }, id: source }, valuation: 'PRICED', latest_snapshot: { id, received_at: new Date().toISOString(), observation: { snapshot: { ts_event: '1791331200000000000' } } } };
  await page.route('**/fixture-api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (request.method() === 'GET') {
      await route.fulfill({ json: path.endsWith('/current') ? current : latest ?? initial }); return;
    }
    const body = request.postDataJSON(); writes.push({ path, key: request.headers()['idempotency-key'], body });
    if (path.endsWith('/capital-exit-previews')) {
      await route.fulfill({ status: 201, json: { resource: { ...preview, valid_until: future, scope: body.scope, policy: body.policy, funds: { ...preview.funds, requested: { amount: body.scope.amount, currency: body.scope.currency } }, capability: mode === 'blocked' ? 'BLOCKED' : 'SUPPORTED', reason_codes: mode === 'blocked' ? ['account_owner_binding_unavailable'] : [] } } }); return;
    }
    if (path.endsWith('/capital-exits')) {
      startCount++;
      if (mode === 'conflict') { await route.fulfill({ status: 409, json: { code: 'STALE_PREVIEW', detail: 'Preview changed; refresh required', status: 409, request_id: id, field_errors: [] } }); return; }
      latest = initial;
      if (mode === 'unknown' && startCount === 1) { await route.abort(); return; }
    } else if (body.action === 'CANCEL') latest = { ...initial, state: 'CANCELLED_RESERVED', last_phase: 'REDUCING' };
    else if (body.action === 'PAUSE') latest = { ...initial, state: 'PAUSED', last_phase: 'REDUCING' };
    await route.fulfill({ status: 202, json: { resource: latest } });
  });
  return writes;
}
async function plan(page: Page) {
  await page.getByLabel('申请退出金额', { exact: true }).fill('9007199254740993.123456789');
  await page.getByLabel('账户抵押币种', { exact: true }).fill('USD');
  await page.getByRole('button', { name: '预览影响', exact: true }).click();
  await expect(page.getByText('原始退出影响预览', { exact: true })).toBeVisible();
}
test('duplicate start, close and reopen retain one intent; cancel retains partial funds', async ({ page }) => {
  const writes = await fixture(page); await page.goto(endpoint); await plan(page);
  await page.getByRole('checkbox', { name: '我已核对账户' }).check();
  await page.getByRole('button', { name: '开始退出', exact: true }).dblclick();
  await expect(page.getByText('已请求，等待原生确认', { exact: true })).toBeVisible();
  expect(writes.filter(write => write.path.endsWith('/capital-exits'))).toHaveLength(1);
  await page.getByRole('button', { name: '关闭', exact: true }).last().click();
  await page.getByRole('button', { name: '重新打开账户' }).click();
  await expect(page.getByText('禁止再投入', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '取消后续退出', exact: true }).click();
  await page.getByRole('checkbox', { name: '确认此操作及上述后果' }).check();
  await page.getByRole('button', { name: '提交此操作', exact: true }).click();
  await expect(page.getByText('后续退出已取消，资金仍保留', { exact: true })).toBeVisible();
  expect(writes.at(-1)?.body.action).toBe('CANCEL');
  await expect(page.getByText('10 USD', { exact: true }).first()).toBeVisible();
});
test('lost response and browser reload replay only original body/key', async ({ page }) => {
  const writes = await fixture(page, 'unknown'); await page.goto(endpoint); await plan(page);
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
  const writes = await fixture(page, 'conflict'); await page.goto(endpoint); await plan(page);
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
