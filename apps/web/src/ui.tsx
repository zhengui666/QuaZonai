import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import { App, Button, Skeleton, Table } from 'antd';
import { CheckCircleIcon, ClockIcon, CopyIcon, FlaskIcon, InfoIcon, WarningCircleIcon } from '@phosphor-icons/react';
import { useEffect, useState, type ReactNode, type TableHTMLAttributes } from 'react';
import type { Metric } from '../../../packages/contracts/src/index.ts';
import { ApiError, shortError } from './api';
import { metricDefinition, partitionLabel } from './metric-glossary';
import { FactLabel, MetricLabel } from './metric-label';
export { MetricLabel } from './metric-label';

const labels: Record<string, string> = {
  REVIEW_BLOCKED: "Review blocked",
  PLANNED: "Planned", SEARCHING: "Researching", CLOSING: "Preparing conclusions", EVALUATING_INDEPENDENT: "Independent evaluation in progress", CONCLUDED: "Concluded", PAUSED: "Paused",
  CAPTURING: "Gathering evidence", COMPUTING: "Computing", EVALUATING: "Comparing decisions", SUPERSEDED: "Superseded by newer evidence", WAITING_EVIDENCE: "Awaiting evidence",
  AVAILABLE: "Evidence available", INSUFFICIENT: "Insufficient evidence", UNAVAILABLE: "Not yet available", STALE: "Evidence is out of date", INCOMPARABLE: "Not comparable", AMBIGUOUS: "Ambiguous evidence", INVALID: "Invalid evidence",
  IDEA: "Economic hypothesis", RESEARCH: "Research", SELECTION: "Candidate selection", INDEPENDENT_EVALUATION: "Independent evaluation", SHADOW: "Shadow observation", PAPER: "Paper trading", TARGET_OPERATION: "Target operation", MONITORING: "Monitoring", RETIRED: "Retired",
  READY: "Evidence ready for this stage", NOT_READY: "Requirements not met", NEEDS_EVIDENCE: "More evidence needed", CURRENT: "Current version", OPEN: "Open", IN_PROGRESS: "Collecting additional evidence", RESOLVED: "Resolved", UNRESOLVABLE: "Currently unresolvable",
  RUNNING: "Researching", IDLE: "Idle", STARTED: "Computing", COMPUTED: "Computed", FAILED: "Failed", STOPPED: "Stopped", REJECTED: "Rejected",
  QUEUED: "Queued", OBSERVING: "Observing", UNCERTAIN: "Unconfirmed", PENDING: "Pending", ACKNOWLEDGED: "Acknowledged", EXPIRED: "Expired", REVOKED: "Revoked", NOT_OFFERED: "Not delivered",
  loaded: "Loaded", disabled: "Disabled", pending_restart: "Restart required", error: "Failed to load", running: "Running", pending: "Queued", waiting: "Waiting", completing: "Finishing",
  terminal: "Ended", completed: "Completed", failed: "Failed", aborted: "Aborted", faulted: "Execution error", done: "Answered", queued: "Pending processing", placed: "Submitted for execution", unanswered: "Unanswered", cancelled: "Cancelled",
};
export function TableSurface(props: TableHTMLAttributes<HTMLTableElement>) {
  useTranslation('core'); return <table {...props} tabIndex={0} />; }
export function stateLabel(value: string) { return labels[value] ? tr(labels[value]!) : value; }
export function Status({ value, plain = false, label }: { value: string; plain?: boolean; label?: string }) {
  useTranslation('core');
  const tone = /FAILED|REJECTED|failed|faulted|^error$/.test(value) ? 'error' : /UNCERTAIN|pending_restart|unanswered/.test(value) ? 'warning' : /RUNNING|STARTED|running/.test(value) ? 'active' : 'neutral';
  const Icon = tone === 'error' || tone === 'warning' ? WarningCircleIcon : /RUNNING|STARTED|QUEUED|PENDING|running|pending|queued|waiting|completing/.test(value) ? ClockIcon : /COMPUTED|ACKNOWLEDGED|^loaded$|^completed$|^done$|^configured$|^ready$/.test(value) ? CheckCircleIcon : InfoIcon;
  return <span className={`status ${tone} ${plain ? 'plain' : ''}`}><Icon size={14} />{label ?? stateLabel(value)}</span>;
}
export function Loading({ rows = 3 }: { rows?: number }) {
  useTranslation('core'); return <div className="loading-state" role="status" aria-label={tr("Loading")}><Skeleton active={false} title paragraph={{ rows }} /></div>; }
export function ErrorNotice({ error, retry }: { error: unknown; retry?: () => void }) {
  useTranslation('core');
  if (!error) return null;
  return <div className="error-notice" role="alert"><div className="error-heading"><WarningCircleIcon size={17} /><span>{shortError(error)}</span>{retry && <Button size="small" onClick={retry}>{tr("Retry")}</Button>}</div>{error instanceof ApiError && error.code && <details><summary>{tr("Technical details")}</summary><code>{error.code}</code></details>}</div>;
}
export function Empty({ title, action, children }: { title: string; action?: ReactNode; children?: ReactNode }) {
  useTranslation('core'); return <div className="empty"><div className="empty-icon"><FlaskIcon size={24} weight="duotone" /></div><h3>{title}</h3>{children}{action}</div>; }
export function Facts({ items }: { items: [ReactNode, ReactNode][] }) {
  useTranslation('core'); return <dl className="facts">{items.map(([label, value], index) => <div key={typeof label === 'string' ? label : index}><dt>{typeof label === 'string' ? <FactLabel label={label} /> : label}</dt><dd>{value ?? tr("Not provided")}</dd></div>)}</dl>; }
export function Timestamp({ value }: { value: string | null | undefined }) {
  useTranslation('core'); return value ? <time dateTime={value}>{value.replace('T', ' ').replace(/Z$/, ' UTC')}</time> : <span className="quiet">{tr("Not recorded")}</span>; }
export function exact(value: number | string | null | undefined, missing = tr("Not provided")) { return value == null ? missing : String(value); }
export function Exact({ value, unit, missing }: { value: number | string | null | undefined; unit?: string; missing?: string }) {
  useTranslation('core'); return <span className="exact-value">{exact(value, missing)}{value != null && unit && <span className="value-unit"> {unit}</span>}</span>; }
const availability: Record<string, string> = { INSUFFICIENT_SAMPLES: "Insufficient samples", CONSTANT_SCORE: "Constant score", CONSTANT_LABEL: "Constant label", NONFINITE_DATA: "Non-finite data", NONFINITE_RESULT: "Non-finite result", AVAILABLE: "Available" };
export function metricStatus(metric: Metric) { return metric.status ? availability[metric.status] ? tr(availability[metric.status]!) : tr("Unknown availability") : metric.value == null ? tr("Not provided") : ''; }
export function metricLabel(metric: Metric) { return metricDefinition(metric.key).label; }
export function MetricField({ metric }: { metric: Metric }) {
  useTranslation('core');
  return <div className="metric-name"><MetricLabel metricKey={metric.key} />{metric.partition && <small>{partitionLabel(metric.partition)}{metric.component !== undefined && tr(' / Component {{component}}', { component: metric.component })}</small>}<details className="metric-technical"><summary>{tr("Raw fields")}</summary><code>{metric.key}</code>{metric.label !== metric.key && <span>{metric.label}</span>}{metric.partition && <code>{metric.partition}</code>}{metric.status && <code>{metric.status}</code>}</details></div>;
}
export function Metrics({ metrics }: { metrics: Metric[] }) {
  useTranslation('core');
  if (!metrics.length) return <p className="quiet">{tr("No metrics provided")}</p>;
  return <Table<Metric> className="metric-table" rowKey="key" size="small" pagination={false} dataSource={metrics} columns={[
    { title: tr("Metric"), render: (_value, metric) => <MetricField metric={metric} /> },
    { title: tr("Value"), align: 'right', render: (_value, metric) => <Exact value={metric.value} unit={metric.unit} missing={tr("Unavailable")} /> },
    { title: tr("Availability"), render: (_value, metric) => <span className="quiet">{metricStatus(metric)}</span> },
  ]} />;
}
export function EvidenceMetrics({ metrics }: { metrics: Metric[] }) {
  useTranslation('core');
  const independent = metrics.filter(metric => metric.partition === 'HELDOUT' || metric.key.startsWith('HELDOUT.'));
  const source = independent.length ? independent : metrics.some(metric => metric.partition || /^(TRAIN|VALIDATION|HELDOUT)\./.test(metric.key)) ? [] : metrics;
  const ic = source.find(metric => /(?:pearson_ic|^ic)$/.test(metric.key));
  const included = source.find(metric => metric.key.endsWith('counts.included'));
  const excluded = source.find(metric => metric.key.endsWith('counts.excluded'));
  const counters = [included, excluded].filter((item): item is Metric => !!item);
  const longCounts = counters.some(metric => String(metric.value).length > 9);
  if (!ic && !included && !excluded) return <Metrics metrics={metrics} />;
  return <><div className={`metric-grid${longCounts ? ' long-counts' : ''}`} data-columns={counters.length + Number(!!ic)}>
    {ic && <div className="metric-primary"><span className="metric-label"><MetricLabel metricKey={ic.key} label={independent.length ? tr("Held-out information coefficient (IC)") : undefined} /></span><strong><Exact value={ic.value} unit={ic.unit} missing={tr("Unavailable")} /></strong>{metricStatus(ic) && <span className="metric-reason"><InfoIcon size={13} />{metricStatus(ic)}</span>}</div>}
    {counters.map(metric => <div className="metric-count" key={metric.key}><span className="metric-label"><MetricLabel metricKey={metric.key} /></span><strong><Exact value={metric.value} missing={tr("Unavailable")} /></strong></div>)}
  </div>{independent.length > 0 && <div className="partition-row"><span className="partition-label">{tr("Held-out sample")}</span>{ic?.component !== undefined && <span>{tr("Component")} <code>{ic.component}</code></span>}</div>}<details className="metrics-disclosure"><summary>{tr("All metrics")}</summary><Metrics metrics={metrics} /></details></>;
}
export function Copy({ value, label = tr("Copy ID") }: { value: string; label?: string }) {
  useTranslation('core');
  const [status, setStatus] = useState('');
  return <span className="copy-control"><Button disabled={false} type="text" aria-label={label} icon={<CopyIcon size={14} />} onClick={() => { void navigator.clipboard.writeText(value).then(() => setStatus(tr("Copied")), () => setStatus(tr("Could not copy"))); }} /><span role="status">{status}</span></span>;
}
export function DraftNotice({ saved }: { saved: boolean }) {
  useTranslation('core'); return saved ? null : <p className="draft-notice" role="status">{tr("Draft not saved. Keep this page open.")}</p>; }
export function PendingCommand({ command }: { command: { pending?: { persisted: boolean }; busy: boolean; values?: Record<string, unknown>; canAbandon?: boolean; abandon?: () => void; previousRaw?: string; previousPersisted?: boolean } }) {
  useTranslation('core');
  const confirm = useConfirm(); const identity = command.values?.requestId;
  return <>{command.pending && !command.busy && <div className="pending-command" role="status"><div>{tr("Submission outcome is unconfirmed")}{!command.pending.persisted && <span>  {tr("/ Keep this page open")}</span>}</div>{typeof identity === 'string' && <details><summary>{tr("Original submission ID")}</summary><code>{identity}</code><Copy value={identity} label={tr("Copy original submission ID")} /></details>}{command.canAbandon && <Button disabled={false} onClick={() => confirm(tr("Keep the original submission and edit again?"), tr("The original submission will not be cancelled. Your next submission will have a new ID."), () => command.abandon?.())}>{tr("Edit again")}</Button>}</div>}{command.previousRaw && <PreviousCommand raw={command.previousRaw} persisted={command.previousPersisted !== false} />}</>;
}
function PreviousCommand({ raw, persisted }: { raw: string; persisted: boolean }) {
  useTranslation('core');
  const [download, setDownload] = useState(''); const record = JSON.parse(raw) as Record<string, unknown>;
  useEffect(() => { const url = URL.createObjectURL(new Blob([raw], { type: 'application/json;charset=utf-8' })); setDownload(url); return () => URL.revokeObjectURL(url); }, [raw]);
  const subject = [record.goal, record.name, record.portfolioId].find(value => typeof value === 'string');
  return <details className="previous-command"><summary>{tr("Previous original submission")}</summary><Facts items={[[tr("Request ID"), typeof record.requestId === 'string' ? <><code>{record.requestId}</code><Copy value={record.requestId} label={tr("Copy previous request ID")} /></> : tr("Not recorded")], [tr("Original submission"), typeof subject === 'string' ? subject : tr("Not recorded")]]} />{!persisted && <p className="draft-notice">{tr("The original submission was not saved. Download it first.")}</p>}<Button href={download} download="quazonai-original-command.json" disabled={!download}>{tr("Download original submission")}</Button></details>;
}
export function useConfirm() {
  const { modal } = App.useApp();
  return (title: string, content: string, action: () => void) => modal.confirm({ title, content, okText: title.replace(/[？?]$/, ''), cancelText: tr("Cancel"), okButtonProps: { danger: true }, onOk: () => { action(); } });
}
