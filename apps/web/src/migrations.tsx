import { Alert, App, Button, Checkbox, Collapse, Descriptions, Drawer, Form, Input, Modal, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { uuidPattern } from './auth';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

type Report = Schema['HistoricalImportReportV1'];
type Import = Schema['HistoricalImportRequestV1'];
export function MigrationManagement() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [creating, setCreating] = useState(false); const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['migration-reports', history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/migrations/reports', { params: { query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="large">
    <Alert showIcon type="info" title="旧数据以只读历史保留" description="导入不会启动旧任务或继承旧资格、审批和凭据。缺表、排除项和未核验关系仍需处理。" />
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
      <Alert showIcon type="info" title="先核对部署者登记的原导出编号" description="这里只接受已登记的原包编号。试运行保存报告，实际导入保存只读历史；两者均不授予新的资格。" />
      <Form form={form} layout="vertical" disabled={!!submitted || !online} initialValues={{ dry_run: true }}>
        <Form.Item name="export_ref" label="已登记的导出编号" rules={[{ required: true, message: '请输入原导出编号。' }, { pattern: uuidPattern, message: '请输入完整 UUIDv7 编号。' }]}><Input autoComplete="off" /></Form.Item>
        <Form.Item name="dry_run" valuePropName="checked"><Checkbox>仅试运行，不创建历史记录</Checkbox></Form.Item>
      </Form>
      <ErrorNotice error={mutation.error} />
      {submitted && mutation.isError && <Alert showIcon type="warning" title="结果尚未确认，重试保留原编号、方式和幂等键。" />}
      {receipt && <><Alert showIcon type={receipt.manual_review_required ? 'warning' : 'info'} title={receipt.dry_run ? '试运行报告已保存' : '只读历史导入报告已保存'} description="报告已保存不代表整体迁移验收通过。" /><Typography.Text className="break-word">{receipt.id}</Typography.Text></>}
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
        <Alert showIcon type="warning" title={value.report.manual_review_required ? '需要人工复核' : '仍须核对完整迁移验收'} description="此处为原导出检查和身份映射。原字段内容、产物与密封沿袭不由这些计数证明。" />
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
      <Table<Schema['HistoricalMappingViewV1']> rowKey="id" dataSource={query.isError ? [] : query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 1000 }} locale={{ emptyText: <NoData text={report.dry_run ? '试运行没有创建历史映射。' : '本报告没有历史映射。'} /> }} columns={[
        { title: '新追溯编号', dataIndex: 'id' }, { title: '原表', key: 'table', render: (_, row) => row.key.source_table },
        { title: '完整原主键', key: 'key', render: (_, row) => <span className="break-word">{JSON.stringify(row.key.values)}</span> },
        { title: '首次导入报告', dataIndex: 'first_import_id' }, { title: '处置', key: 'disposition', render: (_, row) => row.disposition === 'LEGACY_REVALIDATION_REQUIRED' ? '旧证据须重新验证' : '只读历史' },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </>;
}
