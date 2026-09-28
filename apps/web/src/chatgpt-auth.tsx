import { Alert, App, Button, Card, Descriptions, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { useEffect, useSyncExternalStore } from 'react';
import { api, ApiFailure, dataOf, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, useClock, useOnline } from './ui';
import { setSettingsWork } from './settings-work';

type Operation = Schema['CodexAccountOperationV1'];
type Challenge = { id: string; code: Schema['CodexDeviceCodeV1'] };
type Action = Schema['CodexAccountActionV1'];
type AuthSnapshot = { challenge?: Challenge; pendingStart: boolean; pendingCancel: boolean; unknownStart: boolean; unknownCancel: boolean;
  startError?: unknown; cancelError?: unknown };
class AuthSession {
  startIntent = new Intent(); cancelIntent = new Intent();
  startRequest?: { action: Action; body: Schema['CodexAccountRequestV1'] };
  cancelRequest?: Schema['CodexLoginCancelV1'];
  startedId?: string; refreshed?: string;
  snapshot: AuthSnapshot = { pendingStart: false, pendingCancel: false, unknownStart: false, unknownCancel: false };
  private listeners = new Set<() => void>();
  private reconciling = false;
  constructor(readonly profileId: string) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  getSnapshot = () => this.snapshot;
  get observed() { return this.listeners.size > 0; }
  update(changes: Partial<AuthSnapshot> = {}) {
    this.snapshot = { ...this.snapshot, ...changes };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`chatgpt-auth:${this.profileId}`, !!(this.startRequest || this.cancelRequest || this.snapshot.challenge
      || this.snapshot.pendingStart || this.snapshot.pendingCancel || this.snapshot.unknownStart || this.snapshot.unknownCancel));
  }
  async reconcileUnknownStart(client: QueryClient) {
    const request = this.startRequest;
    if (!request || !this.snapshot.unknownStart || this.reconciling) return;
    this.reconciling = true;
    const path = request.action === 'LOGIN' ? '/api/v2/codex/login/start' : '/api/v2/codex/logout';
    try {
      const result = dataOf(await api.POST(path, { body: request.body,
        params: { header: this.startIntent.headers('POST', path, request.body) } }));
      this.startedId = result.current.operation.id;
      client.setQueryData(['codex', 'account-operation', this.profileId], result.current);
      if (activeAccountOperation(result.current)) this.update({
        challenge: result.device_code ? { id: result.current.operation.id, code: result.device_code } : undefined,
        unknownStart: false, startError: undefined,
      });
      else {
        this.startRequest = undefined; this.startIntent.clear(); this.startedId = undefined;
        this.update({ challenge: undefined, unknownStart: false, startError: undefined });
      }
    } catch { /* Keep the original request for an explicit same-key retry. */ }
    finally { this.reconciling = false; }
  }
}
const sessions = new Map<string, AuthSession>();
function sessionFor(profileId: string) {
  let session = sessions.get(profileId);
  if (!session) { session = new AuthSession(profileId); sessions.set(profileId, session); }
  return session;
}
const labels: Record<Operation['state'], string> = {
  REQUESTED: '正在准备', WAITING: '等待 ChatGPT 授权', CANCEL_REQUESTED: '正在取消，请等待确认',
  SUCCEEDED: '已完成', CANCELLED: '已取消', FAILED: '未完成', UNKNOWN: '结果未知，请刷新账号状态',
};
const reasons: Record<Schema['CodexAccountReasonV1'], string> = {
  NATIVE_LOGIN_COMPLETED: 'ChatGPT 登录成功', NATIVE_LOGIN_REJECTED: 'ChatGPT 授权未完成，请重新登录',
  NATIVE_LOGOUT_COMPLETED: '已退出 ChatGPT', NATIVE_CANCEL_CONFIRMED: '登录已取消',
  CONFIRMED_NOT_SENT: '登录已取消', NATIVE_RESPONSE_UNKNOWN: '尚未确认结果，请刷新账号状态',
  NATIVE_CONTRACT_UNSUPPORTED: 'Codex 账号响应不兼容', NATIVE_VERSION_UNSUPPORTED: '无法验证 Codex 版本',
  DEPLOYMENT_UNAVAILABLE: 'Codex 运行环境不可用，请检查部署配置', PROFILE_CHANGED: '配置已变更，请重试',
  WAIT_WINDOW_ENDED: '等待授权已超时，请刷新账号状态', OWNER_UNAVAILABLE: '登录连接已中断，请刷新账号状态',
};
export function activeAccountOperation(operation: Operation | null | undefined): boolean {
  return !!operation && ['REQUESTED', 'WAITING', 'CANCEL_REQUESTED'].includes(operation.state);
}
export function liveLoginCode(challenge: Challenge | undefined, operation: Operation | null | undefined, now: number) {
  if (!challenge || !operation || operation.operation.id !== challenge.id || operation.operation.action !== 'LOGIN'
    || operation.state !== 'WAITING' || Date.parse(operation.operation.deadline_at) <= now) return undefined;
  try {
    const url = new URL(challenge.code.verification_url);
    if (url.protocol === 'https:' && !url.username && !url.password) return challenge.code;
  } catch { /* Invalid native links must never become executable browser URLs. */ }
  return undefined;
}
function uncertain(error: unknown) {
  return !(error instanceof ApiFailure) || ['NETWORK_UNKNOWN', 'HTTP_CONTRACT_ERROR'].includes(error.code);
}

export function ChatgptAuth({ profile, account, disabled, onBusy, onChanged }: {
  profile: Schema['CodexProfileViewV1']; account?: Schema['CodexAccountV1']; disabled: boolean;
  onBusy: (busy: boolean) => void; onChanged: () => Promise<void>;
}) {
  const online = useOnline(); const now = useClock(); const client = useQueryClient(); const { modal } = App.useApp();
  const session = sessionFor(profile.id);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const key = ['codex', 'account-operation', profile.id];
  const latest = useQuery<Operation | null>({ queryKey: key, staleTime: 0,
    refetchInterval: query => online ? (activeAccountOperation(query.state.data) ? 2_000 : 15_000) : false,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/login', { params: { query: { profile_id: profile.id } }, signal })),
  });
  const start = useMutation({ mutationFn: async (action: Action) => {
    session.startRequest ??= { action, body: { schema_version: 1, profile_id: profile.id, expected_revision: profile.revision } };
    const request = session.startRequest;
    const path = request.action === 'LOGIN' ? '/api/v2/codex/login/start' : '/api/v2/codex/logout';
    await client.cancelQueries({ queryKey: key, exact: true });
    const result = dataOf(await api.POST(path, { body: request.body,
      params: { header: session.startIntent.headers('POST', path, request.body) } }));
    const activeResult = activeAccountOperation(result.current);
    session.startedId = activeResult ? result.current.operation.id : undefined;
    if (!activeResult) { session.startRequest = undefined; session.startIntent.clear(); }
    // Only component memory owns the one-time code; mutation/query caches receive no code.
    session.update({ challenge: activeResult && result.device_code ? { id: result.current.operation.id, code: result.device_code } : undefined,
      pendingStart: false, unknownStart: false, startError: undefined });
    client.setQueryData(key, result.current);
  }, onError: error => {
    if (!uncertain(error)) { session.startRequest = undefined; session.startIntent.clear(); }
    session.update({ pendingStart: false, unknownStart: uncertain(error), startError: error });
    if (uncertain(error) && !session.observed) void session.reconcileUnknownStart(client);
    void latest.refetch();
  } });
  const cancel = useMutation({ mutationFn: async (operation: Operation) => {
    session.cancelRequest ??= { schema_version: 1, operation_id: operation.operation.id, expected_revision: operation.revision };
    const body = session.cancelRequest;
    dataOf(await api.POST('/api/v2/codex/login/cancel', { body,
      params: { header: session.cancelIntent.headers('POST', '/api/v2/codex/login/cancel', body) } }));
    session.update({ challenge: undefined, pendingCancel: false, unknownCancel: false, cancelError: undefined });
    await latest.refetch();
  }, onSuccess: () => { session.cancelRequest = undefined; session.cancelIntent.clear(); session.update(); }, onError: error => {
    if (!uncertain(error)) { session.cancelRequest = undefined; session.cancelIntent.clear(); }
    session.update({ pendingCancel: false, unknownCancel: uncertain(error), cancelError: error });
    void latest.refetch();
  } });
  const operation = latest.data;
  const active = activeAccountOperation(operation);
  const expired = active && !!operation && Date.parse(operation.operation.deadline_at) <= now;
  const unknownStart = state.unknownStart && !!session.startRequest;
  const unknownCancel = state.unknownCancel && !!session.cancelRequest;
  const pending = state.pendingStart || state.pendingCancel || start.isPending || cancel.isPending;
  const busy = pending || unknownStart || unknownCancel || active || latest.isPending || latest.isError;
  const code = !latest.isError && online ? liveLoginCode(state.challenge, operation, now) : undefined;
  useEffect(() => { onBusy(busy); }, [busy, onBusy]);
  useEffect(() => {
    if (state.challenge && operation && !latest.isError && (!active || expired || operation.state === 'CANCEL_REQUESTED'))
      session.update({ challenge: undefined });
  }, [state.challenge, active, expired, operation, latest.isError, session]);
  useEffect(() => {
    if (!operation || active || latest.isError) return;
    const version = `${operation.operation.id}:${operation.revision}`;
    const owner = sessions.get(operation.operation.profile_id);
    if (owner && owner !== session) {
      if (owner.startedId === operation.operation.id || owner.cancelRequest?.operation_id === operation.operation.id) {
        owner.startRequest = undefined; owner.startIntent.clear(); owner.startedId = undefined;
        owner.cancelRequest = undefined; owner.cancelIntent.clear();
        owner.update({ challenge: undefined, pendingStart: false, pendingCancel: false, unknownStart: false, unknownCancel: false,
          startError: undefined, cancelError: undefined });
      } else void owner.reconcileUnknownStart(client);
    }
    for (const candidate of sessions.values()) {
      if (candidate === session || candidate === owner || candidate.cancelRequest?.operation_id !== operation.operation.id) continue;
      candidate.cancelRequest = undefined; candidate.cancelIntent.clear();
      candidate.update({ pendingCancel: false, unknownCancel: false, cancelError: undefined });
    }
    if (session.refreshed === version) return;
    session.refreshed = version;
    const ownedStart = session.startedId === operation.operation.id;
    const ownedCancel = session.cancelRequest?.operation_id === operation.operation.id;
    if (ownedStart) {
      session.startRequest = undefined; session.startIntent.clear(); session.startedId = undefined;
    }
    if (ownedCancel) {
      session.cancelRequest = undefined; session.cancelIntent.clear(); cancel.reset();
    }
    session.update({ challenge: undefined, ...(ownedStart ? { unknownStart: false, startError: undefined } : {}),
      ...(ownedCancel ? { unknownCancel: false, cancelError: undefined } : {}) });
    void onChanged();
  }, [operation, active, latest.isError, onChanged, cancel.reset, session, client]);
  function startOperation(action: Action) {
    session.update({ pendingStart: true, unknownStart: false, startError: undefined }); start.mutate(action);
  }
  function cancelOperation(value: Operation) {
    session.update({ pendingCancel: true, unknownCancel: false, cancelError: undefined }); cancel.mutate(value);
  }
  function begin(action: Action) {
    session.startRequest = undefined; session.startIntent.clear(); session.startedId = undefined;
    session.update({ challenge: undefined }); cancel.reset(); startOperation(action);
  }
  const unavailable = !online || disabled || pending || latest.isPending || latest.isError;
  return <Card title="共享 ChatGPT 账号" size="small">
    <Space orientation="vertical" className="full-width">
      <Typography.Text type="secondary">研究员与独立审阅员共用此账号，只需登录一次。退出登录会影响所有角色。</Typography.Text>
      <Descriptions size="small" column={1} items={[
        { key: 'status', label: '账号', children: active ? '登录状态正在更新' : account?.authentication_kind === 'CHATGPT' ? '已登录 ChatGPT'
          : account?.authentication_kind ? '当前使用其他认证方式' : account ? '未登录 ChatGPT' : '待检测' },
        ...(account?.authentication_kind === 'CHATGPT' && !active && account.plan_type
          ? [{ key: 'plan', label: '套餐', children: account.plan_type }] : []),
      ]} />
      <Space wrap>
        <Button type="primary" aria-label="登录 ChatGPT" disabled={unavailable || unknownStart || unknownCancel || (active && !expired)} loading={state.pendingStart || start.isPending}
          onClick={() => begin('LOGIN')}>登录 ChatGPT</Button>
        {account?.authentication_kind === 'CHATGPT' && <Button disabled={unavailable || unknownStart || unknownCancel || active}
          onClick={() => modal.confirm({ title: '退出 ChatGPT？', content: '研究员与审阅员共用此账号。', okText: '退出', cancelText: '取消',
            onOk: () => begin('LOGOUT') })}>退出 ChatGPT</Button>}
        {unknownStart && <Button disabled={!online || pending} onClick={() => startOperation(session.startRequest!.action)}>重试当前操作</Button>}
        {operation?.operation.action === 'LOGIN' && active && <Button aria-label="取消登录" disabled={unavailable || unknownStart || (operation.state === 'CANCEL_REQUESTED' && !unknownCancel)}
          loading={state.pendingCancel || cancel.isPending} onClick={() => cancelOperation(operation)}>取消登录</Button>}
        {!code && !unknownStart && operation?.state === 'WAITING' && !expired && session.startedId === operation.operation.id
          && <Button disabled={unavailable} onClick={() => startOperation('LOGIN')}>获取授权码</Button>}
      </Space>
      {operation && <Alert showIcon type={operation.state === 'SUCCEEDED' ? 'success' : operation.state === 'FAILED' || operation.state === 'UNKNOWN' || expired ? 'warning' : 'info'}
        title={expired ? '等待授权已超时，可重新登录；原操作结果仍待确认' : operation.reason ? reasons[operation.reason] : labels[operation.state]} />}
      {code && <Space orientation="vertical">
        <Typography.Text>在 OpenAI 页面输入授权码：</Typography.Text>
        <Typography.Text code copyable={{ text: code.user_code }} aria-label="ChatGPT 授权码">{code.user_code}</Typography.Text>
        <Button href={code.verification_url} target="_blank" rel="noopener noreferrer">打开 ChatGPT 授权页面</Button>
        <Typography.Text type="secondary">授权后会自动更新。剩余等待时间：{Math.max(0, Math.ceil((Date.parse(operation!.operation.deadline_at) - now) / 1000))} 秒</Typography.Text>
      </Space>}
      {active && !expired && !code && !pending && !unknownStart && !session.startRequest && operation?.state !== 'CANCEL_REQUESTED'
        && <Typography.Text>请在发起登录的页面完成授权，或取消后重新登录。</Typography.Text>}
      <ErrorNotice error={latest.error} retry={() => { void latest.refetch(); }} />
      <ErrorNotice error={state.startError} />
      <ErrorNotice error={state.cancelError} />
    </Space>
  </Card>;
}
