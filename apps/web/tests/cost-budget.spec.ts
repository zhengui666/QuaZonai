// SYNTHETIC form/dispatch regression, not native accounting or research evidence.
import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';
import { initialBudget, initialStop } from '../src/brief-fields';
import { fixture, id, project, reply } from './fixtures';

async function editor(page: Page, budget: Schema['BudgetV1'] = initialBudget) {
  const state = await fixture(page);
  const draft: Schema['BriefView'] = {
    id: id(10), project_id: project.id, version: 1, revision: '1', state: 'DRAFT',
    content: { hypothesis: 'Synthetic cost field regression', economic_rationale: 'No production evidence',
      universe_version_id: id(11), target_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1',
      base_currency: 'USD', benchmark_ref: null, evaluation_policy_id: id(12), execution_assumptions_id: id(13),
      budget, stop_rule: { ...initialStop } },
    bindings: [{ dataset_revision_id: id(14), role: 'DISCOVERY', access_policy: 'METADATA_ONLY' }],
    supersedes_id: null, frozen_at: null, created_at: '2026-09-08T00:00:00Z', updated_at: '2026-09-08T00:00:00Z',
  };
  await page.route(url => url.pathname === `/api/v2/projects/${project.id}/briefs`, route => reply(route, { schema_version: 1, items: [draft], next_cursor: null }));
  await page.goto('/');
  await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  return state;
}
async function mode(page: Page, label: string) {
  const select = page.getByLabel('费用约束方式');
  await select.scrollIntoViewIfNeeded(); await select.click();
  const option = page.locator('.ant-select-dropdown:visible').getByText(label, { exact: true });
  await option.scrollIntoViewIfNeeded(); await option.click();
}
const save = (page: Page) => page.getByRole('button', { name: '保存 Brief 草稿', exact: true });

test('estimated mode requires a positive amount and native currency before HTTP dispatch', async ({ page }) => {
  const state = await editor(page);
  await mode(page, '估算值（不等于实际账单）');
  await save(page).click();
  await expect(page.getByText('估算费用必须是严格大于零的精确十进制金额。', { exact: true })).toBeVisible();
  await expect(page.getByText('请选择服务器原生币种表支持的币种代码。', { exact: true })).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await page.getByLabel('费用上限（估算模式必填）').fill('0');
  await page.getByLabel('费用币种（估算模式必填）').fill('ZZZ');
  await save(page).click();
  await expect(page.getByText('请选择服务器原生币种表支持的币种代码。', { exact: true })).toBeVisible();
  expect(state.commands).toHaveLength(0);
});

test('explicitly disabling metering clears amount and currency but loading never does', async ({ page }) => {
  const state = await editor(page, { ...initialBudget, cost_enforcement: 'ESTIMATED', max_cost_decimal: '12.50', cost_currency: 'USD' });
  await expect(page.getByLabel('费用上限（估算模式必填）')).toHaveValue('12.50');
  await expect(page.getByLabel('费用币种（估算模式必填）')).toHaveValue('USD');
  expect(state.commands).toHaveLength(0);
  await mode(page, '没有可用费用度量');
  await expect(page.getByLabel('费用上限（估算模式必填）')).toHaveValue('');
  await expect(page.getByLabel('费用币种（估算模式必填）')).toHaveValue('');
  expect(state.commands).toHaveLength(0);
});

for (const amount of ['+000.0100', '.1', '1.']) {
  test(`native decimal spelling ${amount} reaches HTTP unchanged`, async ({ page }) => {
    const state = await editor(page);
    await mode(page, '估算值（不等于实际账单）');
    await page.getByLabel('费用上限（估算模式必填）').fill(amount);
    await page.getByLabel('费用币种（估算模式必填）').fill('USD');
    await save(page).click();
    await expect.poll(() => state.commands.filter(command => command.method === 'PATCH').length).toBe(1);
    expect(state.commands.find(command => command.method === 'PATCH')?.body).toMatchObject({
      content: { budget: { cost_enforcement: 'ESTIMATED', max_cost_decimal: amount, cost_currency: 'USD' } },
    });
  });
}

test('valid estimates preserve the exact decimal string in the dispatched request', async ({ page }) => {
  const state = await editor(page);
  await mode(page, '估算值（不等于实际账单）');
  const amount = '12345678901234567890.123456789012345678';
  await page.getByLabel('费用上限（估算模式必填）').fill(amount);
  await page.getByLabel('费用币种（估算模式必填）').fill('USD');
  await save(page).click();
  await expect.poll(() => state.commands.filter(command => command.method === 'PATCH').length).toBe(1);
  expect(state.commands.find(command => command.method === 'PATCH')?.body).toMatchObject({
    content: { budget: { cost_enforcement: 'ESTIMATED', max_cost_decimal: amount, cost_currency: 'USD' } },
  });
  // Dispatch is the evidence here; this fixture does not assert a server commit.
});

test('nonexistent base currency and repeated dataset identity cannot submit', async ({ page }) => {
  const state = await editor(page);
  await page.getByLabel('基础币种（ISO 4217）').fill('AAA');
  await save(page).click();
  await expect(page.getByText('基础币种必须属于服务器原生币种表。', { exact: true })).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await page.getByLabel('基础币种（ISO 4217）').fill('USD');
  await page.getByRole('button', { name: '添加数据绑定', exact: true }).click();
  await page.getByLabel('数据集版本', { exact: true }).nth(1).fill(id(14));
  await save(page).click();
  await expect(page.getByText('同一个数据集版本只能绑定一次，不能通过更改角色或访问边界重复添加。', { exact: true })).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await page.getByRole('button', { name: '删除绑定 2', exact: true }).click();
  await expect(page.getByText('同一个数据集版本只能绑定一次，不能通过更改角色或访问边界重复添加。', { exact: true })).toHaveCount(0);
});

test('budget relationships revalidate both edited bounds without expanding limits', async ({ page }) => {
  const state = await editor(page);
  await page.getByLabel('每个 Mission 最大轮次').fill('1');
  await page.getByLabel('最大实验数', { exact: true }).fill('1');
  await save(page).click();
  await expect(page.getByText('最大修复轮次不能超过每个 Mission 最大轮次。', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('合格 Alpha 目标数不能超过最大实验数。', { exact: true }).first()).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await page.getByLabel('最大修复轮次', { exact: true }).fill('0');
  await page.getByLabel('合格 Alpha 目标数', { exact: true }).fill('1');
  await expect(page.getByText('最大修复轮次不能超过每个 Mission 最大轮次。', { exact: true })).toHaveCount(0);
  await expect(page.getByText('合格 Alpha 目标数不能超过最大实验数。', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('每个 Mission 最大轮次')).toHaveValue('1');
  await expect(page.getByLabel('最大实验数', { exact: true })).toHaveValue('1');
  await save(page).click();
  await expect.poll(() => state.commands.filter(command => command.method === 'PATCH').length).toBe(1);
});
