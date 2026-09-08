import type { Schema } from './api';
import { ApiFailure, isCounter } from './api';
export type BriefContent = Schema['BriefContentV1'];
export const initialBudget: Schema['BudgetV1'] = {
  schema_version: 1, max_experiments: 10, max_parallel_runs: 1, max_turns_per_mission: 10,
  max_repair_turns: 2, max_wall_seconds: 3600, max_cpu_seconds: '3600', max_memory_mib: 4096,
  max_output_bytes: '104857600', max_cycles_per_day: 1, min_cycle_interval_seconds: 3600,
  max_tokens: null, max_cost_decimal: null, cost_currency: null, cost_enforcement: 'UNAVAILABLE',
};
export const initialStop: Schema['StopRuleV1'] = {
  schema_version: 1, stop_on_qualified_count: 2, stop_on_budget: true,
  stop_on_no_improvement_trials: 20, stop_on_invalid_data: true,
};
export function briefContent(value: BriefContent): BriefContent {
  const budget = { ...value.budget, schema_version: 1 as const };
  budget.max_tokens ||= null;
  budget.max_cost_decimal ||= null;
  budget.cost_currency ||= null;
  const common = { ...value, benchmark_ref: value.benchmark_ref || null, budget,
    stop_rule: { ...value.stop_rule, schema_version: 1 as const,
      stop_on_no_improvement_trials: value.stop_rule.stop_on_no_improvement_trials ?? null },
  };
  if (value.horizon_kind === 'VARIABLE_INTERVAL') return { ...common, horizon_kind: 'VARIABLE_INTERVAL', horizon_value: null };
  if (typeof value.horizon_value !== 'string' || !isCounter(value.horizon_value, true)) {
    throw new ApiFailure('VALIDATION_ERROR', '固定预测周期必须是正整数字符串。');
  }
  return { ...common, horizon_kind: value.horizon_kind, horizon_value: value.horizon_value };
}
