import { useState } from 'react';
import { Button, Table } from 'antd';
import type { ReviewMetric, ReviewMetricRule, ReviewReport, ReviewWeights } from '../../../packages/contracts/src/review.ts';
import { Disclosure } from './disclosure';
import { Exact, Facts, MetricLabel, Status, Timestamp } from './ui';
import { metricDefinition } from './metric-glossary';
import './review-report.css';

const decisions = { PASS: '通过评审', REJECT: '不予放行', INCONCLUSIVE: '证据不足，暂不放行' };
const targets = { FACTOR: '因子', PORTFOLIO: '因子组合', ALLOCATION: '多组合资金配置' };
const levels = { FACTOR: '因子融合权重', ASSET: '资产目标权重', PORTFOLIO: '多组合资金权重' };
const availability = { AVAILABLE: '可用', UNAVAILABLE: '不可用', UNSUPPORTED: '不支持', INSUFFICIENT: '样本不足', INVALID: '无效' };
const analysisStates = { COMPLETE: '已完成', INCOMPLETE: '证据不完整', FAILED: '执行失败' };

/** Render only the server's frozen decision and values. The browser never re-scores a report. */
export function ReviewReportPanel({ report }: { report?: ReviewReport | null }) {
  const [exportError, setExportError] = useState('');
  const download = (format: 'json' | 'md') => {
    if (!report) return;
    let url: string | undefined;
    try {
      const content = format === 'json' ? JSON.stringify(report, null, 2) : reviewMarkdown(report);
      url = URL.createObjectURL(new Blob([content], { type: format === 'json' ? 'application/json;charset=utf-8' : 'text/markdown;charset=utf-8' }));
      const link = document.createElement('a'); link.href = url; link.download = reviewFilename(report, format);
      document.body.append(link); link.click(); link.remove(); setExportError('');
      const allocatedUrl = url; setTimeout(() => URL.revokeObjectURL(allocatedUrl), 1000);
    } catch {
      if (url) URL.revokeObjectURL(url);
      setExportError('报告导出失败，请重试');
    }
  };
  if (!report) return <section className="result-section review-report"><h3>独立评审</h3><p className="quiet">尚无评审报告</p></section>;
  return <section className="result-section review-report" aria-label="独立评审报告">
    <div className="section-heading review-heading"><h3>独立评审 · {targets[report.target.kind]}</h3><div className="review-exports"><Button size="small" onClick={() => download('md')}>导出报告</Button><Button size="small" onClick={() => download('json')}>导出 JSON</Button></div></div>
    <p className="quiet review-target">评审对象 <code>{report.target.id}</code></p>
    {exportError && <p className="error-text" role="alert">{exportError}</p>}
    <div className="review-outcome" data-decision={report.decision}>
      <div className="review-total"><span>总分（0–100）</span><strong><Exact value={report.totalScore} missing="不可用" /></strong></div>
      <div className="review-decision"><Status value={report.decision === 'REJECT' ? 'REJECTED' : report.decision === 'INCONCLUSIVE' ? 'UNCERTAIN' : 'READY'} label={decisions[report.decision]} /><p className="quiet">{targets[report.target.kind]} · 质量系数 <Exact value={report.qualityCoefficient} /></p></div>
    </div>
    {!!report.hardFailures.length && <ReviewReasons title="硬性不通过" values={report.hardFailures} tone="failure" />}
    {!!report.missingEvidence.length && <ReviewReasons title="待补证据" values={report.missingEvidence} tone="missing" />}
    {report.narrative && <div className="review-narrative"><h4>审阅说明</h4><p>{report.narrative}</p></div>}
    <h4>维度评分</h4>
    <Table className="review-dimensions" rowKey="id" size="small" pagination={false} scroll={{ x: 460 }} dataSource={report.dimensions} columns={[
      { title: '维度', dataIndex: 'label' },
      { title: '得分（0–100）', align: 'right', render: (_cell, row) => <Exact value={row.score} missing="不可用" /> },
      { title: '计分权重', align: 'right', render: (_cell, row) => <Exact value={row.weight} /> },
      { title: '证据', render: (_cell, row) => <><span>{row.status === 'COMPLETE' ? '完整' : '不完整'}</span>{!!row.reasons.length && <ReviewReasons values={row.reasons} />}</> },
    ]} />
    {report.weights.map((weights, index) => <WeightRecommendation key={`${weights.level}:${index}`} weights={weights} />)}
    <Disclosure title="科学工具与指标证据" className="review-evidence">
      {!report.analyses.length ? <p className="quiet">未记录工具分析</p> : report.analyses.map((analysis, index) => <div className="review-analysis" key={`${analysis.kind}:${index}`}><h4>{analysis.kind}</h4><span>{analysisStates[analysis.status]}</span>{analysis.reason && <p>{analysis.reason}</p>}{analysis.evidenceRef && <p className="quiet">证据 <code>{analysis.evidenceRef}</code></p>}{!!analysis.metrics.length && <p className="quiet">指标 <code>{analysis.metrics.join(' / ')}</code></p>}</div>)}
      {!!report.metrics.length && <Table<ReviewMetric> rowKey={row => `${row.key}:${row.evidenceRef}`} size="small" pagination={false} scroll={{ x: 620 }} dataSource={report.metrics} columns={[
        { title: '指标', render: (_cell, row) => <><MetricLabel metricKey={row.key} /><details className="metric-technical"><summary>原始字段</summary><code>{row.key}</code><code>{row.method}</code><code>{row.frequency}</code><code>{row.evidenceRef}</code></details></> },
        { title: '数值', align: 'right', render: (_cell, row) => <Exact value={row.value} unit={row.unit} missing="不可用" /> },
        { title: '观测数', align: 'right', dataIndex: 'observationCount' },
        { title: '可用性', render: (_cell, row) => <>{availability[row.status]}{row.reason && <p>{row.reason}</p>}</> },
      ]} />}
      {!!report.checks.length && <><h4>检查记录</h4><ul className="review-checks">{report.checks.map(check => <li key={check.id}><code>{check.id}</code><span>{({ PASS: '通过', FAIL: '未通过', UNKNOWN: '未知' })[check.status]}</span><p>{check.reason}</p>{check.evidenceRefs.map(ref => <code key={ref}>{ref}</code>)}</li>)}</ul></>}
    </Disclosure>
    <Disclosure title="候选版本与评分政策" className="review-provenance">
      <Facts items={[["评审对象", targets[report.target.kind]], ["对象身份", <code>{report.target.id}</code>], ["候选版本", <code>{report.target.version}</code>], ["报告身份", <code>{report.id}</code>], ["政策身份", <code>{report.policyId}</code>], ["评估开始（ns）", <code>{report.target.evaluationStartNs}</code>], ["报告时间", <Timestamp value={report.createdAt} />], ["放行分数", report.policy.passScore], ["观察分数", report.policy.observeScore], ["关键维度最低分", report.policy.dimensionFloor], ["参考集合根血缘数", report.policy.calibration?.rootCount ?? '未提供']]} />
      {!!report.policy.metricRules.length && <><h4>冻结计分标尺与准入底线</h4><p className="quiet">归一化锚点决定相对得分；指标还须满足独立的准入底线，高相对得分不能抵消未达底线。</p><Table rowKey="key" size="small" pagination={false} scroll={{ x: 700 }} dataSource={report.policy.metricRules} columns={[
        { title: '指标', render: (_cell, row) => <MetricLabel metricKey={row.key} /> },
        { title: '方向', render: (_cell, row) => row.direction === 'HIGHER' ? '越高越好' : '越低越好' },
        { title: '低位锚点', align: 'right', render: (_cell, row) => <Exact value={row.low} unit={row.unit} /> },
        { title: '高位锚点', align: 'right', render: (_cell, row) => <Exact value={row.high} unit={row.unit} /> },
        { title: '准入底线', align: 'right', render: (_cell, row) => <Exact value={minimumTargetLabel(row)} unit={row.minimumTarget ? row.unit : undefined} /> },
        { title: '最少观测', align: 'right', dataIndex: 'minimumObservations' },
        { title: '要求', render: (_cell, row) => row.required ? '必需' : '补充' },
      ]} /></>}
      {!!report.target.sourceEvidenceRefs.length && <ReviewReasons title="来源证据" values={report.target.sourceEvidenceRefs} />}
      {!!report.reasonCodes.length && <ReviewReasons title="决策原因" values={report.reasonCodes} />}
    </Disclosure>
  </section>;
}

function ReviewReasons({ title, values, tone = '' }: { title?: string; values: string[]; tone?: string }) {
  return <div className={`review-reasons ${tone}`}>{title && <h4>{title}</h4>}<ul>{values.map((reason, index) => <li key={`${index}:${reason}`}>{reviewReason(reason)}{reviewReason(reason) !== reason && <details className="reason-technical"><summary>原始原因</summary><code>{reason}</code></details>}</li>)}</ul></div>;
}

export function reviewReason(reason: string): string {
  const known: Record<string, string> = { reference_calibration_missing: '缺少冻结的历史参考集合，当前评分不能用于自动放行。', reference_calibration_not_prior: '历史参考集合与本次评估区间重叠，须使用更早的参考集合。', score_below_reject_threshold: '评分低于观察区间，不予放行。', required_dimension_below_floor: '统计、稳健性或可交易性维度未达到最低分。', observation_band: '评分处于观察区间，暂不自动放行。', below_absolute_admission_minimum: '未达到冻结的绝对准入底线，较高的相对得分仍不能放行。', missing_check: '缺少检查结果', missing: '缺少必需指标', insufficient_or_invalid: '样本不足或指标值无效', method_or_scope_mismatch: '指标方法、单位或频率与冻结政策不一致' };
  if (known[reason]) return known[reason]!;
  const index = reason.indexOf(':');
  if (index < 0) return reason;
  const field = reason.slice(0, index); const detail = reason.slice(index + 1);
  const checks: Record<string, string> = { provenance: '数据与输入来源', implementation: '模型实现', trial_ledger: '完整试验账本', reproducibility: '可复现性', no_leakage: '时间泄漏检查', independent_window: '独立评估窗口', execution_constraints: '执行约束' };
  const metric = metricDefinition(field).label;
  const label = checks[field] ?? (metric === '未识别指标' ? undefined : metric);
  return label ? `${label}：${known[detail] ?? detail}` : reason;
}

function WeightRecommendation({ weights }: { weights: ReviewWeights }) {
  return <section className="review-weights"><h4>{levels[weights.level]}</h4><p>{weights.status === 'PROPOSED' ? '候选建议，须完成新候选共享账户验证' : '当前不可配权'}</p>{weights.reason && <p>{weights.reason}</p>}{!!weights.members.length && <Table rowKey="id" size="small" pagination={false} scroll={{ x: 360 }} dataSource={weights.members} columns={[{ title: '成员身份', render: (_cell, row) => <code>{row.id}</code> }, { title: '权重（1 = 100%）', align: 'right', render: (_cell, row) => <Exact value={row.weight} /> }]} />}<Facts items={[["新候选", weights.candidateId ? <code>{weights.candidateId}</code> : '尚未创建'], ["配权报告来源", weights.sourceReportIds.length ? weights.sourceReportIds.map(id => <code key={id}>{id}</code>) : '未记录']]} /></section>;
}

export function reviewFilename(report: ReviewReport, extension: 'json' | 'md'): string {
  const identity = report.id.replace(/[^a-zA-Z0-9_.-]/g, '_').slice(0, 80) || 'report';
  return `quazonai-review-${identity}.${extension}`;
}

// Keep model prose and evidence identities inert in downloaded Markdown as well as React.
const text = (value: unknown): string => String(value ?? '不可用').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/[\\`*_{}\[\]()#+.!|]/g, '\\$&').replace(/[\r\n]+/g, ' ');
const minimumTargetLabel = (rule: ReviewMetricRule): string => rule.minimumTarget ? `${rule.minimumTarget.inclusive ? '≥' : '>'} ${rule.minimumTarget.value}` : '未设置';
export function reviewMarkdown(report: ReviewReport): string {
  const lines = ['# QuaZonai 独立评审报告', '', `对象：${targets[report.target.kind]} ${text(report.target.id)}`, `候选版本：${text(report.target.version)}`, `报告：${text(report.id)}`, `政策：${text(report.policyId)}`, `时间：${text(report.createdAt)}`, '', `结论：${decisions[report.decision]}`, `总分（0–100）：${text(report.totalScore)}`, `质量系数：${text(report.qualityCoefficient)}`, ''];
  const reasons = (title: string, values: string[]) => { if (values.length) lines.push(`## ${title}`, '', ...values.map(value => `- ${text(reviewReason(value))}${reviewReason(value) === value ? '' : `（${text(value)}）`}`), ''); };
  reasons('硬性不通过', report.hardFailures); reasons('待补证据', report.missingEvidence);
  if (report.narrative) lines.push('## 审阅说明', '', text(report.narrative), '');
  lines.push('## 维度评分', '', '| 维度 | 得分（0–100） | 计分权重 | 证据 |', '| --- | ---: | ---: | --- |', ...report.dimensions.map(row => `| ${text(row.label)} | ${text(row.score)} | ${text(row.weight)} | ${row.status === 'COMPLETE' ? '完整' : '不完整'} ${row.reasons.map(text).join(' / ')} |`), '');
  for (const weights of report.weights) {
    lines.push(`## ${levels[weights.level]}`, '', weights.status === 'PROPOSED' ? '候选建议，须完成新候选共享账户验证' : '当前不可配权', `新候选：${text(weights.candidateId)}`, `原因：${text(weights.reason)}`, '', '| 成员 | 权重（1 = 100%） |', '| --- | ---: |', ...weights.members.map(member => `| ${text(member.id)} | ${text(member.weight)} |`), '', `报告来源：${weights.sourceReportIds.map(text).join(' / ')}`, '');
  }
  lines.push('## 科学工具与指标证据', '', ...report.analyses.map(analysis => `- ${text(analysis.kind)}：${analysisStates[analysis.status]}；${text(analysis.reason)}；证据 ${text(analysis.evidenceRef)}`), '', '| 指标 | 原始字段 | 数值 | 单位 | 可用性 | 观测数 | 方法 | 频率 | 证据 |', '| --- | --- | ---: | --- | --- | ---: | --- | --- | --- |', ...report.metrics.map(metric => `| ${text(metricDefinition(metric.key).label)} | ${text(metric.key)} | ${text(metric.value)} | ${text(metric.unit)} | ${availability[metric.status]} ${text(metric.reason ?? '')} | ${text(metric.observationCount)} | ${text(metric.method)} | ${text(metric.frequency)} | ${text(metric.evidenceRef)} |`), '');
  reasons('来源证据', report.target.sourceEvidenceRefs); reasons('决策原因', report.reasonCodes);
  lines.push('## 检查记录', '', ...report.checks.map(check => `- ${text(check.id)}：${text(check.status)}；${text(check.reason)}；${check.evidenceRefs.map(text).join(' / ')}`), '', '## 冻结放行条件', '', `通过分数：${text(report.policy.passScore)}；观察分数：${text(report.policy.observeScore)}；关键维度最低分：${text(report.policy.dimensionFloor)}`, `参考集合根血缘数：${text(report.policy.calibration?.rootCount)}`, '');
  if (report.policy.metricRules.length) lines.push('### 冻结计分标尺与准入底线', '', '归一化锚点决定相对得分；指标还须满足独立的准入底线，高相对得分不能抵消未达底线。', '', '| 指标 | 原始字段 | 方向 | 低位锚点 | 高位锚点 | 准入底线 | 单位 | 最少观测 | 要求 |', '| --- | --- | --- | ---: | ---: | --- | --- | ---: | --- |', ...report.policy.metricRules.map(rule => `| ${text(metricDefinition(rule.key).label)} | ${text(rule.key)} | ${rule.direction === 'HIGHER' ? '越高越好' : '越低越好'} | ${text(rule.low)} | ${text(rule.high)} | ${text(minimumTargetLabel(rule))} | ${text(rule.unit)} | ${text(rule.minimumObservations)} | ${rule.required ? '必需' : '补充'} |`), '');
  return lines.join('\n');
}
