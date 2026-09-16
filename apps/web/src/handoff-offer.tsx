import { Alert, App, Button, Input, Modal, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, sameInstant, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, useGuard, useOnline } from './ui';

export function HandoffOffer({ release, approval, close }: { release: Schema['ReleaseViewV1']; approval: Schema['ApprovalViewV1']; close: () => void }) {
  const [until, setUntil] = useState(''); const [submitted, setSubmitted] = useState<Schema['HandoffOfferV1']>();
  const [receipt, setReceipt] = useState<Schema['HandoffViewV1']>();
  const intent = useRef(new Intent()); const unknown = useRef(false);
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const source = useQuery({ queryKey: ['offer-source', release.id, approval.id], enabled: !submitted && !receipt, queryFn: async ({ signal }) => {
    const current = dataOf(await api.GET('/api/v2/approvals/{id}', { params: { path: { id: approval.id } }, signal }));
    if (current.id !== approval.id || current.release_id !== release.id || current.project_id !== release.project_id || current.candidate_id !== release.candidate_id || current.downstream_id !== approval.downstream_id || current.environment !== approval.environment) throw new Error('原审批关联不匹配。');
    let cursor: string | undefined; let latest: Schema['HandoffViewV1'] | undefined; let duplicate = false;
    const seen = new Set<string>();
    do {
      const page = dataOf(await api.GET('/api/v2/projects/{id}/handoffs', { params: { path: { id: release.project_id }, query: { cursor, limit: 100 } }, signal }));
      for (const item of page.items) {
        if (item.project_id !== release.project_id) throw new Error('交付历史不属于原项目。');
        if (item.downstream_id !== current.downstream_id || item.environment !== current.environment) continue;
        if (item.release_id === release.id || (item.candidate_id === release.candidate_id && item.claimed_at !== null)) duplicate = true;
        if (item.mandate_id === release.mandate_id && (!latest || BigInt(item.delivery_sequence) > BigInt(latest.delivery_sequence))) latest = item;
      }
      cursor = page.next_cursor ?? undefined;
      if (cursor && seen.has(cursor)) throw new Error('交付分页未前进，请重新读取。');
      if (cursor) seen.add(cursor);
    } while (cursor);
    return { current, latest, duplicate };
  } });
  const mutation = useMutation({ mutationFn: async (body: Schema['HandoffOfferV1']) => {
    const result = dataOf(await api.POST('/api/v2/handoffs', { body, params: { header: intent.current.headers('POST', '/api/v2/handoffs', body) } }));
    const h = result.resource;
    if (h.project_id !== release.project_id || h.candidate_id !== release.candidate_id || h.mandate_id !== release.mandate_id || h.release_id !== body.release_id || h.approval_id !== body.approval_id || h.downstream_id !== approval.downstream_id || h.environment !== approval.environment || h.supersedes_handoff_id !== body.supersedes_handoff_id || !sameInstant(h.expires_at, body.expires_at)) throw new Error('Offer 回执与原请求不匹配。');
    return h;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['handoffs', release.project_id] }); }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) { setSubmitted(undefined); void source.refetch(); }
  } });
  useGuard(!receipt);
  const timestamp = Date.parse(until); const current = source.data?.current;
  const ready = !!current && current.authority_kind === 'OPERATOR' && current.downstream_revision !== null && current.decision_ordinal !== null && current.readiness_observation_id !== null && !source.data?.duplicate && !source.isFetching && !source.isError
    && Number.isFinite(timestamp) && timestamp > Date.now() && timestamp <= Math.min(Date.parse(release.valid_until), Date.parse(current.valid_until));
  function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!ready) return;
    const body: Schema['HandoffOfferV1'] = { schema_version: 1, release_id: release.id, approval_id: approval.id, supersedes_handoff_id: source.data?.latest?.id ?? null, expires_at: new Date(timestamp).toISOString() };
    setSubmitted(body); mutation.mutate(body);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (submitted && !receipt) modal.confirm({ title: '关闭尚未确认的 Offer？', content: '关闭不会撤销可能已登记的交付。请先核对交付历史。', okText: '关闭并核对', cancelText: '保留请求', onOk: close });
    else close();
  }
  return <Modal open title="登记原目标 Offer" width={720} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss} onOk={submit} confirmLoading={mutation.isPending}
    okText={submitted ? '重试同一 Offer' : '确认登记 Offer'} cancelText="返回" okButtonProps={{ disabled: !online || (!submitted && !ready) }} footer={receipt ? <Button onClick={close}>返回原审批</Button> : undefined}>
    <Space orientation="vertical" className="full-width">
      <Alert showIcon type="info" title="Offer 不代表下游已领取或真实成交" description="服务端重验原审批、来源、撤销、决定与新鲜探测；前版已领取的事实不会被改写。" />
      <Typography.Text className="break-word">原审批：{approval.id}；下游：{approval.downstream_id}；环境：{approval.environment}</Typography.Text>
      {current && <Typography.Text>原审批期限：{displayTime(current.valid_until)}；前版：{source.data?.latest?.id ?? '无前版交付'}</Typography.Text>}
      <Typography.Text>Offer 截止时间（本地时间）</Typography.Text>
      <Input aria-label="Offer 截止时间（本地时间）" type="datetime-local" value={until} onChange={e => setUntil(e.target.value)} disabled={!!submitted || !!receipt} />
      <ErrorNotice error={source.error ?? mutation.error} />
      {source.data?.duplicate && <Alert showIcon type="warning" title="原版本已登记 Offer 或原候选已领取，不能重复交付。" />}
      {submitted && mutation.isError && <Alert showIcon type="warning" title="结果尚未确认，重试保留原审批、前版、期限和幂等键。" />}
      {receipt && <Alert showIcon type="success" title="原 Offer 已登记。" description={`${receipt.id}；原回执状态：${receipt.state}。下游实际状态请查看交付记录。`} />}
    </Space>
  </Modal>;
}
