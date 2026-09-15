import { expect, test } from '@playwright/test';
import { navigate } from './fixtures';

test.use({ baseURL: 'http://127.0.0.1:4179' });

test('synthetic preview renders native-contract records without a backend or writable delivery', async ({ page }) => {
  const failures: string[] = [];
  page.on('pageerror', error => failures.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('note', { name: '合成预览说明' })).toContainText('尚非完整 Demo');
  await expect(page.getByRole('button', { name: 'SYNTHETIC · 双 Alpha 研究示例', exact: true })).toBeVisible()
    .catch(error => { throw new Error(`${error.message}\nBrowser errors: ${failures.join('; ')}`); });
  await navigate(page, 'Alpha');
  await page.getByRole('combobox', { name: '选择 Alpha 所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  for (const n of [1, 2]) await expect(page.getByRole('button', { name: `SYNTHETIC · 信号 ${n}`, exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'SYNTHETIC · 信号 1', exact: true }).click();
  await page.getByRole('button', { name: '版本 1', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Alpha 版本 1', exact: true })).toBeVisible();
  await expect(page.getByText('FIXTURE', { exact: true }).first()).toBeVisible();
  await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await page.getByRole('button', { name: '配置 v1', exact: true }).click();
  const mandate = page.getByRole('dialog', { name: '不可变组合配置', exact: true });
  await expect(mandate.getByText(/clarabel::solver::DefaultSolver/)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(mandate).toBeHidden();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await page.getByRole('button', { name: '01990000-0000-7000-8000-000000000203', exact: true }).click();
  const candidate = page.getByRole('dialog', { name: '不可变候选快照', exact: true });
  await expect(candidate.getByText('SYNTHETIC_NO_QUALIFIED_ALPHA', { exact: true })).toBeVisible();
  await expect(candidate.getByText('无目标，不补造权重或现金。', { exact: true })).toBeVisible();
  await expect(candidate.getByRole('button', { name: '请求组合 Study', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(candidate).toBeHidden();
  await page.getByRole('tab', { name: '执行假设', exact: true }).click();
  await page.getByRole('button', { name: /^查看假设 / }).click();
  const assumptions = page.getByRole('dialog', { name: '不可变执行假设', exact: true });
  await expect(assumptions.getByText('CONSERVATIVE_ASSUMPTION · 保守假设，不是数据支持成本证明', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(assumptions).toBeHidden();
  await page.getByRole('tab', { name: '评估政策', exact: true }).click();
  await page.getByRole('button', { name: '政策 v1', exact: true }).click();
  const policy = page.getByRole('dialog', { name: '不可变评估政策', exact: true });
  await expect(policy.getByLabel('原完整评估政策', { exact: true })).toContainText('SYNTHETIC · 真实数据与独立证据仍是资格前提');
  await page.keyboard.press('Escape');
  await expect(policy).toBeHidden();
  for (const title of ['交付', '运行', '设置']) {
    await navigate(page, title);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
  }
  const denied = await page.request.post('/api/v2/handoffs/arbitrary/claim', { data: {} });
  expect(denied.status()).toBe(403);
  expect((await denied.json()).detail).toContain('不执行');
  expect(failures).toEqual([]);
});
