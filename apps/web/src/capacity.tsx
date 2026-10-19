import { useState } from 'react';
import { Button, Form, Input, Table } from 'antd';
import type { CapitalAssumptions, CapacityConstraint, CapacityReport, CapacityScenario, FactorView } from '../../../packages/contracts/src/index.ts';
import { researchCapital } from '../../../packages/contracts/src/research.ts';
import { api } from './api';
import { readDraft, saveDraft } from './commands';
import { DraftNotice, ErrorNotice, Exact, Facts, MetricLabel, TableSurface, Timestamp } from './ui';
import { Reason } from './diagnostics';
import { Disclosure } from './disclosure';
import { nanosecondTimestamp } from './capacity-values';
export type CapitalFormFields = { scenarios?: string; participation?: string; maximumAgeSeconds?: string };
export function capitalFields(policy?: CapitalAssumptions): CapitalFormFields {
  return policy ? { scenarios: policy.scenarios.join(', '), participation: policy.participation, maximumAgeSeconds: String(policy.maximumAgeSeconds) } : {};
}
export function capitalInput(fields: CapitalFormFields, fallbackCapital?: string): CapitalAssumptions {
  return researchCapital({ scenarios: (fields.scenarios?.trim() || fallbackCapital || '').split(/[,，]/).map(value => value.trim()), participation: fields.participation, maximumAgeSeconds: Number(fields.maximumAgeSeconds) });
}
export function CapitalFields({ required = false }: { required?: boolean }) {
  return <><Form.Item label="资本情景" name="scenarios" extra={<><MetricLabel metricKey="scenarioCapital" label="情景口径" /> · 逗号分隔的精确金额。组合留空时使用本次本金。</>} rules={[{ required, message: '请输入资本情景' }]}><Input inputMode="decimal" placeholder="1000, 10000" /></Form.Item><div className="form-columns"><Form.Item label="成交量参与率" name="participation" extra={<><MetricLabel metricKey="participation" label="参与率口径" /> · 0.01 = 成交量的 1%</>} rules={[{ required, message: '请输入成交量参与率' }]}><Input inputMode="decimal" placeholder="0.01" /></Form.Item><Form.Item label="流动性数据最大时差（秒）" name="maximumAgeSeconds" extra={<MetricLabel metricKey="maximumAgeSeconds" label="数据时差口径" />} rules={[{ required, message: '请输入正整数时差' }]}><Input inputMode="numeric" /></Form.Item></div></>;
}
export function FactorCapacity({ factor, report }: { factor: FactorView; report?: CapacityReport }) {
  const key = `factor-capacity:${factor.id}`; const [form] = Form.useForm<CapitalFormFields>();
  const [initial] = useState(() => readDraft<CapitalFormFields>(key) ?? capitalFields(factor.capacityAssumptions));
  const [result, setResult] = useState<CapacityReport>(); const [error, setError] = useState<unknown>(); const [busy, setBusy] = useState(false); const [saved, setSaved] = useState(false);
  return <><Capacity report={result ?? report} /><details className="provenance-disclosure capacity-settings"><summary>设置容量评估假设</summary><p className="quiet">不修改原研究、模型或独立证据。仅按原执行映射评估历史条件容量；缺少转换政策或市场数据仍不可用。</p><Form className="factor-capacity-form" form={form} initialValues={initial} layout="vertical" disabled={busy || !factor.combinable} onValuesChange={(_change, values) => setSaved(saveDraft(key, values))} onFinish={async values => {
    setError(undefined); setBusy(true);
    try { const policy = capitalInput(values); setSaved(saveDraft(key, capitalFields(policy))); setResult(await api<CapacityReport>(`/factors/${encodeURIComponent(factor.id)}/capacity`, { method: 'POST', body: policy })); } catch (value) { setError(value); } finally { setBusy(false); }
  }}><CapitalFields required /><ErrorNotice error={error} /><DraftNotice saved={saved} /><Button htmlType="submit" loading={busy}>评估容量</Button></Form></details></>;
}
export function Capacity({ report }: { report?: CapacityReport }) {
  const available = report && report.status !== 'UNAVAILABLE';
  return <section className="result-section capacity-section" aria-label="条件容量评估"><div className="section-heading"><h3>条件容量评估</h3><span className="capacity-kind">历史流动性</span></div>
    {available ? <><div className="capacity-headline"><MetricLabel metricKey="maximumCapital" /><strong><Exact value={report.maximumCapital} unit={report.currency} missing="未证明非零资金上限" /></strong></div><p className="quiet capacity-caveat">历史 BAR 参与率估计，不是收益承诺、盈利最优本金或当前市场深度。</p>{report.reason && <Reason value={report.reason} />}<Facts items={[["最低实用本金", <Exact value={report.minimumCapital} unit={report.currency} />], ["约束标的", report.bindingInstrument ?? '未形成约束'], ["成交量参与率", report.assumptions.participation ?? '未记录'], ["流动性数据最大时差", `${report.assumptions.maximumAgeSeconds ?? '未记录'} 秒`]]} />
      {!!report.constraints?.length && <section className="capacity-constraints"><h4>最紧历史约束</h4><Table<CapacityConstraint> components={{ table: TableSurface }} rowKey="kind" size="small" pagination={false} scroll={{ x: 760 }} dataSource={report.constraints} columns={[
        { title: '需求路径', render: (_value, row) => <MetricLabel metricKey={row.kind === 'FULL_EXIT' ? 'exitTurnover' : 'entryRebalanceTurnover'} label={row.kind === 'FULL_EXIT' ? '独立全量退出' : '建仓／再平衡'} /> },
        { title: <MetricLabel metricKey="maximumCapital" label="约束资金上限" />, render: (_value, row) => <Exact value={row.maximumCapital} unit={report.currency} /> },
        { title: '标的', dataIndex: 'instrumentId' }, { title: <MetricLabel metricKey="decisionNs" label="限制决策时刻" />, render: (_value, row) => <CapacityClock value={row.decisionNs} /> },
      ]} /></section>}
      <section className="capacity-scenarios"><h4>资本情景</h4><ScenarioTable scenarios={report.scenarios} currency={report.currency} /></section></> : <div className="capacity-unavailable" role="status"><strong>容量不可用</strong><Reason value={report?.reason} fallback={report ? '原报告未提供已分类的具体原因，请按原始身份核对执行。' : '尚无容量报告，不据此推断可用资金。请查看原执行状态，并明确资金与策略映射假设。'} />{report?.assumptions.participation && <Facts items={[["所用成交量参与率", report.assumptions.participation], ["流动性数据最大时差", `${report.assumptions.maximumAgeSeconds} 秒`], ["资本情景", report.scenarios.map(row => row.capital).join(' / ')]]} />}</div>}
    {report?.coverage && <section className="capacity-coverage"><h4>观测覆盖</h4><Facts items={[
      [<MetricLabel metricKey="decisionCount" />, <Exact value={report.coverage.decisionCount} />], [<MetricLabel metricKey="assessedDecisionCount" />, <Exact value={report.coverage.assessedDecisionCount} />],
      [<MetricLabel metricKey="requiredObservations" />, <Exact value={report.coverage.requiredObservations} />], [<MetricLabel metricKey="coveredObservations" />, <Exact value={report.coverage.coveredObservations} />],
      [<MetricLabel metricKey="uniqueBars" />, <Exact value={report.coverage.uniqueBars} />], [<MetricLabel metricKey="reusedBarObservations" />, <Exact value={report.coverage.reusedBarObservations} />],
    ]} /></section>}
    {!!report?.volumeStress?.length && <section className="capacity-stress"><h4>假设成交量压力</h4><p className="quiet">这是明确假设的流动性敏感性，不是概率、预测或置信区间；不在浏览器重新计算资金。</p><Table<NonNullable<CapacityReport['volumeStress']>[number]> components={{ table: TableSurface }} rowKey="volumeMultiplier" size="small" pagination={false} scroll={{ x: 490 }} dataSource={report.volumeStress} columns={[
      { title: <MetricLabel metricKey="volumeMultiplier" />, render: (_value, row) => <><Exact value={row.volumeMultiplier} />{row.volumeMultiplier === '1' ? '（基准）' : row.volumeMultiplier === '0.5' ? '（假设减半）' : '（明确假设）'}</> },
      { title: <MetricLabel metricKey="maximumCapital" />, render: (_value, row) => <Exact value={row.maximumCapital} unit={report.currency} missing={row.status === 'NON_BINDING' ? '未证明上限' : '不可用'} /> },
      { title: '说明', render: (_value, row) => row.reason ? <Reason value={row.reason} /> : <span>{({ UNAVAILABLE: '不可用', NON_BINDING: '未形成约束', ESTIMATED: '历史条件估计' } as Record<string, string>)[row.status] ?? '未识别状态'}</span> },
    ]} /><Disclosure title="压力情景下的资金明细">{report.volumeStress.map(stress => <div className="stress-scenario" key={stress.volumeMultiplier}><h5>成交量 × {stress.volumeMultiplier}</h5><ScenarioTable scenarios={stress.scenarios} currency={report.currency} /></div>)}</Disclosure></section>}
    <div className="economic-capacity"><strong><MetricLabel metricKey="economicCapacity" label={report?.economicCapacity ? '经济容量不可用' : '经济容量未提供'} /></strong><p>历史流动性上限不是盈利容量。还需要匹配资金规模的样本外净收益、市场校准的冲击、价差／费用与执行及退出证据；不以任意冲击系数或置信区间代替。</p>{report?.economicCapacity && <><ul>{report.economicCapacity.missingInputs.map(input => <li key={input}>{economicInputs[input] ?? '未识别经济输入（见原始字段）'}</li>)}</ul><details className="reason-technical"><summary>原始经济容量缺失项</summary><code>{report.economicCapacity.reason}</code>{report.economicCapacity.missingInputs.map(input => <code key={input}>{input}</code>)}</details></>}</div>
    {report && <details className="provenance-disclosure capacity-provenance"><summary>方法、假设与原始时钟</summary><p className="quiet">范围：{report.scope === 'portfolio' ? '共享账户组合的净目标需求，不加总成员容量' : /^r\d+\.t\d+\.m\d+$/.test(report.scope) ? '原单因子执行映射；纯分数没有独立金额容量' : '原报告声明的范围，见下方原始字段'}。仅使用决策前已可得的历史 BAR 名义成交量。</p>{report.methodology?.id === 'HISTORICAL_BAR_PARTICIPATION_V2' ? <div className="capacity-method"><p>资金上限 = 参与率 × 原 BAR 名义成交量 ÷ max（累计绝对净目标变化，最大保留目标权重），再取所有标的／BAR 中最小值。</p><p>从现金建仓；重复 BAR 共享同一预算。全量退出是独立替代路径，不与策略换仓相加。采用静态资金与目标权重近似，不重建原生成交、盈亏或持仓漂移。</p></div> : <p className="quiet">原报告未提供当前分项方法，不补推缺失的约束、覆盖或压力结果。</p>}{report.methodology && <Disclosure title="原生方法字段"><code>{report.methodology.id}</code><p>{report.methodology.formula}</p>{report.methodology.notes.map(note => <p key={note}>{note}</p>)}</Disclosure>}{!!report.constraints?.length && <Disclosure title="约束 BAR 的原始流动性与时钟">{report.constraints.map(constraint => <div className="constraint-source" key={constraint.kind}><h5>{constraint.kind === 'FULL_EXIT' ? '独立全量退出' : '建仓／再平衡'} / {constraint.instrumentId}</h5><Facts items={[
      [<MetricLabel metricKey="observedNotional" />, <Exact value={constraint.observedNotional} unit={report.currency} />], [<MetricLabel metricKey="requiredWeight" />, <Exact value={constraint.requiredWeight} />],
      [<MetricLabel metricKey="barEventNs" />, <CapacityClock value={constraint.barEventNs} />], [<MetricLabel metricKey="barAvailableNs" />, <CapacityClock value={constraint.barAvailableNs} />],
    ]} /></div>)}</Disclosure>}<Facts items={[["原始状态", report.status], ["报告范围", report.scope], ["原始方法", report.basis], ["执行窗口", report.assumptions.executionWindow], ["退出假设", report.assumptions.exit], ["原始费用假设", report.assumptions.fees], ["原始深度假设", report.assumptions.depth], ["观测开始（ns）", report.observationStartNs ?? '未知'], ["观测结束（ns）", report.observationEndNs ?? '未知'], ["决策时钟（ns）", report.decisionClockNs ?? '未知'], ["源目标有效期（ns）", report.validUntilNs ?? '未知']]} /></details>}
  </section>;
}
const economicInputs: Record<string, string> = {
  matched_out_of_sample_returns_at_capital_scales: '匹配各资金规模的样本外净收益', venue_calibrated_market_impact: '按市场与交易场所校准的价格冲击',
  matched_spread_fees_and_execution_timing: '匹配的价差、费用与执行时序', validated_exit_execution: '经过验证的退出执行',
};
function CapacityClock({ value }: { value: string }) {
  const timestamp = nanosecondTimestamp(value);
  return <div className="capacity-clock">{timestamp && <Timestamp value={timestamp} />}<code>{value} ns</code></div>;
}
function ScenarioTable({ scenarios, currency }: { scenarios: CapacityScenario[]; currency: string }) {
  return <Table<CapacityScenario> components={{ table: TableSurface }} rowKey="capital" size="small" pagination={false} scroll={{ x: 860 }} dataSource={scenarios} columns={[
    { title: <MetricLabel metricKey="scenarioCapital" />, render: (_value, row) => <Exact value={row.capital} unit={currency} /> },
    { title: <MetricLabel metricKey="requiredTurnover" />, render: (_value, row) => <Exact value={row.requiredTurnover} unit={currency} /> },
    { title: <MetricLabel metricKey="maximumParticipation" />, render: (_value, row) => <Exact value={row.maximumParticipation} /> },
    { title: <MetricLabel metricKey="usableCapital" />, render: (_value, row) => <Exact value={row.usableCapital} unit={currency} /> },
    { title: <MetricLabel metricKey="unallocatedCash" />, render: (_value, row) => <Exact value={row.unallocatedCash} unit={currency} /> },
    { title: '需求分解', render: (_value, row) => [row.entryRebalanceTurnover, row.exitTurnover, row.entryRebalanceParticipation, row.exitParticipation].some(value => value !== undefined) ? <details className="scenario-breakdown"><summary>建仓与退出</summary><Facts items={[
      [<MetricLabel metricKey="entryRebalanceTurnover" />, <Exact value={row.entryRebalanceTurnover} unit={currency} />], [<MetricLabel metricKey="exitTurnover" />, <Exact value={row.exitTurnover} unit={currency} />],
      [<MetricLabel metricKey="entryRebalanceParticipation" />, <Exact value={row.entryRebalanceParticipation} />], [<MetricLabel metricKey="exitParticipation" />, <Exact value={row.exitParticipation} />],
    ]} /></details> : <span className="quiet">未记录</span> },
    { title: '可行性', render: (_value, row) => <div>{({ FEASIBLE: '条件可行', CONSTRAINED: '受容量约束', TOO_SMALL: '低于最小订单', NON_BINDING: '无非零需求', UNAVAILABLE: '不可用' } as Record<string, string>)[row.status] ?? '未识别状态'}{row.reason && <Reason value={row.reason} />}</div> },
  ]} />;
}
