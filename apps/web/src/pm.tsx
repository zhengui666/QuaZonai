import { Button, Table, Tabs } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { AssetTarget, CapacityEstimate, MetricObservation, ModeledOperatingComparison, PmMandateView, PortfolioDecision } from '../../../packages/contracts/src/pm.ts';
import { useCommand } from './commands';
import { useResource } from './data-hooks';
import { href, type Route } from './router';
import { Disclosure, RecordTitle } from './disclosure';
import { shortTitle } from './titles';
import { Empty, ErrorNotice, Exact, Facts, Loading, PendingCommand, Status, useConfirm } from './ui';
import { pmAssumption, pmCapitalShare, pmMetricValue, pmRatio, pmReason, pmTime } from './pm-values';
import { DeleteRecord } from './delete-record';
import './pm.css';

const actionNames: Record<string, string> = { HOLD: '保持现有配置', REBALANCE: '调整持仓', REDUCE: '减少风险', CASH: '保留现金', NEEDS_EVIDENCE: '补齐证据后再决定', NO_FEASIBLE_SOLUTION: '当前无可行配置' };
function decisionLabel(decision: PortfolioDecision, context?: string): string {
  if (context === 'FORWARD_MODEL_ESTIMATE') return actionNames[decision.action] ?? decision.action;
  if (!decision.validUntilNs) {
    if (decision.action === 'REBALANCE') return '历史比较选出的配置';
    if (decision.action === 'HOLD') return '历史比较倾向维持原持仓';
    if (decision.action === 'CASH') return '历史比较倾向保留现金';
  }
  return actionNames[decision.action] ?? decision.action;
}
const authorityNames: Record<string, string> = { RESEARCH_ONLY: '研究', SHADOW_TARGETS: '影子观察', AUTHORIZED_TARGETS: '已授权目标更新' };
export default function Pm({ route }: { route: Route }) {
  return route.creating ? <Empty title="旧投资研究目标创建入口已移除" action={<a href={href('/research/new')}>开始原生研究</a>} /> : route.id ? <MandateDetail id={route.id} /> : <Mandates />;
}
function Mandates() {
  const query = useResource<PmMandateView[]>('/mandates', { poll: 5000 });
  return <div className="index-page pm-page"><header className="page-heading"><h1>投资决策</h1></header>
    <p className="pm-introduction">查看已有账户配置、原始目标回执与后续实际结果。新的研究从原生研究入口开始。</p>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="尚无投资目标"><p>通过原生研究探索策略；已有账户的目标和退出资金仍在目标与观测中管理。</p></Empty> : <Table<PmMandateView> rowKey={row => row.mandate.id} dataSource={query.data} pagination={{ pageSize: 15, hideOnSinglePage: true }} scroll={{ x: 750 }} columns={[
      { title: '投资目标', render: (_value, row) => <a className="record-link" href={href(`/pm/${encodeURIComponent(row.mandate.id)}`)}>{shortTitle(row.mandate.ownerGoal)}</a> },
      { title: '当前判断', render: (_value, row) => row.currentDecision ? decisionLabel(row.currentDecision, row.comparisons.find(item => item.id === row.currentDecision?.comparisonRef)?.evaluationContext) : row.lastCycle ? <Status plain value={row.lastCycle.state} /> : '等待研究证据' },
      { title: '可投入资金', render: (_value, row) => <MoneyValue money={row.currentDecision?.capital?.committedCapital} /> },
      { title: '运行范围', render: (_value, row) => authorityNames[row.mandate.operatingAuthority] },
      { title: '最近依据', render: (_value, row) => pmTime(row.lastCycle?.asOfNs) },
    ]} />}
  </div>;
}
function MandateDetail({ id }: { id: string }) {
  const path = `/mandates/${encodeURIComponent(id)}`; const query = useResource<PmMandateView>(path, { poll: 3000 }); const view = query.data;
  const currentContext = view?.comparisons.find(row => row.id === view.currentDecision?.comparisonRef)?.evaluationContext;
  return <div className="pm-page"><a className="text-link back-link" href={href('/pm')}><ArrowLeftIcon size={14} />投资决策</a>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : view && <>
      <header className="pm-heading"><div><RecordTitle name={view.mandate.ownerGoal} fallback="投资目标" /><span className="quiet">{authorityNames[view.mandate.operatingAuthority]} · {view.mandate.baseCurrency}</span></div><PmActions view={view} path={path} /></header>
      {view.mandate.capitalSource.kind === 'ACCOUNT' && <a className="text-link" href={href(`/observations/${encodeURIComponent(view.mandate.capitalSource.accountBindingId)}?section=capital`)}>管理账户退出资金</a>}
      <section className="pm-current"><h2>{currentContext === 'FORWARD_MODEL_ESTIMATE' ? '当前模型配置' : view.currentDecision?.validUntilNs ? '当前投资判断' : '当前研究结论'}</h2><h3>{view.currentDecision ? decisionLabel(view.currentDecision, currentContext) : '等待可比较的研究证据'}</h3>
        {view.lastCycle && <p className="quiet">最近依据 {pmTime(view.lastCycle.asOfNs)} · <Status plain value={view.lastCycle.state} /></p>}
        {view.currentDecision && <><DecisionSummary decision={view.currentDecision} /><ReasonList codes={view.currentDecision.reasonCodes} /></>}
        {view.mandate.unresolvedIntent.length > 0 && <ul className="pm-reasons">{view.mandate.unresolvedIntent.map((text, index) => <li key={index}>{text}</li>)}</ul>}
      </section>
      <Tabs items={[
        { key: 'allocation', label: '组合与资金', children: <Allocation view={view} /> },
        { key: 'research', label: '研究与策略', children: <ResearchProgress view={view} /> },
        { key: 'capacity', label: '容量与可行资金', children: view.capacityEstimates.length ? view.capacityEstimates.map(row => <Capacity key={row.id} estimate={row} />) : <Empty title="容量证据尚未形成"><p>系统需要相应规模、成本和退出证据；未知容量不会被填成无限。</p></Empty> },
        { key: 'outcome', label: '后续结果', children: <><h2>目标更新回执</h2><Table rowKey="id" dataSource={view.targetCommands} pagination={false} scroll={{ x: 600 }} columns={[{ title: '状态', render: (_value, row) => <Status plain value={row.state} /> }, { title: '原请求', render: (_value, row) => <code>{row.idempotencyKey}</code> }, { title: '有效至', render: (_value, row) => pmTime(row.validUntilNs) }]} /><h2>账户观测</h2><Table rowKey="id" dataSource={view.outcomes} pagination={false} scroll={{ x: 600 }} columns={[{ title: '观测时点', render: (_value, row) => pmTime(row.availableAtNs) }, { title: '目标回执', render: (_value, row) => <Status plain value={row.receiptState} /> }, { title: '账户对账', dataIndex: 'reconciliationStatus' }, { title: '已观测指标', render: (_value, row) => <PmMetrics metrics={row.realizedMetrics} /> }]} /><p className="quiet">目标被接收、实际成交和账户收益分别记录。</p></> },
        { key: 'history', label: '决策变化', children: <Table<PortfolioDecision> rowKey="id" dataSource={view.history} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 650 }} columns={[{ title: '形成时间', render: (_value, row) => pmTime(row.createdAtNs) }, { title: '判断', render: (_value, row) => decisionLabel(row, view.comparisons.find(item => item.id === row.comparisonRef)?.evaluationContext) }, { title: '投入', render: (_value, row) => <MoneyValue money={row.capital?.committedCapital} /> }, { title: '状态', render: (_value, row) => <Status plain value={row.status} /> }, { title: '原因', render: (_value, row) => <ReasonList codes={row.reasonCodes} /> }]} /> },
      ]} />
      {view.gaps.filter(row => row.state !== 'RESOLVED').length > 0 && <section className="pm-section"><h2>需要补齐的证据</h2>{view.gaps.filter(row => row.state !== 'RESOLVED').map(row => <div key={row.id} className="pm-gap"><p>{pmReason(row.reasonCode)}</p><Status plain value={row.state} /><Disclosure title="证据条件与下一步"><Facts items={[["所需能力", row.requiredCapability], ["原因", row.reasonCode], ["可执行的补证据方式", row.permittedRemedy]]} /></Disclosure></div>)}</section>}
    </>}
  </div>;
}
function PmActions({ view, path }: { view: PmMandateView; path: string }) {
  const cache = useQueryClient(); const confirm = useConfirm();
  const accepted = () => { void cache.invalidateQueries({ queryKey: [path] }); void cache.invalidateQueries({ queryKey: ['/mandates'] }); };
  const recalculate = useCommand<unknown>(`pm:${view.mandate.id}:recompute`, accepted, `${path}/recompute`);
  const retire = useCommand<unknown>(`pm:${view.mandate.id}:retire`, accepted, `${path}/retire`);
  const targets = useCommand<unknown>(`pm:${view.mandate.id}:targets`, accepted, `${path}/targets`);
  const current = view.currentDecision;
  const strategy = view.strategies.find(row => row.id === current?.strategyVersionId);
  const ready = !!current?.validUntilNs && /^\d+$/.test(current.validUntilNs) && BigInt(current.validUntilNs) > BigInt(Date.now()) * 1_000_000n && !!current.accountBasis && current.status === 'CURRENT'
    && view.assessments.some(row => row.id === strategy?.pipelineAssessmentId && row.strategyVersionId === current.strategyVersionId && (row.stage === 'TARGET_OPERATION' || (row.stage === 'PAPER' && row.requirementsVersion === 'qz.pm.paper-shadow-source-target-qualification/1')) && row.outcome === 'READY');
  const paperReady = ready && view.assessments.some(row => row.id === strategy?.pipelineAssessmentId && row.stage === 'PAPER');
  return <div className="pm-actions"><Button loading={recalculate.busy} disabled={recalculate.busy} onClick={() => { void recalculate.submit({ expectedRevision: view.mandate.revision }); }}>{recalculate.pending ? '核对原重算请求' : '重算当前证据'}</Button>
    {view.mandate.operatingAuthority === 'AUTHORIZED_TARGETS' && <Button type="primary" loading={targets.busy} disabled={targets.busy || (!ready && !targets.pending)} onClick={() => { void targets.submit({ expectedRevision: view.mandate.revision, decisionId: current?.id }); }}>{targets.pending ? '核对原目标请求' : ready ? paperReady ? '更新纸面账户目标' : '更新账户目标' : '等待运营证据'}</Button>}
    {view.currentDecision && <Button danger disabled={retire.busy} loading={retire.busy} onClick={() => confirm('退役当前策略？', '停止新增策略消费；已有目标与持仓继续按原事实对账。', () => { void retire.submit({ expectedRevision: view.mandate.revision, strategyVersionId: view.currentDecision!.strategyVersionId }); })}>{retire.pending ? '核对原退役请求' : '退役策略'}</Button>}
    <DeleteRecord key={view.mandate.id} kind="mandate" id={view.mandate.id} />
    <ErrorNotice error={recalculate.error ?? retire.error ?? targets.error} /><PendingCommand command={recalculate} /><PendingCommand command={retire} /><PendingCommand command={targets} />
  </div>;
}
function DecisionSummary({ decision }: { decision: PortfolioDecision }) {
  const capital = decision.capital;
  const valuation = decision.accountBasis?.valuation;
  return <>{valuation && <Facts items={[["原余额派生估值", <MoneyValue money={valuation.reportValue} />], ["估值时点", pmTime(valuation.asOfNs)]]} />}<Facts items={[["可分配预算", <MoneyValue money={capital?.deployableBudget} />], ["策略投入", <MoneyValue money={capital?.committedCapital} />], ["未投入资金", <MoneyValue money={capital?.uncommittedCash} />], ["账户预留", <MoneyValue money={capital?.reserved} />], ["判断有效至", decision.validUntilNs ? pmTime(decision.validUntilNs) : '未建立可交付时效']]} /></>;
}
function Allocation({ view }: { view: PmMandateView }) {
  const decision = view.currentDecision;
  if (!decision) return <Empty title="尚无可消费的配置"><p>研究结论、账户条件与资金可行性满足后，会生成带依据的新决策。</p></Empty>;
  const comparison = view.comparisons.find(row => row.id === decision.comparisonRef);
  const ensemble = view.ensembles.find(row => row.id === decision.ensembleRevisionId);
  const forward = comparison?.evaluationContext === 'FORWARD_MODEL_ESTIMATE';
  return <>{ensemble && <section className="pm-section"><h2>因子组合比例</h2><Table rowKey="factorVersionId" pagination={false} dataSource={ensemble.members} scroll={{ x: 550 }} columns={[{ title: '因子版本', render: (_value, row) => <code>{row.factorVersionId}</code> }, { title: '预测组合系数', render: (_value, row) => pmRatio(row.weight) }, { title: '经济同族', render: (_value, row) => <code>{row.economicFamilyId}</code> }]} /><p className="quiet">这组系数用于组合预测；资产仓位和实际投入资金分别计算。</p></section>}
    <section className="pm-section"><h2>资产仓位</h2>{forward ? <p className="quiet">根据本次可得行情、冻结模型与原账户持仓计算。连续模型目标与按交易单位取整后的预计调整分开展示；预计调整尚未成交。目标交付须满足当前运营证据。</p> : !decision.validUntilNs && <p className="quiet">研究参考金额按历史末期权重与初始净值换算；当前持仓及实际净调整金额尚未建立。</p>}<Table rowKey="instrumentId" pagination={false} dataSource={decision.assetTargets} scroll={{ x: 750 }} columns={[
    { title: '资产', dataIndex: 'instrumentId' }, { title: '目标占账户净值', render: (_value, row) => pmRatio(row.navWeight) }, { title: forward || decision.validUntilNs ? '目标金额' : '研究参考金额', render: (_value, row) => <MoneyValue money={row.targetNotional} /> }, { title: '当前金额', render: (_value, row) => <MoneyValue money={row.currentNotional} /> }, { title: forward ? '模型目标净差额' : '净调整金额', render: (_value, row) => <MoneyValue money={row.netTradeNotional} /> },
  ]} />{forward && <ExecutionEstimates targets={decision.assetTargets} />}</section>
    <section className="pm-section"><h2>策略预算</h2><Table rowKey="strategyVersionId" pagination={false} dataSource={decision.strategyAllocations} columns={[{ title: '策略版本', render: (_value, row) => <code>{row.strategyVersionId}</code> }, { title: '承诺资金', render: (_value, row) => <MoneyValue money={row.committedCapital} /> }, { title: '占策略总投入', render: (_value, row) => row.capitalFraction === null ? pmCapitalShare(row.committedCapital, decision.capital?.committedCapital) : pmRatio(row.capitalFraction) }]} /></section>
    {comparison && <section className="pm-section"><h2>保持与调整的比较</h2>{comparison.modeledComparison && <ModeledComparison comparison={comparison.modeledComparison} />}<p className="quiet">比较区间：{pmTime(comparison.scope.interval.startNs)} 至 {pmTime(comparison.scope.interval.endExclusiveNs)}（不含结束时点）。{comparison.evaluationContext === 'HISTORICAL_JOINT_REPLAY' ? '这是历史共享账户回放。' : forward ? '这是当前模型估计，尚未观测到该区间的收益。' : '这是后续实际观测。'}</p><Table rowKey="id" dataSource={comparison.candidates} pagination={false} scroll={{ x: 850 }} columns={[{ title: '候选动作', render: (_value, row) => row.kind === 'HOLD_CURRENT' ? '维持原持仓' : row.kind === 'REBALANCE' && !forward && !decision.validUntilNs ? '历史候选配置' : actionNames[row.kind] ?? row.kind }, { title: '过渡成本', render: (_value, row) => <PmMetrics metrics={row.transitionCost} /> }, { title: '净效果', render: (_value, row) => <PmMetrics metrics={row.netOutcome.filter(metric => metric.definition.direction !== 'DIAGNOSTIC')} /> }, { title: '下行表现', render: (_value, row) => <PmMetrics metrics={row.downside.filter(metric => metric.definition.direction !== 'DIAGNOSTIC')} /> }, { title: forward ? '计算诊断' : '回放成本与成交', render: (_value, row) => <PmMetrics metrics={[...row.netOutcome, ...row.downside].filter(metric => metric.definition.direction === 'DIAGNOSTIC')} /> }, { title: '结论', render: (_value, row) => row.id === comparison.selectedCandidateId ? '本轮选择' : <Status plain value={row.status} /> }]} /><ReasonList codes={comparison.reasonCodes} /></section>}
  </>;
}
function ExecutionEstimates({ targets }: { targets: AssetTarget[] }) {
  const rows = targets.flatMap(row => row.executionEstimate ? [{ instrumentId: row.instrumentId, ...row.executionEstimate }] : []);
  if (!rows.length) return null;
  return <><h3>按交易单位预计的调整</h3><p className="quiet">按本次标记价格与原交易单位计算，不代表已发送订单、成交或实际持仓。</p><Table rowKey="instrumentId" dataSource={rows} pagination={false} scroll={{ x: 900 }} columns={[
    { title: '资产', dataIndex: 'instrumentId' }, { title: '预计调整后数量', render: (_value, row) => <Exact value={row.resultingQuantity} /> },
    { title: '预计调整后金额', render: (_value, row) => <MoneyValue money={row.resultingNotional} /> },
    { title: '预计净交易数量', render: (_value, row) => <Exact value={row.netTradeQuantity} /> },
    { title: '预计净交易金额', render: (_value, row) => <MoneyValue money={row.netTradeNotional} /> },
  ]} /></>;
}
function ModeledComparison({ comparison }: { comparison: ModeledOperatingComparison }) {
  const rows = [{ kind: '保持现有持仓', ...comparison.hold }, ...(comparison.proposed ? [{ kind: '候选调整', ...comparison.proposed }] : [])];
  return <div className="pm-modeled-comparison"><h3>同一预测期限下的模型估计</h3><p className="quiet">{comparison.method === 'MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1' ? '按原交易单位取整后的预计持仓计算。' : '按连续模型目标计算，尚未包含交易单位取整的影响。'}收益与方差按冻结模型的原预测期限计算；线性交易成本已从预测收益扣除。它们不代表每日绩效、未来回撤或已实现收益。</p>
    <Table rowKey="kind" pagination={false} dataSource={rows} scroll={{ x: 850 }} columns={[
      { title: '方案', dataIndex: 'kind' }, { title: '预测毛收益率', render: (_value, row) => pmRatio(row.grossExpectedReturn) },
      { title: '线性交易成本率', render: (_value, row) => pmRatio(row.linearTurnoverCost) }, { title: '预测净收益率', render: (_value, row) => pmRatio(row.netExpectedReturn) },
      { title: '收益方差', render: (_value, row) => <Exact value={row.variance} /> }, { title: '方法目标值（越小越优）', render: (_value, row) => <Exact value={row.objectiveValue} /> },
    ]} />
    {comparison.targetBlockReason && <ReasonList codes={[comparison.targetBlockReason]} />}
    <Disclosure title="模型比较依据"><Facts items={[["计算时点", pmTime(comparison.asOfNs)], ["方法风险参数", <Exact value={comparison.riskAversion} />], ["原预测期限", comparison.horizonRef]]} /><p>目标值为风险参数乘以收益方差，加线性成本，再减预测毛收益。参数属于事前登记的方法，不代表系统推断了你的风险偏好。</p>{comparison.limitations.length > 0 && <ul>{comparison.limitations.map((item, index) => <li key={index}>{pmAssumption(item)}</li>)}</ul>}</Disclosure>
  </div>;
}
function ResearchProgress({ view }: { view: PmMandateView }) {
  return <><section className="pm-section"><h2>研究进展</h2><Table rowKey="id" dataSource={view.campaigns} pagination={false} columns={[{ title: '原始研究', render: (_value, row) => <a href={href(`/research/${row.runtime.conversationId}`)}>查看研究</a> }, { title: '阶段', render: (_value, row) => <Status plain value={row.state} /> }, { title: '试验数', render: (_value, row) => row.trialIds.length }, { title: '结论', render: (_value, row) => row.conclusionReason ? <ReasonList codes={[row.conclusionReason]} /> : '研究中' }]} /></section>
    <section className="pm-section"><h2>策略生命周期</h2><Table rowKey="id" dataSource={view.assessments} pagination={false} scroll={{ x: 550 }} columns={[{ title: '阶段', render: (_value, row) => <Status plain value={row.stage} /> }, { title: '评估', render: (_value, row) => <Status plain value={row.outcome} /> }, { title: '依据时点', render: (_value, row) => pmTime(row.evaluatedAtNs) }, { title: '原因', render: (_value, row) => <ReasonList codes={row.reasonCodes} /> }]} /></section>
  </>;
}
function Capacity({ estimate }: { estimate: CapacityEstimate }) {
  const names = { OBSERVED_LIQUIDITY: '观测流动性', CONDITIONAL_FEASIBILITY: '条件可执行资金', ECONOMIC: '经济容量' };
  return <section className="pm-section"><div className="section-heading"><h2>{names[estimate.level]}</h2><Status plain value={estimate.status} /></div>
    <Facts items={[["实际测试最小资金", <MoneyValue money={estimate.minimumTestedCapital} />], ["实际测试最大资金", <MoneyValue money={estimate.maximumTestedCapital} />], ["有证据支持的上界", <MoneyValue money={estimate.supportedCapitalUpperBound} />], ["依据时点", pmTime(estimate.asOfNs)]]} />
    <p className="quiet">只有已验证的资金点或有依据的可行区间可供本次配置使用。</p>
    <Table rowKey={row => `${row.capital.currency}:${row.capital.amount}`} dataSource={estimate.curve} pagination={false} scroll={{ x: 650 }} columns={[{ title: '测试资金', render: (_value, row) => <MoneyValue money={row.capital} /> }, { title: '可用性', render: (_value, row) => <Status plain value={row.status} /> }, { title: '净表现', render: (_value, row) => <PmMetrics metrics={row.netPerformance} /> }, { title: '成本', render: (_value, row) => <MoneyValue money={row.estimatedCost} /> }, { title: '成交满足率', render: (_value, row) => pmRatio(row.fillRatio) }]} />
    {estimate.missingEvidence.length > 0 && <ReasonList codes={estimate.missingEvidence} />}
    {estimate.assumptions.length > 0 && <Disclosure title="本次容量结论的假设"><ul>{estimate.assumptions.map((text, index) => <li key={index}>{pmAssumption(text)}</li>)}</ul></Disclosure>}
  </section>;
}
function PmMetrics({ metrics }: { metrics: MetricObservation[] }) {
  return metrics.length ? <dl className="pm-inline-metrics">{metrics.map((metric, index) => <div key={`${metric.definition.id}:${index}`}><dt>{metric.definition.label}</dt><dd>{pmMetricValue(metric)}</dd></div>)}</dl> : <span className="quiet">未形成证据</span>;
}
function MoneyValue({ money }: { money: { amount: string; currency: string } | null | undefined }) { return <Exact value={money?.amount} unit={money?.currency} missing="未确定" />; }
function ReasonList({ codes }: { codes: string[] }) { return codes.length ? <div><ul className="pm-reasons">{[...new Set(codes.map(pmReason))].map(text => <li key={text}>{text}</li>)}</ul><Disclosure title="原始判断条件"><ul>{codes.map((code, index) => <li key={index}><code>{code}</code></li>)}</ul></Disclosure></div> : null; }
