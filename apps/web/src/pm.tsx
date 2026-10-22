import { useEffect, useState } from 'react';
import { Button, Form, Input, InputNumber, Select, Table, Tabs } from 'antd';
import { ArrowLeftIcon, ArrowRightIcon, PlusIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { DatasetView, ModelView, ObservationView, SettingsView } from '../../../packages/contracts/src/index.ts';
import type { CapacityEstimate, MetricObservation, PmMandateStart, PmMandateView, PortfolioDecision } from '../../../packages/contracts/src/pm.ts';
import { decimalUnits, draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { href, navigate, type Route } from './router';
import { Disclosure, RecordTitle } from './disclosure';
import { shortTitle } from './titles';
import { DraftNotice, Empty, ErrorNotice, Exact, Facts, Loading, PendingCommand, Status, useConfirm } from './ui';
import { pmAssumption, pmCapitalShare, pmMetricValue, pmRatio, pmReason, pmTime } from './pm-values';
import './pm.css';

const actionNames: Record<string, string> = { HOLD: '保持现有配置', REBALANCE: '调整持仓', REDUCE: '减少风险', CASH: '保留现金', NEEDS_EVIDENCE: '补齐证据后再决定', NO_FEASIBLE_SOLUTION: '当前无可行配置' };
function decisionLabel(decision: PortfolioDecision): string {
  if (!decision.validUntilNs) {
    if (decision.action === 'REBALANCE') return '历史比较选出的配置';
    if (decision.action === 'HOLD') return '历史比较倾向维持原持仓';
    if (decision.action === 'CASH') return '历史比较倾向保留现金';
  }
  return actionNames[decision.action] ?? decision.action;
}
const authorityNames: Record<string, string> = { RESEARCH_ONLY: '研究', SHADOW_TARGETS: '影子观察', AUTHORIZED_TARGETS: '已授权目标更新' };
export default function Pm({ route }: { route: Route }) {
  return route.creating ? <NewMandate /> : route.id ? <MandateDetail id={route.id} /> : <Mandates />;
}
function Mandates() {
  const query = useResource<PmMandateView[]>('/mandates', { poll: 5000 });
  return <div className="index-page pm-page"><header className="page-heading"><h1>投资决策</h1><Button type="primary" icon={<PlusIcon size={15} />} onClick={() => navigate('/pm/new')}>创建投资目标</Button></header>
    <p className="pm-introduction">从资金目标开始，持续跟踪研究证据、组合、资金配置与后续结果。</p>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="尚无投资目标"><p>提供业务目标、数据和资金意图，系统据此研究、比较并更新配置依据。</p></Empty> : <Table<PmMandateView> rowKey={row => row.mandate.id} dataSource={query.data} pagination={{ pageSize: 15, hideOnSinglePage: true }} scroll={{ x: 750 }} columns={[
      { title: '投资目标', render: (_value, row) => <a className="record-link" href={href(`/pm/${encodeURIComponent(row.mandate.id)}`)}>{shortTitle(row.mandate.ownerGoal)}</a> },
      { title: '当前判断', render: (_value, row) => row.currentDecision ? decisionLabel(row.currentDecision) : row.lastCycle ? <Status plain value={row.lastCycle.state} /> : '等待研究证据' },
      { title: '可投入资金', render: (_value, row) => <MoneyValue money={row.currentDecision?.capital?.committedCapital} /> },
      { title: '运行范围', render: (_value, row) => authorityNames[row.mandate.operatingAuthority] },
      { title: '最近依据', render: (_value, row) => pmTime(row.lastCycle?.asOfNs) },
    ]} />}
  </div>;
}
interface MandateForm { goal: string; datasetId: string; fundingKind: 'RESEARCH_SCENARIO' | 'ACCOUNT'; capital: string; accountBindingId?: string; model: string; maxTrials: number; businessPreference: 'NET_RETURN' | 'DOWNSIDE_FIRST' | 'FRONTIER'; operatingAuthority: NonNullable<PmMandateStart['operatingAuthority']> }
function NewMandate() {
  const datasets = useResource<DatasetView[]>('/datasets'); const models = useResource<ModelView[]>('/research/models'); const settings = useResource<SettingsView>('/settings');
  const observations = useResource<ObservationView[]>('/observations'); const [form] = Form.useForm<MandateForm>(); const [saved, setSaved] = useState(draftIsPersisted('pm'));
  const command = useCommand<PmMandateView>('mandates', result => { removeDraft('pm'); navigate(`/pm/${encodeURIComponent(result.mandate.id)}`); }, '/mandates');
  const original = command.values as unknown as PmMandateStart | undefined;
  const datasetId = Form.useWatch('datasetId', form); const fundingKind = Form.useWatch('fundingKind', form);
  const dataset = datasets.data?.find(row => row.id === datasetId);
  useEffect(() => {
    if (original) form.setFieldsValue({ goal: original.goal, datasetId: original.datasetId, model: `${original.model.provider}:${original.model.modelId}`, maxTrials: original.maxTrials,
      fundingKind: original.funding.kind, businessPreference: original.businessPreference ?? 'FRONTIER', operatingAuthority: original.operatingAuthority ?? 'RESEARCH_ONLY', ...(original.funding.kind === 'RESEARCH_SCENARIO' ? { capital: original.funding.capital.amount } : { accountBindingId: original.funding.accountBindingId }) });
    else { const draft = readDraft<Partial<MandateForm>>('pm'); if (draft) form.setFieldsValue(draft); }
  }, [form, original]);
  useEffect(() => {
    if (!form.getFieldValue('datasetId') && datasets.data?.length === 1) form.setFieldValue('datasetId', datasets.data[0]!.id);
    if (!form.getFieldValue('model') && models.data?.length) {
      const preferred = models.data.find(row => row.provider === settings.data?.defaultModel?.provider && row.modelId === settings.data?.defaultModel?.modelId) ?? (models.data.length === 1 ? models.data[0] : undefined);
      if (preferred) form.setFieldValue('model', `${preferred.provider}:${preferred.modelId}`);
    }
  }, [datasets.data, models.data, settings.data, form]);
  return <div className="authoring-page pm-authoring"><a className="text-link back-link" href={href('/pm')}><ArrowLeftIcon size={14} />投资决策</a><div className="authoring-heading"><h1>创建投资目标</h1></div>
    <p className="pm-introduction">描述资金与账户配置希望实现的业务结果。因子成员、比例、资本试验点与容量由系统计算和记录。<a href={href('/research/new')}>独立因子研究</a>也可单独开展。</p>
    <ErrorNotice error={datasets.error ?? models.error ?? settings.error} retry={() => { void datasets.refetch(); void models.refetch(); void settings.refetch(); }} />
    {datasets.isPending || models.isPending ? <Loading /> : <Form<MandateForm> form={form} layout="vertical" initialValues={{ fundingKind: 'RESEARCH_SCENARIO', businessPreference: 'FRONTIER', operatingAuthority: 'RESEARCH_ONLY' }} disabled={!!command.pending || command.busy}
      onValuesChange={(_changed, value) => { if (!command.pending) setSaved(saveDraft('pm', value)); }} onFinish={value => {
        const split = value.model.indexOf(':'); if (split < 1 || !dataset) return;
        void command.submit({ goal: value.goal, datasetId: value.datasetId, model: { provider: value.model.slice(0, split), modelId: value.model.slice(split + 1) }, maxTrials: value.maxTrials,
          businessPreference: value.businessPreference, operatingAuthority: value.operatingAuthority, funding: value.fundingKind === 'ACCOUNT' ? { kind: 'ACCOUNT', accountBindingId: value.accountBindingId } : { kind: 'RESEARCH_SCENARIO', capital: { amount: value.capital, currency: dataset.currency } } });
      }}>
      <PendingCommand command={command} />
      <Form.Item name="goal" label="投资目标" rules={[{ required: true, whitespace: true, message: '请描述希望实现的收益、稳定性与资金用途' }]}><Input.TextArea rows={4} autoFocus placeholder="说明希望赚取什么收益、为什么可能存在机会、重视哪些下行表现，以及资金用途。" /></Form.Item>
      <Form.Item name="datasetId" label="研究数据" rules={[{ required: true, message: '请选择已登记的数据' }]}><Select options={datasets.data?.map(row => ({ value: row.id, label: `${row.name} / ${row.currency}`, disabled: !row.ready }))} /></Form.Item>
      <Form.Item name="businessPreference" label="候选取舍" extra="业务偏好会随原目标冻结；不会代替资金、成本或可行性检查。"><Select options={[{ value: 'FRONTIER', label: '先比较收益与风险取舍' }, { value: 'NET_RETURN', label: '净收益优先' }, { value: 'DOWNSIDE_FIRST', label: '下行表现优先' }]} /></Form.Item>
      <Form.Item name="fundingKind" label="资金来源"><Select options={[{ value: 'RESEARCH_SCENARIO', label: '研究资金场景' }, { value: 'ACCOUNT', label: '已有观测账户' }]} /></Form.Item>
      <Form.Item name="operatingAuthority" label="运行范围" extra="目标更新需要独立评估、当前账户和执行证据全部满足；系统不会发送券商订单。"><Select options={[{ value: 'RESEARCH_ONLY', label: '研究与比较' }, { value: 'SHADOW_TARGETS', label: '研究并观察影子目标' }, { value: 'AUTHORIZED_TARGETS', label: '条件满足后更新已有账户目标' }]} /></Form.Item>
      {fundingKind === 'ACCOUNT' ? <><ErrorNotice error={observations.error} retry={() => { void observations.refetch(); }} /><Form.Item name="accountBindingId" label="观测账户" rules={[{ required: true, message: '请选择已有账户绑定' }]}><Select options={observations.data?.map(row => ({ value: row.id, label: `${row.name} / ${row.currency}` }))} /></Form.Item></> : <Form.Item name="capital" label="研究本金" rules={[{ required: true, message: '请输入研究本金' }, { validator: (_rule, value: string) => !value || (decimalUnits(value) ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入精确正金额，最多18位小数')) }]}><Input inputMode="decimal" suffix={dataset?.currency} /></Form.Item>}
      <Form.Item name="model" label="研究模型" rules={[{ required: true, message: '请选择可用模型' }]}><Select options={models.data?.map(row => ({ value: `${row.provider}:${row.modelId}`, label: row.name }))} /></Form.Item>
      <Form.Item name="maxTrials" label="研究试验预算" rules={[{ required: true, message: '请给出本轮可投入的试验次数' }]}><InputNumber min={1} precision={0} style={{ width: '100%' }} /></Form.Item>
      <ErrorNotice error={command.error} /><DraftNotice saved={saved || !!command.pending} />
      <div className="form-actions"><Button disabled={false} onClick={() => navigate('/pm')}>返回</Button>{command.pending ? <Button type="primary" loading={command.busy} disabled={command.busy} onClick={() => { void command.submit(); }}>核对原提交</Button> : <Button type="primary" htmlType="submit">开始研究 <ArrowRightIcon size={15} /></Button>}</div>
    </Form>}
  </div>;
}
function MandateDetail({ id }: { id: string }) {
  const path = `/mandates/${encodeURIComponent(id)}`; const query = useResource<PmMandateView>(path, { poll: 3000 }); const view = query.data;
  return <div className="pm-page"><a className="text-link back-link" href={href('/pm')}><ArrowLeftIcon size={14} />投资决策</a>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : view && <>
      <header className="pm-heading"><div><RecordTitle name={view.mandate.ownerGoal} fallback="投资目标" /><span className="quiet">{authorityNames[view.mandate.operatingAuthority]} · {view.mandate.baseCurrency}</span></div><PmActions view={view} path={path} /></header>
      <section className="pm-current"><h2>{view.currentDecision?.validUntilNs ? '当前投资判断' : '当前研究结论'}</h2><h3>{view.currentDecision ? decisionLabel(view.currentDecision) : '等待可比较的研究证据'}</h3>
        {view.lastCycle && <p className="quiet">最近依据 {pmTime(view.lastCycle.asOfNs)} · <Status plain value={view.lastCycle.state} /></p>}
        {view.currentDecision && <><DecisionSummary decision={view.currentDecision} /><ReasonList codes={view.currentDecision.reasonCodes} /></>}
        {view.mandate.unresolvedIntent.length > 0 && <ul className="pm-reasons">{view.mandate.unresolvedIntent.map((text, index) => <li key={index}>{text}</li>)}</ul>}
      </section>
      <Tabs items={[
        { key: 'allocation', label: '组合与资金', children: <Allocation view={view} /> },
        { key: 'research', label: '研究与策略', children: <ResearchProgress view={view} /> },
        { key: 'capacity', label: '容量与可行资金', children: view.capacityEstimates.length ? view.capacityEstimates.map(row => <Capacity key={row.id} estimate={row} />) : <Empty title="容量证据尚未形成"><p>系统需要相应规模、成本和退出证据；未知容量不会被填成无限。</p></Empty> },
        { key: 'outcome', label: '后续结果', children: <><h2>目标更新回执</h2><Table rowKey="id" dataSource={view.targetCommands} pagination={false} scroll={{ x: 600 }} columns={[{ title: '状态', render: (_value, row) => <Status plain value={row.state} /> }, { title: '原请求', render: (_value, row) => <code>{row.idempotencyKey}</code> }, { title: '有效至', render: (_value, row) => pmTime(row.validUntilNs) }]} /><h2>账户观测</h2><Table rowKey="id" dataSource={view.outcomes} pagination={false} scroll={{ x: 600 }} columns={[{ title: '观测时点', render: (_value, row) => pmTime(row.availableAtNs) }, { title: '目标回执', render: (_value, row) => <Status plain value={row.receiptState} /> }, { title: '账户对账', dataIndex: 'reconciliationStatus' }, { title: '已观测指标', render: (_value, row) => <PmMetrics metrics={row.realizedMetrics} /> }]} /><p className="quiet">目标被接收、实际成交和账户收益分别记录。</p></> },
        { key: 'history', label: '决策变化', children: <Table<PortfolioDecision> rowKey="id" dataSource={view.history} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 650 }} columns={[{ title: '形成时间', render: (_value, row) => pmTime(row.createdAtNs) }, { title: '判断', render: (_value, row) => decisionLabel(row) }, { title: '投入', render: (_value, row) => <MoneyValue money={row.capital?.committedCapital} /> }, { title: '状态', render: (_value, row) => <Status plain value={row.status} /> }, { title: '原因', render: (_value, row) => <ReasonList codes={row.reasonCodes} /> }]} /> },
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
    && view.assessments.some(row => row.id === strategy?.pipelineAssessmentId && row.strategyVersionId === current.strategyVersionId && row.stage === 'TARGET_OPERATION' && row.outcome === 'READY');
  return <div className="pm-actions"><Button loading={recalculate.busy} disabled={recalculate.busy} onClick={() => { void recalculate.submit({ expectedRevision: view.mandate.revision }); }}>{recalculate.pending ? '核对原重算请求' : '重算当前证据'}</Button>
    {view.mandate.operatingAuthority === 'AUTHORIZED_TARGETS' && <Button type="primary" loading={targets.busy} disabled={targets.busy || (!ready && !targets.pending)} onClick={() => { void targets.submit({ expectedRevision: view.mandate.revision, decisionId: current?.id }); }}>{targets.pending ? '核对原目标请求' : ready ? '更新账户目标' : '等待运营证据'}</Button>}
    {view.currentDecision && <Button danger disabled={retire.busy} loading={retire.busy} onClick={() => confirm('退役当前策略？', '停止新增策略消费；已有目标与持仓继续按原事实对账。', () => { void retire.submit({ expectedRevision: view.mandate.revision, strategyVersionId: view.currentDecision!.strategyVersionId }); })}>{retire.pending ? '核对原退役请求' : '退役策略'}</Button>}
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
  return <>{ensemble && <section className="pm-section"><h2>因子组合比例</h2><Table rowKey="factorVersionId" pagination={false} dataSource={ensemble.members} scroll={{ x: 550 }} columns={[{ title: '因子版本', render: (_value, row) => <code>{row.factorVersionId}</code> }, { title: '预测组合系数', render: (_value, row) => pmRatio(row.weight) }, { title: '经济同族', render: (_value, row) => <code>{row.economicFamilyId}</code> }]} /><p className="quiet">这组系数用于组合预测；资产仓位和实际投入资金分别计算。</p></section>}
    <section className="pm-section"><h2>资产仓位</h2>{!decision.validUntilNs && <p className="quiet">研究参考金额按历史末期权重与初始净值换算；当前持仓及实际净调整金额尚未建立。</p>}<Table rowKey="instrumentId" pagination={false} dataSource={decision.assetTargets} scroll={{ x: 750 }} columns={[
    { title: '资产', dataIndex: 'instrumentId' }, { title: '目标占账户净值', render: (_value, row) => pmRatio(row.navWeight) }, { title: decision.validUntilNs ? '目标金额' : '研究参考金额', render: (_value, row) => <MoneyValue money={row.targetNotional} /> }, { title: '当前金额', render: (_value, row) => <MoneyValue money={row.currentNotional} /> }, { title: '净调整金额', render: (_value, row) => <MoneyValue money={row.netTradeNotional} /> },
  ]} /></section>
    <section className="pm-section"><h2>策略预算</h2><Table rowKey="strategyVersionId" pagination={false} dataSource={decision.strategyAllocations} columns={[{ title: '策略版本', render: (_value, row) => <code>{row.strategyVersionId}</code> }, { title: '承诺资金', render: (_value, row) => <MoneyValue money={row.committedCapital} /> }, { title: '占策略总投入', render: (_value, row) => row.capitalFraction === null ? pmCapitalShare(row.committedCapital, decision.capital?.committedCapital) : pmRatio(row.capitalFraction) }]} /></section>
    {comparison && <section className="pm-section"><h2>保持与调整的比较</h2><p className="quiet">比较区间：{pmTime(comparison.scope.interval.startNs)} 至 {pmTime(comparison.scope.interval.endExclusiveNs)}（不含结束时点）。{comparison.evaluationContext === 'HISTORICAL_JOINT_REPLAY' ? '这是历史共享账户回放。' : ''}</p><Table rowKey="id" dataSource={comparison.candidates} pagination={false} scroll={{ x: 850 }} columns={[{ title: '候选动作', render: (_value, row) => row.kind === 'HOLD_CURRENT' ? '维持原持仓' : row.kind === 'REBALANCE' && !decision.validUntilNs ? '历史候选配置' : actionNames[row.kind] ?? row.kind }, { title: '过渡成本', render: (_value, row) => <PmMetrics metrics={row.transitionCost} /> }, { title: '净效果', render: (_value, row) => <PmMetrics metrics={row.netOutcome.filter(metric => metric.definition.direction !== 'DIAGNOSTIC')} /> }, { title: '下行表现', render: (_value, row) => <PmMetrics metrics={row.downside.filter(metric => metric.definition.direction !== 'DIAGNOSTIC')} /> }, { title: '回放成本与成交', render: (_value, row) => <PmMetrics metrics={[...row.netOutcome, ...row.downside].filter(metric => metric.definition.direction === 'DIAGNOSTIC')} /> }, { title: '结论', render: (_value, row) => row.id === comparison.selectedCandidateId ? '本轮选择' : <Status plain value={row.status} /> }]} /><ReasonList codes={comparison.reasonCodes} /></section>}
  </>;
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
