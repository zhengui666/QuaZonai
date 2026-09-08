// SYNTHETIC presentation tests: no frozen/qualified production records are created.
import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { fixture, id, project, reply } from './fixtures';
import { initialBudget, initialStop } from '../src/brief-fields';
import type { Schema } from '../src/api';

function brief(state: 'DRAFT' | 'FROZEN' = 'FROZEN'): Schema['BriefView'] {
  return {
    id: id(10), project_id: project.id, version: 1, revision: '1', state,
    content: {
      hypothesis: 'Synthetic UI field validation', economic_rationale: 'Not native research evidence',
      universe_version_id: id(11), target_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1',
      base_currency: 'USD', benchmark_ref: null, evaluation_policy_id: id(12), execution_assumptions_id: id(13),
      budget: { ...initialBudget }, stop_rule: { ...initialStop },
    },
    bindings: [{ dataset_revision_id: id(14), role: 'DISCOVERY', access_policy: 'METADATA_ONLY' }],
    supersedes_id: null, frozen_at: state === 'FROZEN' ? '2026-09-08T00:00:00Z' : null,
    created_at: '2026-09-08T00:00:00Z', updated_at: '2026-09-08T00:00:00Z',
  };
}
async function menu(page: Page, label: string) {
  await page.getByLabel(label).click();
  return page.locator('.ant-select-dropdown:visible .ant-select-item-option-content');
}
async function choose(page: Page, label: string, value: string) {
  await (await menu(page, label)).filter({ hasText: new RegExp(`^${value}$`) }).click();
  const control = page.getByLabel(label);
  await expect(control).toHaveAttribute('aria-expanded', 'false');
  // A click completing is not proof that a controlled form accepted the choice.
  // Observe this field's committed presentation before changing a dependent field.
  const field = page.locator('.ant-form-item').filter({ has: control });
  // The v6 content and its leaving popup can both contain the same label.
  // Observe the committed selection and wait for the real popup to finish closing.
  await expect(field.locator('.ant-select-content')).toHaveText(value);
  await expect(field.locator('.ant-select-dropdown:visible')).toHaveCount(0);
}

test('archived projects never offer an exit transition', async ({ page }) => {
  const state = await fixture(page);
  state.projects = [{ ...project, state: 'ARCHIVED', archived_at: '2026-09-08T00:00:00Z' }];
  await page.goto('/'); await page.getByRole('button', { name: '编辑', exact: true }).click();
  await expect(await menu(page, '项目状态')).toHaveText(['归档']);
  expect(state.commands).toHaveLength(0);
});

test('a missing or non-frozen current Brief never enables ACTIVE', async ({ page }) => {
  const state = await fixture(page);
  let current: Schema['BriefView'] | undefined;
  await page.route(url => url.pathname === `/api/v2/briefs/${id(10)}`, route => reply(route, current));
  for (const variant of ['missing', 'draft', 'foreign'] as const) {
    current = variant === 'foreign' ? { ...brief(), project_id: id(90) } : brief('DRAFT');
    state.projects = [{ ...project, current_brief_id: variant === 'missing' ? null : id(10) }];
    await page.goto('/'); await page.getByRole('button', { name: '编辑', exact: true }).click();
    const options = await menu(page, '项目状态');
    await expect(options.filter({ hasText: /^启用$/ })).toHaveCount(0);
    await expect(options).toHaveCount(3);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: '取消', exact: true }).click();
  }
  expect(state.commands).toHaveLength(0);
});

test('ACTIVE appears only after the exact frozen Brief response is read', async ({ page }) => {
  const state = await fixture(page);
  state.projects = [{ ...project, current_brief_id: id(10) }];
  let release: (() => void) | undefined;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route(url => url.pathname === `/api/v2/briefs/${id(10)}`, async route => { await gate; await reply(route, brief()); });
  await page.goto('/'); await page.getByRole('button', { name: '编辑', exact: true }).click();
  await expect((await menu(page, '项目状态')).filter({ hasText: /^启用$/ })).toHaveCount(0);
  await page.keyboard.press('Escape');
  const response = page.waitForResponse(url => new URL(url.url()).pathname === `/api/v2/briefs/${id(10)}`);
  release?.(); await response;
  await expect((await menu(page, '项目状态')).filter({ hasText: /^启用$/ })).toBeVisible();
  expect(state.commands).toHaveLength(0);
});

test('partition changes clear incompatible access rather than granting a stronger scope', async ({ page }) => {
  const state = await fixture(page);
  const draft = brief('DRAFT');
  await page.route(url => url.pathname === `/api/v2/projects/${project.id}/briefs`, route => reply(route, { schema_version: 1, items: [draft], next_cursor: null }));
  await page.goto('/'); await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  await choose(page, '访问边界', 'RESEARCH_READ');
  await choose(page, '数据角色', 'SEALED');
  await expect(page.getByText('请选择兼容的访问边界', { exact: true })).toBeVisible();
  const sealed = await menu(page, '访问边界');
  await expect(sealed).toHaveText(['METADATA_ONLY', 'EVALUATOR_ONLY']);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(page.getByText('访问边界必须符合所选数据角色，请明确重选。')).toBeVisible();
  expect(state.commands).toHaveLength(0);
  await choose(page, '访问边界', 'EVALUATOR_ONLY');
  await choose(page, '数据角色', 'VALIDATION');
  await expect(page.getByText('请选择兼容的访问边界', { exact: true })).toBeVisible();
  await choose(page, '访问边界', 'METADATA_ONLY');
  await choose(page, '数据角色', 'SEALED');
  await expect(page.getByText('请选择兼容的访问边界', { exact: true })).toHaveCount(0);
  // Assert the visible retained value, not the obsolete pre-v6 selection CSS.
  await expect(page.getByText('METADATA_ONLY', { exact: true }).filter({ visible: true })).toBeVisible();
  const costs = await menu(page, '费用约束方式');
  await expect(costs).toHaveText(['没有可用费用度量', '估算值（不等于实际账单）']);
  expect(state.commands).toHaveLength(0);
});

test('schema-valid but unsupported loaded cost and access values cannot submit', async ({ page }) => {
  const state = await fixture(page);
  const draft = brief('DRAFT');
  draft.content.budget.cost_enforcement = 'EXACT';
  draft.bindings = [{ dataset_revision_id: id(14), role: 'SEALED', access_policy: 'RESEARCH_READ' }];
  await page.route(url => url.pathname === `/api/v2/projects/${project.id}/briefs`, route => reply(route, { schema_version: 1, items: [draft], next_cursor: null }));
  await page.goto('/'); await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  await page.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(page.getByText('请选择当前已支持的费用约束方式。')).toBeVisible();
  await expect(page.getByText('访问边界必须符合所选数据角色，请明确重选。')).toBeVisible();
  expect(state.commands).toHaveLength(0);
});
