import type { Schema } from './api';
import { ApiFailure, isCounter } from './api';
import { costBudgetErrors } from './cost-budget';
import { budgetRelationError } from './authoring-constraints';
import { validateBaseCurrency } from '@quazonai/web/response-contract/base-currency';
export type BriefContent = Schema['BriefContentV1'];
export const relaxedResearchBudget: Schema['BudgetV1'] = {
  schema_version: 1, max_experiments: 32, max_parallel_runs: null, max_turns_per_mission: null,
  max_repair_turns: null, max_wall_seconds: null, max_cpu_seconds: null, max_memory_mib: null,
  max_output_bytes: null, max_cycles_per_day: null, min_cycle_interval_seconds: 0,
  max_tokens: null, max_cost_decimal: null, cost_currency: null, cost_enforcement: 'UNAVAILABLE',
};
// Preserve an in-progress cost tuple; submission validates the complete budget.
// Infer the draft result rather than claiming it already satisfies BudgetV1.
export function applyRelaxedResearchBudget(current?: Partial<Schema['BudgetV1']>) {
  const next = { ...relaxedResearchBudget, ...current, max_wall_seconds: null, max_cpu_seconds: null,
    max_parallel_runs: null,
    max_tokens: null, max_output_bytes: null, max_turns_per_mission: null, max_repair_turns: null, max_cycles_per_day: null };
  // The explicit preset removes execution caps; the scientific trial policy is
  // preserved exactly, even when it is lower than the new-draft default.
  next.min_cycle_interval_seconds = 0;
  // Preserve an existing memory quota; only an explicit field edit removes it.
  return next;
}
export const initialBudget = { ...relaxedResearchBudget };
export const initialStop: Schema['StopRuleV1'] = {
  schema_version: 1, stop_on_qualified_count: 2, stop_on_budget: true,
  stop_on_no_improvement_trials: 20, stop_on_invalid_data: true,
};
export function briefContent(value: BriefContent): BriefContent {
  if (!validateBaseCurrency(value.base_currency)) throw new ApiFailure('VALIDATION_ERROR', '基础币种必须属于服务器原生币种表。');
  for (const relation of ['turns', 'experiments'] as const) {
    const problem = budgetRelationError(value, relation);
    if (problem) throw new ApiFailure('VALIDATION_ERROR', problem);
  }
  const costError = Object.values(costBudgetErrors(value.budget)).find(message => message !== undefined);
  if (costError) throw new ApiFailure('VALIDATION_ERROR', costError);
  const budget = { ...value.budget, schema_version: 1 as const };
  budget.max_tokens = budget.max_tokens === '' ? null : budget.max_tokens ?? null;
  budget.max_wall_seconds ??= null;
  budget.max_parallel_runs ??= null;
  budget.max_memory_mib ??= null;
  if (budget.max_memory_mib !== null && (!Number.isInteger(budget.max_memory_mib)
    || budget.max_memory_mib < 1 || budget.max_memory_mib > 4294967295)) {
    throw new ApiFailure('VALIDATION_ERROR', '内存限额必须留空或为 1 至 4294967295 MiB 的整数。');
  }
  budget.max_turns_per_mission ??= null;
  budget.max_repair_turns ??= null;
  budget.max_cycles_per_day ??= null;
  budget.max_cpu_seconds = budget.max_cpu_seconds === '' ? null : budget.max_cpu_seconds ?? null;
  budget.max_output_bytes = budget.max_output_bytes === '' ? null : budget.max_output_bytes ?? null;
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
