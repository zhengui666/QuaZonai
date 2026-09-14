import { Alert, App, Button, Form, Input, Modal, Select, Space, Table } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, QueryPanel, useGuard, useOnline } from './ui';

export function ApprovalRevoke({ approval, close }: { approval: Schema['ApprovalViewV1']; close: () => void }) {
  const [scheduled, setScheduled] = useState(false); const [at, setAt] = useState('');
  const [code, setCode] = useState(''); const [reason, setReason] = useState('');
  const [submitted, setSubmitted] = useState<Schema['ApprovalRevokeV1']>();
  const [receipt, setReceipt] = useState<Schema['ApprovalRevocationViewV1']>();
  const intent = useRef(new Intent()); const unknown = useRef(false);
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const history = useQuery({ queryKey: ['approval-revocations', approval.id], enabled: !submitted && !receipt, queryFn: async ({ signal }) => {
    const items: Schema['ApprovalRevocationViewV1'][] = []; let cursor: string | undefined; const seen = new Set<string>();
    do {
      const page = dataOf(await api.GET('/api/v2/approvals/{id}/revocations', { params: { path: { id: approval.id }, query: { cursor, limit: 100 } }, signal }));
      if (page.items.some(item => item.approval_id !== approval.id)) throw new Error('撤销记录不属于原审批。');
      items.push(...page.items); cursor = page.next_cursor ?? undefined;
      if (cursor && seen.has(cursor)) throw new Error('撤销分页未前进。');
      if (cursor) seen.add(cursor);
    } while (cursor);
    return items;
  } });
  const latest = history.data?.reduce<Schema['ApprovalRevocationViewV1'] | undefined>((a, b) => !a || b.id > a.id ? b : a, undefined);
  const earliest = history.data?.reduce<number>((a, b) => Math.min(a, Date.parse(b.effective_at)), Infinity);
  const mutation = useMutation({ mutationFn: async (body: Schema['ApprovalRevokeV1']) => {
    const result = dataOf(await api.POST('/api/v2/approvals/{id}/revoke', { body, params: { path: { id: approval.id }, header: intent.current.headers('POST', `/api/v2/approvals/${approval.id}/revoke`, body) } }));
    const r = result.resource;
    if (r.approval_id !== approval.id || r.reason_code !== body.reason_code || r.reason !== body.reason || (body.effective_at != null && Date.parse(r.effective_at) !== Date.parse(body.effective_at))) throw new Error('撤销回执与原请求不匹配。');
    return r;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['handoffs', approval.project_id] }); await client.invalidateQueries({ queryKey: ['approval-revocations', approval.id] }); }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) { setSubmitted(undefined); void history.refetch(); }
  } });
  useGuard(!receipt);
  const time = Date.parse(at);
  const ready = !!history.data && !history.isError && !history.isFetching && code.trim().length > 0 && [...code].length <= 120 && reason.trim().length > 0 && [...reason].length <= 2000
    && (!scheduled || (Number.isFinite(time) && time > Date.now() && time <= (earliest ?? Infinity)));
  function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!ready) return;
    const body: Schema['ApprovalRevokeV1'] = { schema_version: 1, expected_latest_revocation_id: latest?.id ?? null, effective_at: scheduled ? new Date(time).toISOString() : null, reason_code: code, reason };
    setSubmitted(body); mutation.mutate(body);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (submitted && !receipt) modal.confirm({ title: '关闭尚未确认的撤销？', content: '关闭不会取消可能已保存的撤销，请先核对原记录。', okText: '关闭并核对', cancelText: '保留请求', onOk: close }); else close();
  }
  return <Modal open title="撤销原审批" width={760} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss} onOk={submit} confirmLoading={mutation.isPending}
    okText={submitted ? '重试同一撤销' : '确认追加撤销'} cancelText="返回" okButtonProps={{ danger: true, disabled: !online || (!submitted && !ready) }} footer={receipt ? <Button onClick={close}>返回审批历史</Button> : undefined}>
    <Space orientation="vertical" className="full-width">
      <Alert showIcon type="warning" title="撤销限制未来交付，不撤单或平仓" description="立即或预约撤销均追加原记录；最早生效时间不能推后，已领取和 ACK 事实保留。" />
      <QueryPanel pending={history.isPending} error={history.error} stale={!!history.data} reload={() => { void history.refetch(); }}>
        <Table<Schema['ApprovalRevocationViewV1']> rowKey="id" dataSource={history.data} size="small" pagination={{ pageSize: 5 }} scroll={{ x: 600 }} onHeaderRow={() => ({ tabIndex: 0 })} columns={[
          { title: '原撤销', dataIndex: 'id' }, { title: '生效于', dataIndex: 'effective_at', render: displayTime }, { title: '原因', dataIndex: 'reason' },
        ]} />
      </QueryPanel>
      <Form layout="vertical" disabled={!!submitted || !!receipt}>
        <Form.Item label="生效方式"><Select aria-label="生效方式" value={scheduled ? 'scheduled' : 'now'} onChange={value => setScheduled(value === 'scheduled')} options={[{ value: 'now', label: '立即生效' }, { value: 'scheduled', label: '指定时间' }]} /></Form.Item>
        {scheduled && <Form.Item label="生效时间（本地时间）"><Input aria-label="生效时间（本地时间）" type="datetime-local" value={at} onChange={e => setAt(e.target.value)} /></Form.Item>}
        <Form.Item label="原因代码"><Input aria-label="原因代码" value={code} onChange={e => setCode(e.target.value)} /></Form.Item>
        <Form.Item label="撤销原因"><Input.TextArea aria-label="撤销原因" value={reason} onChange={e => setReason(e.target.value)} /></Form.Item>
      </Form>
      <ErrorNotice error={mutation.error} />
      {submitted && mutation.isError && <Alert showIcon type="warning" title="结果尚未确认，重试保留原撤销请求和幂等键。" />}
      {receipt && <Alert showIcon type="success" title="原撤销已追加。" description={`${receipt.id}；生效于 ${displayTime(receipt.effective_at)}，已领取事实保留。`} />}
    </Space>
  </Modal>;
}
