import { Alert, App, Button, Checkbox, Collapse, Descriptions, Drawer, Form, Input, Modal, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { uuidPattern } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

type Report = Schema['HistoricalImportReportV1'];
type Import = Schema['HistoricalImportRequestV1'];
export function MigrationManagement() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [creating, setCreating] = useState(false); const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['migration-reports', history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/migrations/reports', { params: { query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="large">
    
    <Button onClick={() => setCreating(true)}>导入历史投影</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Report> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 800 }} locale={{ emptyText: <NoData text="尚无历史导入报告。" /> }} columns={[
        { title: '导入报告', key: 'id', render: (_, row) => <Button disabled={query.isError} onClick={() => setSelected(row.id)}>{row.id}</Button> },
        { title: '方式', key: 'mode', render: (_, row) => row.dry_run ? '仅试运行' : '只读历史导入' },
        { title: '投影 / 新增 / 已有', key: 'counts', render: (_, row) => `${row.projected_rows} / ${row.new_rows} / ${row.existing_rows}` },
        { title: '复核', key: 'review', render: (_, row) => row.manual_review_required ? '需要人工复核' : '仍须完成整体迁移验收' },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {creating && <ImportEditor close={() => setCreating(false)} />}
    {selected && <ImportDetail key={selected} id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}
function ImportEditor({ close }: { close: () => void }) {
  const [form] = Form.useForm<Omit<Import, 'schema_version'>>(); const [submitted, setSubmitted] = useState<Import>();
  const [receipt, setReceipt] = useState<Report>(); const intent = useRef(new Intent()); const unknown = useRef(false);
  const client = useQueryClient(); const online = useOnline(); const { modal } = App.useApp();
  const mutation = useMutation({ mutationFn: async (body: Import) => {
    const result = dataOf(await api.POST('/api/v2/migrations/import', { body, params: { header: intent.current.headers('POST', '/api/v2/migrations/import', body) } }));
    if (result.resource.export_ref !== body.export_ref || result.resource.dry_run !== body.dry_run) throw new Error('导入回执与原请求不一致。');
    return result.resource;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['migration-reports'] }); }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) setSubmitted(undefined);
  } });
  useGuard(!receipt);
  async function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    const fields = await form.validateFields().catch(() => undefined);
    if (fields) { const body: Import = { schema_version: 1, export_ref: fields.export_ref, dry_run: fields.dry_run }; setSubmitted(body); mutation.mutate(body); }
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (submitted && !receipt) modal.confirm({ title: '关闭结果尚未确认的导入？', content: '关闭不会撤销可能已提交的导入。请先核对原报告，避免创建重复请求。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  return <Modal open title="导入历史投影" onCancel={dismiss} onOk={() => { void submit(); }} maskClosable={false} closable={!mutation.isPending} confirmLoading={mutation.isPending}
    okText={submitted ? '重试同一导入请求' : '提交导入请求'} cancelText="返回" okButtonProps={{ disabled: !online }} footer={receipt ? <Button onClick={close}>返回报告列表</Button> : undefined}>
    <Space orientation="vertical" className="full-width" size="middle">
      
      <Form form={form} layout="vertical" disabled={!!submitted || !online} initialValues={{ dry_run: true }}>
        <Form.Item name="export_ref" label="已登记的导出编号" rules={[{ required: true, message: '请输入原导出编号。' }, { pattern: uuidPattern, message: '请输入完整 UUIDv7 编号。' }]}><Input autoComplete="off" /></Form.Item>
        <Form.Item name="dry_run" valuePropName="checked"><Checkbox>仅试运行，不创建历史记录</Checkbox></Form.Item>
      </Form>
      <ErrorNotice error={mutation.error} />
      {submitted && mutation.isError && <Alert showIcon type="warning" title="提交结果未知，请重试当前操作" />}
      {receipt && <><Alert showIcon type={receipt.manual_review_required ? 'warning' : 'info'} title={receipt.dry_run ? '试运行报告已保存' : '只读历史导入报告已保存'} /><Typography.Text className="break-word">{receipt.id}</Typography.Text></>}
    </Space>
  </Modal>;
}
function ImportDetail({ id, close }: { id: string; close: () => void }) {
  const query = useQuery({ queryKey: ['migration-detail', id], queryFn: async ({ signal }) => {
    const [report, source] = await Promise.all([
      api.GET('/api/v2/migrations/reports/{id}', { params: { path: { id } }, signal }).then(dataOf),
      api.GET('/api/v2/migrations/reports/{id}/source', { params: { path: { id } }, signal }).then(dataOf),
    ]);
    if (report.id !== id || source.source_installation_id !== report.source_installation_id) throw new Error('导入报告或原安装身份不一致。');
    return { report, source };
  } });
  const value = query.isError ? undefined : query.data;
  return <Drawer open title="历史导入报告" width={1000} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" className="full-width" size="large">
        <Alert showIcon type="warning" title={value.report.manual_review_required ? '需要人工复核' : '仍须核对完整迁移验收'} />
        <Descriptions column={1} className="break-word" items={[
          { key: 'id', label: '报告编号', children: id }, { key: 'export', label: '原导出编号', children: value.report.export_ref },
          { key: 'installation', label: '原安装编号', children: value.report.source_installation_id },
          { key: 'mode', label: '方式', children: value.report.dry_run ? '仅试运行' : '只读历史导入' },
          { key: 'counts', label: '投影 / 新增 / 已有', children: `${value.report.projected_rows} / ${value.report.new_rows} / ${value.report.existing_rows}` },
          { key: 'checked', label: '已核对投影关系数', children: value.report.checked_relationships },
          { key: 'missing', label: '缺少的预期表', children: value.source.missing_tables.join('、') || '无' },
          { key: 'unverified', label: '尚未核验的关系', children: value.report.unverified_relationships.join('、') || '无' },
          { key: 'schema', label: '原库版本 / 检查时间', children: `${value.source.inspection.source_schema_version} / ${displayTime(value.source.inspection.inspected_at)}` },
        ]} />
        <Table<Schema['HistoricalTableExportV1']> rowKey="table" dataSource={value.source.tables} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 650 }} columns={[
          { title: '原表', dataIndex: 'table' }, { title: '原行数', dataIndex: 'source_rows' }, { title: '投影行数', dataIndex: 'projected_rows' },
          { title: '结构', key: 'schema', render: (_, row) => row.unsupported_schema ? '不支持' : '已核对' },
          { title: '排除字段及原因', key: 'excluded', render: (_, row) => row.excluded_columns.map(c => `${c.column}：${c.reason}`).join('；') || '无' },
        ]} />
        <Collapse items={[{ key: 'relations', label: '原外键与类型检查元数据', children: <pre tabIndex={0} className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(value.source.inspection, null, 2)}</pre> }]} />
        <Collapse items={[{ key: 'artifacts', label: '历史附件与覆盖情况', children: <HistoricalArtifacts report={value.report} /> }]} />
        <Typography.Title level={3}>原身份映射</Typography.Title>
        <Mappings report={value.report} />
      </Space>}
    </QueryPanel>
  </Drawer>;
}
function Mappings({ report }: { report: Report }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['migration-mappings', report.id, history.at(-1)], queryFn: async ({ signal }) => {
    const result = dataOf(await api.GET('/api/v2/migrations/reports/{id}/mappings', { params: { path: { id: report.id }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (result.items.some(row => row.key.source_installation_id !== report.source_installation_id)) throw new Error('原身份映射不属于这份导入来源。');
    return result;
  } });
  return <>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['HistoricalMappingViewV1']> rowKey="id" expandable={{ expandedRowRender: row => <RecordFields report={report.id} record={row.id} /> }} dataSource={query.isError ? [] : query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 1000 }} locale={{ emptyText: <NoData text={report.dry_run ? '试运行没有创建历史映射。' : '本报告没有历史映射。'} /> }} columns={[
        { title: '新追溯编号', dataIndex: 'id' }, { title: '原表', key: 'table', render: (_, row) => row.key.source_table },
        { title: '完整原主键', key: 'key', render: (_, row) => <span className="break-word">{JSON.stringify(row.key.values)}</span> },
        { title: '首次导入报告', dataIndex: 'first_import_id' }, { title: '处置', key: 'disposition', render: (_, row) => row.disposition === 'LEGACY_REVALIDATION_REQUIRED' ? '旧证据须重新验证' : '只读历史' },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </>;
}


function RecordFields({ report, record }: { report: string; record: string }) {
  const [selected, setSelected] = useState<Schema['HistoricalFieldSummaryV1']>();
  const query = useQuery({ queryKey: ['historical-fields', report, record], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/migrations/reports/{id}/records/{record}/fields', { params: { path: { id: report, record } }, signal }));
    if (value.report_id !== report || value.record_id !== record) throw new Error('字段目录不属于原报告记录。');
    return value;
  } });
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {!query.isError && <Space orientation="vertical" className="full-width">
      <Typography.Text>已导入的原字段；被排除的字段不在此目录中。</Typography.Text>
      <Table<Schema['HistoricalFieldSummaryV1']> rowKey="name" dataSource={query.data?.fields} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 400 }} columns={[
        { title: '字段', key: 'name', render: (_, field) => <Button onClick={() => setSelected(field)}>查看字段 {field.name}</Button> },
        { title: '字符数', key: 'length', render: (_, field) => field.character_count ?? 'SQL NULL' },
      ]} />
      {selected && <FieldContent key={selected.name} report={report} record={record} field={selected} />}
    </Space>}
  </QueryPanel>;
}
function FieldContent({ report, record, field }: { report: string; record: string; field: Schema['HistoricalFieldSummaryV1'] }) {
  const [history, setHistory] = useState<(string | undefined)[]>(['0']); const offset = history.at(-1) ?? '0';
  const query = useQuery({ queryKey: ['historical-field', report, record, field.name, offset], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/migrations/reports/{id}/records/{record}/field', { params: { path: { id: report, record }, query: { name: field.name, offset } }, signal }));
    const end = BigInt(offset) + BigInt(Array.from(value.text ?? '').length);
    const total = BigInt(field.character_count ?? '0');
    if (value.report_id !== report || value.record_id !== record || value.name !== field.name || value.offset !== offset || value.total_characters !== field.character_count
      || (value.text === null) !== (field.character_count === null) || end > total || value.next_offset !== (end < total ? end.toString() : null)) throw new Error('字段分段不属于原记录或字符位置。');
    return value;
  } });
  const value = query.isError ? undefined : query.data;
  return <Space orientation="vertical" className="full-width">
    <Typography.Title level={4}>原字段：{field.name}</Typography.Title>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {value && <>
        <Typography.Text>字符位置（从0起）：{value.offset}；总字符数：{value.total_characters ?? 'NULL'}</Typography.Text>
        {value.text === null ? <Typography.Text>SQL NULL（无值）</Typography.Text> : value.text === '' ? <Typography.Text>空字符串（0字符）</Typography.Text>
          : <pre aria-label="原字段内容" tabIndex={0} className="break-word" style={{ whiteSpace: 'pre-wrap', maxHeight: 360, overflow: 'auto' }}>{value.text}</pre>}
      </>}
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_offset} loading={query.isFetching} move={setHistory} />
  </Space>;
}


type HistoricalArtifact = Schema['HistoricalArtifactResultV1'];
const artifactOutcome = { COPIED: '公开副本可读取', MISSING: '原文件缺失', UNSUPPORTED: '不支持的文件', UNREADABLE: '原文件不可读取', SEALED_RETAINED: '保留密封，不读取', MANUAL_REVIEW_REQUIRED: '尚待人工确认公开' };
function HistoricalArtifacts({ report }: { report: Report }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const transfer = useRef<{ controller: AbortController; url?: string } | undefined>(undefined);
  useEffect(() => () => { transfer.current?.controller.abort(); if (transfer.current?.url) URL.revokeObjectURL(transfer.current.url); }, []);
  const query = useQuery({ queryKey: ['historical-artifacts', report.id, history.at(-1)], queryFn: async ({ signal }) => {
    const [summary, results] = await Promise.all([
      api.GET('/api/v2/migrations/reports/{id}/artifacts/summary', { params: { path: { id: report.id } }, signal }).then(dataOf),
      api.GET('/api/v2/migrations/reports/{id}/artifacts', { params: { path: { id: report.id }, query: { cursor: history.at(-1), limit: 25 } }, signal }).then(dataOf),
    ]);
    if (summary.report_id !== report.id || BigInt(summary.stored_records) > BigInt(summary.readable_records)
      || BigInt(summary.readable_records) > BigInt(summary.selected_records) || BigInt(summary.selected_records) > BigInt(summary.projected_records) || BigInt(summary.projected_records) > BigInt(summary.source_records)
      || (report.dry_run && summary.stored_records !== '0')
      || results.items.some(r => r.report_id !== report.id || (r.stored && (report.dry_run || !r.record_id || !r.verified_readable || r.source_outcome !== 'COPIED' || !r.byte_count || BigInt(r.byte_count) === 0n || BigInt(r.byte_count) > 67108864n)))) throw new Error('附件结果不属于这份报告或计数不一致。');
    return { summary, results };
  } });
  const download = useMutation({ mutationFn: async (row: HistoricalArtifact) => {
    if (!row.stored || !row.record_id || !row.byte_count || report.dry_run) throw new Error('此报告没有可下载的副本。');
    transfer.current?.controller.abort(); if (transfer.current?.url) URL.revokeObjectURL(transfer.current.url);
    const current = { controller: new AbortController(), url: undefined as string | undefined }; transfer.current = current;
    const blob = dataOf(await api.GET('/api/v2/migrations/reports/{id}/artifacts/{record}/content', { params: { path: { id: report.id, record: row.record_id } }, parseAs: 'blob', signal: current.controller.signal }));
    if (current.controller.signal.aborted) return;
    if (!(blob instanceof Blob) || BigInt(blob.size) !== BigInt(row.byte_count)) throw new Error('下载字节数与原副本不一致。');
    current.url = URL.createObjectURL(blob);
    const anchor = document.createElement('a'); anchor.href = current.url; anchor.download = `${row.record_id}.bin`; anchor.click();
  } });
  const value = query.isError ? undefined : query.data;
  return <section aria-label="历史附件"><Space orientation="vertical" className="full-width">
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {value && <>
        <Descriptions column={1} items={[
          { key: 'coverage', label: '原附件 / 可映射 / 已选择', children: `${value.summary.source_records} / ${value.summary.projected_records} / ${value.summary.selected_records}` },
          { key: 'copies', label: '可读取 / 已存储', children: `${value.summary.readable_records} / ${value.summary.stored_records}` },
        ]} />
        <Alert showIcon type="info" title={report.dry_run ? '试运行未保存副本，不能下载。' : '仅可下载本报告已保存的公开副本。'} />
        <Table<HistoricalArtifact> rowKey="id" dataSource={value.results.items} pagination={false} scroll={{ x: 800 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="本报告没有可映射的历史附件。" /> }} columns={[
          { title: '原表 / 编号', key: 'identity', render: (_, row) => <span className="break-word">{row.identity.source_table} / {row.identity.source_id}</span> },
          { title: '结果', key: 'outcome', render: (_, row) => row.source_outcome ? artifactOutcome[row.source_outcome] : '未选择' },
          { title: '字节数', key: 'bytes', render: (_, row) => row.byte_count ?? '未读取' },
          { title: '副本', key: 'download', render: (_, row) => <Button disabled={!row.stored || query.isFetching || download.isPending} onClick={() => download.mutate(row)}>下载副本 {row.record_id ?? row.identity.source_id}</Button> },
        ]} />
      </>}
    </QueryPanel>
    <ErrorNotice error={download.error} />
    <Pager history={history} next={query.isError ? undefined : value?.results.next_cursor} loading={query.isFetching} move={setHistory} />
  </Space></section>;
}
