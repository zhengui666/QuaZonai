import { Button, Checkbox, Form, Input, InputNumber, Select, Typography } from 'antd';
import { isCounter } from './api';
import type { Schema } from './api';
import { costOptions } from './authoring-options';
import { costBudgetErrors } from './cost-budget';
import type { CostFields } from './cost-budget';
import { budgetRelationError } from './authoring-constraints';
import { applyRelaxedResearchBudget } from './brief-fields';

export const counterRules = [{ required: true }, { validator: (_: unknown, value: unknown) => typeof value === 'string' && isCounter(value, true) ? Promise.resolve() : Promise.reject(new Error('请输入 1 至 9223372036854775807 的整数字符串。')) }];
type BudgetForm = { content: { budget: Schema['BudgetV1']; stop_rule: Schema['StopRuleV1'] } };
const relationDependencies = (name: string): (string | number)[][] => name === 'max_repair_turns'
  ? [['content', 'budget', 'max_turns_per_mission']]
  : name === 'max_turns_per_mission' ? [['content', 'budget', 'max_repair_turns']]
  : name === 'max_experiments' ? [['content', 'stop_rule', 'stop_on_qualified_count']]
  : name === 'stop_on_qualified_count' ? [['content', 'budget', 'max_experiments']] : [];
const costDependencies: ['content', 'budget', keyof CostFields][] = [
  ['content', 'budget', 'cost_enforcement'], ['content', 'budget', 'max_cost_decimal'], ['content', 'budget', 'cost_currency'],
];

export function BudgetFields() {
  const form = Form.useFormInstance<BudgetForm>();
  function relationRules(name: string) {
    const relation = name === 'max_repair_turns' || name === 'max_turns_per_mission' ? 'turns'
      : name === 'max_experiments' || name === 'stop_on_qualified_count' ? 'experiments' : undefined;
    return relation === undefined ? [] : [{ validator: async () => {
      const problem = budgetRelationError(form.getFieldValue(['content']) ?? {}, relation);
      if (problem) throw new Error(problem);
    } }];
  }
  function costRules(field: keyof CostFields) {
    return [{ validator: async () => {
      const problem = costBudgetErrors(form.getFieldValue(['content', 'budget']) ?? {})[field];
      if (problem) throw new Error(problem);
    } }];
  }
  return <>
    <Typography.Title level={3}>执行预算</Typography.Title>
    <Button onClick={() => form.setFieldValue(['content', 'budget'], applyRelaxedResearchBudget(form.getFieldValue(['content', 'budget'])))}>使用宽松研究预算</Button>
    <Typography.Paragraph type="secondary">宽松研究取消任务总超时、累计 CPU、Token、产物大小、Mission 轮次、修复轮次、每日 Cycle 和并行上限；保留实验数、已设置的内存限额和费用设置，内存留空表示不设任务内存限额，实际资源须通过 Runtime 能力检查。只修改当前草稿，不更改已冻结研究。</Typography.Paragraph>
    
    <div className="field-grid">
      {([
        ['max_experiments', '最大实验数', 1, 4294967295],
        ['min_cycle_interval_seconds', 'Cycle 最小间隔（秒）', 0, 4294967295],
      ] as const).map(([name, label, min, max]) => <Form.Item key={name} name={['content', 'budget', name]} label={label} dependencies={relationDependencies(name)} rules={[{ required: true, type: 'integer', min, max }, ...relationRules(name)]}><InputNumber min={min} max={max} precision={0} className="full-width" /></Form.Item>)}
      {([
        ['max_memory_mib', '最大内存（MiB，留空不设任务内存限额）', 1],
        ['max_parallel_runs', '最大并行运行（留空不设人工上限）', 1],
        ['max_turns_per_mission', '每个 Mission 最大轮次（留空不设上限）', 1],
        ['max_repair_turns', '最大修复轮次（留空不设上限，0 禁用修复）', 0],
        ['max_cycles_per_day', '每日最大 Cycle 数（留空不设上限）', 1],
      ] as const).map(([name, label, min]) => <Form.Item key={name} name={['content', 'budget', name]} label={label} dependencies={relationDependencies(name)} rules={[{ type: 'integer', min, max: 4294967295 }, ...relationRules(name)]}><InputNumber min={min} max={4294967295} precision={0} className="full-width" placeholder="不设上限" /></Form.Item>)}
      {([['max_cpu_seconds', '累计 CPU 秒数（留空不设上限）'], ['max_output_bytes', '产物字节数（留空不设上限）']] as const).map(([name, label]) => <Form.Item key={name} name={['content', 'budget', name]} label={label} rules={[{ validator: (_, value: unknown) => !value || (typeof value === 'string' && isCounter(value, true)) ? Promise.resolve() : Promise.reject(new Error('需要正整数字符串。')) }]}><Input inputMode="numeric" maxLength={19} allowClear /></Form.Item>)}
      <Form.Item name={['content', 'budget', 'max_wall_seconds']} label="任务总超时（秒，留空不设上限）" rules={[{ type: 'integer', min: 1, max: 4294967295 }]}><InputNumber min={1} max={4294967295} precision={0} className="full-width" placeholder="不设任务总超时" /></Form.Item>
      <Form.Item name={['content', 'budget', 'max_tokens']} label="最大 Token 数（可选）" rules={[{ validator: (_, value: unknown) => !value || (typeof value === 'string' && isCounter(value, true)) ? Promise.resolve() : Promise.reject(new Error('需要正整数字符串。')) }]}><Input inputMode="numeric" maxLength={19} /></Form.Item>
      <Form.Item name={['content', 'budget', 'max_cost_decimal']} label="费用上限（估算模式必填）"
        dependencies={costDependencies.filter(path => path[2] !== 'max_cost_decimal')} rules={costRules('max_cost_decimal')}>
        <Input inputMode="decimal" maxLength={64} allowClear />
      </Form.Item>
      <Form.Item name={['content', 'budget', 'cost_currency']} label="费用币种（估算模式必填）"
        dependencies={costDependencies.filter(path => path[2] !== 'cost_currency')} rules={costRules('cost_currency')}>
        <Input maxLength={3} allowClear />
      </Form.Item>
      <Form.Item name={['content', 'budget', 'cost_enforcement']} label="费用约束方式"
        dependencies={costDependencies.filter(path => path[2] !== 'cost_enforcement')}
        rules={[{ required: true }, ...costRules('cost_enforcement')]}>
        <Select options={costOptions} onChange={value => {
          // Only an explicit user action clears an incompatible tuple. Loading a
          // saved or frozen record must never rewrite its historical values.
          if (value === 'UNAVAILABLE') {
            form.setFieldValue(['content', 'budget', 'max_cost_decimal'], null);
            form.setFieldValue(['content', 'budget', 'cost_currency'], null);
          }
        }} />
      </Form.Item>
    </div>
    <Typography.Title level={3}>停止条件</Typography.Title>
    <div className="field-grid">
      <Form.Item name={['content', 'stop_rule', 'stop_on_qualified_count']} label="合格 Alpha 目标数" dependencies={relationDependencies('stop_on_qualified_count')} rules={[{ required: true, type: 'integer', min: 1, max: 65535 }, ...relationRules('stop_on_qualified_count')]}><InputNumber min={1} max={65535} precision={0} /></Form.Item>
      <Form.Item name={['content', 'stop_rule', 'stop_on_no_improvement_trials']} label="连续无改善实验数（可选）" rules={[{ type: 'integer', min: 1, max: 65535 }]}><InputNumber min={1} max={65535} precision={0} /></Form.Item>
    </div>
    <Form.Item name={['content', 'stop_rule', 'stop_on_budget']} valuePropName="checked"><Checkbox>预算耗尽时停止</Checkbox></Form.Item>
    <Form.Item name={['content', 'stop_rule', 'stop_on_invalid_data']} valuePropName="checked"><Checkbox>数据无效时停止</Checkbox></Form.Item>
  </>;
}
