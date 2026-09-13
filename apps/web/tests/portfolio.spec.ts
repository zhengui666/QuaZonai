// SYNTHETIC UI contracts, not native qualification or portfolio acceptance evidence.
import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

test('immutable Mandate preserves exact inputs and retries the original receipt without starting a build', async ({ page }) => {
  await fixture(page);
  const writes: { body: Schema['MandateCreateV1']; key: string | null }[] = [];
  let saved: Schema['MandateViewV1'] | undefined;
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path === `/api/v2/projects/${project.id}/portfolio-mandates`) return reply(route, { schema_version: 1, items: saved ? [saved] : [], next_cursor: null });
    if (path === '/api/v2/portfolio-mandates' && request.method() === 'POST') {
      const body = request.postDataJSON() as Schema['MandateCreateV1'];
      writes.push({ body, key: await request.headerValue('Idempotency-Key') });
      saved = { id: id(80), project_id: project.id, version: 1, created_at: '2026-09-13T00:00:00Z', content: body.content };
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: saved }, 201);
    }
    if (path === `/api/v2/portfolio-mandates/${id(80)}`) return reply(route, saved);
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: '新建组合配置', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: '新建不可变组合配置', exact: true });
  for (const [label, value] of [['Runtime 编号', id(20)], ['Runtime 配置版本', '9007199254740993'], ['投资域版本编号', id(21)], ['评估政策编号', id(22)], ['执行假设编号', id(23)], ['基础币种', 'USD'], ['资本假设', '12345678901234567890.123456789012345678'], ['费用依据产物编号', id(24)]] as const) {
    await drawer.getByLabel(label, { exact: true }).fill(value);
  }
  await drawer.getByLabel('调仓方式', { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').getByText('固定间隔', { exact: true }).click();
  await drawer.getByLabel('间隔秒数', { exact: true }).fill('123');
  await drawer.getByLabel('调仓方式', { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').getByText('手动', { exact: true }).click();
  await drawer.getByRole('button', { name: '保存不可变配置', exact: true }).click();
  await expect.poll(() => writes.length).toBe(1);
  await expect(drawer.getByRole('button', { name: '保存不可变配置', exact: true })).toBeEnabled();
  await drawer.getByRole('button', { name: '保存不可变配置', exact: true }).click();
  await expect(drawer).not.toBeVisible();
  expect(writes).toHaveLength(2); const first = writes[0]; if (!first) throw new Error('Missing original request');
  expect(writes[1]).toEqual(first); expect(first.key).toBeTruthy();
  expect(first.body).toMatchObject({ schema_version: 1, project_id: project.id, runtime_id: id(20), expected_runtime_revision: '9007199254740993', content: {
    capital_assumption: '12345678901234567890.123456789012345678',
    optimizer: { adapter_kind: 'CLARABEL_QP', parameters: { schema_version: 1, risk_aversion: '1' } },
    covariance_estimator: { adapter_kind: 'SAMPLE_COVARIANCE', parameters: { ddof: 1 } },
    alpha_ensemble: { adapter_kind: 'FIXED_WEIGHTED_FORECAST', parameters: {} },
    constraints: { schema_version: 1, group_bounds: [], asset_overrides: [], max_ex_ante_risk: null, max_participation: null, liquidity_ref: null },
    rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, session_offset_seconds: null },
  } });
  await page.getByRole('button', { name: '配置 v1', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '不可变组合配置', exact: true });
  await expect(detail.getByText('资本假设（非真实账户）', { exact: true })).toBeVisible();
  await expect(detail.locator('pre')).toContainText('12345678901234567890.123456789012345678');
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
});

test('Mandate authoring protects dirty input and cannot write offline or with missing references', async ({ page, context }) => {
  const state = await fixture(page);
  await page.route(`**/api/v2/projects/${project.id}/portfolio-mandates*`, route => reply(route, { schema_version: 1, items: [], next_cursor: null }));
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: '新建组合配置', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: '新建不可变组合配置', exact: true });
  await drawer.getByRole('button', { name: '保存不可变配置', exact: true }).click();
  await expect(drawer.getByText('请填写此项。', { exact: true }).first()).toBeVisible();
  await drawer.getByLabel('资本假设', { exact: true }).fill('123.456');
  await drawer.getByRole('button', { name: '取消', exact: true }).click();
  const confirm = page.getByRole('dialog', { name: '放弃未保存的组合配置？', exact: true });
  await confirm.getByRole('button', { name: '继续编辑', exact: true }).click();
  await expect(drawer.getByLabel('资本假设', { exact: true })).toHaveValue('123.456');
  await expect(page.getByRole('combobox', { name: '选择组合所属项目', exact: true })).toBeDisabled();
  await context.setOffline(true);
  await expect(drawer.getByRole('button', { name: '保存不可变配置', exact: true })).toBeDisabled();
  await context.setOffline(false);
  await expect(drawer.getByRole('button', { name: '保存不可变配置', exact: true })).toBeEnabled();
  expect(state.commands).toEqual([]);
  await drawer.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('dialog', { name: '放弃未保存的组合配置？', exact: true }).getByRole('button', { name: '放弃修改', exact: true }).click();
  await expect(drawer).not.toBeVisible();
  await expect(page.getByRole('combobox', { name: '选择组合所属项目', exact: true })).toBeEnabled();
});
