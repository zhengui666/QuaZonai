import { Alert, App, Button, Form, Input, Modal, Select, Space, Table } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { ErrorNotice, QueryPanel, useGuard, useOnline } from './ui';

type DecisionIntent = { kind: 'REJECT'; body: Schema['ReleaseRejectV1'] } | { kind: 'REOPEN'; body: Schema['ReleaseReopenV1'] };
export function ReleaseDecision({ release, close }: { release: Schema['ReleaseViewV1']; close: () => void }) {
  const [downstream, setDownstream] = useState<string>(); const [environment, setEnvironment] = useState<'PAPER' | 'LIVE'>();
  const [kind, setKind] = useState<'REJECT' | 'REOPEN'>('REJECT');
  const [code, setCode] = useState(''); const [reason, setReason] = useState('');
  const [submitted, setSubmitted] = useState<DecisionIntent>(); const [receipt, setReceipt] = useState<Schema['ReleaseDecisionViewV1']>();
  const intent = useRef(new Intent()); const unknown = useRef(false); const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const history = useQuery({ queryKey: ['release-decisions', release.id], enabled: !submitted && !receipt, queryFn: async ({ signal }) => {
    const items: Schema['ReleaseDecisionViewV1'][] = []; let cursor: string | undefined; const seen = new Set<string>();
    do {
      const page = dataOf(await api.GET('/api/v2/releases/{id}/decisions', { params: { path: { id: release.id }, query: { cursor, limit: 100 } }, signal }));
      if (page.items.some(item => item.project_id !== release.project_id || item.candidate_id !== release.candidate_id)) throw new Error('决定记录不属于原候选。');
      items.push(...page.items); cursor = page.next_cursor ?? undefined;
      if (cursor && seen.has(cursor)) throw new Error('决定分页未前进。');
      if (cursor) seen.add(cursor);
    } while (cursor);
    return items;
  } });
  const latest = history.data?.filter(item => item.downstream_id === downstream && item.environment === environment).reduce<Schema['ReleaseDecisionViewV1'] | undefined>((a, b) => !a || b.ordinal > a.ordinal ? b : a, undefined);
  const mutation = useMutation({ mutationFn: async (request: DecisionIntent) => {
    const result = request.kind === 'REJECT'
      ? dataOf(await api.POST('/api/v2/releases/{id}/rejections', { body: request.body, params: { path: { id: release.id }, header: intent.current.headers('POST', `/api/v2/releases/${release.id}/rejections`, request.body) } }))
      : dataOf(await api.POST('/api/v2/release-decisions/{id}/reopen', { body: request.body, params: { path: { id: request.body.expected_latest_decision_id }, header: intent.current.headers('POST', `/api/v2/release-decisions/${request.body.expected_latest_decision_id}/reopen`, request.body) } }));
    const r = result.resource;
    if (r.project_id !== release.project_id || r.candidate_id !== release.candidate_id || r.downstream_id !== downstream || r.environment !== environment || r.decision !== request.kind || r.supersedes_decision_id !== request.body.expected_latest_decision_id || r.reason_code !== request.body.reason_code || r.reason !== request.body.reason) throw new Error('决定回执与原请求不匹配。');
    return r;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['release-decisions', release.id] }); await client.invalidateQueries({ queryKey: ['approval-source', release.id] }); }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) { setSubmitted(undefined); void history.refetch(); }
  } });
  useGuard(!receipt);
  const ready = !!downstream && !!environment && !!history.data && !history.isFetching && !history.isError && code.trim().length > 0 && [...code].length <= 120 && reason.trim().length > 0 && [...reason].length <= 2000 && (kind === 'REJECT' || latest?.decision === 'REJECT');
  function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!ready || !downstream || !environment) return;
    const request: DecisionIntent = kind === 'REJECT'
      ? { kind, body: { schema_version: 1, downstream_id: downstream, environment, expected_latest_decision_id: latest?.id ?? null, reason_code: code, reason } }
      : { kind, body: { schema_version: 1, expected_latest_decision_id: latest!.id, reason_code: code, reason } };
    setSubmitted(request); mutation.mutate(request);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (submitted && !receipt) modal.confirm({ title: '关闭尚未确认的决定？', content: '关闭不会撤销可能已保存的决定，请先核对历史。', okText: '关闭并核对', cancelText: '保留请求', onOk: close }); else close();
  }
  return <Modal open title="人工交付决定" width={760} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss} onOk={submit} confirmLoading={mutation.isPending}
    okText={submitted ? '重试同一决定' : kind === 'REJECT' ? '确认拒绝' : '确认重新考虑'} cancelText="返回" okButtonProps={{ disabled: !online || (!submitted && !ready) }} footer={receipt ? <Button onClick={close}>返回原目标包</Button> : undefined}>
    <Space orientation="vertical" className="full-width">
      <Alert showIcon type="info" title="重新考虑不恢复旧审批" description="决定绑定原候选、下游和环境，跨 Release 保留历史；人工拒绝限制未来交付，不撤单或平仓。" />
      <QueryPanel pending={history.isPending} error={history.error} stale={!!history.data} reload={() => { void history.refetch(); }}>
        <Table<Schema['ReleaseDecisionViewV1']> rowKey="id" dataSource={history.data} size="small" pagination={{ pageSize: 5 }} scroll={{ x: 700 }} onHeaderRow={() => ({ tabIndex: 0 })} columns={[
          { title: '原决定', dataIndex: 'id' }, { title: '下游', dataIndex: 'downstream_id' }, { title: '环境', dataIndex: 'environment' }, { title: '决定', dataIndex: 'decision' }, { title: '序号', dataIndex: 'ordinal' }, { title: '时间', dataIndex: 'decided_at', render: displayTime },
        ]} />
      </QueryPanel>
      <Form layout="vertical" disabled={!!submitted || !!receipt}>
        <Form.Item label="决定下游"><ResourceSelect label="决定下游" value={downstream} onChange={setDownstream} queryKey={['decision-downstreams']} load={async (cursor, signal) => {
          const page = dataOf(await api.GET('/api/v2/integrations/downstreams', { params: { query: { cursor, limit: 50 } }, signal }));
          return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}` })) };
        }} /></Form.Item>
        <Form.Item label="决定环境"><Select aria-label="决定环境" value={environment} onChange={setEnvironment} options={[{ value: 'PAPER', label: 'Paper' }, { value: 'LIVE', label: 'Live' }]} /></Form.Item>
        <Form.Item label="决定动作"><Select aria-label="决定动作" value={kind} onChange={setKind} options={[{ value: 'REJECT', label: '拒绝' }, { value: 'REOPEN', label: '重新考虑' }]} /></Form.Item>
        <Form.Item label="原因代码"><Input aria-label="原因代码" value={code} onChange={e => setCode(e.target.value)} /></Form.Item>
        <Form.Item label="决定原因"><Input.TextArea aria-label="决定原因" value={reason} onChange={e => setReason(e.target.value)} /></Form.Item>
      </Form>
      {kind === 'REOPEN' && latest?.decision !== 'REJECT' && <Alert showIcon type="warning" title="只能重新考虑当前最新的人工拒绝。" />}
      <ErrorNotice error={mutation.error} />
      {submitted && mutation.isError && <Alert showIcon type="warning" title="结果尚未确认，重试保持原决定意图和幂等键。" />}
      {receipt && <Alert showIcon type="success" title="原决定已追加。" description={`${receipt.id} / ${receipt.decision}，不授予审批或交付资格。`} />}
    </Space>
  </Modal>;
}
