import { App, Alert, Button, Descriptions, Form, Modal, Space, Table, Typography } from 'antd';
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
      {retry && <Alert type="warning" showIcon title="请求结果尚未确认。" description="原始内容、版本及幂等键已保留；重试不会重新选择配置或创建新的请求意图。" />}
      {receipt ? 'cycle' in receipt ? <>
        <Alert type="success" showIcon title="Cycle 与准备运行已由服务器登记。" description="排队不是研究完成、资格通过或交付。可在研究周期及运行记录中查看实际状态。" />
        <Descriptions column={1} items={[
          { key: 'cycle', label: 'Cycle', children: receipt.cycle.id },
          { key: 'run', label: '准备运行', children: receipt.run.id },
          { key: 'state', label: '当前回执状态', children: <StateTag value={receipt.run.state} /> },
        ]} />
      </> : <Alert type="success" showIcon title="Brief 已冻结，尚未启动研究。" description="执行上下文已固定。返回后请在项目状态中明确启用项目，再选择研究者与独立 Reviewer 的 Codex 配置启动 Cycle；冻结不会自动启用项目。" /> : <>
        <Alert type="info" showIcon title={freeze ? '冻结后不能修改本版本及执行上下文。' : '本次启动会创建真实任务并预约冻结预算。'}
          description={freeze ? '请选择本项目三个不同用途的已冻结输入和已登记 Runtime；服务器会重新核对数据、许可及原生能力。' : '两角色必须明确选择。可使用同一账号配置，但各自使用独立 Thread；启动时冻结配置版本，不自动更换模型设置。'} />
        <ErrorNotice error={project.error} /><ErrorNotice error={frozen.error} />
        {!freeze && project.data && project.data.state !== 'ACTIVE' && <Alert type="warning" showIcon title="项目尚未启用，不能启动 Cycle。请返回修改项目状态。" />}
        {project.data && <Typography.Text>项目修订 {submitted?.kind === 'start' ? submitted.body.expected_revision : project.data.revision} · <StateTag value={project.data.state} /></Typography.Text>}
        <Form form={form} layout="vertical" disabled={!online || mutation.isPending || submitted !== undefined || unavailable || conflict} onFinish={submit}>
          {freeze ? <>
            <Form.Item name="runtime_id" label="执行 Runtime" rules={required}><ResourceSelect label="选择执行 Runtime" queryKey={['startup', 'runtimes']} load={async (cursor, signal) => {
              const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal }));
              return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled })) };
            }} /></Form.Item>
            <ErrorNotice error={runtime.error} />
            {runtime.data && <Typography.Paragraph>Runtime 修订 {submitted?.kind === 'freeze' ? submitted.body.execution_context.runtime_revision : runtime.data.revision}；登记不等于当前已通过能力检查。</Typography.Paragraph>}
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
              {profile.data && <Typography.Paragraph>{role}：{profile.data.name} · 配置修订 {submitted?.kind === 'start' ? submitted.body[field === 'researcher_id' ? 'researcher_profile' : 'reviewer_profile'].expected_revision : profile.data.revision} · {profile.data.model_settings.use_default_model_settings ? '原生默认设置（实际模型待连接确认）' : '使用已保存的显式设置，不回退其他模型'}</Typography.Paragraph>}
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
  const query = useQuery({ queryKey: ['cycles', projectId, cursor], refetchInterval: 10_000,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: projectId }, query: { cursor, limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="middle">
    <Alert type="info" showIcon title="研究周期只显示服务器持久化事实。" description="准备成功不代表模型、科学实验、独立评审或交付已经完成。" />
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新研究周期</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['CycleViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 800 }} locale={{ emptyText: <NoData text="尚无研究周期。请先冻结 Brief，再明确选择两个角色的配置启动。" /> }} columns={[
        { title: '周期', key: 'id', render: (_, cycle) => <Typography.Text className="break-word" copyable>{cycle.id}</Typography.Text> },
        { title: '状态', key: 'state', render: (_, cycle) => <StateTag value={cycle.state} /> },
        { title: '实际结果 / 下一步', key: 'outcome', render: (_, cycle) => <>{cycle.outcome ?? '尚无周期结论'}<br />{cycle.next_action ?? '尚无下一步记录'}</> },
        { title: '实验 已用 / 预约', key: 'budget', render: (_, cycle) => `${cycle.used_experiments} / ${cycle.reserved_experiments}` },
        { title: '开始时间', key: 'started', render: (_, cycle) => displayTime(cycle.started_at) },
        { title: '准备运行', key: 'run', render: (_, cycle) => cycle.initial_run_id && cycle.available_actions.includes('VIEW_RUNS') ? <Button onClick={() => setRun(cycle.initial_run_id!)}>查看准备运行</Button> : '无可查看的准备运行' },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {run && <RunDetail id={run} close={() => setRun(undefined)} />}
  </Space>;
}
