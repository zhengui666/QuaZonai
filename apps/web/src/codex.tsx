import { Alert, App, Button, Card, Descriptions, Form, Input, Modal, Select, Slider, Space, Switch, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, QueryPanel, useClock, useGuard, useOnline } from './ui';

type Profile = Schema['CodexProfileViewV1'];
type Observation = Schema['CodexObservationV1'];
type Values = Schema['SavedModelSettingsV1'];
const failures: Record<Schema['CodexProbeFailureV1'], string> = {
  DEPLOYMENT_UNAVAILABLE: '未找到本机 Codex，请安装并登录后重启服务',
  NATIVE_UNAVAILABLE: 'Codex 连接失败，请重试',
  VERSION_UNSUPPORTED: 'Codex 版本不兼容',
  CONTRACT_UNSUPPORTED: 'Codex 响应不兼容',
  AUTHENTICATION_REQUIRED: '请先在本机执行 codex login',
  MODEL_SETTINGS_UNSUPPORTED: '模型设置不可用，请恢复本机默认',
};
const states: Record<Schema['CodexObservationStateV1'], string> = {
  NEVER_PROBED: '未检测', STALE: '待刷新', AVAILABLE: '可用', UNAVAILABLE: '不可用',
};
function fresh(observation: Observation | undefined, profile: Profile | undefined, now: number): boolean {
  return !!profile && observation?.profile_id === profile.id && observation.state === 'AVAILABLE'
    && observation.profile_revision === profile.revision && observation.observation?.profile_revision === profile.revision
    && observation.observation.outcome.status === 'AVAILABLE' && Date.parse(observation.observation.valid_until) > now;
}
function canSaveSettings(values: Values, observation: Observation | undefined, profile: Profile, now: number): boolean {
  if (values.use_default_model_settings) return true;
  if (!fresh(observation, profile, now)) return false;
  const native = observation?.observation?.outcome;
  if (native?.status !== 'AVAILABLE') return false;
  const model = native.models.find(item => item.capability.model === (values.saved_model || native.effective.model));
  return !!model
    && (!values.saved_reasoning_effort || model.capability.supported_reasoning_efforts.some(item => item.reasoning_effort === values.saved_reasoning_effort))
    && (!values.saved_fast_mode || model.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast'));
}
function useRefresh() {
  const client = useQueryClient();
  return async () => { await client.invalidateQueries({ queryKey: ['codex'] }); };
}
function ModelControls({ form, observation, profile, disabled }: {
  form: ReturnType<typeof Form.useForm<Values>>[0]; observation?: Observation; profile: Profile; disabled: boolean;
}) {
  const now = useClock();
  const defaults = Form.useWatch('use_default_model_settings', form) ?? true;
  const selected = Form.useWatch('saved_model', form) as string | null | undefined;
  const effort = Form.useWatch('saved_reasoning_effort', form) as string | null | undefined;
  const savedFast = Form.useWatch('saved_fast_mode', form) ?? false;
  function setEffort(value: string | null) { form.setFields([{ name: 'saved_reasoning_effort', value, touched: true }]); }
  const valid = fresh(observation, profile, now);
  const native = observation?.observation?.outcome.status === 'AVAILABLE' ? observation.observation.outcome : undefined;
  const models = valid ? native?.models ?? [] : [];
  const selectedModel = models.find(item => item.capability.model === (selected || native?.effective.model));
  const efforts = selectedModel?.capability.supported_reasoning_efforts ?? [];
  const index = effort ? efforts.findIndex(item => item.reasoning_effort === effort) : -1;
  const fastSupported = selectedModel?.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast') ?? false;
  const options = models.map(item => ({ value: item.capability.model, label: item.capability.display_name }));
  if (selected && !options.some(option => option.value === selected)) options.unshift({ value: selected, label: selected });
  return <>
    <Form.Item name="use_default_model_settings" label="本机默认" valuePropName="checked"><Switch /></Form.Item>
    {!valid && <Alert type="warning" showIcon title="模型目录未就绪" />}
    <Form.Item name="saved_model" label="模型">
      <Select allowClear showSearch optionFilterProp="label" options={options} disabled={disabled || defaults || !valid}
        placeholder={native?.effective.model ?? '本机默认'} onChange={() => setEffort(null)} />
    </Form.Item>
    <Form.Item name="saved_reasoning_effort" hidden><Input /></Form.Item>
    <Form.Item label="推理强度">
      <Space orientation="vertical" className="full-width">
        <Typography.Text>{effort ?? '本机默认'}</Typography.Text>
        {efforts.length > 0 ? <Slider min={0} max={efforts.length} step={1} value={index >= 0 ? index + 1 : 0}
          ariaLabelForHandle="推理强度" disabled={disabled || defaults || !valid}
          marks={{ 0: '默认', ...Object.fromEntries(efforts.flatMap((item, position) => efforts.length <= 6 || position === efforts.length - 1 || position === index ? [[position + 1, item.reasoning_effort]] : [])) }}
          tooltip={{ formatter: value => value === undefined ? '' : value === 0 ? '本机默认' : efforts[value - 1]?.reasoning_effort ?? '' }}
          onChange={value => { const chosen = efforts[value - 1]; if (value === 0) setEffort(null); else if (chosen) setEffort(chosen.reasoning_effort); }} />
          : <Typography.Text type="secondary">暂无选项</Typography.Text>}
        {effort && <Button disabled={disabled || defaults} onClick={() => setEffort(null)}>恢复默认强度</Button>}
      </Space>
    </Form.Item>
    <Form.Item name="saved_fast_mode" label="加速" valuePropName="checked">
      <Switch disabled={disabled || defaults || ((!valid || !fastSupported) && !savedFast)} />
    </Form.Item>
  </>;
}
function ModelDialog({ original, observation, close }: { original: Profile; observation?: Observation; close: () => void }) {
  const [form] = Form.useForm<Values>();
  const online = useOnline(); const { modal } = App.useApp(); const refresh = useRefresh();
  const now = useClock();
  const watched = Form.useWatch(values => values, form) as Values | undefined;
  const valid = canSaveSettings(watched ?? original.model_settings, observation, original, now);
  const intent = useRef(new Intent()); const sent = useRef<Schema['CodexProfileUpdateV1'] | undefined>(undefined);
  const hadUnknown = useRef(false); const [saveValues, setSaveValues] = useState<Values>();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    if (!sent.current && !canSaveSettings(values, observation, original, Date.now())) {
      throw new ApiFailure('MODEL_SETTINGS_UNAVAILABLE', '请刷新模型目录或使用本机默认');
    }
    const body = sent.current ?? {
      schema_version: 1 as const, expected_revision: original.revision,
      model_settings: {
        schema_version: 1 as const, use_default_model_settings: values.use_default_model_settings,
        saved_model: values.saved_model || null, saved_reasoning_effort: values.saved_reasoning_effort || null, saved_fast_mode: values.saved_fast_mode,
      },
    };
    sent.current = body;
    return dataOf(await api.PATCH('/api/v2/settings/codex/{id}', { body, params: {
      path: { id: original.id }, header: intent.current.headers('PATCH', `/api/v2/settings/codex/${original.id}`, body),
    } }));
  }, onSuccess: async () => { close(); await refresh(); }, onError: error => {
    if (!(error instanceof ApiFailure) || error.code === 'NETWORK_UNKNOWN' || error.code === 'HTTP_CONTRACT_ERROR') hadUnknown.current = true;
    if (!hadUnknown.current) { sent.current = undefined; setSaveValues(undefined); }
  } });
  const pending = mutation.isPending;
  const unknown = !!saveValues && mutation.isError;
  useGuard(true);
  function cancel() {
    if (pending) return;
    if (form.isFieldsTouched() || unknown) modal.confirm({
      title: unknown ? '关闭结果未确认的操作？' : '放弃未保存的更改？',
      okText: '关闭', cancelText: '继续编辑', onOk: close,
    });
    else close();
  }
  return <Modal open title="模型设置" width={680} maskClosable={false} closable={!pending} onCancel={cancel}
    onOk={() => { if (online && !pending) { if (unknown && saveValues) mutation.mutate(saveValues); else if (valid) form.submit(); } }}
    okText={unknown ? '重试保存' : '保存'} cancelText="取消" confirmLoading={pending} okButtonProps={{ disabled: !online || (!unknown && !valid) }}>
    {unknown && <Alert type="warning" showIcon title="保存结果未知，请重试当前操作" />}
    {!unknown && !valid && <Alert type="warning" showIcon title="请刷新模型目录或使用本机默认" />}
    <Form form={form} layout="vertical" disabled={!online || pending || unknown} initialValues={original.model_settings}
      onFinish={values => {
        if (!online || pending || unknown || !canSaveSettings(values, observation, original, Date.now())) return;
        const request = structuredClone(values); setSaveValues(request); mutation.mutate(request);
      }}>
      <ModelControls form={form} observation={observation} profile={original} disabled={!online || pending || unknown} />
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}
function ProfileDetails({ id }: { id: string }) {
  const online = useOnline(); const now = useClock(); const refresh = useRefresh(); const intent = useRef(new Intent());
  const [editing, setEditing] = useState<Profile>(); const attempted = useRef<string | undefined>(undefined);
  const query = useQuery({ queryKey: ['codex','profile',id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex/{id}', { params: { path: { id } }, signal })) });
  const observation = useQuery({ queryKey: ['codex','observation',id], refetchInterval: online ? 15_000 : false,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/models', { params: { query: { profile_id: id } }, signal })) });
  const probe = useMutation({ mutationFn: async (profile: Profile) => {
    const body: Schema['CodexProbeRequestV1'] = { schema_version: 1, profile_id: id, expected_revision: profile.revision };
    return dataOf(await api.POST('/api/v2/codex/probe', { body, params: { header: intent.current.headers('POST','/api/v2/codex/probe',body) } }));
  }, onSuccess: async () => { intent.current.clear(); await refresh(); } });
  const profile = query.data; const view = observation.data; const native = view?.observation;
  const valid = !query.isError && !observation.isError && fresh(view, profile, now);
  const mutate = probe.mutate;
  useEffect(() => {
    if (!online || !profile || !view || query.isError || observation.isError || editing || probe.isPending) return;
    const version = `${profile.id}:${profile.revision}`;
    if (attempted.current === version || (view.state !== 'NEVER_PROBED' && view.state !== 'STALE')) return;
    attempted.current = version;
    mutate(profile);
  }, [online, profile, view, query.isError, observation.isError, editing, probe.isPending, mutate]);
  useGuard(probe.isPending);
  return <Card title={profile?.name ?? '本机 Codex'}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!profile} reload={() => { void query.refetch(); }}>
      {profile && <Space orientation="vertical" className="full-width">
        <Space wrap>
          <Button disabled={!online || query.isError || probe.isPending} onClick={() => setEditing(profile)}>模型设置</Button>
          <Button loading={probe.isPending} disabled={!online || query.isError} onClick={() => probe.mutate(profile)}>刷新</Button>
          {view && <Tag>{view.state === 'AVAILABLE' && !valid ? states.STALE : states[view.state]}</Tag>}
        </Space>
        <Descriptions column={1} items={[
          { key: 'defaults', label: '设置', children: profile.model_settings.use_default_model_settings ? '本机默认' : '自定义模型' },
          { key: 'saved', label: '模型 / 推理强度', children: `${profile.model_settings.saved_model ?? '默认'} / ${profile.model_settings.saved_reasoning_effort ?? '默认'}` },
        ]} />
        <ErrorNotice error={probe.error} />
      </Space>}
    </QueryPanel>
    <QueryPanel pending={observation.isPending} error={observation.error} stale={!!view} reload={() => { void observation.refetch(); }}>
      {native?.outcome.status === 'UNAVAILABLE' && <Alert type="warning" showIcon title={failures[native.outcome.reason]} />}
      {native?.outcome.status === 'AVAILABLE' && <>
        {!valid && <Alert type="warning" showIcon title="检测结果已过期" />}
        <Descriptions column={1} items={[
          { key: 'native', label: '版本', children: native.outcome.native_version },
          { key: 'model', label: '当前模型', children: native.outcome.effective.model },
          { key: 'effort', label: '当前推理强度', children: native.outcome.effective.reasoning_effort ?? '默认' },
        ]} />
      </>}
    </QueryPanel>
    {editing && <ModelDialog original={editing} observation={query.isError || observation.isError ? undefined : view} close={() => setEditing(undefined)} />}
  </Card>;
}
export function CodexSettings() {
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['codex','profiles'], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex', { params: { query: { limit: 100 } }, signal })) });
  const profile = query.data?.items.find(item => item.id === selected) ?? query.data?.items[0];
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {profile ? <Space orientation="vertical" className="full-width" size="large">
      <Select aria-label="Codex 角色" className="full-width" value={profile.id} onChange={setSelected}
        options={query.data?.items.map(item => ({ value: item.id, label: item.name }))} />
      <ProfileDetails key={profile.id} id={profile.id} />
    </Space> : <NoData text="本机 Codex 尚未就绪" />}
  </QueryPanel>;
}
