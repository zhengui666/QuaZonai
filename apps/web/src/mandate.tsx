import type { ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import { Facts, MetricLabel } from './ui';
import { Disclosure } from './disclosure';
export function Mandate({ specification: spec }: { specification: ResearchSpecification }) {
  return <section className="mandate-section"><h3>研究任务书</h3><Facts items={[["评价", <><MetricLabel metricKey={spec.evaluation.metric} /> / 最大化 / {spec.evaluation.target ?? '未设阈值'}</>], ["搜索范围", spec.search.scope === 'single-factor' ? '单一 Wasm 因子' : `原生共享账户组合 / 最多 ${spec.search.maxMembers} 个模型`]]} />
    <Disclosure title="研究问题、依据与原始目标" className="text-disclosure"><Facts items={[["经济问题", spec.question], ["假设与依据", spec.rationale], ...(spec.exclusions ? [["排除范围", spec.exclusions] as [string, string]] : []), ["停止条件", '原试验预算或最终选择']]} /></Disclosure>
    {spec.capital && <Disclosure title="原研究资金假设"><Facts items={[["资本情景", spec.capital.scenarios.join(' / ')], ["成交量参与率", spec.capital.participation], ["流动性数据最大时差", `${spec.capital.maximumAgeSeconds} 秒`]]} /></Disclosure>}
  </section>;
}
