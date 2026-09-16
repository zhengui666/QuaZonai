// Field relationships mirror domain authoring rules, never grant authority.
export type BudgetRelations = {
  budget?: { max_turns_per_mission?: number; max_repair_turns?: number; max_experiments?: number };
  stop_rule?: { stop_on_qualified_count?: number };
};
export function budgetRelationError(value: BudgetRelations, relation: 'turns' | 'experiments'): string | undefined {
  const lower = relation === 'turns' ? value.budget?.max_repair_turns : value.stop_rule?.stop_on_qualified_count;
  const upper = relation === 'turns' ? value.budget?.max_turns_per_mission : value.budget?.max_experiments;
  // Required/type/range validators report missing or invalid individual values.
  if (typeof lower !== 'number' || typeof upper !== 'number' || lower <= upper) return undefined;
  return relation === 'turns' ? '最大修复轮次不能超过每个 Mission 最大轮次。' : '合格 Alpha 目标数不能超过最大实验数。';
}
export function bindingListError(values: unknown): string | undefined {
  if (!Array.isArray(values) || values.length < 1 || values.length > 64) return '需要 1 至 64 项真实数据绑定。';
  const seen = new Set<string>();
  for (const value of values) {
    if (value === null || typeof value !== 'object' || !('dataset_revision_id' in value)) continue;
    const id: unknown = value.dataset_revision_id;
    if (typeof id !== 'string' || id.length === 0) continue;
    const identity = id.toLowerCase();
    if (seen.has(identity)) return '同一个数据集版本只能绑定一次，不能通过更改角色或访问边界重复添加。';
    seen.add(identity);
  }
  return undefined;
}
