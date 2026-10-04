import { Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { Pager, QueryPanel } from './ui';

export function AutomationPolicies({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['automation-policies', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/automation-policies', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('自动化政策不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['AutomationPolicyViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }} onHeaderRow={() => ({ tabIndex: 0 })} columns={[
        { title: '原政策', dataIndex: 'id' }, { title: '模式', key: 'mode', render: (_, item) => item.content.mode },
        { title: '原期限', key: 'until', render: (_, item) => displayTime(item.content.valid_until) }, { title: '授权于', dataIndex: 'authorized_at', render: displayTime },
      ]} expandable={{ expandedRowRender: item => <AutomationPolicyDetail policy={item} /> }} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </Space>;
}

export function AutomationPolicyDetail({ policy }: { policy: Schema['AutomationPolicyViewV1'] }) {
  const history = useQuery({ queryKey: ['policy-revocations', policy.id], queryFn: async ({ signal }) => {
    const items: Schema['PolicyRevocationViewV1'][] = []; let cursor: string | undefined; const seen = new Set<string>();
    do {
      const page = dataOf(await api.GET('/api/v2/automation-policies/{id}/revocations', { params: { path: { id: policy.id }, query: { cursor, limit: 100 } }, signal }));
      if (page.items.some(item => item.automation_policy_id !== policy.id)) throw new Error('撤销记录不属于原授权。');
      items.push(...page.items); cursor = page.next_cursor ?? undefined;
      if (cursor && seen.has(cursor)) throw new Error('撤销分页未前进。');
      if (cursor) seen.add(cursor);
    } while (cursor);
    return items;
  } });
  return <Space orientation="vertical" className="full-width">
    <pre tabIndex={0} aria-label="原自动化政策" className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(policy, null, 2)}</pre>
    <Typography.Title level={3}>原政策撤销历史</Typography.Title>
    <QueryPanel pending={history.isPending} error={history.error} stale={!!history.data} reload={() => { void history.refetch(); }}>
      <Table<Schema['PolicyRevocationViewV1']> rowKey="id" dataSource={history.data} size="small" pagination={{ pageSize: 5 }} scroll={{ x: 600 }} onHeaderRow={() => ({ tabIndex: 0 })} columns={[
        { title: '原撤销', dataIndex: 'id' }, { title: '生效于', dataIndex: 'effective_at', render: displayTime }, { title: '原因', dataIndex: 'reason' },
      ]} />
    </QueryPanel>
  </Space>;
}
