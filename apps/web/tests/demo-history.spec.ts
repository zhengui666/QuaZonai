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
    await expect(evaluation.getByText(/当时没有未过期有效期/)).toBeVisible();
    await expect(evaluation.getByText('本评估没有发表指标；不能把缺失解释成0或通过。', { exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(evaluation).toBeHidden();
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
  await candidate.getByRole('button', { name: '评估 00000506', exact: true }).click();
  const evaluation = page.getByRole('dialog', { name: '候选研究评估', exact: true });
  await expect(evaluation.getByText('PORTFOLIO', { exact: true })).toBeVisible();
  await expect(evaluation.getByText('FIXTURE', { exact: true })).toBeVisible();
  await expect(evaluation.getByText('01990000-0000-7000-8000-000000000500', { exact: true })).toBeVisible();
  await expect(evaluation.getByText(/当时没有未过期有效期/)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(evaluation).toBeHidden();
  await expect(candidate.getByRole('button', { name: '冻结目标包', exact: true })).toBeDisabled();
  expect(failures).toEqual([]);
  expect(writes).toEqual([]);
});
