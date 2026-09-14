import { App, Alert, Button, Descriptions, Form, Input, InputNumber, Modal, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent, isCounter } from './api';
import type { Schema } from './api';
import { counterRules } from './budget-fields';
import { ResourceSelect } from './resource-select';
import { RunDetail } from './runs';
import { ErrorNotice, useGuard, useOnline } from './ui';

type Request = Schema['PortfolioStudyRequestV1'];
type Fields = Omit<Request['limits'], 'schema_version' | 'experiments'> & { cycle_id: string; runtime_id: string };

export function PortfolioStudy({ candidate, close }: { candidate: Schema['CandidateViewV1']; close: () => void }) {
  const [form] = Form.useForm<Fields>();
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const intent = useRef(new Intent()); const hadUnknown = useRef(false);
  const [submitted, setSubmitted] = useState<Request>();
  const [receipt, setReceipt] = useState<Schema['RunSnapshotV1']>();
  const [showRun, setShowRun] = useState(false);
  const cycleId: string | undefined = Form.useWatch('cycle_id', form);
  const runtimeId: string | undefined = Form.useWatch('runtime_id', form);
  const mandate = useQuery({ queryKey: ['mandate', candidate.mandate_id], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/portfolio-mandates/{id}', { params: { path: { id: candidate.mandate_id } }, signal }));
    if (value.id !== candidate.mandate_id || value.project_id !== candidate.project_id) throw new Error('Mandate 不属于原候选。');
    return value;
  } });
  const policyId = mandate.data?.content.required_evaluation_policy_id;
  const policy = useQuery({ queryKey: ['study-policy', policyId], enabled: !!policyId && !mandate.isError, queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/evaluation-policies/{id}', { params: { path: { id: policyId! } }, signal }));
    if (value.id !== policyId || value.project_id !== candidate.project_id) throw new Error('政策不属于原候选。');
    return value;
  } });
  const cycle = useQuery({ queryKey: ['cycle', cycleId], enabled: !!cycleId, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/cycles/{id}', { params: { path: { id: cycleId! } }, signal })) });
  const runtime = useQuery({ queryKey: ['study-runtime', runtimeId], enabled: !!runtimeId, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: runtimeId! } }, signal })) });
  const ready = !!policy.data?.portfolio_study_plan && !!policy.data.portfolio_metric_requirements
    && cycle.data?.id === cycleId && cycle.data?.project_id === candidate.project_id && cycle.data.state === 'RUNNING'
    && runtime.data?.id === runtimeId && runtime.data?.configuration.enabled && runtime.data.configuration.allowed_capabilities.includes('PORTFOLIO_SIMULATE')
    && [mandate, policy, cycle, runtime].every(query => !query.isError && !query.isFetching);
  const mutation = useMutation({ mutationFn: async (body: Request) => {
    const result = dataOf(await api.POST('/api/v2/portfolio-studies', { params: { header: intent.current.headers('POST', '/api/v2/portfolio-studies', body) }, body }));
    if (result.resource.project_id !== candidate.project_id || result.resource.cycle_id !== body.cycle_id || result.resource.kind !== 'PORTFOLIO_SIMULATE') throw new Error('返回的运行不属于原研究请求。');
    return result;
  }, onSuccess: async result => {
    setReceipt(result.resource); intent.current.clear();
    await Promise.all([client.invalidateQueries({ queryKey: ['runs'] }), client.invalidateQueries({ queryKey: ['cycles', candidate.project_id] })]);
  }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) hadUnknown.current = true;
    if (rejected && !hadUnknown.current) setSubmitted(undefined);
  } });
  useGuard(!receipt);
  const retry = submitted !== undefined && mutation.isError;
  function submit(value: Fields) {
    if (!online || mutation.isPending || submitted || !ready || !runtime.data) return;
    const body: Request = { schema_version: 1, candidate_id: candidate.id, cycle_id: value.cycle_id, runtime_id: value.runtime_id,
      expected_runtime_revision: runtime.data.revision, limits: { schema_version: 1, experiments: 0,
        cpu_seconds: value.cpu_seconds, wall_seconds: value.wall_seconds, memory_mib: value.memory_mib, output_bytes: value.output_bytes } };
    setSubmitted(body); mutation.mutate(body);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (retry) modal.confirm({ title: '关闭未确认的 Study 请求？', content: '关闭不撤销可能已登记的 Run。请核对运行记录，不要重复创建研究。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  if (showRun && receipt) return <RunDetail id={receipt.id} close={() => setShowRun(false)} />;
  const plan = policy.data?.portfolio_study_plan;
  return <Modal open title="确认请求组合 Study" width={760} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss}
    footer={receipt ? <Button onClick={close}>返回候选</Button> : undefined} cancelText="返回" okText={retry ? '重试同一请求' : '确认请求 Study'}
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || (!retry && !ready) }}
    onOk={() => { if (!online || mutation.isPending || receipt) return; if (retry && submitted) mutation.mutate(submitted); else form.submit(); }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Typography.Paragraph className="break-word">原候选：{candidate.id}</Typography.Paragraph>
      <Alert type="info" showIcon title="研究使用原完整成员和冻结政策，不使用历史候选权重作为持仓。" description="服务器重验原模型、费用、来源和预算。默认限额只是可修改草稿；请求不授予 PASS、审批或交付权限。" />
      {[mandate.error, policy.error, cycle.error, runtime.error, mutation.error].map((error, index) => <ErrorNotice key={index} error={error} />)}
      {policy.data && !plan && <Alert type="warning" showIcon title="原政策没有 Study 计划，不能启动研究。" />}
      {plan && <Descriptions column={1} size="small" className="break-word" items={[
        { key: 'policy', label: '原政策', children: policyId }, { key: 'input', label: '冻结 Study 输入', children: plan.input_set_id },
        { key: 'start', label: '研究起点', children: displayTime(plan.evaluation_start) },
        { key: 'end', label: '研究终点', children: '固定取原数据版本结束，不接受覆盖' },
        { key: 'cutoffs', label: '手动时点', children: plan.manual_cutoffs?.map(displayTime).join('；') ?? '按原 Mandate 调度' },
      ]} />}
      {retry && <Alert type="warning" showIcon title="请求结果尚未确认。" description="重试保留原内容、Runtime 修订与幂等键，不重新选择研究来源。" />}
      {receipt ? <><Alert type="success" showIcon title="Study Run 已登记。" description="202 不是科学通过、资格或交付批准。" />
        <Typography.Text className="break-word">Run {receipt.id} · {receipt.state}</Typography.Text><Button onClick={() => setShowRun(true)}>查看 Study 运行</Button></> :
      <Form form={form} layout="vertical" onFinish={submit} disabled={!online || mutation.isPending || submitted !== undefined}
        initialValues={{ cpu_seconds: '10', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' }}>
        <Form.Item name="cycle_id" label="承担研究预算的 Cycle" rules={[{ required: true }]}>
          <ResourceSelect label="选择 Study Cycle" queryKey={['study-cycles', candidate.project_id]} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: candidate.project_id }, query: { cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === candidate.project_id).map(item => ({ value: item.id, label: `Cycle ${item.ordinal} · ${item.state} · ${item.id}`, disabled: item.state !== 'RUNNING' })) };
          }} />
        </Form.Item>
        <Form.Item name="runtime_id" label="运行 Runtime" rules={[{ required: true }]}>
          <ResourceSelect label="选择 Study Runtime" queryKey={['study-runtimes']} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled || !item.configuration.allowed_capabilities.includes('PORTFOLIO_SIMULATE') })) };
          }} />
        </Form.Item>
        {runtime.data && <Typography.Paragraph className="break-word">Runtime 修订：{runtime.data.revision}（提交后冻结）</Typography.Paragraph>}
        <div className="field-grid">
          <Form.Item name="cpu_seconds" label="CPU 秒数上限" rules={counterRules}><Input inputMode="numeric" maxLength={19} /></Form.Item>
          <Form.Item name="wall_seconds" label="墙钟秒数上限" rules={[{ required: true, type: 'integer', min: 1, max: 86400 }]}><InputNumber min={1} max={86400} precision={0} /></Form.Item>
          <Form.Item name="memory_mib" label="内存上限（MiB）" rules={[{ required: true, type: 'integer', min: 1, max: 1048576 }]}><InputNumber min={1} max={1048576} precision={0} /></Form.Item>
          <Form.Item name="output_bytes" label="输出字节上限" rules={[{ required: true }, { validator: (_: unknown, value: unknown) => typeof value === 'string' && isCounter(value, true) && BigInt(value) <= 67108864n ? Promise.resolve() : Promise.reject(new Error('请输入 1 至 67108864 的整数字符串。')) }]}><Input inputMode="numeric" maxLength={19} /></Form.Item>
        </div>
      </Form>}
    </Space>
  </Modal>;
}
