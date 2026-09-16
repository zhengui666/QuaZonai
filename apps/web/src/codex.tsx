import { Alert, App, Button, Card, Descriptions, Form, Input, Modal, Radio, Select, Slider, Space, Switch, Table, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { SecretReference } from './integrations';
import { CodexAccount } from './codex-account';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, useClock, useGuard, useOnline } from './ui';

type Profile = Schema['CodexProfileViewV1'];
type Observation = Schema['CodexObservationV1'];
type Advertised = Schema['CodexAdvertisedModelV1'];
type Values = {
  name: string; home_binding?: string; mode: 'SYSTEM' | 'CUSTOM_PROVIDER'; base_url?: string; credential_ref?: string;
  use_default_model_settings: boolean; saved_model?: string | null; saved_reasoning_effort?: string | null; saved_fast_mode: boolean;
};
const required = { required: true, message: '请填写此项。' };
const failure: Record<Schema['CodexProbeFailureV1'], string> = {
  DEPLOYMENT_UNAVAILABLE: '原生账号目录或凭据绑定不可用，请核对部署配置。',
  NATIVE_UNAVAILABLE: '原生 Codex 连接未完成，请检查实例状态后重新探测。',
  VERSION_UNSUPPORTED: '原生 Codex 版本与当前协议不一致。',
  CONTRACT_UNSUPPORTED: '原生响应不满足已锁定的协议，不能推测其能力。',
  AUTHENTICATION_REQUIRED: '原生 Codex 尚未完成账号认证，请按运行文档在同一个账号目录中登录。',
  MODEL_SETTINGS_UNSUPPORTED: '实际模型目录不支持所保存的模型、推理强度或加速档位，或者原生响应没有采用这些设置。',
};
const states: Record<Schema['CodexObservationStateV1'], string> = {
  NEVER_PROBED: '尚未探测', STALE: '观测已失效', AVAILABLE: '原生连接及配置可用', UNAVAILABLE: '当前不可用',
};

function fresh(observation: Observation | undefined, profile: Profile | undefined, now: number): boolean {
  return !!profile && observation?.profile_id === profile.id && observation.state === 'AVAILABLE'
    && observation.profile_revision === profile.revision && observation.observation?.profile_revision === profile.revision
    && observation.observation.outcome.status === 'AVAILABLE' && Date.parse(observation.observation.valid_until) > now;
}

function useRefresh() {
  const client = useQueryClient();
  return async () => { await client.invalidateQueries({ queryKey: ['codex'] }); };
}

function ModelControls({ form, observation, profile, connectionUnchanged, disabled }: {
  form: ReturnType<typeof Form.useForm<Values>>[0]; observation?: Observation; profile?: Profile; connectionUnchanged: boolean; disabled: boolean;
}) {
  const now = useClock();
  const defaultMode = Form.useWatch('use_default_model_settings', form) ?? true;
  const selected = Form.useWatch('saved_model', form) as string | null | undefined;
  const effort = Form.useWatch('saved_reasoning_effort', form) as string | null | undefined;
  const savedFast = Form.useWatch('saved_fast_mode', form) ?? false;
  function setEffort(value: string | null) { form.setFields([{ name: 'saved_reasoning_effort', value, touched: true }]); }
  function clearModel() { form.setFields([{ name: 'saved_model', value: null, touched: true }]); }
  const valid = connectionUnchanged && fresh(observation, profile, now);
  const available = observation?.observation?.outcome.status === 'AVAILABLE' ? observation.observation.outcome : undefined;
  const models: Advertised[] = valid ? available?.models ?? [] : [];
  const selectedModel = models.find(item => item.capability.model === (selected || available?.effective.model));
  const efforts = selectedModel?.capability.supported_reasoning_efforts ?? [];
  const index = effort ? efforts.findIndex(item => item.reasoning_effort === effort) : -1;
  const fastSupported = selectedModel?.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast') ?? false;
  const options = models.map(item => ({ value: item.capability.model, label: `${item.capability.display_name}${item.capability.hidden ? '（原生目录隐藏项）' : ''}` }));
  if (selected && !options.some(option => option.value === selected)) options.unshift({ value: selected, label: `${selected}（已保存，未由当前有效目录确认）` });
  return <>
    <Form.Item name="use_default_model_settings" label="使用 Codex 原生默认模型设置" valuePropName="checked"
      extra="开启时不发送 model、effort 或加速档位覆盖；下方已保存值会保留，但不会执行。"><Switch /></Form.Item>
    {!valid && <Alert type="info" showIcon title="模型能力尚未由当前配置的有效原生探测确认。" description="可先保存连接，再执行探测。不会用固定型号、推理等级或目录中的默认标记猜测实际模型。" />}
    <Form.Item name="saved_model" label="保存的模型覆盖" extra="留空表示使用原生实际模型；推理强度仍可单独设置。">
      <Select allowClear showSearch optionFilterProp="label" options={options} disabled={disabled || defaultMode || !valid}
        placeholder={available?.effective.model ? `原生实际模型：${available.effective.model}` : '等待原生模型目录'}
        onChange={() => setEffort(null)} />
    </Form.Item>
    <Button disabled={disabled || defaultMode || !selected} onClick={clearModel}>清除模型覆盖</Button>
    <Form.Item name="saved_reasoning_effort" hidden><Input /></Form.Item>
    <Form.Item label="推理强度" extra={efforts[index]?.description ?? (effort ? `已保存 ${effort}，当前目录未确认。` : '未设置覆盖；保持 Codex 原生设置。')}>
      <Space orientation="vertical" className="full-width">
        <Typography.Text>{effort ? `保存值：${effort}` : '使用原生推理设置'}</Typography.Text>
        {efforts.length > 0 ? <Slider min={0} max={efforts.length} step={1} value={index >= 0 ? index + 1 : 0}
          ariaLabelForHandle="保存的推理强度" disabled={disabled || defaultMode || !valid}
          marks={{ 0: '原生', ...Object.fromEntries(efforts.flatMap((item, position) => efforts.length <= 6 || position === efforts.length - 1 || position === index ? [[position + 1, item.reasoning_effort]] : [])) }}
          tooltip={{ formatter: value => value === undefined ? '' : value === 0 ? '不覆盖原生设置' : efforts[value - 1]?.reasoning_effort ?? '' }}
          onChange={value => { const chosen = efforts[value - 1]; if (value === 0) setEffort(null); else if (chosen) setEffort(chosen.reasoning_effort); }} />
          : <Typography.Text type="secondary">等待原生推理选项</Typography.Text>}
        <Button disabled={disabled || defaultMode || !effort} onClick={() => setEffort(null)}>清除推理强度覆盖</Button>
      </Space>
    </Form.Item>
    <Form.Item name="saved_fast_mode" label="使用原生加速档位" valuePropName="checked"
      extra={fastSupported ? '实际选择当前模型公告的 priority 或 fast 档位；费用和额度仍由原生服务决定。' : '当前有效模型目录没有公告加速档位，不会替换模型或降低推理强度。'}>
      <Switch disabled={disabled || defaultMode || ((!valid || !fastSupported) && !savedFast)} />
    </Form.Item>
  </>;
}

function ProfileDialog({ original, observation, close }: { original?: Profile; observation?: Observation; close: () => void }) {
  const [form] = Form.useForm<Values>();
  const online = useOnline(); const { modal } = App.useApp(); const refresh = useRefresh(); const intent = useRef(new Intent());
  const [secretBusy, setSecretBusy] = useState(false);
  const [saveValues, setSaveValues] = useState<Values>();
  type Save = { update: Schema['CodexProfileUpdateV1'] } | { create: Schema['CodexProfileCreateV1'] };
  const sent = useRef<Save | undefined>(undefined);
  const hadUnknown = useRef(false);
  async function send(request: Save) {
    if ('update' in request && original) {
      const body = request.update;
      return dataOf(await api.PATCH('/api/v2/settings/codex/{id}', { body, params: { path: { id: original.id }, header: intent.current.headers('PATCH', `/api/v2/settings/codex/${original.id}`, body) } }));
    }
    if ('create' in request && !original) {
      const body = request.create;
      return dataOf(await api.POST('/api/v2/settings/codex', { body, params: { header: intent.current.headers('POST', '/api/v2/settings/codex', body) } }));
    }
    throw new ApiFailure('LOCAL_VALIDATION_ERROR', '保存目标已变化，请重新载入配置。');
  }
  const homes = useQuery({ queryKey: ['codex','homes'], enabled: !original, queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/homes', { signal })) });
  const mode = Form.useWatch('mode', form) ?? original?.connection_mode ?? 'SYSTEM';
  const baseUrl = Form.useWatch('base_url', form) ?? original?.custom_base_url ?? '';
  const credential = Form.useWatch('credential_ref', form);
  const unchanged = !!original && mode === original.connection_mode && (mode === 'SYSTEM' || (baseUrl === original.custom_base_url && !credential));
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    if (sent.current) return send(sent.current);
    const model_settings: Schema['SavedModelSettingsV1'] = {
      schema_version: 1, use_default_model_settings: values.use_default_model_settings,
      saved_model: values.saved_model || null, saved_reasoning_effort: values.saved_reasoning_effort || null, saved_fast_mode: values.saved_fast_mode,
    };
    if (values.mode === 'CUSTOM_PROVIDER' && (!values.base_url || (!values.credential_ref && original?.connection_mode !== 'CUSTOM_PROVIDER'))) {
      throw new ApiFailure('LOCAL_VALIDATION_ERROR', '请填写自定义 Provider 的 HTTPS 地址，并登记其独立凭据。');
    }
    if (original) {
      const connection: Schema['CodexConnectionUpdateV1'] = values.mode === 'SYSTEM' ? { mode: 'SYSTEM' }
        : { mode: 'CUSTOM_PROVIDER', base_url: values.base_url!, credential_ref: values.credential_ref ?? null };
      const body: Schema['CodexProfileUpdateV1'] = { schema_version: 1, expected_revision: original.revision, name: values.name, connection, model_settings };
      sent.current = { update: body };
      return send(sent.current);
    }
    const home = homes.data?.find(item => item.reference === values.home_binding);
    if (!home) throw new ApiFailure('LOCAL_VALIDATION_ERROR', '请选择当前部署已经登记的原生账号目录。');
    const connection: Schema['CodexConnectionCreateV1'] = values.mode === 'SYSTEM' ? { mode: 'SYSTEM' }
      : { mode: 'CUSTOM_PROVIDER', base_url: values.base_url!, credential_ref: values.credential_ref! };
    const body: Schema['CodexProfileCreateV1'] = { schema_version: 1, name: values.name, home_binding: home.reference, profile_origin: home.profile_origin, connection, model_settings };
    sent.current = { create: body };
    return send(sent.current);
  }, onSuccess: async () => { intent.current.clear(); await refresh(); close(); },
  onError: error => {
    const rejectedBeforeExecution = error instanceof ApiFailure
      && ((error.problem && error.status >= 400 && error.status < 500)
        || error.code === 'LOCAL_VALIDATION_ERROR' || error.code === 'OFFLINE');
    // A later authentication/revision rejection cannot disprove an earlier lost
    // acknowledgement. Keep that original intent frozen until its receipt returns.
    if (!rejectedBeforeExecution) hadUnknown.current = true;
    if (rejectedBeforeExecution && !hadUnknown.current) {
      sent.current = undefined;
      setSaveValues(undefined);
    }
  } });
  const pending = mutation.isPending || secretBusy;
  const unknownSave = !!saveValues && mutation.isError;
  const formDisabled = pending || !online || unknownSave;
  useGuard(true);
  function cancel() {
    if (pending) return;
    if (form.isFieldsTouched()) modal.confirm({ title: '放弃尚未保存的 Codex 设置？', content: '已登记的凭据不会被删除；现有研究会话也不会改用另一配置。', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
    else close();
  }
  return <Modal open title={original ? '修改 Codex 配置' : '登记 Codex 配置'} width={760} maskClosable={false} closable={!pending}
    onCancel={cancel} onOk={() => { if (online && !pending) { if (unknownSave && saveValues) mutation.mutate(saveValues); else form.submit(); } }}
    okText={unknownSave ? '重试同一保存请求' : '保存 Codex 配置'} cancelText="返回"
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || secretBusy || (!original && !unknownSave && (homes.isError || homes.isPending)) }}>
    <Alert type="info" showIcon title="原生账号、连接来源与模型设置相互独立。" description="SYSTEM 使用部署选定的 Codex 配置及原生登录，不要求网页填写 URL 或 API Key。保存配置不是探测成功，也不会启动推理或改变旧 Thread。" />
    {original && <ResourceFacts id={original.id} revision={original.revision} updated={original.updated_at} />}
    {unknownSave && <Alert type="warning" showIcon title="保存结果尚未确认。" description="原始请求及幂等键已保留；重试不会更换内容。关闭页面不撤销可能已经完成的保存。" />}
    <Form form={form} layout="vertical" disabled={formDisabled} initialValues={original ? {
      name: original.name, mode: original.connection_mode, base_url: original.custom_base_url ?? '', ...original.model_settings,
    } : { mode: 'SYSTEM', use_default_model_settings: true, saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false }} onFinish={values => { const request = structuredClone(values); setSaveValues(request); mutation.mutate(request); }}>
      <Form.Item name="name" label="配置名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      {original ? <Typography.Paragraph>原生账号目录绑定：{original.home_binding ?? '历史绑定尚未登记'}（不可修改；切换目录须登记新配置）</Typography.Paragraph>
        : <QueryPanel pending={homes.isPending} error={homes.error} stale={!!homes.data} reload={() => { void homes.refetch(); }}>
          <Form.Item name="home_binding" label="部署登记的原生账号目录" rules={[required]} extra="这里只显示部署标签，不读取或展示宿主路径。">
            <Select options={homes.data?.map(home => ({ value: home.reference, label: `${home.label}（${home.profile_origin === 'MANAGED_VOLUME' ? '实例管理卷' : '显式挂载'}）` }))} />
          </Form.Item>
          {homes.data?.length === 0 && <Alert type="warning" showIcon title="部署尚未登记 Codex 账号目录。" description="按运行文档设置 --codex-deployment 后重启服务；页面不会偷偷使用宿主 ~/.codex。" />}
        </QueryPanel>}
      <Form.Item name="mode" label="连接来源" rules={[required]}><Radio.Group options={[{ value: 'SYSTEM', label: 'SYSTEM：沿用原生 Codex' }, { value: 'CUSTOM_PROVIDER', label: '自定义 Provider' }]} /></Form.Item>
      {mode === 'CUSTOM_PROVIDER' && <>
        <Form.Item name="base_url" label="Provider HTTPS API 地址" rules={[required, { max: 2048 }, { pattern: /^https:\/\//, message: '必须使用 HTTPS。' }]} extra="可以包含 /v1 路径，但不能包含用户名、密钥、查询参数或片段。"><Input maxLength={2048} placeholder="https://provider.example/v1" /></Form.Item>
        <Form.Item name="credential_ref" label="独立 Provider 凭据" rules={original?.connection_mode === 'CUSTOM_PROVIDER' && original.credential_configured ? [] : [required]}>
          <SecretReference purpose="CUSTOM_PROVIDER" configured={!!original && original.connection_mode === 'CUSTOM_PROVIDER' && original.credential_configured} disabled={formDisabled} onBusy={setSecretBusy} />
        </Form.Item>
      </>}
      <ModelControls form={form} observation={observation} profile={original} connectionUnchanged={unchanged} disabled={formDisabled} />
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function ProfileDetails({ id }: { id: string }) {
  const [accountBusy, setAccountBusy] = useState(false);
  const online = useOnline(); const now = useClock(); const refresh = useRefresh(); const intent = useRef(new Intent()); const [editing, setEditing] = useState<Profile>();
  const query = useQuery({ queryKey: ['codex','profile',id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex/{id}', { params: { path: { id } }, signal })) });
  const observation = useQuery({ queryKey: ['codex','observation',id], refetchInterval: online ? 15000 : false, queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/models', { params: { query: { profile_id: id } }, signal })) });
  const probe = useMutation({ mutationFn: async (profile: Profile) => {
    const body: Schema['CodexProbeRequestV1'] = { schema_version: 1, profile_id: id, expected_revision: profile.revision };
    return dataOf(await api.POST('/api/v2/codex/probe', { body, params: { header: intent.current.headers('POST','/api/v2/codex/probe',body) } }));
  }, onSuccess: async () => { intent.current.clear(); await refresh(); } });
  useGuard(probe.isPending);
  const profile = query.data; const view = observation.data; const native = view?.observation;
  const valid = !query.isError && !observation.isError && fresh(view, profile, now);
  return <Card title={profile?.name ?? 'Codex 配置详情'}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!profile} reload={() => { void query.refetch(); }}>
      {profile && <Space orientation="vertical" className="full-width">
        <ResourceFacts id={profile.id} revision={profile.revision} updated={profile.updated_at} />
        <Descriptions column={1} items={[
          { key: 'home', label: '原生账号目录绑定', children: profile.home_binding ?? '历史配置未登记为部署绑定' },
          { key: 'mode', label: '连接来源', children: profile.connection_mode === 'SYSTEM' ? 'SYSTEM：使用该目录的原生 Codex 配置与认证' : 'CUSTOM_PROVIDER：使用独立 Provider 凭据' },
          { key: 'url', label: 'Provider 覆盖', children: profile.connection_mode === 'SYSTEM' ? '不覆盖 URL、API Key 或原生 Provider' : profile.custom_base_url ?? '未配置' },
          { key: 'model', label: '模型设置', children: profile.model_settings.use_default_model_settings ? '沿用原生默认；保存值不执行' : '使用已保存的非空覆盖；须通过原生目录校验' },
          { key: 'saved', label: '保存的模型 / 推理强度', children: `${profile.model_settings.saved_model ?? '原生实际模型'} / ${profile.model_settings.saved_reasoning_effort ?? '原生推理设置'}` },
        ]} />
        <Space wrap>
          <Button disabled={!online || query.isError || probe.isPending || accountBusy || !profile.home_binding} onClick={() => setEditing(profile)}>修改 Codex 设置</Button>
          <Button type="primary" loading={probe.isPending} disabled={!online || query.isError || accountBusy || !profile.home_binding} onClick={() => probe.mutate(profile)}>探测 Codex 连接与模型</Button>
        </Space>
        <Typography.Paragraph type="secondary">探测读取原生账号状态和完整模型目录，不发起付费推理。读取本页或刷新列表也不会隐式启动探测。</Typography.Paragraph>
        <ErrorNotice error={probe.error} />
      </Space>}
    </QueryPanel>
    {profile?.connection_mode === 'SYSTEM' && <CodexAccount key={profile.id} profile={profile} onBusy={setAccountBusy} />}
    <QueryPanel pending={observation.isPending} error={observation.error} stale={!!view} reload={() => { void observation.refetch(); }}>
      {view && <Tag>{view.state === 'AVAILABLE' && !valid ? states.STALE : states[view.state]}</Tag>}
      {native && <Descriptions column={1} items={[
        { key: 'time', label: '观测时间', children: displayTime(native.observed_at) },
        { key: 'expiry', label: '有效至', children: displayTime(native.valid_until) },
        { key: 'revision', label: '观测配置版本', children: native.profile_revision },
      ]} />}
      {native?.outcome.status === 'UNAVAILABLE' && <Alert type="warning" showIcon title={failure[native.outcome.reason]} />}
      {native?.outcome.status === 'AVAILABLE' && <>
        {!valid && <Alert type="warning" showIcon title="以下是历史观测，不代表当前配置仍可使用。" />}
        <Descriptions column={1} items={[
          { key: 'native', label: '原生 Codex 版本', children: native.outcome.native_version },
          { key: 'auth', label: '原生认证类型', children: native.outcome.account.authentication_kind ?? (native.outcome.account.requires_openai_auth ? '需要认证' : '原生 Provider 不要求 OpenAI 认证') },
          { key: 'plan', label: '原生计划', children: native.outcome.account.plan_type ?? '原生协议未提供' },
          { key: 'model', label: '实际模型', children: native.outcome.effective.model },
          { key: 'provider', label: '实际 Provider', children: native.outcome.effective.provider },
          { key: 'effort', label: '实际推理强度', children: native.outcome.effective.reasoning_effort ?? '原生协议未提供' },
          { key: 'tier', label: '实际服务档位', children: native.outcome.effective.service_tier ?? '原生协议未提供显式档位' },
          { key: 'models', label: '完整原生目录条目数', children: native.outcome.models.length },
        ]} />
      </>}
    </QueryPanel>
    {editing && <ProfileDialog original={editing} observation={query.isError || observation.isError ? undefined : view} close={() => setEditing(undefined)} />}
  </Card>;
}

export function CodexSettings() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [creating, setCreating] = useState(false); const [selected, setSelected] = useState<string>(); const online = useOnline();
  const query = useQuery({ queryKey: ['codex','profiles',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>Codex 原生连接与模型</Typography.Title>
    <Alert type="info" showIcon title="模型登录和 Token 刷新由原生 Codex 管理。" description="网页只管理本项目配置和非秘密观测；不会读取或导入 auth.json。已有 Thread 保留原配置，切换配置不会重置研究预算。" />
    <Button type="primary" disabled={!online} onClick={() => setCreating(true)}>登记 Codex 配置</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Profile> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 620 }} locale={{ emptyText: <NoData text="尚无 Codex 配置。先在部署中登记原生账号目录，然后在这里选择 SYSTEM 或独立 Provider。" /> }} columns={[
        { title: '名称', dataIndex: 'name' }, { title: '连接来源', dataIndex: 'connection_mode' }, { title: '版本', dataIndex: 'revision' },
        { title: '模型设置', key: 'model', render: (_, item) => item.model_settings.use_default_model_settings ? '原生默认' : '显式覆盖' },
        { title: '操作', key: 'show', render: (_, item) => <Button onClick={() => setSelected(item.id)}>查看 Codex 配置</Button> },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <ProfileDetails key={selected} id={selected} />}
    {creating && <ProfileDialog close={() => setCreating(false)} />}
  </Space>;
}
