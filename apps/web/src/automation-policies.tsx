import { Alert, App, Button, Checkbox, Drawer, Form, Input, InputNumber, Select, Space, Table } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { Requirements } from './evaluation-policies';
import { PolicyRevoke } from './approval-revoke';
import { counterRules } from './budget-fields';
import { ErrorNotice, Pager, QueryPanel, useGuard, useOnline } from './ui';

type Content = Schema['AutomationPolicyContentV1'];
export function AutomationPolicies({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [creating, setCreating] = useState(false);
  const [revoking, setRevoking] = useState<Schema['AutomationPolicyViewV1']>();
  const query = useQuery({ queryKey: ['automation-policies', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/automation-policies', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('自动化政策不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Alert showIcon type="info" title="自动化政策是有界授权，不是交付结果" description="新版本不改写旧审批或已领取事实。Auto HandOff 晋级仍要求完整 Paper 观察与原生资格；Agent 不能授权。" />
    <Button onClick={() => setCreating(true)}>冻结自动化政策</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['AutomationPolicyViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }} onHeaderRow={() => ({ tabIndex: 0 })} columns={[
        { title: '原政策', dataIndex: 'id' }, { title: '管理', key: 'revoke', render: (_, item) => <Button danger disabled={query.isError || query.isFetching} onClick={() => setRevoking(item)}>撤销政策</Button> }, { title: '模式', key: 'mode', render: (_, item) => item.content.mode },
        { title: '原期限', key: 'until', render: (_, item) => displayTime(item.content.valid_until) }, { title: '授权于', dataIndex: 'authorized_at', render: displayTime },
      ]} expandable={{ expandedRowRender: item => <pre tabIndex={0} aria-label="原自动化政策" className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(item, null, 2)}</pre> }} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {revoking && <PolicyRevoke policy={revoking} close={() => setRevoking(undefined)} />}
    {creating && <PolicyEditor project={project} close={() => setCreating(false)} />}
  </Space>;
}

function PolicyEditor({ project, close }: { project: string; close: () => void }) {
  const [form] = Form.useForm<Content>(); const [submitted, setSubmitted] = useState<Schema['AutomationAuthorizeV1']>();
  const [receipt, setReceipt] = useState<Schema['AutomationPolicyViewV1']>(); const [error, setError] = useState<unknown>();
  const intent = useRef(new Intent()); const unknown = useRef(false); const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const current = useQuery({ queryKey: ['automation-project', project], enabled: !submitted && !receipt, queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/projects/{id}', { params: { path: { id: project } }, signal }));
    if (value.id !== project) throw new Error('项目版本不匹配。'); return value;
  } });
  const mutation = useMutation({ mutationFn: async (body: Schema['AutomationAuthorizeV1']) => {
    const result = dataOf(await api.POST('/api/v2/projects/{id}/automation-policies', { body, params: { path: { id: project }, header: intent.current.headers('POST', `/api/v2/projects/${project}/automation-policies`, body) } }));
    if (result.resource.project_id !== project) throw new Error('政策回执不属于原项目。'); return result.resource;
  }, onSuccess: async result => { setReceipt(result); intent.current.clear(); await client.invalidateQueries({ queryKey: ['automation-policies', project] }); await client.invalidateQueries({ queryKey: ['projects'] }); }, onError: failure => {
    const rejected = failure instanceof ApiFailure && ((!!failure.problem && failure.status >= 400 && failure.status < 500) || failure.code === 'OFFLINE');
    if (!rejected) unknown.current = true;
    if (rejected && !unknown.current) { setSubmitted(undefined); void current.refetch(); }
  } });
  useGuard(!receipt);
  async function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!current.data || current.isFetching || current.isError || current.data.state === 'ARCHIVED') return;
    try {
      const values = await form.validateFields();
      const metrics = (items: Schema['MetricRequirementV1'][] | undefined) => {
        if (!items?.length || !items.some(item => item.required)) throw new Error('每组必须至少包含一项必需指标。');
        return items.map(item => ({ ...item, schema_version: 1 as const, required: item.required === true, threshold_low: item.threshold_low || null, threshold_high: item.threshold_high || null }));
      };
      const content: Content = { ...values, enabled_for_new_rebalances: values.enabled_for_new_rebalances === true, valid_until: new Date(values.valid_until).toISOString(), promotion_metric_requirements: metrics(values.promotion_metric_requirements), degradation_metric_requirements: metrics(values.degradation_metric_requirements) };
      const body: Schema['AutomationAuthorizeV1'] = { schema_version: 1, expected_project_revision: current.data.revision, content };
      setError(undefined); setSubmitted(body); mutation.mutate(body);
    } catch (failure) { setError(failure instanceof Error ? failure : new Error('请检查表单中的必填项。')); }
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (!receipt) modal.confirm({ title: '关闭未完成的政策？', content: submitted ? '关闭不会撤销可能已保存的授权。请先核对原政策历史。' : '关闭将放弃当前表单。', okText: '关闭', cancelText: '保留表单', onOk: close }); else close();
  }
  return <Drawer title="冻结自动化政策" open width={850} maskClosable={false} closable={!mutation.isPending} onClose={dismiss}>
    <Alert showIcon type="warning" title="冻结政策授权后续有界自动化" description="明确设置全部限制，保存不代表 Worker 已运行或 Paper/Live 已交付。归档项目不能新增授权。" />
    <ErrorNotice error={current.error ?? error ?? mutation.error} />
    {submitted && mutation.isError && <Alert showIcon type="warning" title="结果尚未确认，重试保留原项目版本和完整政策。" />}
    {receipt ? <><Alert showIcon type="success" title="原自动化政策已冻结。" description={receipt.id} /><Button onClick={close}>返回政策历史</Button></> : <Form form={form} layout="vertical" disabled={!!submitted || !online} initialValues={{ enabled_for_new_rebalances: false, promotion_metric_requirements: [{ required: true }], degradation_metric_requirements: [{ required: true }] }}>
      <Form.Item name="mode" label="自动化模式" rules={[{ required: true }]}><Select options={['MANUAL', 'AUTO_PAPER', 'AUTO_HANDOFF'].map(value => ({ value, label: value }))} /></Form.Item>
      <Form.Item name="mandate_id" label="原组合配置" rules={[{ required: true }]}><ResourceSelect label="原组合配置" queryKey={['automation-mandates', project]} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects/{id}/portfolio-mandates', { params: { path: { id: project }, query: { cursor, limit: 50 } }, signal }));
        if (page.items.some(item => item.project_id !== project)) throw new Error('组合配置不属于原项目。');
        return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: item.id })) };
      }} /></Form.Item>
      <Form.Item name="downstream_id" label="原下游" rules={[{ required: true }]}><ResourceSelect label="原下游" queryKey={['automation-downstreams']} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/integrations/downstreams', { params: { query: { cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled })) };
      }} /></Form.Item>
      {([['required_paper_observations', '每流最少 Paper 观察数'], ['max_rebalances_per_day', '每日最多候选再平衡数']] as const).map(([name, label]) => <Form.Item key={name} name={name} label={label} rules={[{ required: true, type: 'integer', min: 1, max: 2147483647 }]}><InputNumber precision={0} min={1} max={2147483647} /></Form.Item>)}
      {([['minimum_paper_elapsed_seconds', 'Paper 最少经过秒数'], ['max_feedback_age_seconds', '反馈最大年龄秒数']] as const).map(([name, label]) => <Form.Item key={name} name={name} label={label} rules={counterRules}><Input inputMode="numeric" /></Form.Item>)}
      <Requirements name="promotion_metric_requirements" title="晋级指标" /><Requirements name="degradation_metric_requirements" title="维持指标" />
      <Form.Item name="valid_until" label="授权截止时间（本地时间）" rules={[{ required: true }, { validator: async (_, value) => { if (!Number.isFinite(Date.parse(value)) || Date.parse(value) <= Date.now()) throw new Error('请选择未来时间。'); } }]}><Input type="datetime-local" /></Form.Item>
      <Form.Item name="enabled_for_new_rebalances" valuePropName="checked"><Checkbox>允许新的自动再平衡</Checkbox></Form.Item>
    </Form>}
    {!receipt && <Button type="primary" loading={mutation.isPending} disabled={!online || (!submitted && (!current.data || current.isFetching || current.isError || current.data.state === 'ARCHIVED'))} onClick={() => { void submit(); }}>{submitted ? '重试同一政策' : '确认冻结政策'}</Button>}
  </Drawer>;
}
