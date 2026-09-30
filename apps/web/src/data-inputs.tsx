import { Alert, App, Button, Card, Descriptions, Form, Input, InputNumber, Modal, Select, Space, Table, Tag, Typography } from 'antd';
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { useContext, useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf, Intent, sameInstant, terminal } from './api';
import type { Schema } from './api';
import { createFrozenRequest, datasetIssue, requiresReload, runtimeValidationIssue, unknownOutcome, validationArtifacts,
  validationInputIssue, validationRequest } from './data-input-options';
import type { DatasetSelection, ValidationLimits, WorkbenchPurpose } from './data-input-options';
import { ResourceSelect } from './resource-select';
import { RunDetail } from './runs';
import { ErrorNotice, GuardContext, NoData, Pager, QueryPanel, StateTag, useClock, useGuard, useOnline } from './ui';

type Dataset = Schema['DatasetView'];
type Source = Schema['DataSourceView'];
type FrozenReceipt = Schema['CommandResult_InputSetView'];
type RunReceipt = Schema['CommandResult_RunSnapshotV1'];
type Submission<T> = { body: T; headers: { 'Idempotency-Key': string } };
const required = { required: true, message: '请填写此项' };
function localFailure(error: unknown) { return error instanceof Error ? new ApiFailure('INVALID_INPUT', error.message) : error; }
function Identity({ value }: { value: string }) { return <Typography.Text className="break-word" copyable>{value}</Typography.Text>; }

function useSources(ids: string[]) {
  return useQueries({ queries: [...new Set(ids)].map(id => ({ queryKey: ['data','source',id],
    queryFn: async ({ signal }: { signal: AbortSignal }) => {
      const source = dataOf(await api.GET('/api/v2/data/sources/{id}', { params: { path: { id } }, signal }));
      if (source.id !== id) throw new ApiFailure('HTTP_CONTRACT_ERROR', '数据源身份不匹配');
      return source;
    } })) });
}
function useBindings(input?: Schema['InputSetView']) {
  const ids = input?.items.flatMap(({ item }) => item.kind === 'DATASET' ? [item.dataset_revision_id] : []) ?? [];
  const datasets = useQueries({ queries: ids.map(id => ({ queryKey: ['data','revision',id], queryFn: async ({ signal }: { signal: AbortSignal }) => {
    const dataset = dataOf(await api.GET('/api/v2/data/revisions/{id}', { params: { path: { id } }, signal }));
    if (dataset.id !== id) throw new ApiFailure('HTTP_CONTRACT_ERROR', '数据版本身份不匹配');
    return dataset;
  } })) });
  const sources = useSources(datasets.flatMap(query => query.data ? [query.data.source_id] : []));
  const selections = datasets.flatMap(query => {
    const dataset = query.data; const source = sources.find(value => value.data?.id === dataset?.source_id)?.data;
    return dataset && source ? [{ dataset, source }] : [];
  });
  return { datasets, sources, selections, loading: [...datasets, ...sources].some(query => query.isFetching),
    error: [...datasets, ...sources].find(query => query.isError)?.error,
    reload: () => { for (const query of [...datasets, ...sources]) void query.refetch(); } };
}

function DatasetFacts({ dataset, source }: { dataset: Dataset; source?: Source }) {
  return <Space orientation="vertical" size="small" className="full-width">
    <Space wrap><Tag>{dataset.partition}</Tag><Tag>{dataset.origin}</Tag><Tag>PIT: {dataset.pit_status}</Tag></Space>
    {(dataset.origin !== 'REAL' || dataset.pit_status !== 'VERIFIED') && <Alert type="warning" showIcon title="该版本未构成合格真实 PIT 证据；结构验证成功也不会改变其资格。" />}
    <Descriptions size="small" column={1} layout="vertical" className="break-word" items={[
      { key: 'id', label: '不可变数据版本', children: <Identity value={dataset.id} /> },
      { key: 'snapshot', label: '原生 snapshot / 存储版本', children: `${dataset.native_snapshot_ref} / ${dataset.storage_version}` },
      { key: 'source', label: '数据源 / Runtime', children: `${dataset.source_id} / ${source?.runtime_id ?? '尚未解析'}` },
      { key: 'revision', label: '数据源配置版本', children: source?.revision ?? '尚未解析' },
      { key: 'rows', label: '行数 / 类型', children: `${dataset.row_count} / ${dataset.data_kind}` },
      { key: 'available', label: 'available_through（精确 UTC）', children: dataset.available_through },
      { key: 'event', label: '事件范围（精确 UTC）', children: `${dataset.event_start} — ${dataset.event_end}` },
      { key: 'policy', label: '修订政策 / Universe', children: `${dataset.revision_policy} / ${dataset.universe_version_id}` },
      { key: 'license', label: '读取时许可 / 检查时点', children: `${dataset.license_state} / ${dataset.checked_at}` },
      { key: 'grant', label: '原授权 ID', children: dataset.data_use_grant_id },
      { key: 'metadata', label: '登记元数据 ID（全局记录）', children: dataset.native_metadata_artifact_id ?? '没有原生登记证据' },
      { key: 'quality', label: '登记质量报告 ID（全局记录）', children: dataset.quality_artifact_id },
    ]} />
  </Space>;
}

export function DataInputs() {
  const [project, setProject] = useState<string>(); const { blocked } = useContext(GuardContext);
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>冻结输入与数据验证</Typography.Title>
    <Typography.Paragraph>先选择项目，再创建不可变输入。独立数据质量验证单独排队，不修改登记数据、PIT 或许可。</Typography.Paragraph>
    <ResourceSelect label="冻结输入所属研究项目" value={project} disabled={blocked} onChange={setProject}
      queryKey={['data','input-projects']} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
        return { items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })), next_cursor: page.next_cursor };
      }} />
    {project ? <ProjectInputs key={project} project={project} /> : <NoData text="明确选择一个研究项目后查看冻结输入。" />}
  </Space>;
}
function ProjectInputs({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<string>(); const [receipt, setReceipt] = useState<FrozenReceipt>();
  const online = useOnline(); const client = useQueryClient();
  const query = useQuery({ queryKey: ['data','inputs',project,history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/input-sets', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '冻结输入不属于当前项目');
    return page;
  } });
  return <Space orientation="vertical" className="full-width" size="middle">
    <Space wrap><Button type="primary" disabled={!online} onClick={() => setCreating(true)}>新建冻结输入</Button>
      <Button disabled={!online || query.isFetching} onClick={() => { void query.refetch(); }}>刷新冻结输入</Button></Space>
    <Typography.Text type="secondary">列表保留全部用途，包括 SEALED；此工作台只创建和验证 DISCOVERY / VALIDATION。</Typography.Text>
    {receipt && <Card title="原始创建回执">
      <Typography.Paragraph className="break-word">已创建并冻结：{receipt.resource.header.id} · 重放：{receipt.replayed ? '是' : '否'}</Typography.Paragraph>
      <details><summary>查看原始创建回执</summary><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(receipt, null, 2)}</pre></details>
    </Card>}
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['InputSetSummary']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }}
        locale={{ emptyText: <NoData text="此项目尚无冻结输入。" /> }} columns={[
          { title: '输入 ID', key: 'id', render: (_, item) => <Button type="link" onClick={() => setSelected(item.id)}>{item.id}</Button> },
          { title: '用途', dataIndex: 'purpose' }, { title: '版本', dataIndex: 'revision' },
          { title: '决策截止（精确 UTC）', dataIndex: 'decision_cutoff' }, { title: '冻结于（精确 UTC）', dataIndex: 'frozen_at' },
        ]} />
      <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <InputDetails key={selected} id={selected} project={project} />}
    {creating && <CreateInput project={project} close={() => setCreating(false)} created={result => {
      setReceipt(result); setSelected(result.resource.header.id); setCreating(false);
      client.setQueryData(['data','input',project,result.resource.header.id], result.resource);
      setHistory([undefined]); void client.invalidateQueries({ queryKey: ['data','inputs',project] });
    }} />}
  </Space>;
}

function CreateInput({ project, close, created }: { project: string; close: () => void; created: (receipt: FrozenReceipt) => void }) {
  const [form] = Form.useForm<{ cutoff: string }>(); const [purpose, setPurpose] = useState<WorkbenchPurpose>('DISCOVERY');
  const [runtime, setRuntime] = useState<string>(); const [selected, setSelected] = useState<DatasetSelection[]>([]);
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [failure, setFailure] = useState<unknown>();
  const [request, setRequest] = useState<Submission<Schema['InputSetCreate']>>();
  const intent = useRef(new Intent()); const submitting = useRef(false); const { modal } = App.useApp(); const online = useOnline(); const now = useClock();
  const query = useQuery({ queryKey: ['data','input-revisions',purpose,history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/revisions', {
    params: { query: { partition: purpose, cursor: history.at(-1), limit: 25 } }, signal,
  })) });
  const sources = useSources(query.data?.items.map(item => item.source_id) ?? []);
  const mutation = useMutation({ mutationFn: async (submission: Submission<Schema['InputSetCreate']>) => {
    const result = dataOf(await api.POST('/api/v2/input-sets', { body: submission.body, params: { header: submission.headers } }));
    const actual = result.resource;
    if (actual.header.project_id !== submission.body.project_id || actual.header.purpose !== submission.body.purpose
      || !sameInstant(actual.header.decision_cutoff, submission.body.decision_cutoff)
      || actual.items.length !== submission.body.items.length || actual.items.some((entry, index) => {
        const expected = submission.body.items[index]; return entry.ordinal !== index || entry.item.kind !== 'DATASET' || expected?.kind !== 'DATASET'
          || entry.item.dataset_revision_id !== expected.dataset_revision_id || entry.item.role !== expected.role;
      })) throw new ApiFailure('HTTP_CONTRACT_ERROR', '创建回执与原请求不匹配，保留原请求后重试');
    return result;
  }, onSuccess: created, onSettled: () => { submitting.current = false; } });
  useGuard(true);
  const locked = request !== undefined || mutation.isPending;
  function cancel() {
    if (submitting.current || mutation.isPending) return;
    if (request || selected.length || runtime || form.isFieldsTouched()) modal.confirm({ title: '放弃这次冻结输入编辑？',
      content: request ? '已发送请求不会撤销。关闭会丢失本页的原请求重试身份；结果未知时应先原样重试取得回执。' : '未保存的选择和截止时间将丢失。',
      okText: '确认放弃', cancelText: '继续编辑', onOk: close });
    else close();
  }
  function submit(cutoff: string) {
    if (!online || submitting.current || mutation.isPending) return;
    try {
      if (!request && (query.isError || query.isFetching || sources.some(value => value.isError || value.isFetching))) throw new Error('数据列表或数据源尚未更新，请重新载入后提交');
      const body = request?.body ?? createFrozenRequest(project, purpose, cutoff, runtime ?? '', selected);
      const original = request ?? { body, headers: intent.current.headers('POST','/api/v2/input-sets',body) };
      setFailure(undefined); setRequest(original); submitting.current = true; mutation.mutate(original);
    } catch (error) { setFailure(localFailure(error)); }
  }
  return <Modal open width={1000} title="创建并冻结项目输入" maskClosable={false} closable={!mutation.isPending}
    onCancel={cancel} onOk={() => { if (request) submit(request.body.decision_cutoff); else form.submit(); }}
    okText={request ? '原样重试创建请求' : '确认创建并冻结'} cancelText="返回" confirmLoading={mutation.isPending}
    okButtonProps={{ disabled: !online || (mutation.error instanceof ApiFailure && mutation.error.retryAt > now) || (requiresReload(mutation.error) && !unknownOutcome(mutation.error)), 'aria-busy': mutation.isPending }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Alert type="info" showIcon title="一次请求即创建并冻结，不能修改原输入。最多选择 255 个数据版本，预留原生任务参数位置。" />
      <Typography.Text className="break-word">项目：{project}</Typography.Text>
      <Form form={form} layout="vertical" disabled={locked || !online} onFinish={({ cutoff }) => submit(cutoff)}>
        <Form.Item label="冻结用途"><Select aria-label="冻结用途" value={purpose} options={['DISCOVERY', 'VALIDATION'].map(value => ({ value, label: value }))}
          onChange={value => { setPurpose(value); setSelected([]); setHistory([undefined]); setFailure(undefined); }} /></Form.Item>
        <Form.Item label="数据所属 Runtime"><ResourceSelect label="选择冻结输入的 Runtime" value={runtime} onChange={id => { setRuntime(id); setSelected([]); }}
          queryKey={['data','input-runtimes']} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal }));
            return { items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled })), next_cursor: page.next_cursor };
          }} /></Form.Item>
        <Form.Item name="cutoff" label="决策截止（精确 UTC）" rules={[required]} extra="使用 UTC Z 格式，最多六位小数；不转换或舍入微秒。例如 2026-01-01T00:00:00.000001Z">
          <Input placeholder="YYYY-MM-DDTHH:mm:ss.ffffffZ" autoComplete="off" /></Form.Item>
      </Form>
      <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
        <Table<Dataset> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 950 }}
          rowSelection={{ selectedRowKeys: selected.map(item => item.dataset.id), preserveSelectedRowKeys: true, hideSelectAll: true,
            getCheckboxProps: dataset => { const source = sources.find(item => item.data?.id === dataset.source_id); return {
              disabled: locked || !online || query.isError || query.isFetching || !runtime || source?.isError || source?.isFetching
                || !source?.data || !!datasetIssue({ dataset, source: source.data }, purpose, runtime) || (selected.length >= 255 && !selected.some(item => item.dataset.id === dataset.id)),
            }; }, onSelect: (dataset, checked) => { const source = sources.find(item => item.data?.id === dataset.source_id)?.data;
              if (locked || !runtime || !source) return;
              setSelected(previous => checked ? [...previous, { dataset, source }] : previous.filter(item => item.dataset.id !== dataset.id));
            } }}
          expandable={{ expandedRowRender: dataset => <DatasetFacts dataset={dataset} source={sources.find(item => item.data?.id === dataset.source_id)?.data} /> }}
          locale={{ emptyText: <NoData text="没有此用途的已登记数据版本。请先在数据登记完成准备。" /> }} columns={[
            { title: '数据版本', dataIndex: 'id' }, { title: '原生 snapshot / 存储版本', key: 'snapshot', render: (_, item) => `${item.native_snapshot_ref} / ${item.storage_version}` },
            { title: '来源 / PIT', key: 'origin', render: (_, item) => `${item.origin} / ${item.pit_status}` },
            { title: '可选择状态', key: 'status', render: (_, dataset) => {
              const source = sources.find(item => item.data?.id === dataset.source_id);
              return !runtime ? '先选择 Runtime' : source?.isError ? '数据源读取失败' : !source?.data ? '正在解析数据源' : datasetIssue({ dataset, source: source.data }, purpose, runtime) ?? '可选择（服务端再次核验）';
            } },
          ]} />
        <Pager history={history} next={locked || query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching || locked} move={setHistory} />
      </QueryPanel>
      {sources.some(query => query.isError) && <ErrorNotice error={sources.find(query => query.isError)?.error} retry={() => { for (const query of sources) void query.refetch(); }} />}
      <Typography.Text>已选择 {selected.length} / 255 个数据版本</Typography.Text>
      {selected.map(item => <Card key={item.dataset.id} size="small" title={item.dataset.id} extra={<Button disabled={locked || !online} onClick={() => setSelected(previous => previous.filter(value => value.dataset.id !== item.dataset.id))}>移除</Button>}><DatasetFacts {...item} /></Card>)}
      <ErrorNotice error={failure ?? mutation.error} />
      {mutation.isError && request && <>
        <Alert type="warning" showIcon title={unknownOutcome(mutation.error) ? '提交结果未知：原请求与幂等键已保留，只能原样重试以取得原回执。' : '请求被拒绝：可保留原请求重试，或明确重新载入后开始新编辑。'} />
        {!unknownOutcome(mutation.error) && <Button disabled={!online} onClick={() => {
          setRequest(undefined); intent.current.clear(); mutation.reset(); setSelected([]); setRuntime(undefined); void query.refetch(); for (const query of sources) void query.refetch();
        }}>重新载入后重新编辑</Button>}
      </>}
    </Space>
  </Modal>;
}

function InputDetails({ id, project }: { id: string; project: string }) {
  const [validating, setValidating] = useState(false); const [receipt, setReceipt] = useState<RunReceipt>();
  const query = useQuery({ queryKey: ['data','input',project,id], queryFn: async ({ signal }) => {
    const input = dataOf(await api.GET('/api/v2/input-sets/{id}', { params: { path: { id } }, signal }));
    if (input.header.id !== id || input.header.project_id !== project) throw new ApiFailure('HTTP_CONTRACT_ERROR', '冻结输入身份不匹配');
    return input;
  } });
  const bindings = useBindings(query.data); const online = useOnline(); const client = useQueryClient();
  const issue = query.data ? validationInputIssue(query.data, project) : undefined;
  return <Card title="原始冻结输入详情">
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); bindings.reload(); }}>
      {query.data && <Space orientation="vertical" className="full-width" size="middle">
        <Descriptions column={1} className="break-word" items={[
          { key: 'id', label: '输入 ID', children: <Identity value={id} /> }, { key: 'project', label: '项目', children: project },
          { key: 'purpose', label: '用途', children: query.data.header.purpose }, { key: 'revision', label: '版本', children: query.data.header.revision },
          { key: 'cutoff', label: '决策截止（精确 UTC）', children: query.data.header.decision_cutoff },
          { key: 'frozen', label: '冻结于（精确 UTC）', children: query.data.header.frozen_at },
        ]} />
        {query.data.items.map(entry => <Card key={entry.id} size="small" title={`#${entry.ordinal} · ${entry.item.kind} · ${entry.item.role}`}>
          {entry.item.kind === 'DATASET' ? (() => {
            const datasetId = entry.item.dataset_revision_id;
            const selection = bindings.selections.find(item => item.dataset.id === datasetId);
            return selection ? <DatasetFacts {...selection} /> : <Typography.Text className="break-word">数据版本：{datasetId} · 元数据尚未载入</Typography.Text>;
          })() : <Typography.Text className="break-word">产物 ID：{entry.item.artifact_id} · 来源：{entry.origin}（这里只显示冻结身份，不读取内容）</Typography.Text>}
        </Card>)}
        <ErrorNotice error={bindings.error} retry={bindings.reload} />
        {issue && <Alert type="info" showIcon title={issue} />}
        <Button type="primary" disabled={!online || query.isError || query.isFetching || !!issue || !!bindings.error || bindings.loading
          || bindings.selections.length !== query.data.items.length} onClick={() => setValidating(true)}>请求数据质量验证</Button>
        <Typography.Text type="secondary">读取时许可只是当前观测；最终许可、数据绑定、Runtime 版本和限额由服务端重新核验。验证不会赋予真实来源、PIT、Sealed 或交易资格。</Typography.Text>
        {validating && <ValidateInput input={query.data} selections={bindings.selections} project={project} close={() => setValidating(false)}
          reload={bindings.reload} queued={result => {
            setReceipt(result); setValidating(false); client.setQueryData(['run',result.resource.id], result.resource);
            void client.invalidateQueries({ queryKey: ['data','validation-runs',project,id] });
          }} />}
        <ValidationRuns project={project} inputId={id} receipt={receipt} />
      </Space>}
    </QueryPanel>
  </Card>;
}

function ValidateInput({ input, project, selections, close, queued, reload }: {
  input: Schema['InputSetView']; project: string; selections: DatasetSelection[]; close: () => void;
  queued: (receipt: RunReceipt) => void; reload: () => void;
}) {
  const [form] = Form.useForm<ValidationLimits>(); const [runtimeId, setRuntimeId] = useState<string>();
  const [failure, setFailure] = useState<unknown>(); const [request, setRequest] = useState<Submission<Schema['DataValidateRequest']>>();
  const intent = useRef(new Intent()); const submitting = useRef(false); const online = useOnline(); const now = useClock(); const { modal } = App.useApp();
  const runtime = useQuery({ queryKey: ['integrations','runtime',runtimeId], enabled: !!runtimeId,
    queryFn: async ({ signal }) => {
      if (!runtimeId) throw new Error('Runtime missing');
      const result = dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: runtimeId } }, signal }));
      if (result.id !== runtimeId) throw new ApiFailure('HTTP_CONTRACT_ERROR', 'Runtime 身份不匹配');
      return result;
    } });
  const readiness = useQuery({ queryKey: ['integrations','readiness',runtimeId], enabled: !!runtimeId,
    refetchInterval: request || !online ? false : 15_000,
    queryFn: async ({ signal }) => {
      if (!runtimeId) throw new Error('Runtime missing');
      return dataOf(await api.GET('/api/v2/integrations/runtimes/{id}/readiness', { params: { path: { id: runtimeId } }, signal }));
    } });
  const issue = runtime.data && readiness.data ? runtimeValidationIssue(runtime.data, readiness.data, now) : '请选择并载入实际绑定的 Runtime';
  const blocked = !!issue || runtime.isError || readiness.isError || runtime.isFetching || readiness.isFetching;
  const capabilities = readiness.data?.latest_observation?.outcome;
  const mutation = useMutation({ mutationFn: async (submission: Submission<Schema['DataValidateRequest']>) => {
    const result = dataOf(await api.POST('/api/v2/data/validate', { body: submission.body, params: { header: submission.headers } }));
    if (result.resource.project_id !== submission.body.project_id || result.resource.input_set_id !== submission.body.input_set_id
      || result.resource.kind !== 'DATA_VALIDATE') throw new ApiFailure('HTTP_CONTRACT_ERROR', '运行回执不匹配；请保留原请求重试');
    return result;
  }, onSuccess: queued, onSettled: () => { submitting.current = false; } });
  useGuard(true);
  function cancel() {
    if (submitting.current || mutation.isPending) return;
    if (request || runtimeId || form.isFieldsTouched()) modal.confirm({ title: '放弃这次数据验证请求？',
      content: request ? '关闭不会取消已排队的运行，并会丢失原请求重试身份。结果未知时应先原样重试取得回执。' : '未提交的 Runtime 选择和限额将丢失。',
      okText: '确认放弃', cancelText: '继续编辑', onOk: close });
    else close();
  }
  function submit(limits: ValidationLimits) {
    if (!online || submitting.current || mutation.isPending) return;
    try {
      if (!request && (blocked || !runtime.data)) throw new Error(issue ?? 'Runtime 读取失败，请重载');
      const body = request?.body ?? validationRequest(input, project, runtime.data!, selections, limits);
      if (!request && capabilities?.status === 'AVAILABLE') {
        const cap = capabilities.capabilities;
        if (body.limits.wall_seconds > cap.max_wall_seconds || body.limits.memory_mib > cap.max_memory_mib
          || BigInt(body.limits.output_bytes) > BigInt(cap.max_output_bytes)
          || (BigInt(body.limits.cpu_seconds) + BigInt(body.limits.wall_seconds) - 1n) / BigInt(body.limits.wall_seconds) > BigInt(cap.max_cpu)) {
          throw new Error('资源限额超过此 Runtime 的当前原生观测');
        }
      }
      const original = request ?? { body, headers: intent.current.headers('POST','/api/v2/data/validate',body) };
      setFailure(undefined); setRequest(original); submitting.current = true; mutation.mutate(original);
    } catch (error) { setFailure(localFailure(error)); }
  }
  const locked = !!request || mutation.isPending;
  const runtimeIds = [...new Set(selections.map(item => item.source.runtime_id))];
  return <Modal open title="单独请求 DATA_VALIDATE" width={760} maskClosable={false} closable={!mutation.isPending}
    onCancel={cancel} onOk={() => { if (request) submit(request.body.limits); else form.submit(); }}
    okText={request ? '原样重试验证请求' : '确认排队数据验证'} cancelText="返回" confirmLoading={mutation.isPending}
    okButtonProps={{ disabled: !online || (mutation.error instanceof ApiFailure && mutation.error.retryAt > now) || (!request && blocked) || (requiresReload(mutation.error) && !unknownOutcome(mutation.error)), 'aria-busy': mutation.isPending }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Alert type="warning" showIcon title="此操作只排队原生数据质量验证。排队、进程成功及报告生成都不代表数据资格通过。" />
      <Typography.Text className="break-word">原输入：{input.header.id} · {input.header.purpose} · {input.header.decision_cutoff}</Typography.Text>
      {runtimeIds.length !== 1 && <Alert type="error" showIcon title="原输入跨多个 Runtime，不能提交独立验证。请创建同一 Runtime 的新冻结输入。" />}
      <Form form={form} layout="vertical" disabled={locked || !online} onFinish={submit}
        initialValues={{ cpu_seconds: '60', wall_seconds: 60, memory_mib: 512, output_bytes: '1048576' }}>
        <Form.Item label="确认实际数据 Runtime"><Select aria-label="确认实际数据 Runtime" value={runtimeId} onChange={setRuntimeId}
          disabled={locked || !online || runtimeIds.length !== 1} options={runtimeIds.map(id => ({ value: id, label: id }))} placeholder="明确选择实际绑定的 Runtime" /></Form.Item>
        <Form.Item name="cpu_seconds" label="CPU 总秒数（精确整数）" rules={[required]}><Input inputMode="numeric" /></Form.Item>
        <Form.Item name="wall_seconds" label="墙钟时间上限（秒）" rules={[required]}><InputNumber min={1} max={86400} precision={0} className="full-width" /></Form.Item>
        <Form.Item name="memory_mib" label="内存上限（MiB）" rules={[required]}><InputNumber min={1} max={1048576} precision={0} className="full-width" /></Form.Item>
        <Form.Item name="output_bytes" label="输出上限（精确字节数，最多 64 MiB）" rules={[required]}><Input inputMode="numeric" /></Form.Item>
      </Form>
      {runtime.data && <Typography.Text className="break-word">{runtime.data.configuration.name} · Runtime {runtime.data.id} · 配置版本 {request?.body.expected_runtime_revision ?? runtime.data.revision}</Typography.Text>}
      {readiness.data?.latest_observation && <Typography.Text className="break-word">观测配置版本 {readiness.data.latest_observation.integration_revision} · 有效至 {readiness.data.latest_observation.valid_until}</Typography.Text>}
      {!request && issue && <Alert type="warning" showIcon title={issue} />}
      {!request && runtimeId && <Button className="table-title" disabled={!online || runtime.isFetching || readiness.isFetching} onClick={() => { void runtime.refetch(); void readiness.refetch(); reload(); }}>重载 Runtime 与原始数据绑定</Button>}
      <ErrorNotice error={runtime.error ?? readiness.error} />
      <ErrorNotice error={failure ?? mutation.error} />
      {mutation.isError && request && <>
        <Alert type="warning" showIcon title={unknownOutcome(mutation.error) ? '提交结果未知：原请求和幂等键已锁定，重试不会改用新的 Runtime 版本。' : '请求被拒绝。不能静默替换已提交的 Runtime 版本或数据版本。'} />
        {!unknownOutcome(mutation.error) && <Button disabled={!online} onClick={() => {
          setRequest(undefined); intent.current.clear(); mutation.reset(); setRuntimeId(undefined); reload();
          void runtime.refetch(); void readiness.refetch();
        }}>重新载入后重新确认</Button>}
      </>}
    </Space>
  </Modal>;
}

function ValidationRuns({ project, inputId, receipt }: { project: string; inputId: string; receipt?: RunReceipt }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [selected, setSelected] = useState<string>();
  const [detail, setDetail] = useState<string>(); const online = useOnline();
  const query = useQuery({ queryKey: ['data','validation-runs',project,inputId,history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/runs', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '运行记录不属于当前项目');
    return page;
  }, refetchInterval: online ? 10_000 : false });
  const runId = selected ?? receipt?.resource.id;
  const runs = query.data?.items.filter(run => run.input_set_id === inputId && run.kind === 'DATA_VALIDATE');
  return <Card title="独立数据验证运行与产物">
    <Space orientation="vertical" className="full-width" size="middle">
      {receipt && <>
        <Alert type="info" showIcon title={`已收到 DATA_VALIDATE 原始排队回执 · ${receipt.resource.id} · 重放：${receipt.replayed ? '是' : '否'}`} />
        <details><summary>查看原始排队回执（不随运行状态改变）</summary><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(receipt, null, 2)}</pre></details>
      </>}
      <Typography.Text type="secondary">按项目逐页查找此冻结输入的 DATA_VALIDATE；空页不代表后续页没有运行。</Typography.Text>
      <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
        <Table<Schema['RunSnapshotV1']> rowKey="id" dataSource={runs} pagination={false} scroll={{ x: 700 }}
          locale={{ emptyText: <NoData text="这一项目运行页没有匹配的数据验证。" /> }} columns={[
            { title: '运行 ID', dataIndex: 'id' }, { title: '当前状态', key: 'state', render: (_, run) => <StateTag value={run.state} /> },
            { title: '操作', key: 'actions', render: (_, run) => <Space wrap><Button onClick={() => setDetail(run.id)}>运行详情与取消</Button><Button onClick={() => setSelected(run.id)}>查看产物</Button></Space> },
          ]} />
        <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
      </QueryPanel>
      {runId && <><Button onClick={() => setDetail(runId)}>打开所选运行详情与取消</Button><ValidationArtifacts key={runId} runId={runId} project={project} inputId={inputId} /></>}
      {detail && <RunDetail id={detail} close={() => setDetail(undefined)} />}
    </Space>
  </Card>;
}

function ValidationArtifacts({ runId, project, inputId }: { runId: string; project: string; inputId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const online = useOnline();
  const transfer = useRef<{ controller: AbortController; url?: string } | undefined>(undefined);
  useEffect(() => () => { transfer.current?.controller.abort(); if (transfer.current?.url) URL.revokeObjectURL(transfer.current.url); }, []);
  const run = useQuery({ queryKey: ['run',runId], queryFn: async ({ signal }) => {
    const result = dataOf(await api.GET('/api/v2/runs/{id}', { params: { path: { id: runId } }, signal }));
    if (result.id !== runId || result.project_id !== project || result.input_set_id !== inputId || result.kind !== 'DATA_VALIDATE') throw new ApiFailure('HTTP_CONTRACT_ERROR', '运行身份与冻结输入不匹配');
    return result;
  }, refetchInterval: current => online && current.state.data && !terminal(current.state.data.state) ? 5000 : false });
  const query = useQuery({ queryKey: ['data','validation-artifacts',project,runId,history.at(-1)],
    queryFn: async ({ signal }) => {
      const page = dataOf(await api.GET('/api/v2/artifacts', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
      if (page.items.some(item => item.project_id !== project)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '产物不属于当前项目');
      return page;
    }, refetchInterval: online && run.data && !terminal(run.data.state) ? 5000 : false });
  const download = useMutation({ mutationFn: async (artifact: Schema['ArtifactView']) => {
    if (!online || run.isError || !run.data || query.isError) return;
    transfer.current?.controller.abort(); if (transfer.current?.url) URL.revokeObjectURL(transfer.current.url);
    const current = { controller: new AbortController(), url: undefined as string | undefined }; transfer.current = current;
    const params = { path: { id: artifact.id } }; const signal = current.controller.signal;
    const metadata = dataOf(await api.GET('/api/v2/artifacts/{id}', { params, signal }));
    if (metadata.id !== artifact.id || validationArtifacts([metadata], project, runId, run.data.active_attempt_id).length !== 1
      || metadata.producer_attempt_id !== artifact.producer_attempt_id || metadata.kind !== artifact.kind
      || metadata.schema_name !== artifact.schema_name || metadata.schema_version !== artifact.schema_version
      || metadata.byte_count !== artifact.byte_count || BigInt(metadata.byte_count) <= 0n || BigInt(metadata.byte_count) > 67108864n) throw new ApiFailure('HTTP_CONTRACT_ERROR', '产物与原始运行/尝试绑定不一致');
    const blob = dataOf(await api.GET('/api/v2/artifacts/{id}/content', { params, signal, parseAs: 'blob' }));
    if (signal.aborted) return;
    if (!(blob instanceof Blob) || BigInt(blob.size) !== BigInt(metadata.byte_count)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '下载字节数与产物元数据不一致');
    current.url = URL.createObjectURL(blob); const anchor = document.createElement('a'); anchor.href = current.url; anchor.download = `${metadata.id}.bin`; anchor.click();
  } });
  return <Space orientation="vertical" className="full-width">
    <Typography.Text className="break-word">所选运行：{runId} · 当前状态：{run.data?.state ?? '正在载入'}</Typography.Text>
    <Typography.Text type="secondary">仅列出与此运行快照 active_attempt_id 精确匹配的 Runtime 产物；历史尝试不混入当前结果。失败或未结束尝试的产物不代表验证通过。</Typography.Text>
    <Typography.Text className="break-word">快照尝试：{run.data?.active_attempt_id ?? '尚无活动尝试'} · 尝试次数：{run.data?.current_attempt_no ?? '尚未载入'}</Typography.Text>
    {run.data && run.data.state !== 'SUCCEEDED' && <Alert type="info" showIcon title="当前运行尚未成功；这里的产物不作为已通过的数据质量结论。" />}
    <ErrorNotice error={run.error} retry={() => { void run.refetch(); }} />
    <Button disabled={!online || query.isFetching} onClick={() => { void query.refetch(); }}>刷新所选运行产物</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ArtifactView']> rowKey="id" dataSource={validationArtifacts(query.data?.items ?? [], project, runId, run.data?.active_attempt_id)} pagination={false} scroll={{ x: 1000 }}
        locale={{ emptyText: <NoData text="这一项目产物页没有此运行的产物；可能尚未发布，请继续翻页或刷新。" /> }} columns={[
          { title: '产物 ID', dataIndex: 'id' }, { title: '生产运行', dataIndex: 'producer_run_id' }, { title: '生产尝试', dataIndex: 'producer_attempt_id' },
          { title: '种类 / 模式 / 来源', key: 'schema', render: (_, item) => `${item.kind} / ${item.schema_name}@${item.schema_version} / ${item.origin}` },
          { title: '字节', dataIndex: 'byte_count' }, { title: '原始内容', key: 'download', render: (_, item) => <Button disabled={!online || run.isError || !run.data || query.isError || download.isPending} onClick={() => download.mutate(item)}>下载原始产物</Button> },
        ]} />
      <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    <ErrorNotice error={download.error} />
  </Space>;
}
