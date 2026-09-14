import { App, Alert, Button, Card, Select, Form, Input, InputNumber, Modal, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent, isCounter, isDecimal } from './api';
import type { Schema } from './api';
import { counterRules } from './budget-fields';
import { ResourceSelect } from './resource-select';
import { RunDetail } from './runs';
import { ErrorNotice, useGuard, useOnline } from './ui';

type Request = Schema['PortfolioBuildRequestV1'];
type Fields = Omit<Request['limits'], 'schema_version' | 'experiments'> & { cycle_id: string; runtime_id: string; input_set_id: string; environment: Request['environment']; source_kind: 'FORWARD_SNAPSHOT' | 'LAST_TARGET'; source_id: string; members: { alpha_id: string; version_id: string; qualification_id: string; ensemble_weight: string }[] };

export function PortfolioBuild({ mandate, close }: { mandate: Schema['MandateViewV1']; close: () => void }) {
  const [form] = Form.useForm<Fields>();
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const intent = useRef(new Intent()); const hadUnknown = useRef(false);
  const [submitted, setSubmitted] = useState<Request>();
  const [receipt, setReceipt] = useState<Schema['RunSnapshotV1']>();
  const [showRun, setShowRun] = useState(false);
  const cycleId: string | undefined = Form.useWatch('cycle_id', form);
  const runtimeId: string | undefined = Form.useWatch('runtime_id', form);
  const sourceKind = Form.useWatch('source_kind', form);
  const environment = Form.useWatch('environment', form);
  const inputId = Form.useWatch('input_set_id', form);
  const input = useQuery({ queryKey: ['build-input', inputId], enabled: !!inputId, queryFn: async ({ signal }) =>
    dataOf(await api.GET('/api/v2/input-sets/{id}', { params: { path: { id: inputId! } }, signal })) });
  const cycle = useQuery({ queryKey: ['cycle', cycleId], enabled: !!cycleId, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/cycles/{id}', { params: { path: { id: cycleId! } }, signal })) });
  const runtime = useQuery({ queryKey: ['build-runtime', runtimeId], enabled: !!runtimeId, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: runtimeId! } }, signal })) });
  const ready = input.data?.header.id === inputId && input.data?.header.project_id === mandate.project_id && input.data.header.purpose === 'FORWARD'
    && cycle.data?.id === cycleId && cycle.data?.project_id === mandate.project_id && cycle.data.state === 'RUNNING'
    && runtime.data?.id === runtimeId && runtime.data?.configuration.enabled && runtime.data.configuration.allowed_capabilities.includes('PORTFOLIO_BUILD')
    && [input, cycle, runtime].every(query => !query.isError && !query.isFetching);
  const mutation = useMutation({ mutationFn: async (body: Request) => {
    const result = dataOf(await api.POST('/api/v2/portfolio-builds', { params: { header: intent.current.headers('POST', '/api/v2/portfolio-builds', body) }, body }));
    if (result.resource.project_id !== mandate.project_id || result.resource.cycle_id !== body.cycle_id || result.resource.kind !== 'PORTFOLIO_BUILD' || result.resource.input_set_id !== body.input_set_id) throw new Error('返回的运行不属于原研究请求。');
    return result;
  }, onSuccess: async result => {
    setReceipt(result.resource); intent.current.clear();
    await Promise.all([client.invalidateQueries({ queryKey: ['runs'] }), client.invalidateQueries({ queryKey: ['cycles', mandate.project_id] })]);
  }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) hadUnknown.current = true;
    if (rejected && !hadUnknown.current) setSubmitted(undefined);
  } });
  useGuard(!receipt);
  const retry = submitted !== undefined && mutation.isError;
  function submit(value: Fields) {
    if (!online || mutation.isPending || submitted || !ready || !runtime.data) return;
    const body: Request = { schema_version: 1, mandate_id: mandate.id, cycle_id: value.cycle_id, runtime_id: value.runtime_id, input_set_id: value.input_set_id, environment: value.environment,
      current_weights_source: value.source_kind === 'FORWARD_SNAPSHOT' ? { kind: value.source_kind, snapshot_id: value.source_id } : { kind: value.source_kind, candidate_id: value.source_id },
      members: value.members.map(({ qualification_id, ensemble_weight }) => ({ qualification_id, ensemble_weight })),
      expected_runtime_revision: runtime.data.revision, limits: { schema_version: 1, experiments: 0,
        cpu_seconds: value.cpu_seconds, wall_seconds: value.wall_seconds, memory_mib: value.memory_mib, output_bytes: value.output_bytes } };
    setSubmitted(body); mutation.mutate(body);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (retry) modal.confirm({ title: '关闭未确认的 Build 请求？', content: '关闭不撤销可能已登记的 Run。请核对运行记录，不要重复创建研究。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  if (showRun && receipt) return <RunDetail id={receipt.id} close={() => setShowRun(false)} />;
  return <Modal open title="确认请求组合 Build" width={760} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss}
    footer={receipt ? <Button onClick={close}>返回配置</Button> : undefined} cancelText="返回" okText={retry ? '重试同一请求' : '确认请求 Build'}
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || (!retry && !ready) }}
    onOk={() => { if (!online || mutation.isPending || receipt) return; if (retry && submitted) mutation.mutate(submitted); else form.submit(); }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Typography.Paragraph className="break-word">原组合配置：{mandate.id}</Typography.Paragraph>
      <Alert type="info" showIcon title="从原资格与冻结输入构建目标，不填写预测或资产持仓。" description="至少选择两个不同 Alpha，至少两个权重为正且合计为 1。资格窗口开放不代表当前可用，服务器重验政策、许可、生命周期和原证据。LAST_TARGET 仅为历史目标假设；202 不是候选、科学 PASS 或交付授权。" />
      {[input.error, cycle.error, runtime.error, mutation.error].map((error, index) => <ErrorNotice key={index} error={error} />)}
      {retry && <Alert type="warning" showIcon title="请求结果尚未确认。" description="重试保留原内容、Runtime 修订与幂等键，不重新选择研究来源。" />}
      {receipt ? <><Alert type="success" showIcon title="Build Run 已登记。" description="202 不是科学通过、资格或交付批准。" />
        <Typography.Text className="break-word">Run {receipt.id} · {receipt.state}</Typography.Text><Button onClick={() => setShowRun(true)}>查看 Build 运行</Button></> :
      <Form form={form} layout="vertical" onFinish={submit} disabled={!online || mutation.isPending || submitted !== undefined}
        initialValues={{ environment: 'PAPER', source_kind: 'FORWARD_SNAPSHOT', members: [{}, {}], cpu_seconds: '10', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' }}>
        <Form.Item name="cycle_id" label="承担研究预算的 Cycle" rules={[{ required: true }]}>
          <ResourceSelect label="选择 Build Cycle" queryKey={['build-cycles', mandate.project_id]} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: mandate.project_id }, query: { cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === mandate.project_id).map(item => ({ value: item.id, label: `Cycle ${item.ordinal} · ${item.state} · ${item.id}`, disabled: item.state !== 'RUNNING' })) };
          }} />
        </Form.Item>
        <Form.Item name="runtime_id" label="运行 Runtime" rules={[{ required: true }]}>
          <ResourceSelect label="选择 Build Runtime" queryKey={['build-runtimes']} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.configuration.name} · ${item.id}`, disabled: !item.configuration.enabled || !item.configuration.allowed_capabilities.includes('PORTFOLIO_BUILD') })) };
          }} />
        </Form.Item>
        <Form.Item name="input_set_id" label="冻结 Forward 输入" rules={[{ required: true }]}>
          <ResourceSelect label="选择构建输入" queryKey={['build-inputs', mandate.project_id]} load={async (cursor, signal) => {
            const page = dataOf(await api.GET('/api/v2/input-sets', { params: { query: { project_id: mandate.project_id, cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === mandate.project_id).map(item => ({ value: item.id, label: `${item.purpose} · ${displayTime(item.decision_cutoff)} · ${item.id}`, disabled: item.purpose !== 'FORWARD' })) };
          }} />
        </Form.Item>
        <Form.Item name="environment" label="构建环境" rules={[{ required: true }]}><Select onChange={() => form.setFieldValue('source_id', undefined)} options={[{ value: 'PAPER', label: 'PAPER' }, { value: 'LIVE', label: 'LIVE' }]} /></Form.Item>
        <Form.Item name="source_kind" label="当前权重来源" rules={[{ required: true }]}><Select onChange={() => form.setFieldValue('source_id', undefined)} options={[{ value: 'FORWARD_SNAPSHOT', label: '下游原权重快照' }, { value: 'LAST_TARGET', label: '历史目标假设（不是实际持仓）' }]} /></Form.Item>
        <Form.Item name="source_id" label="原权重来源记录" rules={[{ required: true }]}>
          <ResourceSelect key={`${sourceKind}/${environment}`} label="选择原权重记录" queryKey={['build-weights', mandate.project_id, sourceKind, environment]} load={async (cursor, signal) => {
            if (sourceKind === 'LAST_TARGET') {
              const page = dataOf(await api.GET('/api/v2/projects/{id}/portfolio-candidates', { params: { path: { id: mandate.project_id }, query: { cursor, limit: 50 } }, signal }));
              return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === mandate.project_id).map(item => ({ value: item.id, label: `历史目标假设 · ${displayTime(item.decision_asof)} · ${item.id}`, disabled: item.execution_status !== 'SUCCEEDED' || item.evidence_status !== 'VALID' || !item.target_artifact_id || item.origin === 'FIXTURE' })) };
            }
            const page = dataOf(await api.GET('/api/v2/projects/{id}/forward-weight-snapshots', { params: { path: { id: mandate.project_id }, query: { cursor, limit: 50 } }, signal }));
            return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === mandate.project_id).map(item => ({ value: item.id, label: `${item.environment} · 下游 ${item.downstream_id} · ${displayTime(item.received_at)} · ${item.id}`, disabled: item.environment !== environment || item.content.base_currency !== mandate.content.base_currency })) };
          }} />
        </Form.Item>
        <Form.List name="members" rules={[{ validator: async (_, members: Fields['members']) => {
          if (!members || members.length < 2 || members.length > 256 || new Set(members.map(member => member.alpha_id)).size !== members.length) throw new Error('需要 2 至 256 个不同 Alpha 的原资格。');
        } }]}>{(fields, { add, remove }, { errors }) => <>
          {fields.map(field => <Card key={field.key} size="small" title={`组合成员 ${field.name + 1}`}>
            <BuildMember index={field.name} project={mandate.project_id} policy={mandate.content.required_evaluation_policy_id} />
            <Button onClick={() => remove(field.name)}>删除成员 {field.name + 1}</Button>
          </Card>)}
          <Form.ErrorList errors={errors} /><Button disabled={fields.length >= 256 || !online || submitted !== undefined || mutation.isPending} onClick={() => add({})}>添加组合成员</Button>
        </>}</Form.List>
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

function BuildMember({ index, project, policy }: { index: number; project: string; policy: string }) {
  const form = Form.useFormInstance<Fields>();
  const alpha = Form.useWatch(['members', index, 'alpha_id'], form);
  const version = Form.useWatch(['members', index, 'version_id'], form);
  return <>
    <Form.Item name={[index, 'alpha_id']} label="原 Alpha" rules={[{ required: true }]}>
      <ResourceSelect label={`选择成员 ${index + 1} Alpha`} queryKey={['build-alphas', project]} onChange={() => { form.setFieldValue(['members', index, 'version_id'], undefined); form.setFieldValue(['members', index, 'qualification_id'], undefined); }} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/alphas', { params: { query: { project_id: project, cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === project).map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
      }} />
    </Form.Item>
    <Form.Item name={[index, 'version_id']} label="原 Alpha 版本" rules={[{ required: true }]}>
      <ResourceSelect label={`选择成员 ${index + 1} 版本`} disabled={!alpha} queryKey={['build-versions', project, alpha]} onChange={() => form.setFieldValue(['members', index, 'qualification_id'], undefined)} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/alphas/{id}/versions', { params: { path: { id: alpha! }, query: { cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === project && item.alpha_id === alpha).map(item => ({ value: item.id, label: `v${item.version} · ${item.id}` })) };
      }} />
    </Form.Item>
    <Form.Item name={[index, 'qualification_id']} label="原资格（服务器重验）" rules={[{ required: true }]}>
      <ResourceSelect label={`选择成员 ${index + 1} 资格`} disabled={!version} queryKey={['build-qualifications', version, policy]} load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/alpha-versions/{id}/qualifications', { params: { path: { id: version! }, query: { cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.filter(item => item.alpha_version_id === version).map(item => ({ value: item.id, label: `${item.id} · 窗口${item.grant_window_open ? '开放' : '关闭'} · ${displayTime(item.valid_until)}`, disabled: item.policy_id !== policy || !item.grant_window_open })) };
      }} />
    </Form.Item>
    <Form.Item name={[index, 'ensemble_weight']} label="预测聚合权重" rules={[{ required: true }, { validator: async (_, value: unknown) => { if (!isDecimal(value) || String(value).startsWith('-')) throw new Error('请输入非负十进制字符串。'); } }]}><Input inputMode="decimal" /></Form.Item>
  </>;
}
