import { Alert, App, Button, Descriptions, Space, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, QueryPanel, useClock, useGuard, useOnline } from './ui';

type Operation = Schema['CodexAccountOperationV1'];
type Command = ({ path: '/api/v2/codex/login/start' | '/api/v2/codex/logout'; body: Schema['CodexAccountRequestV1'] }
  | { path: '/api/v2/codex/login/cancel'; body: Schema['CodexLoginCancelV1'] }) & { intent: Intent; uncertain: boolean };
const terminal = (value: Operation) => ['SUCCEEDED', 'CANCELLED', 'FAILED', 'UNKNOWN'].includes(value.state);
const states: Record<Operation['state'], string> = {
  REQUESTED: '账号操作已接受', WAITING: '等待原生登录结果', CANCEL_REQUESTED: '已请求取消，尚未确认',
  SUCCEEDED: '原生账号操作已完成', CANCELLED: '取消已确认', FAILED: '账号操作失败', UNKNOWN: '账号结果无法确认',
};

export function CodexAccount({ profile, onBusy }: { profile: Schema['CodexProfileViewV1']; onBusy: (value: boolean) => void }) {
  const online = useOnline(); const now = useClock(); const client = useQueryClient(); const { modal } = App.useApp();
  const queryKey = ['codex', 'login', profile.id];
  const [request, setRequest] = useState<Command>();
  const [login, setLogin] = useState<{ id: string; command: Command }>();
  // Never put the one-time code in the query/mutation cache, storage or telemetry.
  const [code, setCode] = useState<{ id: string; value: Schema['CodexDeviceCodeV1'] }>();
  const [accepted, setAccepted] = useState(false);
  const query = useQuery<Operation | null>({ queryKey,
    refetchInterval: online ? query => query.state.data && !terminal(query.state.data) ? 1000 : false : false,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/codex/login', { params: { query: { profile_id: profile.id } }, signal })),
  });
  const mutation = useMutation({ mutationFn: async (command: Command) => {
    const { path, body, intent } = command;
    const header = intent.headers('POST', path, body);
    if (command.path === '/api/v2/codex/login/cancel') {
      dataOf(await api.POST(command.path, { body: command.body, params: { header } }));
      setCode(undefined);
    } else {
      const response = dataOf(await api.POST(command.path, { body: command.body, params: { header } }));
      client.setQueryData(queryKey, response.current);
      setCode(response.device_code ? { id: response.current.operation.id, value: response.device_code } : undefined);
      if (command.path === '/api/v2/codex/login/start') setLogin({ id: response.current.operation.id, command });
    }
    command.uncertain = false;
    setRequest(undefined); setAccepted(true);
    await client.invalidateQueries({ queryKey: ['codex'] });
  }, onError: (error, command) => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (rejected && !command.uncertain) setRequest(undefined);
    else command.uncertain = true;
  } });
  const operation = query.data;
  const active = !!operation && !terminal(operation);
  const expired = !!operation && Date.parse(operation.operation.deadline_at) <= now;
  const busy = active || !!request || mutation.isPending;
  useGuard(busy);
  useEffect(() => { onBusy(busy); return () => onBusy(false); }, [busy, onBusy]);
  useEffect(() => {
    if (code && (expired || operation?.operation.id !== code.id || operation?.state !== 'WAITING')) setCode(undefined);
  }, [code, expired, operation]);
  function send(command: Command) { setRequest(command); setAccepted(false); mutation.mutate(command); }
  function start(action: 'login' | 'logout') {
    const command: Command = { path: action === 'login' ? '/api/v2/codex/login/start' : '/api/v2/codex/logout',
      body: { schema_version: 1, profile_id: profile.id, expected_revision: profile.revision }, intent: new Intent(), uncertain: false };
    modal.confirm({ title: action === 'login' ? '开始原生 ChatGPT 登录？' : '注销这个原生 Codex 账号？',
      content: '操作会改变该目录的原生认证并使旧模型观测失效。不会清空研究预算、取消既有任务或删除 Thread。',
      okText: '确认账号操作', cancelText: '返回', onOk: () => { send(command); } });
  }
  const disabled = !online || query.isError || query.isPending || mutation.isPending || !!request || !profile.home_binding;
  const visibleCode = code && code.id === operation?.operation.id && operation.state === 'WAITING' && !expired ? code.value : undefined;
  return <Space orientation="vertical" className="full-width">
    <Typography.Title level={3}>原生账号登录</Typography.Title>
    <Typography.Paragraph>由 Codex 保存和刷新令牌；这里只展示原生设备码和非秘密操作状态。请勿把设备码、账号凭据或认证文件发给模型。</Typography.Paragraph>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!operation} reload={() => { void query.refetch(); }}>
      {operation ? <>
        <Tag>{states[operation.state]}</Tag>
        <Descriptions column={1} items={[
          { key: 'id', label: '账号操作', children: operation.operation.id },
          { key: 'action', label: '操作类型', children: operation.operation.action === 'LOGIN' ? '登录' : '注销' },
          { key: 'deadline', label: '本项目等待截止', children: displayTime(operation.operation.deadline_at) },
          { key: 'reason', label: '原生结果原因', children: operation.reason ?? '尚无终态结果' },
          { key: 'account', label: '操作结束时的认证观测', children: operation.account
            ? `${operation.account.authentication_kind ?? '未认证'} / ${operation.account.plan_type ?? '原生未提供计划'}` : '尚无账号观测' },
        ]} />
        {(operation.state === 'UNKNOWN' || (active && expired)) && <Alert type="warning" showIcon title="不能确认账号是否已经变化。" description="等待截止不等于取消成功。可查询状态或明确开始新操作；完成后须重新探测配置。" />}
      </> : <Typography.Text>尚无账号操作；读取本页不会触发登录。</Typography.Text>}
    </QueryPanel>
    {visibleCode && <Alert type="info" showIcon title="在原生验证页输入设备码" description={<Space orientation="vertical">
      {/^https:\/\//.test(visibleCode.verification_url) && <Typography.Link href={visibleCode.verification_url} target="_blank" rel="noreferrer">打开原生验证页</Typography.Link>}
      <Typography.Text code>{visibleCode.user_code}</Typography.Text>
      <Typography.Text>关闭页面不会取消登录；此码仅保留在本页内存，等待截止并不代表原生设备码到期。</Typography.Text>
    </Space>} />}
    <Space wrap>
      <Button disabled={disabled || (active && !expired)} onClick={() => start('login')}>登录 ChatGPT 账号</Button>
      <Button danger disabled={disabled || (active && !expired)} onClick={() => start('logout')}>注销原生账号</Button>
      {login?.id === operation?.operation.id && operation?.state === 'WAITING' && !expired && <Button disabled={disabled} onClick={() => send(login!.command)}>重新显示原生设备码</Button>}
      {active && operation.operation.action === 'LOGIN' && operation.state !== 'CANCEL_REQUESTED' && <Button disabled={disabled} onClick={() => send({
        path: '/api/v2/codex/login/cancel', body: { schema_version: 1, operation_id: operation.operation.id, expected_revision: operation.revision }, intent: new Intent(), uncertain: false,
      })}>请求取消登录</Button>}
      <Button disabled={!online || mutation.isPending} onClick={() => { void query.refetch(); }}>刷新账号操作状态</Button>
    </Space>
    {request && !mutation.isPending && <Alert type="warning" showIcon title="账号请求结果尚未确认。" description={<Button disabled={!online} onClick={() => mutation.mutate(request)}>重试同一账号请求</Button>} />}
    {accepted && <Typography.Text>请求已接受；账号是否完成或取消，以实际操作状态为准。结束后请重新探测 Codex 连接与模型。</Typography.Text>}
    <ErrorNotice error={mutation.error} />
  </Space>;
}
