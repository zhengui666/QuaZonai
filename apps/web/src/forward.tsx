import { Button, Descriptions, Drawer, Space, Table } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel } from './ui';

export function Forward({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Schema['ForwardMessageViewV1']>();
  const query = useQuery({ queryKey: ['forward', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/forward', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('Forward 消息不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新 Forward 消息</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ForwardMessageViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="暂无 Forward 消息" /> }} columns={[
        { title: '消息', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError || query.isFetching} onClick={() => setSelected(item)}>Forward {item.id.slice(-8)}</Button> },
        { title: '流', dataIndex: 'stream_id' }, { title: '序号', dataIndex: 'sequence' }, { title: '消息修订', dataIndex: 'message_revision' },
        { title: '覆盖', dataIndex: 'coverage_status' }, { title: '原观测数', dataIndex: 'observation_count' },
        { title: '开始', dataIndex: 'window_start', render: displayTime }, { title: '结束', dataIndex: 'window_end', render: displayTime },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <ForwardDetail key={selected.id} item={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

function ForwardDetail({ item, close }: { item: Schema['ForwardMessageViewV1']; close: () => void }) {
  const query = useQuery({ queryKey: ['forward-window', item.project_id, item.handoff_id, item.stream_id], queryFn: async ({ signal }) => {
    const window = dataOf(await api.GET('/api/v2/handoffs/{id}/forward-window', { params: { path: { id: item.handoff_id }, query: { stream_id: item.stream_id } }, signal }));
    if (window.handoff_id !== item.handoff_id || window.stream_id !== item.stream_id) throw new Error('Forward 窗口与原交付或流不一致。');
    return window;
  } });
  const window = query.data;
  return <Drawer title="Forward 原消息与当前窗口" open onClose={close} width={760}>
    <Descriptions column={1} className="break-word" items={[
      { key: 'id', label: '原消息', children: item.id }, { key: 'external', label: '下游消息编号', children: item.external_message_id },
      { key: 'handoff', label: '原 Handoff', children: item.handoff_id }, { key: 'release', label: '原 Release', children: item.release_id },
      { key: 'downstream', label: '原下游', children: item.downstream_id }, { key: 'stream', label: '原流', children: item.stream_id },
      { key: 'supersedes', label: '更正的原消息', children: item.supersedes_message_id ?? '无更正引用' },
      { key: 'artifact', label: '原报告产物', children: item.report_artifact_id },
      { key: 'issued', label: '下游发布时间', children: displayTime(item.issued_at) }, { key: 'received', label: '收到时间', children: displayTime(item.received_at) },
    ]} />
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新连续窗口</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!window} reload={() => { void query.refetch(); }}>
      {window && <Descriptions column={1} className="break-word" items={[
        { key: 'continuous', label: '当前连续性', children: window.is_contiguous ? '连续（不代表资格或晋级）' : '不连续或证据不足' },
        { key: 'count', label: '完整观测数', children: window.complete_observations },
        { key: 'reasons', label: '窗口原因', children: window.reason_codes.length ? window.reason_codes.join(' · ') : '无窗口缺口原因（不代表健康）' },
        { key: 'frequency', label: '收益频率', children: window.returns_frequency ?? '未提供' },
        { key: 'start', label: '当前窗口开始', children: window.window_start ? displayTime(window.window_start) : '无窗口' },
        { key: 'end', label: '当前窗口结束', children: window.window_end ? displayTime(window.window_end) : '无窗口' },
        { key: 'messages', label: '当前采纳消息', children: window.latest_message_ids.join(' · ') || '无消息' },
      ]} />}
    </QueryPanel>
  </Drawer>;
}
