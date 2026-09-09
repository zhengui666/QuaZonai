import { Alert, Button, Card, Descriptions, Drawer, Modal, Select, Space, Table, Timeline, Typography } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchEventSource } from '@microsoft/fetch-event-source';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, AUTH_CHANGED, dataOf, displayTime, Intent, responseFailure, terminal } from './api';
import { responseKind } from './generated/responses.cjs';
import type { Schema } from './api';
import { decodeRunEvent } from './run-events';
import { ErrorNotice, NoData, Pager, QueryPanel, StateTag, useGuard, useOnline } from './ui';

type Run = Schema['RunSnapshotV1'];
export function Runs({ projectId }: { projectId?: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [state, setState] = useState<Schema['RunState']>();
  const [selected, setSelected] = useState<string>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['runs', projectId, state, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/runs', {
    params: { query: { project_id: projectId, state, cursor, limit: 25 } }, signal,
  })), refetchInterval: 10_000 });
  return <Space orientation="vertical" className="full-width" size="middle">
    {!projectId && <Typography.Title level={1}>运行</Typography.Title>}
    <Alert type="info" showIcon title="显示服务器持久化状态。离开页面或断开网络不会取消运行。" description="没有真实进度时不显示百分比；运行成功也不等于 Alpha 合格或已获批准。" />
    <Space wrap>
      <Select<Schema['RunState']> aria-label="按运行状态筛选" allowClear placeholder="全部运行状态" className="state-select" value={state} onChange={value => { setState(value); setHistory([undefined]); }} options={(['QUEUED', 'DISPATCHING', 'RUNNING', 'RECONCILING', 'CANCEL_REQUESTED', 'SUCCEEDED', 'FAILED', 'CANCELLED'] as const).map(value => ({ value, label: <StateTag value={value} /> }))} />
      <Button icon={<ReloadOutlined aria-hidden />} aria-label="刷新运行" aria-busy={query.isFetching} loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新运行</Button>
    </Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Run> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }} locale={{ emptyText: <NoData text="当前筛选下没有运行记录。" /> }} columns={[
        { title: '运行', key: 'id', render: (_, run) => <Button type="link" className="table-title" onClick={() => setSelected(run.id)}>{run.kind} · {run.id.slice(-8)}</Button> },
        { title: '状态', key: 'state', render: (_, run) => <StateTag value={run.state} /> },
        { title: '尝试次数', dataIndex: 'current_attempt_no' },
        { title: '入队时间', key: 'queued', render: (_, run) => displayTime(run.queued_at) },
        { title: '截止时间', key: 'deadline', render: (_, run) => displayTime(run.deadline_at) },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <RunDetail id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}
function RunDetail({ id, close }: { id: string; close: () => void }) {
  const client = useQueryClient(); const online = useOnline();
  const query = useQuery({ queryKey: ['run', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/runs/{id}', { params: { path: { id } }, signal })),
    refetchInterval: current => current.state.data && terminal(current.state.data.state) ? false : 5000,
  });
  const [target, setTarget] = useState<Run>();
  const intent = useRef(new Intent());
  const cancel = useMutation({ mutationFn: async (run: Run) => {
    const body: Schema['RunCancelV1'] = { schema_version: 1, expected_revision: run.revision };
    return dataOf(await api.POST('/api/v2/runs/{id}/cancel', { params: { path: { id: run.id }, header: intent.current.headers('POST', `/api/v2/runs/${run.id}/cancel`, body) }, body }));
  }, onSuccess: async result => {
    client.setQueryData(['run', id], result.resource);
    intent.current.clear(); setTarget(undefined); await client.invalidateQueries({ queryKey: ['runs'] });
  } });
  useGuard(target !== undefined || cancel.isPending);
  return <Drawer title="运行详情" open width={800} onClose={() => { if (!cancel.isPending && !target) close(); }} closable={!target && !cancel.isPending} maskClosable={!target && !cancel.isPending}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <>
        <Space wrap><StateTag value={query.data.state} /><Typography.Text>版本 {query.data.revision}</Typography.Text></Space>
        <Descriptions column={1} items={[
          { key: 'id', label: '运行编号', children: <Typography.Text className="break-word" copyable>{id}</Typography.Text> },
          { key: 'kind', label: '运行类型', children: query.data.kind },
          { key: 'project', label: '研究项目', children: query.data.project_id },
          { key: 'cycle', label: '所属 Cycle', children: query.data.cycle_id ?? '非 Cycle 管理任务' },
          { key: 'attempt', label: '活动尝试', children: query.data.active_attempt_id ?? '尚无活动尝试' },
          { key: 'deadline', label: '截止时间', children: displayTime(query.data.deadline_at) },
          { key: 'cancel', label: '取消请求时间', children: displayTime(query.data.cancellation_requested_at) },
          { key: 'reason', label: '终止原因', children: query.data.terminal_reason_code ?? '尚无终止原因' },
        ]} />
        <Timeline items={[
          { content: `入队：${displayTime(query.data.queued_at)}` },
          { content: `开始：${displayTime(query.data.started_at)}` },
          { content: `结束：${displayTime(query.data.finished_at)}` },
        ]} />
        <Alert type="info" showIcon title="取消是请求，不是即时终止。只有服务器返回 CANCELLED 才表示已取消。" />
        <Button danger disabled={!online || query.isError || cancel.isPending} onClick={() => { cancel.reset(); setTarget(query.data); }}>请求取消运行</Button>
        <RunEvents key={id} snapshot={query.data} />
      </>}
    </QueryPanel>
    <Modal open={target !== undefined} title="确认请求取消这一运行？" onCancel={() => { if (!cancel.isPending) setTarget(undefined); }}
      onOk={() => { if (target && !cancel.isPending && online) cancel.mutate(target); }} okText="确认请求取消" cancelText="返回" confirmLoading={cancel.isPending}
      okButtonProps={{ 'aria-label': '确认请求取消', 'aria-busy': cancel.isPending, danger: true, disabled: !online || (cancel.error instanceof ApiFailure && cancel.error.code === 'REVISION_CONFLICT') }} closable={!cancel.isPending} maskClosable={!cancel.isPending}>
      <Typography.Paragraph className="break-word">运行：{target?.id} · 确认版本：{target?.revision}</Typography.Paragraph>
      <Typography.Paragraph>取消不会擦除已有证据；正在核对的结果仍需服务器确认。</Typography.Paragraph>
      <ErrorNotice error={cancel.error} />
      {cancel.error instanceof ApiFailure && cancel.error.code === 'REVISION_CONFLICT' && <Button onClick={() => { setTarget(undefined); void query.refetch(); }}>关闭确认并重载最新版本</Button>}
    </Modal>
  </Drawer>;
}
function RunEvents({ snapshot }: { snapshot: Run }) {
  const client = useQueryClient(); const online = useOnline();
  const [events, setEvents] = useState<Schema['RunEventV1'][]>([]);
  const [error, setError] = useState<unknown>();
  const [connection, setConnection] = useState('正在连接');
  const [generation, setGeneration] = useState(0);
  const last = useRef(snapshot.last_event_seq);
  const current = useRef(snapshot); current.current = snapshot;
  const stopped = terminal(snapshot.state);
  useEffect(() => {
    if (!online || stopped) return;
    const controller = new AbortController();
    let retries = 0;
    let refresh: ReturnType<typeof setTimeout> | undefined;
    const invalidate = () => {
      if (refresh) return;
      refresh = setTimeout(() => { refresh = undefined; void client.invalidateQueries({ queryKey: ['run', snapshot.id] }); }, 200);
    };
    setError(undefined); setConnection('正在连接');
    void fetchEventSource(`/api/v2/runs/${encodeURIComponent(snapshot.id)}/events`, {
      signal: controller.signal, credentials: 'same-origin', cache: 'no-store', redirect: 'error',
      headers: { 'last-event-id': `${snapshot.id}:${last.current}` },
      async onopen(response) {
        if (controller.signal.aborted) return;
        if (!response.ok) {
          const failure = await responseFailure(response, '/api/v2/runs/{id}/events', 'GET');
          if (failure.code === 'AUTH_REQUIRED') window.dispatchEvent(new Event(AUTH_CHANGED));
          throw failure;
        }
        if (responseKind('/api/v2/runs/{id}/events', 'GET', response.status, response.headers.get('content-type')) !== 'event-stream') {
          throw new ApiFailure('HTTP_CONTRACT_ERROR', '运行事件接口没有返回合同声明的事件流。', response.status);
        }
        retries = 0; setConnection('已连接');
      },
      onmessage(frame) {
        if (controller.signal.aborted) return;
        const event = decodeRunEvent(frame, snapshot.id, last.current);
        if (!event) return;
        last.current = event.seq;
        setEvents(previous => [...previous.slice(-99), event]);
        invalidate();
      },
      onclose() {
        if (controller.signal.aborted || terminal(current.current.state)) return;
        throw new Error('stream disconnected');
      },
      onerror(failure: unknown) {
        if (controller.signal.aborted) throw failure;
        setConnection('连接中断');
        if (failure instanceof ApiFailure) throw failure;
        if (++retries >= 5) throw new ApiFailure('STREAM_DISCONNECTED', '事件连接多次中断。快照仍会定时刷新，请手动重新连接。');
        return Math.min(1000 * 2 ** retries, 30_000);
      },
    }).catch(failure => { if (!controller.signal.aborted) { setError(failure); setConnection('事件流已暂停'); } });
    return () => { controller.abort(); if (refresh) clearTimeout(refresh); };
  }, [snapshot.id, generation, online, stopped, client]);
  async function reconnect() {
    const latest = dataOf(await api.GET('/api/v2/runs/{id}', { params: { path: { id: snapshot.id } } }));
    client.setQueryData(['run', snapshot.id], latest);
    last.current = latest.last_event_seq; setEvents([]); setGeneration(value => value + 1);
  }
  return <Card title="实时事件" extra={<Button disabled={!online} onClick={() => { void reconnect().catch(setError); }}>重载快照并连接</Button>}>
    <Typography.Paragraph type="secondary">{stopped ? '运行已终止' : online ? connection : '离线'}。仅显示本页订阅后的最近 100 项事件；完整状态以运行快照为准。</Typography.Paragraph>
    <ErrorNotice error={error} />
    {events.length === 0 ? <NoData text="本次订阅尚无新事件，不代表历史没有事件。" /> : <Timeline items={events.map(event => ({ key: event.seq, content: <><Typography.Text>{event.event_type} · #{event.seq}</Typography.Text><br /><Typography.Text type="secondary">{displayTime(event.occurred_at)}</Typography.Text></> }))} />}
  </Card>;
}
