import type { ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import { Facts } from './ui';
export function Mandate({ specification: spec }: { specification: ResearchSpecification }) {
  return <section className="mandate-section"><h3>研究任务书</h3><Facts items={[["经济问题", spec.question], ["假设与依据", spec.rationale], ...(spec.exclusions ? [["排除范围", spec.exclusions] as [string, string]] : []), ["评价", `${spec.evaluation.metric === 'pearson_ic' ? 'Pearson IC' : spec.evaluation.metric} / ${spec.evaluation.direction === 'maximize' ? '最大化' : '最小化'} / ${spec.evaluation.target ?? '未设阈值'}`], ["搜索范围", spec.search.scope === 'single-factor' ? '单一 Wasm 因子' : `原生共享账户组合 / 最多 ${spec.search.maxMembers} 个模型`], ["停止条件", '原试验预算或最终选择'], ["资金情景", spec.capital ? `${spec.capital.scenarios.join(' / ')}；成交量参与率 ${spec.capital.participation}；数据最大时差 ${spec.capital.maximumAgeSeconds} 秒` : '未请求（不阻止原始因子研究）']]} /></section>;
}
