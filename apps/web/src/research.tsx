import { Alert, App, Button, Descriptions, Form, Input, Select, Space, Tag, Typography } from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf } from './api';
import { ErrorNotice, QueryPanel, useGuard, useOnline } from './ui';
import { conversationsOf, jsonObjectInput, modelsOf, nativeText, object, policyInput, receiptOf, ResearchIntent, tasksOf, usageOf, viewOf } from './research-native';
import type { NativeObject, ResearchSubmission } from './research-native';
import { NativeJson, NativeUsage, ResearchFinal, ResearchTrials } from './research-results';

const rootKey = ['durable-research'] as const;
export function Research() {
  const [selected, setSelected] = useState<number>();
  const [creating, setCreating] = useState(true);
  const [history, setHistory] = useState<(NativeObject | undefined)[]>([undefined]);
  const [draftVersion, setDraftVersion] = useState(0);
  const [admission, setAdmission] = useState<ReturnType<typeof receiptOf>>();
  const cursor = history.at(-1); const online = useOnline();
  const listing = useQuery({ queryKey: [...rootKey, 'list', cursor], refetchInterval: online ? 10_000 : false,
    queryFn: async ({ signal }) => conversationsOf(dataOf(await api.GET('/api/v2/research', {
      params: { query: { cursor: cursor ? JSON.stringify(cursor) : undefined } }, signal,
    }))) });
  return <div className="durable-workspace">
    <header className="research-heading durable-heading"><div><span className="eyebrow">DURABLE RESEARCH</span><Typography.Title level={1}>研究</Typography.Title>
      <p>定义目标与边界，由 Host 上的原生任务持续推进；这里观察实际研究记录。</p></div>
      <Button type="primary" icon={<PlusOutlined aria-hidden />} onClick={() => { setDraftVersion(value => value + 1); setCreating(true); }} disabled={creating}>新建研究</Button>
    </header>
    {!creating && admission && admission.conversationId === selected && <Alert className="global-notice" type="success" showIcon title="研究提交已被接纳"
      description={`Conversation #${admission.conversationId} · Submission #${admission.submissionId}。接纳回执不是计算成功证明。`} />}
    <div className="durable-layout">
      <aside className="durable-studies" aria-labelledby="durable-studies-title">
        <div className="section-toolbar"><div><h2 id="durable-studies-title">已保存的研究</h2><p>原生会话 · 持久化于 Host</p></div>
          <Button type="text" icon={<ReloadOutlined aria-hidden />} aria-label="刷新研究列表" loading={listing.isFetching} onClick={() => { void listing.refetch(); }} /></div>
        <QueryPanel pending={listing.isPending} error={listing.error} stale={!!listing.data} reload={() => { void listing.refetch(); }}>
          {listing.data?.items.length === 0 ? <div className="durable-empty"><h3>尚无研究记录</h3><p>填写研究目标、模型和插件输入后提交。不会自动填入数据或启动演示研究。</p></div>
            : <ul className="durable-study-list">{listing.data?.items.map(item => <li key={item.id}>
              <button aria-pressed={!creating && selected === item.id} onClick={() => { setSelected(item.id); setCreating(false); }}>
                <strong>研究 #{item.id}</strong><span>{object(item.parent) ? `分支来源 #${nativeText(object(item.parent)?.conversationId)}` : '原生会话'}<span aria-hidden> →</span></span>
              </button>
            </li>)}</ul>}
          <Space wrap className="durable-pager"><Button disabled={listing.isFetching || history.length <= 1} onClick={() => setHistory(history.slice(0, -1))}>上一页</Button>
            <Typography.Text>第 {history.length} 页</Typography.Text><Button disabled={listing.isFetching || !listing.data?.cursor} onClick={() => {
              if (listing.data?.cursor) setHistory([...history, listing.data.cursor]);
            }}>下一页</Button></Space>
          {listing.data && <NativeJson label="会话列表 · 原生 JSON" value={listing.data.raw} />}
        </QueryPanel>
      </aside>
      <div className="durable-main">
        {/* Keep this form intent mounted when inspecting an existing study. No agent state is persisted. */}
        <section hidden={!creating} className="durable-compose"><ResearchForm draftVersion={draftVersion} onAdmitted={receipt => {
          setAdmission(receipt); setSelected(receipt.conversationId); setCreating(false);
        }} /></section>
        {!creating && selected !== undefined && <ResearchDetail key={selected} id={selected} />}
      </div>
    </div>
    <HostSnapshot />
  </div>;
}

type Fields = { goal: string; model: string; data: string; policy: string };
function ResearchForm({ draftVersion, onAdmitted }: { draftVersion: number; onAdmitted: (receipt: ReturnType<typeof receiptOf>) => void }) {
  const [form] = Form.useForm<Fields>(); const online = useOnline(); const client = useQueryClient();
  const [dirty, setDirty] = useState(false); const [captured, setCaptured] = useState<ResearchIntent>();
  const intent = useRef<ResearchIntent | undefined>(undefined); const hadUnknown = useRef(false);
  const models = useQuery({ queryKey: [...rootKey, 'models'], queryFn: async ({ signal }) => modelsOf(dataOf(await api.GET('/api/v2/research/models', { signal }))) });
  const modelKey: string | undefined = Form.useWatch('model', form);
  const selectedModel = models.data?.find(model => JSON.stringify([model.provider, model.modelId]) === modelKey);
  const mutation = useMutation({ mutationFn: async (body: ResearchSubmission) => receiptOf(dataOf(await api.POST('/api/v2/research', { body }))),
    onSuccess: receipt => {
      setDirty(false);
      void client.invalidateQueries({ queryKey: rootKey });
      onAdmitted(receipt);
    }, onError: error => {
      const rejected = error instanceof ApiFailure && (error.code === 'OFFLINE'
        || (error.status >= 400 && error.status < 500 && (!!error.problem || error.code === 'RESEARCH_NATIVE_ERROR')));
      if (rejected && !hadUnknown.current) { intent.current = undefined; setCaptured(undefined); }
      else hadUnknown.current = true;
    } });
  const held = !!captured && !mutation.isSuccess;
  useGuard(dirty || held || mutation.isPending);
  const canSubmit = online && !!selectedModel && !models.isError && !models.isPending && !mutation.isPending && !captured;
  function submit(value: Fields) {
    if (!canSubmit || intent.current || !selectedModel) return;
    const request = new ResearchIntent({ goal: value.goal, model: { provider: selectedModel.provider, modelId: selectedModel.modelId },
      data: jsonObjectInput(value.data, '数据输入'), policy: policyInput(value.policy) });
    intent.current = request; setCaptured(request); mutation.mutate(request.body);
  }
  function reset() {
    if (!mutation.isSuccess) return;
    intent.current = undefined; setCaptured(undefined); hadUnknown.current = false; setDirty(false);
    mutation.reset(); form.resetFields();
  }
  useEffect(() => { if (draftVersion > 0 && mutation.isSuccess) reset(); }, [draftVersion]);
  const jsonRule = (label: string, policy = false) => ({ validator: (_: unknown, value: string | undefined) => {
    try { if (policy) policyInput(value ?? ''); else jsonObjectInput(value ?? '', label); return Promise.resolve(); }
    catch (error) { return Promise.reject(error); }
  } });
  return <Space orientation="vertical" className="full-width" size="middle">
    <div className="section-toolbar"><div><h2>提交研究目标</h2><p>模型负责插件通用研究；数据与政策由你明确提供。</p></div><Tag>原生 pi-durable</Tag></div>
    {models.isPending && <div role="status">正在读取可用模型…</div>}
    <ErrorNotice error={models.error} retry={() => { void models.refetch(); }} />
    {models.data?.length === 0 && <Alert type="warning" showIcon title="Host 尚无可用模型" description={<>
      在运行 research Host 的服务环境中配置官方 pi-ai Provider 凭据（例如 OPENAI_API_KEY 或 ANTHROPIC_API_KEY），或在 QZ_RESEARCH_CONFIG 指向的配置文件中设置 providerModules，然后重启 Host 并刷新模型列表。此页面不收集模型密钥，也不使用 Codex 登录。
    </>} action={<Button onClick={() => { void models.refetch(); }} loading={models.isFetching}>刷新模型列表</Button>} />}
    <ErrorNotice error={mutation.error} />
    {held && mutation.isError && <Alert type="warning" showIcon title="原请求已保留，提交结果待确认" description="重试会发送完全相同的 requestId 与请求正文。请勿另开页面重复提交；本页不把超时视为失败，也不自动重试。" />}
    {mutation.data && <Alert type="success" showIcon title="研究提交已被接纳" description={<>
      Conversation #{mutation.data.conversationId} · Submission #{mutation.data.submissionId}。这是接纳回执，不是计算成功证明。
    </>} action={<Button onClick={() => { if (mutation.data) onAdmitted(mutation.data); }}>查看研究</Button>} />}
    <Form form={form} layout="vertical" onValuesChange={() => setDirty(true)} onFinish={submit} disabled={!!captured || mutation.isPending}>
      <Form.Item name="goal" label="研究目标" rules={[{ required: true, whitespace: true, message: '请明确填写研究目标。' }]}>
        <Input.TextArea autoSize={{ minRows: 3, maxRows: 12 }} placeholder="描述要探索的问题、预期证据与停止条件" />
      </Form.Item>
      <Form.Item name="model" label="可用模型" rules={[{ required: true, message: '请选择 Host 实际可用的模型。' }]}>
        <Select aria-label="可用模型" virtual={false} showSearch optionFilterProp="label" placeholder="选择 provider / model" loading={models.isFetching}
          disabled={!!captured || models.isPending || models.isError || models.data?.length === 0} options={models.data?.map(model => ({
            value: JSON.stringify([model.provider, model.modelId]), label: `${model.name} · ${model.provider}/${model.modelId}${model.reasoning ? ' · reasoning' : ''}`,
          }))} />
      </Form.Item>
      <div className="durable-input-grid">
        <Form.Item name="data" label="数据输入 · JSON 对象" extra="按 Host 已安装插件提供真实数据引用和参数，不自动选择数据集。" rules={[jsonRule('数据输入')]}>
          <Input.TextArea className="json-input" autoSize={{ minRows: 6, maxRows: 22 }} placeholder="输入插件要求的 data 对象" spellCheck={false} />
        </Form.Item>
        <Form.Item name="policy" label="研究政策 · JSON 对象" extra="按插件提供预算与边界；max_trials 如提供，必须为正整数。无额外政策时显式填写 {}。" rules={[jsonRule('研究政策', true)]}>
          <Input.TextArea className="json-input" autoSize={{ minRows: 6, maxRows: 22 }} placeholder="输入 policy 对象" spellCheck={false} />
        </Form.Item>
      </div>
      <p className="section-caption">HTTP 202 仅表示接纳。原生 Host 持续执行；浏览器刷新 GET 只是观察，不是调度器。</p>
      {!captured && <Button type="primary" htmlType="submit" disabled={!canSubmit}>提交研究</Button>}
    </Form>
    {captured && <>
      <Typography.Text className="break-word" copyable>requestId：{captured.body.requestId}</Typography.Text>
      <NativeJson label="本次提交 · 精确请求正文" value={captured.body} />
      {mutation.isPending && <div role="status">正在等待原生提交回执…</div>}
      {held && mutation.isError && <Button type="primary" disabled={!online || mutation.isPending} loading={mutation.isPending}
        onClick={() => { if (intent.current) mutation.mutate(intent.current.body); }}>重试同一研究请求</Button>}
      {mutation.isSuccess && <Button onClick={reset}>清空表单，开始新的研究</Button>}
    </>}
  </Space>;
}

function ResearchDetail({ id }: { id: number }) {
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const query = useQuery({ queryKey: [...rootKey, 'view', id], refetchInterval: online ? 5_000 : false,
    queryFn: async ({ signal }) => viewOf(dataOf(await api.GET('/api/v2/research/{id}', { params: { path: { id } }, signal })), id) });
  const stop = useMutation({ mutationFn: async () => dataOf(await api.POST('/api/v2/research/{id}/stop', { params: { path: { id } } })),
    onSuccess: () => { void client.invalidateQueries({ queryKey: rootKey }); } });
  useGuard(stop.isPending);
  const value = query.data; const input = object(value?.research.input); const live = object(value?.docs['pi.live']);
  const run = object(live?.run); const generation = object(live?.generation);
  function requestStop() {
    modal.confirm({ title: `请求停止研究 #${id}？`, content: '请求会传给原生 Host，包括后台任务。接纳请求不证明任务已取消；以之后的原生任务快照为准。',
      okText: '请求停止', cancelText: '继续观察', okButtonProps: { danger: true }, onOk: () => { stop.mutate(); } });
  }
  return <section className="durable-detail" aria-label={`研究 #${id} 详情`}>
    <div className="section-toolbar"><div><span className="eyebrow">CONVERSATION #{id}</span><h2>研究详情</h2></div>
      <Space wrap><Button icon={<ReloadOutlined aria-hidden />} loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新研究</Button>
        <Button danger disabled={!online || !value || query.isError || query.isFetching || stop.isPending || stop.isSuccess} loading={stop.isPending} onClick={requestStop}>请求停止</Button></Space></div>
    <ErrorNotice error={stop.error} />
    {stop.isSuccess && <Alert type="warning" showIcon title="已请求停止，等待原生状态确认" description="HTTP 202 不是取消证明。继续查看 pi.live 与 Host 任务快照；本地请求结束不等于远端任务结束。" />}
    {stop.isError && <Alert type="warning" showIcon title="停止结果未确认" description="请刷新原生记录。再次点击请求停止只重发停止操作，不会新建研究。" />}
    <QueryPanel pending={query.isPending} error={query.error} stale={!!value} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" className="full-width" size="large">
        <div className="durable-goal"><h3>{typeof input?.goal === 'string' ? input.goal : '原生研究目标未报告'}</h3>
          <p className="section-caption">模型：{nativeText(object(input?.model)?.provider)} / {nativeText(object(input?.model)?.modelId)}</p></div>
        <section className="native-live" aria-label="原生活动状态"><h3>原生活动状态 · pi.live</h3>
          <Descriptions size="small" column={1} items={[
            { key: 'run', label: 'run.taskId', children: run ? nativeText(run.taskId) : '快照中无活动 run（不代表研究成功或已取消）' },
            { key: 'attempt', label: 'generation.attempt', children: nativeText(generation?.attempt) },
            { key: 'retry', label: 'generation.retry', children: nativeText(generation?.retry) },
            { key: 'deferred', label: 'generation.deferred', children: nativeText(generation?.deferred) },
          ]} />
          <NativeJson label="pi.live · 原生 JSON" value={value.docs['pi.live']} />
        </section>
        <ResearchTrials trials={value.research.trials} />
        <ResearchFinal value={value.research.final} />
        <details className="native-json"><summary>本研究测量用量 · pi.usage</summary><NativeUsage value={value.docs['pi.usage']} label="本研究用量" /></details>
        <NativeJson label="研究输入 · 原生 data / policy" value={value.research.input} />
        <NativeJson label="完整原生 ConversationView · entries / docs / research" value={value.raw} />
      </Space>}
    </QueryPanel>
  </section>;
}

function HostSnapshot() {
  const online = useOnline();
  const tasks = useQuery({ queryKey: [...rootKey, 'tasks'], refetchInterval: online ? 10_000 : false,
    queryFn: async ({ signal }) => tasksOf(dataOf(await api.GET('/api/v2/research/tasks', { signal }))) });
  const usage = useQuery({ queryKey: [...rootKey, 'usage'], refetchInterval: online ? 10_000 : false,
    queryFn: async ({ signal }) => usageOf(dataOf(await api.GET('/api/v2/research/usage', { signal }))) });
  return <details className="durable-host"><summary>Host 任务与用量 · 所有研究</summary>
    <div className="durable-host-grid">
      <section aria-labelledby="native-tasks-title"><div className="section-toolbar"><div><h2 id="native-tasks-title">原生任务快照</h2><p>Harness.inspect · 活动 / 可运行图，不是完整任务历史。</p></div>
        <Button aria-label="刷新任务快照" icon={<ReloadOutlined aria-hidden />} loading={tasks.isFetching} onClick={() => { void tasks.refetch(); }} /></div>
        <QueryPanel pending={tasks.isPending} error={tasks.error} stale={!!tasks.data} reload={() => { void tasks.refetch(); }}>
          {tasks.data && <>
            <Typography.Text>scheduling：{nativeText(tasks.data.scheduling)}</Typography.Text>
            {tasks.data.tasks.length === 0 ? <p className="section-caption">快照中没有未结束任务，不推定成功或已取消。</p> : <div className="native-tasks">{tasks.data.tasks.map((task, index) => {
              const record = object(task.record); const state = object(task.state);
              return <article key={index}><div className="record-label"><strong>Task #{nativeText(record?.id)}</strong><Tag>{nativeText(state?.kind)}</Tag></div>
                <Descriptions size="small" column={1} items={[
                  { key: 'kind', label: 'record.kind', children: nativeText(record?.kind) },
                  { key: 'conversation', label: 'conversationId', children: nativeText(record?.conversationId) },
                  { key: 'state', label: 'record.state.status', children: nativeText(object(record?.state)?.status) },
                  { key: 'abort', label: 'abortRequested（非取消证明）', children: nativeText(record?.abortRequested) },
                  { key: 'background', label: 'background', children: nativeText(record?.background) },
                  { key: 'on', label: '等待 on', children: nativeText(state?.on) },
                  { key: 'reason', label: '原生阻塞原因', children: nativeText(state?.reason) },
                  { key: 'error', label: '原生任务错误', children: nativeText(state?.error) },
                ]} /><NativeJson label={`Task #${nativeText(record?.id)} · 原生 JSON`} value={task} />
              </article>;
            })}</div>}
            <NativeJson label="Harness.inspect · 完整任务与 submissions JSON" value={tasks.data} />
          </>}
        </QueryPanel>
      </section>
      <section aria-labelledby="native-usage-title"><div className="section-toolbar"><div><h2 id="native-usage-title">Host 测量用量</h2><p>Harness.usage · 所有会话累计，非本研究独占。</p></div>
        <Button aria-label="刷新 Host 用量" icon={<ReloadOutlined aria-hidden />} loading={usage.isFetching} onClick={() => { void usage.refetch(); }} /></div>
        <QueryPanel pending={usage.isPending} error={usage.error} stale={!!usage.data} reload={() => { void usage.refetch(); }}>
          {usage.data && <NativeUsage value={usage.data.raw} label="Host 用量" />}
        </QueryPanel>
      </section>
    </div>
  </details>;
}
