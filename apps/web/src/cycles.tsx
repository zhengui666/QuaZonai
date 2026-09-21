import { App, Alert, Button, Descriptions, Drawer, Form, Modal, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { RunDetail } from './runs';
import { ErrorNotice, NoData, Pager, QueryPanel, StateTag, useGuard, useOnline } from './ui';

type Brief = Schema['BriefView'];
type Fields = { runtime_id?: string; discovery_input_set_id: string; validation_input_set_id: string; sealed_input_set_id: string; researcher_id?: string; reviewer_id?: string };
type Submitted = { kind: 'freeze'; body: Schema['BriefFreezeV1'] } | { kind: 'start'; body: Schema['CycleStartV1'] };
const required = [{ required: true, message: '请明确选择已有记录。' }];

function useProfile(id?: string) {
  return useQuery({ queryKey: ['codex', 'profile', id], enabled: !!id, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex/{id}', { params: { path: { id: id! } }, signal })) });
}

export function BriefExecution({ brief, close }: { brief: Brief; close: () => void }) {
  const freeze = brief.state === 'DRAFT';
  const [form] = Form.useForm<Fields>();
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const intent = useRef(new Intent()); const hadUnknown = useRef(false);
  const [submitted, setSubmitted] = useState<Submitted>();
  const [receipt, setReceipt] = useState<Schema['FrozenBriefV1'] | Schema['CycleStartedV1']>();
  const runtimeId: string | undefined = Form.useWatch('runtime_id', form);
  const researcherId: string | undefined = Form.useWatch('researcher_id', form);
  const reviewerId: string | undefined = Form.useWatch('reviewer_id', form);
  const researcher = useProfile(researcherId); const reviewer = useProfile(reviewerId);
  const runtime = useQuery({ queryKey: ['integrations', 'runtime', runtimeId], enabled: freeze && !!runtimeId,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: runtimeId! } }, signal })) });
  const project = useQuery({ queryKey: ['project', brief.project_id], staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}', { params: { path: { id: brief.project_id } }, signal })) });
  const frozen = useQuery({ queryKey: ['frozen-brief', brief.id], enabled: !freeze,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/briefs/{id}/execution-context', { params: { path: { id: brief.id } }, signal })) });
  const mutation = useMutation({ mutationFn: async (request: Submitted) => {
    if (request.kind === 'freeze') return dataOf(await api.POST('/api/v2/briefs/{id}/freeze', {
      params: { path: { id: brief.id }, header: intent.current.headers('POST', `/api/v2/briefs/${brief.id}/freeze`, request.body) }, body: request.body,
    }));
    return dataOf(await api.POST('/api/v2/projects/{id}/cycles', {
      params: { path: { id: brief.project_id }, header: intent.current.headers('POST', `/api/v2/projects/${brief.project_id}/cycles`, request.body) }, body: request.body,
    }));
  }, onSuccess: async result => {
    setReceipt(result.resource); intent.current.clear();
    await Promise.all([
      client.invalidateQueries({ queryKey: ['briefs', brief.project_id] }),
      client.invalidateQueries({ queryKey: ['project', brief.project_id] }),
      client.invalidateQueries({ queryKey: ['projects'] }),
      client.invalidateQueries({ queryKey: ['cycles', brief.project_id] }),
      client.invalidateQueries({ queryKey: ['runs'] }),
    ]);
  }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) hadUnknown.current = true;
    // A later 4xx cannot disprove an earlier lost acknowledgement.
    if (rejected && !hadUnknown.current) setSubmitted(undefined);
  } });
  useGuard(true);
  const conflict = submitted === undefined && mutation.error instanceof ApiFailure && mutation.error.code === 'REVISION_CONFLICT';
  const unavailable = !project.data || project.isError || project.isFetching || (freeze ? project.data.state === 'ARCHIVED' : project.data.state !== 'ACTIVE');
  const ready = freeze ? !!runtime.data?.configuration.enabled && !runtime.isError && !runtime.isFetching
    : !!frozen.data && !frozen.isError && !frozen.isFetching && !!researcher.data?.home_binding && !!reviewer.data?.home_binding
      && !researcher.isError && !reviewer.isError && !researcher.isFetching && !reviewer.isFetching;
  const retry = submitted !== undefined && mutation.isError;
  function submit(value: Fields) {
    if (!online || mutation.isPending || unavailable || !ready || submitted || conflict) return;
    const request: Submitted = freeze ? { kind: 'freeze', body: { schema_version: 1, expected_revision: brief.revision,
      execution_context: { schema_version: 1, runtime_id: runtime.data!.id, runtime_revision: runtime.data!.revision,
        discovery_input_set_id: value.discovery_input_set_id, validation_input_set_id: value.validation_input_set_id, sealed_input_set_id: value.sealed_input_set_id } } }
      : { kind: 'start', body: { schema_version: 1, brief_id: brief.id, expected_revision: project.data!.revision,
        researcher_profile: { profile_id: researcher.data!.id, expected_revision: researcher.data!.revision },
        reviewer_profile: { profile_id: reviewer.data!.id, expected_revision: reviewer.data!.revision } } };
    setSubmitted(request); mutation.mutate(request);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (retry) modal.confirm({ title: '关闭未确认的请求？', content: '关闭不会撤销可能已完成的冻结或启动。请先核对 Brief 和研究周期，不要立即重复启动。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  return <Modal open title={freeze ? '冻结 Brief 执行上下文' : '确认启动研究 Cycle'} width={760} maskClosable={false}
    onCancel={dismiss} closable={!mutation.isPending} footer={receipt ? <Button onClick={close}>返回查看记录</Button> : undefined}
    onOk={() => { if (!online || mutation.isPending || receipt) return; if (retry && submitted) mutation.mutate(submitted); else form.submit(); }}
    okText={retry ? '重试同一请求' : freeze ? '确认冻结 Brief' : '确认启动 Cycle'} cancelText="返回"
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || conflict || (!retry && (unavailable || !ready)) }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Typography.Paragraph className="break-word">Brief {brief.id} · 版本 {brief.version} · 修订 {brief.revision}</Typography.Paragraph>
      <ErrorNotice error={mutation.error} />
      {mutation.isError && !submitted && <Button onClick={() => { void Promise.all([
        client.invalidateQueries({ queryKey: ['briefs', brief.project_id] }), client.invalidateQueries({ queryKey: ['project', brief.project_id] }),
        client.invalidateQueries({ queryKey: ['codex'] }), client.invalidateQueries({ queryKey: ['integrations'] }),
      ]).then(close); }}>关闭并重载最新记录</Button>}
      {retry && <Alert type="warning" showIcon title="提交结果未知，请重试当前操作" />}
      {receipt ? 'cycle' in receipt ? <>
        <Alert type="success" showIcon title="研究周期已创建" />
        <Descriptions column={1} items={[
          { key: 'cycle', label: 'Cycle', children: receipt.cycle.id },
          { key: 'run', label: '准备运行', children: receipt.run.id },
          { key: 'state', label: '当前回执状态', children: <StateTag value={receipt.run.state} /> },
        ]} />
      </> : <Alert type="success" showIcon title="Brief 已冻结" /> : <>
        
        <ErrorNotice error={project.error} /><ErrorNotice error={frozen.error} />
        {!freeze && project.data && project.data.state !== 'ACTIVE' && <Alert type="warning" showIcon title="请先启用项目" />}
        {project.data && <Typography.Text>项目修订 {submitted?.kind === 'start' ? submitted.body.expected_revision : project.data.revision} · <StateTag value={project.data.state} /></Typography.Text>}
        <Form form={form} layout="vertical" disabled={!online || mutation.isPending || submitted !== undefined || unavailable || conflict} onFinish={submit}>
          {freeze ? <>
            <Form.Item name="runtime_id" label="执行 Runtime" rules={required}><ResourceSelect label="选择执行 Runtime" queryKey={['startup', 'runtimes']} load={async (cursor, signal) => {
              const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal }));
              return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled })) };
            }} /></Form.Item>
            <ErrorNotice error={runtime.error} />
            {runtime.data && <Typography.Paragraph>Runtime 修订 {submitted?.kind === 'freeze' ? submitted.body.execution_context.runtime_revision : runtime.data.revision}</Typography.Paragraph>}
            {(['DISCOVERY', 'VALIDATION', 'SEALED'] as const).map(purpose => <Form.Item key={purpose} name={`${purpose.toLowerCase()}_input_set_id`} label={`${purpose} 输入集`} rules={required}>
              <ResourceSelect label={`选择 ${purpose} 输入集`} queryKey={['startup', 'inputs', brief.project_id, purpose]} load={async (cursor, signal) => {
                const page = dataOf(await api.GET('/api/v2/input-sets', { params: { query: { project_id: brief.project_id, cursor, limit: 50 } }, signal }));
                return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === brief.project_id && item.purpose === purpose).map(item => ({ value: item.id, label: `${item.id} · 截止 ${displayTime(item.decision_cutoff)}` })) };
              }} />
            </Form.Item>)}
          </> : <>
            {frozen.data && <Descriptions column={1} size="small" items={[
              { key: 'runtime', label: '冻结 Runtime / 修订', children: `${frozen.data.execution_context.runtime_id} / ${frozen.data.execution_context.runtime_revision}` },
              ...(['discovery', 'validation', 'sealed'] as const).map(role => ({ key: role, label: `${role.toUpperCase()} 输入`, children: frozen.data!.execution_context[`${role}_input_set_id`] })),
            ]} />}
            {([['researcher_id', '研究者', researcher], ['reviewer_id', '独立 Reviewer', reviewer]] as const).map(([field, role, profile]) => <div key={field}>
              <Form.Item name={field} label={`${role} Codex 配置`} rules={required}><ResourceSelect label={`选择${role} Codex 配置`} queryKey={['startup', 'profiles']} load={async (cursor, signal) => {
                const page = dataOf(await api.GET('/api/v2/settings/codex', { params: { query: { cursor, limit: 50 } }, signal }));
                return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}`, disabled: !item.home_binding })) };
              }} /></Form.Item>
              <ErrorNotice error={profile.error} />
              {profile.data && <Typography.Paragraph>{role}：{profile.data.name} · 配置修订 {submitted?.kind === 'start' ? submitted.body[field === 'researcher_id' ? 'researcher_profile' : 'reviewer_profile'].expected_revision : profile.data.revision} · {profile.data.model_settings.use_default_model_settings ? '本机默认' : '自定义模型'}</Typography.Paragraph>}
            </div>)}
          </>}
        </Form>
      </>}
    </Space>
  </Modal>;
}

export function Cycles({ projectId }: { projectId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [run, setRun] = useState<string>(); const cursor = history.at(-1);
  const [selection, setSelection] = useState<string>();
  const query = useQuery({ queryKey: ['cycles', projectId, cursor], refetchInterval: 10_000,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: projectId }, query: { cursor, limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="middle">
    
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新研究周期</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['CycleViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 800 }} locale={{ emptyText: <NoData text="暂无研究周期" /> }} columns={[
        { title: '周期', key: 'id', render: (_, cycle) => <Typography.Text className="break-word" copyable>{cycle.id}</Typography.Text> },
        { title: '状态', key: 'state', render: (_, cycle) => <StateTag value={cycle.state} /> },
        { title: '实际结果 / 下一步', key: 'outcome', render: (_, cycle) => <>{cycle.outcome ?? '尚无周期结论'}<br />{cycle.next_action ?? '尚无下一步记录'}</> },
        { title: '实验 已用 / 预约', key: 'budget', render: (_, cycle) => `${cycle.used_experiments} / ${cycle.reserved_experiments}` },
        { title: '开始时间', key: 'started', render: (_, cycle) => displayTime(cycle.started_at) },
        { title: '准备运行', key: 'run', render: (_, cycle) => cycle.initial_run_id && cycle.available_actions.includes('VIEW_RUNS') ? <Button onClick={() => setRun(cycle.initial_run_id!)}>查看准备运行</Button> : '无可查看的准备运行' },
        { title: '冻结比较', key: 'selection', render: (_, cycle) => cycle.available_actions.includes('VIEW_SELECTION') ? <Button onClick={() => setSelection(cycle.id)}>查看试验选择</Button> : '尚未形成选择快照' },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {run && <RunDetail id={run} close={() => setRun(undefined)} />}
    {selection && <CycleSelection key={selection} id={selection} close={() => setSelection(undefined)} />}
  </Space>;
}

function CycleSelection({ id, close }: { id: string; close: () => void }) {
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
