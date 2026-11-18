import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import { useEffect, useState } from 'react';
import { Button, Input, Select, Table, Tabs, Tag } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { ParameterScanTrial, ParameterScanTrials, ParameterScanView, ParameterValues } from '../../../packages/contracts/src/parameter-scans.ts';
import { api } from './api';
import { useParameterScanDetail, useResource } from './data-hooks';
import { ExecutionFailure } from './diagnostics';
import { RecordTitle } from './disclosure';
import { ListTools, useListWorkspace } from './list-workspace';
import { scanNativeMetrics } from './parameter-scan-values';
import { href, navigate, type Route } from './router';
import { Copy, Empty, ErrorNotice, Exact, Facts, Loading, Metrics, Status, Timestamp } from './ui';
import './parameter-scans.css';

const collection = '/parameter-scans';
const active = (value: ParameterScanView) => ['QUEUED', 'RUNNING', 'RECONCILING', 'STOPPING'].includes(value.status);
const labels: Record<string, string> = { STRATEGY: "Strategy account backtest", COMPLETED: "Computation completed", STOPPING: "Stopping", RECONCILING: "Remote receipt unconfirmed", SKIPPED: "Skipped by constraints", PENDING: "Not executed" };
const parameterLabel = (key: string) => key;
function ScanStatus({ status }: { status: string }) {
  useTranslation('core'); return <Status value={status} label={labels[status] ? tr(labels[status]!) : undefined} />; }
function ParameterFacts({ values, copy = false }: { values: ParameterValues; copy?: boolean }) {
  useTranslation('core'); return <div className="scan-parameter-list">{Object.entries(values).map(([key, value]) => <span key={key}>{parameterLabel(key)} <Exact value={value} /></span>)}{copy && <Copy value={JSON.stringify(values, null, 2)} label={tr("Copy parameters")} />}</div>; }
function StaleNotice({ stale, at }: { stale: boolean; at: number }) {
  useTranslation('core'); return stale && at ? <p className="quiet" role="status">{tr("The connection has not updated. The last available records are shown below:")}<Timestamp value={new Date(at).toISOString()} /></p> : null; }

export default function ParameterScans({ route }: { route: Route }) {
  useTranslation('core');
  if (route.creating) return <Empty title={tr("Parameter grids are available in native research")} action={<a href={href('/research/new')}>{tr("New parameter-grid research")}</a>} />;
  return route.id ? <ScanDetail key={route.id} route={route} id={route.id} /> : <ScanList route={route} />;
}

function ScanList({ route }: { route: Route }) {
  useTranslation('core');
  const list = useListWorkspace<ParameterScanView>(route, collection, 'createdAt');
  return <section className="parameter-scans list-workspace" aria-label={tr("Parameter scan list")}>
    <header className="list-heading"><h1>{tr("Historical parameter scans")}</h1><a href={href('/research/new')}>{tr("New parameter-grid research")}</a></header><p>{tr("For new research, QuaZonai generates parameter combinations, Pi orchestrates the work, and Nautilus runs native backtests. The records below are historical only.")}</p>

    <ListTools list={list} searchLabel={tr("Search parameter scans")} placeholder={tr("Search scan names, sources, or datasets")} filters={[{ key: 'status', label: tr("Execution status") }, { key: 'mode', label: tr("Scan type"), options: [{ value: 'STRATEGY', label: tr("Strategy account backtest") }] }, { key: 'datasetId', label: tr("Dataset") }]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} /><StaleNotice stale={list.query.isError} at={list.query.dataUpdatedAt} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<ParameterScanView> {...list.tableProps} rowKey="id" size="small" scroll={{ x: 1150 }} locale={{ emptyText: <Empty title={list.hasFilters ? tr("No matching parameter scans") : tr("No parameter scans yet")} /> }} columns={[
      { title: tr("Scan name"), key: 'name', width: 220, fixed: 'left', ...list.sortColumn('name'), render: (_cell, row) => <a className="list-record-name" {...list.linkProps(row.id)}>{row.name}</a> },
      { title: tr("Type"), width: 135, render: (_cell, row) => tr(labels[row.mode] ?? row.mode) },
      { title: tr("Execution status"), width: 150, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: tr("Dataset"), width: 180, render: (_cell, row) => row.sourceView.datasetName ?? row.sourceView.datasetId ?? tr("Not recorded") },
      { title: tr("Completed / eligible"), width: 160, render: (_cell, row) => <><Exact value={row.counts.completed} /> / <Exact value={row.counts.eligible} /></> },
      { title: tr("Failed / skipped"), width: 125, render: (_cell, row) => <><Exact value={row.counts.failed} /> / <Exact value={row.counts.skipped} /></> },
      { title: tr("Created"), key: 'createdAt', width: 200, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}

function ScanDetail({ route, id }: { route: Route; id: string }) {
  useTranslation('core');
  const cache = useQueryClient(); const query = useParameterScanDetail(id); const value = query.data;
  const [stopping, setStopping] = useState(false); const [error, setError] = useState<unknown>();
  const requestedTab = route.params.get('tab'); const tab = requestedTab && ['overview', 'trials', 'inputs'].includes(requestedTab) ? requestedTab : 'overview';
  const update = (patch: Record<string, string | undefined>) => { const params = new URLSearchParams(route.params); for (const [key, item] of Object.entries(patch)) { if (item === undefined) params.delete(key); else params.set(key, item); } navigate(`${collection}/${encodeURIComponent(id)}?${params}`, true); };
  const stop = async () => { setStopping(true); setError(undefined); try { const result = await api<ParameterScanView>(`${collection}/${encodeURIComponent(id)}/stop`, { method: 'POST' }); cache.setQueryData([`${collection}/${encodeURIComponent(id)}`], result); void cache.invalidateQueries({ queryKey: [collection] }); } catch (failure) { setError(failure); } finally { setStopping(false); } };
  return <div className="parameter-scans"><a className="text-link back-link" href={href(collection)}><ArrowLeftIcon size={14} />{tr("Parameter scan list")}</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{value && <><ScanStatus status={value.status} /><div className="scan-command-actions"><Button onClick={() => { void query.refetch(); }}>{tr("Refresh")}</Button>{active(value) && <Button danger disabled={stopping || value.status === 'STOPPING'} loading={stopping} onClick={() => { void stop(); }}>{value.status === 'STOPPING' ? tr("Stopping") : tr("Stop scan")}</Button>}</div></>}</div><RecordTitle name={value?.name} fallback={tr("Parameter scans")} />{value && <div className="study-context"><span>{tr(labels[value.mode] ?? value.mode)}</span><span>{value.sourceView.datasetName ?? value.sourceView.datasetId}</span><Tag>{tr("Exploratory results; not qualified as research evidence")}</Tag></div>}</header>
    <ErrorNotice error={query.error ?? error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : value && <Tabs className="document-tabs" destroyOnHidden activeKey={tab} onChange={next => update({ tab: next })} items={[
      { key: 'overview', label: tr("Overview"), children: <div className="scan-detail-content"><ScanOverview value={value} /><Button onClick={() => update({ tab: 'trials' })}>{tr("View all parameter results")}</Button></div> },
      { key: 'trials', label: tr("Results table"), children: <div className="scan-detail-content"><ScanTrials scan={value} route={route} update={update} /></div> },
      { key: 'inputs', label: tr("Inputs and execution"), children: <div className="scan-detail-content"><Facts items={[[tr("Scan"), value.id], [tr("Original request"), value.requestId], [tr("Main task"), value.taskId], [tr("Source"), value.sourceView.label], [tr("Source research"), value.sourceView.researchId], [tr("Created"), <Timestamp value={value.createdAt} />], [tr("Updated"), <Timestamp value={value.updatedAt} />], [tr("Concurrent tasks"), value.execution.concurrency]]} />{value.sourceView.assumptions.length > 0 && <section className="result-section"><h3>{tr("Original hypothesis")}</h3><ul>{value.sourceView.assumptions.map((item, index) => <li key={index}>{item}</li>)}</ul></section>}<details><summary>{tr("Original parameter plan")}</summary><Copy value={JSON.stringify(value.input, null, 2)} label={tr("Copy scan plan")} /><pre className="scan-code">{JSON.stringify(value.input, null, 2)}</pre></details><details><summary>{tr("Parameter combinations and fixed values")}</summary><pre className="scan-code">{JSON.stringify(value.grid, null, 2)}</pre></details></div> },
    ]} />}
  </article></div>;
}

function ScanOverview({ value }: { value: ParameterScanView }) {
  useTranslation('core');
  const rows: [string, string][] = [[tr("All combinations"), value.counts.total], [tr("Eligible"), value.counts.eligible], [tr("Completed"), value.counts.completed], [tr("Comparable"), value.counts.comparable], [tr("Running"), value.counts.running], [tr("Not executed"), value.counts.pending], [tr("Failed"), value.counts.failed], [tr("Skipped by constraints"), value.counts.skipped], [tr("Stopped"), value.counts.stopped], [tr("Remote outcome unconfirmed"), value.counts.unknown]];
  return <><dl className="scan-summary">{rows.map(([label, count]) => <div key={label}><dt>{label}</dt><dd><Exact value={count} /></dd></div>)}</dl>
    {active(value) && <div className="scan-notice" role="status"><p>{value.status === 'STOPPING' ? tr("The stop request was recorded. Waiting for the current native execution to end and its remote receipt to be verified.") : value.status === 'RECONCILING' ? tr("The original outcome remains uncertain. The same remote invocation is being checked.") : tr("Running")}</p><p className="quiet">{tr("Updated")} <Timestamp value={value.updatedAt} />{tr(". A stopped local task does not mean the remote execution has stopped.")}</p></div>}
    <ExecutionFailure error={value.error} identity={tr("Parameter scan {{id}} / Main task #{{task}}", { id: value.id, task: value.taskId })} title={tr("Scan did not finish completely")} />

  </>;
}

function ScanTrials({ scan, route, update }: { scan: ParameterScanView; route: Route; update: (patch: Record<string, string | undefined>) => void }) {
  useTranslation('core');
  const page = Math.max(1, Number(route.params.get('trialPage')) || 1); const pageSize = 20; const status = route.params.get('trialStatus') ?? '';
  const params = new URLSearchParams({ page: String(page), pageSize: String(pageSize), sort: 'ordinal', order: 'asc' }); if (status) params.set('status', status);
  const path = `${collection}/${encodeURIComponent(scan.id)}/trials?${params}`;
  const query = useResource<ParameterScanTrials>(path, { poll: active(scan) ? 3000 : undefined });
  const ordinal = route.params.get('trial'); const detail = useResource<ParameterScanTrial>(`${collection}/${encodeURIComponent(scan.id)}/trials/${encodeURIComponent(ordinal ?? '')}`, { enabled: ordinal !== null, poll: active(scan) && ordinal !== null ? 3000 : undefined });
  const [lookup, setLookup] = useState(ordinal ?? ''); const [lookupError, setLookupError] = useState(false);
  useEffect(() => { setLookup(ordinal ?? ''); setLookupError(false); }, [ordinal]);
  return <><div className="scan-toolbar"><label>{tr("Execution status")}<Select aria-label={tr("Filter trial status")} allowClear value={status || undefined} onChange={value => update({ trialStatus: value, trialPage: undefined })} options={['PENDING', 'RUNNING', 'RECONCILING', 'COMPLETED', 'FAILED', 'STOPPED', 'SKIPPED'].map(value => ({ value, label: (labels[value] ? tr(labels[value]!) : undefined) ?? ({ RUNNING: tr("Running"), FAILED: tr("Failed"), STOPPED: tr("Stopped") })[value as 'RUNNING'] ?? value }))} /></label></div>

    <div className="scan-toolbar"><Input.Search aria-label={tr("Look up an exact parameter index")} placeholder={tr("Parameter index (zero-based)")} value={lookup} onChange={event => { setLookup(event.target.value); setLookupError(false); }} enterButton={tr("View parameter point")} onSearch={value => { if (!/^(0|[1-9]\d*)$/.test(value)) { setLookupError(true); return; } update({ trial: value }); }} /></div>
    {lookupError && <p className="scan-subtle-error" role="alert">{tr("Enter a complete non-negative integer index, starting at zero.")}</p>}
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    <Table<ParameterScanTrial> rowKey="ordinal" size="small" loading={query.isPending} dataSource={query.data?.items ?? []} scroll={{ x: 1100 }} pagination={{ current: query.data?.page ?? page, pageSize, total: query.data?.total ?? 0, showSizeChanger: false, onChange: next => update({ trialPage: String(next) }) }} columns={[
      { title: tr("Trial"), width: 90, fixed: 'left', render: (_cell, row) => <button className="text-link" onClick={() => update({ trial: row.ordinal })}>#{row.ordinal}</button> },
      { title: tr("Parameters"), width: 360, render: (_cell, row) => <ParameterFacts values={row.parameters} /> },
      { title: tr("Execution"), width: 140, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: tr("Native objective statistic"), width: 150, render: (_cell, row) => <Exact value={row.metric?.value} unit={row.metric?.unit} missing={tr("Unavailable")} /> },
      { title: tr("Metric availability / execution error"), width: 230, render: (_cell, row) => row.metric?.reason ?? row.error ?? row.skipReason ?? (row.status === 'COMPLETED' && row.metric?.value == null ? tr("Objective metric not provided") : '—') },
      { title: tr("Remote receipt"), width: 140, render: (_cell, row) => row.remoteStatus ?? tr("Not recorded") },
    ]} locale={{ emptyText: <Empty title={tr("No matching parameter results")} /> }} />
    {ordinal !== null && <section className="result-section"><div className="section-heading"><h3>{tr("Trial #")}{ordinal}</h3><Button size="small" onClick={() => update({ trial: undefined })}>{tr("Collapse")}</Button></div><ErrorNotice error={detail.error} retry={() => { void detail.refetch(); }} />{detail.isPending ? <Loading /> : detail.data && <><ParameterFacts values={detail.data.parameters} copy /><Facts items={[[tr("Execution status"), <ScanStatus status={detail.data.status} />], [tr("Scientific task"), detail.data.taskId], [tr("Remote receipt"), detail.data.remoteStatus], [tr("Constraints"), detail.data.skipReason]]} /><ExecutionFailure error={detail.data.error} identity={tr("Parameter scan {{id}} / Trial #{{trial}}", { id: scan.id, trial: detail.data.ordinal })} title={tr("Trial not completed")} /><Metrics metrics={scanNativeMetrics(detail.data.metrics)} />{detail.data.details?.equityPerformance && <details><summary>{tr("Official daily equity returns and annualization basis")}</summary><p>{tr("Annualization is an extrapolation, not an observed full-year return.")}</p><pre className="scan-code">{JSON.stringify(detail.data.details.equityPerformance, null, 2)}</pre></details>}{detail.data.details?.instrumentDefinition && <details><summary>{tr("Original instrument specification evidence")}</summary><pre className="scan-code">{JSON.stringify(detail.data.details.instrumentDefinition, null, 2)}</pre></details>}{detail.data.details?.curveReason === 'native_equity_pages_not_loaded' && <p role="note">{tr("Equity snapshots are saved; the curve has not been loaded.")}</p>}{detail.data.sampleCounts && <details><summary>{tr("Sample and warm-up counts")}</summary><pre className="scan-code">{JSON.stringify(detail.data.sampleCounts, null, 2)}</pre></details>}</>}</section>}
  </>;
}
