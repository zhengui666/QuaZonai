import { Alert, App, Button, Card, Descriptions, Select, Slider, Space, Switch, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { api, ApiFailure, dataOf, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, QueryPanel, useClock, useOnline } from './ui';
import { ChatgptAuth } from './chatgpt-auth';
import { setSettingsWork } from './settings-work';

type Profile = Schema['CodexProfileViewV1'];
type Observation = Schema['CodexObservationV1'];
type Values = Schema['SavedModelSettingsV1'];
type ModelState = { pending: boolean; uncertain: boolean; error?: unknown };
class ModelSaveSession {
  intent = new Intent(); sent?: Schema['CodexProfileUpdateV1']; values?: Values;
  state: ModelState = { pending: false, uncertain: false };
  private listeners = new Set<() => void>();
  constructor(readonly profileId: string) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  getSnapshot = () => this.state;
  update(changes: Partial<ModelState>) {
    this.state = { ...this.state, ...changes };
    this.listeners.forEach(listener => listener());
    modelListeners.forEach(listener => listener());
    setSettingsWork(`codex-model:${this.profileId}`, this.state.pending || this.state.uncertain);
  }
}
const modelSessions = new Map<string, ModelSaveSession>();
const modelListeners = new Set<() => void>();
const subscribeModelSaves = (listener: () => void) => { modelListeners.add(listener); return () => { modelListeners.delete(listener); }; };
const modelSavesBusy = () => [...modelSessions.values()].some(session => session.state.pending || session.state.uncertain);
function modelSessionFor(id: string) {
  let session = modelSessions.get(id);
  if (!session) { session = new ModelSaveSession(id); modelSessions.set(id, session); }
  return session;
}
const failures: Record<Schema['CodexProbeFailureV1'], string> = {
  DEPLOYMENT_UNAVAILABLE: 'Codex 运行环境不可用，请检查部署配置',
  NATIVE_UNAVAILABLE: 'Codex 连接失败，请重试',
  VERSION_UNSUPPORTED: '无法验证 Codex 版本信息',
  CONTRACT_UNSUPPORTED: 'Codex 响应不兼容',
  AUTHENTICATION_REQUIRED: '请登录 ChatGPT',
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
  if (!values.saved_model && !values.saved_reasoning_effort && !values.saved_fast_mode) return true;
  const model = native.models.find(item => item.capability.model === (values.saved_model || native.native_default_model));
  return !!model
    && (!values.saved_reasoning_effort || model.capability.supported_reasoning_efforts.some(item => item.reasoning_effort === values.saved_reasoning_effort))
    && (!values.saved_fast_mode || model.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast'));
}
function useRefresh() {
  const client = useQueryClient();
  return async () => { await client.invalidateQueries({ queryKey: ['codex'] }); };
}
function ModelControls({ values, observation, profile, disabled, save }: {
  values: Values; observation?: Observation; profile: Profile; disabled: boolean; save: (values: Values) => void;
}) {
  const now = useClock();
  const { use_default_model_settings: defaults, saved_model: selected, saved_reasoning_effort: effort, saved_fast_mode: savedFast } = values;
  const valid = fresh(observation, profile, now);
  const native = observation?.observation?.outcome.status === 'AVAILABLE' ? observation.observation.outcome : undefined;
  const models = valid ? native?.models ?? [] : [];
  const selectedModel = models.find(item => item.capability.model === (selected || native?.native_default_model));
  const efforts = selectedModel?.capability.supported_reasoning_efforts ?? [];
  const index = effort ? efforts.findIndex(item => item.reasoning_effort === effort) : -1;
  const canonicalPosition = index >= 0 ? index + 1 : 0;
  const [position, setPosition] = useState(canonicalPosition);
  useEffect(() => setPosition(canonicalPosition), [canonicalPosition, profile.revision, selected]);
  const fastSupported = selectedModel?.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast') ?? false;
  const options = models.map(item => ({ value: item.capability.model, label: item.capability.display_name }));
  if (selected && !options.some(option => option.value === selected)) options.unshift({ value: selected, label: selected });
  return <>
    <Space><Typography.Text>本机默认</Typography.Text><Switch aria-label="本机默认" checked={defaults}
      disabled={disabled || (defaults && !valid)} onChange={checked => save({ ...values, use_default_model_settings: checked })} /></Space>
    {!valid && <Alert type="warning" showIcon title="模型目录未就绪" />}
    <div className="full-width"><Typography.Text>模型</Typography.Text><div>
      <Select aria-label="模型" className="full-width" allowClear showSearch optionFilterProp="label" options={options} value={selected}
        disabled={disabled || defaults || !valid} placeholder={native?.native_default_model ?? '本机默认'}
        onChange={model => save({ ...values, saved_model: model || null, saved_reasoning_effort: null,
          saved_fast_mode: !!models.find(item => item.capability.model === (model || native?.native_default_model))?.service_tiers.some(tier => tier.id === 'priority' || tier.id === 'fast') && savedFast })} />
    </div></div>
    <Space orientation="vertical" className="full-width"><Typography.Text>推理强度：{effort ?? '本机默认'}</Typography.Text>
      {efforts.length > 0 ? <Slider min={0} max={efforts.length} step={1} value={position} onChange={setPosition}
        ariaLabelForHandle="推理强度" disabled={disabled || defaults || !valid}
        marks={{ 0: '默认', ...Object.fromEntries(efforts.flatMap((item, position) => efforts.length <= 6 || position === efforts.length - 1 || position === index ? [[position + 1, item.reasoning_effort]] : [])) }}
        tooltip={{ formatter: value => value === undefined ? '' : value === 0 ? '本机默认' : efforts[value - 1]?.reasoning_effort ?? '' }}
        onChangeComplete={next => {
          setPosition(canonicalPosition);
          save({ ...values, saved_reasoning_effort: next === 0 ? null : efforts[next - 1]?.reasoning_effort ?? null });
        }} />
        : <Typography.Text type="secondary">暂无选项</Typography.Text>}
      {effort && <Button disabled={disabled || defaults} onClick={() => save({ ...values, saved_reasoning_effort: null })}>恢复默认强度</Button>}
    </Space>
    <Space><Typography.Text>速度</Typography.Text><Switch aria-label="速度" checkedChildren="加速" unCheckedChildren="标准" checked={savedFast}
      disabled={disabled || defaults || !valid || (!fastSupported && !savedFast)} onChange={checked => save({ ...values, saved_fast_mode: checked })} /></Space>
  </>;
}
function ModelSettings({ profile, observation, disabled }: { profile: Profile; observation?: Observation; disabled: boolean }) {
  const online = useOnline(); const refresh = useRefresh(); const query = useQueryClient();
  const { message } = App.useApp();
  const session = modelSessionFor(profile.id);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const body: Schema['CodexProfileUpdateV1'] = session.sent ?? {
      schema_version: 1, expected_revision: profile.revision,
      model_settings: {
        schema_version: 1, use_default_model_settings: values.use_default_model_settings,
        saved_model: values.saved_model || null, saved_reasoning_effort: values.saved_reasoning_effort || null, saved_fast_mode: values.saved_fast_mode,
      },
    };
    session.sent = body; session.values = values;
    return dataOf(await api.PATCH('/api/v2/settings/codex/{id}', { body, params: {
      path: { id: profile.id }, header: session.intent.headers('PATCH', `/api/v2/settings/codex/${profile.id}`, body),
    } }));
  }, onSuccess: result => {
    session.sent = undefined; session.values = undefined; session.intent.clear();
    session.update({ pending: false, uncertain: false, error: undefined });
    query.setQueryData(['codex', 'profile', profile.id], result.resource);
    void refresh();
  }, onError: error => {
    const rejected = error instanceof ApiFailure && !!error.problem && error.status >= 400 && error.status < 500;
    if (rejected || (error instanceof ApiFailure && error.code === 'OFFLINE')) {
      session.sent = undefined; session.values = undefined; session.intent.clear();
    }
    session.update({ pending: false, uncertain: !rejected && !(error instanceof ApiFailure && error.code === 'OFFLINE'), error });
    void message.error('设置更新失败，请检查输入或重试');
  } });
  function save(values: Values) {
    if (disabled || !online || state.pending || state.uncertain || JSON.stringify(values) === JSON.stringify(profile.model_settings)
      || !canSaveSettings(values, observation, profile, Date.now())) return;
    session.update({ pending: true, error: undefined });
    mutation.mutate(values);
  }
  return <Space orientation="vertical" className="full-width">
    <ModelControls values={profile.model_settings} observation={observation} profile={profile}
      disabled={disabled || !online || state.pending || state.uncertain} save={save} />
    {state.pending && <Typography.Text role="status">正在保存</Typography.Text>}
    <ErrorNotice error={state.error} />
    {state.uncertain && <Button disabled={!online} onClick={() => {
      if (session.values) { session.update({ pending: true, error: undefined }); mutation.mutate(session.values); }
    }}>重试</Button>}
    {!!state.error && !state.uncertain && <Button onClick={() => { void query.invalidateQueries({ queryKey: ['codex'] }); }}>重新载入</Button>}
  </Space>;
}
function ProfileDetails({ id, profiles, onSelect }: { id: string; profiles: Profile[]; onSelect: (id: string) => void }) {
  const online = useOnline(); const now = useClock(); const client = useQueryClient(); const intent = useRef(new Intent());
  const modelBusy = useSyncExternalStore(subscribeModelSaves, modelSavesBusy, () => false);
  const [accountBusy, setAccountBusy] = useState(true);
  const attempted = useRef<string | undefined>(undefined);
  const accountChanged = useCallback(async () => {
    attempted.current = undefined; intent.current.clear();
    await client.invalidateQueries({ queryKey: ['codex'] });
  }, [client]);
  const query = useQuery({ queryKey: ['codex','profile',id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex/{id}', { params: { path: { id } }, signal })) });
  const observation = useQuery({ queryKey: ['codex','observation',id], refetchInterval: online ? 15_000 : false,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/models', { params: { query: { profile_id: id } }, signal })) });
  const probe = useMutation({ mutationFn: async (profile: Profile) => {
    const body: Schema['CodexProbeRequestV1'] = { schema_version: 1, profile_id: id, expected_revision: profile.revision };
    return dataOf(await api.POST('/api/v2/codex/probe', { body, params: { header: intent.current.headers('POST','/api/v2/codex/probe',body) } }));
  }, onSuccess: async () => {
    intent.current.clear();
    await Promise.all([
      client.invalidateQueries({ queryKey: ['codex','profile',id], exact: true }),
      client.invalidateQueries({ queryKey: ['codex','observation',id], exact: true }),
    ]);
  } });
  const profile = query.data; const view = observation.data; const native = view?.observation;
  const detected = native?.outcome.status === 'AVAILABLE' ? native.outcome : undefined;
  const tier = detected?.effective.service_tier;
  const speed = !tier || tier === 'default' ? '标准'
    : detected?.models.find(item => item.capability.model === detected.effective.model)?.service_tiers.find(item => item.id === tier)?.name ?? tier;
  const valid = !accountBusy && !query.isError && !observation.isError && fresh(view, profile, now);
  const mutate = probe.mutate;
  useEffect(() => {
    if (!online || !profile || !view || query.isError || observation.isError || query.isFetching || observation.isFetching || accountBusy || probe.isPending) return;
    const version = `${profile.id}:${profile.revision}`;
    if (attempted.current === version || (view.state !== 'NEVER_PROBED' && view.state !== 'STALE')) return;
    attempted.current = version;
    mutate(profile);
  }, [online, profile, view, query.isError, observation.isError, query.isFetching, observation.isFetching, accountBusy, probe.isPending, mutate]);
  return <Space orientation="vertical" className="full-width" size="large">
    {profile && <ChatgptAuth key={profile.id} profile={profile} account={valid && native?.outcome.status === 'AVAILABLE' ? native.outcome.account : undefined}
      disabled={query.isError || probe.isPending || modelBusy} onBusy={setAccountBusy} onChanged={accountChanged} />}
    <Card title="角色模型设置">
      <Space orientation="vertical" className="full-width">
        <Typography.Text type="secondary">模型、推理强度和速度按角色独立保存。</Typography.Text>
        <Select aria-label="Codex 角色" className="full-width" value={id} onChange={onSelect}
          options={profiles.map(item => ({ value: item.id, label: item.name }))} />
        <QueryPanel pending={query.isPending} error={query.error} stale={!!profile} reload={() => { void query.refetch(); }}>
          {profile && <Space orientation="vertical" className="full-width">
            <Space wrap><Button loading={probe.isPending} disabled={!online || query.isError || accountBusy} onClick={() => probe.mutate(profile)}>刷新</Button>
              {view && <Tag>{view.state === 'AVAILABLE' && !valid ? states.STALE : states[view.state]}</Tag>}</Space>
            <ModelSettings key={profile.id} profile={profile} observation={query.isError || observation.isError ? undefined : view}
              disabled={query.isError || probe.isPending || accountBusy} />
            <Descriptions column={1} items={[
              { key: 'defaults', label: '设置', children: profile.model_settings.use_default_model_settings ? '本机默认' : '自定义模型' },
              { key: 'saved', label: '模型 / 推理强度', children: `${profile.model_settings.saved_model ?? '默认'} / ${profile.model_settings.saved_reasoning_effort ?? '默认'}` },
              { key: 'speed', label: '速度', children: profile.model_settings.use_default_model_settings ? '本机默认' : profile.model_settings.saved_fast_mode ? '加速' : '标准' },
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
              { key: 'speed', label: '当前速度', children: speed },
            ]} />
          </>}
        </QueryPanel>
      </Space>
    </Card>
  </Space>;
}
export function CodexSettings() {
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['codex','profiles'], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/settings/codex', { params: { query: { limit: 100 } }, signal })) });
  const profile = query.data?.items.find(item => item.id === selected) ?? query.data?.items[0];
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {profile ? <ProfileDetails key={profile.id} id={profile.id} profiles={query.data?.items ?? []} onSelect={setSelected} /> : <NoData text="Codex 尚未就绪" />}
  </QueryPanel>;
}
