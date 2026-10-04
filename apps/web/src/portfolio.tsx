import { Button, Descriptions, Drawer, Space, Table, Tabs, Typography } from 'antd';
import { ExecutionAssumptions } from './execution-assumptions';
import { Candidates } from './portfolio-candidates';
import { EvaluationPolicies } from './evaluation-policies';
import { useQuery } from '@tanstack/react-query';
import { useContext, useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { GuardContext, NoData, Pager, QueryPanel } from './ui';
import { isForecastMandate } from './producer-views';

type Mandate = Schema['MandateViewEnvelopeV2'];
export function Portfolios() {
  const [project, setProject] = useState<string>();
  const { blocked } = useContext(GuardContext);
  return <Space orientation="vertical" size="large" className="full-width">
    <Typography.Title level={1}>组合</Typography.Title>
    <Typography.Paragraph>只读查看组合配置、候选与原始证据。构建和研究操作由外部 Agent 通过 CLI/Skill 执行。</Typography.Paragraph>
    <ResourceSelect label="选择组合所属项目" value={project} onChange={setProject} disabled={blocked} queryKey={['portfolio-projects']} load={async (cursor, signal) => {
      const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
      return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
    }} />
    {project ? <Tabs key={project} items={[{ key: 'mandates', label: '组合配置', children: <Mandates project={project} /> }, { key: 'assumptions', label: '执行假设', children: <ExecutionAssumptions project={project} /> }, { key: 'candidates', label: '候选快照', children: <Candidates project={project} /> }, { key: 'policies', label: '评估政策', children: <EvaluationPolicies project={project} /> }]} /> : <NoData text="请选择项目" />}
  </Space>;
}

export function Mandates({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['mandates', project, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/portfolio-mandates', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Space wrap><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新配置</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Mandate> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="暂无组合配置" /> }} columns={[
        { title: '版本', key: 'version', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>配置 v{item.version}</Button> },
        { title: '目标 / 分配方法', key: 'objective', render: (_, item) => isForecastMandate(item) ? item.content.objective : item.content.allocation_method },
        { title: '资本假设', key: 'capital', render: (_, item) => `${item.content.capital_assumption} ${item.content.base_currency}` },
        { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <MandateDetail project={project} id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

export function MandateDetail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const query = useQuery({ queryKey: ['mandate', id, project], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/portfolio-mandates/{id}', { params: { path: { id } }, signal }));
    if (value.id !== id || value.project_id !== project) throw new Error('配置不属于所选项目。');
    return value;
  } });
  return <Drawer title="不可变组合配置" open onClose={close} width={760}>
    
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <><Descriptions column={1} items={[
        { key: 'id', label: '配置编号', children: <Typography.Text className="break-word" copyable>{query.data.id}</Typography.Text> },
        { key: 'version', label: '版本', children: query.data.version },
        { key: 'created', label: '创建于', children: displayTime(query.data.created_at) },
        { key: 'capital', label: '资本假设（非真实账户）', children: `${query.data.content.capital_assumption} ${query.data.content.base_currency}` },
      ]} /><Typography.Title level={2}>服务器保存的完整配置</Typography.Title><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data.content, null, 2)}</pre></>}
    </QueryPanel>
  </Drawer>;
}
