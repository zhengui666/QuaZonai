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
  for (const title of ['组合', '交付', '运行', '设置']) {
    await navigate(page, title);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
  }
  const denied = await page.request.post('/api/v2/handoffs/arbitrary/claim', { data: {} });
  expect(denied.status()).toBe(403);
  expect((await denied.json()).detail).toContain('不执行');
  expect(failures).toEqual([]);
});
