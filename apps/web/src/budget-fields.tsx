import { Checkbox, Form, Input, InputNumber, Select, Typography } from 'antd';
import { isCounter, isDecimal } from './api';
import { costOptions } from './authoring-options';
export const counterRules = [{ required: true }, { validator: (_: unknown, value: unknown) => typeof value === 'string' && isCounter(value, true) ? Promise.resolve() : Promise.reject(new Error('请输入 1 至 9223372036854775807 的整数字符串。')) }];
export function BudgetFields() {
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
      <Form.Item name={['content', 'budget', 'max_cost_decimal']} label="最大费用（可选，十进制）" rules={[{ validator: (_, value: unknown) => !value || (typeof value === 'string' && isDecimal(value)) ? Promise.resolve() : Promise.reject(new Error('请输入普通十进制字符串，不使用指数。')) }]}><Input inputMode="decimal" maxLength={64} /></Form.Item>
      <Form.Item name={['content', 'budget', 'cost_currency']} label="费用币种（配置费用时必填）" rules={[{ pattern: /^[A-Z]{3}$/ }]}><Input maxLength={3} /></Form.Item>
      <Form.Item name={['content', 'budget', 'cost_enforcement']} label="费用约束方式"
        extra="精确账单能力尚未接通，不能选择 EXACT。已有不支持值需明确修改。"
        rules={[{ required: true }, { validator: (_, value: unknown) => costOptions.some(option => option.value === value)
          ? Promise.resolve() : Promise.reject(new Error('请选择当前已支持的费用约束方式。')) }]}>
        <Select options={costOptions} />
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
