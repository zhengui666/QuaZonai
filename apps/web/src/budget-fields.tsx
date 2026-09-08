import { Checkbox, Form, Input, InputNumber, Select, Typography } from 'antd';
import { isCounter } from './api';
import type { Schema } from './api';
import { costOptions } from './authoring-options';
import { costBudgetErrors } from './cost-budget';
import type { CostFields } from './cost-budget';

export const counterRules = [{ required: true }, { validator: (_: unknown, value: unknown) => typeof value === 'string' && isCounter(value, true) ? Promise.resolve() : Promise.reject(new Error('请输入 1 至 9223372036854775807 的整数字符串。')) }];
type BudgetForm = { content: { budget: Schema['BudgetV1'] } };
const costDependencies: ['content', 'budget', keyof CostFields][] = [
  ['content', 'budget', 'cost_enforcement'], ['content', 'budget', 'max_cost_decimal'], ['content', 'budget', 'cost_currency'],
];

export function BudgetFields() {
  const form = Form.useFormInstance<BudgetForm>();
  function costRules(field: keyof CostFields) {
    return [{ validator: async () => {
      const problem = costBudgetErrors(form.getFieldValue(['content', 'budget']) ?? {})[field];
      if (problem) throw new Error(problem);
    } }];
  }
  return <>
    <Typography.Title level={3}>预算上限</Typography.Title>
    <Typography.Paragraph type="secondary">初始值仅是可修改的草稿建议，不代表实测资源、模型报价或获准运行。</Typography.Paragraph>
    <div className="field-grid">
      {([
        ['max_experiments', '最大实验数', 1, 4294967295], ['max_parallel_runs', '最大并行运行', 1, 65535],
        ['max_turns_per_mission', '每个 Mission 最大轮次', 1, 65535], ['max_repair_turns', '最大修复轮次', 0, 65535],
        ['max_wall_seconds', '最长实际耗时（秒）', 1, 4294967295], ['max_memory_mib', '最大内存（MiB）', 1, 4294967295],
        ['max_cycles_per_day', '每日最大 Cycle 数', 1, 65535], ['min_cycle_interval_seconds', 'Cycle 最小间隔（秒）', 0, 4294967295],
      ] as const).map(([name, label, min, max]) => <Form.Item key={name} name={['content', 'budget', name]} label={label} rules={[{ required: true, type: 'integer', min, max }]}><InputNumber min={min} max={max} precision={0} className="full-width" /></Form.Item>)}
      {([['max_cpu_seconds', '最大 CPU 秒数'], ['max_output_bytes', '最大产物字节数']] as const).map(([name, label]) => <Form.Item key={name} name={['content', 'budget', name]} label={label} rules={counterRules}><Input inputMode="numeric" maxLength={19} /></Form.Item>)}
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
        extra="精确账单能力尚未接通。估算模式须填写正金额及受支持币种；切换为没有费用度量会清空这两项。"
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
      <Form.Item name={['content', 'stop_rule', 'stop_on_qualified_count']} label="合格 Alpha 目标数" rules={[{ required: true, type: 'integer', min: 1, max: 65535 }]}><InputNumber min={1} max={65535} precision={0} /></Form.Item>
      <Form.Item name={['content', 'stop_rule', 'stop_on_no_improvement_trials']} label="连续无改善实验数（可选）" rules={[{ type: 'integer', min: 1, max: 65535 }]}><InputNumber min={1} max={65535} precision={0} /></Form.Item>
    </div>
    <Form.Item name={['content', 'stop_rule', 'stop_on_budget']} valuePropName="checked"><Checkbox>预算耗尽时停止</Checkbox></Form.Item>
    <Form.Item name={['content', 'stop_rule', 'stop_on_invalid_data']} valuePropName="checked"><Checkbox>数据无效时停止</Checkbox></Form.Item>
  </>;
}
