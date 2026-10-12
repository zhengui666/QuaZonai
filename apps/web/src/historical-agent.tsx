import { Card, Space, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf } from './api';
import { Pager, QueryPanel } from './ui';

function OriginalJson({ label, value }: { label: string; value: unknown }) {
  return <details open><summary>{label}</summary><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(value, null, 2)}</pre></details>;
}

export function HistoricalAgent({ id }: { id: string }) {
  const [turnHistory, setTurnHistory] = useState<(string | undefined)[]>([undefined]);
  const [wakeHistory, setWakeHistory] = useState<(string | undefined)[]>([undefined]);
  const turnCursor = turnHistory.at(-1); const wakeCursor = wakeHistory.at(-1);
  const query = useQuery({ queryKey: ['historical-agent', id, turnCursor, wakeCursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/runs/{id}/agent-history', {
    params: { path: { id }, query: { turn_cursor: turnCursor, wake_cursor: wakeCursor, limit: 25 } }, signal,
  })) });
  return <Card title="历史 CODEX 记录（只读）">
    <Typography.Paragraph type="secondary">仅显示原始会话、配置、轮次、用量与项目唤醒事实。null 表示未记录，不推断取消、终止或零用量；这些不是 Durable 任务。</Typography.Paragraph>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <Space orientation="vertical" className="full-width">
        <OriginalJson label="原始运行与会话" value={query.data.execution} />
        <OriginalJson label="原始配置元数据（不含凭据或启动路径）" value={query.data.profile} />
        <OriginalJson label="原始轮次与实际用量" value={query.data.turns.items} />
        <Pager history={turnHistory} next={query.data.turns.next_cursor} loading={query.isFetching} move={setTurnHistory} />
        <OriginalJson label="项目原始唤醒事实" value={query.data.wakes.items} />
        <Pager history={wakeHistory} next={query.data.wakes.next_cursor} loading={query.isFetching} move={setWakeHistory} />
      </Space>}
    </QueryPanel>
  </Card>;
}
