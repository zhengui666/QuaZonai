import { useState } from 'react';
import { Button, Form, Input, Table } from 'antd';
import type { CapitalAssumptions, CapacityReport, CapacityScenario, FactorView } from '../../../packages/contracts/src/index.ts';
import { researchCapital } from '../../../packages/contracts/src/research.ts';
import { api } from './api';
import { readDraft, saveDraft } from './commands';
import { DraftNotice, ErrorNotice, Exact, Facts, TableSurface } from './ui';
export type CapitalFormFields = { scenarios?: string; participation?: string; maximumAgeSeconds?: string };
export function capitalFields(policy?: CapitalAssumptions): CapitalFormFields {
  return policy ? { scenarios: policy.scenarios.join(', '), participation: policy.participation, maximumAgeSeconds: String(policy.maximumAgeSeconds) } : {};
}
export function capitalInput(fields: CapitalFormFields, fallbackCapital?: string): CapitalAssumptions {
  return researchCapital({ scenarios: (fields.scenarios?.trim() || fallbackCapital || '').split(/[,，]/).map(value => value.trim()), participation: fields.participation, maximumAgeSeconds: Number(fields.maximumAgeSeconds) });
}
export function CapitalFields({ required = false }: { required?: boolean }) {
  return <><Form.Item label="资本情景" name="scenarios" extra="逗号分隔的精确金额。组合留空时使用本次本金。" rules={[{ required, message: '请输入资本情景' }]}><Input inputMode="decimal" placeholder="1000, 10000" /></Form.Item><div className="form-columns"><Form.Item label="成交量参与率" name="participation" extra="例如 0.01 = 成交量的 1%" rules={[{ required, message: '请输入成交量参与率' }]}><Input inputMode="decimal" placeholder="0.01" /></Form.Item><Form.Item label="流动性数据最大时差（秒）" name="maximumAgeSeconds" rules={[{ required, message: '请输入正整数时差' }]}><Input inputMode="numeric" /></Form.Item></div></>;
}
export function FactorCapacity({ factor, report }: { factor: FactorView; report?: CapacityReport }) {
  const key = `factor-capacity:${factor.id}`; const [form] = Form.useForm<CapitalFormFields>();
  const [initial] = useState(() => readDraft<CapitalFormFields>(key) ?? capitalFields(factor.capacityAssumptions));
  const [result, setResult] = useState<CapacityReport>(); const [error, setError] = useState<unknown>(); const [busy, setBusy] = useState(false); const [saved, setSaved] = useState(false);
  return <><details className="provenance-disclosure" open={!factor.capacityAssumptions}><summary>设置容量评估假设</summary><p className="quiet">不修改原研究、模型或独立证据。仅按原执行映射评估历史条件容量；缺少转换政策或市场数据仍不可用。</p><Form className="factor-capacity-form" form={form} initialValues={initial} layout="vertical" disabled={busy || !factor.combinable} onValuesChange={(_change, values) => setSaved(saveDraft(key, values))} onFinish={async values => {
    setError(undefined); setBusy(true);
    try { const policy = capitalInput(values); setSaved(saveDraft(key, capitalFields(policy))); setResult(await api<CapacityReport>(`/factors/${encodeURIComponent(factor.id)}/capacity`, { method: 'POST', body: policy })); } catch (value) { setError(value); } finally { setBusy(false); }
  }}><CapitalFields required /><ErrorNotice error={error} /><DraftNotice saved={saved} /><Button htmlType="submit" loading={busy}>评估容量</Button></Form></details><Capacity report={result ?? report} /></>;
}
export function Capacity({ report }: { report?: CapacityReport }) {
  return <section className="result-section capacity-section"><h3>条件容量评估</h3>{!report || report.status === 'UNAVAILABLE' ? <><p className="quiet" role="status">容量不可用：{report?.reason ?? '未记录资金与执行假设'}</p>{report?.assumptions.participation && <Facts items={[["所用成交量参与率", report.assumptions.participation], ["流动性数据最大时差", `${report.assumptions.maximumAgeSeconds} 秒`], ["资本情景", report.scenarios.map(row => row.capital).join(' / ')]]} />}</> : <><p className="quiet">历史 BAR 参与率估计，不是收益承诺或当前市场深度。</p><Facts items={[["上限", <Exact value={report.maximumCapital} unit={report.currency} missing="无非零需求，未证明上限" />], ["最低实用本金", <Exact value={report.minimumCapital} unit={report.currency} />], ["约束标的", report.bindingInstrument ?? '无'], ["成交量参与率", report.assumptions.participation ?? '未记录'], ["流动性数据最大时差", `${report.assumptions.maximumAgeSeconds ?? '未记录'} 秒`]]} />
    <Table<CapacityScenario> components={{ table: TableSurface }} rowKey="capital" size="small" pagination={false} scroll={{ x: 620 }} dataSource={report.scenarios} columns={[{ title: '资本情景', render: (_value, row) => <Exact value={row.capital} unit={report.currency} /> }, { title: '最大单笔需求', render: (_value, row) => <Exact value={row.requiredTurnover} /> }, { title: '最大参与率', render: (_value, row) => <Exact value={row.maximumParticipation} /> }, { title: '可用本金', render: (_value, row) => <Exact value={row.usableCapital} /> }, { title: '剩余现金', render: (_value, row) => <Exact value={row.unallocatedCash} /> }, { title: '可行性', render: (_value, row) => <span>{({ FEASIBLE: '条件可行', CONSTRAINED: '受容量约束', TOO_SMALL: '低于最小订单', NON_BINDING: '无非零需求' } as Record<string, string>)[row.status] ?? row.status}{row.reason && <small className="quiet"> / {row.reason}</small>}</span> }]} />
    <details className="provenance-disclosure"><summary>假设与原始时钟</summary><Facts items={[["执行窗口", report.assumptions.executionWindow], ["退出假设", report.assumptions.exit], ["费用", report.assumptions.fees], ["深度", report.assumptions.depth], ["观测开始（ns）", report.observationStartNs ?? '未知'], ["观测结束（ns）", report.observationEndNs ?? '未知'], ["决策时钟（ns）", report.decisionClockNs ?? '未知'], ["源目标有效期（ns）", report.validUntilNs ?? '未知']]} /></details></> }</section>;
}
