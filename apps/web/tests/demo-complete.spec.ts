// Complete credential-free SYNTHETIC interaction. It proves Demo behavior only;
// no account, native model, Runtime, qualification or production delivery executes.
import { expect, test } from '@playwright/test';
import { navigate } from './fixtures';

test.use({ baseURL: 'http://127.0.0.1:4179' });

async function choose(page: import('@playwright/test').Page, label: string, text: string) {
  const field = page.getByRole('combobox', { name: label, exact: true });
  await field.click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: text }).click();
  await expect(field).toHaveAttribute('aria-expanded', 'false');
}

test('credential-free Demo freezes context, starts a synthetic Cycle and reaches research-to-package evidence', async ({ page }) => {
  test.setTimeout(90_000);
  const failed: string[] = [];
  page.on('pageerror', error => failed.push(error.message));
  page.on('response', response => {
    if (new URL(response.url()).pathname.startsWith('/api/') && response.status() >= 500) failed.push(`${response.status()} ${response.url()}`);
  });

  await page.goto('/');
  await expect(page.getByRole('note', { name: '合成预览说明' })).toContainText('SYNTHETIC / FIXTURE');
  await page.getByRole('button', { name: 'SYNTHETIC · 双 Alpha 研究示例', exact: true }).click();
  await page.getByRole('button', { name: '查看冻结版本', exact: true }).click();
  await page.getByRole('button', { name: '以此创建新版本', exact: true }).click();
  const brief = page.getByRole('dialog', { name: 'Brief · 版本 1 的新草稿', exact: true });
  await brief.getByLabel('可检验的假设', { exact: true }).fill('SYNTHETIC · 完整交互 Demo，不构成真实研究结论');
  await brief.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(brief).toBeHidden();

  const row = page.getByRole('row').filter({ hasText: 'SYNTHETIC · 完整交互 Demo' });
  await row.getByRole('button', { name: '冻结执行上下文', exact: true }).click();
  await choose(page, '选择执行 Runtime', 'SYNTHETIC · Demo Runtime');
  for (const purpose of ['DISCOVERY', 'VALIDATION', 'SEALED']) {
    const field = page.getByRole('combobox', { name: `选择 ${purpose} 输入集`, exact: true });
    await field.click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').first().click();
  }
  await page.getByRole('button', { name: '确认冻结 Brief', exact: true }).click();
  await expect(page.getByText('Brief 已冻结，尚未启动研究。', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();

  const project = page.getByText('SYNTHETIC · 双 Alpha 研究示例', { exact: true }).first();
  await expect(project).toBeVisible();
  const activate = page.getByRole('button', { name: '修改项目状态', exact: true });
  await activate.click();
  const editor = page.getByRole('dialog', { name: '编辑研究项目', exact: true });
  await editor.getByLabel('项目状态', { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: '启用' }).click();
  await editor.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(editor).toBeHidden();

  await page.getByRole('button', { name: '启动新 Cycle', exact: true }).click();
  await choose(page, '选择研究者 Codex 配置', 'SYNTHETIC · Demo Researcher');
  await choose(page, '选择独立 Reviewer Codex 配置', 'SYNTHETIC · Demo Reviewer');
  await page.getByRole('button', { name: '确认启动 Cycle', exact: true }).click();
  await expect(page.getByText('Cycle 与准备运行已由服务器登记。', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await expect(page.getByText('QUALIFIED_CANDIDATES', { exact: true })).toBeVisible();
  await expect(page.getByText(/SYNTHETIC · 查看双 Alpha/)).toBeVisible();

  await navigate(page, 'Alpha');
  await page.getByRole('combobox', { name: '选择 Alpha 所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await expect(page.getByRole('button', { name: 'SYNTHETIC · 历史展示 1', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'SYNTHETIC · 历史展示 2', exact: true })).toBeVisible();

  await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await expect(page.getByRole('button', { name: '01990000-0000-7000-8000-000000000500', exact: true })).toBeVisible();

  await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await expect(page.getByRole('button', { name: 'Release 00000510', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Release 00000510', exact: true }).click();
  await expect(page.getByText(/SYNTHETIC \/ FIXTURE/).first()).toBeVisible();
  await expect(page.getByText(/不能审批、登记 Offer 或领取/).first()).toBeVisible();
  expect(failed).toEqual([]);
});
