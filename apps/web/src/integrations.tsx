import { Alert, App, Button, Card, Descriptions, Form, Input, Modal, Select, Space, Switch, Table, Tabs, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, useClock, useGuard, useOnline } from './ui';

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

/** Secret plaintext is confined to this transient field and the write-only request.
 * The query/mutation caches and browser storage never receive the secret as state. */
function SecretReference({ value, onChange, purpose, configured, disabled, onBusy }: {
  value?: string | null; onChange?: (id: string | undefined) => void;
  purpose: 'RUNTIME' | 'DOWNSTREAM' | 'TLS_CA'; configured: boolean;
  disabled: boolean; onBusy: (busy: boolean) => void;
}) {
  const [secret, setSecret] = useState(''); const [pending, setPending] = useState(false); const [error, setError] = useState<unknown>();
  const active = useRef(false); const intent = useRef(new Intent()); const online = useOnline();
  const isCa = purpose === 'TLS_CA';
  const valid = isCa ? secret.length >= 1 && secret.length <= 65536 && /^[\x00-\x7f]+(?![\s\S])/.test(secret)
    : secret.length >= (purpose === 'RUNTIME' ? 32 : 1) && secret.length <= 8192 && /^[!-~]+(?![\s\S])/.test(secret);
  async function register() {
    if (active.current || !online || !valid || disabled) return;
    active.current = true; setPending(true); onBusy(true); setError(undefined);
    const common = { schema_version: 1 as const, label: isCa ? 'Runtime CA certificate' : `${purpose} service credential` };
    const body: Schema['IntegrationSecretCreate'] = purpose === 'RUNTIME'
      ? { intent: { ...common, purpose: 'RUNTIME' }, value: secret }
      : purpose === 'DOWNSTREAM' ? { intent: { ...common, purpose: 'DOWNSTREAM' }, value: secret }
        : { intent: { ...common, purpose: 'TLS_CA' }, value: secret };
    try {
      const result = dataOf(await api.POST('/api/v2/settings/credentials', {
        body, params: { header: intent.current.headers('POST','/api/v2/settings/credentials',body) },
      }));
      onChange?.(result.resource.id); setSecret(''); intent.current.clear();
    } catch (error) { setError(error); }
    finally { active.current = false; setPending(false); onBusy(false); }
  }
  return <Space orientation="vertical" className="full-width">
    {configured && !value && <Typography.Text type="secondary">保留已有的原生凭据；界面不会读取或回显其值。</Typography.Text>}
    {value && <Alert type="success" showIcon title="新凭据已登记，提交配置后才会绑定。" description={<Space wrap><Typography.Text copyable>{value}</Typography.Text><Button size="small" disabled={disabled || pending} onClick={() => onChange?.(undefined)}>放弃本次绑定</Button></Space>} />}
    {isCa ? <Input.TextArea aria-label="新的 CA PEM 证书" value={secret} onChange={event => setSecret(event.target.value)} rows={5} maxLength={65536} disabled={disabled || pending || !online} autoComplete="off" />
      : <Input.Password aria-label={`新的 ${purpose} 凭据`} value={secret} onChange={event => setSecret(event.target.value)} maxLength={8192} disabled={disabled || pending || !online} autoComplete="new-password" />}
    <Typography.Text type="secondary">{isCa ? '最多 65536 个 ASCII 字节；证书还必须通过服务器原生 PEM 解析。' : `${purpose === 'RUNTIME' ? '32' : '1'}–8192 个无空白可打印 ASCII 字节。仅输入目标服务已设置的凭据，不要填写券商密码或钱包私钥。`}</Typography.Text>
    <Button onClick={() => { void register(); }} loading={pending} disabled={!online || !valid || disabled}>登记{isCa ? '证书' : '凭据'}</Button>
    <ErrorNotice error={error} />
  </Space>;
}

function RuntimeDialog({ original, close }: { original?: Runtime; close: () => void }) {
  type Values = Schema['RuntimeConfigurationV1'] & { credential_ref?: string; ca_certificate_ref?: string };
  const [form] = Form.useForm<Values>(); const [secretBusy, setSecretBusy] = useState(false);
  const online = useOnline(); const intent = useRef(new Intent()); const refresh = useRefresh(); const { modal } = App.useApp();
  const tls = Form.useWatch('tls_policy', form) ?? original?.configuration.tls_policy ?? 'SYSTEM_CA';
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const common = { name: values.name, endpoint: values.endpoint, allowed_capabilities: values.allowed_capabilities, enabled: values.enabled };
    if (original) {
      const base = { schema_version: 1 as const, expected_revision: original.revision, credential_ref: values.credential_ref ?? null };
      const body: Schema['RuntimeUpdate'] = values.tls_policy === 'PINNED_CA'
        ? { ...base, configuration: { ...common, tls_policy: 'PINNED_CA', development_http: false }, ca_certificate_ref: values.ca_certificate_ref ?? null }
        : { ...base, configuration: { ...common, tls_policy: 'SYSTEM_CA', development_http: values.development_http }, ca_certificate_ref: null };
      return dataOf(await api.PATCH('/api/v2/integrations/runtimes/{id}', { body, params: { path: { id: original.id },
        header: intent.current.headers('PATCH',`/api/v2/integrations/runtimes/${original.id}`,body) } }));
    }
    if (!values.credential_ref || (values.tls_policy === 'PINNED_CA' && !values.ca_certificate_ref)) throw new ApiFailure('LOCAL_VALIDATION_ERROR','请先登记本次必需的凭据和证书。');
    const body: Schema['RuntimeCreate'] = values.tls_policy === 'PINNED_CA'
      ? { schema_version: 1, credential_ref: values.credential_ref, ca_certificate_ref: values.ca_certificate_ref!, configuration: { ...common, tls_policy: 'PINNED_CA', development_http: false } }
      : { schema_version: 1, credential_ref: values.credential_ref, ca_certificate_ref: null, configuration: { ...common, tls_policy: 'SYSTEM_CA', development_http: values.development_http } };
    return dataOf(await api.POST('/api/v2/integrations/runtimes', { body, params: { header: intent.current.headers('POST','/api/v2/integrations/runtimes',body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  const pending = mutation.isPending || secretBusy;
  useGuard(true);
  function cancel() {
    if (pending) return;
    if (form.isFieldsTouched()) modal.confirm({ title: '放弃尚未提交的 Runtime 配置？', content: '已经登记的凭据不会被删除，已有 Run 的冻结配置也不会改变。', okText: '确认放弃', cancelText: '继续编辑', onOk: close });
    else close();
  }
  return <Modal open width={760} title={original ? '修改 Runtime 配置' : '登记 Runtime'} maskClosable={false} closable={!pending}
    onCancel={cancel} onOk={() => { if (online && !pending) form.submit(); }} confirmLoading={mutation.isPending}
    okText="保存配置" cancelText="返回" okButtonProps={{ disabled: !online || secretBusy }}>
    <Alert type="info" showIcon title="保存配置不代表真实可用。保存后须执行原生探测；配置变更会使旧探测失效。" />
    {original && <ResourceFacts id={original.id} revision={original.revision} updated={original.updated_at} />}
    <Form form={form} layout="vertical" initialValues={original ? original.configuration : { tls_policy: 'SYSTEM_CA', enabled: true, development_http: false, allowed_capabilities: ['DATA_VALIDATE'] }} disabled={pending || !online} onFinish={values => mutation.mutate(values)}>
      <Form.Item name="name" label="名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="endpoint" label="Runtime HTTPS origin" rules={[required, { max: 2048 }]} extra="只填协议、主机和端口；不填 /runtime/v1 路径、用户名、查询参数或片段。实际连接还受部署允许列表限制。"><Input maxLength={2048} placeholder="https://runtime.example" /></Form.Item>
      <Form.Item name="tls_policy" label="TLS 信任方式" rules={[required]}><Select onChange={() => form.setFieldValue('development_http', false)} options={[
        { value: 'SYSTEM_CA', label: '系统可信 CA' }, { value: 'PINNED_CA', label: '指定 CA 证书' },
      ]} /></Form.Item>
      <Form.Item name="credential_ref" label={original ? '轮换 Runtime 凭据（不登记则保留）' : 'Runtime 服务凭据'} rules={original ? [] : [required]}>
        <SecretReference purpose="RUNTIME" configured={original?.credential_configured ?? false} disabled={pending || !online} onBusy={setSecretBusy} />
      </Form.Item>
      {tls === 'PINNED_CA' && <Form.Item name="ca_certificate_ref" label="指定 CA 证书" preserve={false}
        rules={original?.configuration.tls_policy === 'PINNED_CA' && original.ca_configured ? [] : [required]}>
        <SecretReference purpose="TLS_CA" configured={original?.configuration.tls_policy === 'PINNED_CA' && original.ca_configured} disabled={pending || !online} onBusy={setSecretBusy} />
      </Form.Item>}
      <Form.Item name="allowed_capabilities" label="允许的任务类型" rules={[required]} extra="这是人工意图；实际可执行类型必须同时出现在新鲜原生探测中。"><Select mode="multiple" options={jobs} /></Form.Item>
      <Form.Item name="enabled" label="允许新任务" valuePropName="checked"><Switch /></Form.Item>
      <Form.Item name="development_http" label="显式本机 HTTP 开发模式" valuePropName="checked" extra="仅 SYSTEM_CA 与字面量 loopback 地址可用；部署也必须显式允许。"><Switch disabled={tls === 'PINNED_CA' || pending || !online} /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function RuntimeDetails({ id }: { id: string }) {
  const online = useOnline(); const time = useClock(); const [editing, setEditing] = useState<Runtime>(); const intent = useRef(new Intent()); const refresh = useRefresh();
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
          <Button type="primary" loading={probe.isPending} disabled={!online || query.isError || !runtime.configuration.enabled} onClick={() => probe.mutate(runtime)}>执行原生探测</Button></Space>
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
          { key: 'wall', label: '墙钟时间上限（秒）', children: observation.outcome.capabilities.max_wall_seconds },
          { key: 'bytes', label: '输出上限（字节）', children: observation.outcome.capabilities.max_output_bytes },
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
      <Table<Runtime> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="尚无 Runtime。先按运行文档启动远端原生网关，再登记其地址、凭据与任务范围。" /> }} columns={[
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
  const [form] = Form.useForm<Values>(); const [secretBusy, setSecretBusy] = useState(false); const online = useOnline(); const intent = useRef(new Intent()); const refresh = useRefresh();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const configuration: Schema['DownstreamConfigurationV1'] = { name: values.name, endpoint: values.endpoint, accepted_package_versions: ['1'], environments: values.environments, enabled: values.enabled, development_http: values.development_http };
    if (original) {
      const body: Schema['DownstreamUpdate'] = { schema_version: 1, expected_revision: original.revision, configuration, credential_ref: values.credential_ref ?? null };
      return dataOf(await api.PATCH('/api/v2/integrations/downstreams/{id}', { body, params: { path: { id: original.id }, header: intent.current.headers('PATCH',`/api/v2/integrations/downstreams/${original.id}`,body) } }));
    }
    if (!values.credential_ref) throw new ApiFailure('LOCAL_VALIDATION_ERROR','请先登记下游服务凭据。');
    const body: Schema['DownstreamCreate'] = { schema_version: 1, configuration, credential_ref: values.credential_ref };
    return dataOf(await api.POST('/api/v2/integrations/downstreams', { body, params: { header: intent.current.headers('POST','/api/v2/integrations/downstreams',body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  const pending = mutation.isPending || secretBusy; useGuard(true);
  return <Modal open title={original ? '修改目标交付下游' : '登记目标交付下游'} width={760} maskClosable={false} closable={!pending}
    onCancel={() => { if (!pending) close(); }} onOk={() => { if (online && !pending) form.submit(); }} confirmLoading={mutation.isPending}
    okText="保存下游配置" cancelText="返回" okButtonProps={{ disabled: !online || secretBusy }}>
    <Alert type="warning" showIcon title="这里只登记 target-only 目标包接收服务。" description="不保存券商账户、钱包私钥、真实订单或仓位。停用只阻止未来交付，不会撤单、平仓或停止已经领取目标的交易。" />
    {original && <ResourceFacts id={original.id} revision={original.revision} updated={original.updated_at} />}
    <Form form={form} layout="vertical" disabled={pending || !online} initialValues={original?.configuration ?? { environments: 'PAPER', enabled: true, development_http: false }} onFinish={values => mutation.mutate(values)}>
      <Form.Item name="name" label="下游名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="endpoint" label="下游 HTTPS origin" rules={[required, { max: 2048 }]}><Input maxLength={2048} placeholder="https://downstream.example" /></Form.Item>
      <Form.Item name="environments" label="允许环境" rules={[required]}><Select options={[
        { value: 'PAPER', label: '仅 Paper' }, { value: 'LIVE', label: '仅 Live' }, { value: 'BOTH', label: 'Paper 与 Live（仍须分别审批）' },
      ]} /></Form.Item>
      <Form.Item name="credential_ref" label={original ? '轮换下游服务凭据（可保留）' : '下游服务凭据'} rules={original ? [] : [required]}>
        <SecretReference purpose="DOWNSTREAM" configured={original?.credential_configured ?? false} disabled={pending || !online} onBusy={setSecretBusy} />
      </Form.Item>
      <Typography.Paragraph>本系统原生目标包版本为 1；保存配置并不证明接收端兼容或已经执行。</Typography.Paragraph>
      <Form.Item name="enabled" label="允许未来目标交付" valuePropName="checked"><Switch /></Form.Item>
      <Form.Item name="development_http" label="显式本机 HTTP 开发模式" valuePropName="checked"><Switch /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function Downstreams() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [editing, setEditing] = useState<{ original?: Downstream }>(); const online = useOnline();
  const query = useQuery({ queryKey: ['integrations','downstreams',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/downstreams', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width">
    <Button type="primary" disabled={!online} onClick={() => setEditing({})}>登记目标交付下游</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Downstream> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="尚未登记下游服务。没有有效下游、兼容探测、独立资格和相应审批，不会交付目标。" /> }} columns={[
        { title: '名称', key: 'name', render: (_, item) => item.configuration.name }, { title: '服务地址', key: 'endpoint', render: (_, item) => item.configuration.endpoint },
        { title: '环境', key: 'environment', render: (_, item) => item.configuration.environments }, { title: '版本', dataIndex: 'revision' },
        { title: '状态', key: 'enabled', render: (_, item) => item.configuration.enabled ? '允许未来交付' : '已停用' },
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
