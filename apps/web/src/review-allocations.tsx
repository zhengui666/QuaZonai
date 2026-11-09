import { useEffect, useRef, useState } from 'react';
import { Button, Form, Input, Select, Table } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { DatasetView, PortfolioView } from '../../../packages/contracts/src/index.ts';
import type { ReviewAllocationView } from '../../../packages/contracts/src/review.ts';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { ListTools, listPathFor, useListWorkspace } from './list-workspace';
import type { AllocationListItem } from '../../../packages/contracts/src/lists.ts';
import { DraftNotice, ErrorNotice, Exact, Facts, Loading, PendingCommand, Status, Timestamp } from './ui';
import { ReviewReportPanel } from './review-report';
import { allocationInput, portfolioAllocationEligible, type AllocationFields } from './review-allocation-input';
import { shortTitle } from './titles';
import { RecordTitle } from './disclosure';

/** Optional consumer on the existing portfolio page; only frozen source identities leave this form. */
export function PortfolioAllocations({ route = { page: 'portfolios', creating: false, params: new URLSearchParams({ view: 'allocations' }) }, portfolios: suppliedPortfolios }: { route?: Route; portfolios?: PortfolioView[] }) {
  const cache = useQueryClient(); const list = useListWorkspace<AllocationListItem>(route, '/review-allocations', 'createdAt');
  const [open, setOpen] = useState(false);
  const candidates = useResource<PortfolioView[]>('/portfolios', { enabled: open && !suppliedPortfolios, poll: open ? 5000 : undefined });
  const portfolios = suppliedPortfolios ?? candidates.data ?? [];
  const candidatesLoaded = suppliedPortfolios !== undefined || candidates.data !== undefined;
  const datasets = useResource<DatasetView[]>('/datasets', { enabled: open });
  const [form] = Form.useForm<AllocationFields>(); const initialized = useRef(false);
  const [saved, setSaved] = useState(() => draftIsPersisted('review-allocation')); const [validationError, setValidationError] = useState<string>();
  const command = useCommand<ReviewAllocationView>('review-allocations', value => {
    removeDraft('review-allocation'); setOpen(false); initialized.current = false;
    cache.setQueryData([`/review-allocations/${encodeURIComponent(value.id)}`], value);
    void cache.invalidateQueries({ queryKey: ['/review-allocations'] });
    const active = currentRoute();
    if (active.page === 'portfolios' && active.params.get('view') === 'allocations' && !active.id && !active.creating && !active.unknown) navigate(list.detailPath(value.id, { view: 'allocations' }));
  });
  const ids = Form.useWatch('portfolioIds', form) as string[] | undefined;
  const eligible = portfolios.filter(portfolioAllocationEligible);
  const options = portfolios.map(portfolio => ({ value: portfolio.id, label: `${shortTitle(portfolio.name, undefined, '组合')} / ${portfolio.currency}`, disabled: !portfolioAllocationEligible(portfolio) }));
  useEffect(() => {
    if (!open || initialized.current) return;
    initialized.current = true;
    if (command.values) {
      const original = command.values;
      form.setFieldsValue({ ...original,
        memberCaps: Object.fromEntries((original.memberCaps as { id: string; maximumWeight: string }[]).map(row => [row.id, row.maximumWeight])),
        currentWeights: Object.fromEntries((original.currentWeights as { id: string; weight: string }[]).map(row => [row.id, row.weight])),
        clusters: original.clusterCaps as AllocationFields['clusters'],
      } as AllocationFields);
    } else form.setFieldsValue(readDraft<AllocationFields>('review-allocation') ?? { clusters: [{ id: '', members: [], maximumWeight: '' }] });
  }, [open, form, command.values]);
  return <section className="review-allocation-section" aria-label="多组合资金配置">
    <div className="section-heading"><h2>多组合资金配置</h2><Button disabled={candidatesLoaded && eligible.length < 2 && !command.pending} onClick={() => setOpen(value => !value)}>{open ? '收起配置' : command.pending ? '核对原配置提交' : '配置多组合资金'}</Button></div>
    {candidatesLoaded && eligible.length < 2 && <p className="quiet" role="status">至少需要两个已计算且通过独立评审的组合。当前有 {eligible.length} 个；请先补齐各组合的评审证据。</p>}
    <p className="quiet">使用相同币种、数据集和原始本金的组合，以质量和风险分散生成资金权重，再验证合并资产目标与共享账户结果。</p>
    {open && <><ErrorNotice error={candidates.error} retry={() => { void candidates.refetch(); }} />{candidates.isPending && !suppliedPortfolios ? <Loading /> : <Form className="review-allocation-form" form={form} layout="vertical" disabled={!!command.pending || command.busy} scrollToFirstError={{ focus: true }} onValuesChange={(_changed, values) => { setValidationError(undefined); if (!command.pending) setSaved(saveDraft('review-allocation', values)); }} onFinish={values => {
      try { const input = allocationInput(values, portfolios); setValidationError(undefined); void command.submit(input); }
      catch (error) { setValidationError(error instanceof Error ? error.message : '配置约束无效'); }
    }}>
      <PendingCommand command={command} />
      <Form.Item label="配置名称" name="name" rules={[{ required: true, whitespace: true, message: '请填写配置名称' }]}><Input /></Form.Item>
      <Form.Item label="来源组合（至少两个）" name="portfolioIds" rules={[{ required: true, type: 'array', min: 2, message: '请选择至少两个已通过评审的组合' }]}><Select mode="multiple" showSearch optionFilterProp="label" options={options} /></Form.Item>
      <Form.Item label="独立验证数据集（可选）" name="independentDatasetId" extra="请选择未用于本次调权的同币种数据。留空仅进行验证分区研究，不获得独立通过；服务端会检查根血缘与读取历史。"><Select allowClear loading={datasets.isPending} placeholder="仅验证分区研究" options={(datasets.data ?? []).map(dataset => ({ value: dataset.id, label: `${dataset.name} / ${dataset.currency}`, disabled: !dataset.ready || !!ids?.length && dataset.currency !== portfolios.find(row => row.id === ids[0])?.currency }))} /></Form.Item>
      <ErrorNotice error={datasets.error} retry={() => { void datasets.refetch(); }} />
      {(ids ?? []).map(id => <div className="review-allocation-member" key={id}><h3>{shortTitle(portfolios.find(row => row.id === id)?.name, undefined, '所选组合')}</h3><div className="form-columns">
        <Form.Item label="单组合权重上限（0–1）" name={['memberCaps', id]} rules={[{ required: true, message: '请明确填写上限' }]}><Input inputMode="decimal" /></Form.Item>
        <Form.Item label="当前资金权重（0–1）" name={['currentWeights', id]} extra="从全现金开始时，请明确填 0。" rules={[{ required: true, message: '请明确填写当前权重' }]}><Input inputMode="decimal" /></Form.Item>
      </div></div>)}
      <h3>相关簇约束</h3><p className="quiet">至少明确一组相关组合及其合计上限。若只限制全部所选组合，可将它们归为一组并明确填写上限 1。</p>
      <Form.List name="clusters">{(fields, { add, remove }) => <>{fields.map(field => <div className="review-allocation-cluster" key={field.key}>
        <Form.Item label="簇名称" name={[field.name, 'id']} rules={[{ required: true, whitespace: true, message: '请填写簇名称' }]}><Input /></Form.Item>
        <Form.Item label="簇成员" name={[field.name, 'members']} rules={[{ required: true, type: 'array', min: 2, message: '请至少选择两个所选组合' }]}><Select mode="multiple" options={options.filter(option => ids?.includes(option.value))} /></Form.Item>
        <Form.Item label="簇合计上限（0–1）" name={[field.name, 'maximumWeight']} rules={[{ required: true, message: '请明确填写相关簇上限' }]}><Input inputMode="decimal" /></Form.Item>
        <Button type="text" danger onClick={() => remove(field.name)}>移除此簇</Button>
      </div>)}<Button type="dashed" onClick={() => add({ id: '', members: [], maximumWeight: '' })}>增加相关簇</Button></>}</Form.List>
      <div className="form-columns review-allocation-limits"><Form.Item label="最大换手（0–2）" name="maximumTurnover" extra="各组合新旧资金权重绝对变化之和；1 = 100%。" rules={[{ required: true, message: '请填写最大换手' }]}><Input inputMode="decimal" /></Form.Item></div>
      {validationError && <p className="error-text" role="alert">{validationError}</p>}<ErrorNotice error={command.error} /><DraftNotice saved={saved || !!command.pending} />
      <div className="form-actions">{command.pending ? <Button type="primary" disabled={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>核对原提交</Button> : <Button type="primary" htmlType="submit" loading={command.busy} disabled={eligible.length < 2}>计算资金配置</Button>}</div>
    </Form>}</>}
    <ListTools list={list} searchLabel="搜索资金配置" placeholder="搜索名称或来源组合" filters={[
      { key: 'status', label: '执行状态' }, { key: 'reviewDecision', label: '评审结论' },
      { key: 'createdFrom', label: '创建日期起', kind: 'date', advanced: true }, { key: 'createdTo', label: '创建日期止', kind: 'date', advanced: true },
      { key: 'source', label: '来源组合', advanced: true },
    ]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<AllocationListItem> {...list.tableProps} rowKey="id" size="small" dataSource={list.items} scroll={{ x: 780 }} locale={{ emptyText: list.hasFilters ? '没有匹配的资金配置' : '尚无多组合资金配置记录' }} columns={[
      { title: '配置名称', key: 'name', width: 240, fixed: 'left', ellipsis: true, ...list.sortColumn('name'), render: (_cell, row) => <a className="record-link" {...list.linkProps(row.id, { view: 'allocations' })}>{shortTitle(row.name, undefined, '资金配置')}</a> },
      { title: '执行状态', width: 120, render: (_cell, row) => <Status plain value={row.status} /> },
      { title: '评审结论', width: 120, render: (_cell, row) => row.reviewDecision ? ({ PASS: '通过评审', REJECT: '不予放行', INCONCLUSIVE: '证据不足' }[row.reviewDecision] ?? '未识别结论') : '未评审' },
      { title: '来源组合数', key: 'memberCount', width: 110, align: 'right', ...list.sortColumn('memberCount'), dataIndex: 'memberCount' },
      { title: '创建时间', key: 'createdAt', width: 205, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}

  </section>;
}

export function AllocationDetail({ id, route }: { id: string; route: Route }) {
  const query = useResource<ReviewAllocationView>(`/review-allocations/${encodeURIComponent(id)}`, { poll: 3000 });
  return <div className="portfolio-result"><a className="text-link back-link" href={href(listPathFor(route))}><ArrowLeftIcon size={14} />多组合资金配置列表</a>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : query.data && <div className="study-document"><div className="result-content"><AllocationResult value={query.data} /></div></div>}
  </div>;
}

export function AllocationResult({ value }: { value: ReviewAllocationView }) {
  return <article><RecordTitle name={value.name} fallback="资金配置" /><Status value={value.status} /><Facts items={[["配置身份", <code>{value.id}</code>], ["计算任务", value.taskId ?? '未记录'], ["已使用科学调用", value.consumedScientificCalls], ["创建时间", <Timestamp value={value.createdAt} />]]} />
    <p className="quiet">来源组合 {value.portfolioIds.map(id => <a key={id} href={href(`/portfolios/${encodeURIComponent(id)}`)}>{id} </a>)}</p>
    {value.error && <ErrorNotice error={new Error(value.error)} />}
    {value.weights && <section><h4>候选资金权重</h4><p>{value.weights.status === 'PROPOSED' ? '已产生候选权重，按新候选及整体评审结果继续。' : '当前无法生成配置权重。'}</p>{value.weights.reason && <p>{value.weights.reason}</p>}{!!value.weights.members.length && <Table rowKey="id" size="small" pagination={false} dataSource={value.weights.members} columns={[{ title: '组合身份', dataIndex: 'id' }, { title: '资金权重（1 = 100%）', align: 'right', render: (_cell, row) => <Exact value={row.weight} /> }]} />}<Facts items={[["新候选", value.weights.candidateId ?? '尚未创建'], ["报告来源", value.weights.sourceReportIds.join(' / ') || '未记录']]} /></section>}
    <ReviewReportPanel report={value.review} />
  </article>;
}
