import { App, Button, Skeleton, Table } from 'antd';
import { CheckCircleIcon, ClockIcon, CopyIcon, FlaskIcon, InfoIcon, WarningCircleIcon } from '@phosphor-icons/react';
import { useEffect, useState, type ReactNode, type TableHTMLAttributes } from 'react';
import type { Metric } from '../../../packages/contracts/src/index.ts';
import { ApiError, shortError } from './api';
import { metricDefinition, partitionLabel } from './metric-glossary';
import { FactLabel, MetricLabel } from './metric-label';
export { MetricLabel } from './metric-label';

const labels: Record<string, string> = {
  RUNNING: '研究中', IDLE: '当前无执行', STARTED: '计算中', COMPUTED: '已计算', FAILED: '失败', STOPPED: '已停止', REJECTED: '已拒绝',
  QUEUED: '待执行', OBSERVING: '观测中', UNCERTAIN: '待核对', PENDING: '待接收', ACKNOWLEDGED: '已接收', EXPIRED: '已过期', REVOKED: '已撤销', NOT_OFFERED: '未交付',
  loaded: '已加载', disabled: '已停用', pending_restart: '待重启', error: '加载失败', running: '执行中', pending: '待执行', waiting: '等待中', completing: '收尾中',
  terminal: '已结束', completed: '已完成', failed: '失败', aborted: '已中止', faulted: '执行异常', done: '已回答', queued: '待处理', placed: '已送入执行', unanswered: '未完成回答', cancelled: '已取消',
};
export function TableSurface(props: TableHTMLAttributes<HTMLTableElement>) { return <table {...props} tabIndex={0} />; }
export function stateLabel(value: string) { return labels[value] ?? value; }
export function Status({ value, plain = false, label }: { value: string; plain?: boolean; label?: string }) {
  const tone = /FAILED|REJECTED|failed|faulted|^error$/.test(value) ? 'error' : /UNCERTAIN|pending_restart|unanswered/.test(value) ? 'warning' : /RUNNING|STARTED|running/.test(value) ? 'active' : 'neutral';
  const Icon = tone === 'error' || tone === 'warning' ? WarningCircleIcon : /RUNNING|STARTED|QUEUED|PENDING|running|pending|queued|waiting|completing/.test(value) ? ClockIcon : /COMPUTED|ACKNOWLEDGED|^loaded$|^completed$|^done$|^configured$|^ready$/.test(value) ? CheckCircleIcon : InfoIcon;
  return <span className={`status ${tone} ${plain ? 'plain' : ''}`}><Icon size={14} />{label ?? stateLabel(value)}</span>;
}
export function Loading({ rows = 3 }: { rows?: number }) { return <div className="loading-state" role="status" aria-label="加载中"><Skeleton active={false} title paragraph={{ rows }} /></div>; }
export function ErrorNotice({ error, retry }: { error: unknown; retry?: () => void }) {
  if (!error) return null;
  return <div className="error-notice" role="alert"><div className="error-heading"><WarningCircleIcon size={17} /><span>{shortError(error)}</span>{retry && <Button size="small" onClick={retry}>重试</Button>}</div>{error instanceof ApiError && error.code && <details><summary>技术详情</summary><code>{error.code}</code></details>}</div>;
}
export function Empty({ title, action, children }: { title: string; action?: ReactNode; children?: ReactNode }) { return <div className="empty"><div className="empty-icon"><FlaskIcon size={24} weight="duotone" /></div><h3>{title}</h3>{children}{action}</div>; }
export function Facts({ items }: { items: [ReactNode, ReactNode][] }) { return <dl className="facts">{items.map(([label, value], index) => <div key={typeof label === 'string' ? label : index}><dt>{typeof label === 'string' ? <FactLabel label={label} /> : label}</dt><dd>{value ?? '未提供'}</dd></div>)}</dl>; }
export function Timestamp({ value }: { value: string | null | undefined }) { return value ? <time dateTime={value}>{value.replace('T', ' ').replace(/Z$/, ' UTC')}</time> : <span className="quiet">未记录</span>; }
export function exact(value: number | string | null | undefined, missing = '未提供') { return value == null ? missing : String(value); }
export function Exact({ value, unit, missing }: { value: number | string | null | undefined; unit?: string; missing?: string }) { return <span className="exact-value">{exact(value, missing)}{value != null && unit && <span className="value-unit"> {unit}</span>}</span>; }
const availability: Record<string, string> = { INSUFFICIENT_SAMPLES: '样本不足', CONSTANT_SCORE: '分数恒定', CONSTANT_LABEL: '标签恒定', NONFINITE_DATA: '数据非有限', NONFINITE_RESULT: '结果非有限', AVAILABLE: '可用' };
export function metricStatus(metric: Metric) { return metric.status ? availability[metric.status] ?? '未识别可用性' : metric.value == null ? '未提供' : ''; }
export function metricLabel(metric: Metric) { return metricDefinition(metric.key).label; }
export function MetricField({ metric }: { metric: Metric }) {
  return <div className="metric-name"><MetricLabel metricKey={metric.key} />{metric.partition && <small>{partitionLabel(metric.partition)}{metric.component !== undefined && ` / 分量 ${metric.component}`}</small>}<details className="metric-technical"><summary>原始字段</summary><code>{metric.key}</code>{metric.label !== metric.key && <span>{metric.label}</span>}{metric.partition && <code>{metric.partition}</code>}{metric.status && <code>{metric.status}</code>}</details></div>;
}
export function Metrics({ metrics }: { metrics: Metric[] }) {
  if (!metrics.length) return <p className="quiet">未提供指标</p>;
  return <Table<Metric> className="metric-table" rowKey="key" size="small" pagination={false} dataSource={metrics} columns={[
    { title: '指标', render: (_value, metric) => <MetricField metric={metric} /> },
    { title: '数值', align: 'right', render: (_value, metric) => <Exact value={metric.value} unit={metric.unit} missing="不可用" /> },
    { title: '可用性', render: (_value, metric) => <span className="quiet">{metricStatus(metric)}</span> },
  ]} />;
}
export function EvidenceMetrics({ metrics }: { metrics: Metric[] }) {
  const independent = metrics.filter(metric => metric.partition === 'HELDOUT' || metric.key.startsWith('HELDOUT.'));
  const source = independent.length ? independent : metrics.some(metric => metric.partition || /^(TRAIN|VALIDATION|HELDOUT)\./.test(metric.key)) ? [] : metrics;
  const ic = source.find(metric => /(?:pearson_ic|^ic)$/.test(metric.key));
  const included = source.find(metric => metric.key.endsWith('counts.included'));
  const excluded = source.find(metric => metric.key.endsWith('counts.excluded'));
  const counters = [included, excluded].filter((item): item is Metric => !!item);
  const longCounts = counters.some(metric => String(metric.value).length > 9);
  if (!ic && !included && !excluded) return <Metrics metrics={metrics} />;
  return <><div className={`metric-grid${longCounts ? ' long-counts' : ''}`} data-columns={counters.length + Number(!!ic)}>
    {ic && <div className="metric-primary"><span className="metric-label"><MetricLabel metricKey={ic.key} label={independent.length ? '独立样本 IC' : undefined} /></span><strong><Exact value={ic.value} unit={ic.unit} missing="不可用" /></strong>{metricStatus(ic) && <span className="metric-reason"><InfoIcon size={13} />{metricStatus(ic)}</span>}</div>}
    {counters.map(metric => <div className="metric-count" key={metric.key}><span className="metric-label"><MetricLabel metricKey={metric.key} /></span><strong><Exact value={metric.value} missing="不可用" /></strong></div>)}
  </div>{independent.length > 0 && <div className="partition-row"><span className="partition-label">独立样本</span>{ic?.component !== undefined && <span>分量 <code>{ic.component}</code></span>}</div>}<details className="metrics-disclosure"><summary>全部指标</summary><Metrics metrics={metrics} /></details></>;
}
export function Copy({ value, label = '复制身份' }: { value: string; label?: string }) {
  const [status, setStatus] = useState('');
  return <span className="copy-control"><Button disabled={false} type="text" aria-label={label} icon={<CopyIcon size={14} />} onClick={() => { void navigator.clipboard.writeText(value).then(() => setStatus('已复制'), () => setStatus('未能复制')); }} /><span role="status">{status}</span></span>;
}
export function DraftNotice({ saved }: { saved: boolean }) { return saved ? null : <p className="draft-notice" role="status">草稿未保存，请勿刷新页面</p>; }
export function PendingCommand({ command }: { command: { pending?: { persisted: boolean }; busy: boolean; values?: Record<string, unknown>; canAbandon?: boolean; abandon?: () => void; previousRaw?: string; previousPersisted?: boolean } }) {
  const confirm = useConfirm(); const identity = command.values?.requestId;
  return <>{command.pending && !command.busy && <div className="pending-command" role="status"><div>提交结果待核对{!command.pending.persisted && <span> / 请保留此页面</span>}</div>{typeof identity === 'string' && <details><summary>原始提交身份</summary><code>{identity}</code><Copy value={identity} label="复制原始提交身份" /></details>}{command.canAbandon && <Button disabled={false} onClick={() => confirm('保留原提交并重新编辑？', '不取消原提交；下一次提交使用新身份。', () => command.abandon?.())}>重新编辑</Button>}</div>}{command.previousRaw && <PreviousCommand raw={command.previousRaw} persisted={command.previousPersisted !== false} />}</>;
}
function PreviousCommand({ raw, persisted }: { raw: string; persisted: boolean }) {
  const [download, setDownload] = useState(''); const record = JSON.parse(raw) as Record<string, unknown>;
  useEffect(() => { const url = URL.createObjectURL(new Blob([raw], { type: 'application/json;charset=utf-8' })); setDownload(url); return () => URL.revokeObjectURL(url); }, [raw]);
  const subject = [record.goal, record.name, record.portfolioId].find(value => typeof value === 'string');
  return <details className="previous-command"><summary>上一份原提交</summary><Facts items={[['请求身份', typeof record.requestId === 'string' ? <><code>{record.requestId}</code><Copy value={record.requestId} label="复制上一份请求身份" /></> : '未记录'], ['原提交内容', typeof subject === 'string' ? subject : '未记录']]} />{!persisted && <p className="draft-notice">原提交未保存，请先下载</p>}<Button href={download} download="quazonai-original-command.json" disabled={!download}>下载原提交</Button></details>;
}
export function useConfirm() {
  const { modal } = App.useApp();
  return (title: string, content: string, action: () => void) => modal.confirm({ title, content, okText: title.replace('？', ''), cancelText: '取消', okButtonProps: { danger: true }, onOk: () => { action(); } });
}
