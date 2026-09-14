// Controlled browser contracts, not native scientific or policy-admission evidence.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

async function choose(page: Page, label: string, option: string) {
  await page.getByLabel(label, { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: option }).click();
}
async function tag(page: Page, label: string, value: string) {
  const field = page.getByLabel(label, { exact: true });
  await field.fill(value); await field.press('Enter'); await field.press('Escape');
}
async function open(page: Page) {
  await page.goto('/'); await navigate(page, '组合');
  await choose(page, '选择组合所属项目', project.name);
  await page.getByRole('tab', { name: '评估政策', exact: true }).click();
}
function saved(body: Schema['EvaluationPolicyCreate']): Schema['EvaluationPolicyView'] {
  const { schema_version: _schema, selection, comparison_input_set_id, execution_assumptions_id, ...value } = body;
  return { ...value, portfolio_study_plan: body.portfolio_study_plan ?? null, portfolio_metric_requirements: body.portfolio_metric_requirements ?? null, id: id(71), version: 1, created_at: '2026-09-14T00:00:00Z', selection_rule: {
    ...selection, schema_version: 1, comparison_input_set_id, execution_assumptions_id,
    comparable_scope: 'FAMILY_LINEAGE', root_lineage_id: project.root_lineage_id, family_id: id(72),
    tie_break: 'EXPERIMENT_ID_ASC', missing_required_metric: 'INCONCLUSIVE',
  } };
}

for (const mode of ['none', 'thresholds', 'manual', 'scheduled'] as const) test(`immutable policy preserves independent requirements and retries exact intent: ${mode}`, async ({ page }) => {
  const portfolio = mode !== 'none'; const study = mode === 'manual' || mode === 'scheduled';
  await fixture(page);
  let original: Schema['EvaluationPolicyCreate'] | undefined; let key: string | undefined; let calls = 0;
  let stored: Schema['EvaluationPolicyView'] | undefined;
  await page.route('**/api/v2/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/evaluation-policies' && route.request().method() === 'GET') return reply(route, { schema_version: 1, items: stored ? [stored] : [], next_cursor: null });
    if (path === `/api/v2/evaluation-policies/${id(71)}`) return reply(route, stored);
    if (path === '/api/v2/evaluation-policies' && route.request().method() === 'POST') {
      const body = route.request().postDataJSON() as Schema['EvaluationPolicyCreate'];
      const currentKey = await route.request().headerValue('Idempotency-Key'); expect(currentKey).toBeTruthy();
      if (++calls === 1) {
        original = body; key = currentKey!; stored = saved(body); return route.abort('failed');
      }
      expect(body).toEqual(original); expect(currentKey).toBe(key);
      return reply(route, { schema_version: 1, resource: stored!, replayed: true } satisfies Schema['CommandResult_EvaluationPolicyView'], 201);
    }
    return route.fallback();
  });
  await open(page); await page.getByRole('button', { name: '新建评估政策', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '新建不可变评估政策', exact: true });
  for (const [label, value] of [
    ['研究问题', 'Controlled independent policy, not scientific PASS'], ['冻结比较输入编号', id(73)], ['原执行假设编号', id(74)],
    ['选择指标代码', 'original-ic'], ['选择指标 Scope', 'asset:0/fold:0'], ['选择方法', 'original-method'], ['选择方法版本', '0.7.3'],
    ['选择单位', 'UNITLESS_SCORE'], ['选择频率', 'FIXED_BARS'], ['训练样本数', '9007199254740993'], ['测试样本数', '10'],
    ['清除观察数', '0'], ['隔离观察数', '0'], ['标签固定跨度（CPCV 必填）', '1'], ['原 Sealed 数据版本编号', id(75)],
    ['最大缺失比例（0 至 1）', '0.123456789012345678'], ['证据有效秒数', '7200'],
  ] as const) await editor.getByLabel(label, { exact: true }).fill(value);
  await choose(page, '选择评估类型', 'WALK_FORWARD'); await choose(page, '选择方向', 'MAXIMIZE');
  await choose(page, '切分方法', portfolio ? 'CPCV_FIXED_HORIZON' : 'WALK_FORWARD');
  if (portfolio) {
    await editor.getByLabel('总组数', { exact: true }).fill('4');
    await editor.getByLabel('测试组数', { exact: true }).fill('1');
  } else await editor.getByLabel('窗口步长', { exact: true }).fill('10');
  for (const [label, value] of [['选择候选数', '2'], ['总体最少样本数', '2'], ['每血缘最多 Sealed 使用次数', '1']] as const) await editor.getByLabel(label, { exact: true }).fill(value);
  await choose(page, '是否要求真实数据', '要求 REAL');
  await tag(page, '必需能力名称（可留空）', 'original-capability');
  if (portfolio) await editor.getByRole('checkbox', { name: '定义独立组合要求', exact: true }).check();
  if (study) {
    await editor.getByRole('checkbox', { name: '冻结组合研究计划', exact: true }).check();
    await editor.getByLabel('组合研究输入编号', { exact: true }).fill(id(76));
    await editor.getByLabel('组合研究起点', { exact: true }).fill('2026-09-14T00:00:00.000001Z');
    await editor.getByRole('checkbox', { name: '冻结手动调仓时点', exact: true }).check();
    await editor.getByLabel('研究时点 1', { exact: true }).fill('2026-09-14T00:00:00.000001Z');
    await editor.getByLabel('研究时点 2', { exact: true }).fill('2026-09-14T01:00:00.000001Z');
    if (mode === 'scheduled') await editor.getByRole('checkbox', { name: '冻结手动调仓时点', exact: true }).uncheck();
  }
  for (const title of portfolio ? ['Validation', 'Sealed', '组合'] : ['Validation', 'Sealed']) {
    await editor.getByLabel(`${title} 1 指标代码`, { exact: true }).fill(title === 'Validation' ? 'original-ic' : `${title}-independent`);
    await editor.getByLabel(`${title} 1 Scope`, { exact: true }).fill(title === 'Validation' ? 'asset:0/fold:0' : title === 'Sealed' ? 'asset:0' : 'portfolio');
    await choose(page, `${title} 1 比较器`, title === 'Validation' ? 'GT' : title === 'Sealed' ? 'LT' : 'BETWEEN');
    if (title !== 'Sealed') await editor.getByLabel(`${title} 1 下端点`, { exact: true }).fill('0.123456789012345678');
    if (title !== 'Validation') await editor.getByLabel(`${title} 1 上端点`, { exact: true }).fill('0.987654321098765432');
    await editor.getByLabel(`${title} 1 最少样本`, { exact: true }).fill('9007199254740993');
    await tag(page, `${title} 1 方法白名单`, title === 'Validation' ? 'original-method' : `${title}-method`);
  }
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await editor.getByRole('button', { name: '保存不可变评估政策', exact: true }).click();
  await expect(editor.getByRole('alert').filter({ hasText: '连接中断，尚不能确定操作是否已提交。' })).toBeVisible();
  expect(calls).toBe(1); expect(original?.project_id).toBe(project.id);
  expect(original?.split_policy).toMatchObject({ schema_version: 1, train_size: '9007199254740993', interval_validation_required: true, step_size: portfolio ? null : '10', group_count: portfolio ? 4 : null, test_group_count: portfolio ? 1 : null });
  expect(original?.metric_requirements[0]).toMatchObject({ schema_version: 1, threshold_low: '0.123456789012345678', threshold_high: null, required: true });
  expect(original?.sealed_metric_requirements[0]).toMatchObject({ threshold_low: null, threshold_high: '0.987654321098765432', method_allowlist: ['Sealed-method'] });
  expect(original?.portfolio_metric_requirements).toEqual(portfolio ? [{ schema_version: 1, metric_code: '组合-independent', scope: 'portfolio', comparator: 'BETWEEN', threshold_low: '0.123456789012345678', threshold_high: '0.987654321098765432', required: true, minimum_observations: '9007199254740993', method_allowlist: ['组合-method'] }] : null);
  expect(original?.require_real_data).toBe(true); expect(original).not.toHaveProperty('use_portfolio');
  expect(original?.portfolio_study_plan).toEqual(study ? { schema_version: 1, input_set_id: id(76), evaluation_start: '2026-09-14T00:00:00.000001Z', manual_cutoffs: mode === 'manual' ? ['2026-09-14T00:00:00.000001Z', '2026-09-14T01:00:00.000001Z'] : null } : null);
  expect(original).not.toHaveProperty('use_study'); expect(original).not.toHaveProperty('use_manual_study');
  await editor.getByRole('button', { name: '保存不可变评估政策', exact: true }).click();
  await expect(editor).not.toBeVisible(); expect(calls).toBe(2);
  await page.getByRole('button', { name: '政策 v1', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '不可变评估政策', exact: true });
  await expect(detail.locator('pre')).toContainText('9007199254740993');
  await expect(detail.locator('pre')).toContainText('0.123456789012345678');
  await expect(detail.getByRole('button', { name: /保存|审批|启动|交付/ })).toHaveCount(0);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await detail.getByRole('button', { name: '关闭', exact: true }).click();
  stored = { ...stored!, project_id: id(90) };
  await page.getByRole('button', { name: '刷新政策', exact: true }).click();
  await expect(page.getByRole('button', { name: '政策 v1', exact: true })).toBeDisabled();
});

test('policy list can return after failed pagination', async ({ page }) => {
  await fixture(page);
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/portfolio-mandates')) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (url.pathname === '/api/v2/evaluation-policies') {
      if (url.searchParams.has('cursor')) return route.abort('failed');
      return reply(route, { schema_version: 1, items: [], next_cursor: id(79) });
    }
    return route.fallback();
  });
  await open(page);
  const panel = page.getByRole('tabpanel', { name: '评估政策', exact: true });
  await panel.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(panel.getByRole('button', { name: '上一页', exact: true })).toBeEnabled();
  await panel.getByRole('button', { name: '上一页', exact: true }).click();
  await expect(panel.getByText('尚无评估政策，不填充默认合格阈值。', { exact: true })).toBeVisible();
});
