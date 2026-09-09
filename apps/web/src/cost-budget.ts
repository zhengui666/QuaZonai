// Joint authoring constraints. The server remains the authority at submission.
import { validateCostAmount, validateCostCurrency } from './generated/responses.cjs';
import { costOptions } from './authoring-options';

export type CostFields = {
  cost_enforcement?: unknown;
  max_cost_decimal?: unknown;
  cost_currency?: unknown;
};
export type CostErrors = Partial<Record<keyof CostFields, string>>;
const absent = (value: unknown) => value === null || value === undefined || value === '';

export function costBudgetErrors(value: CostFields): CostErrors {
  const errors: CostErrors = {};
  if (!costOptions.some(option => option.value === value.cost_enforcement)) {
    errors.cost_enforcement = '请选择当前已支持的费用约束方式。';
    return errors;
  }
  if (value.cost_enforcement === 'UNAVAILABLE') {
    if (!absent(value.max_cost_decimal)) errors.max_cost_decimal = '没有费用度量时，金额必须留空。';
    if (!absent(value.cost_currency)) errors.cost_currency = '没有费用度量时，币种必须留空。';
    if (Object.keys(errors).length) errors.cost_enforcement = '请清空金额和币种，或明确选择估算费用模式。';
    return errors;
  }
  if (!validateCostAmount(value.max_cost_decimal)) {
    errors.max_cost_decimal = '估算费用必须是严格大于零的精确十进制金额。';
  }
  if (typeof value.cost_currency !== 'string' || !validateCostCurrency(value.cost_currency)) {
    errors.cost_currency = '请选择服务器原生币种表支持的币种代码。';
  }
  return errors;
}
