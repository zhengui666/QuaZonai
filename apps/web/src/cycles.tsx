import { Button, Descriptions, Drawer, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { RunDetail } from './runs';
import { NoData, Pager, QueryPanel, StateTag } from './ui';

export function BriefExecutionContext({ briefId }: { briefId: string }) {
  const query = useQuery({ queryKey: ['frozen-brief', briefId],
    queryFn: async ({ signal }) => {
      const value = dataOf(await api.GET('/api/v2/briefs/{id}/execution-context', { params: { path: { id: briefId } }, signal }));
      if (value.brief.id !== briefId) throw new Error('执行上下文不属于原 Brief。');
      return value;
    } });
  return <section aria-label="原冻结执行上下文">
    <Typography.Title level={3}>原冻结执行上下文</Typography.Title>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <Descriptions column={1} items={[
        { key: 'runtime', label: '冻结 Runtime / 修订', children: `${query.data.execution_context.runtime_id} / ${query.data.execution_context.runtime_revision}` },
        ...(['discovery', 'validation', 'sealed'] as const).map(role => ({ key: role, label: `${role.toUpperCase()} 输入`, children: query.data!.execution_context[`${role}_input_set_id`] })),
      ]} />}
    </QueryPanel>
  </section>;
}

export function Cycles({ projectId }: { projectId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [run, setRun] = useState<string>(); const cursor = history.at(-1);
  const [selection, setSelection] = useState<string>();
  const query = useQuery({ queryKey: ['cycles', projectId, cursor], refetchInterval: 10_000,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: projectId }, query: { cursor, limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="middle">
    
    <div className="section-toolbar"><div><h2>研究周期</h2><p>从实际执行状态，定位下一步与可审阅的证据。</p></div><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新研究周期</Button></div>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data?.items.length === 0 ? <NoData text="暂无研究周期" /> : <div className="cycle-list">{query.data?.items.map(cycle => <article className="cycle-record" key={cycle.id}>
        <div className="cycle-record-heading"><div className="record-label"><StateTag value={cycle.state} /><time dateTime={cycle.started_at ?? undefined}>开始于 {displayTime(cycle.started_at)}</time></div><Typography.Text className="break-word" copyable>{cycle.id}</Typography.Text></div>
        <div className="cycle-outcome"><div><span>实际结果</span><p>{cycle.outcome ?? '尚无周期结论'}</p></div><div><span>下一步</span><p>{cycle.next_action ?? '尚无下一步记录'}</p></div></div>
        <footer><span className="cycle-budget">实验 已用 / 预约：{cycle.used_experiments} / {cycle.reserved_experiments}</span><Space wrap>
          {cycle.initial_run_id && cycle.available_actions.includes('VIEW_RUNS') ? <Button onClick={() => setRun(cycle.initial_run_id!)}>查看准备运行</Button> : <span className="section-caption">无可查看的准备运行</span>}
          {cycle.available_actions.includes('VIEW_SELECTION') ? <Button onClick={() => setSelection(cycle.id)}>查看试验选择</Button> : <span className="section-caption">尚未形成选择快照</span>}
        </Space></footer>
      </article>)}</div>}
      {(history.length > 1 || query.data?.next_cursor) && <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />}
    </QueryPanel>
    {run && <RunDetail id={run} close={() => setRun(undefined)} />}
    {selection && <CycleSelection key={selection} id={selection} close={() => setSelection(undefined)} />}
  </Space>;
}

export function CycleSelection({ id, close }: { id: string; close: () => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['cycle-selection', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/cycles/{id}/selection', { params: { path: { id } }, signal })) });
  const trials = useQuery({ queryKey: ['cycle-selection-trials', id, cursor], enabled: !!query.data && !query.isError,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/cycles/{id}/selection/trials', { params: { path: { id }, query: { cursor, limit: 25 } }, signal })) });
  const value = query.data;
  return <Drawer title="冻结试验选择" open width={1100} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!value} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" size="middle" className="full-width break-word">
        
        <Descriptions column={1} items={[
          { key: 'cycle', label: '原 Cycle', children: value.cycle_id },
          { key: 'run', label: '原研究 Mission', children: value.research_run_id },
          { key: 'time', label: '快照形成时间', children: displayTime(value.created_at) },
          { key: 'status', label: '冻结集合状态（非科学结论）', children: value.status },
          { key: 'counts', label: '登记 / 可比 / 选中 / 未完成', children: `${value.trial_count} / ${value.eligible_count} / ${value.selected_count} / ${value.unfinished_count}` },
          { key: 'policy', label: '冻结政策', children: value.policy_id },
          { key: 'lineage', label: 'Family / 根血缘', children: `${value.rule.family_id} / ${value.rule.root_lineage_id}` },
          { key: 'input', label: '原比较输入 / 执行假设', children: `${value.rule.comparison_input_set_id} / ${value.rule.execution_assumptions_id}` },
          { key: 'metric', label: '选择指标 / Scope', children: `${value.rule.evaluation_kind} / ${value.rule.metric_code} / ${value.rule.metric_scope}` },
          { key: 'method', label: '方法 / 版本 / 单位 / 频率', children: `${value.rule.method_id} / ${value.rule.method_version} / ${value.rule.unit} / ${value.rule.frequency}` },
          { key: 'rule', label: '排序 / 请求候选数 / 平手', children: `${value.rule.direction} / ${value.rule.candidate_count} / ${value.rule.tie_break}` },
        ]} />
        <QueryPanel pending={trials.isPending} error={trials.error} stale={!!trials.data} reload={() => { void trials.refetch(); }}>
          <Table<Schema['CycleSelectionTrialV1']> rowKey="experiment_id" dataSource={trials.data?.items} pagination={false} scroll={{ x: 950 }} onHeaderRow={() => ({ tabIndex: 0 })}
            locale={{ emptyText: <NoData text="暂无试验" /> }} columns={[
              { title: '原试验', dataIndex: 'experiment_id' },
              { title: '形成时执行状态', key: 'state', render: (_, item) => item.execution_state ?? '未执行' },
              { title: '比较 / 排除理由', dataIndex: 'reason' },
              { title: '原排名', key: 'rank', render: (_, item) => item.rank ?? '不参与排名' },
              { title: '被选中', key: 'selected', render: (_, item) => item.selected ? '是（非批准）' : '否' },
              { title: '原指标数值', key: 'metric', render: (_, item) => item.selection_metric?.value ?? `缺值：${item.selection_metric?.reason_code ?? item.reason}` },
            ]} expandable={{ expandedRowRender: item => <Descriptions column={1} items={[
              { key: 'cycle', label: '原试验 Cycle', children: item.source_cycle_id },
              { key: 'execution', label: '执行状态对应原 Run', children: item.execution_run_id ?? '未执行' },
              { key: 'compile', label: '原编译 Run', children: item.compile_run_id ?? '未准入' },
              { key: 'discovery', label: '原 Discovery Run', children: item.discovery_run_id ?? '未准入' },
              { key: 'validation', label: '原 Validation Run', children: item.validation_run_id ?? '未准入' },
              { key: 'alpha', label: '原 Alpha 版本', children: item.alpha_version_id ?? '未登记' },
              { key: 'review-alpha', label: '冻结审阅版本（非资格）', children: item.review_alpha_version_id ?? '未形成审阅目标' },
              { key: 'evaluation', label: '原正式评估', children: item.evaluation_id ?? '未发表' },
              { key: 'unfinished', label: '可比试验尚未完成', children: item.unfinished ? '是，完整集合尚不能确定' : '否' },
              ...(item.selection_metric ? [
                { key: 'name', label: '原指标 / Scope', children: `${item.selection_metric.metric_code} / ${item.selection_metric.scope}` },
                { key: 'status', label: '原指标状态 / 原因', children: `${item.selection_metric.status} / ${item.selection_metric.reason_code ?? '无缺值原因'}` },
                { key: 'method', label: '原方法 / 版本', children: `${item.selection_metric.method_id} / ${item.selection_metric.method_version}` },
                { key: 'unit', label: '原单位 / 频率', children: `${item.selection_metric.unit} / ${item.selection_metric.frequency}` },
                { key: 'count', label: '原样本数', children: item.selection_metric.observation_count },
                { key: 'period', label: '原指标区间', children: `${displayTime(item.selection_metric.period_start)} — ${displayTime(item.selection_metric.period_end)}` },
                { key: 'source', label: '原来源报告（不下载）', children: item.selection_metric.source_artifact_id },
              ] : []),
            ]} /> }} />
          <Pager history={history} next={trials.data?.next_cursor} loading={trials.isFetching} move={setHistory} />
        </QueryPanel>
      </Space>}
    </QueryPanel>
  </Drawer>;
}
