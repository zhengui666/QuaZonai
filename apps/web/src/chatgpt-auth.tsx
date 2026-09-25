import { Alert, App, Button, Card, Descriptions, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api, ApiFailure, dataOf, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, useClock, useGuard, useOnline } from './ui';

type Operation = Schema['CodexAccountOperationV1'];
type Challenge = { id: string; code: Schema['CodexDeviceCodeV1'] };
type Action = Schema['CodexAccountActionV1'];
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
  const [challenge, setChallenge] = useState<Challenge>();
  const startIntent = useRef(new Intent()); const cancelIntent = useRef(new Intent());
  const startRequest = useRef<{ action: Action; body: Schema['CodexAccountRequestV1'] } | undefined>(undefined);
  const cancelRequest = useRef<Schema['CodexLoginCancelV1'] | undefined>(undefined);
  const startedId = useRef<string | undefined>(undefined); const refreshed = useRef<string | undefined>(undefined);
  const key = ['codex', 'account-operation', profile.id];
  const latest = useQuery<Operation | null>({ queryKey: key, staleTime: 0,
    refetchInterval: query => online ? (activeAccountOperation(query.state.data) ? 2_000 : 15_000) : false,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/login', { params: { query: { profile_id: profile.id } }, signal })),
  });
  const start = useMutation({ mutationFn: async (action: Action) => {
    startRequest.current ??= { action, body: { schema_version: 1, profile_id: profile.id, expected_revision: profile.revision } };
    const request = startRequest.current;
    const path = request.action === 'LOGIN' ? '/api/v2/codex/login/start' : '/api/v2/codex/logout';
    await client.cancelQueries({ queryKey: key, exact: true });
    const result = dataOf(await api.POST(path, { body: request.body,
      params: { header: startIntent.current.headers('POST', path, request.body) } }));
    startedId.current = result.current.operation.id;
    // Only component memory owns the one-time code; mutation/query caches receive no code.
    setChallenge(result.device_code ? { id: result.current.operation.id, code: result.device_code } : undefined);
    client.setQueryData(key, result.current);
  }, onError: error => {
    if (!uncertain(error)) { startRequest.current = undefined; startIntent.current.clear(); }
    void latest.refetch();
  } });
  const cancel = useMutation({ mutationFn: async (operation: Operation) => {
    cancelRequest.current ??= { schema_version: 1, operation_id: operation.operation.id, expected_revision: operation.revision };
    const body = cancelRequest.current;
    dataOf(await api.POST('/api/v2/codex/login/cancel', { body,
      params: { header: cancelIntent.current.headers('POST', '/api/v2/codex/login/cancel', body) } }));
    setChallenge(undefined);
    await latest.refetch();
  }, onSuccess: () => { cancelRequest.current = undefined; cancelIntent.current.clear(); }, onError: error => {
    if (!uncertain(error)) { cancelRequest.current = undefined; cancelIntent.current.clear(); }
    void latest.refetch();
  } });
  const operation = latest.data;
  const active = activeAccountOperation(operation);
  const expired = active && !!operation && Date.parse(operation.operation.deadline_at) <= now;
  const unknownStart = start.isError && !!startRequest.current;
  const unknownCancel = cancel.isError && !!cancelRequest.current;
  const pending = start.isPending || cancel.isPending;
  const busy = pending || unknownStart || unknownCancel || active || latest.isPending || latest.isError;
  const code = !latest.isError && online ? liveLoginCode(challenge, operation, now) : undefined;
  useGuard(pending || unknownStart || unknownCancel || (active && !expired));
  useEffect(() => { onBusy(busy); }, [busy, onBusy]);
  useEffect(() => {
    if (challenge && (!active || expired || operation?.state === 'CANCEL_REQUESTED')) setChallenge(undefined);
  }, [challenge, active, expired, operation?.state]);
  useEffect(() => {
    if (!operation || active || latest.isError) return;
    const version = `${operation.operation.id}:${operation.revision}`;
    if (refreshed.current === version) return;
    refreshed.current = version;
    if (startedId.current === operation.operation.id) {
      startRequest.current = undefined; startIntent.current.clear(); startedId.current = undefined;
    }
    if (cancelRequest.current?.operation_id === operation.operation.id) {
      cancelRequest.current = undefined; cancelIntent.current.clear(); cancel.reset();
    }
    void onChanged();
  }, [operation, active, latest.isError, onChanged, cancel.reset]);
  function begin(action: Action) {
    startRequest.current = undefined; startIntent.current.clear(); startedId.current = undefined;
    setChallenge(undefined); cancel.reset(); start.mutate(action);
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
        <Button type="primary" aria-label="登录 ChatGPT" disabled={unavailable || unknownStart || unknownCancel || (active && !expired)} loading={start.isPending}
          onClick={() => begin('LOGIN')}>登录 ChatGPT</Button>
        {account?.authentication_kind === 'CHATGPT' && <Button disabled={unavailable || unknownStart || unknownCancel || active}
          onClick={() => modal.confirm({ title: '退出 ChatGPT？', content: '研究员与审阅员共用此账号。', okText: '退出', cancelText: '取消',
            onOk: () => begin('LOGOUT') })}>退出 ChatGPT</Button>}
        {unknownStart && <Button disabled={!online || pending} onClick={() => start.mutate(startRequest.current!.action)}>重试当前操作</Button>}
        {operation?.operation.action === 'LOGIN' && active && <Button aria-label="取消登录" disabled={unavailable || unknownStart || (operation.state === 'CANCEL_REQUESTED' && !unknownCancel)}
          loading={cancel.isPending} onClick={() => cancel.mutate(operation)}>取消登录</Button>}
        {!code && !unknownStart && operation?.state === 'WAITING' && !expired && startedId.current === operation.operation.id
          && <Button disabled={unavailable} onClick={() => start.mutate('LOGIN')}>获取授权码</Button>}
      </Space>
      {operation && <Alert showIcon type={operation.state === 'SUCCEEDED' ? 'success' : operation.state === 'FAILED' || operation.state === 'UNKNOWN' || expired ? 'warning' : 'info'}
        title={expired ? '等待授权已超时，可重新登录；原操作结果仍待确认' : operation.reason ? reasons[operation.reason] : labels[operation.state]} />}
      {code && <Space orientation="vertical">
        <Typography.Text>在 OpenAI 页面输入授权码：</Typography.Text>
        <Typography.Text code copyable={{ text: code.user_code }} aria-label="ChatGPT 授权码">{code.user_code}</Typography.Text>
        <Button href={code.verification_url} target="_blank" rel="noopener noreferrer">打开 ChatGPT 授权页面</Button>
        <Typography.Text type="secondary">授权后会自动更新。剩余等待时间：{Math.max(0, Math.ceil((Date.parse(operation!.operation.deadline_at) - now) / 1000))} 秒</Typography.Text>
      </Space>}
      {active && !expired && !code && !pending && !unknownStart && !startRequest.current && operation?.state !== 'CANCEL_REQUESTED'
        && <Typography.Text>请在发起登录的页面完成授权，或取消后重新登录。</Typography.Text>}
      <ErrorNotice error={latest.error} retry={() => { void latest.refetch(); }} />
      <ErrorNotice error={start.error} />
      <ErrorNotice error={cancel.error} />
    </Space>
  </Card>;
}
