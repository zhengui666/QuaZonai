import { Button, Drawer, Space, Table } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel } from './ui';

type Policy = Schema['EvaluationPolicyView'];

export function EvaluationPolicies({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['evaluation-policies', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/evaluation-policies', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('政策不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width" size="middle">
    
    <Space wrap><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新政策</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Policy> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="暂无评估政策" /> }} columns={[
        { title: '版本', key: 'version', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>政策 v{item.version}</Button> },
        { title: '研究问题', dataIndex: 'question' }, { title: '选择评估', key: 'kind', render: (_, item) => item.selection_rule.evaluation_kind },
        { title: '组合要求', key: 'portfolio', render: (_, item) => item.portfolio_metric_requirements === null ? '未定义，不能授予组合 PASS' : `${item.portfolio_metric_requirements.length} 项独立要求` },
        { title: '组合研究计划', key: 'study', render: (_, item) => item.portfolio_study_plan === null ? '未定义' : '已冻结原输入与起点' },
        { title: '创建于', dataIndex: 'created_at', render: displayTime },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <EvaluationPolicyDetail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
  </Space>;
}

export function EvaluationPolicyDetail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const query = useQuery({ queryKey: ['evaluation-policy', project, id], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/evaluation-policies/{id}', { params: { path: { id } }, signal }));
    if (value.id !== id || value.project_id !== project) throw new Error('服务器返回了其他政策。');
    return value;
  } });
  return <Drawer title="不可变评估政策" open width={850} onClose={close}>
    
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <pre tabIndex={0} aria-label="原完整评估政策" className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data, null, 2)}</pre>}
    </QueryPanel>
  </Drawer>;
}
