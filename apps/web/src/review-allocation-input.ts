import type { PortfolioView } from '../../../packages/contracts/src/index.ts';
import type { ReviewAllocationStart } from '../../../packages/contracts/src/review.ts';
import { decimalUnits } from './commands';

export interface AllocationFields {
  name: string; portfolioIds: string[];
  memberCaps: Record<string, string>; currentWeights: Record<string, string>;
  clusters: { id: string; members: string[]; maximumWeight: string }[];
  maximumTurnover: string; maximumScientificCalls: string;
  independentDatasetId?: string;
}
const ONE = 1_000_000_000_000_000_000n;
export function portfolioAllocationEligible(portfolio: PortfolioView): boolean {
  return portfolio.status === 'COMPUTED' && portfolio.review?.decision === 'PASS' && portfolio.review.target.kind === 'PORTFOLIO' && portfolio.review.target.id === portfolio.id;
}
function fraction(value: string | undefined, positive: boolean, maximum = ONE): bigint {
  const parsed = decimalUnits(value ?? '');
  if (parsed === undefined || (positive ? parsed <= 0n : parsed < 0n) || parsed > maximum) throw new Error('请填写范围内的精确小数，最多 18 位小数');
  return parsed;
}
/** Send only source identities and explicit constraints. Scientific evidence is resolved by the service. */
export function allocationInput(fields: AllocationFields, portfolios: PortfolioView[]): Omit<ReviewAllocationStart, 'requestId'> {
  if (!fields.name?.trim()) throw new Error('请填写配置名称');
  const ids = fields.portfolioIds;
  if (!Array.isArray(ids) || ids.length < 2 || ids.length > 64 || new Set(ids).size !== ids.length) throw new Error('请选择至少两个不同的已通过评审组合');
  const members = ids.map(id => portfolios.find(row => row.id === id));
  if (members.some(member => !member || !portfolioAllocationEligible(member))) throw new Error('所选组合缺少当前通过的评审，请刷新来源');
  const first = members[0]!;
  if (members.some(member => member!.currency !== first.currency || member!.datasetId !== first.datasetId || member!.capital !== first.capital)) throw new Error('组合须使用相同币种、数据集与原始本金');
  let caps = 0n; let current = 0n;
  for (const id of ids) { caps += fraction(fields.memberCaps?.[id], true); current += fraction(fields.currentWeights?.[id], false); }
  if (caps < ONE) throw new Error('组合权重上限的合计至少须为 1');
  if (current > ONE) throw new Error('当前组合权重合计不能超过 1，未分配部分视为现金');
  if (!Array.isArray(fields.clusters) || !fields.clusters.length || fields.clusters.length > 128) throw new Error('请明确填写至少一个相关簇约束');
  const clusterIds = new Set<string>();
  for (const cluster of fields.clusters) {
    if (!cluster.id?.trim() || clusterIds.has(cluster.id) || !Array.isArray(cluster.members) || cluster.members.length < 2 || new Set(cluster.members).size !== cluster.members.length || cluster.members.some(id => !ids.includes(id))) throw new Error('请填写不重复的相关簇名称，并选择至少两个所选组合');
    clusterIds.add(cluster.id); fraction(cluster.maximumWeight, true);
  }
  fraction(fields.maximumTurnover, false, 2n * ONE);
  if (!/^\d+$/.test(fields.maximumScientificCalls ?? '') || !Number.isSafeInteger(Number(fields.maximumScientificCalls)) || Number(fields.maximumScientificCalls) < 1 || Number(fields.maximumScientificCalls) > 4096) throw new Error('科学调用预算须为 1 至 4096 的整数');
  return {
    name: fields.name, portfolioIds: [...ids], memberCaps: ids.map(id => ({ id, maximumWeight: fields.memberCaps[id]! })),
    currentWeights: ids.map(id => ({ id, weight: fields.currentWeights[id]! })),
    clusterCaps: fields.clusters.map(cluster => ({ id: cluster.id, members: [...cluster.members], maximumWeight: cluster.maximumWeight })),
    maximumTurnover: fields.maximumTurnover, maximumScientificCalls: Number(fields.maximumScientificCalls),
    ...(fields.independentDatasetId?.trim() ? { independentDatasetId: fields.independentDatasetId } : {}),
  };
}
