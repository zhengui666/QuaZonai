import { Button, Descriptions, Drawer, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel } from './ui';

type View = Schema['ExecutionAssumptionsViewV1'];

export function ExecutionAssumptions({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['execution-assumptions', project, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/execution-assumptions', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    
    <Space wrap><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新执行假设</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<View> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="暂无执行假设" /> }} columns={[
        { title: '假设编号', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>查看假设 {item.id}</Button> },
        { title: '资本假设', key: 'capital', render: (_, item) => `${item.settings.starting_capital} ${item.settings.base_currency}` },
        { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <ExecutionAssumptionDetail id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

export function ExecutionAssumptionDetail({ id, close }: { id: string; close: () => void }) {
  const query = useQuery({ queryKey: ['execution-assumption', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/execution-assumptions/{id}', { params: { path: { id } }, signal })) });
  return <Drawer title="不可变执行假设" open width={760} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <><Descriptions column={1} items={([
        ['id', '假设编号'], ['input_set_id', '冻结输入'], ['dataset_revision_id', '数据版本'], ['runtime_id', 'Runtime'],
        ['capability_snapshot_artifact_id', '能力探测证据'], ['fee_schedule_artifact_id', '原配置产物'], ['engine_image_ref', '原生镜像'],
        ['venue_capability_ref', '市场'], ['calendar_version', '日历版本'], ['settlement_rule_ref', '结算规则'],
        ['cost_assumption_status', '成本假设'],
      ] as const).map(([key, label]) => ({ key, label, children: <Typography.Text className="break-word" copyable>{query.data![key]}</Typography.Text> }))} />
      
      {query.data.bar_liquidity && <Descriptions column={1} items={[
        { key: 'report', label: '原生历史流动性报告', children: query.data.bar_liquidity.report_artifact_id },
        { key: 'age', label: '历史量最长年龄（秒）', children: query.data.bar_liquidity.maximum_age_seconds },
        { key: 'participation', label: '单 BAR 参与率上限', children: query.data.bar_liquidity.participation_limit },
        { key: 'expiry', label: '原假设失效时刻（不含）', children: displayTime(query.data.bar_liquidity_valid_until) },
      ]} />}
      {query.data.rolling_liquidity && <Descriptions column={1} items={[
        { key: 'policy', label: '原滚动流动性政策', children: query.data.rolling_liquidity_artifact_id },
        { key: 'age', label: '每步历史 BAR 最长年龄（秒）', children: query.data.rolling_liquidity.maximum_age_seconds },
        { key: 'participation', label: '滚动单 BAR 参与率上限', children: query.data.rolling_liquidity.participation_limit },
      ]} />}
      <Typography.Title level={2}>服务器保存的原生模型配置</Typography.Title><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data.settings, null, 2)}</pre></>}
    </QueryPanel>
  </Drawer>;
}
