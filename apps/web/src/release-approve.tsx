import { Alert, App, Button, Form, Input, Modal, Select, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { ErrorNotice, useGuard, useOnline } from './ui';

export function ReleaseApprove({ release, close }: { release: Schema['ReleaseViewV1']; close: () => void }) {
  const [downstream, setDownstream] = useState<string>();
  const [environment, setEnvironment] = useState<'PAPER' | 'LIVE'>();
  const [until, setUntil] = useState('');
  const [submitted, setSubmitted] = useState<Schema['ReleaseApproveV1']>();
  const [receipt, setReceipt] = useState<Schema['ApprovalViewV1']>();
  const intent = useRef(new Intent()); const unknown = useRef(false);
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const source = useQuery({ queryKey: ['approval-source', release.id, downstream, environment], enabled: !!downstream && !!environment && !submitted && !receipt, queryFn: async ({ signal }) => {
    const down = dataOf(await api.GET('/api/v2/integrations/downstreams/{id}', { params: { path: { id: downstream! } }, signal }));
    if (down.id !== downstream) throw new Error('返回的下游配置不匹配。');
    let cursor: string | undefined; let latest: Schema['ReleaseDecisionViewV1'] | undefined;
    const seen = new Set<string>();
    do {
      const page = dataOf(await api.GET('/api/v2/releases/{id}/decisions', { params: { path: { id: release.id }, query: { cursor, limit: 100 } }, signal }));
      for (const item of page.items) {
        if (item.project_id !== release.project_id || item.candidate_id !== release.candidate_id) throw new Error('返回的决定不属于原候选。');
        if (item.downstream_id === downstream && item.environment === environment && (!latest || item.ordinal > latest.ordinal)) latest = item;
      }
      cursor = page.next_cursor ?? undefined;
      if (cursor && seen.has(cursor)) throw new Error('决定分页未前进，请重新读取。');
      if (cursor) seen.add(cursor);
    } while (cursor);
    return { down, latest };
  } });
  const mutation = useMutation({ mutationFn: async (body: Schema['ReleaseApproveV1']) => {
    const result = dataOf(await api.POST('/api/v2/releases/{id}/approvals', { body, params: { path: { id: release.id }, header: intent.current.headers('POST', `/api/v2/releases/${release.id}/approvals`, body) } }));
    const a = result.resource;
    if (a.project_id !== release.project_id || a.release_id !== release.id || a.candidate_id !== release.candidate_id || a.downstream_id !== body.downstream_id || a.environment !== body.environment || a.downstream_revision !== body.expected_downstream_revision || a.authority_kind !== 'OPERATOR' || Date.parse(a.valid_until) !== Date.parse(body.valid_until)) throw new Error('审批回执与原请求不匹配。');
    return a;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['release-approvals', release.id] }); }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) { setSubmitted(undefined); void source.refetch(); }
  } });
  useGuard(!receipt);
  const timestamp = Date.parse(until);
  const ready = !!environment && !!source.data && !source.isFetching && !source.isError && source.data.down.configuration.enabled
    && (source.data.down.configuration.environments === 'BOTH' || source.data.down.configuration.environments === environment) && source.data.latest?.decision !== 'REJECT'
    && Number.isFinite(timestamp) && timestamp > Date.now() && timestamp <= Date.parse(release.valid_until);
  function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!ready || !source.data || !environment || !downstream) return;
    const body: Schema['ReleaseApproveV1'] = { schema_version: 1, downstream_id: downstream, environment, expected_downstream_revision: source.data.down.revision, expected_latest_decision_id: source.data.latest?.id ?? null, valid_until: new Date(timestamp).toISOString() };
    setSubmitted(body); mutation.mutate(body);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (submitted && !receipt) modal.confirm({ title: '关闭尚未确认的审批？', content: '关闭不会撤销可能已保存的审批。请先核对原审批历史。', okText: '关闭并核对', cancelText: '保留请求', onOk: close });
    else close();
  }
  return <Modal open title="审批原目标包" width={720} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss} onOk={submit}
    confirmLoading={mutation.isPending} okText={submitted ? '重试同一审批' : '确认审批'} cancelText="返回" okButtonProps={{ disabled: !online || (!submitted && !ready) }} footer={receipt ? <Button onClick={close}>返回原审批历史</Button> : undefined}>
    <Space orientation="vertical" className="full-width">
      <Alert type="info" showIcon title="审批不会发送 Offer 或代表下游领取" description="服务端重验原数据许可、资格、决定、下游新鲜探测与期限。Paper 审批不能用于 Live。" />
      <Typography.Text className="break-word">原 Release：{release.id}；最迟期限：{displayTime(release.valid_until)}</Typography.Text>
      <Form layout="vertical" disabled={!!submitted || !!receipt}>
        <Form.Item label="选择审批下游"><ResourceSelect label="选择审批下游" value={downstream} onChange={setDownstream} queryKey={['approval-downstreams']} load={async (cursor, signal) => {
          const page = dataOf(await api.GET('/api/v2/integrations/downstreams', { params: { query: { cursor, limit: 50 } }, signal }));
          return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled })) };
        }} /></Form.Item>
        <Form.Item label="审批环境"><Select aria-label="审批环境" value={environment} onChange={setEnvironment} options={[{ value: 'PAPER', label: 'Paper' }, { value: 'LIVE', label: 'Live' }]} /></Form.Item>
        <Form.Item label="审批截止时间（本地时间）"><Input aria-label="审批截止时间（本地时间）" type="datetime-local" value={until} onChange={e => setUntil(e.target.value)} /></Form.Item>
      </Form>
      <ErrorNotice error={source.error ?? mutation.error} />
      {source.data && <Typography.Text>原下游版本：{source.data.down.revision}；原决定：{source.data.latest ? `${source.data.latest.decision} / ${source.data.latest.id}` : '无历史决定'}</Typography.Text>}
      {source.data?.latest?.decision === 'REJECT' && <Alert type="warning" showIcon title="原候选已被人工拒绝，不能审批。" />}
      {submitted && mutation.isError && <Alert type="warning" showIcon title="结果尚未确认，重试保留原请求及幂等键。" />}
      {receipt && <Alert type="success" showIcon title="原审批已保存，尚未发送 Offer。" description={receipt.id} />}
    </Space>
  </Modal>;
}
