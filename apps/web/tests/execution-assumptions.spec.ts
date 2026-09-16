// Controlled UI contracts only; native source/transaction evidence lives in Rust tests.
import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

for (const mode of ['none', 'snapshot', 'rolling']) test(`execution assumptions preserve models, exact retry and liquidity=${mode}`, async ({ page, context }) => {
  const liquidity = mode === 'snapshot';
  await fixture(page);
  const writes: { body: Schema['ExecutionAssumptionsCreateV1']; key: string | null }[] = [];
  let saved: Schema['ExecutionAssumptionsViewV1'] | undefined;
  await page.route('**/api/v2/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path === `/api/v2/projects/${project.id}/portfolio-mandates`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === `/api/v2/projects/${project.id}/execution-assumptions`) return reply(route, { schema_version: 1, items: saved ? [saved] : [], next_cursor: null });
    if (path === '/api/v2/execution-assumptions' && request.method() === 'POST') {
      const body = request.postDataJSON() as Schema['ExecutionAssumptionsCreateV1'];
      writes.push({ body, key: await request.headerValue('Idempotency-Key') });
      saved = { id: id(80), project_id: project.id, input_set_id: body.input_set_id, dataset_revision_id: body.dataset_revision_id, runtime_id: body.runtime_id,
        capability_snapshot_artifact_id: id(81), fee_schedule_artifact_id: id(82), engine_image_ref: 'fixture-native-image', venue_capability_ref: 'SIM',
        calendar_version: '1', settlement_rule_ref: body.settlement_rule_ref, cost_assumption_status: 'CONSERVATIVE_ASSUMPTION', settings: body.settings,
        bar_liquidity: body.bar_liquidity, bar_liquidity_valid_until: body.bar_liquidity ? '2026-09-14T00:00:00Z' : null,
        rolling_liquidity: body.rolling_liquidity, rolling_liquidity_artifact_id: body.rolling_liquidity ? id(84) : null, created_at: '2026-09-13T00:00:00Z' };
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: saved }, 201);
    }
    if (path === `/api/v2/execution-assumptions/${id(80)}`) return reply(route, saved);
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: '执行假设', exact: true }).click();
  await page.getByRole('button', { name: '新建执行假设', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: '新建不可变执行假设', exact: true });
  await drawer.getByRole('button', { name: '保存不可变执行假设', exact: true }).click();
  await expect(drawer.getByText('请填写此项。', { exact: true }).first()).toBeVisible();
  expect(writes).toHaveLength(0);
  for (const [label, value] of [
    ['Runtime 编号', id(20)], ['Runtime 配置版本', '9007199254740993'], ['冻结输入编号', id(21)], ['数据版本编号', id(22)],
    ['结算规则引用', 'spot-original-rule'], ['基础币种', 'USD'], ['资本假设', '12345678901234567890.123456789012345678'], ['杠杆上限', '1'], ['敞口容差', '0.000001'],
    ['快照间隔（毫秒）', '1000'], ['限价成交概率（0 至 1）', '1'], ['滑点概率（0 至 1）', '0.125'], ['随机种子', '9007199254740993'],
    ['基础延迟（纳秒）', '0'], ['插入附加延迟（纳秒）', '1'], ['更新附加延迟（纳秒）', '0'], ['取消附加延迟（纳秒）', '0'],
    ['资产 1 标识', 'EUR/USD.SIM'], ['资产 1 maker 费率', '-0.0001'], ['资产 1 taker 费率', '0.001'],
  ] as const) await drawer.getByLabel(label, { exact: true }).fill(value);
  await drawer.getByLabel('模拟账户模型', { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').getByText('现金', { exact: true }).click();
  const liquidityToggle = drawer.getByRole('checkbox', { name: '绑定历史单 BAR 流动性假设', exact: true });
  await liquidityToggle.check();
  await drawer.getByLabel('原生历史流动性报告编号', { exact: true }).fill(id(83));
  await drawer.getByLabel('历史量最长年龄（秒）', { exact: true }).fill('86400');
  await drawer.getByLabel('单 BAR 参与率上限（大于 0 且不超过 1）', { exact: true }).fill('0.123456789012345678');
  await liquidityToggle.uncheck();
  await expect(drawer.getByLabel('原生历史流动性报告编号', { exact: true })).toHaveCount(0);
  if (liquidity) {
    await liquidityToggle.check();
    for (const label of ['原生历史流动性报告编号', '历史量最长年龄（秒）', '单 BAR 参与率上限（大于 0 且不超过 1）']) await expect(drawer.getByLabel(label, { exact: true })).toHaveValue('');
    await drawer.getByRole('button', { name: '保存不可变执行假设', exact: true }).click();
    await expect(drawer.getByText('请填写此项。', { exact: true }).first()).toBeVisible();
    expect(writes).toHaveLength(0);
    await drawer.getByLabel('原生历史流动性报告编号', { exact: true }).fill(id(83));
    await drawer.getByLabel('历史量最长年龄（秒）', { exact: true }).fill('86400');
    await drawer.getByLabel('单 BAR 参与率上限（大于 0 且不超过 1）', { exact: true }).fill('0.123456789012345678');
  }
  if (mode === 'rolling') {
    await drawer.getByRole('checkbox', { name: '登记滚动 BAR 流动性政策', exact: true }).check();
    await expect(liquidityToggle).toBeDisabled();
    await drawer.getByLabel('每步历史 BAR 最长年龄（秒）', { exact: true }).fill('3600');
    await drawer.getByLabel('滚动单 BAR 参与率上限（大于 0 且不超过 1）', { exact: true }).fill('0.123456789012345678');
  }
  await drawer.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('dialog', { name: '放弃未保存的执行假设？', exact: true }).getByRole('button', { name: '继续编辑', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '选择组合所属项目', exact: true })).toBeDisabled();
  await context.setOffline(true);
  await expect(drawer.getByRole('button', { name: '保存不可变执行假设', exact: true })).toBeDisabled();
  await expect(liquidityToggle).toBeDisabled();
  await expect(drawer.getByRole('checkbox', { name: '登记滚动 BAR 流动性政策', exact: true })).toBeDisabled();
  await context.setOffline(false);
  await drawer.getByRole('button', { name: '保存不可变执行假设', exact: true }).click();
  await expect.poll(() => writes.length).toBe(1);
  await expect(drawer.getByRole('button', { name: '保存不可变执行假设', exact: true })).toBeEnabled();
  await drawer.getByRole('button', { name: '保存不可变执行假设', exact: true }).click();
  await expect(drawer).not.toBeVisible();
  await expect(page.getByRole('combobox', { name: '选择组合所属项目', exact: true })).toBeEnabled();
  expect(writes).toHaveLength(2); expect(writes[1]).toEqual(writes[0]); expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body.bar_liquidity).toEqual(liquidity ? { schema_version: 1, report_artifact_id: id(83), maximum_age_seconds: 86400, participation_limit: '0.123456789012345678' } : null);
  expect(writes[0]?.body).not.toHaveProperty('use_bar_liquidity');
  expect(writes[0]?.body).not.toHaveProperty('use_rolling_liquidity');
  expect(writes[0]?.body.rolling_liquidity).toEqual(mode === 'rolling' ? { schema_version: 1, maximum_age_seconds: 3600, participation_limit: '0.123456789012345678' } : null);
  expect(writes[0]?.body).toMatchObject({ schema_version: 1, project_id: project.id, expected_runtime_revision: '9007199254740993', settings: {
    starting_capital: '12345678901234567890.123456789012345678', account_kind: 'CASH', snapshot_interval_ms: 1000,
    fill_model: { adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_version: '0.63.0', parameters: { random_seed: '9007199254740993', prob_slippage: '0.125' } },
    latency_model: { adapter_kind: 'NAUTILUS_STATIC_LATENCY', parameters: { base_latency_ns: '0', insert_latency_ns: '1', update_latency_ns: '0', cancel_latency_ns: '0' } },
    fee_model: { adapter_kind: 'NAUTILUS_MAKER_TAKER', parameters: {} }, fee_rates: [{ instrument_id: 'EUR/USD.SIM', maker: '-0.0001', taker: '0.001' }],
  } });
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.getByRole('button', { name: `查看假设 ${id(80)}`, exact: true }).click();
  const detail = page.getByRole('dialog', { name: '不可变执行假设', exact: true });
  await expect(detail.getByText(id(81), { exact: true })).toBeVisible();
  await expect(detail.locator('pre')).toContainText('9007199254740993');
  if (liquidity) {
    await expect(detail.getByText(id(83), { exact: true })).toBeVisible();
    await expect(detail.getByText('0.123456789012345678', { exact: true })).toBeVisible();
    await expect(detail.getByText('原假设失效时刻（不含）', { exact: true })).toBeVisible();
  } else await expect(detail.getByText('原生历史流动性报告', { exact: true })).toHaveCount(0);
  if (mode === 'rolling') await expect(detail.getByText(id(84), { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
});
