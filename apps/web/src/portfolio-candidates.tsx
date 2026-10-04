import { Button, Descriptions, Drawer, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { NoData, Pager, QueryPanel } from './ui';
import { EvaluationDetail } from './alphas';
import { isForecastCandidate, isForecastCandidateDetail } from './producer-views';

type Candidate = Schema['PortfolioCandidateListEnvelopeV2'];

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
      <Table<Candidate> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 850 }} locale={{ emptyText: <NoData text="暂无候选" /> }} columns={[
        { title: '候选编号', dataIndex: 'id', render: (id: string) => <Button type="link" disabled={query.isError} onClick={() => setSelected(id)}>{id}</Button> },
        { title: '执行状态', key: 'execution', render: (_, item) => isForecastCandidate(item) ? item.execution_status : '原生记录未提供' },
        { title: '求解状态 / 分配方法', key: 'solver', render: (_, item) => isForecastCandidate(item) ? item.solver_status : item.allocation_method },
        { title: '原证据状态', key: 'evidence', render: (_, item) => isForecastCandidate(item) ? item.evidence_status : '原生记录未提供' },
        { title: '来源', key: 'origin', render: (_, item) => isForecastCandidate(item) ? item.origin : item.input_provenance.market_data_origin },
        { title: '决策时点', dataIndex: 'decision_asof', render: displayTime },
        { title: '原因', key: 'reason', render: (_, item) => isForecastCandidate(item) ? item.reason_code ?? '无' : '原生记录未提供' },
      ]} />
    </QueryPanel>
    <Pager next={query.isError ? undefined : query.data?.next_cursor} history={history} loading={query.isFetching} move={setHistory} />
    {selected && <CandidateDetail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
  </Space>;
}

export function CandidateDetail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const query = useQuery({ queryKey: ['portfolio-candidate', project, id], queryFn: async ({ signal }) => {
    const detail = dataOf(await api.GET('/api/v2/portfolio-candidates/{id}', { params: { path: { id } }, signal }));
    const identity = isForecastCandidateDetail(detail) ? detail.header : detail;
    if (identity.id !== id || identity.project_id !== project) throw new Error('服务器返回了其他候选记录。');
    return detail;
  } });
  const forecast = query.data && isForecastCandidateDetail(query.data) ? query.data : undefined;
  const strategy = query.data && !isForecastCandidateDetail(query.data) ? query.data : undefined;
  const header = forecast?.header;
  return <Drawer title="不可变候选快照" open onClose={close} width={900}>
    
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {header && forecast && <>
        <Descriptions column={1} items={Object.entries(header).map(([key, value]) => ({ key, label: key, children: <Typography.Text className="break-word">{value ?? '未生成'}</Typography.Text> }))} />
        <Typography.Title level={2}>原始 Alpha 成员</Typography.Title>
        <Table rowKey="alpha_version_id" dataSource={forecast.members} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 800 }} columns={[
          { title: 'Alpha 版本', dataIndex: 'alpha_version_id' }, { title: '原资格', dataIndex: 'qualification_id' },
          { title: '混合权重', dataIndex: 'ensemble_weight' }, { title: '预测单位', dataIndex: 'forecast_unit' },
          { title: '覆盖比例', dataIndex: 'coverage_fraction' }, { title: '原校准', dataIndex: 'calibration_id', render: (value: string | null) => value ?? '无' },
        ]} />
        <Typography.Title level={2}>原始目标快照</Typography.Title>
        <Table rowKey="instrument_id" dataSource={forecast.targets} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 700 }} locale={{ emptyText: <NoData text="暂无目标" /> }} columns={[
          { title: '资产', dataIndex: 'instrument_id' }, { title: '目标权重', dataIndex: 'target_weight' }, { title: '币种', dataIndex: 'currency' },
          { title: '起始', dataIndex: 'asof', render: displayTime }, { title: '截止', dataIndex: 'valid_until', render: displayTime },
        ]} />
        {!query.isError && <CandidateEvaluations id={id} project={project} />}
      </>}
      {strategy && <StrategyCandidateDetail candidate={strategy} />}
    </QueryPanel>
  </Drawer>;
}

function StrategyCandidateDetail({ candidate }: { candidate: Schema['StrategyPortfolioCandidateV1'] }) {
  return <>
    <Descriptions column={1} className="break-word" items={[
      { key: 'id', label: '候选编号', children: candidate.id },
      { key: 'project', label: '项目', children: candidate.project_id },
      { key: 'mandate', label: '原组合配置', children: candidate.mandate_id },
      { key: 'source', label: '策略来源', children: candidate.source_kind },
      { key: 'method', label: '分配方法', children: candidate.allocation_method },
      { key: 'purpose', label: '用途', children: candidate.purpose.purpose },
      { key: 'run', label: '原运行', children: candidate.run_id },
      { key: 'attempt', label: '接受的 Attempt', children: candidate.accepted_attempt_id },
      { key: 'report', label: '原报告', children: candidate.report_artifact_id },
      { key: 'input', label: '冻结输入', children: candidate.input_set_id },
      { key: 'asof', label: '决策时点', children: displayTime(candidate.decision_asof) },
      { key: 'created', label: '创建于', children: displayTime(candidate.created_at) },
      { key: 'cash', label: '现金权重', children: candidate.cash_weight },
    ]} />
    <Typography.Paragraph>原生候选记录未提供预测求解状态或科学资格结论；接受的执行不代表证据通过或已获交付审批。</Typography.Paragraph>
    <Typography.Title level={2}>原始策略成员</Typography.Title>
    <Table<Schema['StrategyMemberSelectionV1']> rowKey="alpha_version_id" dataSource={candidate.members} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 700 }} columns={[
      { title: 'Alpha 版本', dataIndex: 'alpha_version_id' }, { title: '混合权重', dataIndex: 'ensemble_weight' },
    ]} />
    <Typography.Title level={2}>原始目标快照</Typography.Title>
    <Table<Schema['AllocationTargetV1']> rowKey="instrument_id" dataSource={candidate.targets} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 700 }} locale={{ emptyText: <NoData text="暂无目标" /> }} columns={[
      { title: '资产', dataIndex: 'instrument_id' }, { title: '目标权重', dataIndex: 'weight' }, { title: '币种', dataIndex: 'currency' },
    ]} />
    <Typography.Title level={2}>原始输入来源与用途</Typography.Title>
    <pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify({ input_provenance: candidate.input_provenance, purpose: candidate.purpose }, null, 2)}</pre>
  </>;
}

function CandidateEvaluations({ id, project }: { id: string; project: string }) {
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
      <Table<Schema['EvaluationView']> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 850 }} locale={{ emptyText: <NoData text="暂无候选评估" /> }} columns={[
        { title: '评估', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>评估 {item.id.slice(-8)}</Button> },
        { title: '评估类型', dataIndex: 'evaluation_kind' },
        { title: '执行状态', dataIndex: 'execution_status' }, { title: '证据状态', dataIndex: 'evidence_status' },
        { title: '科学决策（非资格）', dataIndex: 'decision' }, { title: '来源', dataIndex: 'origin' },
        { title: '原有效期', key: 'validity', render: (_, item) => item.valid_until ? `${displayTime(item.valid_until)} · ${item.unexpired_at_read ? '读取时未过期' : '读取时已过期'}` : '未授予有效期' },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <EvaluationDetail key={selected} id={selected} candidate={{ id, project }} close={() => setSelected(undefined)} />}
  </Space>;
}
