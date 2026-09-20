import { Alert, Button, Descriptions, Drawer, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel, useOnline } from './ui';
import { EvaluationDetail } from './alphas';
import { PortfolioStudy } from './portfolio-study';
import { ReleaseCreate } from './release-create';

type Candidate = Schema['CandidateViewV1'];

export function Candidates({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['portfolio-candidates', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/portfolio-candidates', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('候选记录不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" size="middle" className="full-width">
    
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新候选</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Candidate> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 850 }} locale={{ emptyText: <NoData text="尚无已发布候选，不会生成示例目标。" /> }} columns={[
        { title: '候选编号', dataIndex: 'id', render: (id: string) => <Button type="link" disabled={query.isError} onClick={() => setSelected(id)}>{id}</Button> },
        { title: '执行状态', dataIndex: 'execution_status' }, { title: '求解状态', dataIndex: 'solver_status' },
        { title: '原证据状态', dataIndex: 'evidence_status' }, { title: '来源', dataIndex: 'origin' },
        { title: '决策时点', dataIndex: 'decision_asof', render: displayTime },
        { title: '原因', dataIndex: 'reason_code', render: (value: string | null) => value ?? '无' },
      ]} />
    </QueryPanel>
    <Pager next={query.isError ? undefined : query.data?.next_cursor} history={history} loading={query.isFetching} move={setHistory} />
    {selected && <Detail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
  </Space>;
}

function Detail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const [study, setStudy] = useState(false);
  const [freezing, setFreezing] = useState<Schema['EvaluationView']>();
  const online = useOnline();
  const query = useQuery({ queryKey: ['portfolio-candidate', project, id], queryFn: async ({ signal }) => {
    const detail = dataOf(await api.GET('/api/v2/portfolio-candidates/{id}', { params: { path: { id } }, signal }));
    if (detail.header.id !== id || detail.header.project_id !== project) throw new Error('服务器返回了其他候选记录。');
    return detail;
  } });
  const header = query.data?.header;
  return <Drawer title="不可变候选快照" open onClose={() => { if (!study && !freezing) close(); }} closable={!study && !freezing} maskClosable={!study && !freezing} width={900}>
    
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {header && query.data && <>
        <Button disabled={!online || query.isError || query.isFetching || header.execution_status !== 'SUCCEEDED' || header.evidence_status !== 'VALID'} onClick={() => setStudy(true)}>请求组合 Study</Button>
        <Descriptions column={1} items={Object.entries(header).map(([key, value]) => ({ key, label: key, children: <Typography.Text className="break-word">{value ?? '未生成'}</Typography.Text> }))} />
        <Typography.Title level={2}>原始 Alpha 成员</Typography.Title>
        <Table rowKey="alpha_version_id" dataSource={query.data.members} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 800 }} columns={[
          { title: 'Alpha 版本', dataIndex: 'alpha_version_id' }, { title: '原资格', dataIndex: 'qualification_id' },
          { title: '混合权重', dataIndex: 'ensemble_weight' }, { title: '预测单位', dataIndex: 'forecast_unit' },
          { title: '覆盖比例', dataIndex: 'coverage_fraction' }, { title: '原校准', dataIndex: 'calibration_id', render: (value: string | null) => value ?? '无' },
        ]} />
        <Typography.Title level={2}>原始目标快照</Typography.Title>
        <Table rowKey="instrument_id" dataSource={query.data.targets} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 700 }} locale={{ emptyText: <NoData text="无目标，不补造权重或现金。" /> }} columns={[
          { title: '资产', dataIndex: 'instrument_id' }, { title: '目标权重', dataIndex: 'target_weight' }, { title: '币种', dataIndex: 'currency' },
          { title: '起始', dataIndex: 'asof', render: displayTime }, { title: '截止', dataIndex: 'valid_until', render: displayTime },
        ]} />
        {!query.isError && <CandidateEvaluations id={id} project={project} freeze={setFreezing} canFreeze={header.origin === 'REAL' && header.execution_status === 'SUCCEEDED' && header.evidence_status === 'VALID'} />}
      </>}
    </QueryPanel>
    {freezing && <ReleaseCreate project={project} candidate={id} evaluation={freezing} close={() => setFreezing(undefined)} />}
    {study && header && <PortfolioStudy candidate={header} close={() => setStudy(false)} />}
  </Drawer>;
}

function CandidateEvaluations({ id, project, freeze, canFreeze }: { id: string; project: string; canFreeze: boolean; freeze: (evaluation: Schema['EvaluationView']) => void }) {
  const online = useOnline();
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['candidate-evaluations', project, id, cursor], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/portfolio-candidates/{id}/evaluations', { params: { path: { id }, query: { cursor, limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project || item.subject_candidate_id !== id || item.subject_alpha_version_id !== null || !['FORWARD', 'PORTFOLIO'].includes(item.evaluation_kind))) throw new Error('评估不属于当前候选。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Typography.Title level={2}>已发表的候选研究评估</Typography.Title>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新候选评估</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['EvaluationView']> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 850 }} locale={{ emptyText: <NoData text="尚无已发表的候选评估；不代表通过，也不会自动运行模拟。" /> }} columns={[
        { title: '评估', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>评估 {item.id.slice(-8)}</Button> },
        { title: '评估类型', dataIndex: 'evaluation_kind' },
        { title: '目标包', key: 'release', render: (_, item) => <Button disabled={!online || !canFreeze || query.isError || query.isFetching || item.evaluation_kind !== 'PORTFOLIO' || item.execution_status !== 'SUCCEEDED' || item.evidence_status !== 'VALID' || item.decision !== 'PASS' || item.origin !== 'REAL' || !item.unexpired_at_read} onClick={() => freeze(item)}>冻结目标包</Button> },
        { title: '执行状态', dataIndex: 'execution_status' }, { title: '证据状态', dataIndex: 'evidence_status' },
        { title: '科学决策（非资格）', dataIndex: 'decision' }, { title: '来源', dataIndex: 'origin' },
        { title: '原有效期', key: 'validity', render: (_, item) => item.valid_until ? `${displayTime(item.valid_until)} · ${item.unexpired_at_read ? '读取时未过期' : '读取时已过期'}` : '未授予有效期' },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <EvaluationDetail key={selected} id={selected} candidate={{ id, project }} close={() => setSelected(undefined)} />}
  </Space>;
}
