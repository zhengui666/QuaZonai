import { Button, Descriptions, Space, Table, Tabs } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel } from './ui';

export function ForwardHistory({ project }: { project: string }) {
  return <Space orientation="vertical" className="full-width">
    
    <Tabs items={[{ key: 'observations', label: '劣化观察', children: <Observations project={project} /> }, { key: 'wakes', label: 'Wake 记录', children: <Wakes project={project} /> }]} />
  </Space>;
}

function Observations({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['forward-observations', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/forward-observations', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('观察不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新观察</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ForwardObservationViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 800 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="暂无观察记录" /> }} columns={[
        { title: '原观察', dataIndex: 'id' }, { title: '原分类', dataIndex: 'classification' },
        { title: '原原因', key: 'reasons', render: (_, item) => item.reason_codes.join(' · ') || '无原因记录' },
        { title: '观察时间', dataIndex: 'observed_at', render: displayTime },
      ]} expandable={{ expandedRowRender: item => <Descriptions column={1} className="break-word" items={[
        { key: 'release', label: '原 Release', children: item.release_id }, { key: 'evaluation', label: '原 Evaluation', children: item.evaluation_id },
        { key: 'policy', label: '原政策', children: item.policy_id }, { key: 'created', label: '记录时间', children: displayTime(item.created_at) },
      ]} /> }} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </Space>;
}

function Wakes({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['wakes', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/wakes', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('Wake 不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新 Wake</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['WakeViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 900 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="尚无 Wake 记录。" /> }} columns={[
        { title: 'Wake', dataIndex: 'id' }, { title: '触发', dataIndex: 'trigger' }, { title: '原状态', dataIndex: 'state' },
        { title: '原原因', dataIndex: 'reason' }, { title: '最早尝试时间', dataIndex: 'not_before', render: displayTime },
      ]} expandable={{ expandedRowRender: item => <Descriptions column={1} className="break-word" items={[
        { key: 'observation', label: '原观察', children: item.observation_id ?? '无观察引用' },
        { key: 'cycle', label: '已创建的原周期', children: item.consumed_cycle_id ?? '尚未创建周期' },
        { key: 'revision', label: '修订', children: item.revision }, { key: 'created', label: '记录时间', children: displayTime(item.created_at) },
        { key: 'updated', label: '更新时间', children: displayTime(item.updated_at) },
      ]} /> }} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </Space>;
}
