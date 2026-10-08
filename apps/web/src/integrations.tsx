import { Alert, Button, Card, Descriptions, Form, Input, Modal, Select, Space, Switch, Table, Tabs, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, useClock, useOnline } from './ui';
import { useFormAutosave } from './settings-autosave';
import { setSettingsWork, useSettingsWorkKey } from './settings-work';
import { useSettingsCommand } from './settings-command';
import { activeTargetVersions, downstreamTargetStatus, targetDownstreamConfiguration } from './producer-views';
import { integrationSecretValueValid } from './integration-secret-value';

type Runtime = Schema['RuntimeView'];
type Downstream = Schema['DownstreamView'];
const required = { required: true, message: '请填写此项。' };
const jobs: { value: Schema['RunKind']; label: string }[] = [
  { value: 'DATA_VALIDATE', label: '数据验证及模型编译' },
  { value: 'AGENT_RESEARCH', label: '研究 Mission' },
  { value: 'ALPHA_EVALUATE', label: 'Alpha 科学计算' },
  { value: 'PORTFOLIO_BUILD', label: '组合优化' },
  { value: 'PORTFOLIO_SIMULATE', label: '共享资金模拟' },
  { value: 'FORWARD_EVALUATE', label: 'Forward 评估' },
  { value: 'EXPORT', label: '导出' }, { value: 'IMPORT', label: '导入' },
];
const readinessNames: Record<Schema['RuntimeReadinessState'], string> = {
  NOT_CHECKED: '尚未探测', DISABLED: '已停用', STALE: '探测已过期', UNAVAILABLE: '当前不可用', AVAILABLE: '有有效原生探测',
};

function useRefresh() {
  const client = useQueryClient();
  return async () => { await Promise.all([
    client.invalidateQueries({ queryKey: ['integrations'] }),
    client.invalidateQueries({ queryKey: ['data'] }),
  ]); };
}

type SecretState = { pending: boolean; unknown: boolean; ref?: string; error?: unknown };
class SecretSession {
  private intent = new Intent();
  private request?: Schema['IntegrationSecretCreate'];
  private listeners = new Set<() => void>();
  state: SecretState = { pending: false, unknown: false };
  constructor(readonly key: string) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  getSnapshot = () => this.state;
  private update(changes: Partial<SecretState>) {
    this.state = { ...this.state, ...changes };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`secret:${this.key}`, this.state.pending || this.state.unknown || !!this.state.ref);
  }
  async register(body?: Schema['IntegrationSecretCreate']) {
    if (this.state.pending || this.state.ref) return;
    this.request ??= body;
    const request = this.request;
    if (!request) return;
    this.update({ pending: true, unknown: false, error: undefined });
    try {
      const result = dataOf(await api.POST('/api/v2/settings/credentials', { body: request,
        params: { header: this.intent.headers('POST', '/api/v2/settings/credentials', request) } }));
      this.request = undefined; this.intent.clear();
      this.update({ pending: false, unknown: false, ref: result.resource.id, error: undefined });
    } catch (error) {
      const rejected = error instanceof ApiFailure && !!error.problem && error.status >= 400 && error.status < 500;
      const offline = error instanceof ApiFailure && error.code === 'OFFLINE';
      if (rejected || offline) { this.request = undefined; this.intent.clear(); }
      this.update({ pending: false, unknown: !rejected && !offline, error });
    }
  }
  abandon() {
    if (this.state.pending || this.state.unknown) return;
    this.request = undefined; this.intent.clear();
    this.update({ unknown: false, ref: undefined, error: undefined });
  }
  consume(ref: string) { if (this.state.ref === ref) this.update({ ref: undefined, error: undefined }); }
}
const secretSessions = new Map<string, SecretSession>();
function secretSessionFor(key: string) {
  let session = secretSessions.get(key);
  if (!session) { session = new SecretSession(key); secretSessions.set(key, session); }
  return session;
}
function consumeSecretSession(key: string, ref: string | undefined) { if (ref) secretSessions.get(key)?.consume(ref); }

/** Secret plaintext stays in this field or an unresolved in-memory request.
 * The query/mutation caches and browser storage never receive it. */
export function SecretReference({ value, onChange, purpose, configured, disabled, onBusy, sessionKey }: {
  value?: string | null; onChange?: (id: string | undefined) => void;
  purpose: 'RUNTIME' | 'DOWNSTREAM' | 'TLS_CA'; configured: boolean;
  disabled: boolean; onBusy: (busy: boolean) => void; sessionKey: string;
}) {
  const [secret, setSecret] = useState(''); const online = useOnline();
  const session = secretSessionFor(sessionKey);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useEffect(() => { onBusy(state.pending); }, [onBusy, state.pending]);
  useEffect(() => { if (state.ref && value !== state.ref) onChange?.(state.ref); }, [state.ref, value, onChange]);
  useEffect(() => { if (state.ref) setSecret(''); }, [state.ref]);
  const isCa = purpose === 'TLS_CA';
  const valid = integrationSecretValueValid(purpose, secret);
  function register() {
    if (state.pending || state.unknown || state.ref || !online || !valid || disabled) return;
    onBusy(true);
    const common = { schema_version: 1 as const, label: isCa ? 'Runtime CA certificate' : `${purpose} service credential` };
    const body: Schema['IntegrationSecretCreate'] = purpose === 'RUNTIME'
      ? { intent: { ...common, purpose: 'RUNTIME' }, value: secret }
      : purpose === 'DOWNSTREAM' ? { intent: { ...common, purpose: 'DOWNSTREAM' }, value: secret }
          : { intent: { ...common, purpose: 'TLS_CA' }, value: secret };
    void session.register(body);
  }
  return <Space orientation="vertical" className="full-width">
    {configured && !value && <Typography.Text type="secondary">已配置</Typography.Text>}
    {(value || state.ref) && <Alert type="success" showIcon title="凭据已登记" description={<Space wrap><Typography.Text copyable>{value || state.ref}</Typography.Text><Button size="small" disabled={disabled || state.pending} onClick={() => { session.abandon(); onChange?.(undefined); }}>放弃本次绑定</Button></Space>} />}
    {isCa ? <Input.TextArea aria-label="新的 CA PEM 证书" value={secret} onChange={event => setSecret(event.target.value)} rows={5} disabled={disabled || state.pending || state.unknown || !!state.ref || !online} autoComplete="off" />
      : <Input.Password aria-label={`新的 ${purpose} 凭据`} value={secret} onChange={event => setSecret(event.target.value)} maxLength={8192} disabled={disabled || state.pending || state.unknown || !!state.ref || !online} autoComplete="new-password" />}
    <Button onClick={register} loading={state.pending} disabled={!online || !valid || disabled || state.unknown || !!state.ref}>登记{isCa ? '证书' : '凭据'}</Button>
    {state.unknown && <Button disabled={!online || state.pending} onClick={() => { onBusy(true); void session.register(); }}>重试登记</Button>}
    <ErrorNotice error={state.error} />
  </Space>;
}

function RuntimeDialog({ original, close }: { original?: Runtime; close: () => void }) {
  type Values = Schema['RuntimeConfigurationV1'] & { credential_ref?: string; ca_certificate_ref?: string };
  const [form] = Form.useForm<Values>(); const [secretBusy, setSecretBusy] = useState(false);
  const online = useOnline(); const refresh = useRefresh();
  const { command, state } = useSettingsCommand('runtime-create', 'Runtime 登记', original ? undefined : async () => { await refresh(); close(); });
  const credentialKey = `runtime:${original?.id ?? 'new'}:credential`;
  const caKey = `runtime:${original?.id ?? 'new'}:ca`;
  const caSession = secretSessionFor(caKey);
  const caState = useSyncExternalStore(caSession.subscribe, caSession.getSnapshot, caSession.getSnapshot);
  const tls = Form.useWatch('tls_policy', form) ?? original?.configuration.tls_policy ?? 'SYSTEM_CA';
  const autosave = useFormAutosave(form, original && `runtime:${original.id}`, (original?.configuration ?? {}) as Values,
    original?.revision, original?.updated_at, online && !!original, async (values, revision, writeIntent) => {
      if (!original) throw new Error('Runtime 不存在');
      const common = { name: values.name, endpoint: values.endpoint, allowed_capabilities: values.allowed_capabilities, enabled: values.enabled };
      const base = { schema_version: 1 as const, expected_revision: revision, credential_ref: values.credential_ref ?? null };
      const body: Schema['RuntimeUpdate'] = values.tls_policy === 'PINNED_CA'
        ? { ...base, configuration: { ...common, tls_policy: 'PINNED_CA', development_http: false }, ca_certificate_ref: values.ca_certificate_ref ?? null }
        : { ...base, configuration: { ...common, tls_policy: 'SYSTEM_CA', development_http: values.development_http }, ca_certificate_ref: null };
      const result = dataOf(await api.PATCH('/api/v2/integrations/runtimes/{id}', { body, params: { path: { id: original.id },
        header: writeIntent.headers('PATCH', `/api/v2/integrations/runtimes/${original.id}`, body) } }));
      consumeSecretSession(credentialKey, values.credential_ref);
      if (body.ca_certificate_ref) consumeSecretSession(caKey, body.ca_certificate_ref);
      await refresh();
      return result.resource;
    }, async () => {
      if (!original) throw new Error('Runtime 不存在');
      const current = dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: original.id } } }));
      await refresh();
      return { values: current.configuration as Values, revision: current.revision, updated_at: current.updated_at, resource: current };
    });
  useEffect(() => {
    if (caState.unknown && form.getFieldValue('tls_policy') !== 'PINNED_CA') form.setFieldValue('tls_policy', 'PINNED_CA');
  }, [caState.unknown, form]);
  const shown = (autosave.resource as Runtime | undefined) ?? original;
  const pending = (!original && state.pending) || secretBusy;
  function cancel() {
    if (secretBusy || (!original && pending)) return;
    if (original) void autosave.close().then(close);
    else close();
  }
  return <Modal open width={760} title={original ? '修改 Runtime 配置' : '登记 Runtime'} maskClosable={false} closable={!secretBusy && (!!original || !pending)}
    onCancel={cancel} onOk={() => { if (!original && online && !pending) { if (state.unknown) command.retry(); else form.submit(); } }} confirmLoading={!original && state.pending}
    footer={original ? <Button onClick={cancel}>关闭</Button> : undefined}
    okText={state.unknown && !original ? '重试当前操作' : '保存配置'} cancelText="返回" okButtonProps={{ disabled: !online || secretBusy }}>
    
    {original && <ResourceFacts id={original.id} revision={autosave.revision} updated={autosave.updated_at} />}
    <Form form={form} layout="vertical" initialValues={original ? original.configuration : { tls_policy: 'SYSTEM_CA', enabled: true, development_http: false, allowed_capabilities: ['DATA_VALIDATE'] }}
      disabled={!online || secretBusy || (!original && (pending || state.unknown))} onValuesChange={original ? autosave.change : undefined}
      onFinish={original ? undefined : values => {
        const common = { name: values.name, endpoint: values.endpoint, allowed_capabilities: values.allowed_capabilities, enabled: values.enabled };
        const body: Schema['RuntimeCreate'] = values.tls_policy === 'PINNED_CA'
          ? { schema_version: 1, credential_ref: values.credential_ref!, ca_certificate_ref: values.ca_certificate_ref!, configuration: { ...common, tls_policy: 'PINNED_CA', development_http: false } }
          : { schema_version: 1, credential_ref: values.credential_ref!, ca_certificate_ref: null, configuration: { ...common, tls_policy: 'SYSTEM_CA', development_http: values.development_http } };
        void command.submit(async () => {
          if (!values.credential_ref || (values.tls_policy === 'PINNED_CA' && !values.ca_certificate_ref))
            throw new ApiFailure('LOCAL_VALIDATION_ERROR', '请先登记本次必需的凭据和证书。');
          const result = dataOf(await api.POST('/api/v2/integrations/runtimes', { body,
            params: { header: command.intent.headers('POST', '/api/v2/integrations/runtimes', body) } }));
          consumeSecretSession(credentialKey, values.credential_ref);
          if (values.tls_policy === 'PINNED_CA') consumeSecretSession(caKey, values.ca_certificate_ref);
          return result.resource.id;
        }, async () => { await refresh(); close(); });
      }}>
      <Form.Item name="name" label="名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="endpoint" label="Runtime HTTPS origin" rules={[required, { max: 2048 }]}><Input maxLength={2048} placeholder="https://runtime.example" /></Form.Item>
      <Form.Item name="tls_policy" label="TLS 信任方式" rules={[required]}><Select disabled={caState.unknown} onChange={value => {
        form.setFieldValue('development_http', false);
        if (value === 'SYSTEM_CA') { caSession.abandon(); form.setFieldValue('ca_certificate_ref', undefined); }
      }} options={[
        { value: 'SYSTEM_CA', label: '系统可信 CA' }, { value: 'PINNED_CA', label: '指定 CA 证书' },
      ]} /></Form.Item>
      <Form.Item name="credential_ref" label={original ? '轮换 Runtime 凭据（不登记则保留）' : 'Runtime 服务凭据'} rules={original ? [] : [required]}>
        <SecretReference purpose="RUNTIME" sessionKey={credentialKey} configured={shown?.credential_configured ?? false} disabled={pending || (!original && state.unknown) || autosave.saving || autosave.uncertain || !online} onBusy={setSecretBusy} />
      </Form.Item>
      {tls === 'PINNED_CA' && <Form.Item name="ca_certificate_ref" label="指定 CA 证书" preserve={false}
        rules={shown?.configuration.tls_policy === 'PINNED_CA' && shown.ca_configured ? [] : [required]}>
        <SecretReference purpose="TLS_CA" sessionKey={caKey} configured={shown?.configuration.tls_policy === 'PINNED_CA' && !!shown.ca_configured} disabled={pending || (!original && state.unknown) || autosave.saving || autosave.uncertain || !online} onBusy={setSecretBusy} />
      </Form.Item>}
      <Form.Item name="allowed_capabilities" label="允许的任务类型" rules={[required]}><Select mode="multiple" options={jobs} /></Form.Item>
      <Form.Item name="enabled" label="允许新任务" valuePropName="checked"><Switch /></Form.Item>
      <Form.Item name="development_http" label="显式本机 HTTP 开发模式" valuePropName="checked"><Switch disabled={tls === 'PINNED_CA' || pending || !online} /></Form.Item>
      {original && autosave.saving && <Typography.Text role="status">正在保存</Typography.Text>}
      <ErrorNotice error={original ? autosave.error : state.error} />
      {original && !!autosave.error && <Button onClick={autosave.retry}>重试</Button>}
    </Form>
  </Modal>;
}

function RuntimeDetails({ id }: { id: string }) {
  const online = useOnline(); const time = useClock(); const [editing, setEditing] = useState<Runtime>(); const intent = useRef(new Intent()); const refresh = useRefresh();
  const runtimeSaving = useSettingsWorkKey(`autosave:runtime:${id}`);
  const query = useQuery({ queryKey: ['integrations','runtime',id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id } }, signal })) });
  const readiness = useQuery({ queryKey: ['integrations','readiness',id], refetchInterval: online ? 15000 : false, queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes/{id}/readiness', { params: { path: { id } }, signal })) });
  const probe = useMutation({ mutationFn: async (runtime: Runtime) => {
    const body: Schema['RuntimeProbeRequestV1'] = { schema_version: 1, expected_revision: runtime.revision };
    return dataOf(await api.POST('/api/v2/integrations/runtimes/{id}/probe', { body, params: { path: { id },
      header: intent.current.headers('POST',`/api/v2/integrations/runtimes/${id}/probe`,body) } }));
  }, onSuccess: async () => { intent.current.clear(); await refresh(); } });
  const runtime = query.data; const state = readiness.data; const observation = state?.latest_observation;
  const stale = observation !== null && observation !== undefined && Date.parse(observation.valid_until) <= time;
  return <Card title={runtime?.configuration.name ?? 'Runtime 详情'}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!runtime} reload={() => { void query.refetch(); }}>
      {runtime && <>
        <ResourceFacts id={runtime.id} revision={runtime.revision} updated={runtime.updated_at} />
        <Descriptions column={1} items={[
          { key: 'origin', label: '服务地址', children: runtime.configuration.endpoint },
          { key: 'tls', label: 'TLS', children: runtime.configuration.tls_policy },
          { key: 'credential', label: '凭据', children: runtime.credential_configured ? '已配置（不会回显）' : '未配置' },
          { key: 'enabled', label: '新任务', children: runtime.configuration.enabled ? '允许，仍须通过原生探测' : '已停用' },
        ]} />
        <Space wrap><Button disabled={!online || query.isError || probe.isPending} onClick={() => setEditing(runtime)}>修改配置</Button>
          <Button type="primary" loading={probe.isPending} disabled={!online || query.isError || runtimeSaving || !runtime.configuration.enabled} onClick={() => probe.mutate(runtime)}>执行原生探测</Button></Space>
        <ErrorNotice error={probe.error} />
      </>}
    </QueryPanel>
    <QueryPanel pending={readiness.isPending} error={readiness.error} stale={!!state} reload={() => { void readiness.refetch(); }}>
      {state && <Space orientation="vertical" className="full-width">
        <Tag>{stale && state.state === 'AVAILABLE' ? '已超过此观测有效期，请重新探测' : readinessNames[state.state]}</Tag>
        <Typography.Paragraph>当前共同支持的任务：{state.available_job_kinds.length ? state.available_job_kinds.map(kind => jobs.find(job => job.value === kind)?.label ?? kind).join('、') : '没有可准入的任务'}</Typography.Paragraph>
        {observation && <Descriptions column={1} items={[
          { key: 'observed', label: '真实观测时间', children: displayTime(observation.observed_at) },
          { key: 'expires', label: '有效至', children: displayTime(observation.valid_until) },
          { key: 'revision', label: '观测配置版本', children: observation.integration_revision },
          { key: 'outcome', label: '原生结果', children: observation.outcome.status === 'AVAILABLE' ? `Runtime ${observation.outcome.capabilities.runtime_version}` : observation.outcome.reason },
        ]} />}
        {observation?.outcome.status === 'AVAILABLE' && <Descriptions column={1} items={[
          { key: 'cpu', label: 'CPU 核数上限', children: observation.outcome.capabilities.max_cpu },
          { key: 'memory', label: '内存上限 MiB', children: observation.outcome.capabilities.max_memory_mib },
          { key: 'wall', label: '墙钟时间上限（秒）', children: observation.outcome.capabilities.max_wall_seconds ?? '未设置 Runtime 时间上限' },
          { key: 'bytes', label: '输出上限（字节）', children: observation.outcome.capabilities.max_output_bytes ?? '不限' },
          { key: 'engine', label: '原生引擎', children: Object.entries(observation.outcome.capabilities.engine_versions).map(([name, version]) => `${name}: ${version}`).join('；') },
        ]} />}
      </Space>}
    </QueryPanel>
    {editing && <RuntimeDialog original={editing} close={() => setEditing(undefined)} />}
  </Card>;
}

function Runtimes() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [creating, setCreating] = useState(false); const [selected, setSelected] = useState<string>(); const online = useOnline();
  const query = useQuery({ queryKey: ['integrations','runtimes',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="large">
    <Button type="primary" disabled={!online} onClick={() => setCreating(true)}>登记 Runtime</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Runtime> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="暂无 Runtime" /> }} columns={[
        { title: '名称', key: 'name', render: (_, item) => item.configuration.name },
        { title: '服务地址', key: 'endpoint', render: (_, item) => item.configuration.endpoint },
        { title: '版本', dataIndex: 'revision' }, { title: '新任务', key: 'enabled', render: (_, item) => item.configuration.enabled ? '允许' : '停用' },
        { title: '操作', key: 'show', render: (_, item) => <Button onClick={() => setSelected(item.id)}>配置与原生探测</Button> },
      ]} /><Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <RuntimeDetails key={selected} id={selected} />}
    {creating && <RuntimeDialog close={() => setCreating(false)} />}
  </Space>;
}

function DownstreamDialog({ original, close }: { original?: Downstream; close: () => void }) {
  type Values = Schema['DownstreamConfigurationV1'] & { credential_ref?: string };
  const [form] = Form.useForm<Values>(); const [secretBusy, setSecretBusy] = useState(false); const online = useOnline();
  const refresh = useRefresh();
  const { command, state } = useSettingsCommand('downstream-create', '目标交付下游登记', original ? undefined : async () => { await refresh(); close(); });
  const credentialKey = `downstream:${original?.id ?? 'new'}:credential`;
  const autosave = useFormAutosave(form, original && `downstream:${original.id}`, (original?.configuration ?? {}) as Values,
    original?.revision, original?.updated_at, online && !!original, async (values, revision, writeIntent) => {
      if (!original) throw new Error('下游不存在');
      const configuration = targetDownstreamConfiguration({ name: values.name, endpoint: values.endpoint,
        accepted_package_versions: values.accepted_package_versions, environments: values.environments, enabled: values.enabled, development_http: values.development_http });
      const body: Schema['DownstreamUpdate'] = { schema_version: 1, expected_revision: revision, configuration, credential_ref: values.credential_ref ?? null };
      const result = dataOf(await api.PATCH('/api/v2/integrations/downstreams/{id}', { body, params: { path: { id: original.id },
        header: writeIntent.headers('PATCH', `/api/v2/integrations/downstreams/${original.id}`, body) } }));
      consumeSecretSession(credentialKey, values.credential_ref);
      void refresh();
      return result.resource;
    }, async () => {
      if (!original) throw new Error('下游不存在');
      const current = dataOf(await api.GET('/api/v2/integrations/downstreams/{id}', { params: { path: { id: original.id } } }));
      void refresh();
      return { values: current.configuration as Values, revision: current.revision, updated_at: current.updated_at, resource: current };
    });
  const shown = (autosave.resource as Downstream | undefined) ?? original;
  const pending = (!original && state.pending) || secretBusy;
  function cancel() {
    if (secretBusy || (!original && pending)) return;
    if (original) void autosave.close().then(close);
    else close();
  }
  return <Modal open title={original ? '修改目标交付下游' : '登记目标交付下游'} width={760} maskClosable={false} closable={!secretBusy && (!!original || !pending)}
    onCancel={cancel} onOk={() => { if (!original && online && !pending) { if (state.unknown) command.retry(); else form.submit(); } }} confirmLoading={!original && state.pending}
    footer={original ? <Button onClick={cancel}>关闭</Button> : undefined}
    okText={state.unknown && !original ? '重试当前操作' : '保存下游配置'} cancelText="返回" okButtonProps={{ disabled: !online || secretBusy }}>
    
    {original && <ResourceFacts id={original.id} revision={autosave.revision} updated={autosave.updated_at} />}
    {shown && !activeTargetVersions(shown.configuration.accepted_package_versions) && <Alert showIcon type="warning"
      title="该配置含历史目标包版本" description="保留原版本供查阅。保存前须明确移除 V1 并选择 V2；配置选择不会证明下游已经支持 V2，交付仍以原生就绪探测和审批为准。" />}
    <Form form={form} layout="vertical" disabled={!online || secretBusy || (!original && (pending || state.unknown))} initialValues={original?.configuration ?? { accepted_package_versions: ['2'], environments: 'PAPER', enabled: true, development_http: false }}
      onValuesChange={original ? autosave.change : undefined} onFinish={original ? undefined : values => {
        const configuration = targetDownstreamConfiguration({ name: values.name, endpoint: values.endpoint,
          accepted_package_versions: values.accepted_package_versions, environments: values.environments, enabled: values.enabled, development_http: values.development_http });
        const body: Schema['DownstreamCreate'] = { schema_version: 1, configuration, credential_ref: values.credential_ref! };
        void command.submit(async () => {
          if (!values.credential_ref) throw new ApiFailure('LOCAL_VALIDATION_ERROR', '请先登记下游服务凭据。');
          const result = dataOf(await api.POST('/api/v2/integrations/downstreams', { body,
            params: { header: command.intent.headers('POST', '/api/v2/integrations/downstreams', body) } }));
          consumeSecretSession(credentialKey, values.credential_ref);
          return result.resource.id;
        }, async () => { await refresh(); close(); });
      }}>
      <Form.Item name="name" label="下游名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="endpoint" label="下游 HTTPS origin" rules={[required, { max: 2048 }]}><Input maxLength={2048} placeholder="https://downstream.example" /></Form.Item>
      <Form.Item name="accepted_package_versions" label="接受的目标包版本" rules={[required, {
        validator: (_, versions: string[] | undefined) => activeTargetVersions(versions ?? []) ? Promise.resolve() : Promise.reject(new Error('请明确选择且仅保留 V2。')),
      }]}><Select<Schema['PackageSchemaVersion'][]> mode="multiple" options={[
        { value: '2', label: 'V2' },
      ]} labelRender={({ value }) => value === '1' ? 'V1（历史不可交付）' : 'V2'} /></Form.Item>
      <Form.Item name="environments" label="允许环境" rules={[required]}><Select options={[
        { value: 'PAPER', label: '仅 Paper' }, { value: 'LIVE', label: '仅 Live' }, { value: 'BOTH', label: 'Paper 与 Live（仍须分别审批）' },
      ]} /></Form.Item>
      <Form.Item name="credential_ref" label={original ? '轮换下游服务凭据（可保留）' : '下游服务凭据'} rules={original ? [] : [required]}>
        <SecretReference purpose="DOWNSTREAM" sessionKey={credentialKey} configured={shown?.credential_configured ?? false} disabled={pending || (!original && state.unknown) || autosave.saving || autosave.uncertain || !online} onBusy={setSecretBusy} />
      </Form.Item>
      
      <Form.Item name="enabled" label="允许未来目标交付" valuePropName="checked"><Switch /></Form.Item>
      <Form.Item name="development_http" label="显式本机 HTTP 开发模式" valuePropName="checked"><Switch /></Form.Item>
      {original && autosave.saving && <Typography.Text role="status">正在保存</Typography.Text>}
      <ErrorNotice error={original ? autosave.error : state.error} />
      {original && !!autosave.error && <Button onClick={autosave.retry}>重试</Button>}
    </Form>
  </Modal>;
}

function Downstreams() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [editing, setEditing] = useState<{ original?: Downstream }>(); const online = useOnline();
  const query = useQuery({ queryKey: ['integrations','downstreams',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/downstreams', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width">
    <Button type="primary" disabled={!online} onClick={() => setEditing({})}>登记目标交付下游</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Downstream> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="暂无下游服务" /> }} columns={[
        { title: '名称', key: 'name', render: (_, item) => item.configuration.name }, { title: '服务地址', key: 'endpoint', render: (_, item) => item.configuration.endpoint },
        { title: '环境', key: 'environment', render: (_, item) => item.configuration.environments }, { title: '版本', dataIndex: 'revision' },
        { title: '目标包协议', key: 'protocol', render: (_, item) => item.configuration.accepted_package_versions.map(version => version === '1' ? 'V1（历史）' : 'V2').join('、') },
        { title: '状态', key: 'enabled', render: (_, item) => downstreamTargetStatus(item.configuration) },
        { title: '操作', key: 'edit', render: (_, item) => <Button disabled={!online || query.isError} onClick={() => setEditing({ original: item })}>修改下游</Button> },
      ]} /><Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {editing && <DownstreamDialog original={editing.original} close={() => setEditing(undefined)} />}
  </Space>;
}

export function IntegrationManagement() {
  const [tab, setTab] = useState('runtime');
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>原生集成</Typography.Title>
    <Tabs activeKey={tab} onChange={setTab} items={[{ key: 'runtime', label: '计算 Runtime' }, { key: 'downstream', label: '目标交付下游' }]} />
    {tab === 'runtime' ? <Runtimes /> : <Downstreams />}
  </Space>;
}
