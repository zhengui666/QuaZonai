import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button, Form, Select, Segmented, Table } from 'antd';
import { ArrowClockwiseIcon, ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, ArrowsLeftRightIcon, CaretDownIcon, ClockIcon, PlusIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { ObservationMode, ObservationView, PositionView, WorkspaceView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, Exact, Facts, Loading, MetricLabel, PendingCommand, Status, TableSurface, Timestamp, useConfirm } from './ui';
import { RecordTitle, TextDisclosure } from './disclosure';
import { shortTitle } from './titles';
import { CapitalControls } from './account-controls';
import AccountStrategy from './account-strategy';
const EquityPlot = lazy(() => import('./equity-plot'));
export default function Observations({ route }: { route: Route }) {
  useTranslation('core'); return route.creating ? <ObservationBuilder /> : route.id ? <ObservationDetail id={route.id} /> : <ObservationList route={route} />; }
function ObservationList({ route }: { route: Route }) {
  useTranslation('core');
  const query = useResource<ObservationView[]>('/observations', { poll: 5000 }); const mode = route.params.get('mode') ?? 'all';
  const values = query.data?.filter(item => mode === 'all' || item.mode === mode);
  return <div className="index-page"><header className="page-heading"><h1>{tr("Accounts and strategies")}</h1><Button type="primary" icon={<PlusIcon size={15} />} onClick={() => navigate('/observations/new')}>{tr("Link account / new observation")}</Button></header><div className="list-toolbar"><Segmented aria-label={tr("Observation mode")} value={mode} options={[{ value: 'all', label: tr("All") }, { value: 'paper', label: tr("Paper") }, { value: 'live', label: tr("Live") }]} onChange={value => navigate(`/observations${value === 'all' ? '' : `?mode=${value}`}`, true)} /></div><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !values?.length ? !query.error && <Empty title={mode === 'live' ? tr("No live observations yet") : mode === 'paper' ? tr("No paper observations yet") : tr("No observations yet")} action={<a className="text-link" href={href('/portfolios')}>{tr("View portfolio backtests")} <ArrowUpRightIcon size={14} /></a>} /> : <Table<ObservationView> rowKey="id" pagination={{ pageSize: 20, hideOnSinglePage: true }} scroll={{ x: 700 }} dataSource={values} columns={[
      { title: tr("Name"), render: (_value, row) => <><a className="record-link" href={href(`/observations/${encodeURIComponent(row.id)}`)}>{shortTitle(row.name, undefined, tr("Account observations"))}</a>{shortTitle(row.name) !== row.name && <TextDisclosure label={tr("Full observation name")} value={row.name} collapsed />}</> }, { title: tr("Mode"), render: (_value, row) => <span className="mode-label">{row.mode === 'live' ? tr("Live") : tr("Paper")}</span> },
      { title: tr("Target receipt"), render: (_value, row) => row.receipt ? <Status plain value={row.receipt.status} /> : <span className="quiet">{tr("Not provided")}</span> }, { title: tr("Observation status"), render: (_value, row) => <Status plain value={row.status} /> }, { title: tr("Last snapshot"), render: (_value, row) => <Timestamp value={row.updatedAt} /> },
      { title: tr("Account capital"), render: (_value, row) => <a href={href(`/observations/${encodeURIComponent(row.id)}?section=capital`)}>{tr("Withdraw capital")}{row.capitalExit?.exits.length ? tr(" \u00b7 {{count}} plans", { count: row.capitalExit.exits.length }) : ''}</a> },
    ]} />}
  </div>;
}
function ObservationDetail({ id }: { id: string }) {
  useTranslation('core');
  const cache = useQueryClient(); const query = useResource<ObservationView[]>('/observations', { poll: 5000 }); const value = query.data?.find(item => item.id === id);
  const [refreshing, setRefreshing] = useState(false); const [stopping, setStopping] = useState(false); const [error, setError] = useState<unknown>(); const confirm = useConfirm();
  const refreshObserver = useRef<AbortController | undefined>(undefined); const stopInFlight = useRef(false);
  useEffect(() => () => refreshObserver.current?.abort(), [id]);
  const update = async (operation: 'refresh' | 'stop') => {
    if (stopInFlight.current || operation === 'refresh' && refreshObserver.current) return;
    const observer = operation === 'refresh' ? new AbortController() : undefined;
    if (observer) { refreshObserver.current = observer; setRefreshing(true); }
    else { stopInFlight.current = true; setStopping(true); refreshObserver.current?.abort(); }
    setError(undefined);
    try {
      const next = await api<ObservationView>(`/observations/${encodeURIComponent(id)}/${operation}`, { method: 'POST', signal: observer?.signal });
      await cache.cancelQueries({ queryKey: ['/observations'] });
      if (!observer?.signal.aborted) { cache.setQueryData<ObservationView[]>(['/observations'], old => old?.map(row => row.id === id ? next : row)); void cache.invalidateQueries({ queryKey: ['/workspace'] }); }
    } catch (failure) { if (!observer?.signal.aborted) setError(failure); }
    finally { if (observer) { if (refreshObserver.current === observer) { refreshObserver.current = undefined; setRefreshing(false); } } else { stopInFlight.current = false; setStopping(false); } }
  };
  const reconcile = ['PENDING', 'UNCERTAIN'].includes(value?.receipt?.status ?? '') || value?.allocation?.status !== 'SIZED' && ['PENDING', 'UNCERTAIN'].includes(value?.allocation?.receipt?.status ?? '');
  const canRefresh = reconcile || value?.status !== 'STOPPED' && (!!value && !value.portfolioId || value?.receipt?.status === 'ACKNOWLEDGED' || value?.automaticPolicy?.enabled === false);
  const hasSnapshot = value?.updatedAt != null;
  return <div className="observation-page"><a className="text-link back-link" href={href('/observations')}><ArrowLeftIcon size={14} />{tr("Accounts and strategies")}</a><header className="page-heading"><h1>{tr("Accounts and strategies")}</h1></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : !value ? !query.error && <Empty title={tr("Observation not found")} /> : <article className="observation-document">
    <header className="observation-heading"><div><div className="record-context"><span className="mode-label">{value.mode === 'live' ? tr("Live") : tr("Paper")}</span><span>{value.connectionName ?? value.connectionId ?? tr("Connection name not recorded")}</span><Status plain value={value.status} /></div><RecordTitle as="h2" name={value.name} fallback={tr("Account observations")} />{value.portfolioId && <a className="text-link origin-link" href={href(`/portfolios/${encodeURIComponent(value.portfolioId)}`)}>{tr("Original portfolio")} <ArrowUpRightIcon size={13} /></a>}</div>{canRefresh && <Button type="primary" loading={refreshing} disabled={stopping} icon={<ArrowClockwiseIcon size={15} />} onClick={() => { void update('refresh'); }}>{refreshing ? (reconcile ? tr("Checking") : tr("Updating")) : reconcile ? tr("Check receipt") : tr("Update snapshot")}</Button>}</header>
    <ErrorNotice error={error} />{value.error && <div className="recorded-error observation-error" role="alert"><span>{tr("The latest operation did not complete")}</span><details><summary>{tr("Technical details")}</summary><code>{value.error}</code></details></div>}
    {value.receipt && <section className="receipt-section"><div className="section-heading"><h3>{tr("Historical target receipt")}</h3>{value.receipt && <Status value={value.receipt.status} />}</div>{value.receipt ? <><Facts items={[[tr("Receipt ID"), <><code>{value.receipt.id}</code><Copy value={value.receipt.id} label={tr("Copy receipt ID")} /></>], [tr("Expires"), <Timestamp value={value.receipt.expiresAt} />], [tr("Downstream ID"), value.receipt.externalId ?? tr("Not provided")], [tr("Received"), <Timestamp value={value.receipt.acknowledgedAt} />]]} /><details className="provenance-disclosure"><summary>{tr("Source and identity")}</summary><Facts items={[[tr("Connection"), value.connectionId ?? tr("Not recorded")], [tr("Client"), value.provenance?.clientId ?? tr("Not recorded")], [tr("Account"), value.provenance?.accountId ?? tr("Not recorded")], [tr("Snapshot sequence"), value.provenance ? <code>{value.provenance.sequence}</code> : tr("Not recorded")], [tr("Native clock"), value.provenance ? <code>{value.provenance.nativeClockNs}</code> : tr("Not recorded")]]} /></details></> : <p className="quiet">{tr("No receipt provided")}</p>}</section>}
    <section className="snapshot-section"><div className="section-heading"><h3>{tr("Account snapshot")}</h3><span className="quiet">{hasSnapshot ? <>{tr("Last snapshot")} <Timestamp value={value.updatedAt} /></> : tr("No snapshots yet")}</span></div><div className="snapshot-values"><div><span><MetricLabel metricKey="equity" /> <code>{value.currency}</code></span><strong><Exact value={value.equity} /></strong></div><div><span><MetricLabel metricKey="cash" /> <code>{value.currency}</code></span><strong><Exact value={value.cash} /></strong></div></div>
    {hasSnapshot && value.curve.length ? <Suspense fallback={<Loading />}><EquityPlot points={value.curve} currency={value.currency} /></Suspense> : <div className="snapshot-empty"><ClockIcon size={17} /><span>{hasSnapshot ? tr("No equity curve yet") : tr("No account snapshot yet")}</span></div>}</section>
    <section className="positions-section"><div className="section-heading"><h3>{tr("Positions")}</h3></div>{!hasSnapshot ? <p className="quiet">{tr("No snapshots yet")}</p> : !value.positions.length ? <p className="quiet">{tr("No positions")}</p> : <Table<PositionView> components={{ table: TableSurface }} rowKey={(row, index) => `${row.instrumentId}/${index}`} pagination={false} size="small" scroll={{ x: 550 }} dataSource={value.positions} columns={[{ title: tr("Instrument"), dataIndex: 'instrumentId' }, { title: <MetricLabel metricKey="quantity" />, align: 'right', render: (_cell, row) => <Exact value={row.quantity} /> }, { title: <MetricLabel metricKey="positionValue" />, align: 'right', render: (_cell, row) => <Exact value={row.value} /> }, { title: <MetricLabel metricKey="pnl" />, align: 'right', render: (_cell, row) => <Exact value={row.pnl} /> }]} />}</section>
    <AccountStrategy key={value.id} value={value} /><CapitalControls key={value.id} value={value} />
    <footer className="observation-footer"><span className="quiet">{value.status === 'STOPPED' ? tr("Local observation stopped") : ''}</span><Button danger loading={stopping} disabled={value.status === 'STOPPED'} onClick={() => confirm(tr("Stop observation?"), tr("Stops only local observation. It does not stop the native strategy, cancel orders, or close positions."), () => { void update('stop'); })}>{stopping ? tr("Stopping") : tr("Stop observation")}</Button></footer>
  </article>}</div>;
}
function ObservationBuilder() {
  useTranslation('core');
  const cache = useQueryClient(); const workspace = useResource<WorkspaceView>('/workspace');
  const [form] = Form.useForm(); const initialized = useRef(false); const [draft] = useState(() => readDraft<Record<string, unknown>>('observation')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('observation'));
  const command = useCommand<ObservationView>('observations', value => { removeDraft('observation'); void cache.invalidateQueries({ queryKey: ['/observations'] }); if (currentRoute().page === 'observations' && currentRoute().creating) navigate(`/observations/${encodeURIComponent(value.id)}${value.portfolioId ? "" : "?section=capital"}`); });
  const origin = { path: '/observations', label: tr("Accounts and strategies") };
  const mode = Form.useWatch('mode', form) as ObservationMode | undefined;
  useEffect(() => { if (initialized.current || !workspace.data) return; initialized.current = true;
    const saved = command.values ?? draft;
    form.setFieldsValue({ mode: saved?.mode ?? (workspace.data.connections.some(item => item.mode === 'paper') ? 'paper' : undefined), connectionId: saved?.connectionId });
  }, [workspace.data]);
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><ArrowsLeftRightIcon size={24} /></div><h1>{tr("Link an existing account")}</h1></div><ErrorNotice error={workspace.error} retry={() => { void workspace.refetch(); }} />
    {workspace.isPending ? <Loading /> : workspace.data && <>
    {!workspace.data?.connections.length && <div className="prerequisite">{tr("No connections configured")} <a href={href('/settings?section=instances')}>{tr("Configure an instance connection")} <ArrowUpRightIcon size={13} /></a></div>}
    <PendingCommand command={command} />
    <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} disabled={!!command.pending || command.busy} onValuesChange={(_changed, values) => { if (!command.pending) setDraftSaved(saveDraft('observation', values)); }} onFinish={(values: { mode: ObservationMode; connectionId: string }) => { void command.submit({ purpose: 'ACCOUNT_ONLY', mode: values.mode, connectionId: values.connectionId }); }}>

      <Form.Item label={tr("Mode")} name="mode" rules={[{ required: true, message: tr("Select a mode") }]}><Select suffixIcon={<CaretDownIcon size={14} />} options={(['paper', 'live'] as const).map(value => ({ value, label: value === 'paper' ? tr("Paper") : tr("Live"), disabled: !workspace.data?.connections.some(item => item.mode === value) }))} onChange={() => { form.setFieldValue('connectionId', undefined); setDraftSaved(saveDraft('observation', form.getFieldsValue())); }} /></Form.Item>
      <Form.Item label={tr("Connection")} name="connectionId" rules={[{ required: true, message: tr("Select a connection") }]}><Select suffixIcon={<CaretDownIcon size={14} />} options={workspace.data?.connections.filter(item => item.mode === mode).map(item => ({ value: item.id, label: item.name }))} /></Form.Item>
      <ErrorNotice error={command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>{tr("Cancel")}</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? tr("Submitting") : tr("Check submission")}</Button> : <Button type="primary" htmlType="submit" disabled={!workspace.data?.connections.length}>{tr("Link account")} <ArrowRightIcon size={15} /></Button>}</div>
    </Form></>}
  </div>;
}
