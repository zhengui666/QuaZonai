import type { MetricObservation } from '../../../packages/contracts/src/pm.ts';

/** Display only: preserve monetary strings; do not infer cash profit from a return. */
export function pmRatio(value: number | string | null): string {
  if (value === null) return '证据不足';
  if (typeof value === 'number') return Number.isFinite(value) ? `${(value * 100).toFixed(2)}%` : '证据不足';
  if (!/^-?\d+(?:\.\d{1,18})?$/.test(value)) return '证据不足';
  const negative = value.startsWith('-'); const [whole, fraction = ''] = (negative ? value.slice(1) : value).split('.');
  const units = BigInt(whole! + fraction.padEnd(18, '0'));
  const hundredths = (units + 50_000_000_000_000n) / 100_000_000_000_000n;
  return `${negative && hundredths > 0n ? '-' : ''}${hundredths / 100n}.${(hundredths % 100n).toString().padStart(2, '0')}%`;
}
/** A display approximation for ratios such as 1/3; monetary facts stay exact. */
export function pmCapitalShare(part: { amount: string; currency: string }, total: { amount: string; currency: string } | undefined): string {
  if (!total || part.currency !== total.currency || !/^\d+(?:\.\d{1,18})?$/.test(part.amount) || !/^\d+(?:\.\d{1,18})?$/.test(total.amount)) return '未确定';
  const units = (value: string) => { const [whole, fraction = ''] = value.split('.'); return BigInt(whole! + fraction.padEnd(18, '0')); };
  const denominator = units(total.amount); if (!denominator) return '未确定';
  const hundredths = (units(part.amount) * 10_000n + denominator / 2n) / denominator;
  return `≈${hundredths / 100n}.${(hundredths % 100n).toString().padStart(2, '0')}%`;
}
export function pmTime(value: string | null | undefined): string {
  if (!value || !/^\d+$/.test(value)) return '未建立时点';
  const milliseconds = Number(BigInt(value) / 1_000_000n);
  const date = new Date(milliseconds);
  return Number.isFinite(date.getTime()) ? date.toISOString().replace('T', ' ').replace('.000Z', ' UTC').replace(/Z$/, ' UTC') : '时点不可用';
}
export function pmMetricValue(metric: MetricObservation): string {
  if (metric.status !== 'AVAILABLE' || metric.value === null) return '证据不足';
  if (['RETURN_RATIO', 'LOSS_RATIO', 'RATIO'].includes(metric.definition.unit)) return pmRatio(metric.value);
  return `${metric.value}${metric.currency ? ` ${metric.currency}` : ''}`;
}
const reasons: Record<string, string> = {
  HISTORICAL_SELECTION_NOT_INDEPENDENT_EVALUATION: '本轮用于历史候选选择，尚需独立评估',
  FRESH_CASH_ACCOUNT_NOT_AN_EXISTING_ACCOUNT_HOLD_COMPARISON: '本轮从研究现金账户开始，尚未比较已有持仓的保持方案',
  CAPITAL_POINT_IS_INITIAL_NAV_ALLOCATION_WITH_PROFIT_REINVESTMENT_NOT_ABSOLUTE_NOTIONAL_CAP: '资金点代表初始净值下的投入比例；后续按该比例再投资，实际金额随净值变化',
  OWNER_NAV_RETAINED_AT_EVERY_DEPLOYMENT_CAP: '各方案使用相同的初始账户净值，并保留未投入现金',
  REGISTERED_MEAN_VARIANCE_METHOD_RISK_AVERSION_ONE_NOT_INFERRED_OWNER_PREFERENCE: '仓位使用事先登记的均值方差候选方法；方法参数不代表你的风险偏好',
  NO_PARTICIPATION_OR_MARKET_IMPACT_CAPACITY_BOUND: '尚无参与率、市场冲击或经济容量上界的证据',
  NO_FEASIBILITY_INTERPOLATION_BETWEEN_TESTED_POINTS: '已测试资金点之间的可行性尚未验证',
  RUNTIME_FUEL_AND_FAILED_EVENT_CONSUMPTION_NOT_FULLY_METERED: '失败计算的底层事件和资源消耗尚未完全计量',
  owner_preference_unresolved: '收益与下行风险仍有取舍，尚未形成唯一选择',
  selection_preference_unresolved: '候选间仍有需要明确的业务取舍',
  selection_frontier_only: '保留候选前沿，继续比较收益与风险',
  no_viable_candidate: '现有证据未支持可配置的候选',
  no_computed_candidates: '当前尚无完成计算的候选',
  trial_budget_exhausted: '新增实验预算已用完，正在形成研究结论',
  capacity_evidence_unavailable: '尚未证明本次资金规模可以配置',
  economic_capacity_unavailable: '缺少经济容量所需的成本、冲击或退出证据',
  account_equity_unknown: '尚未取得权威账户净值',
  account_reservation_unknown: '账户预留资金尚未完成对账',
  target_expired: '原目标已过期，等待新依据',
  basis_superseded: '计算期间依据发生变化，结果已由新一轮取代',
  dataset_not_ready: '数据尚未达到本轮研究的可用条件',
  no_authorized_execution: '当前授权范围只用于研究与观察',
  no_borrow_boundary_requires_native_cash_account: '当前账户无法证明禁止借款，需要原生现金账户及相应执行设置',
  owner_boundary_normalization_required: '原要求的含义尚未明确，请补充投资意图',
  owner_boundary_execution_unavailable: '已保留这条约束，当前计算适配尚不能执行',
  intent_revision_requires_new_unexposed_data: '意图已经修订，下一轮研究需要新的未暴露数据',
  owner_intent_revision_requires_new_evaluation: '投资意图已经修订，需要按新版本重新评估',
};
export function pmAssumption(value: string): string { return reasons[value] ?? value; }
export function pmReason(code: string): string {
  return reasons[code] ?? (/capacity|capital_scale/.test(code) ? '资金规模或容量证据需要补齐'
    : /account|reservation|holding/.test(code) ? '账户事实或资金需要对账'
      : /dataset|source|clock|label/.test(code) ? '数据时点、覆盖或成熟标签需要核实'
        : /selection|preference|frontier/.test(code) ? '候选取舍或选择依据尚需明确'
          : /unsupported|capability/.test(code) ? '当前数据与运行适配尚不支持这项能力'
            : /budget/.test(code) ? '可用研究资源需要核实'
              : '本轮记录了一项需要核查的证据条件');
}
