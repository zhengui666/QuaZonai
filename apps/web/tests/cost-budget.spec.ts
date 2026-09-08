// SYNTHETIC form/dispatch regression, not native accounting or research evidence.
import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';
import { initialBudget, initialStop } from '../src/brief-fields';
import { fixture, id, project, reply } from './fixtures';

async function editor(page: Page, overrides: Partial<Schema['BudgetV1']> = {}) {
  const state = await fixture(page);
  const draft: Schema['BriefView'] = {
    id: id(10), project_id: project.id, version: 1, revision: '1', state: 'DRAFT',
    content: { hypothesis: 'Synthetic cost field regression', economic_rationale: 'No production evidence',
      universe_version_id: id(11), target_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1',
      base_currency: 'USD', benchmark_ref: null, evaluation_policy_id: id(12), execution_assumptions_id: id(13),
      budget: { ...initialBudget, ...overrides }, stop_rule: { ...initialStop } },
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
  const state = await editor(page, { cost_enforcement: 'UNAVAILABLE', max_cost_decimal: '12.50', cost_currency: 'USD' });
  await expect(page.getByLabel('费用上限（估算模式必填）')).toHaveValue('12.50');
  await expect(page.getByLabel('费用币种（估算模式必填）')).toHaveValue('USD');
  await save(page).click();
  await expect(page.getByText('没有费用度量时，金额必须留空。', { exact: true })).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await mode(page, '估算值（不等于实际账单）');
  await mode(page, '没有可用费用度量');
  await expect(page.getByLabel('费用上限（估算模式必填）')).toHaveValue('');
  await expect(page.getByLabel('费用币种（估算模式必填）')).toHaveValue('');
  expect(state.commands).toHaveLength(0);
});

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
