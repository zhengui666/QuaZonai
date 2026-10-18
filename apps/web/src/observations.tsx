import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button, Checkbox, Form, Input, Select, Segmented, Table } from 'antd';
import { ArrowClockwiseIcon, ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, ArrowsLeftRightIcon, CaretDownIcon, ClockIcon, PlusIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { ObservationMode, ObservationView, PortfolioView, PositionView, WorkspaceView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, Exact, Facts, Loading, PendingCommand, Status, TableSurface, Timestamp, useConfirm } from './ui';
import { AutomaticControls, CapitalControls } from './account-controls';
const EquityPlot = lazy(() => import('./equity-plot'));
export default function Observations({ route }: { route: Route }) { return route.creating ? <ObservationBuilder route={route} /> : route.id ? <ObservationDetail id={route.id} /> : <ObservationList route={route} />; }
function ObservationList({ route }: { route: Route }) {
  const query = useResource<ObservationView[]>('/observations', { poll: 5000 }); const mode = route.params.get('mode') ?? 'all';
  const values = query.data?.filter(item => mode === 'all' || item.mode === mode);
  return <div className="index-page"><header className="page-heading"><h1>目标与观测</h1><Button type="primary" icon={<PlusIcon size={15} />} onClick={() => navigate('/observations/new')}>新建观测</Button></header><div className="list-toolbar"><Segmented aria-label="观测模式" value={mode} options={[{ value: 'all', label: '全部' }, { value: 'paper', label: '模拟' }, { value: 'live', label: '实盘' }]} onChange={value => navigate(`/observations${value === 'all' ? '' : `?mode=${value}`}`, true)} /></div><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !values?.length ? !query.error && <Empty title={mode === 'live' ? '尚无实盘观测' : mode === 'paper' ? '尚无模拟观测' : '尚无观测'} action={<a className="text-link" href={href('/portfolios')}>查看组合回测 <ArrowUpRightIcon size={14} /></a>} /> : <Table<ObservationView> rowKey="id" pagination={{ pageSize: 20, hideOnSinglePage: true }} scroll={{ x: 700 }} dataSource={values} columns={[
      { title: '名称', render: (_value, row) => <a className="record-link" href={href(`/observations/${encodeURIComponent(row.id)}`)}>{row.name}</a> }, { title: '模式', render: (_value, row) => <span className="mode-label">{row.mode === 'live' ? '实盘' : '模拟'}</span> },
      { title: '目标回执', render: (_value, row) => row.receipt ? <Status plain value={row.receipt.status} /> : <span className="quiet">未提供</span> }, { title: '观测状态', render: (_value, row) => <Status plain value={row.status} /> }, { title: '上次快照', render: (_value, row) => <Timestamp value={row.updatedAt} /> },
    ]} />}
  </div>;
}
function ObservationDetail({ id }: { id: string }) {
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
  const reconcile = ['PENDING', 'UNCERTAIN'].includes(value?.receipt?.status ?? '');
  const canRefresh = reconcile || value?.status !== 'STOPPED' && value?.receipt?.status === 'ACKNOWLEDGED';
  const hasSnapshot = value?.updatedAt != null;
  return <div className="observation-page"><a className="text-link back-link" href={href('/observations')}><ArrowLeftIcon size={14} />目标与观测</a><header className="page-heading"><h1>目标与观测</h1></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : !value ? !query.error && <Empty title="未找到观测记录" /> : <article className="observation-document">
    <header className="observation-heading"><div><div className="record-context"><span className="mode-label">{value.mode === 'live' ? '实盘' : '模拟'}</span><span>{value.connectionName ?? value.connectionId ?? '连接名称未记录'}</span><Status plain value={value.status} /></div><h2>{value.name}</h2><a className="text-link origin-link" href={href(`/portfolios/${encodeURIComponent(value.portfolioId)}`)}>原始组合 <ArrowUpRightIcon size={13} /></a></div>{canRefresh && <Button type="primary" loading={refreshing} disabled={stopping} icon={<ArrowClockwiseIcon size={15} />} onClick={() => { void update('refresh'); }}>{refreshing ? (reconcile ? '核对中' : '更新中') : reconcile ? '核对回执' : '更新快照'}</Button>}</header>
    <ErrorNotice error={error} />{value.error && <div className="recorded-error observation-error" role="alert"><span>最近操作未完成</span><details><summary>技术详情</summary><code>{value.error}</code></details></div>}
    <section className="receipt-section"><div className="section-heading"><h3>目标回执</h3>{value.receipt && <Status value={value.receipt.status} />}</div>{value.receipt ? <><Facts items={[["回执身份", <><code>{value.receipt.id}</code><Copy value={value.receipt.id} label="复制回执身份" /></>], ["到期时间", <Timestamp value={value.receipt.expiresAt} />], ["下游身份", value.receipt.externalId ?? '未提供'], ["接收时间", <Timestamp value={value.receipt.acknowledgedAt} />]]} /><details className="provenance-disclosure"><summary>来源与身份</summary><Facts items={[["连接", value.connectionId ?? '未记录'], ["客户端", value.provenance?.clientId ?? '未记录'], ["账户", value.provenance?.accountId ?? '未记录'], ["快照序列", value.provenance ? <code>{value.provenance.sequence}</code> : '未记录'], ["原生时钟", value.provenance ? <code>{value.provenance.nativeClockNs}</code> : '未记录']]} /></details></> : <p className="quiet">未提供回执</p>}</section>
    <section className="snapshot-section"><div className="section-heading"><h3>账户快照</h3><span className="quiet">{hasSnapshot ? <>上次快照 <Timestamp value={value.updatedAt} /></> : '尚无快照'}</span></div><div className="snapshot-values"><div><span>权益 <code>{value.currency}</code></span><strong><Exact value={value.equity} /></strong></div><div><span>现金 <code>{value.currency}</code></span><strong><Exact value={value.cash} /></strong></div></div>
    {hasSnapshot && value.curve.length ? <Suspense fallback={<Loading />}><EquityPlot points={value.curve} currency={value.currency} /></Suspense> : <div className="snapshot-empty"><ClockIcon size={17} /><span>{hasSnapshot ? '尚无权益曲线' : '尚无账户快照'}</span></div>}</section>
    <section className="positions-section"><div className="section-heading"><h3>持仓</h3></div>{!hasSnapshot ? <p className="quiet">尚无快照</p> : !value.positions.length ? <p className="quiet">无持仓</p> : <Table<PositionView> components={{ table: TableSurface }} rowKey={(row, index) => `${row.instrumentId}/${index}`} pagination={false} size="small" scroll={{ x: 550 }} dataSource={value.positions} columns={[{ title: '标的', dataIndex: 'instrumentId' }, { title: '数量', align: 'right', render: (_cell, row) => <Exact value={row.quantity} /> }, { title: '市值', align: 'right', render: (_cell, row) => <Exact value={row.value} /> }, { title: '盈亏', align: 'right', render: (_cell, row) => <Exact value={row.pnl} /> }]} />}</section>
    <AutomaticControls value={value} /><CapitalControls value={value} />
    <footer className="observation-footer"><span className="quiet">{value.status === 'STOPPED' ? '本地观测已停止' : ''}</span><Button danger loading={stopping} disabled={value.status === 'STOPPED'} onClick={() => confirm('停止观测？', '仅停止本地观测，不撤回下游目标。', () => { void update('stop'); })}>{stopping ? '停止中' : '停止观测'}</Button></footer>
  </article>}</div>;
}
function ObservationBuilder({ route }: { route: Route }) {
  const cache = useQueryClient(); const portfolios = useResource<PortfolioView[]>('/portfolios', { poll: 5000 }); const workspace = useResource<WorkspaceView>('/workspace');
  const [form] = Form.useForm(); const initialized = useRef(false); const [draft] = useState(() => readDraft<Record<string, unknown>>('observation')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('observation'));
  const command = useCommand<ObservationView>('observations', value => { removeDraft('observation'); void cache.invalidateQueries({ queryKey: ['/observations'] }); if (currentRoute().page === 'observations' && currentRoute().creating) navigate(`/observations/${encodeURIComponent(value.id)}`); });
  const origin = route.params.get('portfolio') ? { path: `/portfolios/${encodeURIComponent(route.params.get('portfolio')!)}`, label: '原始组合' } : { path: '/observations', label: '目标与观测' };
  const mode = Form.useWatch('mode', form) as ObservationMode | undefined;
  const automatic = Form.useWatch('automaticEnabled', form) as boolean | undefined;
  useEffect(() => { if (initialized.current || !portfolios.data || !workspace.data) return; initialized.current = true; form.setFieldsValue(command.values ? { ...command.values, automaticEnabled: (command.values.automatic as { enabled: boolean } | undefined)?.enabled, cashBuffer: (command.values.automatic as { cashBuffer: string } | undefined)?.cashBuffer, maximumSnapshotAgeSeconds: (command.values.automatic as { maximumSnapshotAgeSeconds: number } | undefined)?.maximumSnapshotAgeSeconds } : { cashBuffer: '0', maximumSnapshotAgeSeconds: '60', mode: workspace.data.connections.some(item => item.mode === 'paper') ? 'paper' : undefined, ...draft, ...(route.params.get('portfolio') ? { portfolioId: route.params.get('portfolio') } : {}) }); }, [portfolios.data, workspace.data]);
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><ArrowsLeftRightIcon size={24} /></div><h1>新建观测</h1></div><ErrorNotice error={portfolios.error ?? workspace.error} retry={() => { void portfolios.refetch(); void workspace.refetch(); }} />
    {portfolios.isPending || workspace.isPending ? <Loading /> : portfolios.data && workspace.data && <>
    {!workspace.data?.connections.length && <div className="prerequisite">尚无已配置连接 <a href="https://github.com/zhengui666/QuaZonai/blob/dev/apps/research/README.md" target="_blank" rel="noreferrer">配置说明 <ArrowUpRightIcon size={13} /></a></div>}
    <PendingCommand command={command} />
    <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} disabled={!!command.pending || command.busy} onValuesChange={(_changed, values) => { if (!command.pending) setDraftSaved(saveDraft('observation', values)); }} onFinish={(values: { portfolioId: string; mode: ObservationMode; connectionId: string; automaticEnabled?: boolean; cashBuffer: string; maximumSnapshotAgeSeconds: string }) => { void command.submit({ portfolioId: values.portfolioId, mode: values.mode, connectionId: values.connectionId, ...(values.automaticEnabled ? { automatic: { enabled: true, cashBuffer: values.cashBuffer, maximumSnapshotAgeSeconds: Number(values.maximumSnapshotAgeSeconds) } } : {}) }); }}>
      <Form.Item label="组合" name="portfolioId" rules={[{ required: true, message: '请选择有目标的组合' }]}><Select autoFocus showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} options={portfolios.data?.map(item => ({ value: item.id, label: `${item.name}${item.targetExpired ? ' / 目标已过期' : ''}`, disabled: item.status !== 'COMPUTED' || !item.targets.length || item.targetExpired && !automatic }))} /></Form.Item>
      <Form.Item label="模式" name="mode" rules={[{ required: true, message: '请选择模式' }]}><Select suffixIcon={<CaretDownIcon size={14} />} options={(['paper', 'live'] as const).map(value => ({ value, label: value === 'paper' ? '模拟' : '实盘', disabled: !workspace.data?.connections.some(item => item.mode === value) }))} onChange={() => { form.setFieldValue('connectionId', undefined); setDraftSaved(saveDraft('observation', form.getFieldsValue())); }} /></Form.Item>
      <Form.Item label="连接" name="connectionId" rules={[{ required: true, message: '请选择连接' }]}><Select suffixIcon={<CaretDownIcon size={14} />} options={workspace.data?.connections.filter(item => item.mode === mode).map(item => ({ value: item.id, label: item.name }))} /></Form.Item>
      <Form.Item name="automaticEnabled" valuePropName="checked"><Checkbox>按账户净值自动调整</Checkbox></Form.Item>
      {automatic && <><p className="quiet">仅在原选池内缩放；容量、预留、当前账户或执行能力未知时不交付。历史过期目标不续期。</p><div className="form-columns"><Form.Item label="显式现金缓冲" name="cashBuffer" rules={[{ required: true }]}><Input inputMode="decimal" /></Form.Item><Form.Item label="账户快照最大年龄（秒）" name="maximumSnapshotAgeSeconds" rules={[{ required: true }]}><Input inputMode="numeric" /></Form.Item></div></>}
      <p className="quiet">仅交付目标，由下游执行</p><ErrorNotice error={command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>取消</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? '提交中' : '核对提交'}</Button> : <Button type="primary" htmlType="submit" disabled={!workspace.data?.connections.length}>交付并观测 <ArrowRightIcon size={15} /></Button>}</div>
    </Form></>}
  </div>;
}
