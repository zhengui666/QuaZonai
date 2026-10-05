import { Alert, Button, Card, Descriptions, Space, Table, Tag, Typography } from 'antd';
import { useMutation, useQueries, useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf, terminal } from './api';
import type { Schema } from './api';
import { validationArtifacts } from './data-input-options';
import { ResourceSelect } from './resource-select';
import { RunDetail } from './runs';
import { ErrorNotice, NoData, Pager, QueryPanel, StateTag, useOnline } from './ui';

type Dataset = Schema['DatasetView'];
type Source = Schema['DataSourceView'];
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
  return { selections,
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

export function DataInputs({ projectId }: { projectId?: string } = {}) {
  const [selection, setProject] = useState<string>();
  const project = projectId ?? selection;
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>冻结输入与数据验证</Typography.Title>
    <Typography.Paragraph>只读查看冻结输入、来源与独立数据验证证据；创建输入和请求验证由外部 Agent 通过 CLI/Skill 执行。</Typography.Paragraph>
    {!projectId && <ResourceSelect label="冻结输入所属研究项目" value={project} onChange={setProject}
      queryKey={['data','input-projects']} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
        return { items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })), next_cursor: page.next_cursor };
      }} />}
    {project ? <ProjectInputs key={project} project={project} /> : <NoData text="明确选择一个研究项目后查看冻结输入。" />}
  </Space>;
}
function ProjectInputs({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const online = useOnline();
  const query = useQuery({ queryKey: ['data','inputs',project,history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/input-sets', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '冻结输入不属于当前项目');
    return page;
  } });
  return <Space orientation="vertical" className="full-width" size="middle">
    <Button disabled={!online || query.isFetching} onClick={() => { void query.refetch(); }}>刷新冻结输入</Button>
    <Typography.Text type="secondary">列表保留全部用途，包括 SEALED；这里只读取冻结身份与原始证据，不更改数据、PIT 或许可。</Typography.Text>
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
  </Space>;
}

export function InputDetails({ id, project }: { id: string; project: string }) {
  const query = useQuery({ queryKey: ['data','input',project,id], queryFn: async ({ signal }) => {
    const input = dataOf(await api.GET('/api/v2/input-sets/{id}', { params: { path: { id } }, signal }));
    if (input.header.id !== id || input.header.project_id !== project) throw new ApiFailure('HTTP_CONTRACT_ERROR', '冻结输入身份不匹配');
    return input;
  } });
  const bindings = useBindings(query.data);
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
        <Typography.Text type="secondary">读取时许可只是当前观测；验证不会赋予真实来源、PIT、Sealed 或交易资格。</Typography.Text>
        <ValidationRuns project={project} inputId={id} />
      </Space>}
    </QueryPanel>
  </Card>;
}

function ValidationRuns({ project, inputId }: { project: string; inputId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [selected, setSelected] = useState<string>();
  const [detail, setDetail] = useState<string>(); const online = useOnline();
  const query = useQuery({ queryKey: ['data','validation-runs',project,inputId,history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/runs', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '运行记录不属于当前项目');
    return page;
  }, refetchInterval: online ? 10_000 : false });
  const runId = selected;
  const runs = query.data?.items.filter(run => run.input_set_id === inputId && run.kind === 'DATA_VALIDATE');
  return <Card title="独立数据验证运行与产物">
    <Space orientation="vertical" className="full-width" size="middle">
      <Typography.Text type="secondary">按项目逐页查找此冻结输入的 DATA_VALIDATE；空页不代表后续页没有运行。</Typography.Text>
      <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
        <Table<Schema['RunSnapshotV1']> rowKey="id" dataSource={runs} pagination={false} scroll={{ x: 700 }}
          locale={{ emptyText: <NoData text="这一项目运行页没有匹配的数据验证。" /> }} columns={[
            { title: '运行 ID', dataIndex: 'id' }, { title: '当前状态', key: 'state', render: (_, run) => <StateTag value={run.state} /> },
            { title: '操作', key: 'actions', render: (_, run) => <Space wrap><Button onClick={() => setDetail(run.id)}>运行详情</Button><Button onClick={() => setSelected(run.id)}>查看产物</Button></Space> },
          ]} />
        <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
      </QueryPanel>
      {runId && <><Button onClick={() => setDetail(runId)}>打开所选运行详情</Button><ValidationArtifacts key={runId} runId={runId} project={project} inputId={inputId} /></>}
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
          { title: '字节', dataIndex: 'byte_count' }, { title: '原始内容', key: 'download', render: (_, item) => <Button aria-busy={download.isPending} disabled={!online || run.isError || !run.data || query.isError || download.isPending} onClick={() => download.mutate(item)}>下载原始产物</Button> },
        ]} />
      <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    <ErrorNotice error={download.error} />
  </Space>;
}
